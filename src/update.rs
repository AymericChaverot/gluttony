//! Non-blocking update check: runs in the background while scanning, caps the
//! network wait, and caches the answer for a day.

use std::{
    fs,
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use console::style;
use serde::{Deserialize, Serialize};

use crate::trash::gluttony_dir;
use crate::ui::{self, MARGIN};

const REPO: &str = "AymericChaverot/gluttony";
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const CACHE_TTL: u64 = 24 * 3600;
const HTTP_TIMEOUT: Duration = Duration::from_secs(2);
/// How long the end of a command may wait for a pending check.
const MAX_WAIT: Duration = Duration::from_millis(800);

#[cfg(windows)]
const INSTALL_CMD: &str =
    "irm https://raw.githubusercontent.com/AymericChaverot/gluttony/main/scripts/install.ps1 | iex";
#[cfg(not(windows))]
const INSTALL_CMD: &str = "curl -fsSL https://raw.githubusercontent.com/AymericChaverot/gluttony/main/scripts/install.sh | sh";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    checked_at: u64,
    latest: String,
}

pub struct Check(JoinHandle<Option<String>>);

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn cache_path() -> Option<std::path::PathBuf> {
    gluttony_dir().map(|d| d.join("update-check.json"))
}

fn read_cache() -> Option<Cache> {
    let raw = fs::read_to_string(cache_path()?).ok()?;
    let cache: Cache = serde_json::from_str(&raw).ok()?;
    (now_secs().saturating_sub(cache.checked_at) < CACHE_TTL).then_some(cache)
}

fn write_cache(latest: &str) {
    let Some(path) = cache_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let cache = Cache {
        checked_at: now_secs(),
        latest: latest.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = fs::write(path, json);
    }
}

fn fetch_latest_tag() -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .into();
    let release: Release = agent
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("User-Agent", format!("gluttony/{CURRENT}"))
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;
    Some(release.tag_name.trim_start_matches('v').to_string())
}

/// Starts the check in the background, unless disabled via
/// `GLUTTONY_NO_UPDATE_CHECK` or when output is not a terminal.
pub fn spawn() -> Option<Check> {
    if std::env::var_os("GLUTTONY_NO_UPDATE_CHECK").is_some() || !console::user_attended() {
        return None;
    }
    Some(Check(thread::spawn(|| {
        if let Some(cache) = read_cache() {
            return Some(cache.latest);
        }
        let latest = fetch_latest_tag()?;
        write_cache(&latest);
        Some(latest)
    })))
}

impl Check {
    /// Prints a notice if a newer release exists. Waits briefly at most.
    pub fn finish(self) {
        let deadline = Instant::now() + MAX_WAIT;
        while !self.0.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        if !self.0.is_finished() {
            return;
        }
        let Ok(Some(latest)) = self.0.join() else {
            return;
        };
        if is_newer(&latest, CURRENT) {
            println!(
                "{MARGIN}{} {}  {} {} {}",
                style("↑").cyan().bold(),
                style("update available").bold(),
                style(CURRENT).dim(),
                style(ui::ARROW).dim(),
                style(&latest).cyan().bold(),
            );
            ui::note(INSTALL_CMD);
            println!();
        }
    }
}

fn is_newer(latest: &str, current: &str) -> bool {
    parse_semver(latest) > parse_semver(current)
}

fn parse_semver(v: &str) -> (u32, u32, u32) {
    let mut parts = v.split('.').filter_map(|p| p.parse::<u32>().ok());
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_newer_version() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
    }

    #[test]
    fn rejects_same_or_older() {
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn parses_short_versions() {
        assert_eq!(parse_semver("1"), (1, 0, 0));
        assert_eq!(parse_semver("1.2"), (1, 2, 0));
        assert_eq!(parse_semver("1.2.3"), (1, 2, 3));
    }
}
