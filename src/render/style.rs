use crate::cli::OutputPolicy;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Strong,
    Dim,
    Good,
    Warning,
    Error,
    Accent,
}

pub struct Styler {
    enabled: bool,
}

impl Styler {
    pub fn new(policy: OutputPolicy) -> Self {
        Self {
            enabled: policy.color,
        }
    }
    pub fn paint(&self, role: Role, text: &str) -> String {
        if !self.enabled {
            return text.to_owned();
        }
        let code = match role {
            Role::Strong => "1",
            Role::Dim => "2",
            Role::Good => "32",
            Role::Warning => "33",
            Role::Error => "31",
            Role::Accent => "36",
        };
        format!("\u{1b}[{code}m{text}\u{1b}[0m")
    }
}

/// Converts untrusted terminal text to one safe display line.
pub fn sanitize(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars() {
        let forbidden = character.is_control()
            || matches!(character, '\u{007f}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
                | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
        if forbidden || character.is_whitespace() {
            pending_space |= !result.is_empty();
            continue;
        }
        if pending_space {
            result.push(' ');
            pending_space = false;
        }
        result.push(character);
    }
    result.trim().to_owned()
}

/// Fits a safe line to terminal cells without ending on a combining mark.
pub fn fit(value: &str, width: usize) -> String {
    let value = sanitize(value);
    if display_width(&value) <= width {
        return value;
    }
    if width == 0 {
        return String::new();
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    let target = width - 3;
    let mut result = String::with_capacity(value.len().min(width));
    let mut used = 0;
    for character in value.chars() {
        let cells = cell_width(character);
        if used + cells > target {
            break;
        }
        result.push(character);
        used += cells;
    }
    while result.chars().last().is_some_and(is_combining) {
        result.pop();
    }
    result.push_str("...");
    result
}

/// Returns visible terminal cells, ignoring ANSI SGR sequences.
pub fn visible_width(value: &str) -> usize {
    let mut plain = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' && characters.peek() == Some(&'[') {
            characters.next();
            for escaped in characters.by_ref() {
                if ('@'..='~').contains(&escaped) {
                    break;
                }
            }
        } else {
            plain.push(character);
        }
    }
    display_width(&plain)
}

fn display_width(value: &str) -> usize {
    value.chars().map(cell_width).sum()
}

fn cell_width(character: char) -> usize {
    if is_combining(character) {
        0
    } else if is_wide(character) {
        2
    } else {
        1
    }
}

fn is_combining(character: char) -> bool {
    matches!(character as u32,
        0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff |
        0x20d0..=0x20ff | 0xfe00..=0xfe0f | 0xfe20..=0xfe2f |
        0x1f3fb..=0x1f3ff | 0xe0100..=0xe01ef)
}

fn is_wide(character: char) -> bool {
    matches!(character as u32,
        0x1100..=0x115f | 0x2329..=0x232a | 0x2e80..=0xa4cf |
        0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xfe10..=0xfe19 |
        0xfe30..=0xfe6f | 0xff00..=0xff60 | 0xffe0..=0xffe6 |
        0x1f000..=0x1faff | 0x20000..=0x3fffd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_uses_cells_and_removes_terminal_controls() {
        let fitted = fit("alpha\n\u{1b}[31m界界界omega", 12);
        assert!(visible_width(&fitted) <= 12);
        assert!(!fitted.contains('\u{1b}'));
        assert!(fitted.ends_with("..."));
    }
}
