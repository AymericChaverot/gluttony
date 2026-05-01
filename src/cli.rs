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
    #[arg(long, conflicts_with = "undo")]
    pub clean: bool,

    /// Remove all detected artefacts without cherry-picking (requires --clean); asks twice
    #[arg(long, requires = "clean")]
    pub all: bool,

    /// Preview what would be removed without deleting anything
    #[arg(long, conflicts_with = "undo")]
    pub dry_run: bool,

    /// Root directory to scan (default: home directory)
    #[arg(long, value_name = "PATH", conflicts_with = "undo")]
    pub path: Option<PathBuf>,

    /// List recent sessions and interactively restore one
    #[arg(long, conflicts_with = "empty_trash")]
    pub undo: bool,

    /// Permanently delete everything in the trash (cannot be undone)
    #[arg(long, conflicts_with = "undo")]
    pub empty_trash: bool,

    /// Print shell completions for the given shell and exit
    #[arg(long, value_name = "SHELL")]
    pub completions: Option<clap_complete::Shell>,
}
