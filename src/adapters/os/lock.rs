use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use fs2::FileExt;
use thiserror::Error;

use crate::ports::lock::{LockOwner, WriterLock};

#[derive(Debug, Error)]
pub(crate) enum LockError {
    #[error("writer lock filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("writer lock at {path} is held by {owner:?}")]
    Busy {
        path: PathBuf,
        owner: Option<String>,
    },
}

pub(crate) struct OsWriterLock {
    path: PathBuf,
}

impl OsWriterLock {
    pub(crate) fn new(git_common_dir: &Path) -> Self {
        Self {
            path: git_common_dir.join("jjk").join("writer.lock"),
        }
    }
}

pub(crate) struct OsWriterGuard {
    file: File,
}
impl Drop for OsWriterGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

impl WriterLock for OsWriterLock {
    type Guard = OsWriterGuard;
    type Error = LockError;

    fn try_acquire(&self, timeout: Duration, owner: LockOwner) -> Result<Self::Guard, Self::Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&self.path)?;
        let deadline = Instant::now() + timeout;
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => {
                    let description = format!(
                        "pid={} operation={}\n",
                        owner.process_id,
                        owner.operation.as_deref().unwrap_or("unknown")
                    );
                    file.set_len(0)?;
                    file.seek(SeekFrom::Start(0))?;
                    file.write_all(description.as_bytes())?;
                    file.sync_data()?;
                    return Ok(OsWriterGuard { file });
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    let mut owner = String::new();
                    file.seek(SeekFrom::Start(0))?;
                    file.read_to_string(&mut owner)?;
                    return Err(LockError::Busy {
                        path: self.path.clone(),
                        owner: (!owner.is_empty()).then_some(owner),
                    });
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}
