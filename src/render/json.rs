use serde::Serialize;

#[derive(Serialize)]
struct Envelope<'a, T: Serialize> {
    schema: &'static str,
    version: u16,
    ok: bool,
    data: &'a T,
}

pub fn render<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(&Envelope { schema: "jjk.cli", version: 1, ok: true, data: value })
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    schema: &'static str,
    version: u16,
    ok: bool,
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
struct ErrorBody<'a> { code: &'a str, message: &'a str }

pub fn render_error(code: &str, message: &str) -> Result<String, serde_json::Error> {
    serde_json::to_string(&ErrorEnvelope { schema: "jjk.cli", version: 1, ok: false, error: ErrorBody { code, message } })
}
