//! Gamescope command building and execution.
//!
//! Constructs the gamescope command line from a resolved profile,
//! including all options, HDR flags, and environment variables.
//! Uses `exec` to replace the current process with gamescope.

use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{Context, Result};

use crate::config::OptionValue;
use crate::profile::ResolvedProfile;

/// A fully constructed command ready for execution.
#[derive(Debug)]
pub struct GamescopeCommand {
    pub binary: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub child: Vec<String>,
    pub needs_workaround: bool,
}

impl GamescopeCommand {
    /// Format the command as a displayable string.
    pub fn display(&self) -> String {
        let capacity = self.binary.len()
            + self.args.iter().map(|s| s.len() + 1).sum::<usize>()
            + self.child.iter().map(|s| s.len() + 1).sum::<usize>()
            + 32; // separator and workaround

        let mut out = String::with_capacity(capacity);
        out.push_str(&self.binary);

        for arg in &self.args {
            out.push(' ');
            out.push_str(arg);
        }

        out.push_str(" --");

        if self.needs_workaround {
            out.push_str(" env DISABLE_HDR_WSI=1");
        }

        for arg in &self.child {
            out.push(' ');
            out.push_str(arg);
        }

        out
    }
}

/// Build a gamescope command from a resolved profile.
pub fn build(profile: &ResolvedProfile, child_cmd: &[String]) -> GamescopeCommand {
    let mut args = build_args(profile);

    if profile.use_hdr {
        args.push("--hdr-enabled".to_string());
        args.push("--hdr-debug-force-output".to_string());
        args.push("--hdr-debug-force-support".to_string());
    }

    GamescopeCommand {
        binary: profile.binary.clone(),
        args,
        env: profile.environment(),
        child: child_cmd.to_vec(),
        needs_workaround: profile.needs_hdr_workaround(),
    }
}

/// Build gamescope CLI arguments from profile options.
fn build_args(profile: &ResolvedProfile) -> Vec<String> {
    let mut args = Vec::with_capacity(profile.options.len() * 2);

    let mut sorted_opts: Vec<_> = profile.options.iter().collect();
    sorted_opts.sort_by(|a, b| a.0.cmp(b.0));

    for (key, value) in sorted_opts {
        match value {
            OptionValue::Bool(true) => args.push(format!("--{}", key)),
            OptionValue::Bool(false) => {} // Omit false flags
            OptionValue::Int(n) => {
                args.push(format!("--{}", key));
                args.push(n.to_string());
            }
            OptionValue::String(s) => {
                args.push(format!("--{}", key));
                args.push(s.clone());
            }
        }
    }

    args
}

/// Execute the gamescope command, replacing the current process.
///
/// Does not return on success - replaces current process with gamescope.
pub fn exec(cmd: GamescopeCommand) -> Result<()> {
    let mut command = Command::new(&cmd.binary);

    for (key, value) in &cmd.env {
        command.env(key, value);
    }

    command.args(&cmd.args);
    command.arg("--");

    if cmd.needs_workaround {
        command.args(["env", "DISABLE_HDR_WSI=1"]);
    }

    command.args(&cmd.child);

    let err = command.exec();
    Err(err).context("Failed to execute gamescope")
}

/// Execute a command directly without gamescope wrapper.
pub fn exec_direct(child_cmd: &[String]) -> Result<()> {
    if child_cmd.is_empty() {
        anyhow::bail!("No command provided");
    }

    let mut command = Command::new(&child_cmd[0]);
    command.args(&child_cmd[1..]);

    let err = command.exec();
    Err(err).context("Failed to execute command")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn mock_profile(use_hdr: bool, use_wsi: bool, binary: &str) -> ResolvedProfile {
        let mut options = HashMap::new();
        options.insert(
            "backend".to_string(),
            OptionValue::String("sdl".to_string()),
        );
        options.insert("fullscreen".to_string(), OptionValue::Bool(true));
        options.insert("output-width".to_string(), OptionValue::Int(2560));

        ResolvedProfile {
            name: "test".to_string(),
            monitor_name: "main".to_string(),
            binary: binary.to_string(),
            use_hdr,
            use_wsi,
            options,
            user_env: HashMap::new(),
        }
    }

    #[test]
    fn test_build_basic_command() {
        let profile = mock_profile(false, false, "gamescope");
        let cmd = build(&profile, &["steam".to_string()]);

        assert_eq!(cmd.binary, "gamescope");
        assert!(cmd.args.contains(&"--fullscreen".to_string()));
        assert!(cmd.args.contains(&"--backend".to_string()));
        assert!(cmd.args.contains(&"sdl".to_string()));
        assert!(!cmd.needs_workaround);
    }

    #[test]
    fn test_build_with_custom_binary() {
        let profile = mock_profile(false, false, "/nix/store/xxx/bin/gamescope");
        let cmd = build(&profile, &["steam".to_string()]);

        assert_eq!(cmd.binary, "/nix/store/xxx/bin/gamescope");
    }

    #[test]
    fn test_build_with_hdr() {
        let profile = mock_profile(true, true, "gamescope");
        let cmd = build(&profile, &["steam".to_string()]);

        assert!(cmd.args.contains(&"--hdr-enabled".to_string()));
        assert!(cmd.args.contains(&"--hdr-debug-force-output".to_string()));
        assert!(cmd.args.contains(&"--hdr-debug-force-support".to_string()));
    }

    #[test]
    fn test_display_format() {
        let profile = mock_profile(false, false, "gamescope");
        let cmd = build(&profile, &["steam".to_string(), "-gamepadui".to_string()]);
        let display = cmd.display();

        assert!(display.starts_with("gamescope"));
        assert!(display.contains("-- steam -gamepadui"));
    }

    #[test]
    fn test_display_no_cloning_overhead() {
        let profile = mock_profile(false, false, "gamescope");
        let cmd = build(&profile, &["steam".to_string()]);

        // Call display multiple times - should be efficient
        let d1 = cmd.display();
        let d2 = cmd.display();
        assert_eq!(d1, d2);
    }
}
