use super::ArtifactKind;

pub fn classify(entry: &walkdir::DirEntry) -> Option<ArtifactKind> {
    let parent = entry.path().parent()?;
    if parent.join("pubspec.yaml").exists() {
        Some(ArtifactKind::FlutterBuild)
    } else {
        None
    }
}
