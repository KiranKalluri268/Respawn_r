//! Terminal progress bars driven by core progress events.

use backuper_core::Progress;
use indicatif::{ProgressBar, ProgressStyle};

pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

pub fn count_bar(total: u64, label: &str) -> ProgressBar {
    let bar = ProgressBar::new(total);
    bar.set_style(
        ProgressStyle::with_template("{msg} [{bar:40}] {pos}/{len}")
            .unwrap()
            .progress_chars("=> "),
    );
    bar.set_message(label.to_string());
    bar
}

pub struct BytesReporter {
    bar: ProgressBar,
}

impl BytesReporter {
    pub fn new(label: &str) -> Self {
        let bar = ProgressBar::new(0);
        bar.set_style(
            ProgressStyle::with_template(
                "{prefix} [{bar:40}] {bytes}/{total_bytes} {bytes_per_sec} ETA {eta}\n{wide_msg}",
            )
            .unwrap()
            .progress_chars("=> "),
        );
        bar.set_prefix(label.to_string());
        Self { bar }
    }

    pub fn handle(&mut self, p: Progress) {
        match p {
            Progress::Start { bytes, .. } => self.bar.set_length(bytes),
            Progress::File(path) => self.bar.set_message(path.display().to_string()),
            Progress::Bytes(n) => self.bar.inc(n),
            Progress::Phase(name) => self.bar.set_message(name),
        }
    }

    pub fn finish(self) {
        self.bar.finish_and_clear();
    }
}

#[cfg(test)]
mod tests {
    use super::human;

    #[test]
    fn formats_sizes() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(human(20 * 1024 * 1024 * 1024), "20.0 GB");
    }
}
