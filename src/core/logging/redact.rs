// Secret masking for anything document-server logs (request/response bodies,
// headers). Mirrors the backend's and desktop shell's rules so every source redacts the
// same keys; the shell re-applies them as a backstop. Only credentials and
// secrets are masked — customer data is kept on purpose.

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

pub const REDACTED: &str = "[REDACTED]";

fn sensitive_key_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(password|passwd|pwd|secret|token|authorization|cookie|api[_-]?key|jwt|cvv|card_?number|otp|\bpin\b|^pin$|_pin$|pin_code|pincode)",
        )
        .expect("valid regex")
    })
}

fn secret_value_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(bearer\s+[A-Za-z0-9\-_\.=]+|eyJ[A-Za-z0-9\-_=]+\.[A-Za-z0-9\-_=]+\.[A-Za-z0-9\-_.+/=]*|\b[a-f0-9]{64}\b)",
        )
        .expect("valid regex")
    })
}

pub fn is_sensitive_key(key: &str) -> bool {
    sensitive_key_re().is_match(key)
}

pub fn redact_text(text: &str) -> String {
    secret_value_re().replace_all(text, REDACTED).into_owned()
}

/// Recursively mask sensitive keys and secret-shaped strings in place.
pub fn redact_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                if is_sensitive_key(key) && !v.is_null() {
                    *v = Value::String(REDACTED.into());
                } else {
                    redact_value(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_value),
        Value::String(s) if secret_value_re().is_match(s) => *s = redact_text(s),
        _ => {}
    }
}

/// Render a captured body for the log: JSON is redacted and re-serialized,
/// other text is secret-scrubbed; both are capped at `cap` bytes.
pub fn body_for_log(bytes: &[u8], cap: usize) -> String {
    let rendered = match serde_json::from_slice::<Value>(bytes) {
        Ok(mut json) => {
            redact_value(&mut json);
            json.to_string()
        }
        Err(_) => redact_text(&String::from_utf8_lossy(bytes)),
    };
    truncate(rendered, cap)
}

fn truncate(mut s: String, cap: usize) -> String {
    if s.len() <= cap {
        return s;
    }
    let mut cut = cap;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
    s.push_str("…[truncated]");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn masks_credentials_keeps_customer_data() {
        let mut v = json!({
            "email": "owner@shop.lk",
            "password": "hunter2",
            "adminPassword": "x",
            "customer": {"phone": "0771234567", "address": "Kandy"},
            "data": {"token": "abc", "items": [{"card_number": "4111"}]}
        });
        redact_value(&mut v);
        assert_eq!(v["email"], "owner@shop.lk");
        assert_eq!(v["password"], REDACTED);
        assert_eq!(v["adminPassword"], REDACTED);
        assert_eq!(v["customer"]["phone"], "0771234567");
        assert_eq!(v["data"]["token"], REDACTED);
        assert_eq!(v["data"]["items"][0]["card_number"], REDACTED);
    }

    #[test]
    fn body_for_log_redacts_and_caps() {
        let body = br#"{"email":"a@b.c","password":"p"}"#;
        let out = body_for_log(body, 1024);
        assert!(out.contains("a@b.c") && out.contains(REDACTED) && !out.contains("\"p\""));

        let long = format!("{{\"note\":\"{}\"}}", "x".repeat(100));
        assert!(body_for_log(long.as_bytes(), 20).ends_with("…[truncated]"));

        let jwt = "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1In0.sig";
        assert!(!body_for_log(jwt.as_bytes(), 1024).contains("eyJhbGci"));
    }
}
