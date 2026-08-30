//! Native-string-safe CLI grammar, routing, input, output, and exit contracts.

pub mod completion;
pub mod definition;
pub mod exit;
pub mod input;
pub mod output;
pub mod route;

pub use output::{
    ColorChoice, OutputMode, OutputParseError, OutputPolicy, ParsedOutput, parse_native_output,
    parse_status_output,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRequest {
    pub mode: OutputMode,
    pub color: ColorChoice,
    pub width: Option<usize>,
}

impl Default for OutputRequest {
    fn default() -> Self {
        Self {
            mode: OutputMode::Human,
            color: ColorChoice::Auto,
            width: None,
        }
    }
}

impl OutputRequest {
    #[must_use]
    pub fn policy(&self) -> OutputPolicy {
        OutputPolicy::detect(self.mode, self.color, self.width)
    }
}

impl From<&ParsedOutput<'_>> for OutputRequest {
    fn from(parsed: &ParsedOutput<'_>) -> Self {
        Self {
            mode: parsed.mode,
            color: parsed.color,
            width: parsed.width,
        }
    }
}
