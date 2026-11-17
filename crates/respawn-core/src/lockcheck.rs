//! Pre-flight checks for files that other programs hold open.

use crate::collect::FileEntry;
use crate::config::Config;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct LockedFile {
    pub path: PathBuf,
    pub reason: String,
}

/// Names of configured lock-prone applications that are currently running.
pub fn running_lock_apps(cfg: &Config) -> Vec<String> {
    let running = running_processes();
    let mut found: Vec<String> = cfg
        .lock_apps
        .iter()
        .filter(|app| running.iter().any(|p| p.eq_ignore_ascii_case(app)))
        .cloned()
        .collect();
    found.dedup();
    found
}

/// Files that cannot currently be opened and read.
pub fn locked_files(files: &[FileEntry], mut on_checked: impl FnMut(usize)) -> Vec<LockedFile> {
    let mut locked = Vec::new();
    for (i, f) in files.iter().enumerate() {
        if let Err(e) = probe(&f.path) {
            locked.push(LockedFile {
                path: f.path.clone(),
                reason: e.to_string(),
            });
        }
        on_checked(i + 1);
    }
    locked
}

fn probe(path: &std::path::Path) -> std::io::Result<()> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = [0u8; 1];
    f.read(&mut buf).map(|_| ())
}

fn running_processes() -> Vec<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let Ok(out) = Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            return Vec::new();
        };
        parse_tasklist(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("true");
        Vec::new()
    }
}

fn parse_tasklist(csv: &str) -> Vec<String> {
    csv.lines()
        .filter_map(|l| l.strip_prefix('"')?.split('"').next().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tasklist_csv() {
        let csv = "\"Code.exe\",\"1234\",\"Console\",\"1\",\"100 K\"\r\n\"cursor.exe\",\"5\",\"Console\",\"1\",\"1 K\"\r\n";
        assert_eq!(parse_tasklist(csv), vec!["Code.exe", "cursor.exe"]);
    }

    #[test]
    fn readable_files_are_not_locked() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "x").unwrap();
        let files = vec![
            FileEntry { path: p, size: 1 },
            FileEntry {
                path: dir.path().join("gone.txt"),
                size: 1,
            },
        ];
        let locked = locked_files(&files, |_| {});
        assert_eq!(locked.len(), 1);
        assert!(locked[0].path.ends_with("gone.txt"));
    }
}
