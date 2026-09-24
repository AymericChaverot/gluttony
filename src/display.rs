use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use console::style;
use serde::Serialize;

use crate::scanner::{Artifact, ArtifactKind, Ecosystem, ScanReport, docker::DockerInfo};
use crate::trash::TrashStats;
use crate::ui::{self, BULLET, DOT, MARGIN, human_size, plural};

const LABEL_W: usize = 18;
const COUNT_W: usize = 6;
const SIZE_W: usize = 8;
const SHARE_W: usize = 5;
const AGE_W: usize = 10;

/// Projects untouched for this long are considered stale.
pub const STALE_AFTER: Duration = Duration::from_secs(90 * 86_400);

// ── Grouping ──────────────────────────────────────────────────────────────────

struct Group {
    kind: ArtifactKind,
    count: usize,
    total: u64,
}

fn group_by_kind(artifacts: &[&Artifact]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for a in artifacts {
        match groups.iter_mut().find(|g| g.kind == a.kind) {
            Some(g) => {
                g.count += 1;
                g.total += a.size;
            }
            None => groups.push(Group {
                kind: a.kind,
                count: 1,
                total: a.size,
            }),
        }
    }
    groups.sort_by_key(|g| Reverse(g.total));
    groups
}

pub struct Project<'a> {
    pub path: Option<PathBuf>,
    pub artifacts: Vec<&'a Artifact>,
    pub total: u64,
    pub last_active: Option<SystemTime>,
}

pub fn group_by_project(artifacts: &[Artifact]) -> Vec<Project<'_>> {
    let mut index: HashMap<Option<PathBuf>, usize> = HashMap::new();
    let mut projects: Vec<Project> = Vec::new();
    for a in artifacts {
        let i = *index.entry(a.project.clone()).or_insert_with(|| {
            projects.push(Project {
                path: a.project.clone(),
                artifacts: Vec::new(),
                total: 0,
                last_active: None,
            });
            projects.len() - 1
        });
        let p = &mut projects[i];
        p.artifacts.push(a);
        p.total += a.size;
        p.last_active = p.last_active.max(a.last_active);
    }
    projects.sort_by_key(|p| Reverse(p.total));
    projects
}

fn project_count(artifacts: &[Artifact]) -> usize {
    let mut seen: Vec<&PathBuf> = artifacts
        .iter()
        .filter_map(|a| a.project.as_ref())
        .collect();
    seen.sort();
    seen.dedup();
    seen.len()
}

// ── Cells ─────────────────────────────────────────────────────────────────────

fn kind_cell(kind: ArtifactKind, width: usize, dot: bool) -> String {
    let eco = ui::eco_style(kind.ecosystem());
    let label = eco.apply_to(format!("{:<w$}", kind.label(), w = width));
    if dot {
        format!("{} {label}", eco.apply_to(BULLET))
    } else {
        label.to_string()
    }
}

fn size_cell(bytes: u64) -> String {
    ui::size_style(bytes)
        .apply_to(format!("{:>SIZE_W$}", human_size(bytes)))
        .to_string()
}

fn column_header(cols: &[(&str, usize, bool)]) -> String {
    let cells: Vec<String> = cols
        .iter()
        .map(|(name, w, right)| {
            if *right {
                format!("{name:>w$}")
            } else {
                format!("{name:<w$}")
            }
        })
        .collect();
    format!("{MARGIN}{}", style(cells.join("  ")).dim())
}

/// One artefact as a table line: kind, size, age and a middle-elided path.
pub fn artifact_line(a: &Artifact, width: usize) -> String {
    let fixed = LABEL_W + 2 + SIZE_W + 2 + AGE_W + 2;
    let path = ui::truncate_middle(&ui::display_path(&a.path), width.saturating_sub(fixed));
    format!(
        "{}  {}  {}  {}",
        kind_cell(a.kind, LABEL_W, false),
        size_cell(a.size),
        ui::age_cell(a.age(), AGE_W),
        style(path).dim()
    )
}

// ── Scan summary ──────────────────────────────────────────────────────────────

pub fn print_header(report: &ScanReport) {
    ui::header(&[
        ui::truncate_middle(
            &ui::display_path(&report.root),
            ui::width().saturating_sub(48),
        ),
        format!("{} entries", ui::fmt_count(report.entries)),
        ui::human_duration(report.elapsed),
    ]);
}

pub fn print_summary(report: &ScanReport, trash: &TrashStats, filtered: bool) {
    let artifacts = &report.artifacts;
    let width = ui::width() - MARGIN.len() * 2;

    if artifacts.is_empty() {
        let msg = if filtered {
            "Nothing matches these filters."
        } else {
            "Spotless. No dev artefacts found."
        };
        ui::success(msg);
        println!();
    } else {
        let total = report.total();
        let projects = project_count(artifacts);
        println!(
            "{MARGIN}{}  {}\n",
            ui::size_style(total).bold().apply_to(human_size(total)),
            style(format!(
                "reclaimable across {}{}",
                plural(artifacts.len(), "artefact"),
                if projects > 0 {
                    format!(" in {}", plural(projects, "project"))
                } else {
                    String::new()
                }
            ))
            .dim()
        );
        print_kind_table(artifacts, total, width);
    }

    if let Some(docker) = &report.docker {
        print_docker(docker);
    }
    if trash.sessions > 0 {
        print_trash_line(trash);
    }
    if trash.purged > 0 {
        ui::note(&format!(
            "{} of expired trash was purged automatically",
            human_size(trash.purged)
        ));
        println!();
    }
}

fn print_kind_table(artifacts: &[Artifact], total: u64, width: usize) {
    let refs: Vec<&Artifact> = artifacts.iter().collect();
    let groups = group_by_kind(&refs);
    let max = groups.first().map_or(0, |g| g.total);
    let fixed = 2 + LABEL_W + 2 + COUNT_W + 2 + SIZE_W + 2 + SHARE_W + 2;
    let bar_w = width.saturating_sub(fixed).clamp(10, 36);
    let table_w = fixed + bar_w;

    println!(
        "{}",
        column_header(&[
            ("  TYPE", LABEL_W + 2, false),
            ("COUNT", COUNT_W, true),
            ("SIZE", SIZE_W, true),
            ("SHARE", SHARE_W, true),
        ])
    );
    println!("{}", ui::rule(table_w));
    for g in &groups {
        let share = if total == 0 {
            0.0
        } else {
            g.total as f64 / total as f64
        };
        let ratio = if max == 0 {
            0.0
        } else {
            g.total as f64 / max as f64
        };
        println!(
            "{MARGIN}{}  {}  {}  {}  {}",
            kind_cell(g.kind, LABEL_W, true),
            style(format!("{:>COUNT_W$}", ui::fmt_count(g.count as u64))).dim(),
            size_cell(g.total),
            style(format!(
                "{:>w$}",
                format!("{:.0}%", share * 100.0),
                w = SHARE_W
            ))
            .dim(),
            ui::bar(ratio, bar_w, &ui::eco_style(g.kind.ecosystem())),
        );
    }
    println!("{}", ui::rule(table_w));
    println!(
        "{MARGIN}  {}  {}  {}\n",
        style(format!("{:<LABEL_W$}", "total")).bold(),
        style(format!(
            "{:>COUNT_W$}",
            ui::fmt_count(artifacts.len() as u64)
        ))
        .bold(),
        ui::size_style(total)
            .bold()
            .apply_to(format!("{:>SIZE_W$}", human_size(total))),
    );
}

fn print_docker(docker: &DockerInfo) {
    let mut parts = vec![format!("{} on disk", human_size(docker.disk_size))];
    if let Some(r) = docker.reclaimable.filter(|r| *r > 0) {
        parts.push(format!(
            "{} prunable",
            ui::size_style(r).apply_to(human_size(r))
        ));
    }
    let line = format!(
        "{}  {}",
        style("Docker").bold(),
        parts.join(&format!(" {} ", style(DOT).dim()))
    );
    // Nothing worth pruning: keep it to a quiet one-liner.
    if docker.reclaimable.is_some_and(|r| r < 100_000_000) {
        println!("{MARGIN}{} {line}\n", style("○").dim());
        return;
    }
    ui::warn(&line);
    ui::note(if docker.reclaimable.is_some() {
        "left alone on purpose: prune it through Docker itself"
    } else {
        "not running, start it to see what can be pruned"
    });
    println!(
        "{MARGIN}  {} {}\n",
        style(ui::ARROW).dim(),
        ui::accent().apply_to("docker system prune -a")
    );
}

fn print_trash_line(trash: &TrashStats) {
    println!(
        "{MARGIN}{} {}  {} in {} {}\n",
        style("○").dim(),
        style("trash").bold(),
        human_size(trash.size),
        plural(trash.sessions, "session"),
        style(format!(
            "{DOT} still on disk, run `gluttony trash empty` to free it"
        ))
        .dim()
    );
}

pub fn print_hints(report: &ScanReport, list: bool, projects: bool) {
    if report.artifacts.is_empty() {
        return;
    }
    ui::hint("gluttony clean", "pick what to remove");
    if !list {
        ui::hint("gluttony --list", "every path, with last activity");
    }
    if !projects {
        ui::hint("gluttony --projects", "grouped by project");
    }
    println!();
}

// ── Detailed views ────────────────────────────────────────────────────────────

pub fn print_list(artifacts: &[Artifact]) {
    if artifacts.is_empty() {
        return;
    }
    let width = ui::width() - MARGIN.len() * 2;
    println!(
        "{}",
        column_header(&[
            ("TYPE", LABEL_W, false),
            ("SIZE", SIZE_W, true),
            ("ACTIVE", AGE_W, false),
            ("PATH", 4, false),
        ])
    );
    println!("{}", ui::rule(width));
    for a in artifacts {
        println!("{MARGIN}{}", artifact_line(a, width));
    }
    println!();
}

pub fn print_projects(artifacts: &[Artifact]) {
    if artifacts.is_empty() {
        return;
    }
    let width = ui::width() - MARGIN.len() * 2;
    let projects = group_by_project(artifacts);

    let name_of = |p: &Project| match &p.path {
        Some(path) => ui::display_path(path),
        None => "global caches".to_string(),
    };
    // Align the contents column on the longest name, leaving room for it.
    let room = width.saturating_sub(SIZE_W + 2 + AGE_W + 2);
    let longest = projects
        .iter()
        .map(|p| name_of(p).chars().count())
        .max()
        .unwrap_or(0);
    let widest_kinds = projects
        .iter()
        .map(|p| {
            let mut kinds: Vec<&str> = p.artifacts.iter().map(|a| a.kind.label()).collect();
            kinds.dedup();
            kinds.iter().map(|k| k.len()).sum::<usize>() + kinds.len().saturating_sub(1) * 3
        })
        .max()
        .unwrap_or(0);
    let name_w = longest.min(room.saturating_sub(widest_kinds + 2)).max(16);

    println!(
        "{}",
        column_header(&[
            ("SIZE", SIZE_W, true),
            ("ACTIVE", AGE_W, false),
            ("PROJECT", name_w, false),
            ("CONTENTS", 8, false),
        ])
    );
    println!("{}", ui::rule(width));
    for p in &projects {
        let age = p
            .last_active
            .and_then(|t| SystemTime::now().duration_since(t).ok());
        let mut kinds: Vec<ArtifactKind> = Vec::new();
        for a in &p.artifacts {
            if !kinds.contains(&a.kind) {
                kinds.push(a.kind);
            }
        }
        let kinds_styled = kinds
            .iter()
            .map(|k| ui::eco_style(k.ecosystem()).apply_to(k.label()).to_string())
            .collect::<Vec<_>>()
            .join(&format!(" {} ", style(DOT).dim()));

        let name = format!("{:<name_w$}", ui::truncate_middle(&name_of(p), name_w));
        let name_cell = if p.path.is_some() {
            style(name).to_string()
        } else {
            style(name).italic().dim().to_string()
        };
        let line = format!(
            "{}  {}  {name_cell}  {kinds_styled}",
            size_cell(p.total),
            ui::age_cell(age, AGE_W),
        );
        println!("{MARGIN}{}", ui::fit(&line, width).trim_end());
    }
    println!();
}

// ── JSON ──────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonArtifact<'a> {
    path: &'a PathBuf,
    kind: &'static str,
    ecosystem: Ecosystem,
    size: u64,
    project: Option<&'a PathBuf>,
    last_active: Option<u64>,
}

#[derive(Serialize)]
struct JsonDocker {
    disk_size: u64,
    reclaimable: Option<u64>,
}

#[derive(Serialize)]
struct JsonReport<'a> {
    root: &'a PathBuf,
    total: u64,
    elapsed_ms: u128,
    artifacts: Vec<JsonArtifact<'a>>,
    docker: Option<JsonDocker>,
    trash: &'a TrashStats,
}

pub fn print_json(report: &ScanReport, trash: &TrashStats) -> crate::error::Result<()> {
    let out = JsonReport {
        root: &report.root,
        total: report.total(),
        elapsed_ms: report.elapsed.as_millis(),
        artifacts: report
            .artifacts
            .iter()
            .map(|a| JsonArtifact {
                path: &a.path,
                kind: a.kind.label(),
                ecosystem: a.kind.ecosystem(),
                size: a.size,
                project: a.project.as_ref(),
                last_active: a
                    .last_active
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
            })
            .collect(),
        docker: report.docker.as_ref().map(|d| JsonDocker {
            disk_size: d.disk_size,
            reclaimable: d.reclaimable,
        }),
        trash,
    };
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

// ── Removal plan ──────────────────────────────────────────────────────────────

/// Grouped overview of what is about to be removed.
pub fn print_plan(artifacts: &[&Artifact], show_all_paths: bool) {
    let width = ui::width() - MARGIN.len() * 2;
    for g in group_by_kind(artifacts) {
        println!(
            "{MARGIN}{}  {}  {}",
            kind_cell(g.kind, LABEL_W, true),
            style(format!("{:>4}", g.count)).dim(),
            size_cell(g.total),
        );
        let paths: Vec<&&Artifact> = artifacts.iter().filter(|a| a.kind == g.kind).collect();
        let shown = if show_all_paths { paths.len() } else { 3 };
        for a in paths.iter().take(shown) {
            let path = ui::truncate_middle(&ui::display_path(&a.path), width - 14);
            println!(
                "{MARGIN}    {}  {}",
                style(format!("{:>8}", human_size(a.size))).dim(),
                style(path).dim()
            );
        }
        if paths.len() > shown {
            ui::note(&format!(
                "  {} and {} more",
                ui::ELLIPSIS,
                paths.len() - shown
            ));
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(project: Option<&str>, kind: ArtifactKind, size: u64) -> Artifact {
        Artifact {
            path: PathBuf::from(format!("/p/{}/{}", project.unwrap_or("g"), kind.label())),
            kind,
            size,
            project: project.map(PathBuf::from),
            last_active: None,
        }
    }

    #[test]
    fn groups_by_project_sorted_by_size() {
        let arts = vec![
            art(Some("a"), ArtifactKind::NodeModules, 10),
            art(Some("b"), ArtifactKind::CargoTarget, 50),
            art(Some("a"), ArtifactKind::NextCache, 5),
            art(None, ArtifactKind::CargoRegistry, 1),
        ];
        let projects = group_by_project(&arts);
        assert_eq!(projects.len(), 3);
        assert_eq!(projects[0].total, 50);
        assert_eq!(projects[1].total, 15);
        assert_eq!(projects[1].artifacts.len(), 2);
        assert!(projects[2].path.is_none());
        assert_eq!(project_count(&arts), 2);
    }

    #[test]
    fn groups_by_kind_sorted_by_size() {
        let arts = [
            art(Some("a"), ArtifactKind::NodeModules, 10),
            art(Some("b"), ArtifactKind::NodeModules, 10),
            art(Some("c"), ArtifactKind::CargoTarget, 15),
        ];
        let refs: Vec<&Artifact> = arts.iter().collect();
        let groups = group_by_kind(&refs);
        assert_eq!(groups[0].kind, ArtifactKind::NodeModules);
        assert_eq!(groups[0].count, 2);
        assert_eq!(groups[1].total, 15);
    }
}
