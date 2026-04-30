use std::{
    cmp::Reverse,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
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
    PytestCache,
    PythonVenv,
    ToxEnv,
    CargoRegistry,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    XcodeDerivedData,
    DockerData,
    NextCache,
    NuxtCache,
    TurboCache,
    ParcelCache,
    FlutterBuild,
    ElixirBuild,
}

impl ArtifactKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NodeModules => "node_modules",
            Self::GradleCache => ".gradle",
            Self::BuildTarget => "target (build)",
            Self::Pycache => "__pycache__",
            Self::PytestCache => ".pytest_cache",
            Self::PythonVenv => "Python venv",
            Self::ToxEnv => ".tox",
            Self::CargoRegistry => "Cargo registry",
            Self::XcodeDerivedData => "Xcode DerivedData",
            Self::DockerData => "Docker data",
            Self::NextCache => ".next",
            Self::NuxtCache => ".nuxt",
            Self::TurboCache => ".turbo",
            Self::ParcelCache => ".parcel-cache",
            Self::FlutterBuild => "Flutter build",
            Self::ElixirBuild => "Elixir _build",
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

fn should_skip_name(name: &str) -> bool {
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
    entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .map_or(false, should_skip_name)
}

// Directories we never recurse into regardless of classification. Without this,
// an unclassified node_modules (no package.json) would be walked, potentially
// exposing artefacts inside installed tools to deletion.
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

// Walk up to max_depth ancestor directories looking for a .git entry.
// Application-bundled node_modules / caches (VS Code, JetBrains, Discord, …)
// are never inside a git repository; this cleanly separates them from real
// developer projects. Works for regular repos and git worktrees (.git can be
// a file in worktrees).
fn has_git_ancestor(path: &Path, max_depth: usize) -> bool {
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
        "node_modules" => {
            let parent = entry.path().parent()?;
            // package.json alone is not enough: installed apps (VS Code, JetBrains,
            // Discord) also have it. A git repo in an ancestor is the real signal.
            if parent.join("package.json").exists() && has_git_ancestor(parent, 4) {
                Some(ArtifactKind::NodeModules)
            } else {
                None
            }
        }
        ".next" | ".nuxt" => {
            let parent = entry.path().parent()?;
            if parent.join("package.json").exists() && has_git_ancestor(parent, 4) {
                Some(if name == ".next" {
                    ArtifactKind::NextCache
                } else {
                    ArtifactKind::NuxtCache
                })
            } else {
                None
            }
        }
        ".turbo" => {
            if has_git_ancestor(entry.path(), 4) {
                Some(ArtifactKind::TurboCache)
            } else {
                None
            }
        }
        ".parcel-cache" => {
            if has_git_ancestor(entry.path(), 3) {
                Some(ArtifactKind::ParcelCache)
            } else {
                None
            }
        }
        ".gradle" => Some(ArtifactKind::GradleCache),
        "__pycache__" => Some(ArtifactKind::Pycache),
        ".pytest_cache" => {
            if has_git_ancestor(entry.path(), 4) {
                Some(ArtifactKind::PytestCache)
            } else {
                None
            }
        }
        ".tox" => {
            let parent = entry.path().parent()?;
            if (parent.join("tox.ini").exists()
                || parent.join("setup.cfg").exists()
                || parent.join("pyproject.toml").exists())
                && has_git_ancestor(parent, 4)
            {
                Some(ArtifactKind::ToxEnv)
            } else {
                None
            }
        }
        ".venv" | "venv" => {
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
        "build" => {
            let parent = entry.path().parent()?;
            if parent.join("pubspec.yaml").exists() {
                Some(ArtifactKind::FlutterBuild)
            } else {
                None
            }
        }
        "_build" => {
            let parent = entry.path().parent()?;
            if parent.join("mix.exs").exists() {
                Some(ArtifactKind::ElixirBuild)
            } else {
                None
            }
        }
        _ => None,
    }
}

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

fn comma(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len().saturating_sub(1) / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

fn walk_subtree(
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
                if local_n % 500 == 0 {
                    let c = checked.fetch_add(500, Ordering::Relaxed) + 500;
                    let f = found_count.load(Ordering::Relaxed);
                    spinner.set_message(format!(
                        "Scanning…  {} entries checked, {} found",
                        comma(c),
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

pub fn scan(root: &Path) -> Result<Vec<Artifact>> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", ""]),
    );
    spinner.set_message("Scanning…");
    spinner.enable_steady_tick(Duration::from_millis(80));

    let checked = AtomicU64::new(0);
    let found_count = AtomicU64::new(0);

    let walk_roots: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map_or(false, |t| t.is_dir()))
                .filter(|e| !should_skip_name(e.file_name().to_str().unwrap_or("")))
                .map(|e| e.path())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|_| vec![root.to_path_buf()]);

    let mut candidates: Vec<(PathBuf, ArtifactKind)> = walk_roots
        .into_par_iter()
        .flat_map_iter(|dir| walk_subtree(&dir, &checked, &found_count, &spinner))
        .collect();

    let home = home_dir();
    let root_encompasses_home = home.as_deref().is_some_and(|h| h.starts_with(root));

    if root_encompasses_home {
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
        "Computing sizes for {} artefact(s)…",
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
    fn classifies_node_modules_with_package_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_artifact(dir.path(), ".git");
        make_artifact(dir.path(), "node_modules");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::NodeModules);
    }

    #[test]
    fn ignores_node_modules_of_installed_app() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_artifact(dir.path(), "node_modules");

        let artifacts = scan(dir.path()).unwrap();
        assert!(artifacts.is_empty());
    }

    #[test]
    fn ignores_node_modules_without_package_json() {
        let dir = tempfile::tempdir().unwrap();
        make_artifact(dir.path(), "node_modules");

        let artifacts = scan(dir.path()).unwrap();
        assert!(artifacts.is_empty());
    }

    #[test]
    fn does_not_recurse_into_unclassified_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let nm = make_artifact(dir.path(), "node_modules");
        fs::create_dir(nm.join("__pycache__")).unwrap();

        let artifacts = scan(dir.path()).unwrap();
        assert!(artifacts.is_empty());
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
    fn classifies_next_cache() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_artifact(dir.path(), ".git");
        make_artifact(dir.path(), ".next");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::NextCache);
    }

    #[test]
    fn classifies_nuxt_cache() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_artifact(dir.path(), ".git");
        make_artifact(dir.path(), ".nuxt");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::NuxtCache);
    }

    #[test]
    fn classifies_turbo_cache() {
        let dir = tempfile::tempdir().unwrap();
        make_artifact(dir.path(), ".git");
        make_artifact(dir.path(), ".turbo");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::TurboCache);
    }

    #[test]
    fn classifies_pytest_cache() {
        let dir = tempfile::tempdir().unwrap();
        make_artifact(dir.path(), ".git");
        make_artifact(dir.path(), ".pytest_cache");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::PytestCache);
    }

    #[test]
    fn classifies_tox_env() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tox.ini"), "").unwrap();
        make_artifact(dir.path(), ".git");
        make_artifact(dir.path(), ".tox");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::ToxEnv);
    }

    #[test]
    fn classifies_flutter_build() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pubspec.yaml"), "").unwrap();
        make_artifact(dir.path(), "build");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::FlutterBuild);
    }

    #[test]
    fn classifies_elixir_build() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("mix.exs"), "").unwrap();
        make_artifact(dir.path(), "_build");

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::ElixirBuild);
    }

    #[test]
    fn does_not_recurse_into_matched_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_artifact(dir.path(), ".git");
        let nm = make_artifact(dir.path(), "node_modules");
        fs::create_dir(nm.join("node_modules")).unwrap();

        let artifacts = scan(dir.path()).unwrap();
        assert_eq!(artifacts.len(), 1);
    }

    #[test]
    fn skips_git_dirs() {
        let dir = tempfile::tempdir().unwrap();
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
