//! Colored terminal output helpers.

use owo_colors::OwoColorize;

use crate::command::GamescopeCommand;

const PREFIX: &str = "[wayscope]";

pub fn profile(name: &str, monitor: &str) {
    println!(
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

pub fn environment(env: &[(String, String)]) {
    println!("{} Environment:", PREFIX.cyan().bold());
    for (key, value) in env {
        println!("    {}={}", key.yellow(), value);
    }
}

pub fn exec_line(cmd: &GamescopeCommand) {
    if cmd.needs_workaround {
        println!(
            "{} HDR workaround: {} for child",
            PREFIX.magenta().bold(),
            "DISABLE_HDR_WSI=1".yellow()
        );
    }
    if !cmd.hoisted_env.is_empty() {
        // Name the vars only — values can contain secrets/paths and always
        // appear on the Exec: line below anyway.
        let names: Vec<&str> = cmd
            .hoisted_env
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        println!(
            "{} Hoisting parent env to child (stripping from gamescope): {}",
            PREFIX.magenta().bold(),
            names.join(", ").yellow()
        );
    }
    println!("{} Exec: {}", PREFIX.cyan().bold(), cmd.display().dimmed());
}

pub fn profile_summary(name: &str, summary: &str) {
    println!("  {}: {}", name.green(), summary.dimmed());
}

pub fn warn(msg: &str) {
    println!("{} {}", PREFIX.yellow().bold(), msg);
}

pub fn success(msg: &str) {
    println!("{} {}", PREFIX.green().bold(), msg);
}

pub fn info(msg: &str) {
    println!("{}", msg.dimmed());
}
