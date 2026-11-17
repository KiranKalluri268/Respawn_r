//! The manifest stored inside every archive. It records original paths and timestamps.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

pub const MANIFEST_ENTRY: &str = "manifest.json";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub tool_version: String,
    pub created_at: String,
    pub computer: String,
    pub files: Vec<ManifestFile>,
    pub dirs: Vec<ManifestDir>,
    /// Files that could not be read during the backup.
    pub skipped: Vec<Skipped>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    /// Original absolute path.
    pub path: String,
    /// Entry name inside the archive.
    pub entry: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestDir {
    pub path: String,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}
