//! Freebox OS API: parsing and computations, without any network access.
//!
//! The HTTP calls live in `freebox_client`. See CLAUDE.md, "Freebox API notes".

use std::collections::BTreeMap;
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
    parse_optional(json)?.ok_or_else(|| FreeboxError::Format("success without result".into()))
}

/// The `result` of a Freebox answer returning a list. The Freebox omits
/// `result` when the list is empty.
fn parse_list<T: DeserializeOwned>(json: &str) -> Result<Vec<T>, FreeboxError> {
    Ok(parse_optional(json)?.unwrap_or_default())
}

/// The `result` of a Freebox answer, `None` if absent, or its error.
fn parse_optional<T: DeserializeOwned>(json: &str) -> Result<Option<T>, FreeboxError> {
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
        Ok(envelope.result)
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
    /// How the device is connected, when the Freebox says so.
    pub access_point: Option<HostAccessPoint>,
}

/// The `access_point` object of a LAN host. Every field is optional: its
/// exact content on a real Freebox is still being checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAccessPoint {
    /// `type`: e.g. `gateway` or `repeater`.
    pub kind: Option<String>,
    /// `connectivity_type`: e.g. `wifi` or `ethernet`.
    pub connectivity: Option<String>,
    /// Bytes sent to the device, if the Freebox counts them here.
    pub tx_bytes: Option<u64>,
    /// Names of the fields present, sorted (diagnostics).
    pub fields: Vec<String>,
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
        access_point: Option<serde_json::Value>,
    }
    #[derive(Deserialize)]
    struct L2Ident {
        id: String,
        #[serde(rename = "type")]
        kind: String,
    }

    let raw_hosts: Vec<RawHost> = parse_list(json)?;
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
                access_point: raw.access_point.as_ref().and_then(host_access_point),
            })
        })
        .collect();
    hosts.sort_by_key(|host| host.name.to_lowercase());
    Ok(hosts)
}

/// The `access_point` object of a host, None if absent or not an object.
/// A field of an unexpected type is read as missing.
fn host_access_point(value: &serde_json::Value) -> Option<HostAccessPoint> {
    let object = value.as_object()?;
    let text = |key: &str| object.get(key)?.as_str().map(String::from);
    let mut fields: Vec<String> = object.keys().cloned().collect();
    fields.sort();
    Some(HostAccessPoint {
        kind: text("type"),
        connectivity: text("connectivity_type"),
        tx_bytes: object.get("tx_bytes").and_then(serde_json::Value::as_u64),
        fields,
    })
}

/// Identifiers of the Wi-Fi access points, from `GET wifi/ap/`.
pub fn parse_access_points(json: &str) -> Result<Vec<u64>, FreeboxError> {
    #[derive(Deserialize)]
    struct AccessPoint {
        id: u64,
    }
    let access_points: Vec<AccessPoint> = parse_list(json)?;
    Ok(access_points.into_iter().map(|ap| ap.id).collect())
}

/// A Wi-Fi client of an access point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Station {
    /// Lowercase MAC address.
    pub mac: String,
    /// Bytes sent by the Freebox to the station: its download.
    pub tx_bytes: u64,
}

/// Clients of one access point, from `GET wifi/ap/{id}/stations/`.
pub fn parse_stations(json: &str) -> Result<Vec<Station>, FreeboxError> {
    #[derive(Deserialize)]
    struct RawStation {
        mac: String,
        tx_bytes: u64,
    }
    let raw_stations: Vec<RawStation> = parse_list(json)?;
    Ok(raw_stations
        .into_iter()
        .map(|raw| Station {
            mac: raw.mac.to_lowercase(),
            tx_bytes: raw.tx_bytes,
        })
        .collect())
}

/// Download counter of every station, all access points together. A station
/// seen on two access points (band switch) keeps its highest counter.
pub fn merge_stations(stations: impl IntoIterator<Item = Station>) -> BTreeMap<String, u64> {
    let mut counters = BTreeMap::new();
    for station in stations {
        let counter = counters.entry(station.mac).or_insert(0);
        *counter = (*counter).max(station.tx_bytes);
    }
    counters
}

/// Download counter of every host whose `access_point` counts its bytes
/// (devices on a repeater are missing from `wifi/ap/{id}/stations/`).
pub fn host_counters(hosts: &[Host]) -> BTreeMap<String, u64> {
    hosts
        .iter()
        .filter_map(|host| Some((host.mac.clone(), host.access_point.as_ref()?.tx_bytes?)))
        .collect()
}

/// Wi-Fi station counters and host counters together, keeping the highest
/// counter of a device present in both.
pub fn download_counters(mut wifi: BTreeMap<String, u64>, hosts: &[Host]) -> BTreeMap<String, u64> {
    for (mac, counter) in host_counters(hosts) {
        let merged = wifi.entry(mac).or_insert(0);
        *merged = (*merged).max(counter);
    }
    wifi
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
                access_point: None,
            }
        );
    }

    fn host_with_access_point(access_point: &str) -> String {
        format!(
            r#"{{"success": true, "result": [{{
                "l2ident": {{"id": "02:00:00:00:00:09", "type": "mac_address"}},
                "primary_name": "PS5",
                "access_point": {access_point}
            }}]}}"#
        )
    }

    #[test]
    fn host_access_point_is_parsed() {
        let json = host_with_access_point(
            r#"{"type": "repeater", "connectivity_type": "wifi", "uid": "x",
                "tx_bytes": 123456, "rx_bytes": 42}"#,
        );
        let hosts = parse_hosts(&json).unwrap();
        assert_eq!(
            hosts[0].access_point,
            Some(HostAccessPoint {
                kind: Some("repeater".into()),
                connectivity: Some("wifi".into()),
                tx_bytes: Some(123_456),
                fields: ["connectivity_type", "rx_bytes", "tx_bytes", "type", "uid"]
                    .map(String::from)
                    .to_vec(),
            })
        );
    }

    #[test]
    fn missing_access_point_fields_are_tolerated() {
        let hosts = parse_hosts(&host_with_access_point("null")).unwrap();
        assert_eq!(hosts[0].access_point, None);
        let hosts = parse_hosts(&host_with_access_point(r#"{"tx_bytes": "oops"}"#)).unwrap();
        assert_eq!(
            hosts[0].access_point,
            Some(HostAccessPoint {
                kind: None,
                connectivity: None,
                tx_bytes: None,
                fields: vec!["tx_bytes".into()],
            })
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

    const WIFI_AP: &str = include_str!("../tests/fixtures/wifi_ap.json");
    const WIFI_STATIONS: &str = include_str!("../tests/fixtures/wifi_ap_stations.json");

    #[test]
    fn access_points_give_their_ids() {
        assert_eq!(parse_access_points(WIFI_AP), Ok(vec![0, 1, 2]));
    }

    #[test]
    fn stations_give_lowercase_mac_and_download_counter() {
        assert_eq!(
            parse_stations(WIFI_STATIONS),
            Ok(vec![
                Station {
                    mac: "02:00:00:00:00:02".into(),
                    tx_bytes: 1_200_000_000
                },
                Station {
                    mac: "02:00:00:00:00:ab".into(),
                    tx_bytes: 45_000_000_000
                },
            ])
        );
    }

    #[test]
    fn access_point_without_clients_has_no_station() {
        assert_eq!(parse_stations(r#"{"success": true}"#), Ok(vec![]));
    }

    #[test]
    fn stations_of_all_access_points_are_merged_keeping_the_highest_counter() {
        let station = |mac: &str, tx_bytes| Station {
            mac: mac.into(),
            tx_bytes,
        };
        let counters = merge_stations([
            station("02:00:00:00:00:02", 500),
            station("02:00:00:00:00:03", 900),
            station("02:00:00:00:00:02", 700),
        ]);
        assert_eq!(counters.len(), 2);
        assert_eq!(counters["02:00:00:00:00:02"], 700);
        assert_eq!(counters["02:00:00:00:00:03"], 900);
    }

    fn host_counting(mac: &str, tx_bytes: Option<u64>) -> Host {
        Host {
            mac: mac.into(),
            name: String::new(),
            vendor: String::new(),
            active: true,
            last_activity: None,
            access_point: Some(HostAccessPoint {
                kind: Some("repeater".into()),
                connectivity: Some("wifi".into()),
                tx_bytes,
                fields: Vec::new(),
            }),
        }
    }

    #[test]
    fn host_counters_come_from_access_point_tx_bytes() {
        let mut no_access_point = host_counting("02:00:00:00:00:03", None);
        no_access_point.access_point = None;
        let counters = host_counters(&[
            host_counting("02:00:00:00:00:01", Some(400)),
            host_counting("02:00:00:00:00:02", None),
            no_access_point,
        ]);
        assert_eq!(
            counters,
            [("02:00:00:00:00:01".to_string(), 400)]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn wifi_and_host_counters_are_merged_keeping_the_highest() {
        let wifi: BTreeMap<String, u64> = [
            ("02:00:00:00:00:01".to_string(), 500),
            ("02:00:00:00:00:02".to_string(), 900),
        ]
        .into_iter()
        .collect();
        let hosts = [
            host_counting("02:00:00:00:00:01", Some(520)),
            host_counting("02:00:00:00:00:02", Some(800)),
            host_counting("02:00:00:00:00:03", Some(300)),
        ];
        let counters = download_counters(wifi, &hosts);
        assert_eq!(
            counters,
            [
                ("02:00:00:00:00:01".to_string(), 520),
                ("02:00:00:00:00:02".to_string(), 900),
                ("02:00:00:00:00:03".to_string(), 300),
            ]
            .into_iter()
            .collect()
        );
    }
}
