use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use console::style;
use dialoguer::Select;
use serde::{Deserialize, Serialize};

use crate::display::{app_theme, display_path, human_size};
use crate::error::Result;

const RETENTION_SECS: u64 = 30 * 24 * 3600;

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

    fn age_label(&self) -> String {
        let diff = now_secs().saturating_sub(self.timestamp);
        if diff < 60 {
            "just now".to_string()
        } else if diff < 3600 {
            format!("{} min ago", diff / 60)
        } else if diff < 86400 {
            format!("{} h ago", diff / 3600)
        } else {
            format!("{} days ago", diff / 86400)
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn gluttony_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| PathBuf::from(h).join(".gluttony"))
}

fn manifest_path() -> Option<PathBuf> {
    gluttony_dir().map(|d| d.join("manifest.json"))
}

fn session_trash_dir(session_id: &str) -> Option<PathBuf> {
    gluttony_dir().map(|d| d.join("trash").join(session_id))
}

fn home_error() -> std::io::Error {
    std::io::Error::other("cannot determine home directory")
}

fn read_manifest() -> Vec<TrashSession> {
    manifest_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_manifest(sessions: &[TrashSession]) -> Result<()> {
    let path = manifest_path().ok_or_else(home_error)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(sessions)?)?;
    Ok(())
}

fn purge_old(sessions: &mut Vec<TrashSession>) {
    let cutoff = now_secs().saturating_sub(RETENTION_SECS);
    sessions.retain(|s| {
        if s.timestamp < cutoff {
            if let Some(dir) = session_trash_dir(&s.id) {
                let _ = fs::remove_dir_all(dir);
            }
            false
        } else {
            true
        }
    });
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

fn move_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    // Try a cheap rename first; fall back to copy+delete for cross-device moves.
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    copy_dir_all(src, dst)?;
    fs::remove_dir_all(src)?;
    Ok(())
}

/// Creates a new empty session directory and returns (session_id, session_dir).
pub fn create_session() -> Result<(String, PathBuf)> {
    let id = now_secs().to_string();
    let dir = session_trash_dir(&id).ok_or_else(home_error)?;
    fs::create_dir_all(&dir)?;
    Ok((id, dir))
}

/// Moves a single artefact into the session directory at the given index slot.
pub fn move_artifact(src: &Path, session_dir: &Path, index: usize) -> std::io::Result<()> {
    move_dir(src, &session_dir.join(index.to_string()))
}

/// Writes the session to the manifest and purges sessions older than 30 days.
/// Sessions with no successfully moved entries are silently discarded.
pub fn commit_session(session_id: &str, entries: Vec<TrashEntry>) -> Result<()> {
    if entries.is_empty() {
        // Nothing was moved; clean up the empty session dir.
        if let Some(dir) = session_trash_dir(session_id) {
            let _ = fs::remove_dir_all(dir);
        }
        return Ok(());
    }

    let session = TrashSession {
        id: session_id.to_string(),
        timestamp: now_secs(),
        entries,
    };

    let mut sessions = read_manifest();
    purge_old(&mut sessions);
    sessions.push(session);
    write_manifest(&sessions)
}

/// Interactive picker: lists recorded sessions and restores the one the user selects.
pub fn undo_interactive() -> Result<()> {
    let mut sessions = read_manifest();

    if sessions.is_empty() {
        println!("{}", style("No recorded sessions to restore.").dim());
        return Ok(());
    }

    sessions.sort_by_key(|s| std::cmp::Reverse(s.timestamp));

    let labels: Vec<String> = sessions
        .iter()
        .map(|s| {
            format!(
                "{:<16}  {:>3} artefact{}  {:>10}",
                s.age_label(),
                s.entries.len(),
                if s.entries.len() == 1 { " " } else { "s" },
                human_size(s.total_size()),
            )
        })
        .collect();

    let maybe = Select::with_theme(&app_theme())
        .with_prompt("Select a session to restore")
        .items(&labels)
        .default(0)
        .interact_opt()
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let Some(idx) = maybe else {
        println!("{}", style("Cancelled.").dim());
        return Ok(());
    };

    let session = &sessions[idx];
    restore_session(session)?;

    let session_id = session.id.clone();
    let mut all = read_manifest();
    all.retain(|s| s.id != session_id);
    write_manifest(&all)
}

fn restore_session(session: &TrashSession) -> Result<()> {
    let session_dir = session_trash_dir(&session.id).ok_or_else(home_error)?;

    let mut restored = 0usize;
    let mut failed = 0usize;

    for entry in &session.entries {
        let src = session_dir.join(entry.index.to_string());
        if let Some(parent) = entry.original.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                println!(
                    "  {} {} -- {}",
                    style("ERR").red(),
                    display_path(&entry.original),
                    style(e.to_string()).dim()
                );
                failed += 1;
                continue;
            }
        }
        match move_dir(&src, &entry.original) {
            Ok(()) => restored += 1,
            Err(e) => {
                println!(
                    "  {} {} -- {}",
                    style("ERR").red(),
                    display_path(&entry.original),
                    style(e.to_string()).dim()
                );
                failed += 1;
            }
        }
    }

    if failed == 0 {
        let _ = fs::remove_dir_all(&session_dir);
        println!(
            "{}",
            style(format!(
                "Restored {} artefact{}.",
                restored,
                if restored == 1 { "" } else { "s" }
            ))
            .bold()
            .green()
        );
    } else {
        println!(
            "{}",
            style(format!(
                "Restored {}. {} could not be restored.",
                restored, failed
            ))
            .yellow()
        );
    }

    Ok(())
}

/// Permanently delete all sessions in the trash after a stern warning and double confirmation.
pub fn empty_trash() -> Result<()> {
    let mut sessions = read_manifest();
    purge_old(&mut sessions);

    if sessions.is_empty() {
        println!("{}", style("Trash is already empty.").dim());
        return Ok(());
    }

    let total_size: u64 = sessions.iter().flat_map(|s| s.entries.iter()).map(|e| e.size).sum();
    let total_entries: usize = sessions.iter().map(|s| s.entries.len()).sum();

    println!(
        "\n  {} sessions  |  {} artefacts  |  {}\n",
        style(sessions.len()).bold(),
        style(total_entries).bold(),
        style(human_size(total_size)).bold(),
    );

    let warning = " ///  WARNING — This action is definitive.  \
Anything cleared from the trash cannot be recovered anymore.  /// ";
    println!("{}", style(warning).white().on_red().bold());
    println!();

    if !confirm_raw("Empty the trash? (1/2)")? {
        println!("{}", style("Aborted.").dim());
        return Ok(());
    }
    if !confirm_raw("Are you absolutely sure? (2/2)")? {
        println!("{}", style("Aborted.").dim());
        return Ok(());
    }

    let mut freed: u64 = 0;
    let mut errors = 0usize;

    for session in &sessions {
        if let Some(dir) = session_trash_dir(&session.id) {
            match fs::remove_dir_all(&dir) {
                Ok(()) => freed += session.total_size(),
                Err(e) => {
                    println!(
                        "  {} {} -- {}",
                        style("ERR").red(),
                        dir.display(),
                        style(e.to_string()).dim()
                    );
                    errors += 1;
                }
            }
        }
    }

    write_manifest(&[])?;

    println!();
    if errors == 0 {
        println!(
            "{}",
            style(format!("Trash emptied. {} permanently freed.", human_size(freed)))
                .bold()
                .green()
        );
    } else {
        println!(
            "{}",
            style(format!(
                "Done. {} freed. {} session{} could not be fully removed.",
                human_size(freed),
                errors,
                if errors == 1 { "" } else { "s" }
            ))
            .yellow()
        );
    }

    Ok(())
}

fn confirm_raw(prompt: &str) -> Result<bool> {
    use std::io::Write;
    print!("{} [y/N] ", style(prompt).bold());
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(matches!(input.trim().to_lowercase().as_str(), "y" | "yes"))
}
