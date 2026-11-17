//! Detects new items inside watched folders that are neither included nor ignored.

use crate::config::Config;
use crate::paths::{expand, is_within};
use anyhow::Result;
use std::path::PathBuf;

/// Returns direct children of watched folders that the configuration does not cover yet.
pub fn find_new(cfg: &Config) -> Result<Vec<PathBuf>> {
    let excludes = cfg.exclude_set()?;
    let known: Vec<PathBuf> = cfg
        .include
        .iter()
        .chain(&cfg.ignore)
        .map(|s| expand(s))
        .collect();
    let mut found = Vec::new();
    for w in &cfg.watch {
        let dir = expand(w);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if excludes.is_excluded(&path) || is_hidden_system(&entry) {
                continue;
            }
            if !known.iter().any(|k| is_within(&path, k)) {
                found.push(path);
            }
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
}

#[cfg(windows)]
fn is_hidden_system(entry: &std::fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN_SYSTEM: u32 = 0x2 | 0x4;
    entry
        .metadata()
        .is_ok_and(|m| m.file_attributes() & HIDDEN_SYSTEM == HIDDEN_SYSTEM)
}

#[cfg(not(windows))]
fn is_hidden_system(_: &std::fs::DirEntry) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn reports_only_unknown_children() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in ["kept", "skipped", "fresh", "node_modules"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::write(root.join("new.txt"), "x").unwrap();

        let mut cfg = Config::template();
        cfg.watch = vec![root.to_string_lossy().into()];
        cfg.add_include(&root.join("kept"));
        cfg.add_ignore(&root.join("skipped"));

        let found = find_new(&cfg).unwrap();
        assert_eq!(found, vec![root.join("fresh"), root.join("new.txt")]);
    }

    #[test]
    fn including_a_parent_covers_children() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("child")).unwrap();
        let mut cfg = Config {
            watch: vec![dir.path().to_string_lossy().into()],
            ..Config::default()
        };
        cfg.add_include(dir.path());
        assert!(find_new(&cfg).unwrap().is_empty());
    }
}
