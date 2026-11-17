//! Reading archives back: opening, manifest access and full integrity verification.

use crate::Progress;
use crate::manifest::{MANIFEST_ENTRY, Manifest};
use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use zip::ZipArchive;
use zip::result::ZipError;

pub type Archive = ZipArchive<BufReader<File>>;

pub fn open(path: &Path) -> Result<Archive> {
    let f = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    ZipArchive::new(BufReader::with_capacity(1 << 20, f)).context("not a valid zip archive")
}

/// Reads the manifest. A wrong password is reported as a clear error.
pub fn read_manifest(archive: &mut Archive, password: &str) -> Result<Manifest> {
    let mut entry = match archive.by_name_decrypt(MANIFEST_ENTRY, password.as_bytes()) {
        Ok(e) => e,
        Err(ZipError::InvalidPassword) => bail!("wrong password"),
        Err(ZipError::FileNotFound) => bail!("this archive was not created by Respawn"),
        Err(e) => return Err(e.into()),
    };
    let mut text = String::new();
    entry.read_to_string(&mut text).map_err(|e| {
        if e.kind() == std::io::ErrorKind::InvalidData {
            anyhow::anyhow!("wrong password")
        } else {
            e.into()
        }
    })?;
    serde_json::from_str(&text).context("manifest is corrupt")
}

#[derive(Debug, Default)]
pub struct VerifyReport {
    pub files_checked: usize,
    pub bytes_checked: u64,
    pub problems: Vec<String>,
}

impl VerifyReport {
    pub fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Decrypts and reads every file, which checks the AES authentication code and CRC of each entry.
pub fn verify(path: &Path, password: &str, on: &mut dyn FnMut(Progress)) -> Result<VerifyReport> {
    let mut archive = open(path)?;
    let manifest = read_manifest(&mut archive, password)?;
    let mut report = VerifyReport::default();
    on(Progress::Start {
        files: manifest.files.len(),
        bytes: manifest.files.iter().map(|f| f.size).sum(),
    });
    let mut buf = vec![0u8; 1 << 20];
    for f in &manifest.files {
        on(Progress::File(f.path.clone().into()));
        let result = (|| -> Result<u64> {
            let mut entry = archive.by_name_decrypt(&f.entry, password.as_bytes())?;
            let mut total = 0u64;
            loop {
                let n = entry.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                total += n as u64;
            }
            Ok(total)
        })();
        match result {
            Ok(n) if n == f.size => report.bytes_checked += n,
            Ok(n) => report
                .problems
                .push(format!("{}: size {n}, expected {}", f.path, f.size)),
            Err(e) => report.problems.push(format!("{}: {e}", f.path)),
        }
        report.files_checked += 1;
        on(Progress::Bytes(f.size));
    }
    Ok(report)
}
