//! Colored terminal output helpers.
//!
//! Stream split: run-path diagnostics (profile banner, environment dump, exec
//! line, warnings) go to stderr so stdout stays clean for the command wayscope
//! wraps. Query commands (list/show/monitors), whose output *is* the product,
//! print to stdout.

use owo_colors::OwoColorize;

use crate::command::GamescopeCommand;
use crate::config::{MonitorDef, ProfileSummary};

const PREFIX: &str = "[wayscope]";

pub fn profile(name: &str, monitor: &str) {
    eprintln!(
        "{} Profile: {} (monitor: {})",
        PREFIX.cyan().bold(),
        name.green().bold(),
        monitor.blue()
    );
}

pub fn header(text: &str) {
    println!("{}", text.bold());
}

pub fn section(text: &str) {
    println!("{}", text.cyan());
}

pub fn key_value(key: &str, value: &str) {
    println!("{}={}", key.yellow(), value);
}

/// Run-path environment banner (stderr).
pub fn environment(env: &[(String, String)]) {
    eprintln!("{} Environment:", PREFIX.cyan().bold());
    for (key, value) in env {
        eprintln!("    {}={}", key.yellow(), value);
    }
}

/// Environment listing under a `show` section header (stdout).
pub fn environment_listing(env: &[(String, String)]) {
    for (key, value) in env {
        key_value(&format!("  {}", key), value);
    }
}

pub fn exec_line(cmd: &GamescopeCommand) {
    if cmd.needs_hdr_workaround() {
        eprintln!(
            "{} HDR workaround: {} for child",
            PREFIX.magenta().bold(),
            "DISABLE_HDR_WSI=1".yellow()
        );
    }
    if !cmd.hoisted_env_names().is_empty() {
        eprintln!(
            "{} Hoisting parent env to child (stripping from gamescope): {}",
            PREFIX.magenta().bold(),
            cmd.hoisted_env_names().join(", ").yellow()
        );
    }
    eprintln!("{} Exec: {}", PREFIX.cyan().bold(), cmd.display().dimmed());
}

pub fn profile_summary(name: &str, summary: &ProfileSummary) {
    let detail = format!(
        "monitor={} HDR={} WSI={}",
        summary.monitor, summary.use_hdr, summary.use_wsi
    );
    println!("  {}: {}", name.green(), detail.dimmed());
}

/// A profile that exists in config but cannot be resolved (stderr, so a piped
/// listing stays machine-readable while the user still sees the reason).
pub fn profile_unresolved(name: &str, err: &anyhow::Error) {
    eprintln!(
        "  {}: {}",
        name.green(),
        format!("unresolved: {}", err).red()
    );
}

pub fn monitor_summary(name: &str, mon: &MonitorDef) {
    let primary_marker = if mon.primary { " (primary)" } else { "" };
    let detail = format!(
        "{}x{}@{}Hz VRR={} HDR={}{}",
        mon.width, mon.height, mon.refreshRate, mon.vrr, mon.hdr, primary_marker
    );
    println!("  {}: {}", name.green(), detail.dimmed());
}

pub fn warn(msg: &str) {
    eprintln!("{} {}", PREFIX.yellow().bold(), msg);
}

pub fn success(msg: &str) {
    println!("{} {}", PREFIX.green().bold(), msg);
}

pub fn info(msg: &str) {
    println!("{}", msg.dimmed());
}
