use std::{
    io::{self, Write},
    path::Path,
};

use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use crate::display::human_size;
use crate::error::{Error, Result};
use crate::scanner::{Artifact, ArtifactKind};

pub fn clean(artifacts: &[Artifact], dry_run: bool) -> Result<()> {
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

    let has_docker = artifacts.iter().any(|a| a.kind == ArtifactKind::DockerData);

    if has_docker && docker_is_running() {
        println!(
            "{}",
            style("Docker is currently running. Stop it before cleaning its data.").red().bold()
        );
        return Ok(());
    }

    println!(
        "{}",
        style(format!(
            "About to remove {} artefact{} ({} total).",
            artifacts.len(),
            if artifacts.len() == 1 { "" } else { "s" },
            human_size(total),
        ))
        .bold()
    );

    if has_docker {
        println!(
            "{}",
            style("  ⚠  Docker data is included — all images, containers and volumes will be lost.")
                .yellow()
        );
    }

    println!("{}\n", style("This cannot be undone.").red());

    if !confirm("Proceed?")? {
        println!("{}", style("Aborted.").dim());
        return Ok(());
    }

    println!();

    let pb = ProgressBar::new(artifacts.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("  {bar:35.green/dim}  {pos}/{len}  {wide_msg:.dim}")
            .unwrap()
            .progress_chars("█░"),
    );

    let mut freed = 0u64;
    let mut error_count = 0usize;

    for a in artifacts {
        pb.set_message(display_path(&a.path));
        match std::fs::remove_dir_all(&a.path) {
            Ok(()) => {
                freed += a.size;
            }
            Err(e) => {
                pb.println(format!(
                    "  {} {} — {}",
                    style("✗").red(),
                    a.path.display(),
                    style(e.to_string()).dim(),
                ));
                error_count += 1;
            }
        }
        pb.inc(1);
    }

    pb.finish_and_clear();
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

    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{} [y/N] ", style(prompt).bold());
    io::stdout().flush().map_err(Error::Io)?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).map_err(Error::Io)?;

    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn docker_is_running() -> bool {
    #[cfg(unix)]
    {
        std::path::Path::new("/var/run/docker.sock").exists()
            || std::path::Path::new("/run/docker.sock").exists()
    }
    #[cfg(windows)]
    {
        // The Docker named pipe exists only while the daemon is running
        std::fs::metadata(r"\\.\pipe\docker_engine").is_ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

fn display_path(path: &Path) -> String {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);

    let raw = match home {
        Some(h) => match path.strip_prefix(&h) {
            Ok(rel) => format!("~/{}", rel.display()),
            Err(_) => path.display().to_string(),
        },
        None => path.display().to_string(),
    };
    raw.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Artifact, ArtifactKind};
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
        clean(&artifacts, true).unwrap();

        assert!(target.exists(), "dry run must not delete the directory");
    }
}
