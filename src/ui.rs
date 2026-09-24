//! Shared visual language: symbols, colours, formatting helpers and prompts.
//!
//! Only plain ASCII and typographic Unicode symbols are used — never emoji —
//! so the output renders identically in every terminal font.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use console::{Key, Style, StyledObject, Term, measure_text_width, style, truncate_str};

use crate::scanner::Ecosystem;

// ── Symbols ───────────────────────────────────────────────────────────────────

pub const CHECK: &str = "✓";
pub const CROSS: &str = "✗";
pub const ARROW: &str = "→";
pub const DOT: &str = "·";
pub const BULLET: &str = "●";
pub const WARN: &str = "▲";
pub const ASK: &str = "?";
pub const ELLIPSIS: &str = "…";
pub const RULE: &str = "─";
pub const BAR_FULL: &str = "━";
pub const BAR_HALF: &str = "╸";

pub const BOUNCE_TICK: Duration = Duration::from_millis(50);
const BOUNCE_TRACK: usize = 24;
const BOUNCE_SEGMENT: usize = 6;

/// Frames of a segment bouncing back and forth on a dim track, in the same
/// style as the table bars, with the segment in `color`. The trailing empty
/// frame is shown once finished.
pub fn bounce_frames(color: &Style) -> Vec<String> {
    let last = BOUNCE_TRACK - BOUNCE_SEGMENT;
    let frame = |pos: usize| {
        format!(
            "{}{}{}",
            track().apply_to(BAR_FULL.repeat(pos)),
            color.apply_to(BAR_FULL.repeat(BOUNCE_SEGMENT)),
            track().apply_to(BAR_FULL.repeat(last - pos)),
        )
    };
    (0..=last)
        .chain((1..last).rev())
        .map(frame)
        .chain(std::iter::once(String::new()))
        .collect()
}

pub const MARGIN: &str = "  ";

// ── Colours ───────────────────────────────────────────────────────────────────

pub fn eco_style(eco: Ecosystem) -> Style {
    let code = match eco {
        Ecosystem::Node => 114,
        Ecosystem::Rust => 209,
        Ecosystem::Python => 221,
        Ecosystem::Jvm => 167,
        Ecosystem::Flutter => 45,
        Ecosystem::Elixir => 141,
        Ecosystem::Go => 80,
        Ecosystem::Ruby => 204,
        Ecosystem::Xcode => 111,
    };
    Style::new().color256(code)
}

pub fn accent() -> Style {
    Style::new().cyan()
}

pub fn track() -> Style {
    Style::new().color256(238)
}

const RED_THRESHOLD: u64 = 1_000_000_000;
const YELLOW_THRESHOLD: u64 = 100_000_000;

pub fn size_style(bytes: u64) -> Style {
    if bytes >= RED_THRESHOLD {
        Style::new().red().bold()
    } else if bytes >= YELLOW_THRESHOLD {
        Style::new().yellow()
    } else {
        Style::new()
    }
}

/// Age of a project, highlighted when it was active recently (i.e. risky to clean).
pub fn age_cell(age: Option<Duration>, width: usize) -> StyledObject<String> {
    let text = format!("{:<width$}", age.map_or_else(|| "-".to_string(), human_age));
    match age {
        Some(a) if a < Duration::from_secs(7 * 86_400) => style(text).yellow(),
        _ => style(text).dim(),
    }
}

// ── Terminal geometry ─────────────────────────────────────────────────────────

/// Usable output width, clamped to keep tables readable.
pub fn width() -> usize {
    Term::stdout()
        .size_checked()
        .map_or(100, |(_, cols)| cols as usize)
        .clamp(60, 120)
}

pub fn height() -> usize {
    Term::stderr()
        .size_checked()
        .map_or(24, |(rows, _)| rows as usize)
}

/// Pads (or truncates) a possibly-styled string to exactly `width` columns.
pub fn fit(s: &str, width: usize) -> String {
    let w = measure_text_width(s);
    if w > width {
        truncate_str(s, width, ELLIPSIS).into_owned()
    } else {
        format!("{s}{}", " ".repeat(width - w))
    }
}

/// Shortens a plain path by eliding its middle: `~/Documents/…/apps/web/.next`.
pub fn truncate_middle(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= width {
        return s.to_string();
    }
    if width <= 2 {
        return ELLIPSIS.to_string();
    }
    // Keep the root-most component (e.g. `~/` or `C:/`) when there is room for it.
    let head_len = s
        .find('/')
        .map(|i| i + 1)
        .filter(|&h| h + 8 < width)
        .unwrap_or(0);
    let tail_len = width - head_len - 1;
    let head: String = chars[..head_len].iter().collect();
    let mut tail: String = chars[chars.len() - tail_len..].iter().collect();
    // Prefer cutting on a component boundary: `~/…/Projects/x` over `~/…jects/x`.
    if let Some(i) = tail.find('/').filter(|&i| i > 0 && i + 1 < tail.len()) {
        tail = tail[i..].to_string();
    }
    format!("{head}{ELLIPSIS}{tail}")
}

// ── Formatting ────────────────────────────────────────────────────────────────

/// Decimal size with three significant digits: `999 B`, `4.53 GB`, `60.2 GB`, `472 MB`.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1_000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = "B";
    for u in UNITS {
        if value < 999.5 {
            break;
        }
        value /= 1_000.0;
        unit = u;
    }
    if value < 9.995 {
        format!("{value:.2} {unit}")
    } else if value < 99.95 {
        format!("{value:.1} {unit}")
    } else {
        format!("{value:.0} {unit}")
    }
}

pub fn human_age(age: Duration) -> String {
    let days = age.as_secs() / 86_400;
    let plural = |n: u64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    match days {
        0 => "today".to_string(),
        1..=13 => plural(days, "day"),
        14..=59 => plural(days / 7, "week"),
        60..=729 => plural(days / 30, "month"),
        _ => plural(days / 365, "year"),
    }
}

pub fn human_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 10.0 {
        format!("{secs:.2}s")
    } else if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        format!("{}m {:02}s", d.as_secs() / 60, d.as_secs() % 60)
    }
}

pub fn fmt_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn plural(n: usize, word: &str) -> String {
    format!(
        "{} {word}{}",
        fmt_count(n as u64),
        if n == 1 { "" } else { "s" }
    )
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Path relative to home (`~/…`), always with forward slashes.
pub fn display_path(path: &Path) -> String {
    let raw = match home_dir().and_then(|h| path.strip_prefix(h).ok().map(Path::to_path_buf)) {
        Some(rel) if rel.as_os_str().is_empty() => "~".to_string(),
        Some(rel) => format!("~/{}", rel.display()),
        None => path.display().to_string(),
    };
    raw.replace('\\', "/")
}

/// Horizontal proportional bar: coloured filled part on a dim track.
pub fn bar(ratio: f64, width: usize, fill: &Style) -> String {
    let halves = (ratio.clamp(0.0, 1.0) * (width * 2) as f64).round() as usize;
    let full = halves / 2;
    let half = halves % 2 == 1;
    let mut filled = BAR_FULL.repeat(full);
    if half {
        filled.push_str(BAR_HALF);
    }
    let rest = width - full - usize::from(half);
    format!(
        "{}{}",
        fill.apply_to(filled),
        track().apply_to(BAR_FULL.repeat(rest))
    )
}

// ── Messages ──────────────────────────────────────────────────────────────────

pub fn rule(width: usize) -> String {
    format!("{MARGIN}{}", track().apply_to(RULE.repeat(width)))
}

pub fn hint(cmd: &str, desc: &str) {
    println!(
        "{MARGIN}{} {}  {}",
        style(ARROW).dim(),
        accent().apply_to(format!("{cmd:<22}")),
        style(desc).dim()
    );
}

pub fn success(msg: &str) {
    println!(
        "{MARGIN}{} {}",
        style(CHECK).green().bold(),
        style(msg).bold()
    );
}

pub fn warn(msg: &str) {
    println!("{MARGIN}{} {}", style(WARN).yellow().bold(), msg);
}

pub fn failure(path: &str, reason: &str) {
    println!(
        "{MARGIN}{} {}  {}",
        style(CROSS).red().bold(),
        path,
        style(reason).dim()
    );
}

pub fn note(msg: &str) {
    println!("{MARGIN}  {}", style(msg).dim());
}

pub fn error(msg: &str) {
    eprintln!(
        "\n{MARGIN}{} {} {msg}\n",
        style(CROSS).red().bold(),
        style("error:").red().bold()
    );
}

/// The ` gluttony ` badge followed by dim context fields.
pub fn header(fields: &[String]) {
    // No background colour: its contrast depends too much on the terminal theme.
    let badge = format!(
        "{} {}",
        accent().bold().apply_to("▍"),
        style("gluttony").bold()
    );
    let version = style(env!("CARGO_PKG_VERSION")).dim();
    let rest = fields
        .iter()
        .map(|f| style(f).dim().to_string())
        .collect::<Vec<_>>()
        .join(&format!(" {} ", style(DOT).dim()));
    println!("\n{MARGIN}{badge} {version}  {rest}\n");
}

// ── Prompts ───────────────────────────────────────────────────────────────────

fn read_line() -> io::Result<String> {
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Single-keystroke yes/no prompt, defaulting to "no".
pub fn confirm(prompt: &str) -> io::Result<bool> {
    let term = Term::stderr();
    let label = format!(
        "{MARGIN}{} {}",
        accent().bold().apply_to(ASK),
        style(prompt).bold()
    );
    let choices = style("y/N").dim();
    if !term.is_term() {
        eprint!("{label}  {choices} ");
        io::stderr().flush()?;
        return Ok(matches!(read_line()?.to_lowercase().as_str(), "y" | "yes"));
    }
    term.write_str(&format!("{label}  {choices} "))?;
    let yes = matches!(term.read_key_raw()?, Key::Char('y' | 'Y'));
    let answer = if yes {
        style("yes").green()
    } else {
        style("no").dim()
    };
    term.clear_line()?;
    term.write_line(&format!("{label}  {answer}"))?;
    Ok(yes)
}

/// Asks the user to type `word` to confirm an irreversible action.
pub fn confirm_typed(prompt: &str, word: &str) -> io::Result<bool> {
    eprint!(
        "{MARGIN}{} {}  {} {} {} ",
        style(ASK).red().bold(),
        style(prompt).bold(),
        style("type").dim(),
        style(word).red().bold(),
        style("to confirm:").dim()
    );
    io::stderr().flush()?;
    Ok(read_line()? == word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_three_significant_digits() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1_000), "1.00 KB");
        assert_eq!(human_size(1_500_000), "1.50 MB");
        assert_eq!(human_size(12_340_000), "12.3 MB");
        assert_eq!(human_size(471_810_000), "472 MB");
        assert_eq!(human_size(999_700_000), "1.00 GB");
        assert_eq!(human_size(60_180_000_000), "60.2 GB");
    }

    #[test]
    fn fmt_count_adds_separators() {
        assert_eq!(fmt_count(0), "0");
        assert_eq!(fmt_count(999), "999");
        assert_eq!(fmt_count(1_000), "1,000");
        assert_eq!(fmt_count(1_697), "1,697");
        assert_eq!(fmt_count(1_000_000), "1,000,000");
    }

    #[test]
    fn human_age_buckets() {
        let d = |n: u64| Duration::from_secs(n * 86_400);
        assert_eq!(human_age(d(0)), "today");
        assert_eq!(human_age(d(1)), "1 day");
        assert_eq!(human_age(d(20)), "2 weeks");
        assert_eq!(human_age(d(95)), "3 months");
        assert_eq!(human_age(d(800)), "2 years");
    }

    #[test]
    fn truncate_middle_keeps_root_and_tail() {
        let p = "~/Documents/Projects/watch-party/apps/app/.next";
        assert_eq!(truncate_middle(p, 100), p);
        let t = truncate_middle(p, 30);
        assert!(t.chars().count() <= 30);
        assert!(t.starts_with("~/…"));
        assert!(t.ends_with("apps/app/.next"));
    }

    #[test]
    fn bounce_frames_are_steady() {
        let frames = bounce_frames(&accent());
        let (last, moving) = frames.split_last().unwrap();
        assert!(last.is_empty(), "final frame clears the loader");
        assert_eq!(moving.len(), 2 * (BOUNCE_TRACK - BOUNCE_SEGMENT));
        for f in moving {
            assert_eq!(measure_text_width(f), BOUNCE_TRACK);
        }
    }

    #[test]
    fn bar_has_exact_width() {
        let s = Style::new();
        for r in [0.0, 0.03, 0.5, 0.97, 1.0] {
            assert_eq!(measure_text_width(&bar(r, 20, &s)), 20);
        }
    }
}
