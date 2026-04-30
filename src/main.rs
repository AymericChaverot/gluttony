mod cleaner;
mod cli;
mod display;
mod error;
mod scanner;
mod update;

use clap::Parser;
use cli::Cli;
use error::Result;

fn main() -> Result<()> {
    let cli = Cli::parse();

    update::check_and_notify();

    let root = cli
        .path
        .unwrap_or_else(|| scanner::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")));

    let t = std::time::Instant::now();
    let artifacts = scanner::scan(&root)?;
    let elapsed = t.elapsed();

    display::print_results(&artifacts, elapsed);

    if artifacts.is_empty() {
        return Ok(());
    }

    if cli.clean {
        cleaner::clean(&artifacts, cli.dry_run)?;
    } else {
        display::print_no_clean_hint();
    }

    Ok(())
}
