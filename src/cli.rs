//! Command-line interface definitions for wayscope.
//!
//! Uses clap's derive macros for declarative argument parsing.
//! The CLI supports three main commands: run (default), list, and show.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Profile-based gamescope wrapper for gaming on Linux.
///
/// Wayscope simplifies running games through gamescope by providing
/// named profiles that define complete configurations for HDR, WSI,
/// and other gamescope settings.
#[derive(Parser)]
#[command(name = "wayscope", version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Path to monitors configuration file
    ///
    /// Defaults to ~/.config/wayscope/monitors.yaml
    #[arg(short, long, global = true)]
    pub monitors: Option<PathBuf>,

    /// Path to profiles configuration file
    ///
    /// Defaults to ~/.config/wayscope/config.yaml
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available subcommands for wayscope.
#[derive(Subcommand)]
pub enum Commands {
    /// Initialize configuration files with examples
    ///
    /// Creates ~/.config/wayscope/ with starter monitors.yaml and
    /// config.yaml files showing all available options.
    #[command(name = "init")]
    Init {
        /// Overwrite existing configuration files
        #[arg(short, long)]
        force: bool,
    },

    /// Run a command through gamescope with the specified profile
    ///
    /// This is the primary command for launching games. The profile
    /// determines HDR, WSI, and other gamescope settings.
    #[command(name = "run")]
    Run(RunArgs),

    /// List all available profiles
    ///
    /// Shows each profile's name, target monitor, and key settings.
    #[command(name = "list", alias = "ls")]
    List,

    /// Show detailed information about a profile
    ///
    /// Displays all resolved settings including options and
    /// environment variables that would be applied.
    #[command(name = "show")]
    Show {
        /// Profile name to inspect
        profile: String,
    },

    /// List available monitors
    ///
    /// Shows configured monitors and their capabilities.
    #[command(name = "monitors")]
    Monitors,
}

/// Arguments for the run subcommand.
#[derive(Parser)]
pub struct RunArgs {
    /// Profile to use
    ///
    /// Selects which configuration profile to apply. Profiles define
    /// HDR, WSI, and gamescope options. Use 'wayscope list' to see
    /// available profiles.
    #[arg(short, long, default_value = "default")]
    pub profile: String,

    /// Command to run inside gamescope
    ///
    /// This is typically a game launcher like 'steam' or 'heroic'.
    /// All arguments after the command are passed through.
    #[arg(required = true, trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        // Test basic run command
        let cli = Cli::try_parse_from(["wayscope", "run", "steam"]).unwrap();
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.profile, "default");
                assert_eq!(args.command, vec!["steam"]);
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_run_with_profile() {
        let cli = Cli::try_parse_from(["wayscope", "run", "-p", "autohdr", "heroic"]).unwrap();
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.profile, "autohdr");
                assert_eq!(args.command, vec!["heroic"]);
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_run_with_command_args() {
        let cli =
            Cli::try_parse_from(["wayscope", "run", "steam", "-gamepadui", "-tenfoot"]).unwrap();
        match cli.command {
            Commands::Run(args) => {
                assert_eq!(args.command, vec!["steam", "-gamepadui", "-tenfoot"]);
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_list_command() {
        let cli = Cli::try_parse_from(["wayscope", "list"]).unwrap();
        assert!(matches!(cli.command, Commands::List));
    }

    #[test]
    fn test_show_command() {
        let cli = Cli::try_parse_from(["wayscope", "show", "autohdr"]).unwrap();
        match cli.command {
            Commands::Show { profile } => assert_eq!(profile, "autohdr"),
            _ => panic!("Expected Show command"),
        }
    }

    #[test]
    fn test_monitors_command() {
        let cli = Cli::try_parse_from(["wayscope", "monitors"]).unwrap();
        assert!(matches!(cli.command, Commands::Monitors));
    }

    #[test]
    fn test_custom_config_paths() {
        let cli = Cli::try_parse_from([
            "wayscope",
            "-m",
            "/custom/monitors.yaml",
            "-c",
            "/custom/config.yaml",
            "list",
        ])
        .unwrap();
        assert_eq!(cli.monitors, Some(PathBuf::from("/custom/monitors.yaml")));
        assert_eq!(cli.config, Some(PathBuf::from("/custom/config.yaml")));
    }

    #[test]
    fn test_init_command() {
        let cli = Cli::try_parse_from(["wayscope", "init"]).unwrap();
        match cli.command {
            Commands::Init { force } => assert!(!force),
            _ => panic!("Expected Init command"),
        }
    }

    #[test]
    fn test_init_command_force() {
        let cli = Cli::try_parse_from(["wayscope", "init", "--force"]).unwrap();
        match cli.command {
            Commands::Init { force } => assert!(force),
            _ => panic!("Expected Init command"),
        }
    }
}
