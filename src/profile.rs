//! Profile resolution - combines profile settings with monitor capabilities.
//!
//! Each profile is standalone (no inheritance). Resolution combines:
//! 1. Base environment variables (always applied)
//! 2. Base options derived from monitor config (resolution, refresh, VRR)
//! 3. Profile-specific options (override/extend base)
//! 4. Profile-specific environment (override/extend base)
//! 5. Conditional HDR/WSI environment variables

use std::collections::HashMap;

use crate::config::OptionValue;

// Base environment variable definitions as static tuples to avoid runtime allocations
const BASE_ENV: &[(&str, &str)] = &[
    ("AMD_VULKAN_ICD", "RADV"),
    ("DISABLE_LAYER_AMD_SWITCHABLE_GRAPHICS_1", "1"),
    ("DISABLE_LAYER_NV_OPTIMUS_1", "1"),
    ("GAMESCOPE_WAYLAND_DISPLAY", "gamescope-0"),
    ("PROTON_ADD_CONFIG", "sdlinput,wayland,hdr"),
    ("PROTON_ENABLE_WAYLAND", "1"),
    ("RADV_PERFTEST", "aco"),
    ("SDL_VIDEODRIVER", "wayland"),
];

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
    /// Builds the complete environment: base vars + user vars + conditional HDR/WSI vars - unset vars.
    ///
    /// Environment variables are applied in this order:
    /// 1. Base environment variables (BASE_ENV constants)
    /// 2. User-defined environment from profile
    /// 3. Conditional HDR/WSI environment variables
    /// 4. Unset variables (removed from final environment)
    pub fn environment(&self) -> Vec<(String, String)> {
        let mut env: HashMap<String, String> = BASE_ENV
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();

        env.extend(self.user_env.clone());

        if self.use_wsi {
            env.insert("ENABLE_GAMESCOPE_WSI".to_string(), "1".to_string());
        }

        if self.use_hdr {
            env.insert("DXVK_HDR".to_string(), "1".to_string());
            env.insert("ENABLE_HDR_WSI".to_string(), "1".to_string());
            env.insert("PROTON_ENABLE_HDR".to_string(), "1".to_string());
        }

        // Apply unset variables (remove specified variables from environment)
        for var_name in &self.unset_vars {
            env.remove(var_name);
        }

        let mut sorted: Vec<_> = env.into_iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        sorted
    }

    /// Wayland backend + WSI + HDR requires DISABLE_HDR_WSI=1 on the child process.
    pub fn needs_hdr_workaround(&self) -> bool {
        let backend = self
            .options
            .get("backend")
            .map(|v| v.to_string())
            .unwrap_or_default();
        backend == "wayland" && self.use_wsi && self.use_hdr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();

        assert_eq!(env_map.get("AMD_VULKAN_ICD"), Some(&"RADV".to_string()));
        assert_eq!(env_map.get("SDL_VIDEODRIVER"), Some(&"wayland".to_string()));
    }

    #[test]
    fn test_hdr_environment() {
        let profile = mock_profile(true, true, "sdl");
        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();

        assert_eq!(env_map.get("DXVK_HDR"), Some(&"1".to_string()));
        assert_eq!(env_map.get("ENABLE_HDR_WSI"), Some(&"1".to_string()));
        assert_eq!(env_map.get("PROTON_ENABLE_HDR"), Some(&"1".to_string()));
    }

    #[test]
    fn test_no_hdr_when_disabled() {
        let profile = mock_profile(false, true, "sdl");
        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();

        assert!(!env_map.contains_key("DXVK_HDR"));
    }

    #[test]
    fn test_wsi_environment() {
        let profile = mock_profile(false, true, "sdl");
        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();

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

        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();
        assert!(!env_map.contains_key("CUSTOM_VAR"));
    }

    #[test]
    fn test_unset_nonexistent_variable() {
        let mut profile = mock_profile(false, false, "sdl");
        profile.unset_vars = vec!["NONEXISTENT".to_string()];

        // Should not panic and still has base environment
        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();
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

        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();
        assert!(!env_map.contains_key("VAR"));
    }

    #[test]
    fn test_unset_base_environment() {
        let mut profile = mock_profile(false, false, "sdl");
        profile.unset_vars = vec!["SDL_VIDEODRIVER".to_string()];

        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();
        assert!(!env_map.contains_key("SDL_VIDEODRIVER"));
    }

    #[test]
    fn test_unset_hdr_environment_variable() {
        let mut profile = mock_profile(true, true, "sdl");
        profile.unset_vars = vec!["DXVK_HDR".to_string(), "PROTON_ENABLE_HDR".to_string()];

        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();
        assert!(!env_map.contains_key("DXVK_HDR"));
        assert!(!env_map.contains_key("PROTON_ENABLE_HDR"));
        // But ENABLE_HDR_WSI should still be there (only those two unset)
        assert_eq!(env_map.get("ENABLE_HDR_WSI"), Some(&"1".to_string()));
    }
}
