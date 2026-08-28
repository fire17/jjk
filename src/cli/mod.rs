//! Native-string-safe CLI grammar, routing, input, output, and exit contracts.

pub mod definition;
pub mod exit;
pub mod input;
pub mod output;
pub mod route;

pub use output::{ColorChoice, OutputMode, OutputPolicy};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputRequest {
    pub mode: OutputMode,
    pub color: ColorChoice,
    pub width: Option<usize>,
}

impl Default for OutputRequest {
    fn default() -> Self {
        Self { mode: OutputMode::Human, color: ColorChoice::Auto, width: None }
    }
}

impl OutputRequest {
    pub fn policy(&self) -> OutputPolicy {
        OutputPolicy::detect(self.mode, self.color, self.width)
    }
}
