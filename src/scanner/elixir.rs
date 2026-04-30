use super::ArtifactKind;

pub fn classify(entry: &walkdir::DirEntry) -> Option<ArtifactKind> {
    let parent = entry.path().parent()?;
    if parent.join("mix.exs").exists() {
        Some(ArtifactKind::ElixirBuild)
    } else {
        None
    }
}
