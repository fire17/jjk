//! Durable local filesystem operations.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ports::filesystem::Filesystem;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Operating-system filesystem adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsFilesystem;

impl Filesystem for OsFilesystem {
    fn atomic_write(&self, destination: &Path, bytes: &[u8]) -> io::Result<()> {
        let parent = destination.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "destination has no parent")
        })?;
        let file_name = destination.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "destination has no file name")
        })?;
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = file_name.to_os_string();
        temp_name.push(format!(".jjk-tmp-{}-{sequence}", std::process::id()));
        let temp = parent.join(temp_name);

        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            fs::rename(&temp, destination)?;
            OpenOptions::new().read(true).open(parent)?.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }

    fn symlink_metadata_exists(&self, path: &Path) -> io::Result<bool> {
        match fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn canonicalize_beneath(&self, root: &Path, candidate: &Path) -> io::Result<PathBuf> {
        let canonical_root = fs::canonicalize(root)?;
        let canonical_candidate = fs::canonicalize(candidate)?;
        if canonical_candidate == canonical_root
            || !canonical_candidate.starts_with(&canonical_root)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "candidate path escapes the registered root",
            ));
        }
        Ok(canonical_candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_beneath_rejects_parent_and_symlink_escape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let inside = root.join("inside");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&inside).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let adapter = OsFilesystem;
        assert_eq!(
            adapter.canonicalize_beneath(&root, &inside).unwrap(),
            fs::canonicalize(&inside).unwrap()
        );
        assert!(adapter.canonicalize_beneath(&root, &outside).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
            assert!(
                adapter
                    .canonicalize_beneath(&root, &root.join("escape"))
                    .is_err()
            );
        }
    }
}
