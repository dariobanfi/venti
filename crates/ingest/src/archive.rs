use std::path::PathBuf;

use venti_sources::{Archive, ArchiveEntry, ArchiveError};

/// Keeps raw payloads as files:
/// `<root>/<source>/<YYYY-MM-DD>/<HHMMSS>-<request tag>`.
pub(crate) struct DirArchive {
    root: PathBuf,
}

impl DirArchive {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
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
