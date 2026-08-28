//! Repository discovery through authoritative Git plumbing.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::adapters::git::command::{GitError, native, trim_line};
use crate::adapters::git::GitCli;
use crate::ports::process::ProcessRunner;
pub use crate::ports::repository::{ObjectFormat, RepositoryDiscovery};
use crate::ports::repository::RepositoryDiscoveryPort;

impl<R: ProcessRunner> GitCli<R> {
    /// Discover worktree, git-dir, common-dir, repository form, and object format.
    pub fn discover(&self, cwd: &Path) -> Result<RepositoryDiscovery, GitError> {
        let output = self.required(cwd, [
            OsString::from("rev-parse"),
            OsString::from("--path-format=absolute"),
            OsString::from("--git-dir"),
            OsString::from("--git-common-dir"),
            OsString::from("--show-toplevel"),
            OsString::from("--is-bare-repository"),
            OsString::from("--is-inside-work-tree"),
            OsString::from("--show-object-format"),
        ])?;
        let mut lines = output.split(|byte| *byte == b'\n');
        let git_dir = path_line(lines.next(), "git directory")?;
        let common_dir = path_line(lines.next(), "Git common directory")?;
        let top = lines.next().ok_or_else(|| invalid("worktree root", "missing line"))?;
        let worktree_root = if top.is_empty() { None } else { Some(path(top, "worktree root")?) };
        let is_bare = boolean(lines.next(), "bare repository")?;
        let inside_worktree = boolean(lines.next(), "inside worktree")?;
        let object_format = match lines.next().unwrap_or_default() {
            b"sha1" => ObjectFormat::Sha1,
            b"sha256" => ObjectFormat::Sha256,
            other if !other.is_empty() => ObjectFormat::Other(native(other.to_vec(), "object format")?),
            _ => return Err(invalid("object format", "missing line")),
        };
        Ok(RepositoryDiscovery {
            invocation_dir: cwd.to_path_buf(),
            worktree_root,
            git_dir,
            common_dir,
            is_bare,
            inside_worktree,
            object_format,
        })
    }
}

impl<R: ProcessRunner> RepositoryDiscoveryPort for GitCli<R> {
    type Error = GitError;

    fn discover(&self, cwd: &Path) -> Result<RepositoryDiscovery, Self::Error> {
        GitCli::discover(self, cwd)
    }
}

fn invalid(field: &'static str, detail: impl Into<String>) -> GitError {
    GitError::InvalidOutput { field, detail: detail.into() }
}

fn boolean(line: Option<&[u8]>, field: &'static str) -> Result<bool, GitError> {
    match line {
        Some(b"true") => Ok(true),
        Some(b"false") => Ok(false),
        Some(value) => Err(invalid(field, format!("expected boolean, got {:?}", String::from_utf8_lossy(value)))),
        None => Err(invalid(field, "missing line")),
    }
}

fn path_line(line: Option<&[u8]>, field: &'static str) -> Result<PathBuf, GitError> {
    path(line.ok_or_else(|| invalid(field, "missing line"))?, field)
}

fn path(bytes: &[u8], field: &'static str) -> Result<PathBuf, GitError> {
    if bytes.is_empty() {
        return Err(invalid(field, "empty path"));
    }
    Ok(PathBuf::from(native(trim_line(bytes.to_vec()), field)?))
}
