//! Locating the external backup drive by a marker file rather than by drive letter.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const MARKER: &str = "backup-drive.id";
pub const BACKUP_DIR: &str = "Backups";

/// Returns the roots of all drives that carry the marker file.
pub fn find_backup_drives() -> Vec<PathBuf> {
    (b'A'..=b'Z')
        .map(|c| PathBuf::from(format!("{}:\\", c as char)))
        .filter(|root| root.join(MARKER).is_file())
        .collect()
}

/// Marks a drive (or any folder) as a backup destination.
pub fn mark(root: &Path) -> Result<PathBuf> {
    std::fs::write(root.join(MARKER), "Respawn destination\n")
        .with_context(|| format!("cannot write marker to {}", root.display()))?;
    let dir = backup_dir(root);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn backup_dir(root: &Path) -> PathBuf {
    root.join(BACKUP_DIR)
}

#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Lists finished backups in a folder, newest first.
pub fn list_backups(dir: &Path) -> Vec<BackupInfo> {
    let mut list: Vec<BackupInfo> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("zip"))
        })
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some(BackupInfo {
                path: e.path(),
                size: m.len(),
                modified: m.modified().ok(),
            })
        })
        .collect();
    list.sort_by(|a, b| b.path.cmp(&a.path));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_creates_marker_and_folder() {
        let dir = tempfile::tempdir().unwrap();
        let out = mark(dir.path()).unwrap();
        assert!(dir.path().join(MARKER).is_file());
        assert!(out.is_dir());
        std::fs::write(out.join("backup-20260101-000000.zip"), "x").unwrap();
        std::fs::write(out.join("backup-20260201-000000.zip"), "x").unwrap();
        std::fs::write(out.join("notes.txt"), "x").unwrap();
        let list = list_backups(&out);
        assert_eq!(list.len(), 2);
        assert!(list[0].path.ends_with("backup-20260201-000000.zip"));
    }
}
