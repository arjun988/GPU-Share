//! Terminal UI helpers — Claude/Gemini-style polished CLI output.

use owo_colors::OwoColorize;
use std::io::{self, Write};

pub fn styles() -> clap::builder::Styles {
    use clap::builder::styling::{AnsiColor, Effects, Styles};
    Styles::styled()
        .header(AnsiColor::BrightCyan.on_default().effects(Effects::BOLD))
        .usage(AnsiColor::BrightCyan.on_default().effects(Effects::BOLD))
        .literal(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
        .placeholder(AnsiColor::BrightBlack.on_default())
        .error(AnsiColor::BrightRed.on_default().effects(Effects::BOLD))
        .valid(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
        .invalid(AnsiColor::BrightRed.on_default().effects(Effects::BOLD))
}

pub fn print_banner() {
    let v = gpumesh_common::VERSION;
    println!("{}", "GPUMesh".bright_cyan().bold());
    println!(
        "{}",
        format!("  Turn idle GPUs into a personal compute network.  v{v}").bright_black()
    );
    println!();
}

pub fn ok(msg: impl AsRef<str>) {
    println!("{} {}", "✔".bright_green().bold(), msg.as_ref());
}

pub fn warn(msg: impl AsRef<str>) {
    println!("{} {}", "!".bright_yellow().bold(), msg.as_ref());
}

pub fn err(msg: impl AsRef<str>) {
    eprintln!("{} {}", "✖".bright_red().bold(), msg.as_ref());
}

pub fn info(msg: impl AsRef<str>) {
    println!("{} {}", "→".bright_cyan().bold(), msg.as_ref());
}

pub fn dim(msg: impl AsRef<str>) {
    println!("{}", msg.as_ref().bright_black());
}

pub fn section(title: impl AsRef<str>) {
    println!();
    println!("{}", title.as_ref().bright_cyan().bold());
}

pub fn kv(key: &str, value: impl AsRef<str>) {
    println!("  {:<14} {}", format!("{key}:").bright_black(), value.as_ref());
}

pub fn check_line(name: &str, passed: bool, detail: &str) {
    if passed {
        println!(
            "  {} {:<18} {}",
            "PASS".bright_green().bold(),
            name,
            detail.bright_black()
        );
    } else {
        println!(
            "  {} {:<18} {}",
            "FAIL".bright_red().bold(),
            name,
            detail.bright_black()
        );
    }
}

pub fn spinner(msg: &str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(
        indicatif::ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

pub fn flush() {
    let _ = io::stdout().flush();
}

pub const AFTER_HELP: &str = "\
Examples:
  gpumesh start
  gpumesh init --name my-pc
  gpumesh share
  gpumesh pair <code>
  gpumesh group create research
  gpumesh run --group research --gpu-memory 20GB python train.py
  gpumesh run --file job.yaml
  gpumesh sync
  gpumesh doctor

Environment:
  GPUMESH_HOME     Config directory override
  GPUMESH_PEER     Default peer for run/cp
  GPUMESH_IMAGE    Default container image
  GPUMESH_LOG      tracing filter (e.g. gpumesh=info)

Docs: https://github.com/gpumesh/gpumesh
";
