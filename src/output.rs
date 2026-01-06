//! Terminal output formatting with colors.
//!
//! Provides consistent, colored output for profile information,
//! environment variables, and execution status.

use owo_colors::OwoColorize;

use crate::command::GamescopeCommand;

const PREFIX: &str = "[wayscope]";

/// Display the active profile name and monitor.
pub fn profile(name: &str, monitor: &str) {
    println!(
        "{} Profile: {} (monitor: {})",
        PREFIX.cyan().bold(),
        name.green().bold(),
        monitor.blue()
    );
}

/// Display a section header.
pub fn header(text: &str) {
    println!("{}", text.bold());
}

/// Display a section label.
pub fn section(text: &str) {
    println!("{}", text.cyan());
}

/// Display a key-value pair.
pub fn key_value(key: &str, value: &str) {
    println!("{}={}", key.yellow(), value);
}

/// Display environment variables.
pub fn environment(env: &[(String, String)]) {
    println!("{} Environment:", PREFIX.cyan().bold());
    for (key, value) in env {
        println!("    {}={}", key.yellow(), value);
    }
}

/// Display the command that will be executed.
pub fn exec_line(cmd: &GamescopeCommand) {
    if cmd.needs_workaround {
        println!(
            "{} HDR workaround: {} for child",
            PREFIX.magenta().bold(),
            "DISABLE_HDR_WSI=1".yellow()
        );
    }
    println!("{} Exec: {}", PREFIX.cyan().bold(), cmd.display().dimmed());
}

/// Display a profile summary in the list.
pub fn profile_summary(name: &str, summary: &str) {
    println!("  {}: {}", name.green(), summary.dimmed());
}

/// Display a warning message.
pub fn warn(msg: &str) {
    println!("{} {}", PREFIX.yellow().bold(), msg);
}

/// Display a success message.
pub fn success(msg: &str) {
    println!("{} {}", PREFIX.green().bold(), msg);
}

/// Display an info message.
pub fn info(msg: &str) {
    println!("{}", msg.dimmed());
}
