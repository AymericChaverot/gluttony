//! Reversible removal: artefacts are moved to `~/.gluttony/trash/<session>/`
//! and recorded in a manifest so `gluttony undo` can put them back.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use console::style;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::picker::{self, Row};
use crate::ui::{self, DOT, MARGIN, human_size, plural};

const RETENTION_DAYS: u64 = 30;
const RETENTION_SECS: u64 = RETENTION_DAYS * 86_400;

#[derive(Debug, Serialize, Deserialize)]
pub struct TrashEntry {
    pub original: PathBuf,
    pub kind: String,
    pub size: u64,
    pub index: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrashSession {
    id: String,
    timestamp: u64,
    entries: Vec<TrashEntry>,
}

impl TrashSession {
    fn total_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    fn age_secs(&self) -> u64 {
        now_secs().saturating_sub(self.timestamp)
    }

    fn age_label(&self) -> String {
        let diff = self.age_secs();
        match diff {
            0..60 => "just now".to_string(),
            60..3600 => format!("{} min ago", diff / 60),
            3600..86_400 => format!("{} h ago", diff / 3600),
            _ => {
                let days = diff / 86_400;
                format!("{days} day{} ago", if days == 1 { "" } else { "s" })
            }
        }
    }

    fn expires_label(&self) -> String {
        let left = RETENTION_SECS.saturating_sub(self.age_secs()) / 86_400;
        match left {
            0 => "today".to_string(),
            1 => "in 1 day".to_string(),
            n => format!("in {n} days"),
        }
    }

    fn kinds_summary(&self) -> String {
        let mut kinds: Vec<&str> = Vec::new();
        for e in &self.entries {
            if !kinds.contains(&e.kind.as_str()) {
                kinds.push(&e.kind);
            }
        }
        kinds.join(&format!(" {DOT} "))
    }
}

#[derive(Debug, Default, Serialize)]
pub struct TrashStats {
    pub sessions: usize,
    pub items: usize,
    pub size: u64,
    /// Bytes purged automatically at startup (expired sessions).
    #[serde(skip)]
    pub purged: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn gluttony_dir() -> Option<PathBuf> {
    ui::home_dir().map(|h| h.join(".gluttony"))
}

fn trash_root() -> Option<PathBuf> {
    gluttony_dir().map(|d| d.join("trash"))
}

fn manifest_path() -> Option<PathBuf> {
    gluttony_dir().map(|d| d.join("manifest.json"))
}

fn session_trash_dir(session_id: &str) -> Option<PathBuf> {
    trash_root().map(|d| d.join(session_id))
}

fn home_error() -> io::Error {
    io::Error::other("cannot determine home directory")
}

fn read_manifest() -> Vec<TrashSession> {
    manifest_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Writes the manifest atomically (temp file + rename) so a crash can never
/// leave a truncated manifest behind.
fn write_manifest(sessions: &[TrashSession]) -> Result<()> {
    let path = manifest_path().ok_or_else(home_error)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(sessions)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

fn stats_of(sessions: &[TrashSession]) -> TrashStats {
    TrashStats {
        sessions: sessions.len(),
        items: sessions.iter().map(|s| s.entries.len()).sum(),
        size: sessions.iter().map(TrashSession::total_size).sum(),
        purged: 0,
    }
}

pub fn stats() -> TrashStats {
    stats_of(&read_manifest())
}

/// Drops sessions past the retention period. Returns what was purged.
fn purge_old(sessions: &mut Vec<TrashSession>) -> TrashStats {
    let (expired, kept): (Vec<_>, Vec<_>) = std::mem::take(sessions)
        .into_iter()
        .partition(|s| s.age_secs() > RETENTION_SECS);
    for s in &expired {
        if let Some(dir) = session_trash_dir(&s.id) {
            let _ = fs::remove_dir_all(dir);
        }
    }
    *sessions = kept;
    stats_of(&expired)
}

/// Purges expired sessions; run on every invocation so the trash never grows
/// unbounded, even when `clean` is not used again.
pub fn auto_purge() -> TrashStats {
    let mut sessions = read_manifest();
    let purged = purge_old(&mut sessions);
    if purged.sessions > 0 {
        let _ = write_manifest(&sessions);
    }
    purged
}

// ── Moving files ──────────────────────────────────────────────────────────────

fn is_cross_device(e: &io::Error) -> bool {
    // EXDEV on Unix, ERROR_NOT_SAME_DEVICE on Windows.
    #[cfg(unix)]
    const CODE: i32 = 18;
    #[cfg(windows)]
    const CODE: i32 = 17;
    #[cfg(not(any(unix, windows)))]
    const CODE: i32 = -1;
    e.raw_os_error() == Some(CODE)
}

#[cfg(unix)]
fn copy_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(fs::read_link(src)?, dst)
}

#[cfg(windows)]
fn copy_symlink(src: &Path, dst: &Path) -> io::Result<()> {
    let target = fs::read_link(src)?;
    if fs::metadata(src).is_ok_and(|m| m.is_dir()) {
        std::os::windows::fs::symlink_dir(target, dst)
    } else {
        std::os::windows::fs::symlink_file(target, dst)
    }
}

#[cfg(not(any(unix, windows)))]
fn copy_symlink(_src: &Path, _dst: &Path) -> io::Result<()> {
    Err(io::Error::other(
        "symlinks are not supported on this platform",
    ))
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if kind.is_symlink() {
            copy_symlink(&entry.path(), &dst_path)?;
        } else if kind.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

/// Moves a directory. Never merges into an existing destination, and only
/// falls back to copy + delete when the move crosses filesystems (a locked
/// file must fail fast rather than trigger a slow partial copy).
fn move_dir(src: &Path, dst: &Path) -> io::Result<()> {
    if dst.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device(&e) => {
            if let Err(e) = copy_dir_all(src, dst) {
                let _ = fs::remove_dir_all(dst);
                return Err(e);
            }
            // The copy is complete; a failure here leaves a partial source
            // behind but the data is safe in the destination.
            let _ = fs::remove_dir_all(src);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Creates a new, empty session directory and returns (session_id, session_dir).
pub fn create_session() -> Result<(String, PathBuf)> {
    let root = trash_root().ok_or_else(home_error)?;
    let base = now_secs().to_string();
    let mut id = base.clone();
    let mut n = 1;
    while root.join(&id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    let dir = root.join(&id);
    fs::create_dir_all(&dir)?;
    Ok((id, dir))
}

/// Moves a single artefact into the session directory at the given index slot.
pub fn move_artifact(src: &Path, session_dir: &Path, index: usize) -> io::Result<()> {
    move_dir(src, &session_dir.join(index.to_string()))
}

/// Records the session in the manifest. Sessions with no entries are discarded.
pub fn commit_session(session_id: &str, entries: Vec<TrashEntry>) -> Result<()> {
    if entries.is_empty() {
        if let Some(dir) = session_trash_dir(session_id) {
            let _ = fs::remove_dir_all(dir);
        }
        return Ok(());
    }
    let mut sessions = read_manifest();
    purge_old(&mut sessions);
    sessions.push(TrashSession {
        id: session_id.to_string(),
        timestamp: now_secs(),
        entries,
    });
    write_manifest(&sessions)
}

// ── Commands ──────────────────────────────────────────────────────────────────

fn sorted_sessions() -> Vec<TrashSession> {
    let mut sessions = read_manifest();
    sessions.sort_by_key(|s| std::cmp::Reverse(s.timestamp));
    sessions
}

fn session_line(s: &TrashSession, width: usize) -> String {
    let line = format!(
        "{:<12}  {:>9}  {}  {:<12}  {}",
        s.age_label(),
        style(plural(s.entries.len(), "item")).dim(),
        ui::size_style(s.total_size()).apply_to(format!("{:>8}", human_size(s.total_size()))),
        style(format!("exp. {}", s.expires_label())).dim(),
        style(s.kinds_summary()).dim(),
    );
    ui::fit(&line, width).trim_end().to_string()
}

/// `gluttony trash`: overview of what the trash holds.
pub fn show() -> Result<()> {
    let sessions = sorted_sessions();
    let stats = stats_of(&sessions);
    let location = trash_root()
        .map(|p| ui::display_path(&p))
        .unwrap_or_default();
    ui::header(&["trash".to_string(), location]);

    if sessions.is_empty() {
        ui::success("The trash is empty.");
        println!();
        return Ok(());
    }

    println!(
        "{MARGIN}{}  {}\n",
        ui::size_style(stats.size)
            .bold()
            .apply_to(human_size(stats.size)),
        style(format!(
            "held in {} across {} {DOT} kept {RETENTION_DAYS} days",
            plural(stats.items, "item"),
            plural(stats.sessions, "session"),
        ))
        .dim()
    );
    let width = ui::width() - MARGIN.len() * 2;
    for s in &sessions {
        println!("{MARGIN}{}", session_line(s, width));
    }
    println!();
    ui::hint("gluttony undo", "restore a session");
    ui::hint("gluttony trash empty", "free the space for good");
    println!();
    Ok(())
}

/// `gluttony undo`: pick a session and move everything back.
pub fn undo_interactive() -> Result<()> {
    let sessions = sorted_sessions();
    ui::header(&["undo".to_string()]);

    if sessions.is_empty() {
        ui::success("Nothing to restore, the trash is empty.");
        println!();
        return Ok(());
    }

    let width = picker::row_width();
    let rows: Vec<Row> = sessions
        .iter()
        .map(|s| Row {
            line: session_line(s, width),
            search: format!(
                "{} {DOT} {} {DOT} {}",
                s.age_label(),
                plural(s.entries.len(), "item"),
                human_size(s.total_size())
            ),
            size: s.total_size(),
            stale: false,
            checked: false,
        })
        .collect();

    let Some(idx) = picker::select("Restore which session?", &rows)? else {
        println!();
        return Ok(());
    };
    println!();
    restore_session(&sessions[idx])
}

fn restore_session(session: &TrashSession) -> Result<()> {
    let session_dir = session_trash_dir(&session.id).ok_or_else(home_error)?;

    let mut failed: Vec<TrashEntry> = Vec::new();
    let mut restored = 0usize;
    let mut restored_size = 0u64;

    for entry in &session.entries {
        let src = session_dir.join(entry.index.to_string());
        let shown = ui::display_path(&entry.original);
        let result = if src.exists() {
            move_dir(&src, &entry.original)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "missing from the trash",
            ))
        };
        match result {
            Ok(()) => {
                restored += 1;
                restored_size += entry.size;
                println!(
                    "{MARGIN}{} {}",
                    style(ui::CHECK).green(),
                    style(shown).dim()
                );
            }
            Err(e) => {
                ui::failure(&shown, &e.to_string());
                if e.kind() != io::ErrorKind::NotFound {
                    failed.push(TrashEntry {
                        original: entry.original.clone(),
                        kind: entry.kind.clone(),
                        size: entry.size,
                        index: entry.index,
                    });
                }
            }
        }
    }

    // Keep entries that could not be restored so they stay recoverable and
    // are still purged once they expire.
    let mut all = read_manifest();
    let remaining = failed.len();
    if failed.is_empty() {
        all.retain(|s| s.id != session.id);
        let _ = fs::remove_dir_all(&session_dir);
    } else if let Some(s) = all.iter_mut().find(|s| s.id == session.id) {
        s.entries = failed;
    }
    write_manifest(&all)?;

    println!();
    if remaining == 0 {
        ui::success(&format!(
            "Restored {} ({}).",
            plural(restored, "artefact"),
            human_size(restored_size)
        ));
    } else {
        ui::warn(&format!(
            "Restored {}, {} left in the trash.",
            plural(restored, "artefact"),
            remaining
        ));
        ui::note("free the destination, then run `gluttony undo` again");
    }
    println!();
    Ok(())
}

/// `gluttony trash empty`: permanently delete everything in the trash.
pub fn empty_trash(yes: bool) -> Result<()> {
    let mut sessions = read_manifest();
    purge_old(&mut sessions);
    ui::header(&["empty trash".to_string()]);

    if sessions.is_empty() {
        write_manifest(&[])?;
        ui::success("The trash is already empty.");
        println!();
        return Ok(());
    }

    let stats = stats_of(&sessions);
    println!(
        "{MARGIN}{}  {}\n",
        ui::size_style(stats.size)
            .bold()
            .apply_to(human_size(stats.size)),
        style(format!(
            "in {} across {}",
            plural(stats.items, "item"),
            plural(stats.sessions, "session")
        ))
        .dim()
    );
    println!(
        "{MARGIN}{}",
        style(format!(
            " {} This is permanent. Emptied items cannot be restored. ",
            ui::WARN
        ))
        .white()
        .on_red()
        .bold()
    );
    println!();

    if !yes && !ui::confirm_typed("Empty the trash?", "empty")? {
        println!();
        ui::note("aborted, nothing was deleted");
        println!();
        return Ok(());
    }

    let mut freed = 0u64;
    let mut kept: Vec<TrashSession> = Vec::new();
    for session in sessions {
        let Some(dir) = session_trash_dir(&session.id) else {
            continue;
        };
        match fs::remove_dir_all(&dir) {
            Ok(()) => freed += session.total_size(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                ui::failure(&ui::display_path(&dir), &e.to_string());
                kept.push(session);
            }
        }
    }
    write_manifest(&kept)?;

    println!();
    if kept.is_empty() {
        ui::success(&format!("Trash emptied. {} freed.", human_size(freed)));
    } else {
        ui::warn(&format!(
            "{} freed, {} could not be fully removed.",
            human_size(freed),
            plural(kept.len(), "session")
        ));
    }
    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_dir_refuses_to_merge() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        fs::create_dir_all(src.join("inner")).unwrap();
        fs::create_dir_all(&dst).unwrap();

        let err = move_dir(&src, &dst).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(src.join("inner").exists(), "source must be untouched");
    }

    #[test]
    fn move_dir_moves_contents() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("nested").join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("f.txt"), "x").unwrap();

        move_dir(&src, &dst).unwrap();
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(dst.join("f.txt")).unwrap(), "x");
    }

    #[test]
    fn copy_dir_all_copies_tree() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("a").join("b")).unwrap();
        fs::write(src.join("a").join("b").join("f"), "y").unwrap();

        let dst = dir.path().join("dst");
        copy_dir_all(&src, &dst).unwrap();
        assert_eq!(fs::read_to_string(dst.join("a/b/f")).unwrap(), "y");
    }

    #[test]
    fn purge_partitions_by_age() {
        let fresh = TrashSession {
            id: "fresh-test".into(),
            timestamp: now_secs(),
            entries: vec![],
        };
        let old = TrashSession {
            id: "old-test-does-not-exist".into(),
            timestamp: now_secs() - RETENTION_SECS - 10,
            entries: vec![TrashEntry {
                original: PathBuf::from("/x"),
                kind: "node_modules".into(),
                size: 42,
                index: 0,
            }],
        };
        let mut sessions = vec![fresh, old];
        let purged = purge_old(&mut sessions);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "fresh-test");
        assert_eq!(purged.size, 42);
    }

    #[test]
    fn labels_are_human() {
        let s = TrashSession {
            id: "x".into(),
            timestamp: now_secs() - 2 * 86_400,
            entries: vec![],
        };
        assert_eq!(s.age_label(), "2 days ago");
        assert_eq!(s.expires_label(), "in 28 days");
    }
}
