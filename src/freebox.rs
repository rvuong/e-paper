//! Freebox OS API: parsing and computations, without any network access.
//!
//! The HTTP calls live in `freebox_client`. See CLAUDE.md, "Freebox API notes".

use std::fmt;

use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use sha1::Sha1;

/// What can go wrong when talking to the Freebox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreeboxError {
    /// The Freebox answered `"success": false`.
    Api { code: String, message: String },
    /// The answer is not what the API documents.
    Format(String),
    /// The authorization request was refused on the Freebox screen.
    AuthorizationDenied,
    /// Nobody answered the authorization request on the Freebox screen in time.
    AuthorizationTimeout,
    /// The authorization request is unknown to the Freebox (expired or revoked).
    AuthorizationUnknown,
    /// Network or HTTP failure.
    Http(String),
}

impl fmt::Display for FreeboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api { code, message } => write!(f, "Freebox error {code}: {message}"),
            Self::Format(detail) => write!(f, "unexpected Freebox answer: {detail}"),
            Self::AuthorizationDenied => write!(f, "authorization refused on the Freebox"),
            Self::AuthorizationTimeout => {
                write!(f, "no answer on the Freebox screen in time, try again")
            }
            Self::AuthorizationUnknown => {
                write!(f, "authorization request unknown to the Freebox, try again")
            }
            Self::Http(detail) => write!(f, "cannot reach the Freebox: {detail}"),
        }
    }
}

impl std::error::Error for FreeboxError {}

/// Base path of the API from the `/api_version` answer, e.g. `/api/v16/`.
pub fn api_base(api_version_json: &str) -> Result<String, FreeboxError> {
    #[derive(Deserialize)]
    struct ApiVersion {
        api_base_url: String,
        api_version: String,
    }
    let info: ApiVersion =
        serde_json::from_str(api_version_json).map_err(|e| FreeboxError::Format(e.to_string()))?;
    let major = info
        .api_version
        .split('.')
        .next()
        .filter(|major| !major.is_empty() && major.chars().all(|c| c.is_ascii_digit()))
        .ok_or_else(|| FreeboxError::Format(format!("api_version {:?}", info.api_version)))?;
    Ok(format!("{}v{major}/", info.api_base_url))
}

/// Session password: HMAC-SHA1 of the challenge, keyed with the app token, in
/// lowercase hexadecimal.
pub fn session_password(app_token: &str, challenge: &str) -> String {
    let mut mac =
        Hmac::<Sha1>::new_from_slice(app_token.as_bytes()).expect("HMAC accepts any key size");
    mac.update(challenge.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The `result` of a Freebox answer, or its error.
pub fn parse_response<T: DeserializeOwned>(json: &str) -> Result<T, FreeboxError> {
    #[derive(Deserialize)]
    struct Envelope<T> {
        success: bool,
        result: Option<T>,
        error_code: Option<String>,
        msg: Option<String>,
    }
    let envelope: Envelope<T> =
        serde_json::from_str(json).map_err(|e| FreeboxError::Format(e.to_string()))?;
    if envelope.success {
        envelope
            .result
            .ok_or_else(|| FreeboxError::Format("success without result".into()))
    } else {
        Err(FreeboxError::Api {
            code: envelope.error_code.unwrap_or_default(),
            message: envelope.msg.unwrap_or_default(),
        })
    }
}

/// What to do after reading the status of an authorization request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationStep {
    /// The request waits for an answer on the Freebox screen: ask again later.
    Wait,
    /// The app token is now valid.
    Granted,
}

/// Next step from the `status` of `GET login/authorize/{track_id}`.
pub fn authorization_step(status: &str) -> Result<AuthorizationStep, FreeboxError> {
    match status {
        "pending" => Ok(AuthorizationStep::Wait),
        "granted" => Ok(AuthorizationStep::Granted),
        "denied" => Err(FreeboxError::AuthorizationDenied),
        "timeout" => Err(FreeboxError::AuthorizationTimeout),
        "unknown" => Err(FreeboxError::AuthorizationUnknown),
        other => Err(FreeboxError::Format(format!(
            "authorization status {other:?}"
        ))),
    }
}

/// A device known to the Freebox (LAN browser).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// Lowercase MAC address.
    pub mac: String,
    pub name: String,
    /// Empty when the Freebox does not know the vendor.
    pub vendor: String,
    pub active: bool,
    /// Unix timestamp in seconds.
    pub last_activity: Option<i64>,
}

/// Devices from a `GET lan/browser/{interface}/` answer, sorted by name.
/// Devices without a MAC address are skipped.
pub fn parse_hosts(json: &str) -> Result<Vec<Host>, FreeboxError> {
    #[derive(Deserialize)]
    struct RawHost {
        l2ident: Option<L2Ident>,
        primary_name: Option<String>,
        vendor_name: Option<String>,
        #[serde(default)]
        active: bool,
        last_activity: Option<i64>,
    }
    #[derive(Deserialize)]
    struct L2Ident {
        id: String,
        #[serde(rename = "type")]
        kind: String,
    }

    let raw_hosts: Vec<RawHost> = parse_response(json)?;
    let mut hosts: Vec<Host> = raw_hosts
        .into_iter()
        .filter_map(|raw| {
            let l2ident = raw.l2ident.filter(|l2| l2.kind == "mac_address")?;
            Some(Host {
                mac: l2ident.id.to_lowercase(),
                name: raw.primary_name.unwrap_or_default(),
                vendor: raw.vendor_name.unwrap_or_default(),
                active: raw.active,
                last_activity: raw.last_activity,
            })
        })
        .collect();
    hosts.sort_by_key(|host| host.name.to_lowercase());
    Ok(hosts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAN_BROWSER: &str = include_str!("../tests/fixtures/lan_browser_pub.json");

    #[test]
    fn api_base_uses_the_major_version() {
        let json =
            r#"{"api_base_url": "/api/", "api_version": "16.1", "device_name": "Freebox Server"}"#;
        assert_eq!(api_base(json), Ok("/api/v16/".to_string()));
    }

    #[test]
    fn api_base_rejects_a_missing_or_malformed_version() {
        assert!(matches!(
            api_base(r#"{"api_base_url": "/api/"}"#),
            Err(FreeboxError::Format(_))
        ));
        assert!(matches!(
            api_base(r#"{"api_base_url": "/api/", "api_version": "v16"}"#),
            Err(FreeboxError::Format(_))
        ));
    }

    #[test]
    fn session_password_is_the_hex_hmac_sha1() {
        // Well-known HMAC-SHA1 example (Wikipedia, "HMAC"), checked with openssl.
        assert_eq!(
            session_password("key", "The quick brown fox jumps over the lazy dog"),
            "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9"
        );
    }

    #[test]
    fn successful_response_gives_the_result() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct Login {
            challenge: String,
        }
        let json = r#"{"success": true, "result": {"logged_in": false, "challenge": "abc"}}"#;
        assert_eq!(
            parse_response::<Login>(json),
            Ok(Login {
                challenge: "abc".into()
            })
        );
    }

    #[test]
    fn failed_response_gives_the_error_code_and_message() {
        let json =
            r#"{"success": false, "error_code": "auth_required", "msg": "Invalid session token"}"#;
        assert_eq!(
            parse_response::<serde_json::Value>(json),
            Err(FreeboxError::Api {
                code: "auth_required".into(),
                message: "Invalid session token".into()
            })
        );
    }

    #[test]
    fn pending_authorization_waits() {
        assert_eq!(authorization_step("pending"), Ok(AuthorizationStep::Wait));
    }

    #[test]
    fn granted_authorization_is_done() {
        assert_eq!(
            authorization_step("granted"),
            Ok(AuthorizationStep::Granted)
        );
    }

    #[test]
    fn denied_timeout_or_unknown_authorization_is_an_error() {
        assert_eq!(
            authorization_step("denied"),
            Err(FreeboxError::AuthorizationDenied)
        );
        assert_eq!(
            authorization_step("timeout"),
            Err(FreeboxError::AuthorizationTimeout)
        );
        assert_eq!(
            authorization_step("unknown"),
            Err(FreeboxError::AuthorizationUnknown)
        );
    }

    #[test]
    fn hosts_give_mac_name_vendor_and_activity() {
        let hosts = parse_hosts(LAN_BROWSER).unwrap();
        let phone = hosts.iter().find(|host| host.name == "Pixel-R").unwrap();
        assert_eq!(
            phone,
            &Host {
                mac: "02:00:00:00:00:02".into(),
                name: "Pixel-R".into(),
                vendor: "Google, Inc.".into(),
                active: true,
                last_activity: Some(1_760_000_000),
            }
        );
    }

    #[test]
    fn missing_or_empty_vendor_is_empty() {
        let hosts = parse_hosts(LAN_BROWSER).unwrap();
        let vendor = |name: &str| {
            hosts
                .iter()
                .find(|host| host.name == name)
                .unwrap()
                .vendor
                .clone()
        };
        assert_eq!(vendor("PS5"), "");
        assert_eq!(vendor("Laptop-M"), "");
    }

    #[test]
    fn hosts_without_mac_address_are_skipped() {
        let hosts = parse_hosts(LAN_BROWSER).unwrap();
        assert_eq!(hosts.len(), 4);
        assert!(hosts.iter().all(|host| host.name != "Unknown"));
    }

    #[test]
    fn hosts_are_sorted_by_name() {
        let names: Vec<String> = parse_hosts(LAN_BROWSER)
            .unwrap()
            .into_iter()
            .map(|host| host.name)
            .collect();
        assert_eq!(names, ["Freebox Player POP", "Laptop-M", "Pixel-R", "PS5"]);
    }
}
