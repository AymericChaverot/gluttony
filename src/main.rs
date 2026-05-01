mod cleaner;
mod cli;
mod display;
mod error;
mod scanner;
mod trash;
mod update;

use clap::CommandFactory;
use clap::Parser;
use cli::Cli;
use error::Result;

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(shell) = cli.completions {
        clap_complete::generate(shell, &mut Cli::command(), "gluttony", &mut std::io::stdout());
        return Ok(());
    }

    if cli.undo {
        return trash::undo_interactive();
    }

    if cli.empty_trash {
        return trash::empty_trash();
    }

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

    if cli.list {
        display::print_list(&artifacts);
    }

    if cli.clean {
        cleaner::clean(&artifacts, cli.dry_run, cli.all)?;
    } else if cli.dry_run {
        cleaner::clean(&artifacts, true, true)?;
    } else if !cli.list {
        display::print_no_clean_hint();
    }

    Ok(())
}
