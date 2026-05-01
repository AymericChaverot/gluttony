use std::path::PathBuf;

use super::home_dir;

pub fn cache_path() -> Option<PathBuf> {
    let p = home_dir()?.join(".gem").join("ruby");
    p.exists().then_some(p)
}
