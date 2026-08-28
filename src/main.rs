//! JJK command-line entry point.

use jjk::adapters::git::passthrough;
use jjk::adapters::os::process::OsProcess;
use jjk::cli::definition::{PASSTHROUGH_HELP, command_descriptors};
use jjk::cli::exit::ExitCode;
use jjk::cli::input::RawInvocation;
use jjk::cli::route::{CommandClass, Route, route};
use jjk::render::json::{EnvelopeMeta, MachineError, render_error_with_meta};
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;

const PROGRAM: &str = "jjk";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ErrorOutput {
    Human,
    Json,
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let invocation = match RawInvocation::current() {
        Ok(invocation) => invocation,
        Err(error) => return report_io("capture the current invocation", &error),
    };
    if let Some(command) = owned_help_request(&invocation.argv) {
        return print_command_help(command);
    }
    if let Some(git_argv) = explicit_git_escape(&invocation.argv) {
        return delegate_git(git_argv, &invocation.cwd);
    }
    let (selected, argv) = route(invocation.argv);

    match selected {
        Route::Help => print_help(),
        Route::Version => print_version(),
        Route::Passthrough => delegate_git(argv, &invocation.cwd),
        Route::Enhanced(name) => dispatch_claimed(name, &argv, &invocation.cwd, true),
        Route::Native(name) => dispatch_claimed(name, &argv, &invocation.cwd, false),
    }
}

fn print_version() -> i32 {
    println!("{PROGRAM} {VERSION}");
    ExitCode::Success.get()
}

fn print_help() -> i32 {
    println!("{PROGRAM} {VERSION}\n{DESCRIPTION}\n");
    println!("USAGE:\n    {PROGRAM} <COMMAND> [ARGS...]\n");
    println!("COMMANDS:");
    for descriptor in command_descriptors() {
        let marker = match descriptor.claim.class {
            CommandClass::JjkNative => "native",
            CommandClass::GitEnhanced => "enhanced",
            CommandClass::TransparentGitPassthrough => "git",
        };
        println!(
            "    {:<12} {:<8} {}",
            descriptor.claim.name, marker, descriptor.summary
        );
    }
    println!("\n    help, -h, --help       Show this help");
    println!("    version, -V, --version Show version");
    println!("\nMore: jjk help <command> or jjk <command> --help");
    println!("\n{PASSTHROUGH_HELP}");
    ExitCode::Success.get()
}
fn owned_help_request(argv: &[OsString]) -> Option<&'static str> {
    let requested = match argv {
        [help, command] if help == "help" => command.to_str(),
        [command, help] if help == "--help" || help == "-h" => command.to_str(),
        _ => None,
    }?;
    command_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.claim.name == requested)
        .map(|descriptor| descriptor.claim.name)
}

fn print_command_help(command: &str) -> i32 {
    let Some(descriptor) = command_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.claim.name == command)
    else {
        return ExitCode::Usage.get();
    };
    let class = match descriptor.claim.class {
        CommandClass::JjkNative => "JJK-native",
        CommandClass::GitEnhanced => "Git-enhanced",
        CommandClass::TransparentGitPassthrough => "Git passthrough",
    };
    println!("{command} — {}", descriptor.summary);
    println!("\nUsage: jjk {command} [arguments]");
    println!("\nClass: {class}");
    println!("\nUse `jjk --help` to return to the stable command index.");
    ExitCode::Success.get()
}

fn explicit_git_escape(argv: &[OsString]) -> Option<Vec<OsString>> {
    match argv {
        [command, separator, git_argv @ ..] if command == "git" && separator == "--" => {
            Some(git_argv.to_vec())
        }
        _ => None,
    }
}

fn dispatch_claimed(name: &str, argv: &[OsString], cwd: &Path, enhanced: bool) -> i32 {
    let tail = &argv[1..];
    let output = match parse_output_flags(tail) {
        Ok(Some(output)) => output,
        Ok(None) if enhanced => return delegate_git(argv.to_vec(), cwd),
        Ok(None) => requested_error_output(tail),
        Err(message) => {
            return report_error(
                ExitCode::Usage,
                "USAGE",
                &message,
                requested_error_output(tail),
                &[],
            );
        }
    };
    match jjk::runtime::dispatch_native(name, tail, cwd) {
        Ok(code) => code,
        Err(error) => report_runtime_error(error, output),
    }
}

fn requested_error_output(args: &[OsString]) -> ErrorOutput {
    for (index, argument) in args.iter().enumerate() {
        match argument.to_str() {
            Some("--json" | "--format=json") => return ErrorOutput::Json,
            Some("--format")
                if args.get(index + 1).and_then(|value| value.to_str()) == Some("json") =>
            {
                return ErrorOutput::Json;
            }
            _ => {}
        }
    }
    ErrorOutput::Human
}

fn report_runtime_error(error: jjk::runtime::RuntimeError, output: ErrorOutput) -> i32 {
    let original = error.to_string();
    let lower = original.to_ascii_lowercase();
    if lower.contains("jjk is not initialized") {
        let recovery = [vec!["jjk", "setup"]];
        return report_error(
            ExitCode::Unavailable,
            "NOT_INITIALIZED",
            "JJK is not initialized; run `jjk setup`",
            output,
            &recovery,
        );
    }

    let (exit, machine_code) = match error.exit_code() {
        2 => (ExitCode::Usage, "USAGE"),
        3 if lower.contains("recovery required")
            || lower.contains("requires recovery")
            || lower.contains("repair required")
            || lower.contains("requires repair") =>
        {
            (ExitCode::RecoveryRequired, "RECOVERY_REQUIRED")
        }
        3 if lower.contains("conflict") || lower.starts_with("refusing ") => {
            (ExitCode::Conflict, "CONFLICT")
        }
        3 => (ExitCode::Unavailable, "UNAVAILABLE"),
        _ => (ExitCode::Internal, "INTERNAL"),
    };
    report_error(exit, machine_code, &original, output, &[])
}

/// Parse presentation-only arguments. `None` means at least one token belongs to Git or
/// the command itself; enhanced commands must then preserve the complete argv via passthrough.
fn parse_output_flags(args: &[OsString]) -> Result<Option<ErrorOutput>, String> {
    let mut output = ErrorOutput::Human;
    let mut index = 0;
    while index < args.len() {
        let Some(arg) = args[index].to_str() else {
            return Ok(None);
        };
        match arg {
            "--json" | "--format=json" => output = ErrorOutput::Json,
            "--format=human" => output = ErrorOutput::Human,
            "--no-color" => {}
            "--format" => {
                index += 1;
                let Some(value) = args.get(index).and_then(|value| value.to_str()) else {
                    return Err("`--format` requires `human` or `json`".into());
                };
                output = match value {
                    "human" => ErrorOutput::Human,
                    "json" => ErrorOutput::Json,
                    _ => {
                        return Err(format!(
                            "unsupported output format `{value}`; expected `human` or `json`"
                        ));
                    }
                };
            }
            "--width" => {
                index += 1;
                let Some(value) = args.get(index).and_then(|value| value.to_str()) else {
                    return Err("`--width` requires a positive integer".into());
                };
                parse_width(value)?;
            }
            value if value.starts_with("--width=") => parse_width(&value[8..])?,
            _ => return Ok(None),
        }
        index += 1;
    }
    Ok(Some(output))
}

fn parse_width(value: &str) -> Result<(), String> {
    if value.parse::<usize>().is_ok_and(|width| width > 0) {
        Ok(())
    } else {
        Err(format!(
            "invalid width `{value}`; expected a positive integer"
        ))
    }
}
fn delegate_git(argv: Vec<OsString>, cwd: &Path) -> i32 {
    let request = passthrough("git", argv, cwd);
    #[cfg(unix)]
    {
        let error = request.exec(&OsProcess);
        report_git_launch(&error)
    }
    #[cfg(not(unix))]
    {
        match request.supervise(&OsProcess) {
            Ok(termination) => jjk::cli::exit::passthrough_exit(termination),
            Err(error) => report_git_launch(&error),
        }
    }
}

fn report_git_launch(error: &io::Error) -> i32 {
    let code = if error.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    };
    let _ = writeln!(
        io::stderr().lock(),
        "{PROGRAM}: could not execute real Git: {error}"
    );
    code
}

fn report_io(action: &str, error: &io::Error) -> i32 {
    let _ = writeln!(
        io::stderr().lock(),
        "{PROGRAM}: could not {action}: {error}"
    );
    ExitCode::Internal.get()
}

fn report_error(
    code: ExitCode,
    machine_code: &str,
    message: &str,
    output: ErrorOutput,
    recovery_commands: &[Vec<&str>],
) -> i32 {
    match output {
        ErrorOutput::Human => {
            let _ = writeln!(io::stderr().lock(), "{PROGRAM}: error: {message}");
        }
        ErrorOutput::Json => {
            let machine_error = MachineError {
                code: machine_code,
                message,
                subject_ids: &[],
                retryable: false,
                recovery_commands,
            };
            match render_error_with_meta(
                &machine_error,
                EnvelopeMeta {
                    outcome: "failed",
                    ..EnvelopeMeta::default()
                },
                &[],
            ) {
                Ok(rendered) => {
                    let _ = writeln!(io::stderr().lock(), "{rendered}");
                }
                Err(error) => {
                    let _ = writeln!(
                        io::stderr().lock(),
                        "{PROGRAM}: error: {message} (JSON rendering failed: {error})"
                    );
                    return ExitCode::Internal.get();
                }
            }
        }
    }
    code.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn native_semantic_arguments_retain_requested_json_error_mode() {
        let tail = args(&["--json", "--", "message"]);
        assert_eq!(parse_output_flags(&tail), Ok(None));
        assert_eq!(requested_error_output(&tail), ErrorOutput::Json);
        let tail = args(&[
            "state", "--format", "json", "--suite", "focused", "--", "false",
        ]);
        assert_eq!(parse_output_flags(&tail), Ok(None));
        assert_eq!(requested_error_output(&tail), ErrorOutput::Json);
    }

    #[test]
    fn semantic_arguments_without_json_keep_human_errors() {
        assert_eq!(
            requested_error_output(&args(&["--", "message"])),
            ErrorOutput::Human
        );
    }

    #[test]
    fn requires_repair_phrase_maps_to_recovery_required() {
        let error =
            jjk::runtime::RuntimeError::Unavailable("operation op-test requires repair".into());
        assert_eq!(error.exit_code(), 3);
        let lower = error.to_string().to_ascii_lowercase();
        assert!(lower.contains("requires repair"));
    }
}
