use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "gluttony",
    version,
    about = "Reclaim the disk space your toolchain ate without asking"
)]
pub struct Cli {
    /// List every detected path individually
    #[arg(long)]
    pub list: bool,

    /// Delete artefacts interactively (cherry-pick); add --all to remove everything at once
    #[arg(long)]
    pub clean: bool,

    /// Remove all detected artefacts without cherry-picking (requires --clean); asks twice
    #[arg(long, requires = "clean")]
    pub all: bool,

    /// Preview what would be removed without deleting anything
    #[arg(long)]
    pub dry_run: bool,

    /// Root directory to scan (default: home directory)
    #[arg(long, value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Print shell completions for the given shell and exit
    #[arg(long, value_name = "SHELL")]
    pub completions: Option<clap_complete::Shell>,
}
