//! wayscope - Profile-based gamescope wrapper for gaming on Linux.
//!
//! Provides a declarative configuration system for running games through
//! gamescope with proper HDR, WSI, and VRR settings. Profiles define
//! complete, tested configurations that users can select at runtime.

use anyhow::{Context, Result};
use clap::Parser;

use crate::cli::{Cli, Commands};
use crate::config::{Config, MonitorsConfig, ProfilesConfig};

mod cli;
mod command;
mod config;
mod init;
mod output;
mod profile;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init { force } => init::run(*force),
        Commands::Run(args) => run_gamescope(&cli, args),
        Commands::List => list_profiles(&cli),
        Commands::Show { profile } => show_profile(&cli, profile),
        Commands::Monitors => list_monitors(&cli),
    }
}

fn run_gamescope(cli: &Cli, args: &cli::RunArgs) -> Result<()> {
    if std::env::var("GAMESCOPE_WAYLAND_DISPLAY").is_ok() {
        output::warn("Already inside Gamescope, running command directly...");
        return command::exec_direct(&args.command);
    }

    let config = load_config(cli)?;
    let profile = config
        .resolve_profile(&args.profile)
        .with_context(|| format!("Failed to resolve profile '{}'", args.profile))?;

    output::profile(&profile.name, &profile.monitor_name);
    let env = profile.environment();
    output::environment(&env);

    if args.skip_gamescope {
        output::warn("Skipping gamescope, running command directly with profile environment...");
        return command::exec_direct_with_env(&args.command, &env, &profile.unset_vars);
    }

    let cmd = command::build(&profile, &args.command);
    output::exec_line(&cmd);

    command::exec(cmd)
}

fn list_profiles(cli: &Cli) -> Result<()> {
    let config = load_config(cli)?;

    output::header("Available profiles:");
    for (name, summary) in config.list_profiles() {
        output::profile_summary(&name, &summary);
    }
    Ok(())
}

fn show_profile(cli: &Cli, profile_name: &str) -> Result<()> {
    let config = load_config(cli)?;
    let profile = config
        .resolve_profile(profile_name)
        .with_context(|| format!("Failed to resolve profile '{}'", profile_name))?;

    output::header(&format!("Profile: {}", profile.name));
    output::section("Settings:");
    output::key_value("  Monitor", &profile.monitor_name);
    output::key_value("  Binary", &profile.binary);
    output::key_value("  HDR", &profile.use_hdr.to_string());
    output::key_value("  WSI", &profile.use_wsi.to_string());

    output::section("Options:");
    let mut opts: Vec<_> = profile.options.iter().collect();
    opts.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in opts {
        output::key_value(&format!("  --{}", key), &value.to_string());
    }

    output::section("Environment:");
    for (key, value) in profile.environment() {
        output::key_value(&format!("  {}", key), &value);
    }

    if !profile.unset_vars.is_empty() {
        output::section("Unset Variables:");
        let mut unset = profile.unset_vars.clone();
        unset.sort();
        for var in unset {
            output::key_value("  -", &var);
        }
    }

    Ok(())
}

fn list_monitors(cli: &Cli) -> Result<()> {
    let path = cli
        .monitors
        .as_ref()
        .cloned()
        .unwrap_or_else(MonitorsConfig::default_path);
    let monitors = MonitorsConfig::load(&path)?;

    output::header("Configured monitors:");

    let mut names: Vec<_> = monitors.monitors.keys().collect();
    names.sort();

    for name in names {
        if let Some(mon) = monitors.monitors.get(name) {
            let primary_marker = if mon.primary { " (primary)" } else { "" };
            let summary = format!(
                "{}x{}@{}Hz VRR={} HDR={}{}",
                mon.width, mon.height, mon.refreshRate, mon.vrr, mon.hdr, primary_marker
            );
            output::profile_summary(name, &summary);
        }
    }
    Ok(())
}

fn load_config(cli: &Cli) -> Result<Config> {
    let monitors_path = cli
        .monitors
        .as_ref()
        .cloned()
        .unwrap_or_else(MonitorsConfig::default_path);
    let profiles_path = cli
        .config
        .as_ref()
        .cloned()
        .unwrap_or_else(ProfilesConfig::default_path);

    Config::load(&monitors_path, &profiles_path).with_context(|| {
        format!(
            "Failed to load config from {} and {}",
            monitors_path.display(),
            profiles_path.display()
        )
    })
}
