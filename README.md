# Respawn

Encrypted, single-file backups of your important folders on Windows, restorable to the exact
same paths on a freshly reset or new machine.

- One AES-256 encrypted `.zip` per backup (also opens with 7-Zip if ever needed)
- Tracks watched folders and asks about new items before each backup
- Skips rebuildable folders (`node_modules`, `target`, `.venv`, ...) via exclude patterns
- Stores photos/videos uncompressed, compresses everything else
- Detects programs holding files open and lets you retry, continue (with a report) or abort
- Verifies every file after writing
- Restores files, empty folders and timestamps (including creation times) to original paths

## Install

Download `respawn.exe` from the [latest release](https://github.com/KiranKalluri268/Respawn_r/releases/latest), or build it:

```
cargo build --release
# binary: target\release\respawn.exe
```

## First-time setup

```
respawn init             # create %APPDATA%\respawn\config.toml
respawn edit             # adjust watch / include / exclude
respawn init-drive E:\   # mark the external drive (once)
respawn scan             # decide on items found in watched folders
respawn preview          # see what will be backed up
```

## Making a backup

```
respawn backup
```

The backup is written to `<drive>\Backups\backup-YYYYMMDD-HHMMSS.zip`. Files that could not be
read are listed in a `.skipped.txt` file next to it.

## Restoring

```
respawn restore E:\Backups\backup-20261117-153000.zip --dry-run
respawn restore E:\Backups\backup-20261117-153000.zip
```

Files are restored to their original absolute paths, so every drive letter used by the backup must
exist. The configuration file is part of every backup and is restored with it.

## Configuration

```toml
watch = ["~/Documents", "~/Pictures"]      # children of these are tracked by `scan`
include = ["~/Documents/Notes", "D:/Projects"]
ignore = ["~/Documents/Old"]
exclude = ["**/node_modules", "**/target"]
store_extensions = ["jpg", "mp4"]
lock_apps = ["Code.exe"]
```

## Development

```
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

The workspace is split into `respawn-core` (all logic, no terminal I/O) and `respawn-cli`
(interactive front end), so other front ends can reuse the core.
