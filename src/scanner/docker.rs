//! Docker is reported, never cleaned: its data lives in a VM disk image or a
//! daemon-owned directory that must only be pruned through Docker itself.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::Deserialize;

#[cfg(unix)]
use super::home_dir;
use super::walker::dir_size;

const CLI_TIMEOUT: Duration = Duration::from_millis(2500);

#[derive(Debug, Clone)]
pub struct DockerInfo {
    /// Bytes used on the host disk by Docker's data directory or VM image.
    pub disk_size: u64,
    /// Bytes `docker system prune` could reclaim, when the daemon answers.
    pub reclaimable: Option<u64>,
}

/// Where Docker keeps its data, and whether it is a single disk image file.
fn storage() -> Option<(PathBuf, bool)> {
    #[cfg(windows)]
    {
        let local = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
        // Current Docker Desktop layout first, then the pre-4.30 one.
        [
            local.join(r"Docker\wsl\disk\docker_data.vhdx"),
            local.join(r"Docker\wsl\data\ext4.vhdx"),
        ]
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| (p, true))
    }
    #[cfg(unix)]
    {
        let home = home_dir();
        let images = [
            "Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw",
            ".docker/desktop/vms/0/data/Docker.raw",
        ];
        let image = home
            .as_ref()
            .and_then(|h| images.iter().map(|p| h.join(p)).find(|p| p.is_file()));
        image.map(|p| (p, true)).or_else(|| {
            let mut dirs = vec![PathBuf::from("/var/lib/docker")];
            if let Some(h) = &home {
                dirs.push(h.join(".local/share/docker"));
            }
            dirs.into_iter().find(|p| p.is_dir()).map(|p| (p, false))
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// Bytes allocated on disk (differs from the apparent size for sparse files
/// such as Docker Desktop's `Docker.raw`).
fn allocated_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::metadata(path) else {
        return 0;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.blocks() * 512
    }
    #[cfg(not(unix))]
    {
        meta.len()
    }
}

/// Inspects Docker's footprint. Returns `None` when Docker is not installed or
/// its data lies outside the scanned area.
pub fn inspect(root: &Path, root_encompasses_home: bool) -> Option<DockerInfo> {
    let (path, is_image) =
        storage().filter(|(p, _)| root_encompasses_home || p.starts_with(root))?;
    // Asking a stopped daemon only waits for a timeout, so check first.
    let reclaimable = thread::spawn(|| daemon_running().then(cli_reclaimable).flatten());
    let disk_size = if is_image {
        allocated_size(&path)
    } else {
        dir_size(&path)
    };
    Some(DockerInfo {
        disk_size,
        reclaimable: reclaimable.join().ok().flatten(),
    })
}

fn daemon_running() -> bool {
    #[cfg(unix)]
    {
        let mut sockets = vec![
            PathBuf::from("/var/run/docker.sock"),
            PathBuf::from("/run/docker.sock"),
        ];
        if let Some(home) = home_dir() {
            sockets.push(home.join(".docker/run/docker.sock"));
            sockets.push(home.join(".docker/desktop/docker.sock"));
        }
        sockets.iter().any(|s| s.exists())
    }
    #[cfg(windows)]
    {
        std::fs::metadata(r"\\.\pipe\docker_engine").is_ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[derive(Deserialize)]
struct DfLine {
    #[serde(rename = "Reclaimable")]
    reclaimable: String,
}

/// Sums the "reclaimable" column of `docker system df`, with a hard timeout
/// so a stuck daemon never stalls the scan.
fn cli_reclaimable() -> Option<u64> {
    let mut child = Command::new("docker")
        .args(["system", "df", "--format", "{{json .}}"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + CLI_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(40)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }

    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    let sizes: Vec<u64> = out.lines().filter_map(parse_df_line).collect();
    (!sizes.is_empty()).then(|| sizes.iter().sum())
}

/// Reclaimable bytes from one `docker system df` JSON line
/// (`"Reclaimable":"48.73MB (100%)"`).
fn parse_df_line(line: &str) -> Option<u64> {
    let row: DfLine = serde_json::from_str(line).ok()?;
    parse_docker_size(row.reclaimable.split_whitespace().next()?)
}

/// Parses Docker's human sizes: `0B`, `301kB`, `12.5MB`, `1.234GB`.
fn parse_docker_size(s: &str) -> Option<u64> {
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (num, unit) = s.split_at(split);
    let value: f64 = num.parse().ok()?;
    let mult = match unit.to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => return None,
    };
    Some((value * mult) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_sizes() {
        assert_eq!(parse_docker_size("0B"), Some(0));
        assert_eq!(parse_docker_size("301kB"), Some(301_000));
        assert_eq!(parse_docker_size("12.5MB"), Some(12_500_000));
        assert_eq!(parse_docker_size("1.234GB"), Some(1_234_000_000));
        assert_eq!(parse_docker_size("n/a"), None);
    }

    #[test]
    fn parses_df_json_lines() {
        let line = r#"{"Active":"1","Reclaimable":"48.73MB (100%)","Size":"48.73MB","TotalCount":"2","Type":"Local Volumes"}"#;
        assert_eq!(parse_df_line(line), Some(48_730_000));
        assert_eq!(parse_df_line("not json"), None);
    }
}
