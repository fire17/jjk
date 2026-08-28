2: //! Operating-system adapters.

pub(crate) mod clock;
pub mod filesystem;
pub(crate) mod ids;
pub(crate) mod lock;
pub mod process;
3: //! Native-string-safe CLI grammar, routing, input, output, and exit contracts.

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
    fn default() -> Self { Self { mode: OutputMode::Human, color: ColorChoice::Auto, width: None } }
}

impl OutputRequest {
    pub fn policy(&self) -> OutputPolicy { OutputPolicy::detect(self.mode, self.color, self.width) }
}
4: //! Effect interfaces implemented by infrastructure adapters.

pub(crate) mod clock;
pub mod filesystem;
pub mod git;
pub(crate) mod ids;
pub mod jj;
pub(crate) mod journal;
pub(crate) mod lock;
pub(crate) mod operation;
pub mod process;
pub(crate) mod projection;
pub mod repository;
5: //! Concrete infrastructure adapters.

pub mod git;
pub mod jj;
pub(crate) mod os;
pub(crate) mod sqlite;
