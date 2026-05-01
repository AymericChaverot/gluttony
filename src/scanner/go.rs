use std::path::PathBuf;

use super::home_dir;

pub fn cache_path() -> Option<PathBuf> {
    // Respect $GOPATH if set, otherwise fall back to ~/go
    let gopath = std::env::var_os("GOPATH")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join("go")))?;
    let p = gopath.join("pkg").join("mod");
    p.exists().then_some(p)
}
