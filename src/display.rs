use std::cmp::Reverse;
use std::path::Path;

use console::{style, Style, StyledObject};
use dialoguer::theme::ColorfulTheme;

use crate::scanner::Artifact;

const TYPE_W: usize = 18;
const COUNT_W: usize = 7;
const SIZE_W: usize = 10;
const BAR_W: usize = 20;
const ROW_W: usize = TYPE_W + 2 + COUNT_W + 2 + SIZE_W + 2 + BAR_W;

// ── Theme ─────────────────────────────────────────────────────────────────────

pub fn app_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_style: Style::new().bold(),
        active_item_style: Style::new().cyan(),
        inactive_item_style: Style::new(),
        checked_item_prefix: style("[x]".to_string()).green(),
        unchecked_item_prefix: style("[ ]".to_string()).dim(),
        active_item_prefix: style("> ".to_string()).cyan(),
        inactive_item_prefix: style("  ".to_string()),
        ..ColorfulTheme::default()
    }
}

// ── Thresholds ────────────────────────────────────────────────────────────────

const RED_THRESHOLD: u64 = 1_000_000_000;
const YELLOW_THRESHOLD: u64 = 100_000_000;

fn size_style(bytes: u64, text: String) -> StyledObject<String> {
    if bytes >= RED_THRESHOLD {
        style(text).red().bold()
    } else if bytes >= YELLOW_THRESHOLD {
        style(text).yellow()
    } else {
        style(text).dim()
    }
}

// ── Grouping ──────────────────────────────────────────────────────────────────

struct Group {
    label: &'static str,
    count: usize,
    total: u64,
}

fn group_artifacts(artifacts: &[Artifact]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for a in artifacts {
        match groups.iter_mut().find(|g| g.label == a.kind.label()) {
            Some(g) => {
                g.count += 1;
                g.total += a.size;
            }
            None => groups.push(Group {
                label: a.kind.label(),
                count: 1,
                total: a.size,
            }),
        }
    }
    groups.sort_by_key(|g| Reverse(g.total));
    groups
}

// ── Formatting helpers ────────────────────────────────────────────────────────

pub fn human_size(bytes: u64) -> String {
    const GB: u64 = 1_000_000_000;
    const MB: u64 = 1_000_000;
    const KB: u64 = 1_000;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub(crate) fn fmt_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

// Returns the number of filled cells for a proportional bar.
// indicatif handles animated bars; this is only for static table display.
fn bar_cells(size: u64, max: u64) -> usize {
    if max == 0 {
        return 0;
    }
    ((size as f64 / max as f64) * BAR_W as f64).round() as usize
}

// ── Public interface ──────────────────────────────────────────────────────────

pub fn print_results(artifacts: &[Artifact], elapsed: std::time::Duration) {
    if artifacts.is_empty() {
        println!("{}", style("No dev artefacts found.").dim());
        return;
    }

    let total: u64 = artifacts.iter().map(|a| a.size).sum();
    let count = artifacts.len();
    let groups = group_artifacts(artifacts);
    let max_size = groups.first().map_or(0, |g| g.total);

    // Header
    let elapsed_str = format!("{:.2}s", elapsed.as_secs_f64());
    let header = size_style(
        total,
        format!(
            "  {} reclaimable — {} artefact{} found in {}",
            human_size(total),
            fmt_count(count as u64),
            if count == 1 { "" } else { "s" },
            elapsed_str
        ),
    );
    println!("\n{header}\n");

    // Column headers
    let type_hdr = style(format!("{:<TYPE_W$}", "TYPE")).dim();
    let count_hdr = style(format!("{:>COUNT_W$}", "COUNT")).dim();
    let size_hdr = style(format!("{:>SIZE_W$}", "SIZE")).dim();
    println!("  {type_hdr}  {count_hdr}  {size_hdr}");

    let sep = style("─".repeat(ROW_W)).dim();
    println!("  {sep}");

    for g in &groups {
        let type_col = format!("{:<TYPE_W$}", g.label);
        let count_col = style(format!("{:>COUNT_W$}", fmt_count(g.count as u64))).dim();
        let size_col = size_style(g.total, format!("{:>SIZE_W$}", human_size(g.total)));

        let filled = bar_cells(g.total, max_size).min(BAR_W);
        let bar_on = size_style(g.total, "█".repeat(filled));
        let bar_off = style("░".repeat(BAR_W - filled)).dim();

        println!("  {type_col}  {count_col}  {size_col}  {bar_on}{bar_off}");
    }

    println!("  {sep}");

    let total_label = style(format!("{:<TYPE_W$}", "Total")).bold();
    let total_count = style(format!("{:>COUNT_W$}", fmt_count(count as u64))).dim().bold();
    let total_size = size_style(total, format!("{:>SIZE_W$}", human_size(total))).bold();
    println!("  {total_label}  {total_count}  {total_size}\n");
}

pub fn print_no_clean_hint() {
    println!("{}", style("  -> Run with --clean to remove them.").dim());
}

pub fn display_path(path: &Path) -> String {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);
    let raw = match home {
        Some(h) => match path.strip_prefix(&h) {
            Ok(rel) => format!("~/{}", rel.display()),
            Err(_) => path.display().to_string(),
        },
        None => path.display().to_string(),
    };
    raw.replace('\\', "/")
}

pub fn print_list(artifacts: &[Artifact]) {
    if artifacts.is_empty() {
        return;
    }
    println!();
    for (i, a) in artifacts.iter().enumerate() {
        println!(
            "  {:>4}  {:<18}  {}  {}",
            style(format!("{}", i + 1)).dim(),
            a.kind.label(),
            size_style(a.size, format!("{:>10}", human_size(a.size))),
            style(display_path(&a.path)).dim(),
        );
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_boundaries() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1_000), "1.00 KB");
        assert_eq!(human_size(1_500_000), "1.50 MB");
        assert_eq!(human_size(2_000_000_000), "2.00 GB");
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
    fn bar_cells_proportional() {
        assert_eq!(bar_cells(100, 100), BAR_W);
        assert_eq!(bar_cells(0, 100), 0);
        assert_eq!(bar_cells(50, 100), BAR_W / 2);
        assert_eq!(bar_cells(0, 0), 0);
    }
}
