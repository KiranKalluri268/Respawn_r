//! Reading and restoring file and folder timestamps (including creation time on Windows).

use std::io;
use std::path::Path;
use std::time::SystemTime;

pub fn read(path: &Path) -> (Option<SystemTime>, Option<SystemTime>) {
    match std::fs::metadata(path) {
        Ok(m) => (m.modified().ok(), m.created().ok()),
        Err(_) => (None, None),
    }
}

/// Sets modified and (on Windows) created times on a file or folder.
pub fn apply(
    path: &Path,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
) -> io::Result<()> {
    if modified.is_none() && created.is_none() {
        return Ok(());
    }
    let mut times = std::fs::FileTimes::new();
    if let Some(m) = modified {
        times = times.set_modified(m);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::{FileTimesExt, OpenOptionsExt};
        const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000; // required to open folders
        if let Some(c) = created {
            times = times.set_created(c);
        }
        let f = std::fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        f.set_times(times)
    }
    #[cfg(not(windows))]
    {
        let _ = created;
        if path.is_dir() {
            return Ok(());
        }
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)?
            .set_times(times)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn restores_created_and_modified_on_files_and_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        for p in [file.as_path(), dir.path()] {
            apply(p, Some(t), Some(t)).unwrap();
            assert_eq!(read(p), (Some(t), Some(t)));
        }
    }
}
