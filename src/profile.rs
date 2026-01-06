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
    ("PROTON_ADD_CONFIG", "sdlinput,wayland"),
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
    /// Profile name for display purposes.
    pub name: String,

    /// Monitor name being used.
    pub monitor_name: String,

    /// Path to gamescope binary.
    pub binary: String,

    /// Whether HDR output is enabled.
    pub use_hdr: bool,

    /// Whether Gamescope WSI is enabled.
    pub use_wsi: bool,

    /// Fully merged gamescope CLI options.
    pub options: HashMap<String, OptionValue>,

    /// User-specified environment variables.
    pub user_env: HashMap<String, String>,
}

impl ResolvedProfile {
    /// Generate the complete environment variable list.
    ///
    /// Combines base environment, profile environment, and conditional
    /// HDR/WSI variables into a sorted list for consistent output.
    pub fn environment(&self) -> Vec<(String, String)> {
        let mut env: HashMap<String, String> = BASE_ENV
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();

        // Merge user-specified environment
        env.extend(self.user_env.clone());

        // Conditional WSI environment
        if self.use_wsi {
            env.insert("ENABLE_GAMESCOPE_WSI".to_string(), "1".to_string());
        }

        // Conditional HDR environment
        if self.use_hdr {
            env.insert("DXVK_HDR".to_string(), "1".to_string());
            env.insert("ENABLE_HDR_WSI".to_string(), "1".to_string());
            env.insert("PROTON_ENABLE_HDR".to_string(), "1".to_string());
        }

        let mut sorted: Vec<_> = env.into_iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        sorted
    }

    /// Check if the HDR workaround is needed.
    ///
    /// When running with wayland backend + WSI + HDR, the child process
    /// needs DISABLE_HDR_WSI=1 to trick gamescope into properly enabling HDR.
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
}
