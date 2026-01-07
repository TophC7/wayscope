//! Configuration file parsing and types.
//!
//! Configuration is split across two files:
//! - `monitors.yaml` - Display definitions with resolution and capabilities
//! - `config.yaml` - Profile definitions that reference monitors

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::profile::ResolvedProfile;

// ============================================================================
// Environment Variable Name Validation
// ============================================================================

/// Validates that an environment variable name follows POSIX conventions.
///
/// POSIX environment variable names must:
/// - Start with a letter (A-Z, a-z) or underscore (_)
/// - Contain only letters, digits (0-9), and underscores
/// - Not be empty
///
/// # Arguments
///
/// * `name` - The environment variable name to validate
///
/// # Returns
///
/// `true` if the name is valid, `false` otherwise
///
/// # Examples
///
/// ```
/// assert!(is_valid_env_var_name("MY_VAR"));
/// assert!(is_valid_env_var_name("_PRIVATE"));
/// assert!(is_valid_env_var_name("var123"));
/// assert!(!is_valid_env_var_name(""));        // Empty
/// assert!(!is_valid_env_var_name("123VAR"));  // Starts with digit
/// assert!(!is_valid_env_var_name("MY=VAR"));  // Contains =
/// assert!(!is_valid_env_var_name("MY VAR"));  // Contains space
/// ```
fn is_valid_env_var_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();

    // First character must be letter or underscore
    // Note: Using pattern matching for clarity - this is a Rust idiom
    // that Python developers should recognize as similar to `if c in 'abc...'`
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }

    // Remaining characters must be letters, digits, or underscores
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Validates all environment variable names in a profile and returns errors for invalid ones.
///
/// # Arguments
///
/// * `profile_name` - Name of the profile (for error messages)
/// * `env_keys` - Iterator of environment variable names to validate
/// * `unset_vars` - List of variables to unset
///
/// # Returns
///
/// `Ok(())` if all names are valid, `Err` with details of invalid names otherwise
fn validate_env_var_names<'a>(
    profile_name: &str,
    env_keys: impl Iterator<Item = &'a String>,
    unset_vars: &[String],
) -> Result<()> {
    let mut invalid = Vec::new();

    for name in env_keys {
        if !is_valid_env_var_name(name) {
            invalid.push(format!("environment key '{}'", name));
        }
    }

    for name in unset_vars {
        if !is_valid_env_var_name(name) {
            invalid.push(format!("unset entry '{}'", name));
        }
    }

    if invalid.is_empty() {
        Ok(())
    } else {
        bail!(
            "Profile '{}': invalid environment variable names: {}",
            profile_name,
            invalid.join(", ")
        )
    }
}

/// Wraps serde_yaml with helpful hints for common YAML syntax errors.
fn parse_yaml<T: DeserializeOwned>(content: &str, path: &Path) -> Result<T> {
    serde_yaml::from_str(content).map_err(|e| {
        let mut msg = format!("Failed to parse {}\n", path.display());

        if let Some(location) = e.location() {
            msg.push_str(&format!(
                "  Error at line {}, column {}\n",
                location.line(),
                location.column()
            ));
        }

        msg.push_str(&format!("  {}\n\n", e));
        msg.push_str("Hint: YAML is sensitive to indentation. Common issues:\n");
        msg.push_str("  - Use spaces, not tabs\n");
        msg.push_str("  - Ensure consistent indentation (2 spaces recommended)\n");
        msg.push_str("  - Check for missing colons after keys\n");
        msg.push_str("  - Wrap strings with special characters in quotes");

        anyhow::anyhow!(msg)
    })
}

// ============================================================================
// Monitor Configuration
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct MonitorsConfig {
    #[serde(default)]
    pub monitors: HashMap<String, MonitorDef>,
}

/// Field names match mix.nix format (refreshRate, not refresh_rate).
#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case)]
pub struct MonitorDef {
    pub width: u32,
    pub height: u32,
    #[serde(alias = "refresh")]
    pub refreshRate: u32,
    #[serde(default)]
    pub vrr: bool,
    #[serde(default)]
    pub hdr: bool,
    #[serde(default, alias = "default")]
    pub primary: bool,
}

impl MonitorsConfig {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("wayscope")
    }

    pub fn default_path() -> PathBuf {
        Self::config_dir().join("monitors.yaml")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read: {}", path.display()))?;
        parse_yaml(&content, path)
    }

    fn get(&self, name: &str) -> Result<&MonitorDef> {
        self.monitors
            .get(name)
            .with_context(|| format!("Unknown monitor '{}'", name))
    }

    fn default_monitor(&self) -> Result<(&String, &MonitorDef)> {
        self.monitors
            .iter()
            .find(|(_, m)| m.primary)
            .with_context(|| "No primary monitor. Set 'primary: true' on one monitor.")
    }
}

// ============================================================================
// Profile Configuration
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ProfilesConfig {
    #[serde(default)]
    pub profiles: HashMap<String, ProfileDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileDef {
    pub monitor: Option<String>,
    #[serde(default = "default_binary")]
    pub binary: String,
    #[serde(rename = "useHDR")]
    pub use_hdr: Option<bool>,
    #[serde(rename = "useWSI")]
    pub use_wsi: Option<bool>,
    #[serde(default)]
    pub options: HashMap<String, OptionValue>,
    #[serde(default)]
    pub environment: HashMap<String, EnvValue>,
    #[serde(default)]
    pub unset: Vec<String>,
}

fn default_binary() -> String {
    "gamescope".to_string()
}

impl ProfilesConfig {
    pub fn default_path() -> PathBuf {
        MonitorsConfig::config_dir().join("config.yaml")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read: {}", path.display()))?;
        parse_yaml(&content, path)
    }

    fn get(&self, name: &str) -> Result<&ProfileDef> {
        self.profiles
            .get(name)
            .with_context(|| format!("Unknown profile '{}'", name))
    }

    fn names(&self) -> Vec<&String> {
        let mut names: Vec<_> = self.profiles.keys().collect();
        names.sort();
        names
    }
}

// ============================================================================
// Value Types
// ============================================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OptionValue {
    Bool(bool),
    Int(i64),
    String(String),
}

impl std::fmt::Display for OptionValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{}", b),
            Self::Int(i) => write!(f, "{}", i),
            Self::String(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    Int(i64),
    String(String),
}

impl std::fmt::Display for EnvValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(i) => write!(f, "{}", i),
            Self::String(s) => write!(f, "{}", s),
        }
    }
}

// ============================================================================
// Combined Configuration
// ============================================================================

#[derive(Debug)]
pub struct Config {
    pub monitors: MonitorsConfig,
    pub profiles: ProfilesConfig,
}

impl Config {
    pub fn load(monitors_path: &Path, profiles_path: &Path) -> Result<Self> {
        let monitors = MonitorsConfig::load(monitors_path)?;
        let profiles = ProfilesConfig::load(profiles_path)?;

        // Validate each profile
        for (name, profile) in &profiles.profiles {
            // Validate environment variable names (both set and unset)
            validate_env_var_names(name, profile.environment.keys(), &profile.unset)?;

            // Validate monitor reference exists
            if let Some(ref mon_name) = profile.monitor {
                if !monitors.monitors.contains_key(mon_name) {
                    bail!(
                        "Profile '{}' references unknown monitor '{}'",
                        name,
                        mon_name
                    );
                }
            }
            // Note: We don't deduplicate unset vars because env_remove() is idempotent.
            // Duplicate entries in the config are harmless and removing them adds complexity.
        }

        Ok(Self { monitors, profiles })
    }

    /// Combines profile settings with monitor config into a ready-to-execute profile.
    pub fn resolve_profile(&self, name: &str) -> Result<ResolvedProfile> {
        let profile = self.profiles.get(name)?;

        let (monitor_name, monitor) = match &profile.monitor {
            Some(n) => (n.clone(), self.monitors.get(n)?),
            None => {
                let (n, m) = self.monitors.default_monitor()?;
                (n.clone(), m)
            }
        };

        let mut options = base_options(monitor);
        for (key, value) in &profile.options {
            options.insert(key.clone(), value.clone());
        }

        let user_env = profile
            .environment
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect();

        Ok(ResolvedProfile {
            name: name.to_string(),
            monitor_name,
            binary: profile.binary.clone(),
            use_hdr: profile.use_hdr.unwrap_or(monitor.hdr),
            use_wsi: profile.use_wsi.unwrap_or(true),
            options,
            user_env,
            unset_vars: profile.unset.clone(),
        })
    }

    pub fn list_profiles(&self) -> Vec<(String, String)> {
        self.profiles
            .names()
            .into_iter()
            .filter_map(|name| {
                self.resolve_profile(name).ok().map(|p| {
                    let summary = format!(
                        "monitor={} HDR={} WSI={}",
                        p.monitor_name, p.use_hdr, p.use_wsi
                    );
                    // p.name is already owned; no need to clone `name` again
                    (p.name, summary)
                })
            })
            .collect()
    }
}

/// Sensible gamescope defaults derived from monitor specs.
fn base_options(monitor: &MonitorDef) -> HashMap<String, OptionValue> {
    let mut opts = HashMap::with_capacity(10);

    opts.insert(
        "backend".to_string(),
        OptionValue::String("sdl".to_string()),
    );
    opts.insert("fade-out-duration".to_string(), OptionValue::Int(200));
    opts.insert("fullscreen".to_string(), OptionValue::Bool(true));
    opts.insert("immediate-flips".to_string(), OptionValue::Bool(true));
    opts.insert(
        "nested-refresh".to_string(),
        OptionValue::Int(i64::from(monitor.refreshRate)),
    );
    opts.insert(
        "output-height".to_string(),
        OptionValue::Int(i64::from(monitor.height)),
    );
    opts.insert(
        "output-width".to_string(),
        OptionValue::Int(i64::from(monitor.width)),
    );
    opts.insert("rt".to_string(), OptionValue::Bool(true));

    if monitor.vrr {
        opts.insert("adaptive-sync".to_string(), OptionValue::Bool(true));
    }

    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        let monitors_yaml = r#"
monitors:
  main:
    width: 2560
    height: 1440
    refreshRate: 165
    vrr: true
    hdr: true
    primary: true
  tv:
    width: 3840
    height: 2160
    refreshRate: 120
    hdr: true
"#;

        let profiles_yaml = r#"
profiles:
  default:
    useHDR: true
    useWSI: true
    options:
      backend: sdl

  autohdr:
    useWSI: false

  couch:
    monitor: tv
    useHDR: true
    binary: /custom/gamescope

  performance:
    useHDR: false
    options:
      fsr-upscaling: true
"#;

        let monitors: MonitorsConfig = serde_yaml::from_str(monitors_yaml).unwrap();
        let profiles: ProfilesConfig = serde_yaml::from_str(profiles_yaml).unwrap();
        Config { monitors, profiles }
    }

    #[test]
    fn test_resolve_default_profile() {
        let config = test_config();
        let profile = config.resolve_profile("default").unwrap();

        assert_eq!(profile.name, "default");
        assert_eq!(profile.monitor_name, "main");
        assert!(profile.use_hdr);
        assert!(profile.use_wsi);
        assert_eq!(profile.binary, "gamescope");
    }

    #[test]
    fn test_resolve_with_custom_monitor() {
        let config = test_config();
        let profile = config.resolve_profile("couch").unwrap();

        assert_eq!(profile.monitor_name, "tv");
        assert_eq!(profile.binary, "/custom/gamescope");
        assert!(matches!(
            profile.options.get("output-width"),
            Some(OptionValue::Int(3840))
        ));
    }

    #[test]
    fn test_hdr_defaults_to_monitor() {
        let config = test_config();
        let profile = config.resolve_profile("autohdr").unwrap();
        assert!(profile.use_hdr); // Inherits from monitor.hdr
    }

    #[test]
    fn test_wsi_defaults_to_true() {
        let config = test_config();
        let profile = config.resolve_profile("performance").unwrap();
        assert!(profile.use_wsi);
    }

    #[test]
    fn test_unknown_profile_error() {
        let config = test_config();
        assert!(config.resolve_profile("nonexistent").is_err());
    }

    #[test]
    fn test_list_profiles() {
        let config = test_config();
        let profiles = config.list_profiles();
        assert_eq!(profiles.len(), 4);
    }

    #[test]
    fn test_unset_in_resolved_profile() {
        let profiles_yaml = r#"
profiles:
  with-unset:
    useWSI: true
    environment:
      CUSTOM: "value"
    unset:
      - SDL_VIDEODRIVER
      - CUSTOM
"#;

        let profiles: ProfilesConfig = serde_yaml::from_str(profiles_yaml).unwrap();
        let monitors_yaml = r#"
monitors:
  main:
    width: 2560
    height: 1440
    refreshRate: 165
    primary: true
"#;
        let monitors: MonitorsConfig = serde_yaml::from_str(monitors_yaml).unwrap();
        let config = Config { monitors, profiles };

        let profile = config.resolve_profile("with-unset").unwrap();
        assert_eq!(profile.unset_vars.len(), 2);
        assert!(profile.unset_vars.contains(&"SDL_VIDEODRIVER".to_string()));
        assert!(profile.unset_vars.contains(&"CUSTOM".to_string()));

        // Verify unset works in environment
        let env = profile.environment();
        let env_map: HashMap<_, _> = env.into_iter().collect();
        assert!(!env_map.contains_key("SDL_VIDEODRIVER"));
        assert!(!env_map.contains_key("CUSTOM"));
    }

    // ========================================================================
    // Environment Variable Name Validation Tests
    // ========================================================================

    #[test]
    fn test_valid_env_var_names() {
        // Standard POSIX-compliant names
        assert!(is_valid_env_var_name("MY_VAR"));
        assert!(is_valid_env_var_name("_PRIVATE"));
        assert!(is_valid_env_var_name("var123"));
        assert!(is_valid_env_var_name("A"));
        assert!(is_valid_env_var_name("_"));
        assert!(is_valid_env_var_name("SDL_VIDEODRIVER"));
        assert!(is_valid_env_var_name("DXVK_HDR"));
        assert!(is_valid_env_var_name("PROTON_ENABLE_WAYLAND"));
    }

    #[test]
    fn test_invalid_env_var_names() {
        // Empty string
        assert!(!is_valid_env_var_name(""));
        // Starts with digit
        assert!(!is_valid_env_var_name("123VAR"));
        assert!(!is_valid_env_var_name("1"));
        // Contains equals sign (would break shell parsing)
        assert!(!is_valid_env_var_name("MY=VAR"));
        assert!(!is_valid_env_var_name("VAR="));
        // Contains space
        assert!(!is_valid_env_var_name("MY VAR"));
        assert!(!is_valid_env_var_name(" VAR"));
        // Contains special characters
        assert!(!is_valid_env_var_name("VAR$NAME"));
        assert!(!is_valid_env_var_name("VAR-NAME"));
        assert!(!is_valid_env_var_name("VAR.NAME"));
    }

    #[test]
    fn test_validate_env_var_names_success() {
        let env_keys = vec![
            "VALID_VAR".to_string(),
            "_ANOTHER".to_string(),
            "third123".to_string(),
        ];
        let unset = vec!["SDL_VIDEODRIVER".to_string()];

        let result = validate_env_var_names("test-profile", env_keys.iter(), &unset);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_env_var_names_invalid_env_key() {
        let env_keys = vec!["VALID".to_string(), "INVALID=KEY".to_string()];
        let unset = vec![];

        let result = validate_env_var_names("test-profile", env_keys.iter(), &unset);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("INVALID=KEY"));
        assert!(err.contains("test-profile"));
    }

    #[test]
    fn test_validate_env_var_names_invalid_unset() {
        let env_keys: Vec<String> = vec![];
        let unset = vec!["".to_string()]; // Empty string is invalid

        let result = validate_env_var_names("test-profile", env_keys.iter(), &unset);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unset entry"));
    }

    // ========================================================================
    // Integration Tests for Validation During Config Load
    // ========================================================================
    //
    // Note: Deduplication tests were removed because env_remove() is idempotent.
    // Duplicate entries in config are harmless, so we don't deduplicate them.
    // This follows YAGNI - removing complexity we don't need.

    #[test]
    fn test_config_load_accepts_duplicate_unset() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let monitors_path = dir.path().join("monitors.yaml");
        let profiles_path = dir.path().join("config.yaml");

        std::fs::write(
            &monitors_path,
            r#"
monitors:
  main:
    width: 1920
    height: 1080
    refreshRate: 60
    primary: true
"#,
        )
        .unwrap();

        std::fs::write(
            &profiles_path,
            r#"
profiles:
  test:
    unset:
      - SDL_VIDEODRIVER
      - DXVK_HDR
      - SDL_VIDEODRIVER
"#,
        )
        .unwrap();

        // Config should load successfully even with duplicates
        let config = Config::load(&monitors_path, &profiles_path).unwrap();
        let profile = config.resolve_profile("test").unwrap();

        // Duplicates are preserved (env_remove is idempotent, so this is harmless)
        assert_eq!(profile.unset_vars.len(), 3);
        assert!(profile.unset_vars.contains(&"SDL_VIDEODRIVER".to_string()));
        assert!(profile.unset_vars.contains(&"DXVK_HDR".to_string()));
    }

    #[test]
    fn test_config_load_rejects_invalid_env_name() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let monitors_path = dir.path().join("monitors.yaml");
        let profiles_path = dir.path().join("config.yaml");

        std::fs::write(
            &monitors_path,
            r#"
monitors:
  main:
    width: 1920
    height: 1080
    refreshRate: 60
    primary: true
"#,
        )
        .unwrap();

        std::fs::write(
            &profiles_path,
            r#"
profiles:
  test:
    environment:
      VALID_VAR: "1"
      "INVALID=VAR": "2"
"#,
        )
        .unwrap();

        let result = Config::load(&monitors_path, &profiles_path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid environment variable"));
    }

    #[test]
    fn test_config_load_rejects_invalid_unset_name() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let monitors_path = dir.path().join("monitors.yaml");
        let profiles_path = dir.path().join("config.yaml");

        std::fs::write(
            &monitors_path,
            r#"
monitors:
  main:
    width: 1920
    height: 1080
    refreshRate: 60
    primary: true
"#,
        )
        .unwrap();

        std::fs::write(
            &profiles_path,
            r#"
profiles:
  test:
    unset:
      - ""
"#,
        )
        .unwrap();

        let result = Config::load(&monitors_path, &profiles_path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid environment variable"));
    }
}
