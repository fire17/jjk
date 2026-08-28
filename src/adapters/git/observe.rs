//! Bounded Git capability observation.

use std::ffi::OsString;
use std::path::Path;

use crate::adapters::git::command::{GitError, native, trim_line};
use crate::adapters::git::GitCli;
use crate::ports::git::{GitCapabilities, GitPort};
use crate::ports::process::ProcessRunner;
use crate::ports::repository::ObjectFormat;

impl<R: ProcessRunner> GitCli<R> {
    /// Observe Git installation and repository capabilities.
    pub fn capabilities(&self, cwd: &Path) -> Result<GitCapabilities, GitError> {
        let version = self.required(cwd, [OsString::from("--version")])?;
        let version = trim_line(version);
        let version = version.strip_prefix(b"git version ").unwrap_or(&version).to_vec();
        let version = native(version, "Git version")?;

        match self.discover(cwd) {
            Ok(discovery) => {
                let linked_worktree = discovery.git_dir != discovery.common_dir;
                Ok(GitCapabilities {
                    version,
                    object_format: Some(discovery.object_format),
                    common_dir: Some(discovery.common_dir),
                    linked_worktree,
                })
            }
            Err(GitError::Command { .. }) => Ok(GitCapabilities {
                version,
                object_format: None,
                common_dir: None,
                linked_worktree: false,
            }),
            Err(error) => Err(error),
        }
    }
}

impl<R: ProcessRunner> GitPort for GitCli<R> {
    type Error = GitError;

    fn capabilities(&self, cwd: &Path) -> Result<GitCapabilities, Self::Error> {
        GitCli::capabilities(self, cwd)
    }

    fn query<I, S>(&self, cwd: &Path, args: I) -> Result<crate::ports::git::GitOutput, Self::Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.run(cwd, args)
    }
}

/// Parse an object format reported independently of full repository discovery.
pub fn parse_object_format(value: &std::ffi::OsStr) -> ObjectFormat {
    match value.to_str() {
        Some("sha1") => ObjectFormat::Sha1,
        Some("sha256") => ObjectFormat::Sha256,
        _ => ObjectFormat::Other(value.to_os_string()),
    }
}
