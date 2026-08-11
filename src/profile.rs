//! Profile resolution - combines profile settings with monitor capabilities.
//!
//! Each profile is standalone (no inheritance). Resolution combines:
//! 1. Base environment variables (always applied)
//! 2. Base options derived from monitor config (resolution, refresh, VRR)
//! 3. Profile-specific options (override/extend base)
//! 4. Profile-specific environment (override/extend base)
//! 5. Conditional HDR/WSI environment variables
//!
//! This module is also the single owner of the *final* environment handed to a
//! child process: [`ResolvedProfile::resolve_environment`] captures wayscope's
//! own environment once and returns a [`ResolvedEnvironment`] that the gamescope
//! path, the `--skip-gamescope` path, and `wayscope show` all consume, so what
//! is reported is always what is applied.

use std::collections::HashMap;

use crate::config::OptionValue;

/// Wayland socket name gamescope serves on. wayscope exports it for gamescope's
/// children and reads it back to detect that it is running nested.
pub const GAMESCOPE_DISPLAY_VAR: &str = "GAMESCOPE_WAYLAND_DISPLAY";

const BASE_ENV: &[(&str, &str)] = &[
    ("AMD_VULKAN_ICD", "RADV"),
    ("DISABLE_LAYER_AMD_SWITCHABLE_GRAPHICS_1", "1"),
    ("DISABLE_LAYER_NV_OPTIMUS_1", "1"),
    (GAMESCOPE_DISPLAY_VAR, "gamescope-0"),
    ("PROTON_ADD_CONFIG", "sdlinput,wayland,hdr"),
    ("PROTON_ENABLE_WAYLAND", "1"),
    ("RADV_PERFTEST", "aco"),
    ("SDL_VIDEODRIVER", "wayland"),
];

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
pub const HOIST_ENV: &[&str] = &["LD_PRELOAD", "LD_LIBRARY_PATH"];

/// Which launch path an environment is being resolved for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Wrapped in gamescope: [`HOIST_ENV`] vars are stripped from the
    /// compositor and re-exported to its child.
    Gamescope,
    /// No compositor: the command wayscope launches *is* the game, so hoisted
    /// vars stay inherited and no gamescope socket is advertised.
    Direct,
}

/// The complete environment for one launch: what to set, what to remove, and
/// what to re-export to the grandchild through an `env K=V` prefix.
#[derive(Debug, Clone)]
pub struct ResolvedEnvironment {
    /// Variables to set on the child, sorted by name.
    pub set: Vec<(String, String)>,
    /// Variable names to remove from the inherited environment.
    pub unset: Vec<String>,
    /// Variables captured from wayscope's own env, to be re-exported past
    /// gamescope. Always empty in [`LaunchMode::Direct`].
    pub hoisted: Vec<(String, String)>,
}

/// A fully resolved profile ready for execution.
///
/// Combines profile settings with monitor configuration into a complete
/// set of options and environment variables for gamescope.
#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub name: String,
    pub monitor_name: String,
    pub binary: String,
    pub use_hdr: bool,
    pub use_wsi: bool,
    /// Merged gamescope CLI options (monitor defaults + profile overrides).
    pub options: HashMap<String, OptionValue>,
    /// Profile-specific environment variables (merged with base env at runtime).
    pub user_env: HashMap<String, String>,
    /// Environment variable names to unset (removes inherited or base variables).
    pub unset_vars: Vec<String>,
}

impl ResolvedProfile {
    /// Builds the complete environment for `mode`.
    ///
    /// Variables are layered in this order:
    /// 1. [`BASE_ENV`] constants
    /// 2. User-defined environment from the profile
    /// 3. Conditional HDR/WSI variables
    /// 4. The profile's `unset` list (removed from the final set)
    ///
    /// In [`LaunchMode::Gamescope`], [`HOIST_ENV`] vars present in wayscope's
    /// own environment are captured into `hoisted` and added to `unset` so the
    /// compositor never sees them. In [`LaunchMode::Direct`] there is no
    /// compositor to protect, so nothing is hoisted and the gamescope socket
    /// name is dropped rather than advertised for a socket that does not exist.
    pub fn resolve_environment(&self, mode: LaunchMode) -> ResolvedEnvironment {
        // +4 headroom for the conditional WSI/HDR entries below.
        let mut env: HashMap<String, String> =
            HashMap::with_capacity(BASE_ENV.len() + self.user_env.len() + 4);
        env.extend(
            BASE_ENV
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
        );
        env.extend(self.user_env.iter().map(|(k, v)| (k.clone(), v.clone())));

        if self.use_wsi {
            env.insert("ENABLE_GAMESCOPE_WSI".to_string(), "1".to_string());
        }

        if self.use_hdr {
            env.insert("DXVK_HDR".to_string(), "1".to_string());
            env.insert("ENABLE_HDR_WSI".to_string(), "1".to_string());
            env.insert("PROTON_ENABLE_HDR".to_string(), "1".to_string());
        }

        let mut unset = self.unset_vars.clone();
        let hoisted = match mode {
            LaunchMode::Gamescope => {
                // Reading the parent env happens here, once, before anything is
                // handed to exec(); the values travel in this struct.
                let hoisted: Vec<(String, String)> = HOIST_ENV
                    .iter()
                    .filter(|name| !self.user_env.contains_key(**name))
                    .filter_map(|name| std::env::var(name).ok().map(|v| ((*name).to_string(), v)))
                    .collect();
                for (name, _) in &hoisted {
                    if !unset.iter().any(|u| u == name) {
                        unset.push(name.clone());
                    }
                }
                hoisted
            }
            LaunchMode::Direct => {
                env.remove(GAMESCOPE_DISPLAY_VAR);
                Vec::new()
            }
        };

        for var_name in &self.unset_vars {
            env.remove(var_name);
        }

        let mut set: Vec<_> = env.into_iter().collect();
        set.sort_by(|a, b| a.0.cmp(&b.0));

        ResolvedEnvironment {
            set,
            unset,
            hoisted,
        }
    }

    /// Options sorted by name, so display order and argv order always match.
    pub fn sorted_options(&self) -> Vec<(&String, &OptionValue)> {
        let mut opts: Vec<_> = self.options.iter().collect();
        opts.sort_by(|a, b| a.0.cmp(b.0));
        opts
    }

    /// Wayland backend + WSI + HDR requires DISABLE_HDR_WSI=1 on the child process.
    pub fn needs_hdr_workaround(&self) -> bool {
        // `matches!` with a guard: pattern-match the typed variant, then compare.
        // A non-string `backend` value is a clean mismatch rather than being
        // stringified into an accidental match.
        matches!(self.options.get("backend"), Some(OptionValue::String(b)) if b == "wayland")
            && self.use_wsi
            && self.use_hdr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolved gamescope-mode environment as a lookup map.
    fn env_map(profile: &ResolvedProfile) -> HashMap<String, String> {
        profile
            .resolve_environment(LaunchMode::Gamescope)
            .set
            .into_iter()
            .collect()
    }

    fn mock_profile(use_hdr: bool, use_wsi: bool, backend: &str) -> ResolvedProfile {
        let mut options = HashMap::new();
        options.insert(
            "backend".to_string(),
            OptionValue::String(backend.to_string()),
        );

        ResolvedProfile {
            name: "test".to_string(),
            monitor_name: "main".to_string(),
            binary: "gamescope".to_string(),
            use_hdr,
            use_wsi,
            options,
            user_env: HashMap::new(),
            unset_vars: Vec::new(),
        }
    }

    #[test]
    fn test_base_environment_included() {
        let profile = mock_profile(false, false, "sdl");
        let env_map = env_map(&profile);

        assert_eq!(env_map.get("AMD_VULKAN_ICD"), Some(&"RADV".to_string()));
        assert_eq!(env_map.get("SDL_VIDEODRIVER"), Some(&"wayland".to_string()));
    }

    #[test]
    fn test_hdr_environment() {
        let profile = mock_profile(true, true, "sdl");
        let env_map = env_map(&profile);

        assert_eq!(env_map.get("DXVK_HDR"), Some(&"1".to_string()));
        assert_eq!(env_map.get("ENABLE_HDR_WSI"), Some(&"1".to_string()));
        assert_eq!(env_map.get("PROTON_ENABLE_HDR"), Some(&"1".to_string()));
    }

    #[test]
    fn test_no_hdr_when_disabled() {
        let profile = mock_profile(false, true, "sdl");
        let env_map = env_map(&profile);

        assert!(!env_map.contains_key("DXVK_HDR"));
    }

    #[test]
    fn test_wsi_environment() {
        let profile = mock_profile(false, true, "sdl");
        let env_map = env_map(&profile);

        assert_eq!(env_map.get("ENABLE_GAMESCOPE_WSI"), Some(&"1".to_string()));
    }

    #[test]
    fn test_hdr_workaround_needed() {
        let profile = mock_profile(true, true, "wayland");
        assert!(profile.needs_hdr_workaround());
    }

    #[test]
    fn test_hdr_workaround_not_needed_sdl() {
        let profile = mock_profile(true, true, "sdl");
        assert!(!profile.needs_hdr_workaround());
    }

    #[test]
    fn test_hdr_workaround_not_needed_no_hdr() {
        let profile = mock_profile(false, true, "wayland");
        assert!(!profile.needs_hdr_workaround());
    }

    #[test]
    fn test_unset_basic_variable() {
        let mut profile = mock_profile(false, false, "sdl");
        profile
            .user_env
            .insert("CUSTOM_VAR".to_string(), "value".to_string());
        profile.unset_vars = vec!["CUSTOM_VAR".to_string()];

        let env_map = env_map(&profile);
        assert!(!env_map.contains_key("CUSTOM_VAR"));
    }

    #[test]
    fn test_direct_mode_drops_gamescope_display() {
        let profile = mock_profile(false, true, "sdl");
        let direct: HashMap<_, _> = profile
            .resolve_environment(LaunchMode::Direct)
            .set
            .into_iter()
            .collect();

        assert!(!direct.contains_key(GAMESCOPE_DISPLAY_VAR));
        assert!(env_map(&profile).contains_key(GAMESCOPE_DISPLAY_VAR));
    }

    #[test]
    fn test_unset_nonexistent_variable() {
        let mut profile = mock_profile(false, false, "sdl");
        profile.unset_vars = vec!["NONEXISTENT".to_string()];

        // Should not panic and still has base environment
        let env_map = env_map(&profile);
        assert!(!env_map.is_empty());
        assert!(env_map.contains_key("AMD_VULKAN_ICD"));
    }

    #[test]
    fn test_unset_overrides_user_env() {
        let mut profile = mock_profile(false, false, "sdl");
        profile
            .user_env
            .insert("VAR".to_string(), "value".to_string());
        profile.unset_vars = vec!["VAR".to_string()];

        let env_map = env_map(&profile);
        assert!(!env_map.contains_key("VAR"));
    }

    #[test]
    fn test_unset_base_environment() {
        let mut profile = mock_profile(false, false, "sdl");
        profile.unset_vars = vec!["SDL_VIDEODRIVER".to_string()];

        let env_map = env_map(&profile);
        assert!(!env_map.contains_key("SDL_VIDEODRIVER"));
    }

    #[test]
    fn test_unset_hdr_environment_variable() {
        let mut profile = mock_profile(true, true, "sdl");
        profile.unset_vars = vec!["DXVK_HDR".to_string(), "PROTON_ENABLE_HDR".to_string()];

        let env_map = env_map(&profile);
        assert!(!env_map.contains_key("DXVK_HDR"));
        assert!(!env_map.contains_key("PROTON_ENABLE_HDR"));
        // But ENABLE_HDR_WSI should still be there (only those two unset)
        assert_eq!(env_map.get("ENABLE_HDR_WSI"), Some(&"1".to_string()));
    }
}
