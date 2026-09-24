use std::path::Path;
use std::time::{Duration, Instant};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::display::{self, STALE_AFTER};
use crate::error::Result;
use crate::picker::{self, Row};
use crate::scanner::{Artifact, ScanReport};
use crate::trash;
use crate::ui::{self, DOT, MARGIN, human_size, plural};

#[derive(Debug, Clone, Copy, Default)]
pub struct CleanOptions {
    /// Take everything without the interactive picker.
    pub all: bool,
    /// Only show what would be removed.
    pub dry_run: bool,
    /// Delete for good instead of moving to the trash.
    pub permanent: bool,
    /// Skip confirmation prompts.
    pub yes: bool,
}

pub fn clean(report: &ScanReport, opts: CleanOptions) -> Result<()> {
    let artifacts = &report.artifacts;
    if artifacts.is_empty() {
        return Ok(());
    }

    let selected: Vec<&Artifact> = if opts.all || opts.dry_run {
        artifacts.iter().collect()
    } else {
        match pick(artifacts)? {
            Some(sel) if !sel.is_empty() => sel,
            Some(_) => {
                println!();
                ui::note("nothing selected");
                println!();
                return Ok(());
            }
            None => {
                println!();
                return Ok(());
            }
        }
    };
    let total: u64 = selected.iter().map(|a| a.size).sum();

    println!();
    if opts.dry_run {
        println!(
            "{MARGIN}{}  {}\n",
            style(" DRY RUN ").black().on_yellow().bold(),
            style("nothing will be touched").dim()
        );
        display::print_plan(&selected, true);
        println!(
            "{MARGIN}{} would be reclaimed from {}.\n",
            ui::size_style(total).bold().apply_to(human_size(total)),
            plural(selected.len(), "artefact"),
        );
        return Ok(());
    }

    println!(
        "{MARGIN}{} {} {} {}\n",
        style(if opts.permanent {
            "Deleting"
        } else {
            "Removing"
        })
        .bold(),
        style(plural(selected.len(), "artefact")).bold(),
        style(DOT).dim(),
        ui::size_style(total).bold().apply_to(human_size(total)),
    );
    display::print_plan(&selected, false);

    if opts.permanent {
        println!(
            "{MARGIN}{}\n",
            style(format!(
                " {} Permanent deletion. This cannot be undone. ",
                ui::WARN
            ))
            .white()
            .on_red()
            .bold()
        );
    } else {
        ui::note("moved to the trash, restore any time with `gluttony undo`");
        println!();
    }

    if !opts.yes {
        let confirmed = if opts.permanent {
            ui::confirm_typed("Delete permanently?", "delete")?
        } else {
            ui::confirm(&format!("Move {} to the trash?", human_size(total)))?
        };
        if !confirmed {
            println!();
            ui::note("aborted, nothing was touched");
            println!();
            return Ok(());
        }
    }

    println!();
    let free_before = free_space(&report.root);
    let outcome = if opts.permanent {
        delete_permanently(&selected)
    } else {
        move_to_trash(&selected)?
    };
    print_outcome(&outcome, opts.permanent, &report.root, free_before);
    Ok(())
}

fn pick(artifacts: &[Artifact]) -> Result<Option<Vec<&Artifact>>> {
    let width = picker::row_width();
    let rows: Vec<Row> = artifacts
        .iter()
        .map(|a| Row {
            line: display::artifact_line(a, width),
            search: format!("{} {}", a.kind.label(), ui::display_path(&a.path)),
            size: a.size,
            stale: a.age().is_some_and(|age| age >= STALE_AFTER),
            checked: false,
        })
        .collect();
    let chosen = picker::multi_select("Select artefacts to remove", &rows)?;
    Ok(chosen.map(|idx| idx.into_iter().map(|i| &artifacts[i]).collect()))
}

struct Outcome {
    moved: usize,
    bytes: u64,
    failures: Vec<(String, String)>,
    elapsed: Duration,
}

fn progress(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(&format!(
            "{MARGIN}{{bar:32.cyan/238}} {{decimal_bytes:>8}} / {{decimal_total_bytes}}  {{wide_msg:.dim}}"
        ))
        .expect("valid template")
        .progress_chars("━╸━")
    );
    pb
}

fn run_parallel<F>(artifacts: &[&Artifact], op: F) -> (Vec<Option<usize>>, Outcome)
where
    F: Fn(usize, &Artifact) -> std::io::Result<()> + Sync,
{
    let started = Instant::now();
    let total: u64 = artifacts.iter().map(|a| a.size).sum();
    let pb = progress(total);

    let results: Vec<std::result::Result<usize, (String, String)>> = artifacts
        .par_iter()
        .enumerate()
        .map(|(i, a)| {
            let shown = ui::display_path(&a.path);
            pb.set_message(shown.clone());
            let r = op(i, a).map(|()| i).map_err(|e| (shown, e.to_string()));
            pb.inc(a.size);
            r
        })
        .collect();
    pb.finish_and_clear();

    let mut ok = Vec::with_capacity(results.len());
    let mut failures = Vec::new();
    let mut bytes = 0;
    for r in results {
        match r {
            Ok(i) => {
                bytes += artifacts[i].size;
                ok.push(Some(i));
            }
            Err(f) => {
                failures.push(f);
                ok.push(None);
            }
        }
    }
    let moved = ok.iter().flatten().count();
    (
        ok,
        Outcome {
            moved,
            bytes,
            failures,
            elapsed: started.elapsed(),
        },
    )
}

fn move_to_trash(artifacts: &[&Artifact]) -> Result<Outcome> {
    let (session_id, session_dir) = trash::create_session()?;
    let (done, outcome) = run_parallel(artifacts, |i, a| {
        trash::move_artifact(&a.path, &session_dir, i)
    });
    let entries = done
        .into_iter()
        .flatten()
        .map(|i| trash::TrashEntry {
            original: artifacts[i].path.clone(),
            kind: artifacts[i].kind.label().to_string(),
            size: artifacts[i].size,
            index: i,
        })
        .collect();
    trash::commit_session(&session_id, entries)?;
    Ok(outcome)
}

fn delete_permanently(artifacts: &[&Artifact]) -> Outcome {
    run_parallel(artifacts, |_, a| std::fs::remove_dir_all(&a.path)).1
}

fn free_space(path: &Path) -> Option<u64> {
    fs4::available_space(path).ok()
}

fn print_outcome(o: &Outcome, permanent: bool, root: &Path, free_before: Option<u64>) {
    for (path, reason) in &o.failures {
        ui::failure(path, reason);
    }
    if !o.failures.is_empty() {
        println!();
    }

    let what = format!(
        "{} {DOT} {}",
        plural(o.moved, "artefact"),
        ui::human_duration(o.elapsed)
    );
    if o.moved > 0 {
        let verb = if permanent { "Freed" } else { "Moved" };
        let tail = if permanent { "" } else { " to the trash" };
        println!(
            "{MARGIN}{} {} {}{}  {}",
            style(ui::CHECK).green().bold(),
            style(verb).bold(),
            ui::size_style(o.bytes).bold().apply_to(human_size(o.bytes)),
            style(tail).bold(),
            style(what).dim()
        );
    }
    if !o.failures.is_empty() {
        ui::warn(&format!(
            "{} could not be removed (in use or permission denied?)",
            plural(o.failures.len(), "artefact")
        ));
    }

    if permanent {
        if let (Some(before), Some(after)) = (free_before, free_space(root)) {
            ui::note(&format!(
                "free space {} {} {}",
                human_size(before),
                ui::ARROW,
                human_size(after)
            ));
        }
    } else if o.moved > 0 {
        ui::note("still on disk until the trash is emptied (auto after 30 days)");
        println!();
        ui::hint("gluttony undo", "put it all back");
        ui::hint("gluttony trash empty", "free the space now");
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::ArtifactKind;
    use std::fs;

    fn report(path: std::path::PathBuf, size: u64) -> ScanReport {
        ScanReport {
            root: path.parent().unwrap().to_path_buf(),
            artifacts: vec![Artifact {
                path,
                kind: ArtifactKind::NodeModules,
                size,
                project: None,
                last_active: None,
            }],
            docker: None,
            entries: 0,
            elapsed: Duration::ZERO,
        }
    }

    #[test]
    fn dry_run_does_not_delete() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("node_modules");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("index.js"), "hello").unwrap();

        let opts = CleanOptions {
            dry_run: true,
            ..CleanOptions::default()
        };
        clean(&report(target.clone(), 5), opts).unwrap();
        assert!(target.exists(), "dry run must not delete the directory");
    }

    #[test]
    fn permanent_all_yes_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("node_modules");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("index.js"), "hello").unwrap();

        let opts = CleanOptions {
            all: true,
            permanent: true,
            yes: true,
            ..CleanOptions::default()
        };
        clean(&report(target.clone(), 5), opts).unwrap();
        assert!(!target.exists());
    }
}
