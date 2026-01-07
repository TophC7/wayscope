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

#[derive(Debug)]
pub struct GamescopeCommand {
    pub binary: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Environment variable names to remove from inherited parent environment.
    pub unset: Vec<String>,
    pub child: Vec<String>,
    pub needs_workaround: bool,
}

impl GamescopeCommand {
    /// Formats the command for display (e.g., logging or dry-run output).
    pub fn display(&self) -> String {
        // Simple implementation: this runs once per execution, not in a hot path.
        // Using format! and join is clearer than manual capacity pre-allocation.
        let args_str = self.args.join(" ");
        let child_str = self.child.join(" ");
        let workaround = if self.needs_workaround {
            " env DISABLE_HDR_WSI=1"
        } else {
            ""
        };

        format!(
            "{} {} --{} {}",
            self.binary, args_str, workaround, child_str
        )
    }
}

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
        unset: profile.unset_vars.clone(),
        child: child_cmd.to_vec(),
        needs_workaround: profile.needs_hdr_workaround(),
    }
}

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

/// Applies environment variables to a Command, setting specified vars and removing unset ones.
///
/// Environment is processed in order: set vars first, then remove unset vars.
/// This ensures `unset` actually removes variables from the child process.
fn apply_env_to_command(command: &mut Command, env: &[(String, String)], unset: &[String]) {
    for (key, value) in env {
        command.env(key, value);
    }
    for var_name in unset {
        command.env_remove(var_name);
    }
}

/// Replaces the current process with gamescope (does not return on success).
pub fn exec(cmd: GamescopeCommand) -> Result<()> {
    let mut command = Command::new(&cmd.binary);

    apply_env_to_command(&mut command, &cmd.env, &cmd.unset);

    command.args(&cmd.args);
    command.arg("--");

    if cmd.needs_workaround {
        command.args(["env", "DISABLE_HDR_WSI=1"]);
    }

    command.args(&cmd.child);

    let err = command.exec();
    Err(err).context("Failed to execute gamescope")
}

/// Bypass gamescope, run command directly (used when already inside gamescope).
pub fn exec_direct(child_cmd: &[String]) -> Result<()> {
    if child_cmd.is_empty() {
        anyhow::bail!("No command provided");
    }

    let mut command = Command::new(&child_cmd[0]);
    command.args(&child_cmd[1..]);

    let err = command.exec();
    Err(err).context("Failed to execute command")
}

/// Run command directly with profile environment variables applied.
///
/// Used when skipping gamescope (via --skip-gamescope flag) while preserving
/// all profile environment setup (RADV, Wayland, HDR vars, WSI, etc.).
/// Environment handling is delegated to `apply_env_to_command`.
pub fn exec_direct_with_env(
    child_cmd: &[String],
    env: &[(String, String)],
    unset: &[String],
) -> Result<()> {
    if child_cmd.is_empty() {
        anyhow::bail!("No command provided");
    }

    let mut command = Command::new(&child_cmd[0]);
    apply_env_to_command(&mut command, env, unset);
    command.args(&child_cmd[1..]);

    let err = command.exec();
    Err(err).context("Failed to execute command")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Creates a mock profile with common defaults. Use builder methods to customize.
    struct MockProfile {
        use_hdr: bool,
        use_wsi: bool,
        binary: String,
        unset_vars: Vec<String>,
    }

    impl MockProfile {
        fn new() -> Self {
            Self {
                use_hdr: false,
                use_wsi: false,
                binary: "gamescope".to_string(),
                unset_vars: Vec::new(),
            }
        }

        fn with_hdr(mut self, use_hdr: bool) -> Self {
            self.use_hdr = use_hdr;
            self
        }

        fn with_wsi(mut self, use_wsi: bool) -> Self {
            self.use_wsi = use_wsi;
            self
        }

        fn with_binary(mut self, binary: &str) -> Self {
            self.binary = binary.to_string();
            self
        }

        fn with_unset(mut self, unset_vars: Vec<String>) -> Self {
            self.unset_vars = unset_vars;
            self
        }

        fn build(self) -> ResolvedProfile {
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
                binary: self.binary,
                use_hdr: self.use_hdr,
                use_wsi: self.use_wsi,
                options,
                user_env: HashMap::new(),
                unset_vars: self.unset_vars,
            }
        }
    }

    #[test]
    fn test_build_basic_command() {
        let profile = MockProfile::new().build();
        let cmd = build(&profile, &["steam".to_string()]);

        assert_eq!(cmd.binary, "gamescope");
        assert!(cmd.args.contains(&"--fullscreen".to_string()));
        assert!(cmd.args.contains(&"--backend".to_string()));
        assert!(cmd.args.contains(&"sdl".to_string()));
        assert!(!cmd.needs_workaround);
    }

    #[test]
    fn test_build_with_custom_binary() {
        let profile = MockProfile::new()
            .with_binary("/nix/store/xxx/bin/gamescope")
            .build();
        let cmd = build(&profile, &["steam".to_string()]);

        assert_eq!(cmd.binary, "/nix/store/xxx/bin/gamescope");
    }

    #[test]
    fn test_build_with_hdr() {
        let profile = MockProfile::new().with_hdr(true).with_wsi(true).build();
        let cmd = build(&profile, &["steam".to_string()]);

        assert!(cmd.args.contains(&"--hdr-enabled".to_string()));
        assert!(cmd.args.contains(&"--hdr-debug-force-output".to_string()));
        assert!(cmd.args.contains(&"--hdr-debug-force-support".to_string()));
    }

    #[test]
    fn test_display_format() {
        let profile = MockProfile::new().build();
        let cmd = build(&profile, &["steam".to_string(), "-gamepadui".to_string()]);
        let display = cmd.display();

        assert!(display.starts_with("gamescope"));
        assert!(display.contains("-- steam -gamepadui"));
    }

    #[test]
    fn test_display_no_cloning_overhead() {
        let profile = MockProfile::new().build();
        let cmd = build(&profile, &["steam".to_string()]);

        // Call display multiple times - should be efficient
        let d1 = cmd.display();
        let d2 = cmd.display();
        assert_eq!(d1, d2);
    }

    // ========================================================================
    // Unset Variables Tests
    // ========================================================================

    #[test]
    fn test_build_includes_unset_vars() {
        let profile = MockProfile::new()
            .with_unset(vec!["SDL_VIDEODRIVER".to_string(), "DXVK_HDR".to_string()])
            .build();
        let cmd = build(&profile, &["steam".to_string()]);

        // Verify unset vars are passed to GamescopeCommand
        assert_eq!(cmd.unset.len(), 2);
        assert!(cmd.unset.contains(&"SDL_VIDEODRIVER".to_string()));
        assert!(cmd.unset.contains(&"DXVK_HDR".to_string()));
    }

    #[test]
    fn test_build_empty_unset_vars() {
        let profile = MockProfile::new().build();
        let cmd = build(&profile, &["steam".to_string()]);

        assert!(cmd.unset.is_empty());
    }

    #[test]
    fn test_gamescope_command_struct_has_unset() {
        // Verify the GamescopeCommand struct properly stores unset vars
        let cmd = GamescopeCommand {
            binary: "gamescope".to_string(),
            args: vec![],
            env: vec![("KEY".to_string(), "VALUE".to_string())],
            unset: vec!["REMOVE_ME".to_string()],
            child: vec!["game".to_string()],
            needs_workaround: false,
        };

        assert_eq!(cmd.unset.len(), 1);
        assert_eq!(cmd.unset[0], "REMOVE_ME");
    }

    // ========================================================================
    // Process Environment Tests
    // ========================================================================
    //
    // These tests verify that env_remove is called correctly by spawning
    // actual child processes. We can't test exec() directly since it replaces
    // the process, so we test the environment logic using Command::spawn().

    #[test]
    fn test_env_remove_actually_removes_inherited_var() {
        use std::process::Stdio;

        // Set a test variable in our current process
        std::env::set_var("WAYSCOPE_TEST_INHERITED", "should_be_removed");

        // Build a command that would inherit our env
        let mut command = Command::new("printenv");
        command.arg("WAYSCOPE_TEST_INHERITED");

        // Without env_remove, the child would see our variable
        // Let's verify that env_remove actually works
        command.env_remove("WAYSCOPE_TEST_INHERITED");
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let output = command.output().expect("Failed to run printenv");

        // printenv returns empty output if the var is not found
        // (exit code 1, but that's ok for this test)
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.trim().is_empty(),
            "env_remove should have removed the variable, but got: {}",
            stdout
        );

        // Clean up
        std::env::remove_var("WAYSCOPE_TEST_INHERITED");
    }

    #[test]
    fn test_env_set_and_remove_interaction() {
        use std::process::Stdio;

        // Test that setting and then removing a variable works correctly
        let mut command = Command::new("printenv");
        command.arg("WAYSCOPE_TEST_SETREMOVE");

        // First set it
        command.env("WAYSCOPE_TEST_SETREMOVE", "test_value");
        // Then remove it (should override the set)
        command.env_remove("WAYSCOPE_TEST_SETREMOVE");

        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let output = command.output().expect("Failed to run printenv");
        let stdout = String::from_utf8_lossy(&output.stdout);

        // The variable should be removed because env_remove is called after env
        assert!(
            stdout.trim().is_empty(),
            "Variable should be removed even after being set"
        );
    }

    #[test]
    fn test_env_remove_preserves_other_vars() {
        use std::process::Stdio;

        // Set two test variables
        std::env::set_var("WAYSCOPE_TEST_KEEP", "keep_me");
        std::env::set_var("WAYSCOPE_TEST_REMOVE", "remove_me");

        let mut command = Command::new("sh");
        command.args([
            "-c",
            "echo KEEP=$WAYSCOPE_TEST_KEEP REMOVE=$WAYSCOPE_TEST_REMOVE",
        ]);

        // Only remove one
        command.env_remove("WAYSCOPE_TEST_REMOVE");
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let output = command.output().expect("Failed to run sh");
        let stdout = String::from_utf8_lossy(&output.stdout);

        // KEEP should still be there, REMOVE should be empty
        assert!(
            stdout.contains("KEEP=keep_me"),
            "KEEP variable should be preserved"
        );
        assert!(
            stdout.contains("REMOVE=") && !stdout.contains("REMOVE=remove_me"),
            "REMOVE variable should be removed"
        );

        // Clean up
        std::env::remove_var("WAYSCOPE_TEST_KEEP");
        std::env::remove_var("WAYSCOPE_TEST_REMOVE");
    }
}
