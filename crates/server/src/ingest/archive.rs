use std::path::PathBuf;

use chrono::NaiveDate;
use venti_sources::{Archive, ArchiveEntry, ArchiveError};

/// Keeps raw payloads as files:
/// `<root>/<source>/<YYYY-MM-DD>/<HHMMSS>-<request tag>`.
///
/// One folder per source and day makes the rolling window cheap to keep:
/// old days are removed folder by folder.
#[derive(Debug, Clone)]
pub struct DirArchive {
    root: PathBuf,
}

impl DirArchive {
    /// An archive under `root`. The folder is created when the first payload
    /// arrives.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Removes the payloads of every day before `first_kept` and returns how
    /// many day folders went.
    ///
    /// Only folders named as dates inside the source folders are touched,
    /// so a file someone put beside them survives.
    pub fn prune_before(&self, first_kept: NaiveDate) -> std::io::Result<usize> {
        let sources = match std::fs::read_dir(&self.root) {
            Ok(sources) => sources,
            // Nothing was archived yet.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error),
        };
        let mut removed = 0;
        for source in sources {
            let source = source?.path();
            if !source.is_dir() {
                continue;
            }
            for day in std::fs::read_dir(&source)? {
                let day = day?.path();
                let date = day
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| NaiveDate::parse_from_str(name, "%Y-%m-%d").ok());
                if date.is_some_and(|date| date < first_kept) && day.is_dir() {
                    std::fs::remove_dir_all(&day)?;
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }
}

impl Archive for DirArchive {
    fn store(&self, entry: ArchiveEntry<'_>) -> Result<(), ArchiveError> {
        let day = entry.fetched_at.format("%Y-%m-%d").to_string();
        let directory = self.root.join(entry.source.as_str()).join(day);
        // Tags are chosen by adapters and contain slashes; keep one flat file per payload.
        let tag: String = entry
            .request
            .tag
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let file = directory.join(format!("{}-{tag}", entry.fetched_at.format("%H%M%S")));

        std::fs::create_dir_all(&directory)
            .and_then(|()| std::fs::write(&file, entry.body))
            .map_err(|e| ArchiveError(format!("{}: {e}", file.display())))
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use venti_domain::SourceId;
    use venti_sources::Request;

    use super::*;

    /// A fresh folder under the system's temporary folder.
    fn scratch(test: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("venti-archive-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn store(archive: &DirArchive, day: u32, tag: &str) {
        let source = SourceId::new("dwd-10min").unwrap();
        let request = Request::new(tag, "https://example.org/file");
        archive
            .store(ArchiveEntry {
                source: &source,
                request: &request,
                fetched_at: Utc.with_ymd_and_hms(2026, 11, day, 19, 5, 7).unwrap(),
                body: b"payload",
            })
            .unwrap();
    }

    #[test]
    fn payloads_are_filed_by_source_and_day() {
        let root = scratch("files");
        let archive = DirArchive::new(root.clone());

        store(&archive, 18, "data/wind/05792");

        let file = root.join("dwd-10min/2026-11-18/190507-data_wind_05792");
        assert_eq!(std::fs::read(&file).unwrap(), b"payload");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn pruning_removes_old_days_and_nothing_else() {
        let root = scratch("prune");
        let archive = DirArchive::new(root.clone());
        // Nothing archived yet: nothing to do, and no error.
        assert_eq!(archive.prune_before(NaiveDate::MAX).unwrap(), 0);

        for day in [10, 11, 12] {
            store(&archive, day, "stations/wind");
        }
        let notes = root.join("dwd-10min/notes");
        std::fs::create_dir(&notes).unwrap();
        std::fs::write(root.join("README"), "kept").unwrap();

        let first_kept = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        assert_eq!(archive.prune_before(first_kept).unwrap(), 2);

        assert!(!root.join("dwd-10min/2026-11-10").exists());
        assert!(!root.join("dwd-10min/2026-11-11").exists());
        assert!(root.join("dwd-10min/2026-11-12").exists());
        assert!(notes.exists());
        assert!(root.join("README").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
