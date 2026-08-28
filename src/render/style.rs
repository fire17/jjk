use crate::cli::OutputPolicy;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role { Strong, Dim, Good, Warning, Error, Accent }

pub struct Styler { enabled: bool }

impl Styler {
    pub fn new(policy: OutputPolicy) -> Self { Self { enabled: policy.color } }
    pub fn paint(&self, role: Role, text: &str) -> String {
        if !self.enabled { return text.to_owned(); }
        let code = match role { Role::Strong => "1", Role::Dim => "2", Role::Good => "32", Role::Warning => "33", Role::Error => "31", Role::Accent => "36" };
        format!("\u{1b}[{code}m{text}\u{1b}[0m")
    }
}

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
        if pending_space { result.push(' '); pending_space = false; }
        result.push(character);
    }
    result.trim().to_owned()
}

pub fn fit(value: &str, width: usize) -> String {
    let value = sanitize(value);
    let length = value.chars().count();
    if length <= width { return value; }
    if width == 0 { return String::new(); }
    if width <= 3 { return ".".repeat(width); }
    value.chars().take(width - 3).collect::<String>() + "..."
}

pub fn visible_width(value: &str) -> usize {
    let mut count = 0;
    let mut escape = false;
    for character in value.chars() {
        if escape {
            if character == 'm' { escape = false; }
        } else if character == '\u{1b}' {
            escape = true;
        } else {
            count += 1;
        }
    }
    count
}
