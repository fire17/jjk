use std::env;
use std::io::IsTerminal;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode { Human, Json, Quiet }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorChoice { Auto, Always, Never }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputPolicy {
    pub mode: OutputMode,
    pub color: bool,
    pub terminal_width: usize,
    pub is_terminal: bool,
}

impl OutputPolicy {
    pub fn detect(mode: OutputMode, color: ColorChoice, requested_width: Option<usize>) -> Self {
        let is_terminal = std::io::stdout().is_terminal();
        let no_color = env::var_os("NO_COLOR").is_some();
        let color = mode == OutputMode::Human && !no_color && match color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => is_terminal,
        };
        let terminal_width = requested_width.or_else(|| env::var("COLUMNS").ok().and_then(|value| value.parse().ok()))
            .unwrap_or(80).clamp(20, 1_000);
        Self { mode, color, terminal_width, is_terminal }
    }

    pub const fn deterministic(mode: OutputMode, color: bool, terminal_width: usize, is_terminal: bool) -> Self {
        Self { mode, color, terminal_width, is_terminal }
    }
}
