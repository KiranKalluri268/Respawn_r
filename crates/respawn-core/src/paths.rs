//! Path helpers: `~` expansion, case-insensitive comparison keys and archive entry names.

use anyhow::{Result, bail};
use std::path::{Component, Path, PathBuf, Prefix};

/// Expands a leading `~` to the user's home directory.
pub fn expand(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix('~')
        && let Some(home) = dirs::home_dir()
    {
        let rest = rest.trim_start_matches(['/', '\\']);
        return if rest.is_empty() {
            home
        } else {
            home.join(rest)
        };
    }
    PathBuf::from(trimmed)
}

/// Normalised key used to compare paths (Windows paths are case-insensitive).
pub fn key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\").to_lowercase();
    s.trim_end_matches('\\').to_string()
}

/// True if `path` equals `base` or lies inside it.
pub fn is_within(path: &Path, base: &Path) -> bool {
    let (p, b) = (key(path), key(base));
    p == b || p.starts_with(&format!("{b}\\"))
}

/// Maps an absolute path to its name inside the archive, e.g. `C:\Users\x.txt` -> `files/C/Users/x.txt`.
pub fn entry_name(path: &Path) -> Result<String> {
    let mut parts = vec!["files".to_string()];
    let mut has_drive = false;
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => match p.kind() {
                Prefix::Disk(d) | Prefix::VerbatimDisk(d) => {
                    has_drive = true;
                    parts.push((d as char).to_ascii_uppercase().to_string());
                }
                _ => bail!(
                    "unsupported path (network or device path): {}",
                    path.display()
                ),
            },
            Component::RootDir => {}
            Component::Normal(n) => parts.push(n.to_string_lossy().into_owned()),
            Component::CurDir | Component::ParentDir => {
                bail!("path must be absolute and normalised: {}", path.display())
            }
        }
    }
    if !has_drive || parts.len() < 3 {
        bail!(
            "path must be absolute with a drive letter: {}",
            path.display()
        );
    }
    Ok(parts.join("/"))
}

/// Returns the drive letter of an absolute path, if any.
pub fn drive_letter(path: &Path) -> Option<char> {
    match path.components().next()? {
        Component::Prefix(p) => match p.kind() {
            Prefix::Disk(d) | Prefix::VerbatimDisk(d) => Some((d as char).to_ascii_uppercase()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_case_and_separators() {
        assert_eq!(
            key(Path::new("C:/Users/Foo/")),
            key(Path::new("c:\\users\\foo"))
        );
    }

    #[test]
    fn within_checks_boundaries() {
        assert!(is_within(Path::new(r"C:\a\b\c"), Path::new(r"C:\a\b")));
        assert!(is_within(Path::new(r"C:\a\b"), Path::new(r"c:\A\B")));
        assert!(!is_within(Path::new(r"C:\a\bc"), Path::new(r"C:\a\b")));
    }

    #[test]
    fn entry_names_include_drive() {
        assert_eq!(
            entry_name(Path::new(r"K:\Projects\app\main.rs")).unwrap(),
            "files/K/Projects/app/main.rs"
        );
        assert!(entry_name(Path::new(r"relative\path")).is_err());
        assert!(entry_name(Path::new(r"\\server\share\x")).is_err());
    }

    #[test]
    fn drive_letter_is_uppercase() {
        assert_eq!(drive_letter(Path::new(r"k:\x")), Some('K'));
        assert_eq!(drive_letter(Path::new(r"x\y")), None);
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand("~"), home);
        assert_eq!(expand("~/Documents"), home.join("Documents"));
        assert_eq!(expand(r"D:\Data"), PathBuf::from(r"D:\Data"));
    }
}
