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

/// `"key": value` pairs in raw JSON text, for redacting a body too large to
/// parse. The key is checked with `is_sensitive_key`, same as the full rule.
fn json_pair_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#""((?:[^"\\]|\\.)*)"(\s*:\s*)("(?:[^"\\]|\\.)*"|-?\d[\d.eE+-]*|true|false)"#)
            .expect("valid regex")
    })
}

/// Secret-scrub raw JSON text without parsing it: masks the value of every
/// sensitive key and every secret-shaped string.
pub fn redact_json_text(text: &str) -> String {
    let masked = json_pair_re().replace_all(text, |caps: &regex::Captures| {
        if is_sensitive_key(&caps[1]) {
            format!("\"{}\"{}\"{REDACTED}\"", &caps[1], &caps[2])
        } else {
            caps[0].to_string()
        }
    });
    redact_text(&masked)
}

/// Bodies larger than this multiple of the cap are never parsed: only their
/// first `cap` bytes are scrubbed as text, so logging a multi-MB body costs
/// O(cap) instead of a full parse + re-serialize.
const PARSE_LIMIT_FACTOR: usize = 4;

/// Render a captured body for the log: JSON is redacted and re-serialized,
/// other text is secret-scrubbed; both are capped at `cap` bytes.
pub fn body_for_log(bytes: &[u8], cap: usize) -> String {
    if bytes.len() > cap.saturating_mul(PARSE_LIMIT_FACTOR) {
        let mut end = cap.min(bytes.len());
        // Don't split a UTF-8 sequence.
        while end > 0 && end < bytes.len() && (bytes[end] & 0b1100_0000) == 0b1000_0000 {
            end -= 1;
        }
        let prefix = String::from_utf8_lossy(&bytes[..end]);
        return format!(
            "{}…[truncated, {} bytes total]",
            redact_json_text(&prefix),
            bytes.len()
        );
    }
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
    fn huge_bodies_are_scrubbed_as_text_without_parsing() {
        let mut body = String::from(r#"{"email":"a@b.c","adminPassword":"hunter2","items":["#);
        for i in 0..20_000 {
            body.push_str(&format!(
                r#"{{"key":"prod_{i}","api_key":"k{i}","name":"Item"}},"#
            ));
        }
        body.push_str("{}]}");
        let out = body_for_log(body.as_bytes(), 1024);
        // cap + growth from "[REDACTED]" replacements + the size suffix
        assert!(out.len() < 1024 + 512, "capped: {}", out.len());
        assert!(out.contains("a@b.c"));
        assert!(!out.contains("hunter2"));
        assert!(!out.contains("\"k0\""));
        assert!(out.contains("bytes total]"));
    }

    #[test]
    fn json_text_redaction_checks_whole_keys() {
        let out = redact_json_text(r#"{"shipping":"fast","pin":1234,"token":"abc","n":5}"#);
        assert!(out.contains(r#""shipping":"fast""#));
        assert!(out.contains(r#""pin":"[REDACTED]""#));
        assert!(out.contains(r#""token":"[REDACTED]""#));
        assert!(out.contains(r#""n":5"#));
    }

    #[test]
    fn body_for_log_redacts_and_caps() {
        let body = br#"{"email":"a@b.c","password":"p"}"#;
        let out = body_for_log(body, 1024);
        assert!(out.contains("a@b.c") && out.contains(REDACTED) && !out.contains("\"p\""));

        // Within 4x the cap: parsed, redacted, then cut.
        let medium = format!("{{\"note\":\"{}\"}}", "x".repeat(60));
        assert!(body_for_log(medium.as_bytes(), 20).ends_with("…[truncated]"));
        // Beyond 4x the cap: scrubbed as text, never parsed.
        let long = format!("{{\"note\":\"{}\"}}", "x".repeat(100));
        assert!(body_for_log(long.as_bytes(), 20).ends_with("bytes total]"));

        let jwt = "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1In0.sig";
        assert!(!body_for_log(jwt.as_bytes(), 1024).contains("eyJhbGci"));
    }
}
