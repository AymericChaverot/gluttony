use super::{walker::has_git_ancestor, ArtifactKind};

pub fn classify(name: &str, entry: &walkdir::DirEntry) -> Option<ArtifactKind> {
    match name {
        "__pycache__" => Some(ArtifactKind::Pycache),
        ".pytest_cache" => {
            if has_git_ancestor(entry.path(), 4) {
                Some(ArtifactKind::PytestCache)
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
        _ => None,
    }
}
