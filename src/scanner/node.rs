use super::{walker::has_git_ancestor, ArtifactKind};

pub fn classify(name: &str, entry: &walkdir::DirEntry) -> Option<ArtifactKind> {
    match name {
        "node_modules" => {
            let parent = entry.path().parent()?;
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
        _ => None,
    }
}
