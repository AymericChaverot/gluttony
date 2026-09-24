mod build;
pub mod docker;
mod elixir;
mod flutter;
mod go;
mod node;
mod python;
mod ruby;
pub mod walker;

use std::{
    cmp::Reverse,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime},
};

use console::Style;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::Serialize;

use crate::error::Result;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    NodeModules,
    NextCache,
    NuxtCache,
    TurboCache,
    ParcelCache,
    CargoTarget,
    CargoRegistry,
    MavenTarget,
    GradleCache,
    Pycache,
    PytestCache,
    PythonVenv,
    ToxEnv,
    FlutterBuild,
    ElixirBuild,
    GoModuleCache,
    RubyGems,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    XcodeDerivedData,
}

impl ArtifactKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::NodeModules => "node_modules",
            Self::NextCache => ".next",
            Self::NuxtCache => ".nuxt",
            Self::TurboCache => ".turbo",
            Self::ParcelCache => ".parcel-cache",
            Self::CargoTarget => "target (cargo)",
            Self::CargoRegistry => "Cargo registry",
            Self::MavenTarget => "target (maven)",
            Self::GradleCache => ".gradle",
            Self::Pycache => "__pycache__",
            Self::PytestCache => ".pytest_cache",
            Self::PythonVenv => "Python venv",
            Self::ToxEnv => ".tox",
            Self::FlutterBuild => "Flutter build",
            Self::ElixirBuild => "Elixir _build",
            Self::GoModuleCache => "Go module cache",
            Self::RubyGems => "Ruby gems",
            Self::XcodeDerivedData => "Xcode DerivedData",
        }
    }

    pub fn ecosystem(self) -> Ecosystem {
        match self {
            Self::NodeModules
            | Self::NextCache
            | Self::NuxtCache
            | Self::TurboCache
            | Self::ParcelCache => Ecosystem::Node,
            Self::CargoTarget | Self::CargoRegistry => Ecosystem::Rust,
            Self::MavenTarget | Self::GradleCache => Ecosystem::Jvm,
            Self::Pycache | Self::PytestCache | Self::PythonVenv | Self::ToxEnv => {
                Ecosystem::Python
            }
            Self::FlutterBuild => Ecosystem::Flutter,
            Self::ElixirBuild => Ecosystem::Elixir,
            Self::GoModuleCache => Ecosystem::Go,
            Self::RubyGems => Ecosystem::Ruby,
            Self::XcodeDerivedData => Ecosystem::Xcode,
        }
    }

    /// Machine-wide caches that do not belong to a single project.
    pub fn is_global(self) -> bool {
        matches!(
            self,
            Self::CargoRegistry | Self::GoModuleCache | Self::RubyGems | Self::XcodeDerivedData
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Node,
    Rust,
    Python,
    Jvm,
    Flutter,
    Elixir,
    Go,
    Ruby,
    Xcode,
}

#[derive(Debug, Clone)]
pub struct Artifact {
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub size: u64,
    /// Project the artefact belongs to (`None` for global caches).
    pub project: Option<PathBuf>,
    /// Last observed activity in the owning project.
    pub last_active: Option<SystemTime>,
}

impl Artifact {
    pub fn age(&self) -> Option<Duration> {
        self.last_active
            .and_then(|t| SystemTime::now().duration_since(t).ok())
    }
}

/// Narrows down what a scan reports.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub only: Vec<Ecosystem>,
    pub exclude: Vec<String>,
    pub min_size: Option<u64>,
    pub older_than: Option<Duration>,
}

impl Filter {
    /// Cheap checks applied before sizes are computed.
    fn keeps_candidate(&self, path: &Path, kind: ArtifactKind) -> bool {
        if !self.only.is_empty() && !self.only.contains(&kind.ecosystem()) {
            return false;
        }
        let shown = ui::display_path(path);
        !self.exclude.iter().any(|pat| shown.contains(pat.as_str()))
    }

    fn keeps(&self, a: &Artifact) -> bool {
        if self.min_size.is_some_and(|min| a.size < min) {
            return false;
        }
        match self.older_than {
            Some(limit) => a.age().is_some_and(|age| age >= limit),
            None => true,
        }
    }

    pub fn is_active(&self) -> bool {
        !self.only.is_empty()
            || !self.exclude.is_empty()
            || self.min_size.is_some()
            || self.older_than.is_some()
    }
}

pub struct ScanReport {
    pub root: PathBuf,
    pub artifacts: Vec<Artifact>,
    pub docker: Option<docker::DockerInfo>,
    pub entries: u64,
    pub elapsed: Duration,
}

impl ScanReport {
    pub fn total(&self) -> u64 {
        self.artifacts.iter().map(|a| a.size).sum()
    }
}

pub fn home_dir() -> Option<PathBuf> {
    ui::home_dir()
}

/// Loader look for each scan phase: cyan while discovering, green while measuring.
fn loader_style(segment: Style) -> ProgressStyle {
    let frames = ui::bounce_frames(&segment.bold());
    let frames: Vec<&str> = frames.iter().map(String::as_str).collect();
    ProgressStyle::with_template(&format!(
        "{m}{{spinner}}  {{msg}}\n{m}{{prefix}}",
        m = ui::MARGIN
    ))
    .expect("valid template")
    .tick_strings(&frames)
}

fn spinner() -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(loader_style(Style::new().cyan()));
    spinner.enable_steady_tick(ui::BOUNCE_TICK);
    spinner
}

pub fn scan(root: &Path, filter: &Filter) -> Result<ScanReport> {
    let started = Instant::now();
    let spinner = spinner();
    spinner.set_message(console::style("scanning").bold().to_string());
    spinner.set_prefix(console::style(ui::display_path(root)).dim().to_string());

    let checked = AtomicU64::new(0);
    let found_count = AtomicU64::new(0);

    let home = home_dir();
    let root_encompasses_home = home.as_deref().is_some_and(|h| h.starts_with(root));
    let want_docker = filter.only.is_empty();

    let (mut candidates, docker) = rayon::join(
        || walk(root, &checked, &found_count, &spinner),
        || {
            want_docker
                .then(|| docker::inspect(root, root_encompasses_home))
                .flatten()
        },
    );

    if root_encompasses_home {
        candidates.extend(global_caches(home.as_deref()));
    }
    // Path order puts parents first, so this drops duplicates and anything
    // nested inside another artefact (e.g. a `.gradle` in the Go module cache).
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    candidates.dedup_by(|a, b| a.0.starts_with(&b.0));
    candidates.retain(|(p, k)| filter.keeps_candidate(p, *k));

    spinner.set_message(format!(
        "{}  {}",
        console::style("measuring").bold(),
        console::style(ui::plural(candidates.len(), "artefact")).dim()
    ));
    spinner.set_prefix(String::new());
    spinner.set_style(loader_style(Style::new().green()));

    let mut artifacts: Vec<Artifact> = candidates
        .into_par_iter()
        .map(|(path, kind)| {
            let size = walker::dir_size(&path);
            let project = project_root(&path, kind, home.as_deref());
            let last_active = project
                .as_deref()
                .and_then(|p| last_activity(p, path.parent()));
            Artifact {
                path,
                kind,
                size,
                project,
                last_active,
            }
        })
        .filter(|a| filter.keeps(a))
        .collect();

    artifacts.sort_by_key(|a| Reverse(a.size));
    spinner.finish_and_clear();

    Ok(ScanReport {
        root: root.to_path_buf(),
        artifacts,
        docker,
        entries: checked.load(Ordering::Relaxed),
        elapsed: started.elapsed(),
    })
}

fn walk(
    root: &Path,
    checked: &AtomicU64,
    found_count: &AtomicU64,
    spinner: &ProgressBar,
) -> Vec<(PathBuf, ArtifactKind)> {
    let walk_roots: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter(|e| !walker::should_skip_name(e.file_name().to_str().unwrap_or("")))
                .map(|e| e.path())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|_| vec![root.to_path_buf()]);

    walk_roots
        .into_par_iter()
        .flat_map_iter(|dir| walker::walk_subtree(&dir, checked, found_count, spinner))
        .collect()
}

fn global_caches(home: Option<&Path>) -> Vec<(PathBuf, ArtifactKind)> {
    let mut out = Vec::new();

    let cargo_registry = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".cargo")))
        .map(|c| c.join("registry"));
    if let Some(reg) = cargo_registry.filter(|r| r.exists()) {
        out.push((reg, ArtifactKind::CargoRegistry));
    }

    #[cfg(target_os = "macos")]
    if let Some(h) = home {
        let xcode = h.join("Library/Developer/Xcode/DerivedData");
        if xcode.exists() {
            out.push((xcode, ArtifactKind::XcodeDerivedData));
        }
    }

    if let Some(p) = go::cache_path() {
        out.push((p, ArtifactKind::GoModuleCache));
    }
    if let Some(p) = ruby::cache_path() {
        out.push((p, ArtifactKind::RubyGems));
    }
    out
}

/// The nearest ancestor that is a git repository, or the artefact's parent.
fn project_root(path: &Path, kind: ArtifactKind, home: Option<&Path>) -> Option<PathBuf> {
    if kind.is_global() {
        return None;
    }
    let parent = path.parent()?;
    // `~/.gradle` is the user-wide Gradle cache, not a project directory.
    if home == Some(parent) {
        return None;
    }
    let git_root = parent
        .ancestors()
        .take(6)
        .take_while(|p| Some(*p) != home)
        .find(|p| p.join(".git").exists());
    Some(git_root.unwrap_or(parent).to_path_buf())
}

/// Best-effort "last worked on" timestamp: git index activity and the
/// modification times of top-level entries, ignoring artefact directories.
fn last_activity(project: &Path, artifact_parent: Option<&Path>) -> Option<SystemTime> {
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();

    let git = [".git/index", ".git/logs/HEAD", ".git/HEAD"]
        .iter()
        .filter_map(|f| mtime(&project.join(f)));

    let mut dirs = vec![project];
    if let Some(p) = artifact_parent.filter(|p| *p != project) {
        dirs.push(p);
    }
    let entries = dirs.into_iter().flat_map(|dir| {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_str().unwrap_or("");
                name != ".git" && !walker::is_artefact_boundary(name)
            })
            .filter_map(|e| e.metadata().and_then(|m| m.modified()).ok())
    });

    git.chain(entries).max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_dir(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::create_dir(&p).unwrap();
        p
    }

    fn scan_all(root: &Path) -> Vec<Artifact> {
        scan(root, &Filter::default()).unwrap().artifacts
    }

    #[test]
    fn classifies_node_modules_with_package_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_dir(dir.path(), ".git");
        make_dir(dir.path(), "node_modules");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::NodeModules);
        assert_eq!(artifacts[0].project.as_deref(), Some(dir.path()));
        assert!(artifacts[0].last_active.is_some());
    }

    #[test]
    fn ignores_node_modules_of_installed_app() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_dir(dir.path(), "node_modules");

        assert!(scan_all(dir.path()).is_empty());
    }

    #[test]
    fn ignores_node_modules_without_package_json() {
        let dir = tempfile::tempdir().unwrap();
        make_dir(dir.path(), "node_modules");

        assert!(scan_all(dir.path()).is_empty());
    }

    #[test]
    fn does_not_recurse_into_unclassified_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        let nm = make_dir(dir.path(), "node_modules");
        fs::create_dir(nm.join("__pycache__")).unwrap();

        assert!(scan_all(dir.path()).is_empty());
    }

    #[test]
    fn classifies_gradle_cache() {
        let dir = tempfile::tempdir().unwrap();
        make_dir(dir.path(), ".gradle");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::GradleCache);
    }

    #[test]
    fn classifies_pycache() {
        let dir = tempfile::tempdir().unwrap();
        make_dir(dir.path(), "__pycache__");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::Pycache);
    }

    #[test]
    fn classifies_python_venv_with_cfg() {
        let dir = tempfile::tempdir().unwrap();
        let venv = make_dir(dir.path(), ".venv");
        fs::write(venv.join("pyvenv.cfg"), "").unwrap();

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::PythonVenv);
    }

    #[test]
    fn ignores_venv_without_cfg() {
        let dir = tempfile::tempdir().unwrap();
        make_dir(dir.path(), "venv");

        assert!(scan_all(dir.path()).is_empty());
    }

    #[test]
    fn classifies_cargo_target() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        make_dir(dir.path(), "target");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::CargoTarget);
    }

    #[test]
    fn classifies_maven_target() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pom.xml"), "").unwrap();
        make_dir(dir.path(), "target");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::MavenTarget);
    }

    #[test]
    fn ignores_bare_target_dir() {
        let dir = tempfile::tempdir().unwrap();
        make_dir(dir.path(), "target");

        assert!(scan_all(dir.path()).is_empty());
    }

    #[test]
    fn classifies_next_cache() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_dir(dir.path(), ".git");
        make_dir(dir.path(), ".next");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::NextCache);
    }

    #[test]
    fn classifies_nuxt_cache() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_dir(dir.path(), ".git");
        make_dir(dir.path(), ".nuxt");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::NuxtCache);
    }

    #[test]
    fn classifies_turbo_cache() {
        let dir = tempfile::tempdir().unwrap();
        make_dir(dir.path(), ".git");
        make_dir(dir.path(), ".turbo");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::TurboCache);
    }

    #[test]
    fn classifies_pytest_cache() {
        let dir = tempfile::tempdir().unwrap();
        make_dir(dir.path(), ".git");
        make_dir(dir.path(), ".pytest_cache");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::PytestCache);
    }

    #[test]
    fn classifies_tox_env() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tox.ini"), "").unwrap();
        make_dir(dir.path(), ".git");
        make_dir(dir.path(), ".tox");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::ToxEnv);
    }

    #[test]
    fn classifies_flutter_build() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pubspec.yaml"), "").unwrap();
        make_dir(dir.path(), "build");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::FlutterBuild);
    }

    #[test]
    fn classifies_elixir_build() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("mix.exs"), "").unwrap();
        make_dir(dir.path(), "_build");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ArtifactKind::ElixirBuild);
    }

    #[test]
    fn does_not_recurse_into_matched_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        make_dir(dir.path(), ".git");
        let nm = make_dir(dir.path(), "node_modules");
        fs::create_dir(nm.join("node_modules")).unwrap();

        assert_eq!(scan_all(dir.path()).len(), 1);
    }

    #[test]
    fn skips_git_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let git = make_dir(dir.path(), ".git");
        fs::create_dir(git.join("node_modules")).unwrap();

        assert!(scan_all(dir.path()).is_empty());
    }

    #[test]
    fn monorepo_artefacts_belong_to_git_root() {
        let dir = tempfile::tempdir().unwrap();
        make_dir(dir.path(), ".git");
        let app = dir.path().join("apps").join("web");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("package.json"), "{}").unwrap();
        make_dir(&app, "node_modules");

        let artifacts = scan_all(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].project.as_deref(), Some(dir.path()));
    }

    #[test]
    fn filter_only_and_exclude() {
        let dir = tempfile::tempdir().unwrap();
        make_dir(dir.path(), "__pycache__");
        make_dir(dir.path(), ".gradle");

        let only_python = Filter {
            only: vec![Ecosystem::Python],
            ..Filter::default()
        };
        let found = scan(dir.path(), &only_python).unwrap().artifacts;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ArtifactKind::Pycache);

        let exclude_gradle = Filter {
            exclude: vec![".gradle".into()],
            ..Filter::default()
        };
        let found = scan(dir.path(), &exclude_gradle).unwrap().artifacts;
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ArtifactKind::Pycache);
    }

    #[test]
    fn filter_min_size_and_age() {
        let dir = tempfile::tempdir().unwrap();
        let cache = make_dir(dir.path(), "__pycache__");
        fs::write(cache.join("a.pyc"), "0123456789").unwrap();
        fs::write(dir.path().join("a.py"), "").unwrap();

        let big = Filter {
            min_size: Some(11),
            ..Filter::default()
        };
        assert!(scan(dir.path(), &big).unwrap().artifacts.is_empty());

        let stale = Filter {
            older_than: Some(Duration::from_secs(86_400)),
            ..Filter::default()
        };
        assert!(scan(dir.path(), &stale).unwrap().artifacts.is_empty());
    }

    #[test]
    fn dir_size_counts_bytes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.txt"), "world!").unwrap();
        assert_eq!(walker::dir_size(dir.path()), 11);
    }
}
