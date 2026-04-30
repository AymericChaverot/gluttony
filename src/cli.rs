use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "gluttony",
    version,
    about = "Reclaim the disk space your toolchain ate without asking"
)]
pub struct Cli {
    /// Delete artefacts after confirmation
    #[arg(long)]
    pub clean: bool,

    /// Show what --clean would remove without deleting anything
    #[arg(long, requires = "clean")]
    pub dry_run: bool,

    /// Root directory to scan (default: home directory)
    #[arg(long, value_name = "PATH")]
    pub path: Option<PathBuf>,
}
