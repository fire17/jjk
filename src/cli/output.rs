//! Output policy and lossless parsing of JJK-owned presentation flags.

use std::env;
use std::ffi::{OsStr, OsString};
use std::io::IsTerminal;
use std::ops::Range;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    Human,
    Json,
    Quiet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputPolicy {
    pub mode: OutputMode,
    pub color: bool,
    pub terminal_width: usize,
    pub is_terminal: bool,
}

impl OutputPolicy {
    #[must_use]
    pub fn detect(mode: OutputMode, color: ColorChoice, requested_width: Option<usize>) -> Self {
        let is_terminal = std::io::stdout().is_terminal();
        let no_color = env::var_os("NO_COLOR").is_some();
        let color = mode == OutputMode::Human
            && !no_color
            && match color {
                ColorChoice::Always => true,
                ColorChoice::Never => false,
                ColorChoice::Auto => is_terminal,
            };
        let terminal_width = requested_width
            .or_else(|| {
                env::var("COLUMNS")
                    .ok()
                    .and_then(|value| value.parse().ok())
            })
            .unwrap_or(80)
            .clamp(20, 1_000);
        Self {
            mode,
            color,
            terminal_width,
            is_terminal,
        }
    }

    #[must_use]
    pub const fn deterministic(
        mode: OutputMode,
        color: bool,
        terminal_width: usize,
        is_terminal: bool,
    ) -> Self {
        Self {
            mode,
            color,
            terminal_width,
            is_terminal,
        }
    }
}

/// Raw args remain available; consumed spans identify owned presentation flags.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedOutput<'a> {
    pub mode: OutputMode,
    pub color: ColorChoice,
    pub width: Option<usize>,
    raw: &'a [OsString],
    consumed: Vec<Range<usize>>,
}

impl<'a> ParsedOutput<'a> {
    #[must_use]
    pub const fn raw(&self) -> &'a [OsString] {
        self.raw
    }
    pub fn semantic_args(&self) -> impl Iterator<Item = &'a OsStr> + '_ {
        self.raw.iter().enumerate().filter_map(|(index, value)| {
            (!self.consumed.iter().any(|span| span.contains(&index))).then_some(value.as_os_str())
        })
    }
    #[must_use]
    pub fn consumes_all(&self) -> bool {
        self.semantic_args().next().is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputParseError {
    MissingValue(&'static str),
    UnsupportedFormat,
    InvalidWidth,
    InvalidColor,
    ConflictingModes,
}

/// Parse output flags for an already-owned command. `--` ends flag parsing.
pub fn parse_native_output(raw: &[OsString]) -> Result<ParsedOutput<'_>, OutputParseError> {
    let mut mode = OutputMode::Human;
    let mut color = ColorChoice::Auto;
    let mut width = None;
    let mut consumed = Vec::new();
    let mut index = 0;
    while index < raw.len() {
        let token = raw[index].as_os_str();
        if token == OsStr::new("--") {
            break;
        }
        let length = if token == OsStr::new("--json") {
            set_mode(&mut mode, OutputMode::Json)?;
            1
        } else if token == OsStr::new("--quiet") {
            set_mode(&mut mode, OutputMode::Quiet)?;
            1
        } else if token == OsStr::new("--no-color") {
            color = ColorChoice::Never;
            1
        } else if token == OsStr::new("--format") {
            if raw
                .get(index + 1)
                .ok_or(OutputParseError::MissingValue("--format"))?
                != OsStr::new("json")
            {
                return Err(OutputParseError::UnsupportedFormat);
            }
            set_mode(&mut mode, OutputMode::Json)?;
            2
        } else if token == OsStr::new("--width") {
            width = Some(parse_width(
                raw.get(index + 1)
                    .ok_or(OutputParseError::MissingValue("--width"))?,
            )?);
            2
        } else if token == OsStr::new("--color") {
            color = parse_color(
                raw.get(index + 1)
                    .ok_or(OutputParseError::MissingValue("--color"))?,
            )?;
            2
        } else if let Some(value) = token
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            if value != "json" {
                return Err(OutputParseError::UnsupportedFormat);
            }
            set_mode(&mut mode, OutputMode::Json)?;
            1
        } else if let Some(value) = token
            .to_str()
            .and_then(|value| value.strip_prefix("--width="))
        {
            width = Some(parse_width(OsStr::new(value))?);
            1
        } else if let Some(value) = token
            .to_str()
            .and_then(|value| value.strip_prefix("--color="))
        {
            color = parse_color(OsStr::new(value))?;
            1
        } else {
            index += 1;
            continue;
        };
        consumed.push(index..index + length);
        index += length;
    }
    Ok(ParsedOutput {
        mode,
        color,
        width,
        raw,
        consumed,
    })
}

/// Parse the closed enhanced-status grammar. `None` means passthrough unchanged.
#[must_use]
pub fn parse_status_output(raw: &[OsString]) -> Option<ParsedOutput<'_>> {
    let mut mode = OutputMode::Human;
    let mut color = ColorChoice::Auto;
    let mut width = None;
    let mut consumed = Vec::new();
    let mut index = 0;
    while index < raw.len() {
        let token = raw[index].as_os_str();
        let length = if token == OsStr::new("--json") {
            mode = OutputMode::Json;
            1
        } else if token == OsStr::new("--no-color") {
            color = ColorChoice::Never;
            1
        } else if token == OsStr::new("--format") {
            if raw.get(index + 1)? != OsStr::new("json") {
                return None;
            }
            mode = OutputMode::Json;
            2
        } else if token == OsStr::new("--width") {
            width = Some(parse_width(raw.get(index + 1)?).ok()?);
            2
        } else if let Some(value) = token
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            if value != "json" {
                return None;
            }
            mode = OutputMode::Json;
            1
        } else if let Some(value) = token
            .to_str()
            .and_then(|value| value.strip_prefix("--width="))
        {
            width = Some(parse_width(OsStr::new(value)).ok()?);
            1
        } else {
            return None;
        };
        consumed.push(index..index + length);
        index += length;
    }
    Some(ParsedOutput {
        mode,
        color,
        width,
        raw,
        consumed,
    })
}

fn set_mode(current: &mut OutputMode, mode: OutputMode) -> Result<(), OutputParseError> {
    if *current != OutputMode::Human && *current != mode {
        return Err(OutputParseError::ConflictingModes);
    }
    *current = mode;
    Ok(())
}
fn parse_width(value: &OsStr) -> Result<usize, OutputParseError> {
    value
        .to_str()
        .and_then(|value| value.parse().ok())
        .filter(|value: &usize| *value > 0)
        .ok_or(OutputParseError::InvalidWidth)
}
fn parse_color(value: &OsStr) -> Result<ColorChoice, OutputParseError> {
    match value.to_str() {
        Some("auto") => Ok(ColorChoice::Auto),
        Some("always") => Ok(ColorChoice::Always),
        Some("never") => Ok(ColorChoice::Never),
        _ => Err(OutputParseError::InvalidColor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn native_output_is_lossless_and_honors_delimiter() {
        let raw = args(&["target", "--format", "json", "--width=42", "--", "--json"]);
        let parsed = parse_native_output(&raw).unwrap();
        assert_eq!((parsed.mode, parsed.width), (OutputMode::Json, Some(42)));
        assert_eq!(
            parsed.semantic_args().collect::<Vec<_>>(),
            [OsStr::new("target"), OsStr::new("--"), OsStr::new("--json")]
        );
        assert_eq!(parsed.raw(), raw);
    }

    #[test]
    fn status_owns_only_closed_presentation_grammar() {
        assert!(
            parse_status_output(&args(&["--format", "json", "--width=80", "--no-color"]))
                .is_some_and(|parsed| parsed.consumes_all())
        );
        for tail in [
            args(&["--porcelain=v2", "-z"]),
            args(&["--format", "yaml"]),
            args(&["--width", "wide"]),
            args(&["--color", "always"]),
        ] {
            assert!(parse_status_output(&tail).is_none(), "{tail:?}");
        }
    }
}
