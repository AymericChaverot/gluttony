use serde::Deserialize;

use console::style;

const REPO: &str = "AymericChaverot/gluttony";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

pub fn check_and_notify() {
    if let Some(latest) = fetch_latest_tag() {
        let latest_clean = latest.trim_start_matches('v');
        if is_newer(latest_clean, CURRENT) {
            println!(
                "{} {} → {}",
                style("A new version of gluttony is available:").dim(),
                style(CURRENT).dim(),
                style(latest_clean).cyan().bold(),
            );
            println!(
                "{}",
                style("  curl -fsSL https://raw.githubusercontent.com/AymericChaverot/gluttony/main/scripts/install.sh | sh").dim()
            );
            println!();
        }
    }
}

fn fetch_latest_tag() -> Option<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let release: Release = ureq::get(&url)
        .set("User-Agent", &format!("gluttony/{CURRENT}"))
        .call()
        .ok()?
        .into_json()
        .ok()?;
    Some(release.tag_name)
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
