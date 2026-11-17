//! User configuration stored as TOML in `%APPDATA%\respawn\config.toml`.

use crate::paths::{expand, key};
use anyhow::{Context, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Folders whose direct children are tracked. New children are reported by `scan`.
    pub watch: Vec<String>,
    /// Files and folders included in every backup. `~` expands to the home directory.
    pub include: Vec<String>,
    /// Watched children that were deliberately left out.
    pub ignore: Vec<String>,
    /// Glob patterns excluded everywhere (matched case-insensitively against full paths).
    pub exclude: Vec<String>,
    /// File extensions stored without compression (already-compressed formats).
    pub store_extensions: Vec<String>,
    /// Process names that commonly hold files open (checked before a backup).
    pub lock_apps: Vec<String>,
}

impl Config {
    /// A starter configuration for a new installation.
    pub fn template() -> Self {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect();
        Config {
            watch: s(&["~/Documents", "~/Pictures", "~/Videos", "~/Desktop"]),
            include: Vec::new(),
            ignore: Vec::new(),
            exclude: s(&[
                "**/node_modules",
                "**/target",
                "**/.venv",
                "**/venv",
                "**/__pycache__",
                "**/.next",
                "**/dist",
                "**/build",
                "**/.gradle",
                "**/*.tmp",
                "**/Thumbs.db",
                "**/desktop.ini",
            ]),
            store_extensions: s(&[
                "jpg", "jpeg", "png", "gif", "webp", "heic", "mp4", "mkv", "mov", "avi", "webm",
                "mp3", "aac", "flac", "ogg", "m4a", "zip", "7z", "rar", "gz", "zst", "xz", "pdf",
                "docx", "xlsx", "pptx",
            ]),
            lock_apps: s(&["Code.exe", "Cursor.exe", "devenv.exe", "idea64.exe"]),
        }
    }

    /// Default location of the configuration file.
    pub fn default_path() -> Result<PathBuf> {
        let base = dirs::config_dir().context("cannot locate the application data folder")?;
        Ok(base.join("respawn").join("config.toml"))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self)?;
        std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
    }

    /// Adds a path to `include` (removing it from `ignore`).
    pub fn add_include(&mut self, path: &Path) {
        remove_path(&mut self.ignore, path);
        if !contains_path(&self.include, path) {
            self.include.push(path.to_string_lossy().into_owned());
        }
    }

    /// Adds a path to `ignore` (removing it from `include`).
    pub fn add_ignore(&mut self, path: &Path) {
        remove_path(&mut self.include, path);
        if !contains_path(&self.ignore, path) {
            self.ignore.push(path.to_string_lossy().into_owned());
        }
    }

    pub fn exclude_set(&self) -> Result<Excludes> {
        let mut b = GlobSetBuilder::new();
        for pat in &self.exclude {
            let glob = GlobBuilder::new(&pat.replace('\\', "/"))
                .case_insensitive(true)
                .build()
                .with_context(|| format!("invalid exclude pattern '{pat}'"))?;
            b.add(glob);
        }
        Ok(Excludes(b.build()?))
    }

    /// True if the extension should be stored uncompressed.
    pub fn is_stored(&self, path: &Path) -> bool {
        path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .is_some_and(|e| {
                self.store_extensions
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(&e))
            })
    }
}

/// Compiled exclude patterns.
pub struct Excludes(GlobSet);

impl Excludes {
    pub fn is_excluded(&self, path: &Path) -> bool {
        self.0.is_match(path.to_string_lossy().replace('\\', "/"))
    }
}

fn contains_path(list: &[String], path: &Path) -> bool {
    let k = key(path);
    list.iter().any(|s| key(&expand(s)) == k)
}

fn remove_path(list: &mut Vec<String>, path: &Path) {
    let k = key(path);
    list.retain(|s| key(&expand(s)) != k);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_round_trips_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg").join("config.toml");
        let cfg = Config::template();
        cfg.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), cfg);
    }

    #[test]
    fn excludes_match_anywhere_case_insensitively() {
        let ex = Config::template().exclude_set().unwrap();
        assert!(ex.is_excluded(Path::new(r"C:\proj\Node_Modules")));
        assert!(ex.is_excluded(Path::new(r"C:\proj\app\target")));
        assert!(!ex.is_excluded(Path::new(r"C:\proj\src\targets.rs")));
    }

    #[test]
    fn include_and_ignore_are_exclusive() {
        let mut cfg = Config::default();
        let p = Path::new(r"C:\Data\Photos");
        cfg.add_include(p);
        cfg.add_ignore(Path::new(r"c:\data\photos"));
        assert!(cfg.include.is_empty());
        assert_eq!(cfg.ignore.len(), 1);
        cfg.add_include(p);
        assert!(cfg.ignore.is_empty());
        assert_eq!(cfg.include.len(), 1);
    }

    #[test]
    fn stored_extensions() {
        let cfg = Config::template();
        assert!(cfg.is_stored(Path::new("a/IMG.JPG")));
        assert!(!cfg.is_stored(Path::new("a/notes.txt")));
    }
}
