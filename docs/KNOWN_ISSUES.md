# Known issues

Issues found during the first real backup and restore dry run (v0.1.0).

## 1. Mixed path separators in output

Paths from `include` written with forward slashes (for example
`~/AppData/Roaming/Cursor/User`) are kept as-is, so collected paths mix `/` and `\`:

```
C:\Users\HP\AppData/Roaming/Cursor/User\globalStorage\state.vscdb
```

Windows accepts both separators, so backup and restore work, but the paths shown in
reports, the manifest and restore output are inconsistent.

**Fix:** normalise separators to `\` when expanding configured paths (`paths::expand`).

## 2. Restore dry run prints every file

`respawn restore <archive> --dry-run` lists every existing file individually. With about
13,600 files the output is impossible to review.

**Fix:** print a per-folder summary (top-level include path with file count) by default
and add a `--verbose` flag for the full list.

## 3. Rebuildable editor files are backed up

Some files under editor data folders are application binaries or redundant copies that
the editor recreates:

- `Cursor/User/globalStorage/anysphere.cursor-agent-worker/**/versions/` (agent CLI
  binaries such as `node.exe`, `rg.exe` and JS bundles)
- `**/state.vscdb.backup` (VS Code and Cursor database backups)

**Fix:** add these to the starter `exclude` patterns.

## 4. Live SQLite databases may be captured mid-write

When Codex, Cursor or Claude Code are running, their SQLite databases can change while
being read. The `-wal`/`-shm` companions may also disappear during the backup and are
reported as skipped. Closing these applications before a backup avoids both problems;
`lock_apps` warns about them.

**Possible fix:** offer to copy databases through the SQLite online backup API, or retry
files whose size or modified time changed during reading.

## 5. Skip warning undercounts and the report mixes causes

Before writing, the lock check warned that 6 files would be left out. The backup then
skipped 10: four SQLite `-wal`/`-shm` files were deleted by Codex between collection and
reading (`os error 2`). The `.skipped.txt` report lists both kinds together.

**Fix:** treat files that vanish after collection as "removed during backup" rather than
skipped, report them separately in the summary and in `.skipped.txt`, and state in the
final message that the count may differ from the pre-backup warning.

## 6. Lock and socket files are reported as unreadable

Runtime files that are always held open by their owner appear in every skip report:

- `.lock` and `*.sqlite-maintenance.lock` (`os error 33`)
- `*.sock` named sockets (`os error 1920`)
- `~/.codex/tmp`
- OneDrive placeholder files such as `OneDrive/.849C9593-...` (`os error 32`)

These carry no user data. They were excluded in a personal config, but the starter
configuration does not exclude them.

**Fix:** add `**/.lock`, `**/*.sqlite-maintenance.lock`, `**/*.sock` and `**/.codex/tmp`
to the template `exclude` list, and skip socket/reparse-point files during collection.
