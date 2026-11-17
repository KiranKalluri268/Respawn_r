//! Restores an archive to the original absolute paths recorded in its manifest.

use crate::Progress;
use crate::manifest::Manifest;
use crate::paths::drive_letter;
use crate::times;
use crate::verify::{Archive, open, read_manifest};
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

/// What a restore would do, computed before anything is written.
#[derive(Debug)]
pub struct RestorePlan {
    pub manifest: Manifest,
    /// Drive letters used by the backup that do not exist on this computer.
    pub missing_drives: Vec<char>,
    /// Files that already exist at their original location.
    pub existing: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingPolicy {
    Overwrite,
    Skip,
}

#[derive(Debug, Default)]
pub struct RestoreReport {
    pub restored: usize,
    pub bytes: u64,
    pub skipped_existing: usize,
    pub errors: Vec<String>,
}

pub fn plan(archive_path: &Path, password: &str) -> Result<(Archive, RestorePlan)> {
    let mut archive = open(archive_path)?;
    let manifest = read_manifest(&mut archive, password)?;
    let drives: BTreeSet<char> = manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(manifest.dirs.iter().map(|d| d.path.as_str()))
        .filter_map(|p| drive_letter(Path::new(p)))
        .collect();
    let missing_drives = drives
        .into_iter()
        .filter(|d| !Path::new(&format!("{d}:\\")).exists())
        .collect();
    let existing = manifest
        .files
        .iter()
        .map(|f| PathBuf::from(&f.path))
        .filter(|p| p.exists())
        .collect();
    Ok((
        archive,
        RestorePlan {
            manifest,
            missing_drives,
            existing,
        },
    ))
}

pub fn restore(
    archive: &mut Archive,
    plan: &RestorePlan,
    password: &str,
    policy: ExistingPolicy,
    on: &mut dyn FnMut(Progress),
) -> Result<RestoreReport> {
    let m = &plan.manifest;
    let mut report = RestoreReport::default();
    on(Progress::Start {
        files: m.files.len(),
        bytes: m.files.iter().map(|f| f.size).sum(),
    });

    for d in &m.dirs {
        if let Err(e) = std::fs::create_dir_all(&d.path) {
            report.errors.push(format!("{}: {e}", d.path));
        }
    }

    let mut buf = vec![0u8; 1 << 20];
    for f in &m.files {
        let target = PathBuf::from(&f.path);
        on(Progress::File(target.clone()));
        if policy == ExistingPolicy::Skip && target.exists() {
            report.skipped_existing += 1;
            on(Progress::Bytes(f.size));
            continue;
        }
        let result = (|| -> Result<u64> {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut entry = archive.by_name_decrypt(&f.entry, password.as_bytes())?;
            let tmp = temp_path(&target);
            let written = (|| -> Result<u64> {
                let mut out = BufWriter::with_capacity(1 << 20, File::create(&tmp)?);
                let mut total = 0u64;
                loop {
                    let n = entry.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    out.write_all(&buf[..n])?;
                    total += n as u64;
                    on(Progress::Bytes(n as u64));
                }
                out.into_inner().map_err(|e| e.into_error())?.sync_all()?;
                Ok(total)
            })();
            match written {
                Ok(total) => {
                    replace(&tmp, &target)?;
                    times::apply(&target, f.modified, f.created)
                        .with_context(|| "cannot set timestamps")?;
                    Ok(total)
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp);
                    Err(e)
                }
            }
        })();
        match result {
            Ok(n) => {
                report.restored += 1;
                report.bytes += n;
            }
            Err(e) => report.errors.push(format!("{}: {e:#}", f.path)),
        }
    }

    // Folder timestamps last, deepest first, so writing files does not change them again.
    let mut dirs: Vec<_> = m.dirs.iter().collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(Path::new(&d.path).components().count()));
    for d in dirs {
        if let Err(e) = times::apply(Path::new(&d.path), d.modified, d.created) {
            report
                .errors
                .push(format!("{}: cannot set folder timestamps: {e}", d.path));
        }
    }
    Ok(report)
}

fn temp_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".backuper-tmp");
    target.with_file_name(name)
}

fn replace(tmp: &Path, target: &Path) -> std::io::Result<()> {
    if target.exists() {
        // Clear the read-only flag so the old file can be replaced.
        let mut perms = std::fs::metadata(target)?.permissions();
        if perms.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            std::fs::set_permissions(target, perms)?;
        }
    }
    std::fs::rename(tmp, target)
}
