use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use indicatif::ProgressBar;
use walkdir::WalkDir;

use super::{build, elixir, flutter, node, python, ArtifactKind};
use crate::display::fmt_count;

pub fn should_skip_name(name: &str) -> bool {
    if matches!(name, ".git" | ".hg" | ".svn") {
        return true;
    }
    #[cfg(windows)]
    if matches!(name, "WindowsApps" | "MicrosoftEdgeBackups") {
        return true;
    }
    false
}

fn should_skip(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir() && entry.file_name().to_str().is_some_and(should_skip_name)
}

// Directories we never recurse into regardless of classification.
fn is_artefact_boundary(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".gradle"
            | "target"
            | ".next"
            | ".nuxt"
            | ".turbo"
            | ".parcel-cache"
            | ".pytest_cache"
            | ".tox"
            | "_build"
    )
}

/// Check if any ancestor (up to max_depth) contains a .git entry.
pub fn has_git_ancestor(path: &Path, max_depth: usize) -> bool {
    let mut current = path;
    for _ in 0..=max_depth {
        if current.join(".git").exists() {
            return true;
        }
        match current.parent() {
            Some(p) => current = p,
            None => break,
        }
    }
    false
}

fn classify(entry: &walkdir::DirEntry) -> Option<ArtifactKind> {
    if !entry.file_type().is_dir() {
        return None;
    }
    let name = entry.file_name().to_str()?;
    match name {
        "node_modules" | ".next" | ".nuxt" | ".turbo" | ".parcel-cache" => {
            node::classify(name, entry)
        }
        "__pycache__" | ".pytest_cache" | ".venv" | "venv" | ".tox" => {
            python::classify(name, entry)
        }
        ".gradle" => Some(ArtifactKind::GradleCache),
        "target" => build::classify(entry),
        "build" => flutter::classify(entry),
        "_build" => elixir::classify(entry),
        _ => None,
    }
}

pub fn walk_subtree(
    root: &Path,
    checked: &AtomicU64,
    found_count: &AtomicU64,
    spinner: &ProgressBar,
) -> Vec<(PathBuf, ArtifactKind)> {
    let mut found = Vec::new();
    let mut local_n: u64 = 0;
    let mut walker = WalkDir::new(root).follow_links(false).into_iter();
    loop {
        match walker.next() {
            None => break,
            Some(Err(_)) => {
                local_n += 1;
            }
            Some(Ok(entry)) => {
                local_n += 1;
                if local_n.is_multiple_of(500) {
                    let c = checked.fetch_add(500, Ordering::Relaxed) + 500;
                    let f = found_count.load(Ordering::Relaxed);
                    spinner.set_message(format!(
                        "Scanning…  {} entries checked, {} found",
                        fmt_count(c),
                        f,
                    ));
                }
                if should_skip(&entry) {
                    walker.skip_current_dir();
                    continue;
                }
                let name = entry.file_name().to_str().unwrap_or("");
                if let Some(kind) = classify(&entry) {
                    found.push((entry.path().to_owned(), kind));
                    found_count.fetch_add(1, Ordering::Relaxed);
                    walker.skip_current_dir();
                } else if is_artefact_boundary(name) {
                    walker.skip_current_dir();
                }
            }
        }
    }
    let rem = local_n % 500;
    if rem > 0 {
        checked.fetch_add(rem, Ordering::Relaxed);
    }
    found
}

pub fn dir_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}
