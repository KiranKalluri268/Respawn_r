//! Walks included paths and builds the list of files and folders to archive.

use crate::config::Config;
use crate::paths::{expand, key};
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Default)]
pub struct Collected {
    pub files: Vec<FileEntry>,
    /// Every folder walked, so empty folders and folder timestamps can be restored.
    pub dirs: Vec<PathBuf>,
    /// Included paths that do not exist.
    pub missing: Vec<PathBuf>,
    /// Paths that could not be read while walking.
    pub errors: Vec<(PathBuf, String)>,
}

impl Collected {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// Collects all files under the configured includes plus `extra` paths (e.g. the config file).
pub fn collect(cfg: &Config, extra: &[PathBuf]) -> Result<Collected> {
    let excludes = cfg.exclude_set()?;
    let mut out = Collected::default();
    let mut seen_files = HashSet::new();
    let mut seen_dirs = HashSet::new();

    let roots: Vec<PathBuf> = cfg
        .include
        .iter()
        .map(|s| expand(s))
        .chain(extra.iter().cloned())
        .collect();
    for root in roots {
        if !root.exists() {
            out.missing.push(root);
            continue;
        }
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| e.depth() == 0 || !excludes.is_excluded(e.path()));
        for item in walker {
            match item {
                Ok(e) if e.file_type().is_dir() => {
                    if seen_dirs.insert(key(e.path())) {
                        out.dirs.push(e.path().to_path_buf());
                    }
                }
                Ok(e) if e.file_type().is_file() => {
                    if !seen_files.insert(key(e.path())) {
                        continue;
                    }
                    match e.metadata() {
                        Ok(m) => out.files.push(FileEntry {
                            path: e.path().to_path_buf(),
                            size: m.len(),
                        }),
                        Err(err) => out.errors.push((e.path().to_path_buf(), err.to_string())),
                    }
                }
                Ok(_) => {} // symlinks and other special entries are skipped
                Err(err) => {
                    let p = err
                        .path()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| root.clone());
                    out.errors.push((p, err.to_string()));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collects_files_and_honours_excludes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::create_dir_all(root.join("empty")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), "x").unwrap();

        let mut cfg = Config::template();
        cfg.add_include(&root);
        cfg.add_include(&root.join("src")); // overlapping include must not duplicate
        cfg.add_include(&dir.path().join("missing"));

        let c = collect(&cfg, &[]).unwrap();
        let files: Vec<_> = c.files.iter().map(|f| f.path.clone()).collect();
        assert_eq!(files, vec![root.join("src").join("main.rs")]);
        assert_eq!(c.total_bytes(), 12);
        assert!(c.dirs.contains(&root.join("empty")));
        assert!(!c.dirs.iter().any(|d| d.ends_with("node_modules")));
        assert_eq!(c.missing, vec![dir.path().join("missing")]);
    }
}
