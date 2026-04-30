use std::{
    cmp::Reverse,
    path::{Path, PathBuf},
    time::Duration,
};

use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use walkdir::WalkDir;

use crate::error::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactKind {
    NodeModules,
    GradleCache,
    BuildTarget,
    Pycache,
    PythonVenv,
    CargoRegistry,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    XcodeDerivedData,
    DockerData,
}

impl ArtifactKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NodeModules => "node_modules",
            Self::GradleCache => ".gradle",
            Self::BuildTarget => "target (build)",
            Self::Pycache => "__pycache__",
            Self::PythonVenv => "Python venv",
            Self::CargoRegistry => "Cargo registry",
            Self::XcodeDerivedData => "Xcode DerivedData",
            Self::DockerData => "Docker data",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Artifact {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub size: u64,
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn should_skip(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = match entry.file_name().to_str() {
        Some(n) => n,
        None => return false,
    };
    // VCS internals: tiny and never contain build artefacts
    if matches!(name, ".git" | ".hg" | ".svn") {
        return true;
    }
    // Windows: UWP app sandbox and browser backup dirs cause permission errors
    #[cfg(windows)]
    if matches!(name, "WindowsApps" | "MicrosoftEdgeBackups") {
        return true;
    }
    false
}

fn classify(entry: &walkdir::DirEntry) -> Option<ArtifactKind> {
    if !entry.file_type().is_dir() {
        return None;
    }
    let name = entry.file_name().to_str()?;
    match name {
        "node_modules" => Some(ArtifactKind::NodeModules),
        ".gradle" => Some(ArtifactKind::GradleCache),
        "__pycache__" => Some(ArtifactKind::Pycache),
        ".venv" | "venv" => {
            // Avoid false positives: a real venv always has pyvenv.cfg
            if entry.path().join("pyvenv.cfg").exists() {
                Some(ArtifactKind::PythonVenv)
            } else {
                None
            }
        }
        "target" => {
            let parent = entry.path().parent()?;
            if parent.join("Cargo.toml").exists() || parent.join("pom.xml").exists() {
                Some(ArtifactKind::BuildTarget)
            } else {
                None
            }
        }
        _ => None,
    }
}

// Docker stores everything inside a VM disk / VHD — not discoverable by walking the tree.
// We locate the platform-specific data directory directly.
fn docker_data_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        let p = local.join("Docker").join("wsl");
        if p.exists() {
            paths.push(p);
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = home_dir() {
        let p = home.join("Library/Containers/com.docker.docker/Data/vms");
        if p.exists() {
            paths.push(p);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let system = PathBuf::from("/var/lib/docker");
        if system.exists() {
            paths.push(system);
        }
        // Rootless Docker (user-level daemon)
        if let Some(home) = home_dir() {
            let rootless = home.join(".local/share/docker");
            if rootless.exists() {
                paths.push(rootless);
            }
        }
    }

    paths
}

fn dir_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

pub fn scan(root: &Path) -> Result<Vec<Artifact>> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", ""]),
    );
    spinner.set_message("Scanning for dev artefacts...");
    spinner.enable_steady_tick(Duration::from_millis(80));

    let mut candidates: Vec<(PathBuf, ArtifactKind)> = Vec::new();

    let mut walker = WalkDir::new(root).follow_links(false).into_iter();
    loop {
        match walker.next() {
            None => break,
            Some(Err(_)) => continue,
            Some(Ok(entry)) => {
                if should_skip(&entry) {
                    walker.skip_current_dir();
                    continue;
                }
                if let Some(kind) = classify(&entry) {
                    candidates.push((entry.path().to_owned(), kind));
                    walker.skip_current_dir();
                }
            }
        }
    }

    // Fixed-path global artefacts are only relevant when the scan root encompasses home
    let home = home_dir();
    let root_encompasses_home = home.as_deref().is_some_and(|h| h.starts_with(root));

    if root_encompasses_home {
        // Cargo registry lives at $CARGO_HOME/registry — not detectable by directory name alone
        let cargo_registry = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| home.clone().map(|h| h.join(".cargo")))
            .map(|c| c.join("registry"));

        if let Some(reg) = cargo_registry {
            if reg.exists() && !candidates.iter().any(|(p, _)| p == &reg) {
                candidates.push((reg, ArtifactKind::CargoRegistry));
            }
        }

        #[cfg(target_os = "macos")]
        if let Some(ref h) = home {
            let xcode = h.join("Library/Developer/Xcode/DerivedData");
            if xcode.exists() {
                candidates.push((xcode, ArtifactKind::XcodeDerivedData));
            }
        }
    }

    // On Windows/macOS Docker data lives under the user profile → same home gate as Cargo/Xcode.
    // On Linux /var/lib/docker is system-level → always include.
    if root_encompasses_home {
        #[cfg(any(windows, target_os = "macos"))]
        for path in docker_data_paths() {
            if !candidates.iter().any(|(p, _)| p == &path) {
                candidates.push((path, ArtifactKind::DockerData));
            }
        }
    }

    #[cfg(target_os = "linux")]
    for path in docker_data_paths() {
        if !candidates.iter().any(|(p, _)| p == &path) {
            candidates.push((path, ArtifactKind::DockerData));
        }
    }

    spinner.set_message(format!(
        "Computing sizes for {} artefact(s)...",
        candidates.len()
    ));

    let mut artifacts: Vec<Artifact> = candidates
        .into_par_iter()
        .map(|(path, kind)| {
            let size = dir_size(&path);
            Artifact { path, kind, size }
        })
        .collect();

    artifacts.sort_by_key(|a| Reverse(a.size));
    spinner.finish_and_clear();

    Ok(artifacts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_artifact(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::create_dir(&p).unwrap();
        p
    }

    #[test]
    fn classifies_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        make_artifact(dir.path(), "node_modules");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::NodeModules);
    }

    #[test]
    fn classifies_gradle_cache() {
        let dir = tempfile::tempdir().unwrap();
        make_artifact(dir.path(), ".gradle");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::GradleCache);
    }

    #[test]
    fn classifies_pycache() {
        let dir = tempfile::tempdir().unwrap();
        make_artifact(dir.path(), "__pycache__");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::Pycache);
    }

    #[test]
    fn classifies_python_venv_with_cfg() {
        let dir = tempfile::tempdir().unwrap();
        let venv = make_artifact(dir.path(), ".venv");
        fs::write(venv.join("pyvenv.cfg"), "").unwrap();

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::PythonVenv);
    }

    #[test]
    fn ignores_venv_without_cfg() {
        let dir = tempfile::tempdir().unwrap();
        make_artifact(dir.path(), "venv");

        let artifacts = scan(dir.path()).unwrap();
        assert!(artifacts.is_empty());
    }

    #[test]
    fn classifies_cargo_target() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        make_artifact(dir.path(), "target");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::BuildTarget);
    }

    #[test]
    fn classifies_maven_target() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pom.xml"), "").unwrap();
        make_artifact(dir.path(), "target");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::BuildTarget);
    }

    #[test]
    fn ignores_bare_target_dir() {
        let dir = tempfile::tempdir().unwrap();
        make_artifact(dir.path(), "target");

        let artifacts = scan(dir.path()).unwrap();
        assert!(artifacts.is_empty());
    }

    #[test]
    fn does_not_recurse_into_matched_dir() {
        let dir = tempfile::tempdir().unwrap();
        let nm = make_artifact(dir.path(), "node_modules");
        fs::create_dir(nm.join("node_modules")).unwrap();

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
    }

    #[test]
    fn skips_git_dirs() {
        let dir = tempfile::tempdir().unwrap();
        // .git should be skipped entirely — node_modules inside it must not appear
        let git = make_artifact(dir.path(), ".git");
        fs::create_dir(git.join("node_modules")).unwrap();

        let artifacts = scan(dir.path()).unwrap();
        assert!(artifacts.is_empty());
    }

    #[test]
    fn dir_size_counts_bytes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.txt"), "world!").unwrap();
        assert_eq!(dir_size(dir.path()), 11);
    }
}
