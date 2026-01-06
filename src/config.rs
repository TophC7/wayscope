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

/// Parse YAML content with user-friendly error messages.
///
/// YAML is notoriously sensitive to indentation and spacing.
/// This function wraps serde_yaml parsing with helpful hints
/// when syntax errors occur.
fn parse_yaml<T: DeserializeOwned>(content: &str, path: &Path) -> Result<T> {
    serde_yaml::from_str(content).map_err(|e| {
        let mut msg = format!("Failed to parse {}\n", path.display());

        // Include location info if available
        if let Some(location) = e.location() {
            msg.push_str(&format!(
                "  Error at line {}, column {}\n",
                location.line(),
                location.column()
            ));
        }

        // Include the actual error
        msg.push_str(&format!("  {}\n\n", e));

        // Add helpful hints for common YAML issues
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

/// Root structure for monitors.yaml.
#[derive(Debug, Deserialize)]
pub struct MonitorsConfig {
    #[serde(default)]
    pub monitors: HashMap<String, MonitorDef>,
}

/// A single monitor definition with resolution and capabilities.
#[derive(Debug, Clone, Deserialize)]
pub struct MonitorDef {
    pub width: u32,
    pub height: u32,
    pub refresh: u32,
    #[serde(default)]
    pub vrr: bool,
    #[serde(default)]
    pub hdr: bool,
    #[serde(default)]
    pub default: bool,
}

impl MonitorsConfig {
    /// Returns the default configuration directory path.
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("wayscope")
    }

    /// Returns the default monitors file path.
    pub fn default_path() -> PathBuf {
        Self::config_dir().join("monitors.yaml")
    }

    /// Load monitors configuration from a YAML file.
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
            .find(|(_, m)| m.default)
            .with_context(|| "No default monitor. Set 'default: true' on one monitor.")
    }
}

// ============================================================================
// Profile Configuration
// ============================================================================

/// Root structure for config.yaml.
#[derive(Debug, Deserialize)]
pub struct ProfilesConfig {
    #[serde(default)]
    pub profiles: HashMap<String, ProfileDef>,
}

/// A single profile definition.
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
}

fn default_binary() -> String {
    "gamescope".to_string()
}

impl ProfilesConfig {
    /// Returns the default profiles file path.
    pub fn default_path() -> PathBuf {
        MonitorsConfig::config_dir().join("config.yaml")
    }

    /// Load profiles configuration from a YAML file.
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

/// Gamescope option value (string, integer, or boolean).
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

/// Environment variable value (string or integer).
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

/// Combined configuration from both files with resolution methods.
pub struct Config {
    pub monitors: MonitorsConfig,
    pub profiles: ProfilesConfig,
}

impl Config {
    /// Load configuration from specified paths.
    pub fn load(monitors_path: &Path, profiles_path: &Path) -> Result<Self> {
        let monitors = MonitorsConfig::load(monitors_path)?;
        let profiles = ProfilesConfig::load(profiles_path)?;

        // Validate monitor references
        for (name, profile) in &profiles.profiles {
            if let Some(ref mon_name) = profile.monitor {
                if !monitors.monitors.contains_key(mon_name) {
                    bail!(
                        "Profile '{}' references unknown monitor '{}'",
                        name,
                        mon_name
                    );
                }
            }
        }

        Ok(Self { monitors, profiles })
    }

    /// Resolve a profile by name into a complete configuration.
    pub fn resolve_profile(&self, name: &str) -> Result<ResolvedProfile> {
        let profile = self.profiles.get(name)?;

        // Get the monitor (profile-specified or default)
        let (monitor_name, monitor) = match &profile.monitor {
            Some(n) => (n.clone(), self.monitors.get(n)?),
            None => {
                let (n, m) = self.monitors.default_monitor()?;
                (n.clone(), m)
            }
        };

        // Build base options from monitor config
        let mut options = base_options(monitor);
        for (key, value) in &profile.options {
            options.insert(key.clone(), value.clone());
        }

        // Convert user environment
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
        })
    }

    /// List all profiles with summary information.
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
                    (name.clone(), summary)
                })
            })
            .collect()
    }
}

/// Build base gamescope options from monitor configuration.
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
        OptionValue::Int(i64::from(monitor.refresh)),
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
    refresh: 165
    vrr: true
    hdr: true
    default: true
  tv:
    width: 3840
    height: 2160
    refresh: 120
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
}
