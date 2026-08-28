//! Non-mutating optional-JJ capability probe.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::ports::jj::JjCapabilities;
use crate::ports::process::{CapturedProcess, ProcessRunner};

/// Probe optional JJ. Every failure becomes explicit Git-only degradation.
#[must_use]
pub fn probe(runner: &impl ProcessRunner, executable: impl Into<PathBuf>, cwd: &Path) -> JjCapabilities {
    let executable = executable.into();
    let version = run(runner, &executable, cwd, ["--version"]);
    let version = match version {
        Ok(output) if output.termination.success() => Some(text(&output.stdout)),
        Ok(output) => {
            return JjCapabilities::Degraded {
                version: None,
                diagnostic: diagnostic(&output.stderr, output.termination.code),
            };
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return JjCapabilities::Unavailable,
        Err(error) => return JjCapabilities::Degraded { version: None, diagnostic: error.to_string() },
    };

    match run(runner, &executable, cwd, ["root"]) {
        Ok(output) if output.termination.success() => JjCapabilities::Available {
            version: version.unwrap_or_default(),
            root: PathBuf::from(text(&output.stdout)),
        },
        Ok(output) if looks_like_not_repo(&output.stderr) => JjCapabilities::Installed {
            version: version.unwrap_or_default(),
        },
        Ok(output) => JjCapabilities::Degraded {
            version,
            diagnostic: diagnostic(&output.stderr, output.termination.code),
        },
        Err(error) => JjCapabilities::Degraded { version, diagnostic: error.to_string() },
    }
}

fn run<const N: usize>(runner: &impl ProcessRunner, executable: &Path, cwd: &Path, args: [&str; N]) -> std::io::Result<crate::ports::process::ProcessOutput> {
    runner.run_captured(&CapturedProcess {
        executable: executable.to_path_buf(),
        args: args.into_iter().map(OsString::from).collect(),
        cwd: cwd.to_path_buf(),
        env_delta: BTreeMap::new(),
    })
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}

fn diagnostic(stderr: &[u8], code: Option<i32>) -> String {
    let message = text(stderr);
    if message.is_empty() { format!("JJ exited with status {}", code.unwrap_or(-1)) } else { message }
}

fn looks_like_not_repo(stderr: &[u8]) -> bool {
    let value = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    value.contains("no jj repo") || value.contains("no jj repository") || value.contains("not a jj repo")
}
