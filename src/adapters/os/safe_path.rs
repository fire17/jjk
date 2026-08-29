//! Race-resistant filesystem publication below caller-selected destinations.

use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::io::{self, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

#[cfg(unix)]
use rustix::fd::OwnedFd;
#[cfg(unix)]
use rustix::fs::{self, AtFlags, Mode, OFlags, RenameFlags};

/// A destination whose parent directories have been checked without following symlinks.
///
/// Unix publication is relative to a held parent descriptor and uses a no-replace atomic
/// rename. Windows uses exclusive sibling staging, a no-replace hard link for files, and the
/// platform's non-replacing directory rename semantics.
#[cfg(unix)]
pub(crate) struct SafeDestination {
    parent: OwnedFd,
    name: OsString,
    path: PathBuf,
}

#[cfg(not(unix))]
pub(crate) struct SafeDestination {
    parent: PathBuf,
    path: PathBuf,
}

#[cfg(unix)]
impl SafeDestination {
    /// Resolves a destination through `openat`, refusing symlinks at every level.
    pub(crate) fn new(path: &Path) -> io::Result<Self> {
        let requested = lexical_absolute(path)?;
        let absolute = platform_root_alias(requested.clone());
        let name = absolute
            .file_name()
            .ok_or_else(|| invalid("destination must name a file or directory"))?
            .to_owned();
        let parent_path = absolute
            .parent()
            .ok_or_else(|| invalid("destination must have a parent directory"))?;
        let parent = open_or_create_directory_chain(parent_path)?;
        ensure_absent(&parent, &name, path)?;
        Ok(Self {
            parent,
            name,
            path: requested,
        })
    }

    /// Creates an exclusive temporary file beside the destination.
    pub(crate) fn create_staging_file(&self) -> io::Result<SafeStagingFile> {
        for nonce in 0_u32..128 {
            let name = OsString::from(format!(
                ".jjk-publish-{}-{nonce}.tmp",
                uuid::Uuid::now_v7().simple()
            ));
            match fs::openat(
                &self.parent,
                &name,
                OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            ) {
                Ok(fd) => {
                    let file = File::from(fd);
                    return Ok(SafeStagingFile {
                        parent: self.parent.try_clone()?,
                        name,
                        file,
                        published: false,
                    });
                }
                Err(error) if error == rustix::io::Errno::EXIST => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique staging file",
        ))
    }

    /// Atomically publishes a staged file without replacing another entry.
    pub(crate) fn publish(&self, mut staging: SafeStagingFile) -> io::Result<PathBuf> {
        staging.file.sync_all()?;
        fs::renameat_with(
            &staging.parent,
            &staging.name,
            &self.parent,
            &self.name,
            RenameFlags::NOREPLACE,
        )?;
        fs::fsync(&self.parent)?;
        staging.published = true;
        Ok(self.path.clone())
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn create_staging_directory(&self) -> io::Result<SafeStagingDirectory> {
        for nonce in 0_u32..128 {
            let name = OsString::from(format!(
                ".jjk-publish-{}-{nonce}.dir",
                uuid::Uuid::now_v7().simple()
            ));
            match fs::mkdirat(&self.parent, &name, Mode::RWXU) {
                Ok(()) => {
                    let directory = fs::openat(
                        &self.parent,
                        &name,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )?;
                    let external_path = self.path.parent().expect("destination parent").join(&name);
                    return Ok(SafeStagingDirectory {
                        parent: self.parent.try_clone()?,
                        name,
                        directory,
                        external_path,
                        published: false,
                    });
                }
                Err(error) if error == rustix::io::Errno::EXIST => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique staging directory",
        ))
    }

    pub(crate) fn publish_directory(
        &self,
        mut staging: SafeStagingDirectory,
    ) -> io::Result<PathBuf> {
        staging.external_path_verified()?;
        fs::fsync(&staging.directory)?;
        fs::renameat_with(
            &staging.parent,
            &staging.name,
            &self.parent,
            &self.name,
            RenameFlags::NOREPLACE,
        )?;
        fs::fsync(&self.parent)?;
        staging.published = true;
        Ok(self.path.clone())
    }
}

#[cfg(unix)]
pub(crate) struct SafeStagingFile {
    parent: OwnedFd,
    name: OsString,
    file: File,
    published: bool,
}

#[cfg(unix)]
impl SafeStagingFile {
    pub(crate) fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    pub(crate) fn verify_contents(&mut self, expected: &[u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut offset = 0;
        let mut buffer = [0_u8; 64 * 1024];
        while offset < expected.len() {
            let length = (expected.len() - offset).min(buffer.len());
            self.file.read_exact(&mut buffer[..length])?;
            if buffer[..length] != expected[offset..offset + length] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "staged file differs from verified backup bytes",
                ));
            }
            offset += length;
        }
        let mut trailing = [0_u8; 1];
        if self.file.read(&mut trailing)? != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "staged file contains trailing bytes",
            ));
        }
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for SafeStagingFile {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::unlinkat(&self.parent, &self.name, AtFlags::empty());
        }
    }
}

#[cfg(unix)]
pub(crate) struct SafeStagingDirectory {
    parent: OwnedFd,
    name: OsString,
    directory: OwnedFd,
    external_path: PathBuf,
    published: bool,
}

#[cfg(unix)]
impl SafeStagingDirectory {
    pub(crate) fn path(&self) -> io::Result<PathBuf> {
        self.external_path_verified()
    }

    pub(crate) fn external_path_verified(&self) -> io::Result<PathBuf> {
        let held = fs::fstat(&self.directory)?;
        let visible = fs::statat(&self.parent, &self.name, AtFlags::SYMLINK_NOFOLLOW)?;
        if held.st_dev != visible.st_dev || held.st_ino != visible.st_ino {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "staging directory path no longer identifies the held directory",
            ));
        }
        Ok(self.external_path.clone())
    }

    pub(crate) fn child_path(&self, child: &str) -> io::Result<PathBuf> {
        Ok(self.external_path_verified()?.join(child))
    }
}

#[cfg(unix)]
impl Drop for SafeStagingDirectory {
    fn drop(&mut self) {
        if !self.published {
            if let Ok(path) = self.external_path_verified() {
                if let Ok(entries) = std::fs::read_dir(path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let removal = entry.file_type().map(|kind| kind.is_dir());
                        if matches!(removal, Ok(true)) {
                            let _ = std::fs::remove_dir_all(path);
                        } else {
                            let _ = std::fs::remove_file(path);
                        }
                    }
                }
            }
            let _ = fs::unlinkat(&self.parent, &self.name, AtFlags::REMOVEDIR);
        }
    }
}

fn lexical_absolute(path: &Path) -> io::Result<PathBuf> {
    let input = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in input.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}

            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(invalid("destination escapes its filesystem root"));
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    Ok(normalized)
}

#[cfg(unix)]
fn platform_root_alias(path: PathBuf) -> PathBuf {
    #[cfg(target_os = "macos")]
    for (alias, canonical) in [
        (Path::new("/var"), Path::new("/private/var")),
        (Path::new("/tmp"), Path::new("/private/tmp")),
        (Path::new("/etc"), Path::new("/private/etc")),
    ] {
        if let Ok(remainder) = path.strip_prefix(alias) {
            return canonical.join(remainder);
        }
    }
    path
}

#[cfg(unix)]
fn open_or_create_directory_chain(path: &Path) -> io::Result<OwnedFd> {
    let mut directory = fs::openat(
        rustix::fs::CWD,
        Path::new("/"),
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    for component in path.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        match fs::openat(
            &directory,
            part,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(next) => directory = next,
            Err(error) if error == rustix::io::Errno::NOENT => {
                match fs::mkdirat(&directory, part, Mode::RWXU) {
                    Ok(()) => {}
                    Err(error) if error == rustix::io::Errno::EXIST => {}
                    Err(error) => return Err(error.into()),
                }
                directory = fs::openat(
                    &directory,
                    part,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(directory)
}

#[cfg(unix)]
fn ensure_absent(parent: &OwnedFd, name: &OsString, display_path: &Path) -> io::Result<()> {
    match fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("destination already exists: {}", display_path.display()),
        )),
        Err(error) if error == rustix::io::Errno::NOENT => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(not(unix))]
impl SafeDestination {
    pub(crate) fn new(path: &Path) -> io::Result<Self> {
        let path = lexical_absolute(path)?;
        if path.file_name().is_none() {
            return Err(invalid("destination must name a file or directory"));
        }
        let parent = path
            .parent()
            .ok_or_else(|| invalid("destination must have a parent directory"))?
            .to_path_buf();
        create_verified_directory_chain(&parent)?;
        ensure_path_absent(&path)?;
        Ok(Self { parent, path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn create_staging_file(&self) -> io::Result<SafeStagingFile> {
        for nonce in 0_u32..128 {
            let path = self.parent.join(format!(
                ".jjk-publish-{}-{nonce}.tmp",
                uuid::Uuid::now_v7().simple()
            ));
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(SafeStagingFile {
                        path,
                        file,
                        published: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique staging file",
        ))
    }

    pub(crate) fn publish(&self, mut staging: SafeStagingFile) -> io::Result<PathBuf> {
        staging.file.sync_all()?;
        ensure_path_absent(&self.path)?;
        std::fs::hard_link(&staging.path, &self.path)?;
        staging.published = true;
        let _ = std::fs::remove_file(&staging.path);
        Ok(self.path.clone())
    }

    pub(crate) fn create_staging_directory(&self) -> io::Result<SafeStagingDirectory> {
        for nonce in 0_u32..128 {
            let path = self.parent.join(format!(
                ".jjk-publish-{}-{nonce}.dir",
                uuid::Uuid::now_v7().simple()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(SafeStagingDirectory {
                        path,
                        published: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique staging directory",
        ))
    }

    pub(crate) fn publish_directory(
        &self,
        mut staging: SafeStagingDirectory,
    ) -> io::Result<PathBuf> {
        staging.external_path_verified()?;
        ensure_path_absent(&self.path)?;
        std::fs::rename(&staging.path, &self.path)?;
        staging.published = true;
        Ok(self.path.clone())
    }
}

#[cfg(not(unix))]
pub(crate) struct SafeStagingFile {
    path: PathBuf,
    file: File,
    published: bool,
}

#[cfg(not(unix))]
impl SafeStagingFile {
    pub(crate) fn file_mut(&mut self) -> &mut File {
        &mut self.file
    }

    pub(crate) fn verify_contents(&mut self, expected: &[u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut actual = Vec::new();
        self.file.read_to_end(&mut actual)?;
        if actual == expected {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "staged file differs from verified backup bytes",
            ))
        }
    }
}

#[cfg(not(unix))]
impl Drop for SafeStagingFile {
    fn drop(&mut self) {
        if !self.published {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(not(unix))]
pub(crate) struct SafeStagingDirectory {
    path: PathBuf,
    published: bool,
}

#[cfg(not(unix))]
impl SafeStagingDirectory {
    pub(crate) fn path(&self) -> io::Result<PathBuf> {
        self.external_path_verified()
    }

    pub(crate) fn external_path_verified(&self) -> io::Result<PathBuf> {
        let metadata = std::fs::symlink_metadata(&self.path)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "staging directory path no longer identifies the created directory",
            ));
        }
        Ok(self.path.clone())
    }

    pub(crate) fn child_path(&self, child: &str) -> io::Result<PathBuf> {
        Ok(self.external_path_verified()?.join(child))
    }
}

#[cfg(not(unix))]
impl Drop for SafeStagingDirectory {
    fn drop(&mut self) {
        if !self.published {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(not(unix))]
fn create_verified_directory_chain(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "destination parent is not a direct directory: {}",
                    current.display()
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_path_absent(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("destination already exists: {}", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::SafeDestination;
    use std::fs;
    use std::os::unix::fs::symlink;

    #[test]
    fn replaced_staging_directory_is_never_exposed_or_published() {
        let fixture = tempfile::TempDir::new().expect("tempdir");
        let destination_path = fixture.path().join("published");
        let outside = fixture.path().join("outside");
        fs::create_dir(&outside).expect("outside directory");
        fs::write(outside.join("canary"), b"unchanged").expect("outside canary");

        let destination = SafeDestination::new(&destination_path).expect("safe destination");
        let staging = destination
            .create_staging_directory()
            .expect("staging directory");
        let attacker_path = staging.external_path.clone();
        fs::remove_dir(&attacker_path).expect("unlink staged directory while descriptor is held");
        symlink(&outside, &attacker_path).expect("replace staging name with symlink");

        assert!(
            staging.external_path_verified().is_err(),
            "replacement must fail same-inode verification"
        );
        assert!(
            destination.publish_directory(staging).is_err(),
            "replacement must never publish"
        );
        assert!(
            !destination_path.exists(),
            "destination must remain absent after replacement"
        );
        assert_eq!(
            fs::read(outside.join("canary")).expect("outside canary"),
            b"unchanged"
        );
    }
}
