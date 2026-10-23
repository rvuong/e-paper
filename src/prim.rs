//! Île-de-France Mobilités PRIM API: requests and log lines, without any
//! network access.
//!
//! The HTTP calls live in `prim_client`. See guides 09 and 12.

use serde_json::{Value, json};

/// SIRI Lite stop monitoring ("Prochains passages", unit request).
const STOP_MONITORING: &str = "https://prim.iledefrance-mobilites.fr/marketplace/stop-monitoring";

/// A stop to poll: a short label (also the log file name) and its SIRI
/// reference, e.g. `STIF:StopPoint:Q:12345:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stop {
    pub label: String,
    pub monitoring_ref: String,
}

/// Reads a stop given as `label=reference` on the command line.
pub fn parse_stop(arg: &str) -> Result<Stop, String> {
    let (label, monitoring_ref) = arg
        .split_once('=')
        .ok_or_else(|| format!("{arg:?}: expected label=reference"))?;
    if label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!(
            "{label:?}: the label takes letters, digits and '-' only"
        ));
    }
    if monitoring_ref.is_empty() {
        return Err(format!("{arg:?}: the reference is empty"));
    }
    Ok(Stop {
        label: label.into(),
        monitoring_ref: monitoring_ref.into(),
    })
}

/// URL of the next departures at one stop.
pub fn stop_monitoring_url(monitoring_ref: &str) -> String {
    format!("{STOP_MONITORING}?MonitoringRef={monitoring_ref}")
}

/// The API key from the content of its file.
pub fn api_key(file_content: &str) -> Result<String, String> {
    let key = file_content.trim();
    if key.is_empty() {
        Err("the API key file is empty".into())
    } else {
        Ok(key.into())
    }
}

/// One line of the observation log: poll time (Unix seconds), HTTP status
/// (0 when there was no answer) and the body, as JSON when it is JSON.
pub fn log_line(polled_at: u64, status: u16, body: &str) -> String {
    let body = serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.into()));
    json!({ "polled_at": polled_at, "status": status, "body": body }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A. Command line and requests

    #[test]
    fn a_stop_is_a_label_and_a_reference() {
        assert_eq!(
            parse_stop("antony=STIF:StopPoint:Q:12345:"),
            Ok(Stop {
                label: "antony".into(),
                monitoring_ref: "STIF:StopPoint:Q:12345:".into(),
            })
        );
    }

    #[test]
    fn a_stop_without_equals_sign_is_refused() {
        assert!(parse_stop("STIF:StopPoint:Q:12345:").is_err());
    }

    #[test]
    fn a_stop_without_reference_is_refused() {
        assert!(parse_stop("antony=").is_err());
    }

    #[test]
    fn a_label_that_is_not_a_plain_file_name_is_refused() {
        assert!(parse_stop("=STIF:StopPoint:Q:12345:").is_err());
        assert!(parse_stop("../antony=STIF:StopPoint:Q:12345:").is_err());
        assert!(parse_stop("my stop=STIF:StopPoint:Q:12345:").is_err());
    }

    #[test]
    fn the_url_carries_the_reference_as_is() {
        assert_eq!(
            stop_monitoring_url("STIF:StopPoint:Q:12345:"),
            "https://prim.iledefrance-mobilites.fr/marketplace/stop-monitoring\
             ?MonitoringRef=STIF:StopPoint:Q:12345:"
        );
    }

    #[test]
    fn the_api_key_is_trimmed() {
        assert_eq!(api_key("  abc123\n"), Ok("abc123".into()));
    }

    #[test]
    fn an_empty_api_key_is_refused() {
        assert!(api_key(" \n").is_err());
    }

    // B. Log lines

    #[test]
    fn a_json_body_is_logged_as_json() {
        let line = log_line(1_760_252_100, 200, r#"{"Siri": {}}"#);
        let logged: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            logged,
            json!({ "polled_at": 1_760_252_100, "status": 200, "body": { "Siri": {} } })
        );
    }

    #[test]
    fn a_body_that_is_not_json_is_logged_as_text() {
        let line = log_line(1_760_252_100, 502, "<html>Bad Gateway</html>");
        let logged: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(logged["status"], 502);
        assert_eq!(logged["body"], "<html>Bad Gateway</html>");
    }

    #[test]
    fn a_log_entry_is_one_line() {
        let line = log_line(1_760_252_100, 200, "{\n  \"Siri\": \"a\nb\"\n}\n");
        assert!(!line.contains('\n'));
        let line = log_line(1_760_252_100, 0, "timeout\nretry later");
        assert!(!line.contains('\n'));
    }
}
