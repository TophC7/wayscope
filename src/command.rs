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

/// Env vars "hoisted" from the parent process: captured from wayscope's own
/// environment, stripped from gamescope, and re-exported to gamescope's child
/// (reaper → pressure-vessel → proton → game) via an `env K=V` prefix.
///
/// # Why these specifically
///
/// Both are injected by Steam's per-game launch-options mechanism and target
/// the *game* process, not the compositor:
///
/// - `LD_PRELOAD` carries `gameoverlayrenderer.so` (Steam overlay + controller
///   hotplug). Intended to be preloaded into the game so Steam can draw UI
///   and intercept input. Has no business loading into gamescope itself.
/// - `LD_LIBRARY_PATH` carries Steam's Ubuntu steam-runtime `pinned_libs_*`.
///   Intended for the game's Ubuntu-ABI Steam launcher. On NixOS it causes
///   gamescope's dynamic loader to resolve glibc/Vulkan/Wayland against
///   mismatched versions — segfault before `main()` (exit 139, no stderr).
///
/// # Why this is distro-neutral
///
/// On any distro, these vars target the game's process environment; nothing
/// in gamescope itself needs them. Hoisting is a no-op on non-Steam launches
/// (the vars aren't set) and a correctness fix on Steam launches (they are).
/// NixOS users happen to suffer the most visible symptom (segfault), but the
/// semantics — "these env vars belong to the child, not the compositor" —
/// hold identically everywhere.
///
/// # Profile override
///
/// If a profile's `environment` attribute explicitly sets one of these vars,
/// hoisting is skipped for that name: user intent wins. The profile-set
/// value is applied to gamescope *and* inherited by the child as normal.
const HOIST_ENV: &[&str] = &["LD_PRELOAD", "LD_LIBRARY_PATH"];

#[derive(Debug)]
pub struct GamescopeCommand {
    pub binary: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Environment variable names to remove from inherited parent environment.
    pub unset: Vec<String>,
    /// Env vars captured from wayscope's own env at build time, to be
    /// re-exported to gamescope's child via an `env K=V ...` prefix.
    /// See [`HOIST_ENV`] for rationale.
    pub hoisted_env: Vec<(String, String)>,
    pub child: Vec<String>,
    pub needs_workaround: bool,
}

impl GamescopeCommand {
    /// Builds the child-side env prefix tokens: `["env", "KEY=VAL", ...]`.
    ///
    /// Combines the HDR workaround (`DISABLE_HDR_WSI=1` when applicable) with
    /// any hoisted vars. Returns an empty vector when neither applies, so
    /// callers can skip emitting the `env` token entirely.
    fn child_env_prefix(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.needs_workaround {
            out.push("DISABLE_HDR_WSI=1".to_string());
        }
        for (k, v) in &self.hoisted_env {
            out.push(format!("{}={}", k, v));
        }
        if out.is_empty() {
            return out;
        }
        // Prepend the literal `env` program; it's what actually applies
        // the K=V pairs to the child process.
        out.insert(0, "env".to_string());
        out
    }

    /// Formats the command for display (e.g., logging or dry-run output).
    pub fn display(&self) -> String {
        // Simple implementation: this runs once per execution, not in a hot path.
        // Using format! and join is clearer than manual capacity pre-allocation.
        let args_str = self.args.join(" ");
        let child_str = self.child.join(" ");
        let prefix = self.child_env_prefix();
        let prefix_str = if prefix.is_empty() {
            String::new()
        } else {
            format!(" {}", prefix.join(" "))
        };

        format!(
            "{} {} --{} {}",
            self.binary, args_str, prefix_str, child_str
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

    // Capture hoist candidates from wayscope's own env. Reading happens BEFORE
    // we hand the Command object to ld.so via exec(), which is what matters:
    // the values travel with the GamescopeCommand struct, not via inherited
    // env. See `HOIST_ENV` for why these specific vars.
    //
    // Profile-level `environment` overrides hoisting: if the user explicitly
    // set LD_PRELOAD in their profile, they want it on gamescope (and
    // inherited by the child). Don't second-guess.
    let hoisted_env: Vec<(String, String)> = HOIST_ENV
        .iter()
        .filter(|name| !profile.user_env.contains_key(**name))
        .filter_map(|name| std::env::var(name).ok().map(|v| ((*name).to_string(), v)))
        .collect();

    // Extend the unset list with every var we're hoisting, so gamescope's
    // process sees none of them. Preserve ordering and avoid duplicates.
    let mut unset = profile.unset_vars.clone();
    for (name, _) in &hoisted_env {
        if !unset.iter().any(|u| u == name) {
            unset.push(name.clone());
        }
    }

    GamescopeCommand {
        binary: profile.binary.clone(),
        args,
        env: profile.environment(),
        unset,
        hoisted_env,
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

    // Child-side env prefix: HDR workaround + hoisted vars (if any).
    // The `env` utility applies K=V pairs then exec's the rest of argv, so
    // the game process sees LD_PRELOAD/LD_LIBRARY_PATH even though gamescope
    // itself was launched with them stripped.
    let prefix = cmd.child_env_prefix();
    command.args(&prefix);

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
        with_env_vars(CLEAN_HOIST_ENV, || {
            let profile = MockProfile::new().build();
            let cmd = build(&profile, &["steam".to_string(), "-gamepadui".to_string()]);
            let display = cmd.display();

            assert!(display.starts_with("gamescope"));
            // With no hoisted env and no HDR workaround, child runs verbatim.
            assert!(display.contains("-- steam -gamepadui"));
        });
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
        with_env_vars(CLEAN_HOIST_ENV, || {
            let profile = MockProfile::new()
                .with_unset(vec!["SDL_VIDEODRIVER".to_string(), "DXVK_HDR".to_string()])
                .build();
            let cmd = build(&profile, &["steam".to_string()]);

            // With no LD_* in env, only profile-specific unset entries exist.
            assert_eq!(cmd.unset.len(), 2);
            assert!(cmd.unset.contains(&"SDL_VIDEODRIVER".to_string()));
            assert!(cmd.unset.contains(&"DXVK_HDR".to_string()));
        });
    }

    #[test]
    fn test_build_empty_unset_vars() {
        with_env_vars(CLEAN_HOIST_ENV, || {
            let profile = MockProfile::new().build();
            let cmd = build(&profile, &["steam".to_string()]);

            assert!(cmd.unset.is_empty());
        });
    }

    #[test]
    fn test_gamescope_command_struct_has_unset() {
        // Verify the GamescopeCommand struct properly stores unset vars
        let cmd = GamescopeCommand {
            binary: "gamescope".to_string(),
            args: vec![],
            env: vec![("KEY".to_string(), "VALUE".to_string())],
            unset: vec!["REMOVE_ME".to_string()],
            hoisted_env: vec![],
            child: vec!["game".to_string()],
            needs_workaround: false,
        };

        assert_eq!(cmd.unset.len(), 1);
        assert_eq!(cmd.unset[0], "REMOVE_ME");
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
                let cmd = build(&profile, &["steam".to_string()]);

                // Hoisted: value captured from parent env
                assert_eq!(cmd.hoisted_env.len(), 1);
                assert_eq!(cmd.hoisted_env[0].0, "LD_PRELOAD");
                assert_eq!(cmd.hoisted_env[0].1, "/fake/overlay.so");

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
                let cmd = build(&profile, &["steam".to_string()]);

                assert_eq!(cmd.hoisted_env.len(), 2);
                assert!(cmd.unset.contains(&"LD_PRELOAD".to_string()));
                assert!(cmd.unset.contains(&"LD_LIBRARY_PATH".to_string()));
            },
        );
    }

    #[test]
    fn test_hoist_noop_when_parent_unset() {
        with_env_vars(CLEAN_HOIST_ENV, || {
            let profile = MockProfile::new().build();
            let cmd = build(&profile, &["steam".to_string()]);

            assert!(cmd.hoisted_env.is_empty());
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
                let cmd = build(&profile, &["steam".to_string()]);

                assert!(!cmd.hoisted_env.iter().any(|(k, _)| k == "LD_PRELOAD"));
                // And not added to unset (profile env would apply it to gamescope)
                assert!(!cmd.unset.contains(&"LD_PRELOAD".to_string()));
            },
        );
    }

    #[test]
    fn test_child_env_prefix_combines_workaround_and_hoist() {
        let cmd = GamescopeCommand {
            binary: "gamescope".to_string(),
            args: vec![],
            env: vec![],
            unset: vec![],
            hoisted_env: vec![("LD_PRELOAD".to_string(), "/over.so".to_string())],
            child: vec!["game".to_string()],
            needs_workaround: true,
        };
        let prefix = cmd.child_env_prefix();
        assert_eq!(prefix[0], "env");
        assert!(prefix.contains(&"DISABLE_HDR_WSI=1".to_string()));
        assert!(prefix.contains(&"LD_PRELOAD=/over.so".to_string()));
    }

    #[test]
    fn test_child_env_prefix_empty_when_nothing_to_inject() {
        let cmd = GamescopeCommand {
            binary: "gamescope".to_string(),
            args: vec![],
            env: vec![],
            unset: vec![],
            hoisted_env: vec![],
            child: vec!["game".to_string()],
            needs_workaround: false,
        };
        assert!(cmd.child_env_prefix().is_empty());
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
