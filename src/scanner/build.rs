use super::ArtifactKind;

pub fn classify(entry: &walkdir::DirEntry) -> Option<ArtifactKind> {
    let parent = entry.path().parent()?;
    if parent.join("Cargo.toml").exists() {
        Some(ArtifactKind::CargoTarget)
    } else if parent.join("pom.xml").exists() {
        Some(ArtifactKind::MavenTarget)
    } else {
        None
    }
}
