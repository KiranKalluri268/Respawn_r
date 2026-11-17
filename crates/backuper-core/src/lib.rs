//! Core logic for Backup-er: configuration, scanning, archiving, verification and restore.
//!
//! The library performs no terminal I/O. Front ends (CLI, TUI) receive progress through
//! callbacks and make all interactive decisions themselves.

pub mod archive;
pub mod collect;
pub mod config;
pub mod drive;
pub mod lockcheck;
pub mod manifest;
pub mod paths;
pub mod restore;
pub mod scan;
pub mod times;
pub mod verify;

/// Progress events emitted by long-running operations.
#[derive(Debug, Clone)]
pub enum Progress {
    /// Work is starting; totals are known.
    Start { files: usize, bytes: u64 },
    /// A new file is being processed.
    File(std::path::PathBuf),
    /// Bytes processed since the last event.
    Bytes(u64),
    /// A named phase is starting (e.g. "verifying").
    Phase(&'static str),
}
