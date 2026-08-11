//! Gamescope command building and execution.
//!
//! Constructs the gamescope command line from a resolved profile and a
//! [`ResolvedEnvironment`], then `exec`s it, replacing the current process.
//! The argv is built once and shared by execution and display, so the
//! `Exec:` line users copy is exactly what runs.

use std::borrow::Cow;
use std::os::unix::process::CommandExt;
use std::process::Command;

use anyhow::{Context, Result};

use crate::config::OptionValue;
use crate::profile::{ResolvedEnvironment, ResolvedProfile};

/// HDR flags implied by `useHDR`. Wayscope deliberately forces support and
/// output because nested compositor HDR detection is not reliable enough to
/// represent an explicit HDR profile request.
const HDR_FLAGS: &[&str] = &[
    "hdr-enabled",
    "hdr-debug-force-output",
    "hdr-debug-force-support",
];

#[derive(Debug)]
pub struct GamescopeCommand {
    /// Full argv: binary, gamescope options, `--`, optional `env K=V` prefix,
    /// then the child command. Single source of truth for exec and display.
    argv: Vec<String>,
    env: Vec<(String, String)>,
    /// Environment variable names to remove from the inherited parent environment.
    unset: Vec<String>,
    /// Hoisted names retained for diagnostics; values live only in `argv`.
    hoisted_env_names: Vec<String>,
}

impl GamescopeCommand {
    /// Formats shell-escaped argv for display or copying into a POSIX shell.
    pub fn display(&self) -> String {
        let mut display = String::new();
        for (index, arg) in self.argv.iter().enumerate() {
            if index > 0 {
                display.push(' ');
            }
            display.push_str(&shell_escape(arg));
        }
        display
    }

    /// Names of parent variables hoisted past gamescope to its child.
    pub fn hoisted_env_names(&self) -> &[String] {
        &self.hoisted_env_names
    }
}

fn shell_escape(arg: &str) -> Cow<'_, str> {
    if !arg.is_empty()
        && arg.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b'@' | b'%' | b'+' | b'=' | b':' | b',' | b'.' | b'/' | b'-'
                )
        })
    {
        return Cow::Borrowed(arg);
    }

    let mut escaped = String::with_capacity(arg.len() + 2);
    escaped.push('\'');
    for character in arg.chars() {
        if character == '\'' {
            escaped.push_str("'\\''");
        } else {
            escaped.push(character);
        }
    }
    escaped.push('\'');
    Cow::Owned(escaped)
}

/// Builds child-side env prefix tokens: `["env", "KEY=VAL", ...]`.
fn child_env_prefix(child: &[(String, String)], hoisted: &[(String, String)]) -> Vec<String> {
    if child.is_empty() && hoisted.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(child.len() + hoisted.len() + 1);
    out.push("env".to_string());
    out.extend(
        child
            .iter()
            .chain(hoisted)
            .map(|(key, value)| format!("{}={}", key, value)),
    );
    out
}

pub fn build(
    profile: &ResolvedProfile,
    env: ResolvedEnvironment,
    child_cmd: &[String],
) -> GamescopeCommand {
    let prefix = child_env_prefix(&env.child, &env.hoisted);
    let hoisted_env_names = env.hoisted.into_iter().map(|(name, _)| name).collect();

    let mut argv =
        Vec::with_capacity(profile.options.len() * 2 + prefix.len() + child_cmd.len() + 5);
    argv.push(profile.binary.clone());
    append_args(profile, &mut argv);
    argv.push("--".to_string());
    argv.extend(prefix);
    argv.extend_from_slice(child_cmd);

    GamescopeCommand {
        argv,
        env: env.set,
        unset: env.unset,
        hoisted_env_names,
    }
}

fn append_args(profile: &ResolvedProfile, args: &mut Vec<String>) {
    for (key, value) in profile.sorted_options() {
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

    if profile.use_hdr {
        for flag in HDR_FLAGS
            .iter()
            .filter(|flag| !profile.options.contains_key(**flag))
        {
            args.push(format!("--{}", flag));
        }
    }
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
    let mut command = Command::new(&cmd.argv[0]);
    apply_env_to_command(&mut command, &cmd.env, &cmd.unset);
    command.args(&cmd.argv[1..]);

    let err = command.exec();
    Err(err).context("Failed to execute gamescope")
}

/// Runs a command directly, with the given environment applied.
///
/// Used when gamescope is skipped (`--skip-gamescope`) or when wayscope is
/// already running inside gamescope; pass empty slices to inherit unchanged.
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
    use crate::profile::LaunchMode;
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
        let cmd = build(
            &profile,
            profile.resolve_environment(LaunchMode::Gamescope),
            &["steam".to_string()],
        );

        assert_eq!(cmd.argv[0], "gamescope");
        assert!(cmd.argv.contains(&"--fullscreen".to_string()));
        assert!(cmd.argv.contains(&"--backend".to_string()));
        assert!(cmd.argv.contains(&"sdl".to_string()));
    }

    #[test]
    fn test_build_with_custom_binary() {
        let profile = MockProfile::new()
            .with_binary("/nix/store/xxx/bin/gamescope")
            .build();
        let cmd = build(
            &profile,
            profile.resolve_environment(LaunchMode::Gamescope),
            &["steam".to_string()],
        );

        assert_eq!(cmd.argv[0], "/nix/store/xxx/bin/gamescope");
    }

    #[test]
    fn test_build_with_hdr() {
        let profile = MockProfile::new().with_hdr(true).with_wsi(true).build();
        let cmd = build(
            &profile,
            profile.resolve_environment(LaunchMode::Gamescope),
            &["steam".to_string()],
        );

        assert!(cmd.argv.contains(&"--hdr-enabled".to_string()));
        assert!(cmd.argv.contains(&"--hdr-debug-force-output".to_string()));
        assert!(cmd.argv.contains(&"--hdr-debug-force-support".to_string()));
    }

    #[test]
    fn test_display_shell_escapes_argv() {
        with_env_vars(CLEAN_HOIST_ENV, || {
            let profile = MockProfile::new().build();
            let cmd = build(
                &profile,
                profile.resolve_environment(LaunchMode::Gamescope),
                &[
                    "steam client".to_string(),
                    "it's".to_string(),
                    "$HOME;rm".to_string(),
                    String::new(),
                ],
            );
            let display = cmd.display();

            assert!(display.starts_with("gamescope"));
            assert!(
                display.ends_with(
                    "-- env DISABLE_GAMESCOPE_WSI=1 ENABLE_GAMESCOPE_WSI=0 'steam client' 'it'\\''s' '$HOME;rm' ''"
                ),
                "unexpected display: {display}"
            );
        });
    }

    // ========================================================================
    // Unset Variables Tests
    // ========================================================================

    #[test]
    fn test_build_includes_unset_vars() {
        with_env_vars(CLEAN_HOIST_ENV, || {
            let profile = MockProfile::new()
                .with_unset(vec!["SDL_VIDEODRIVER".to_string(), "DXVK_HDR".to_string()])
                .build();
            let cmd = build(
                &profile,
                profile.resolve_environment(LaunchMode::Gamescope),
                &["steam".to_string()],
            );

            // Profile entries plus inherited legacy HDR and parent WSI switches.
            assert_eq!(cmd.unset.len(), 5);
            assert!(cmd.unset.contains(&"SDL_VIDEODRIVER".to_string()));
            assert!(cmd.unset.contains(&"DXVK_HDR".to_string()));
        });
    }

    #[test]
    fn test_build_empty_unset_vars() {
        with_env_vars(CLEAN_HOIST_ENV, || {
            let profile = MockProfile::new().build();
            let cmd = build(
                &profile,
                profile.resolve_environment(LaunchMode::Gamescope),
                &["steam".to_string()],
            );

            assert_eq!(
                cmd.unset,
                [
                    "ENABLE_HDR_WSI",
                    "ENABLE_GAMESCOPE_WSI",
                    "DISABLE_GAMESCOPE_WSI"
                ]
            );
        });
    }

    // ========================================================================
    // Hoist Env Tests
    // ========================================================================
    //
    // std::env is process-global. The test runner parallelizes by default,
    // so a Mutex serializes access and scoped helpers restore prior state.
    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Applies a list of (name, value) overrides for the duration of `f`,
    /// restoring prior state after. Pass `None` to unset a var.
    ///
    /// Takes a slice (not nested calls) because the mutex is non-reentrant —
    /// nesting would deadlock.
    fn with_env_vars<R>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> R) -> R {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let prior: Vec<_> = vars
            .iter()
            .map(|(k, _)| ((*k).to_string(), std::env::var(*k).ok()))
            .collect();
        for (k, v) in vars {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        let result = f();
        for (k, v) in &prior {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        result
    }

    /// Neutralized parent env for tests that must not see ambient LD_* from
    /// the shell `cargo test` inherits.
    const CLEAN_HOIST_ENV: &[(&str, Option<&str>)] =
        &[("LD_PRELOAD", None), ("LD_LIBRARY_PATH", None)];

    #[test]
    fn test_hoist_captures_ld_preload_from_parent() {
        with_env_vars(
            &[
                ("LD_PRELOAD", Some("/fake/overlay.so")),
                ("LD_LIBRARY_PATH", None),
            ],
            || {
                let profile = MockProfile::new().build();
                let cmd = build(
                    &profile,
                    profile.resolve_environment(LaunchMode::Gamescope),
                    &["steam".to_string()],
                );

                // Value moves into argv; diagnostics retain only the name.
                assert_eq!(cmd.hoisted_env_names, ["LD_PRELOAD"]);
                assert!(cmd
                    .argv
                    .contains(&"LD_PRELOAD=/fake/overlay.so".to_string()));

                // Stripped: name appears in unset so gamescope's env drops it
                assert!(cmd.unset.contains(&"LD_PRELOAD".to_string()));
            },
        );
    }

    #[test]
    fn test_hoist_captures_both_vars() {
        with_env_vars(
            &[
                ("LD_PRELOAD", Some("/fake/overlay.so")),
                ("LD_LIBRARY_PATH", Some("/fake/pinned:/more")),
            ],
            || {
                let profile = MockProfile::new().build();
                let cmd = build(
                    &profile,
                    profile.resolve_environment(LaunchMode::Gamescope),
                    &["steam".to_string()],
                );

                assert_eq!(cmd.hoisted_env_names.len(), 2);
                assert!(cmd.unset.contains(&"LD_PRELOAD".to_string()));
                assert!(cmd.unset.contains(&"LD_LIBRARY_PATH".to_string()));
            },
        );
    }

    #[test]
    fn test_hoist_noop_when_parent_unset() {
        with_env_vars(CLEAN_HOIST_ENV, || {
            let profile = MockProfile::new().build();
            let cmd = build(
                &profile,
                profile.resolve_environment(LaunchMode::Gamescope),
                &["steam".to_string()],
            );

            assert!(cmd.hoisted_env_names.is_empty());
            // Nothing to hoist → nothing added to unset for these names
            assert!(!cmd.unset.contains(&"LD_PRELOAD".to_string()));
            assert!(!cmd.unset.contains(&"LD_LIBRARY_PATH".to_string()));
        });
    }

    #[test]
    fn test_hoist_skipped_when_profile_sets_var() {
        // Profile explicitly sets LD_PRELOAD → user intent wins, no hoist.
        with_env_vars(
            &[
                ("LD_PRELOAD", Some("/parent/overlay.so")),
                ("LD_LIBRARY_PATH", None),
            ],
            || {
                let mut profile = MockProfile::new().build();
                profile
                    .user_env
                    .insert("LD_PRELOAD".to_string(), "/profile/custom.so".to_string());
                let cmd = build(
                    &profile,
                    profile.resolve_environment(LaunchMode::Gamescope),
                    &["steam".to_string()],
                );

                assert!(!cmd
                    .hoisted_env_names
                    .iter()
                    .any(|name| name == "LD_PRELOAD"));
                // And not added to unset (profile env would apply it to gamescope)
                assert!(!cmd.unset.contains(&"LD_PRELOAD".to_string()));
            },
        );
    }

    #[test]
    fn test_child_env_prefix_combines_wsi_and_hoist() {
        let prefix = child_env_prefix(
            &[("ENABLE_GAMESCOPE_WSI".to_string(), "1".to_string())],
            &[("LD_PRELOAD".to_string(), "/over.so".to_string())],
        );
        assert_eq!(prefix[0], "env");
        assert!(prefix.contains(&"ENABLE_GAMESCOPE_WSI=1".to_string()));
        assert!(prefix.contains(&"LD_PRELOAD=/over.so".to_string()));
    }

    #[test]
    fn test_child_env_prefix_empty_when_nothing_to_inject() {
        assert!(child_env_prefix(&[], &[]).is_empty());
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
