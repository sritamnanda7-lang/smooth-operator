// SPDX-License-Identifier: MIT
// Modified from Handy v0.10.0 (Copyright (c) 2025 CJ Pais): remote-control and headless flags removed.
use clap::Parser;

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "smooth-operator", about = "Smooth Operator")]
pub struct CliArgs {
    /// Start with the main window hidden
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,
}
