//! Configuration file initialization.
//!
//! Creates starter configuration files with all available options
//! documented so users can see everything they can configure.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::MonitorsConfig;
use crate::output;

/// Default monitors.yaml content with all fields documented.
const DEFAULT_MONITORS: &str = r#"# Wayscope Monitor Configuration
#
# Define your displays here with their hardware capabilities.
# One monitor must be marked as primary.
#
# Note: These values represent what your monitor CAN do.
# Profiles decide what to actually ENABLE (and can use less than full capability).
#
# Field names match mix.nix monitor format for compatibility.

monitors:
  # Primary gaming monitor
  main:
    width: 1920           # Native horizontal resolution
    height: 1080          # Native vertical resolution
    refreshRate: 60       # Refresh rate in Hz
    vrr: false            # Hardware supports VRR (FreeSync/G-Sync)?
    hdr: false            # Hardware supports HDR?
    primary: true         # Use this monitor when profile doesn't specify one

  # Example: Secondary monitor (TV for couch gaming)
  # tv:
  #   width: 3840
  #   height: 2160
  #   refreshRate: 120
  #   vrr: false
  #   hdr: true
  #   primary: false

  # Example: Portable/laptop display
  # portable:
  #   width: 1920
  #   height: 1080
  #   refreshRate: 60
  #   vrr: false
  #   hdr: false
  #   primary: false
"#;

/// Default config.yaml content with all fields documented.
const DEFAULT_CONFIG: &str = r#"# Wayscope Profile Configuration
#
# Define gaming profiles here. Each profile specifies gamescope settings.
#
# Precedence (highest to lowest):
#   1. Profile options (useHDR, adaptive-sync in options, etc.)
#   2. Monitor capabilities (hdr, vrr from monitors.yaml)
#
# This means you can have an HDR-capable monitor but disable HDR per-profile.

profiles:
  # Default profile - used when no profile is specified
  default:
    # monitor: main        # Which monitor to use (omit to use default monitor)
    # binary: gamescope    # Path to gamescope binary (default: gamescope)

    # HDR/WSI settings
    # If omitted, useHDR defaults to monitor's hdr capability
    # useHDR: true         # Enable HDR output (overrides monitor.hdr)
    useWSI: true           # Enable Gamescope WSI layer

    # Gamescope command-line options
    # These override the defaults derived from your monitor config
    options:
      # backend: sdl               # Display backend (sdl, wayland, drm)
      # fullscreen: true           # Run in fullscreen mode
      # borderless: false          # Borderless window mode
      # grab: false                # Grab keyboard/mouse
      # force-grab-cursor: false   # Force cursor grab

      # Resolution and scaling
      # output-width: 2560         # Output width (auto from monitor)
      # output-height: 1440        # Output height (auto from monitor)
      # nested-width: 1920         # Internal render width
      # nested-height: 1080        # Internal render height
      # nested-refresh: 165        # Internal refresh rate (auto from monitor)

      # Upscaling options
      # filter: linear             # Upscale filter (linear, nearest, fsr, nis)
      # fsr-sharpness: 2           # FSR sharpness (0-20, 0=max sharp)
      # nis-sharpness: 10          # NIS sharpness (0-20)

      # Performance options
      # immediate-flips: true      # Reduce latency with immediate flips
      # rt: true                   # Use realtime scheduling
      # adaptive-sync: true        # Enable VRR (auto from monitor.vrr, set false to disable)

      # Visual options
      # fade-out-duration: 200     # Fade duration in ms when losing focus

    # Environment variables passed to games
    # These are in addition to wayscope's default environment
    environment:
      # MANGOHUD: 1                # Enable MangoHud overlay
      # DXVK_ASYNC: 1              # Enable DXVK async shader compilation
      # PROTON_USE_WINED3D: 1      # Use WineD3D instead of DXVK

  # Example: HDR gaming profile (for games with native HDR support)
  # hdr:
  #   useHDR: true
  #   useWSI: true
  #   options:
  #     backend: wayland
  
  # Example: Auto-HDR profile (forced tone mapping for non-HDR games)
  # auto-hdr:
  #   useHDR: true
  #   useWSI: false
  #   options:
  #     backend: sdl
  #     hdr-itm-enabled: true
  
  # Example: Performance profile with FSR upscaling
  # performance:
  #   useHDR: false
  #   useWSI: true
  #   options:
  #     nested-width: 1920
  #     nested-height: 1080
  #     filter: fsr
  #     fsr-sharpness: 5

  # Example: Couch gaming on TV
  # couch:
  #   monitor: tv
  #   useHDR: true
  #   useWSI: true
  #   options:
  #     backend: sdl

  # Example: Using a custom gamescope binary (e.g., from Nix)
  # nix-hdr:
  #   binary: /nix/store/xxx-gamescope/bin/gamescope
  #   useHDR: true
  #   useWSI: true
"#;

/// Initialize configuration directory with starter files.
///
/// Creates ~/.config/wayscope/ with monitors.yaml and config.yaml
/// containing documented examples of all available options.
///
/// # Arguments
///
/// * `force` - If true, overwrite existing files. If false, skip existing files.
///
/// # Errors
///
/// Returns an error if:
/// - The config directory cannot be created
/// - Files cannot be written
/// - Files exist and force is false
pub fn run(force: bool) -> Result<()> {
    let config_dir = MonitorsConfig::config_dir();
    let monitors_path = config_dir.join("monitors.yaml");
    let profiles_path = config_dir.join("config.yaml");

    // Create config directory if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)
            .with_context(|| format!("Failed to create directory: {}", config_dir.display()))?;
        output::success(&format!("Created {}", config_dir.display()));
    }

    // Write monitors.yaml
    write_config_file(&monitors_path, DEFAULT_MONITORS, force)?;

    // Write config.yaml
    write_config_file(&profiles_path, DEFAULT_CONFIG, force)?;

    output::section("\nConfiguration initialized! Next steps:");
    output::info("  1. Edit monitors.yaml to match your display(s)");
    output::info("  2. Edit config.yaml to create your profiles");
    output::info("  3. Run: wayscope run -- <your-game-command>");

    Ok(())
}

/// Write a configuration file, respecting the force flag.
fn write_config_file(path: &Path, content: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        output::warn(&format!(
            "Skipped {} (already exists, use --force to overwrite)",
            path.display()
        ));
        return Ok(());
    }

    if path.exists() && force {
        // Check if current content differs before overwriting
        let existing = fs::read_to_string(path).unwrap_or_default();
        if existing == content {
            output::info(&format!("Unchanged {}", path.display()));
            return Ok(());
        }
    }

    fs::write(path, content)
        .with_context(|| format!("Failed to write: {}", path.display()))?;

    if force && path.exists() {
        output::success(&format!("Overwrote {}", path.display()));
    } else {
        output::success(&format!("Created {}", path.display()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_monitors_is_valid_yaml() {
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(DEFAULT_MONITORS);
        assert!(result.is_ok(), "DEFAULT_MONITORS is not valid YAML");
    }

    #[test]
    fn test_default_config_is_valid_yaml() {
        let result: Result<serde_yaml::Value, _> = serde_yaml::from_str(DEFAULT_CONFIG);
        assert!(result.is_ok(), "DEFAULT_CONFIG is not valid YAML");
    }

    #[test]
    fn test_write_config_file_creates_new() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");

        write_config_file(&path, "test: content", false).unwrap();

        assert!(path.exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), "test: content");
    }

    #[test]
    fn test_write_config_file_skips_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");

        fs::write(&path, "original").unwrap();
        write_config_file(&path, "new content", false).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    }

    #[test]
    fn test_write_config_file_force_overwrites() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.yaml");

        fs::write(&path, "original").unwrap();
        write_config_file(&path, "new content", true).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
    }
}
