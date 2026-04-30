use console::{style, Style};
use dialoguer::{theme::ColorfulTheme, Confirm, MultiSelect};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::display::{display_path, human_size};
use crate::error::{Error, Result};
use crate::scanner::{Artifact, ArtifactKind};

fn app_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_style: Style::new().bold(),
        active_item_style: Style::new().cyan(),
        inactive_item_style: Style::new(),
        checked_item_prefix: style("  ✓ ".to_string()).green(),
        unchecked_item_prefix: style("  ○ ".to_string()).dim(),
        active_item_prefix: style("› ".to_string()).cyan(),
        inactive_item_prefix: style("  ".to_string()),
        ..ColorfulTheme::default()
    }
}

pub fn clean(artifacts: &[Artifact], dry_run: bool, all: bool) -> Result<()> {
    if artifacts.is_empty() {
        return Ok(());
    }

    let total: u64 = artifacts.iter().map(|a| a.size).sum();

    if dry_run {
        println!(
            "{}\n",
            style("Dry run — nothing will be deleted.").yellow().bold()
        );
        for a in artifacts {
            println!("  {} {}", style("would remove").dim(), a.path.display());
        }
        println!(
            "\n{}",
            style(format!("{} would be freed.", human_size(total))).bold()
        );
        return Ok(());
    }

    let selected: Vec<&Artifact> = if all {
        let refs: Vec<&Artifact> = artifacts.iter().collect();
        print_removal_summary(&refs, total);
        if !docker_ok(&refs) {
            return Ok(());
        }
        if !confirm("Remove all artefacts? (1/2)")? {
            println!("{}", style("Aborted.").dim());
            return Ok(());
        }
        if !confirm("Are you absolutely sure? (2/2)")? {
            println!("{}", style("Aborted.").dim());
            return Ok(());
        }
        refs
    } else {
        let sel = pick_selection(artifacts)?;
        if sel.is_empty() {
            println!("{}", style("Nothing selected.").dim());
            return Ok(());
        }
        let sel_total: u64 = sel.iter().map(|a| a.size).sum();
        if !docker_ok(&sel) {
            return Ok(());
        }
        print_removal_summary(&sel, sel_total);
        if !confirm("Proceed?")? {
            println!("{}", style("Aborted.").dim());
            return Ok(());
        }
        sel
    };

    println!();
    perform_deletion(&selected);

    Ok(())
}

fn print_removal_summary(artifacts: &[&Artifact], total: u64) {
    println!(
        "\n{}",
        style(format!(
            "About to remove {} artefact{} ({} total):",
            artifacts.len(),
            if artifacts.len() == 1 { "" } else { "s" },
            human_size(total),
        ))
        .bold()
    );

    let has_docker = artifacts.iter().any(|a| a.kind == ArtifactKind::DockerData);

    for (label, items) in group_by_kind(artifacts) {
        let kind_total: u64 = items.iter().map(|a| a.size).sum();
        println!(
            "  {}  {} {}  ({})",
            style(format!("{:<18}", label)).cyan(),
            style(format!("{:>4}", items.len())).dim(),
            if items.len() == 1 { "path " } else { "paths" },
            style(human_size(kind_total)).yellow(),
        );
        for a in items.iter().take(3) {
            println!("    {}", style(display_path(&a.path)).dim());
        }
        if items.len() > 3 {
            println!("    {}", style(format!("… and {} more", items.len() - 3)).dim());
        }
    }

    println!();

    if has_docker {
        println!(
            "{}\n",
            style(
                "  ⚠  Docker data is included — all images, containers and volumes will be lost."
            )
            .yellow()
        );
    }

    println!("{}", style("This cannot be undone.").red());
    println!(
        "{}\n",
        style("  Tip: run with --dry-run to preview exact paths without deleting.").dim()
    );
}

fn docker_ok(artifacts: &[&Artifact]) -> bool {
    if artifacts.iter().any(|a| a.kind == ArtifactKind::DockerData) && docker_is_running() {
        println!(
            "{}",
            style("Docker is currently running. Stop it before cleaning its data.")
                .red()
                .bold()
        );
        false
    } else {
        true
    }
}

fn pick_selection(artifacts: &[Artifact]) -> Result<Vec<&Artifact>> {
    let labels: Vec<String> = artifacts
        .iter()
        .map(|a| {
            format!(
                "{:<18}  {:>10}  {}",
                a.kind.label(),
                human_size(a.size),
                display_path(&a.path),
            )
        })
        .collect();

    let maybe = MultiSelect::with_theme(&app_theme())
        .with_prompt("Space to toggle  ·  Enter to confirm  ·  Esc to cancel")
        .items(&labels)
        .interact_opt()
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

    match maybe {
        None => Ok(vec![]),
        Some(indices) if indices.is_empty() => Ok(vec![]),
        Some(indices) => Ok(indices.into_iter().map(|i| &artifacts[i]).collect()),
    }
}

fn perform_deletion(artifacts: &[&Artifact]) {
    let pb = ProgressBar::new(artifacts.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("  {bar:35.green/dim}  {pos}/{len}")
            .unwrap()
            .progress_chars("█░"),
    );

    let results: Vec<(bool, u64)> = artifacts
        .par_iter()
        .map(|&a| match std::fs::remove_dir_all(&a.path) {
            Ok(()) => {
                pb.inc(1);
                (true, a.size)
            }
            Err(e) => {
                pb.println(format!(
                    "  {} {} — {}",
                    style("✗").red(),
                    display_path(&a.path),
                    style(e.to_string()).dim(),
                ));
                pb.inc(1);
                (false, 0u64)
            }
        })
        .collect();

    pb.finish_and_clear();

    let freed: u64 = results.iter().filter(|(ok, _)| *ok).map(|(_, s)| s).sum();
    let error_count: usize = results.iter().filter(|(ok, _)| !ok).count();

    println!();
    if error_count == 0 {
        println!(
            "{}",
            style(format!("Done. Freed {}.", human_size(freed)))
                .bold()
                .green()
        );
    } else {
        println!(
            "{}",
            style(format!(
                "Done. Freed {}. {} item{} could not be removed (permission denied?).",
                human_size(freed),
                error_count,
                if error_count == 1 { "" } else { "s" },
            ))
            .yellow()
        );
    }
}

fn confirm(prompt: &str) -> Result<bool> {
    Confirm::with_theme(&app_theme())
        .with_prompt(prompt)
        .default(false)
        .interact_opt()
        .map(|r| r.unwrap_or(false))
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))
}

fn group_by_kind<'a>(artifacts: &[&'a Artifact]) -> Vec<(&'static str, Vec<&'a Artifact>)> {
    let mut groups: Vec<(&'static str, Vec<&Artifact>)> = Vec::new();
    for a in artifacts {
        match groups.iter_mut().find(|(l, _)| *l == a.kind.label()) {
            Some((_, v)) => v.push(a),
            None => groups.push((a.kind.label(), vec![a])),
        }
    }
    groups
}

fn docker_is_running() -> bool {
    #[cfg(unix)]
    {
        std::path::Path::new("/var/run/docker.sock").exists()
            || std::path::Path::new("/run/docker.sock").exists()
    }
    #[cfg(windows)]
    {
        std::fs::metadata(r"\\.\pipe\docker_engine").is_ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_artifact(path: std::path::PathBuf, size: u64) -> Artifact {
        Artifact {
            path,
            kind: ArtifactKind::NodeModules,
            size,
        }
    }

    #[test]
    fn dry_run_does_not_delete() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("node_modules");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("index.js"), "hello").unwrap();

        let artifacts = vec![make_artifact(target.clone(), 5)];
        clean(&artifacts, true, false).unwrap();

        assert!(target.exists(), "dry run must not delete the directory");
    }

}
