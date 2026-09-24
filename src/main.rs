mod cleaner;
mod cli;
mod display;
mod error;
mod picker;
mod scanner;
mod trash;
mod ui;
mod update;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use cli::{Cli, Command, FilterArgs, ScanArgs, TrashAction};
use error::{Error, Result};
use scanner::ScanReport;

fn main() -> ExitCode {
    // Restore the cursor if the user interrupts a prompt or a progress bar.
    let _ = ctrlc::set_handler(|| {
        let _ = console::Term::stderr().show_cursor();
        eprintln!();
        std::process::exit(130);
    });

    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            ui::error(&e.to_string());
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command.unwrap_or(Command::Scan(cli.scan)) {
        Command::Scan(args) => scan(&args),
        Command::Clean(args) => clean(&args),
        Command::Undo => trash::undo_interactive(),
        Command::Trash { action: None } => trash::show(),
        Command::Trash {
            action: Some(TrashAction::Empty { yes }),
        } => trash::empty_trash(yes),
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "gluttony",
                &mut std::io::stdout(),
            );
            Ok(())
        }
    }
}

fn resolve_root(path: Option<&PathBuf>) -> Result<PathBuf> {
    let root = match path {
        Some(p) => p.clone(),
        None => {
            ui::home_dir().ok_or_else(|| Error::Usage("cannot find your home directory".into()))?
        }
    };
    if !root.is_dir() {
        return Err(Error::Usage(format!(
            "`{}` is not a directory",
            root.display()
        )));
    }
    Ok(std::path::absolute(&root)?)
}

fn run_scan(filters: &FilterArgs) -> Result<(ScanReport, trash::TrashStats)> {
    let purged = trash::auto_purge();
    let root = resolve_root(filters.path.as_ref())?;
    let report = scanner::scan(&root, &filters.filter())?;
    let mut stats = trash::stats();
    // Mention what the automatic purge just freed.
    if purged.sessions > 0 {
        stats.purged = purged.size;
    }
    Ok((report, stats))
}

fn scan(args: &ScanArgs) -> Result<()> {
    let check = (!args.json).then(update::spawn).flatten();
    let (report, stats) = run_scan(&args.filters)?;

    if args.json {
        return display::print_json(&report, &stats);
    }

    display::print_header(&report);
    display::print_summary(&report, &stats, args.filters.filter().is_active());
    if args.projects {
        display::print_projects(&report.artifacts);
    }
    if args.list {
        display::print_list(&report.artifacts);
    }
    display::print_hints(&report, args.list, args.projects);

    if let Some(check) = check {
        check.finish();
    }
    Ok(())
}

fn clean(args: &cli::CleanArgs) -> Result<()> {
    let check = update::spawn();
    let (report, stats) = run_scan(&args.filters)?;

    display::print_header(&report);
    display::print_summary(&report, &stats, args.filters.filter().is_active());

    cleaner::clean(
        &report,
        cleaner::CleanOptions {
            all: args.all,
            dry_run: args.dry_run,
            permanent: args.permanent,
            yes: args.yes,
        },
    )?;

    if let Some(check) = check {
        check.finish();
    }
    Ok(())
}
