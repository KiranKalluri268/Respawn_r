//! Creates encrypted zip archives (AES-256) from collected files.

use crate::Progress;
use crate::collect::Collected;
use crate::config::Config;
use crate::manifest::{
    FORMAT_VERSION, MANIFEST_ENTRY, Manifest, ManifestDir, ManifestFile, Skipped,
};
use crate::paths::entry_name;
use crate::times;
use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, ZipWriter};

const CHUNK: usize = 1 << 20;
/// Files at or above this size need Zip64 headers.
const LARGE_FILE: u64 = 0xFFFF_0000;

#[derive(Debug)]
pub struct BackupOutcome {
    pub archive: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub skipped: Vec<Skipped>,
}

/// Archive file name for a given local time, e.g. `backup-20261117-153000.zip`.
pub fn archive_name(now: chrono::DateTime<chrono::Local>) -> String {
    format!("backup-{}.zip", now.format("%Y%m%d-%H%M%S"))
}

/// Writes all collected files into `dest_dir`. The archive is written as `.partial` and renamed
/// only after it is complete, so an interrupted run never leaves a file that looks finished.
pub fn create_backup(
    cfg: &Config,
    collected: &Collected,
    dest_dir: &Path,
    password: &str,
    on: &mut dyn FnMut(Progress),
) -> Result<BackupOutcome> {
    if password.is_empty() {
        bail!("a password is required");
    }
    std::fs::create_dir_all(dest_dir)?;
    let now = chrono::Local::now();
    let final_path = dest_dir.join(archive_name(now));
    let partial = final_path.with_extension("zip.partial");

    let result = write_archive(cfg, collected, &partial, password, now, on);
    match result {
        Ok((files, bytes, skipped)) => {
            std::fs::rename(&partial, &final_path)
                .with_context(|| format!("cannot finalise {}", final_path.display()))?;
            Ok(BackupOutcome {
                archive: final_path,
                files,
                bytes,
                skipped,
            })
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn write_archive(
    cfg: &Config,
    collected: &Collected,
    path: &Path,
    password: &str,
    now: chrono::DateTime<chrono::Local>,
    on: &mut dyn FnMut(Progress),
) -> Result<(usize, u64, Vec<Skipped>)> {
    let file = File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
    let mut zip = ZipWriter::new(BufWriter::with_capacity(CHUNK, file));
    let base = SimpleFileOptions::default().with_aes_encryption(AesMode::Aes256, password);

    let mut manifest = Manifest {
        format_version: FORMAT_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: now.to_rfc3339(),
        computer: std::env::var("COMPUTERNAME").unwrap_or_default(),
        files: Vec::with_capacity(collected.files.len()),
        dirs: Vec::with_capacity(collected.dirs.len()),
        skipped: collected
            .errors
            .iter()
            .map(|(p, r)| Skipped {
                path: p.display().to_string(),
                reason: r.clone(),
            })
            .collect(),
    };

    on(Progress::Start {
        files: collected.files.len(),
        bytes: collected.total_bytes(),
    });
    let mut buf = vec![0u8; CHUNK];
    let mut bytes = 0u64;

    for f in &collected.files {
        on(Progress::File(f.path.clone()));
        let entry = entry_name(&f.path)?;
        let (modified, created) = times::read(&f.path);
        let mut src = match File::open(&f.path) {
            Ok(s) => s,
            Err(e) => {
                manifest.skipped.push(skipped(&f.path, &e));
                on(Progress::Bytes(f.size));
                continue;
            }
        };
        let method = if cfg.is_stored(&f.path) {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        let opts = base
            .compression_method(method)
            .large_file(f.size >= LARGE_FILE);
        zip.start_file(entry.as_str(), opts)?;

        let mut written = 0u64;
        let copy: std::io::Result<()> = loop {
            match src.read(&mut buf) {
                Ok(0) => break Ok(()),
                Ok(n) => {
                    zip.write_all(&buf[..n])?;
                    written += n as u64;
                    on(Progress::Bytes(n as u64));
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => break Err(e),
            }
        };
        if let Err(e) = copy {
            zip.abort_file()?;
            manifest.skipped.push(skipped(&f.path, &e));
            on(Progress::Bytes(f.size.saturating_sub(written)));
            continue;
        }
        bytes += written;
        manifest.files.push(ManifestFile {
            path: f.path.display().to_string(),
            entry,
            size: written,
            modified,
            created,
        });
    }

    for d in &collected.dirs {
        let (modified, created) = times::read(d);
        manifest.dirs.push(ManifestDir {
            path: d.display().to_string(),
            modified,
            created,
        });
    }

    on(Progress::Phase("writing manifest"));
    zip.start_file(
        MANIFEST_ENTRY,
        base.compression_method(CompressionMethod::Deflated),
    )?;
    serde_json::to_writer(&mut zip, &manifest)?;

    let mut out = zip.finish()?;
    out.flush()?;
    out.into_inner().map_err(|e| e.into_error())?.sync_all()?;
    Ok((manifest.files.len(), bytes, manifest.skipped))
}

fn skipped(path: &Path, err: &std::io::Error) -> Skipped {
    Skipped {
        path: path.display().to_string(),
        reason: err.to_string(),
    }
}
