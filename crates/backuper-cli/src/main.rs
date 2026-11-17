//! `backuper` command-line interface.

mod progress;

use anyhow::{Context, Result, bail};
use backuper_core::collect::collect;
use backuper_core::config::Config;
use backuper_core::restore::{ExistingPolicy, plan, restore};
use backuper_core::{archive, drive, lockcheck, scan, verify};
use clap::{Parser, Subcommand};
use dialoguer::{Confirm, Password, Select, theme::ColorfulTheme};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "backuper",
    version,
    about = "Encrypted backups of your important files, restorable to the same paths"
)]
struct Cli {
    /// Use a configuration file other than the default (%APPDATA%\backuper\config.toml).
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a starter configuration file.
    Init {
        /// Overwrite an existing configuration.
        #[arg(long)]
        force: bool,
    },
    /// Open the configuration file in Notepad.
    Edit,
    /// Find new items in watched folders and choose whether to back them up.
    Scan,
    /// Show what a backup would contain without writing anything.
    Preview,
    /// Create an encrypted backup on the external drive.
    Backup {
        /// Write to this folder instead of the marked backup drive.
        #[arg(long)]
        dest: Option<PathBuf>,
        /// Skip the new-item scan.
        #[arg(long)]
        no_scan: bool,
        /// Skip verification after writing.
        #[arg(long)]
        no_verify: bool,
    },
    /// Check that a backup can be fully decrypted and read.
    Verify { archive: PathBuf },
    /// Restore a backup to the original locations.
    Restore {
        archive: PathBuf,
        /// Only show what would be restored.
        #[arg(long)]
        dry_run: bool,
    },
    /// List backups on the backup drive.
    List {
        #[arg(long)]
        dest: Option<PathBuf>,
    },
    /// Mark a drive or folder as the backup destination (e.g. `backuper init-drive E:\`).
    InitDrive { root: PathBuf },
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let cfg_path = match cli.config {
        Some(p) => p,
        None => Config::default_path()?,
    };
    match cli.command {
        Command::Init { force } => init(&cfg_path, force),
        Command::Edit => {
            ensure_config(&cfg_path)?;
            std::process::Command::new("notepad")
                .arg(&cfg_path)
                .status()?;
            Ok(())
        }
        Command::Scan => {
            let mut cfg = load(&cfg_path)?;
            scan_interactive(&mut cfg, &cfg_path)
        }
        Command::Preview => preview(&load(&cfg_path)?, &cfg_path),
        Command::Backup {
            dest,
            no_scan,
            no_verify,
        } => backup(&cfg_path, dest, no_scan, no_verify),
        Command::Verify { archive } => {
            let pw = ask_password(false)?;
            run_verify(&archive, &pw)
        }
        Command::Restore { archive, dry_run } => run_restore(&archive, dry_run),
        Command::List { dest } => list(dest),
        Command::InitDrive { root } => {
            let dir = drive::mark(&root)?;
            println!(
                "Marked {} as a backup drive. Backups will be written to {}",
                root.display(),
                dir.display()
            );
            Ok(())
        }
    }
}

fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

fn init(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            path.display()
        );
    }
    Config::template().save(path)?;
    println!("Created {}", path.display());
    println!("Edit it with `backuper edit`, then run `backuper scan`.");
    Ok(())
}

fn ensure_config(path: &Path) -> Result<()> {
    if !path.exists() {
        Config::template().save(path)?;
        println!("Created a starter configuration at {}", path.display());
    }
    Ok(())
}

fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        bail!(
            "no configuration at {}. Run `backuper init` first.",
            path.display()
        );
    }
    Config::load(path)
}

fn scan_interactive(cfg: &mut Config, cfg_path: &Path) -> Result<()> {
    let found = scan::find_new(cfg)?;
    if found.is_empty() {
        println!("No new items in watched folders.");
        return Ok(());
    }
    println!("Found {} new item(s) in watched folders.", found.len());
    let choices = ["Include in backups", "Ignore permanently", "Decide later"];
    for path in found {
        let kind = if path.is_dir() { "folder" } else { "file" };
        let pick = Select::with_theme(&theme())
            .with_prompt(format!("New {kind}: {}", path.display()))
            .items(choices)
            .default(0)
            .interact()?;
        match pick {
            0 => cfg.add_include(&path),
            1 => cfg.add_ignore(&path),
            _ => continue,
        }
        cfg.save(cfg_path)?; // save after each answer so nothing is lost on Ctrl+C
    }
    Ok(())
}

fn preview(cfg: &Config, cfg_path: &Path) -> Result<()> {
    let c = collect(cfg, &[cfg_path.to_path_buf()])?;
    println!(
        "{} files, {} folders, {}",
        c.files.len(),
        c.dirs.len(),
        progress::human(c.total_bytes())
    );
    for m in &c.missing {
        println!("  missing: {}", m.display());
    }
    for (p, e) in &c.errors {
        println!("  unreadable: {} ({e})", p.display());
    }
    Ok(())
}

fn resolve_dest(dest: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(d) = dest {
        return Ok(d);
    }
    let drives = drive::find_backup_drives();
    match drives.len() {
        0 => bail!(
            "no backup drive found. Plug it in, or mark it once with `backuper init-drive E:\\`, or pass --dest"
        ),
        1 => Ok(drive::backup_dir(&drives[0])),
        _ => {
            let labels: Vec<String> = drives.iter().map(|d| d.display().to_string()).collect();
            let i = Select::with_theme(&theme())
                .with_prompt("Several backup drives found. Which one?")
                .items(&labels)
                .default(0)
                .interact()?;
            Ok(drive::backup_dir(&drives[i]))
        }
    }
}

fn backup(cfg_path: &Path, dest: Option<PathBuf>, no_scan: bool, no_verify: bool) -> Result<()> {
    let mut cfg = load(cfg_path)?;
    let dest = resolve_dest(dest)?;
    if !no_scan {
        scan_interactive(&mut cfg, cfg_path)?;
    }
    if cfg.include.is_empty() {
        bail!("nothing to back up: `include` is empty. Run `backuper scan` or `backuper edit`.");
    }

    println!("Collecting files...");
    let mut collected = collect(&cfg, &[cfg_path.to_path_buf()])?;
    for m in &collected.missing {
        println!("  warning: included path does not exist: {}", m.display());
    }
    println!(
        "{} files, {}",
        collected.files.len(),
        progress::human(collected.total_bytes())
    );

    // Pre-flight: programs holding files open.
    loop {
        let apps = lockcheck::running_lock_apps(&cfg);
        let bar = progress::count_bar(collected.files.len() as u64, "Checking files");
        let locked = lockcheck::locked_files(&collected.files, |n| bar.set_position(n as u64));
        bar.finish_and_clear();
        if apps.is_empty() && locked.is_empty() {
            break;
        }
        if !apps.is_empty() {
            println!(
                "\nThese programs are running and may hold files open: {}",
                apps.join(", ")
            );
        }
        if !locked.is_empty() {
            println!("\n{} file(s) cannot be read right now:", locked.len());
            for l in locked.iter().take(20) {
                println!("  {}  ({})", l.path.display(), l.reason);
            }
            if locked.len() > 20 {
                println!("  ... and {} more", locked.len() - 20);
            }
        }
        println!("Close the programs above for a complete backup.");
        let pick = Select::with_theme(&theme())
            .with_prompt("What next?")
            .items(["Retry (after closing programs)", "Continue anyway", "Abort"])
            .default(0)
            .interact()?;
        match pick {
            0 => continue,
            1 => {
                if !locked.is_empty() {
                    println!(
                        "WARNING: {} file(s) will NOT be included in this backup. They are listed in the report.",
                        locked.len()
                    );
                }
                let locked_keys: std::collections::HashSet<_> =
                    locked.iter().map(|l| l.path.clone()).collect();
                collected.files.retain(|f| !locked_keys.contains(&f.path));
                collected
                    .errors
                    .extend(locked.into_iter().map(|l| (l.path, l.reason)));
                break;
            }
            _ => bail!("aborted"),
        }
    }

    let password = ask_password(true)?;
    println!("Writing backup to {}", dest.display());
    let mut reporter = progress::BytesReporter::new("Backing up");
    let outcome = archive::create_backup(&cfg, &collected, &dest, &password, &mut |p| {
        reporter.handle(p)
    })?;
    reporter.finish();

    if !outcome.skipped.is_empty() {
        let report = outcome.archive.with_extension("skipped.txt");
        let text: String = outcome
            .skipped
            .iter()
            .map(|s| format!("{}\t{}\n", s.path, s.reason))
            .collect();
        std::fs::write(&report, text)?;
        println!(
            "{} file(s) were skipped. See {}",
            outcome.skipped.len(),
            report.display()
        );
    }
    println!(
        "Backed up {} files ({}) to {}",
        outcome.files,
        progress::human(outcome.bytes),
        outcome.archive.display()
    );

    if !no_verify {
        run_verify(&outcome.archive, &password)?;
    }
    Ok(())
}

fn ask_password(confirm: bool) -> Result<String> {
    let theme = theme();
    let mut p = Password::with_theme(&theme).with_prompt("Backup password");
    if confirm {
        p = p.with_confirmation("Repeat password", "Passwords do not match");
        println!("This password is not stored anywhere. Without it the backup cannot be opened.");
    }
    Ok(p.interact()?)
}

fn run_verify(archive_path: &Path, password: &str) -> Result<()> {
    let mut reporter = progress::BytesReporter::new("Verifying");
    let report = verify::verify(archive_path, password, &mut |p| reporter.handle(p))?;
    reporter.finish();
    if report.ok() {
        println!(
            "Verified: {} files, {} - all intact.",
            report.files_checked,
            progress::human(report.bytes_checked)
        );
        Ok(())
    } else {
        for p in &report.problems {
            println!("  {p}");
        }
        bail!(
            "verification failed for {} file(s). Do not rely on this backup.",
            report.problems.len()
        )
    }
}

fn run_restore(archive_path: &Path, dry_run: bool) -> Result<()> {
    let password = ask_password(false)?;
    let (mut zip, plan) = plan(archive_path, &password)?;
    let m = &plan.manifest;
    let total: u64 = m.files.iter().map(|f| f.size).sum();
    println!(
        "Backup from {} ({}): {} files, {}",
        m.created_at,
        if m.computer.is_empty() {
            "unknown computer"
        } else {
            &m.computer
        },
        m.files.len(),
        progress::human(total)
    );
    if !plan.missing_drives.is_empty() {
        let list: Vec<String> = plan
            .missing_drives
            .iter()
            .map(|d| format!("{d}:"))
            .collect();
        bail!(
            "drive(s) {} do not exist on this computer. Files must be restored to their original paths; \
             create or reassign those drive letters (Disk Management) and try again.",
            list.join(", ")
        );
    }
    if dry_run {
        for f in &m.files {
            println!("  {}", f.path);
        }
        println!(
            "{} file(s) already exist and would be affected.",
            plan.existing.len()
        );
        return Ok(());
    }

    let mut policy = ExistingPolicy::Overwrite;
    if !plan.existing.is_empty() {
        println!(
            "{} file(s) already exist at their original location.",
            plan.existing.len()
        );
        let pick = Select::with_theme(&theme())
            .with_prompt("How should existing files be handled?")
            .items([
                "Overwrite with the backup copy",
                "Keep the existing file",
                "Abort",
            ])
            .default(0)
            .interact()?;
        policy = match pick {
            0 => ExistingPolicy::Overwrite,
            1 => ExistingPolicy::Skip,
            _ => bail!("aborted"),
        };
    } else if !Confirm::with_theme(&theme())
        .with_prompt("Restore now?")
        .default(true)
        .interact()?
    {
        bail!("aborted");
    }

    let mut reporter = progress::BytesReporter::new("Restoring");
    let report = restore(&mut zip, &plan, &password, policy, &mut |p| {
        reporter.handle(p)
    })
    .context("restore failed")?;
    reporter.finish();
    println!(
        "Restored {} files ({}); kept {} existing.",
        report.restored,
        progress::human(report.bytes),
        report.skipped_existing
    );
    if !m.skipped.is_empty() {
        println!(
            "Note: {} file(s) were not included when this backup was made.",
            m.skipped.len()
        );
    }
    if !report.errors.is_empty() {
        for e in &report.errors {
            println!("  {e}");
        }
        bail!("{} item(s) could not be restored", report.errors.len());
    }
    Ok(())
}

fn list(dest: Option<PathBuf>) -> Result<()> {
    let dir = resolve_dest(dest)?;
    let backups = drive::list_backups(&dir);
    if backups.is_empty() {
        println!("No backups in {}", dir.display());
    }
    for b in backups {
        println!("{}  {}", progress::human(b.size), b.path.display());
    }
    Ok(())
}
