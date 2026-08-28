//! Operating-system process execution.

use std::process::{Command, Stdio};

use crate::ports::process::{
    CapturedProcess, InheritedProcess, ProcessOutput, ProcessReplacer, ProcessRunner,
    ProcessTermination,
};

/// Native process adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsProcess;

fn termination(status: std::process::ExitStatus) -> ProcessTermination {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ProcessTermination { code: status.code(), signal: status.signal() }
    }
    #[cfg(not(unix))]
    {
        ProcessTermination { code: status.code(), signal: None }
    }
}

impl ProcessRunner for OsProcess {
    fn run_captured(&self, request: &CapturedProcess) -> std::io::Result<ProcessOutput> {
        let mut command = Command::new(&request.executable);
        command.args(&request.args).current_dir(&request.cwd);
        for (name, value) in &request.env_delta {
            if let Some(value) = value {
                command.env(name, value);
            } else {
                command.env_remove(name);
            }
        }
        let output = command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).output()?;
        Ok(ProcessOutput {
            termination: termination(output.status),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    fn run_inherited(&self, request: &InheritedProcess) -> std::io::Result<ProcessTermination> {
        let status = Command::new(&request.executable)
            .args(&request.args)
            .current_dir(&request.cwd)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        Ok(termination(status))
    }
}

impl ProcessReplacer for OsProcess {
    #[cfg(unix)]
    fn replace(&self, request: &InheritedProcess) -> std::io::Error {
        use std::os::unix::process::CommandExt;
        Command::new(&request.executable)
            .args(&request.args)
            .current_dir(&request.cwd)
            .exec()
    }

    #[cfg(not(unix))]
    fn replace(&self, request: &InheritedProcess) -> std::io::Error {
        match self.run_inherited(request) {
            Ok(result) => std::process::exit(result.code.unwrap_or(1)),
            Err(error) => error,
        }
    }
}
