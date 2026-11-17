//! End-to-end: collect -> backup -> verify -> delete originals -> restore -> compare.

use backuper_core::restore::{ExistingPolicy, plan, restore};
use backuper_core::{archive, collect, config::Config, times, verify};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

const PASSWORD: &str = "correct horse battery staple";

fn build_source(root: &Path) {
    fs::create_dir_all(root.join("docs/sub")).unwrap();
    fs::create_dir_all(root.join("photos")).unwrap();
    fs::create_dir_all(root.join("empty")).unwrap();
    fs::create_dir_all(root.join("app/node_modules/x")).unwrap();
    fs::write(root.join("docs/readme.txt"), "hello ".repeat(10_000)).unwrap();
    fs::write(root.join("docs/sub/.env"), "SECRET=1").unwrap();
    let noise: Vec<u8> = (0..3_000_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    fs::write(root.join("photos/IMG_0001.jpg"), &noise).unwrap();
    fs::write(root.join("app/node_modules/x/index.js"), "skip me").unwrap();
}

#[test]
fn backup_verify_and_restore_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("source");
    let dest = tmp.path().join("drive");
    build_source(&src);

    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000);
    times::apply(&src.join("docs/readme.txt"), Some(old), Some(old)).unwrap();
    times::apply(&src.join("docs"), Some(old), Some(old)).unwrap();

    let mut cfg = Config::template();
    cfg.add_include(&src);
    let collected = collect::collect(&cfg, &[]).unwrap();
    assert_eq!(collected.files.len(), 3, "node_modules must be excluded");

    let outcome = archive::create_backup(&cfg, &collected, &dest, PASSWORD, &mut |_| {}).unwrap();
    assert_eq!(outcome.files, 3);
    assert!(outcome.skipped.is_empty());
    assert!(outcome.archive.extension().unwrap() == "zip");
    assert!(
        !fs::read_dir(&dest).unwrap().any(|e| e
            .unwrap()
            .path()
            .to_string_lossy()
            .ends_with(".partial"))
    );

    let report = verify::verify(&outcome.archive, PASSWORD, &mut |_| {}).unwrap();
    assert!(report.ok(), "{:?}", report.problems);
    assert_eq!(report.files_checked, 3);

    assert!(verify::verify(&outcome.archive, "wrong", &mut |_| {}).is_err());

    let original_readme = fs::read(src.join("docs/readme.txt")).unwrap();
    let original_photo = fs::read(src.join("photos/IMG_0001.jpg")).unwrap();
    fs::remove_dir_all(&src).unwrap();

    let (mut zip, p) = plan(&outcome.archive, PASSWORD).unwrap();
    assert!(p.missing_drives.is_empty());
    assert!(p.existing.is_empty());
    let r = restore(
        &mut zip,
        &p,
        PASSWORD,
        ExistingPolicy::Overwrite,
        &mut |_| {},
    )
    .unwrap();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.restored, 3);

    assert_eq!(
        fs::read(src.join("docs/readme.txt")).unwrap(),
        original_readme
    );
    assert_eq!(
        fs::read(src.join("photos/IMG_0001.jpg")).unwrap(),
        original_photo
    );
    assert_eq!(
        fs::read_to_string(src.join("docs/sub/.env")).unwrap(),
        "SECRET=1"
    );
    assert!(src.join("empty").is_dir(), "empty folders are restored");
    assert!(!src.join("app/node_modules").exists());
    assert_eq!(times::read(&src.join("docs/readme.txt")).0, Some(old));
    if cfg!(windows) {
        assert_eq!(times::read(&src.join("docs/readme.txt")).1, Some(old));
        assert_eq!(
            times::read(&src.join("docs")).1,
            Some(old),
            "folder creation time restored"
        );
    }

    // A second restore with Skip leaves existing files untouched.
    fs::write(src.join("docs/readme.txt"), "changed").unwrap();
    let (mut zip, p) = plan(&outcome.archive, PASSWORD).unwrap();
    assert_eq!(p.existing.len(), 3);
    let r = restore(&mut zip, &p, PASSWORD, ExistingPolicy::Skip, &mut |_| {}).unwrap();
    assert_eq!(r.skipped_existing, 3);
    assert_eq!(
        fs::read_to_string(src.join("docs/readme.txt")).unwrap(),
        "changed"
    );
}

#[test]
fn files_vanishing_mid_backup_are_reported_not_fatal() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("source");
    build_source(&src);
    let mut cfg = Config::template();
    cfg.add_include(&src);
    let collected = collect::collect(&cfg, &[]).unwrap();
    fs::remove_file(src.join("docs/sub/.env")).unwrap();

    let outcome = archive::create_backup(
        &cfg,
        &collected,
        &tmp.path().join("d"),
        PASSWORD,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(outcome.files, 2);
    assert_eq!(outcome.skipped.len(), 1);
    assert!(
        verify::verify(&outcome.archive, PASSWORD, &mut |_| {})
            .unwrap()
            .ok()
    );
}
