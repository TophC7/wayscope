//! wayscope - Profile-based gamescope wrapper for gaming on Linux.
//!
//! Provides a declarative configuration system for running games through
//! gamescope with proper HDR, WSI, and VRR settings. Profiles define
//! complete, tested configurations that users can select at runtime.

use anyhow::{Context, Result};
use clap::Parser;

use crate::cli::{Cli, Commands};
use crate::config::{Config, MonitorsConfig};
use crate::profile::{LaunchMode, GAMESCOPE_DISPLAY_VAR};

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
    if std::env::var(GAMESCOPE_DISPLAY_VAR).is_ok() {
        output::warn("Already inside Gamescope, running command directly...");
        return command::exec_direct_with_env(&args.command, &[], &[]);
    }

    let config = load_config(cli)?;
    let profile = config
        .resolve_profile(&args.profile)
        .with_context(|| format!("Failed to resolve profile '{}'", args.profile))?;

    output::profile(&profile.name, &profile.monitor_name);

    let mode = if args.skip_gamescope {
        LaunchMode::Direct
    } else {
        LaunchMode::Gamescope
    };
    let env = profile.resolve_environment(mode);
    output::environment(&env.set);

    if args.skip_gamescope {
        output::warn("Skipping gamescope, running command directly with profile environment...");
        return command::exec_direct_with_env(&args.command, &env.set, &env.unset);
    }

    let cmd = command::build(&profile, env, &args.command);
    output::exec_line(&cmd);

    command::exec(cmd)
}

fn list_profiles(cli: &Cli) -> Result<()> {
    let config = load_config(cli)?;

    output::header("Available profiles:");
    for (name, resolved) in config.list_profiles() {
        match resolved {
            Ok(summary) => output::profile_summary(name, &summary),
            Err(err) => output::profile_unresolved(name, &err),
        }
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
    for (key, value) in profile.sorted_options() {
        output::key_value(&format!("  --{}", key), &value.to_string());
    }

    // Report what `run` would actually apply, hoist/strip included.
    let env = profile.resolve_environment(LaunchMode::Gamescope);

    output::section("Environment:");
    output::environment_listing(&env.set);

    if !env.hoisted.is_empty() {
        output::section("Hoisted To Child (stripped from gamescope):");
        output::environment_listing(&env.hoisted);
    }

    if !env.unset.is_empty() {
        output::section("Unset Variables:");
        let mut unset = env.unset;
        unset.sort();
        for var in unset {
            output::key_value("  -", &var);
        }
    }

    Ok(())
}

fn list_monitors(cli: &Cli) -> Result<()> {
    let monitors = MonitorsConfig::load(&cli.monitors_path())?;

    output::header("Configured monitors:");

    let mut entries: Vec<_> = monitors.monitors.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    for (name, mon) in entries {
        output::monitor_summary(name, mon);
    }
    Ok(())
}

fn load_config(cli: &Cli) -> Result<Config> {
    let monitors_path = cli.monitors_path();
    let profiles_path = cli.profiles_path();

    Config::load(&monitors_path, &profiles_path).with_context(|| {
        format!(
            "Failed to load config from {} and {}",
            monitors_path.display(),
            profiles_path.display()
        )
    })
}
