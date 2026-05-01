use std::path::PathBuf;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use super::home_dir;

pub fn data_paths() -> Vec<PathBuf> {
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
