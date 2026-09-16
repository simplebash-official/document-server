// Runtime logging control over stdin. The desktop shell owns each sidecar's
// stdin and writes one JSON line to change logging while the process runs:
//
//   {"cmd":"log_mode","enabled":true,"http_bodies":true,"sql":"slow"}
//
// This is what lets Settings → System Benchmark measure logging off vs on
// without restarting the services. Only active in `LOG_FORMAT=json` (desktop)
// mode: under Docker/web nothing reads stdin. Unknown or malformed lines are
// ignored, so a future shell can add fields safely.

use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::{SqlLogging, settings};

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct ControlLine {
    pub cmd: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub http_bodies: bool,
    #[serde(default)]
    pub sql: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Parses one control line; `None` for anything that isn't a `log_mode` command.
pub fn parse_control(line: &str) -> Option<ControlLine> {
    let parsed: ControlLine = serde_json::from_str(line.trim()).ok()?;
    (parsed.cmd == "log_mode").then_some(parsed)
}

/// Applies a parsed line and returns what is now in force.
pub fn apply_control(line: &ControlLine) -> (bool, bool, SqlLogging) {
    let requested = line
        .sql
        .as_deref()
        .map_or(SqlLogging::Slow, SqlLogging::parse);
    // sqlx only emits statements if it was wired for them at connect time.
    let sql = match settings().sql_at_startup {
        SqlLogging::Off => SqlLogging::Off,
        _ => requested,
    };
    settings().apply(line.enabled, line.http_bodies, sql);
    (line.enabled, line.http_bodies, sql)
}

/// Reads control lines from stdin for the life of the process (desktop only).
pub fn spawn_stdin_control() {
    if !settings().json {
        return;
    }
    tokio::spawn(async move {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            match parse_control(&line) {
                Some(control) => {
                    let (enabled, bodies, sql) = apply_control(&control);
                    // Logged after applying, so an "off" switch is the last line.
                    tracing::info!(
                        category = "system",
                        event = "log_mode",
                        enabled,
                        http_bodies = bodies,
                        sql = ?sql,
                        "logging mode changed by the desktop shell"
                    );
                }
                None => tracing::debug!(
                    category = "system",
                    event = "control.ignored",
                    "ignored a control line"
                ),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_log_mode_lines_and_ignores_others() {
        let parsed =
            parse_control(r#"{"cmd":"log_mode","enabled":false,"http_bodies":false,"sql":"off"}"#)
                .expect("log_mode line");
        assert!(!parsed.enabled);
        assert_eq!(parsed.sql.as_deref(), Some("off"));

        // Unknown command, malformed JSON and plain text are all ignored.
        assert!(parse_control(r#"{"cmd":"something_else"}"#).is_none());
        assert!(parse_control("not json").is_none());
        assert!(parse_control("").is_none());

        // Missing fields fall back to "recording, no bodies, slow SQL".
        let minimal = parse_control(r#"{"cmd":"log_mode"}"#).expect("minimal line");
        assert!(minimal.enabled);
        assert!(!minimal.http_bodies);
        assert_eq!(minimal.sql, None);
    }

    #[test]
    fn sql_codes_round_trip() {
        for mode in [SqlLogging::All, SqlLogging::Slow, SqlLogging::Off] {
            assert_eq!(SqlLogging::from_code(mode.code()), mode);
        }
        assert_eq!(SqlLogging::parse("ALL"), SqlLogging::All);
        assert_eq!(SqlLogging::parse("nonsense"), SqlLogging::Slow);
    }
}
