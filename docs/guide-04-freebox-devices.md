# Guide 04: Freebox authorization and device mapping

Two subcommands of the `e-paper` binary, run from the dev PC on the Freebox
network:

- `e-paper authorize`: one-time authorization request, confirmed on the
  Freebox screen. The app token is saved in `secrets/freebox_app_token`
  (ignored by git, readable by its owner only).
- `e-paper devices`: opens a session, lists every device known to the Freebox
  and writes a draft mapping file `devices.toml` (ignored by git, never
  overwritten), to complete by hand.

Approved decisions: `ureq` (blocking HTTP, `rustls`), HTTPS checked against the
Freebox root certificates, `clap` for the command line, token in
`secrets/freebox_app_token`, one `[[device]]` entry per device, no overwrite of
an existing mapping file. Approved tests: 21, in two modules.

| Module | Content | Tests |
|---|---|---|
| `freebox` | API version, session password, answers, authorization status, device list | A to E (1–12) |
| `devices` | draft mapping file, reading and validating the mapping | F, G (13–21) |
| `freebox_client` | the HTTP calls (thin, checked by hand) | none |

Checked on the real Freebox while writing this guide: Freebox Ultra (v9),
API 16.1, HTTPS certificate valid for `mafreebox.freebox.fr` and verified with
the root certificates below. The HMAC reference value of test 3 comes from
the well-known HMAC example (Wikipedia), checked with `openssl`.

Same TDD loop as guide 02: red (`todo!()`), green, refactor. Run every command
from the project root; create files in VS Code.

## 1. Dependencies

```bash
cargo add serde --features derive
cargo add serde_json toml hmac sha1 ureq
cargo add clap --features derive
cargo add --dev tempfile
```

- `serde`, `serde_json`, `toml`: read and write JSON and TOML into Rust structs.
- `hmac`, `sha1`: the session password.
- `ureq`: HTTP client, with `rustls` for HTTPS.
- `clap`: subcommands and options of the command line.
- `tempfile` (tests only): a temporary folder for the "no overwrite" test.

## 2. Freebox root certificates

The Freebox HTTPS certificate is signed by Freebox's own authority, unknown to
the usual certificate stores. These two root certificates are public (Freebox
OS SDK documentation, <https://dev.freebox.fr/sdk/os/>): they are versioned and
embedded in the binary.

Create the file `assets/freebox/root-ca.pem` with this content:

```text
-----BEGIN CERTIFICATE-----
MIICWTCCAd+gAwIBAgIJAMaRcLnIgyukMAoGCCqGSM49BAMCMGExCzAJBgNVBAYT
AkZSMQ8wDQYDVQQIDAZGcmFuY2UxDjAMBgNVBAcMBVBhcmlzMRMwEQYDVQQKDApG
cmVlYm94IFNBMRwwGgYDVQQDDBNGcmVlYm94IEVDQyBSb290IENBMB4XDTE1MDkw
MTE4MDIwN1oXDTM1MDgyNzE4MDIwN1owYTELMAkGA1UEBhMCRlIxDzANBgNVBAgM
BkZyYW5jZTEOMAwGA1UEBwwFUGFyaXMxEzARBgNVBAoMCkZyZWVib3ggU0ExHDAa
BgNVBAMME0ZyZWVib3ggRUNDIFJvb3QgQ0EwdjAQBgcqhkjOPQIBBgUrgQQAIgNi
AASCjD6ZKn5ko6cU5Vxh8GA1KqRi6p2GQzndxHtuUmwY8RvBbhZ0GIL7bQ4f08ae
JOv0ycWjEW0fyOnAw6AYdsN6y1eNvH2DVfoXQyGoCSvXQNAUxla+sJuLGICRYiZz
mnijYzBhMB0GA1UdDgQWBBTIB3c2GlbV6EIh2ErEMJvFxMz/QTAfBgNVHSMEGDAW
gBTIB3c2GlbV6EIh2ErEMJvFxMz/QTAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB
/wQEAwIBhjAKBggqhkjOPQQDAgNoADBlAjA8tzEMRVX8vrFuOGDhvZr7OSJjbBr8
gl2I70LeVNGEXZsAThUkqj5Rg9bV8xw3aSMCMQCDjB5CgsLH8EdZmiksdBRRKM2r
vxo6c0dSSNrr7dDN+m2/dRvgoIpGL2GauOGqDFY=
-----END CERTIFICATE-----
-----BEGIN CERTIFICATE-----
MIIFmjCCA4KgAwIBAgIJAKLyz15lYOrYMA0GCSqGSIb3DQEBCwUAMFoxCzAJBgNV
BAYTAkZSMQ8wDQYDVQQIDAZGcmFuY2UxDjAMBgNVBAcMBVBhcmlzMRAwDgYDVQQK
DAdGcmVlYm94MRgwFgYDVQQDDA9GcmVlYm94IFJvb3QgQ0EwHhcNMTUwNzMwMTUw
OTIwWhcNMzUwNzI1MTUwOTIwWjBaMQswCQYDVQQGEwJGUjEPMA0GA1UECAwGRnJh
bmNlMQ4wDAYDVQQHDAVQYXJpczEQMA4GA1UECgwHRnJlZWJveDEYMBYGA1UEAwwP
RnJlZWJveCBSb290IENBMIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEA
xqYIvq8538SH6BJ99jDlOPoyDBrlwKEp879oYplicTC2/p0X66R/ft0en1uSQadC
sL/JTyfgyJAgI1Dq2Y5EYVT/7G6GBtVH6Bxa713mM+I/v0JlTGFalgMqamMuIRDQ
tdyvqEIs8DcfGB/1l2A8UhKOFbHQsMcigxOe9ZodMhtVNn0mUyG+9Zgu1e/YMhsS
iG4Kqap6TGtk80yruS1mMWVSgLOq9F5BGD4rlNlWLo0C3R10mFCpqvsFU+g4kYoA
dTxaIpi1pgng3CGLE0FXgwstJz8RBaZObYEslEYKDzmer5zrU1pVHiwkjsgwbnuy
WtM1Xry3Jxc7N/i1rxFmN/4l/Tcb1F7x4yVZmrzbQVptKSmyTEvPvpzqzdxVWuYi
qIFSe/njl8dX9v5hjbMo4CeLuXIRE4nSq2A7GBm4j9Zb6/l2WIBpnCKtwUVlroKw
NBgB6zHg5WI9nWGuy3ozpP4zyxqXhaTgrQcDDIG/SQS1GOXKGdkCcSa+VkJ0jTf5
od7PxBn9/TuN0yYdgQK3YDjD9F9+CLp8QZK1bnPdVGywPfL1iztngF9J6JohTyL/
VMvpWfS/X6R4Y3p8/eSio4BNuPvm9r0xp6IMpW92V8SYL0N6TQQxzZYgkLV7TbQI
Hw6v64yMbbF0YS9VjS0sFpZcFERVQiodRu7nYNC1jy8CAwEAAaNjMGEwHQYDVR0O
BBYEFD2erMkECujilR0BuER09FdsYIebMB8GA1UdIwQYMBaAFD2erMkECujilR0B
uER09FdsYIebMA8GA1UdEwEB/wQFMAMBAf8wDgYDVR0PAQH/BAQDAgGGMA0GCSqG
SIb3DQEBCwUAA4ICAQAZ2Nx8mWIWckNY8X2t/ymmCbcKxGw8Hn3BfTDcUWQ7GLRf
MGzTqxGSLBQ5tENaclbtTpNrqPv2k6LY0VjfrKoTSS8JfXkm6+FUtyXpsGK8MrLL
hZ/YdADTfbbWOjjD0VaPUoglvo2N4n7rOuRxVYIij11fL/wl3OUZ7GHLgL3qXSz0
+RGW+1oZo8HQ7pb6RwLfv42Gf+2gyNBckM7VVh9R19UkLCsHFqhFBbUmqwJgNA2/
3twgV6Y26qlyHXXODUfV3arLCwFoNB+IIrde1E/JoOry9oKvF8DZTo/Qm6o2KsdZ
dxs/YcIUsCvKX8WCKtH6la/kFCUcXIb8f1u+Y4pjj3PBmKI/1+Rs9GqB0kt1otyx
Q6bqxqBSgsrkuhCfRxwjbfBgmXjIZ/a4muY5uMI0gbl9zbMFEJHDojhH6TUB5qd0
JJlI61gldaT5Ci1aLbvVcJtdeGhElf7pOE9JrXINpP3NOJJaUSueAvxyj/WWoo0v
4KO7njox8F6jCHALNDLdTsX0FTGmUZ/s/QfJry3VNwyjCyWDy1ra4KWoqt6U7SzM
d5jENIZChM8TnDXJzqc+mu00cI3icn9bV9flYCXLTIsprB21wVSMh0XeBGylKxeB
S27oDfFq04XSox7JM9HdTt2hLK96x1T7FpFrBTnALzb7vHv9MhXqAT90fPR/8A==
-----END CERTIFICATE-----
```

## 3. Test fixture: a device list

A Freebox answer to `GET lan/browser/pub/`, with made-up MAC addresses
(`02:00:…` addresses are locally administered: they belong to no real device).
It covers a device without vendor, one with an empty vendor, and one without a
MAC address.

Create the file `tests/fixtures/lan_browser_pub.json` with this content:

```json
{
  "success": true,
  "result": [
    {
      "id": "ether-02:00:00:00:00:02",
      "primary_name": "Pixel-R",
      "host_type": "smartphone",
      "vendor_name": "Google, Inc.",
      "l2ident": { "id": "02:00:00:00:00:02", "type": "mac_address" },
      "active": true,
      "reachable": true,
      "last_activity": 1760000000,
      "last_time_reachable": 1760000000
    },
    {
      "id": "ether-02:00:00:00:00:01",
      "primary_name": "Freebox Player POP",
      "host_type": "freebox_player",
      "vendor_name": "Freebox SAS",
      "l2ident": { "id": "02:00:00:00:00:01", "type": "mac_address" },
      "active": false,
      "reachable": false,
      "last_activity": 1759990000,
      "last_time_reachable": 1759990000
    },
    {
      "id": "ether-02:00:00:00:00:03",
      "primary_name": "PS5",
      "host_type": "workstation",
      "vendor_name": "",
      "l2ident": { "id": "02:00:00:00:00:03", "type": "mac_address" },
      "active": true,
      "reachable": true,
      "last_activity": 1759999000,
      "last_time_reachable": 1759999000
    },
    {
      "id": "ether-02:00:00:00:00:04",
      "primary_name": "Laptop-M",
      "host_type": "laptop",
      "l2ident": { "id": "02:00:00:00:00:04", "type": "mac_address" },
      "active": false,
      "reachable": false,
      "last_time_reachable": 1759000000
    },
    {
      "id": "other-unknown",
      "primary_name": "Unknown",
      "host_type": "other",
      "l2ident": { "id": "unknown", "type": "other" },
      "active": false,
      "reachable": false
    }
  ]
}
```

## 4. `freebox`: API parsing (tests 1–12)

Pure functions only: each one takes the text of a Freebox answer (or plain
values) and returns a result. No network: that is the job of section 6.

**Red.** Create the file `src/freebox.rs` with this content:

```rust
//! Freebox OS API: parsing and computations, without any network access.
//!
//! The HTTP calls live in `freebox_client`. API notes: guide 04.

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
    todo!()
}

/// Session password: HMAC-SHA1 of the challenge, keyed with the app token, in
/// lowercase hexadecimal.
pub fn session_password(app_token: &str, challenge: &str) -> String {
    todo!()
}

/// The `result` of a Freebox answer, or its error.
pub fn parse_response<T: DeserializeOwned>(json: &str) -> Result<T, FreeboxError> {
    todo!()
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
    todo!()
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
    todo!()
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
```

Add this line to `src/lib.rs`:

```rust
pub mod freebox;
```

```bash
cargo test freebox
```

Expected: `12 failed`, each with `not yet implemented` (and "unused import"
warnings: normal at this stage).

**Green.** In `src/freebox.rs`, replace the `todo!()` of each function.

`api_base`: the structure is declared inside the function, as only this
function needs it; `serde` fills it from the JSON.

```rust
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
```

`session_password`:

```rust
    let mut mac =
        Hmac::<Sha1>::new_from_slice(app_token.as_bytes()).expect("HMAC accepts any key size");
    mac.update(challenge.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
```

`parse_response`: the generic `T` is the type of the `result` field, chosen by
the caller.

```rust
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
```

`authorization_step`:

```rust
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
```

`parse_hosts`: `filter_map` drops the devices without a MAC address (the `?`
inside the closure returns `None` for them).

```rust
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
```

```bash
cargo test freebox
```

Expected: `12 passed`.

## 5. `devices`: the mapping file (tests 13–21)

**Red.** Create the file `src/devices.rs` with this content:

```rust
//! Device-to-person mapping (`devices.toml`). It holds MAC addresses: never
//! commit it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::freebox::Host;
use crate::layout::PEOPLE;
use crate::state::Person;

/// One `[[device]]` entry of the mapping file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceEntry {
    pub mac: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    /// R, M, H or L; empty for a shared device.
    #[serde(default)]
    pub person: String,
    #[serde(default)]
    pub smartphone: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeviceFile {
    #[serde(default, rename = "device")]
    devices: Vec<DeviceEntry>,
}

const DRAFT_HEADER: &str = "\
# Device-to-person mapping. It holds MAC addresses: never commit it.
#
# For each device, set `person` to R, M, H or L, or leave it empty for a shared
# device (ignored). Set `smartphone = true` on the phone of each person used
# for presence (one per person). `name` and `vendor` are for information only.

";

/// Draft mapping file listing every device, none assigned yet.
pub fn draft(hosts: &[Host]) -> String {
    todo!()
}

/// Writes a new file; fails with `AlreadyExists` instead of overwriting.
pub fn write_new(path: &Path, content: &str) -> io::Result<()> {
    todo!()
}

/// The person a device belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assignment {
    pub person: Person,
    /// The phone used for presence.
    pub smartphone: bool,
}

/// Assigned devices by lowercase MAC address. Shared devices are not in it.
pub type Mapping = BTreeMap<String, Assignment>;

/// Why a mapping file is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingError {
    Toml(String),
    UnknownPerson { mac: String, person: String },
    TwoSmartphones { person: Person },
    DuplicateMac { mac: String },
}

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(detail) => write!(f, "invalid mapping file: {detail}"),
            Self::UnknownPerson { mac, person } => {
                write!(
                    f,
                    "{mac}: unknown person {person:?} (expected R, M, H, L or empty)"
                )
            }
            Self::TwoSmartphones { person } => {
                write!(f, "{}: more than one smartphone", person.0)
            }
            Self::DuplicateMac { mac } => write!(f, "{mac}: listed more than once"),
        }
    }
}

impl std::error::Error for MappingError {}

/// Reads and validates a mapping file.
pub fn parse_mapping(text: &str) -> Result<Mapping, MappingError> {
    todo!()
}


#[cfg(test)]
mod tests {
    use super::*;

    fn host(mac: &str, name: &str, vendor: &str) -> Host {
        Host {
            mac: mac.into(),
            name: name.into(),
            vendor: vendor.into(),
            active: true,
            last_activity: None,
        }
    }

    fn device(mac: &str, person: &str, smartphone: bool) -> String {
        format!("[[device]]\nmac = \"{mac}\"\nperson = \"{person}\"\nsmartphone = {smartphone}\n\n")
    }

    #[test]
    fn draft_lists_every_device_unassigned() {
        let hosts = [
            host("02:00:00:00:00:01", "Pixel-R", "Google, Inc."),
            host("02:00:00:00:00:02", "PS5", ""),
        ];
        let file: DeviceFile = toml::from_str(&draft(&hosts)).unwrap();
        assert_eq!(
            file.devices,
            [
                DeviceEntry {
                    mac: "02:00:00:00:00:01".into(),
                    name: "Pixel-R".into(),
                    vendor: "Google, Inc.".into(),
                    person: String::new(),
                    smartphone: false,
                },
                DeviceEntry {
                    mac: "02:00:00:00:00:02".into(),
                    name: "PS5".into(),
                    vendor: String::new(),
                    person: String::new(),
                    smartphone: false,
                },
            ]
        );
    }

    #[test]
    fn draft_reads_back_as_a_valid_mapping() {
        let hosts = [host("02:00:00:00:00:01", "Pixel-R", "Google, Inc.")];
        assert_eq!(parse_mapping(&draft(&hosts)), Ok(Mapping::new()));
    }

    #[test]
    fn existing_file_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.toml");
        write_new(&path, "first").unwrap();

        let error = write_new(&path, "second").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
    }

    #[test]
    fn valid_file_gives_each_device_its_person() {
        let text =
            device("02:00:00:00:00:01", "R", true) + &device("02:00:00:00:00:02", "H", false);
        let mapping = parse_mapping(&text).unwrap();
        assert_eq!(
            mapping["02:00:00:00:00:01"],
            Assignment {
                person: Person('R'),
                smartphone: true
            }
        );
        assert_eq!(
            mapping["02:00:00:00:00:02"],
            Assignment {
                person: Person('H'),
                smartphone: false
            }
        );
    }

    #[test]
    fn empty_person_is_a_shared_device() {
        let mapping = parse_mapping(&device("02:00:00:00:00:01", "", false)).unwrap();
        assert!(!mapping.contains_key("02:00:00:00:00:01"));
    }

    #[test]
    fn unknown_person_is_an_error() {
        assert_eq!(
            parse_mapping(&device("02:00:00:00:00:01", "X", false)),
            Err(MappingError::UnknownPerson {
                mac: "02:00:00:00:00:01".into(),
                person: "X".into()
            })
        );
    }

    #[test]
    fn two_smartphones_for_one_person_is_an_error() {
        let text = device("02:00:00:00:00:01", "R", true) + &device("02:00:00:00:00:02", "R", true);
        assert_eq!(
            parse_mapping(&text),
            Err(MappingError::TwoSmartphones {
                person: Person('R')
            })
        );
    }

    #[test]
    fn same_mac_twice_is_an_error() {
        let text =
            device("02:00:00:00:00:01", "R", false) + &device("02:00:00:00:00:01", "H", false);
        assert_eq!(
            parse_mapping(&text),
            Err(MappingError::DuplicateMac {
                mac: "02:00:00:00:00:01".into()
            })
        );
    }

    #[test]
    fn mac_addresses_ignore_case() {
        let mapping = parse_mapping(&device("AA:BB:CC:DD:EE:01", "R", false)).unwrap();
        assert!(mapping.contains_key("aa:bb:cc:dd:ee:01"));

        let text =
            device("AA:BB:CC:DD:EE:01", "R", false) + &device("aa:bb:cc:dd:ee:01", "H", false);
        assert_eq!(
            parse_mapping(&text),
            Err(MappingError::DuplicateMac {
                mac: "aa:bb:cc:dd:ee:01".into()
            })
        );
    }
}
```

Add this line to `src/lib.rs`:

```rust
pub mod devices;
```

```bash
cargo test devices
```

Expected: `9 failed`.

**Green.** In `src/devices.rs`, replace the `todo!()` of `draft` with:

```rust
    let file = DeviceFile {
        devices: hosts
            .iter()
            .map(|host| DeviceEntry {
                mac: host.mac.clone(),
                name: host.name.clone(),
                vendor: host.vendor.clone(),
                person: String::new(),
                smartphone: false,
            })
            .collect(),
    };
    DRAFT_HEADER.to_string() + &toml::to_string(&file).expect("device list serializes to TOML")
```

The `todo!()` of `write_new` with (`create_new` fails if the file exists: it
never overwrites):

```rust
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(content.as_bytes())
```

The `todo!()` of `parse_mapping` with:

```rust
    let file: DeviceFile = toml::from_str(text).map_err(|e| MappingError::Toml(e.to_string()))?;
    let mut mapping = Mapping::new();
    let mut seen = BTreeSet::new();
    for entry in file.devices {
        let mac = entry.mac.to_lowercase();
        if !seen.insert(mac.clone()) {
            return Err(MappingError::DuplicateMac { mac });
        }
        if entry.person.is_empty() {
            continue;
        }
        let person = person_from(&entry.person).ok_or_else(|| MappingError::UnknownPerson {
            mac: mac.clone(),
            person: entry.person.clone(),
        })?;
        let has_smartphone = mapping
            .values()
            .any(|assignment: &Assignment| assignment.person == person && assignment.smartphone);
        if entry.smartphone && has_smartphone {
            return Err(MappingError::TwoSmartphones { person });
        }
        mapping.insert(
            mac,
            Assignment {
                person,
                smartphone: entry.smartphone,
            },
        );
    }
    Ok(mapping)
```

And add this function just after `parse_mapping`:

```rust
/// The household member with this initial, if any.
fn person_from(text: &str) -> Option<Person> {
    PEOPLE
        .into_iter()
        .find(|person| text == person.0.to_string())
}
```

```bash
cargo test devices
```

Expected: `9 passed`.

## 6. `freebox_client`: the HTTP calls

No unit test here, as approved: this module only sends requests and hands the
answers to the tested functions of `freebox`. It is checked by hand in
section 9.

Create the file `src/freebox_client.rs` with this content:

```rust
//! HTTP calls to the Freebox, over HTTPS checked against the Freebox root
//! certificates. Not unit-tested: the logic lives in `freebox`, this module
//! only moves bytes. Checked by hand against the real Freebox.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use ureq::Agent;
use ureq::tls::{PemItem, RootCerts, TlsConfig};

use crate::freebox::{
    AuthorizationStep, FreeboxError, Host, api_base, authorization_step, parse_hosts,
    parse_response, session_password,
};

const HOST: &str = "https://mafreebox.freebox.fr";
/// Freebox ECC Root CA and Freebox Root CA, from the Freebox OS SDK documentation.
const ROOT_CA: &[u8] = include_bytes!("../assets/freebox/root-ca.pem");

const APP_ID: &str = "io.github.rvuong.e-paper";
const APP_NAME: &str = "e-paper dashboard";

/// A connection to the Freebox API, logged in or not.
pub struct Client {
    agent: Agent,
    /// Full API URL, e.g. `https://mafreebox.freebox.fr/api/v16/`.
    api: String,
    session_token: Option<String>,
}

impl Client {
    /// Finds the API version of the Freebox.
    pub fn connect() -> Result<Self, FreeboxError> {
        let certs = ureq::tls::parse_pem(ROOT_CA)
            .filter_map(|item| match item {
                Ok(PemItem::Certificate(cert)) => Some(cert),
                _ => None,
            })
            .collect();
        let tls = TlsConfig::builder()
            .root_certs(RootCerts::Specific(Arc::new(certs)))
            .build();
        let agent: Agent = Agent::config_builder()
            .tls_config(tls)
            // Freebox errors come with a JSON body: read it instead of failing.
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        let mut client = Self {
            agent,
            api: String::new(),
            session_token: None,
        };
        let version = client.get_text(&format!("{HOST}/api_version"))?;
        client.api = format!("{HOST}{}", api_base(&version)?);
        Ok(client)
    }

    /// Asks for an app token, then waits for the answer on the Freebox screen.
    pub fn authorize(&self, device_name: &str) -> Result<String, FreeboxError> {
        #[derive(Deserialize)]
        struct Request {
            app_token: String,
            track_id: u64,
        }
        #[derive(Deserialize)]
        struct Status {
            status: String,
        }

        let request: Request = self.post(
            "login/authorize/",
            json!({
                "app_id": APP_ID,
                "app_name": APP_NAME,
                "app_version": env!("CARGO_PKG_VERSION"),
                "device_name": device_name,
            }),
        )?;
        println!("Accept the request on the front screen of the Freebox Server...");
        loop {
            let status: Status = self.get(&format!("login/authorize/{}", request.track_id))?;
            match authorization_step(&status.status)? {
                AuthorizationStep::Wait => thread::sleep(Duration::from_secs(1)),
                AuthorizationStep::Granted => return Ok(request.app_token),
            }
        }
    }

    /// Opens a session with the app token.
    pub fn login(&mut self, app_token: &str) -> Result<(), FreeboxError> {
        #[derive(Deserialize)]
        struct Login {
            challenge: String,
        }
        #[derive(Deserialize)]
        struct Session {
            session_token: String,
        }

        let login: Login = self.get("login/")?;
        let session: Session = self.post(
            "login/session/",
            json!({
                "app_id": APP_ID,
                "password": session_password(app_token, &login.challenge),
            }),
        )?;
        self.session_token = Some(session.session_token);
        Ok(())
    }

    /// Devices known to the Freebox on the main LAN interface.
    pub fn lan_hosts(&self) -> Result<Vec<Host>, FreeboxError> {
        parse_hosts(&self.get_text(&format!("{}lan/browser/pub/", self.api))?)
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, FreeboxError> {
        parse_response(&self.get_text(&format!("{}{path}", self.api))?)
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: Value) -> Result<T, FreeboxError> {
        let mut request = self.agent.post(format!("{}{path}", self.api));
        if let Some(token) = &self.session_token {
            request = request.header("X-Fbx-App-Auth", token);
        }
        let mut response = request
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(http_error)?;
        parse_response(&response.body_mut().read_to_string().map_err(http_error)?)
    }

    fn get_text(&self, url: &str) -> Result<String, FreeboxError> {
        let mut request = self.agent.get(url);
        if let Some(token) = &self.session_token {
            request = request.header("X-Fbx-App-Auth", token);
        }
        let mut response = request.call().map_err(http_error)?;
        response.body_mut().read_to_string().map_err(http_error)
    }
}

fn http_error(error: ureq::Error) -> FreeboxError {
    FreeboxError::Http(error.to_string())
}
```

Add this line to `src/lib.rs`:

```rust
pub mod freebox_client;
```

## 7. The command line

Replace the whole content of `src/main.rs` (the "Hello, world!") with:

```rust
//! `e-paper`: household e-paper dashboard.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use e_paper::devices;
use e_paper::freebox_client::Client;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// One-time authorization on the Freebox: confirm on its screen.
    Authorize {
        /// Where to store the app token (a secret).
        #[arg(long, default_value = "secrets/freebox_app_token")]
        token: PathBuf,
    },
    /// Writes the devices known to the Freebox into a draft mapping file.
    Devices {
        /// The app token written by `authorize`.
        #[arg(long, default_value = "secrets/freebox_app_token")]
        token: PathBuf,
        /// The draft mapping file to create (never overwritten).
        #[arg(long, default_value = "devices.toml")]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Command::Authorize { token } => authorize(&token),
        Command::Devices { token, output } => list_devices(&token, &output),
    }
}

fn authorize(token_path: &Path) -> Result<(), Box<dyn Error>> {
    if token_path.exists() {
        return Err(format!(
            "{} already exists: delete it to authorize again",
            token_path.display()
        )
        .into());
    }
    let client = Client::connect()?;
    let app_token = client.authorize(&device_name())?;
    save_secret(token_path, &app_token)?;
    println!("Authorized. App token saved in {}", token_path.display());
    Ok(())
}

fn list_devices(token_path: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let app_token = fs::read_to_string(token_path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run `e-paper authorize` first)",
            token_path.display()
        )
    })?;
    let mut client = Client::connect()?;
    client.login(app_token.trim())?;
    let hosts = client.lan_hosts()?;
    devices::write_new(output, &devices::draft(&hosts)).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            format!("{} already exists: not overwritten", output.display())
        } else {
            format!("cannot write {}: {error}", output.display())
        }
    })?;
    println!("{} devices written to {}", hosts.len(), output.display());
    Ok(())
}

/// Name shown on the Freebox screen during the authorization.
fn device_name() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_string())
        .unwrap_or_else(|_| "e-paper".into())
}

/// Writes a secret readable by its owner only; never overwrites.
fn save_secret(path: &Path, secret: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(secret.as_bytes())
}
```

## 8. Checks

```bash
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
cargo run -- --help
```

Expected: `56 passed` for the library plus the golden image test, no clippy
warning, and the help lists `authorize` and `devices`. `cargo fmt` sorts the
`pub mod` lines of `src/lib.rs`.

## 9. Use it on the real Freebox

Have the Freebox screen at hand (front panel of the Freebox Server).

```bash
cargo run -- authorize
```

When the command asks for it, accept the request on the Freebox screen. On
success, the token is in `secrets/freebox_app_token`.

```bash
cargo run -- devices
```

It writes `devices.toml`. Open it and, for each device, fill `person` (R, M, H
or L, or leave it empty for a shared device) and set `smartphone = true` on the
phone of each person. Nothing reads it yet: the next guide (one full cycle)
loads it with `parse_mapping`, which reports any mistake.

If `devices` fails with an `Api` error mentioning permissions, note the exact
message: the access rights of each app are set in Freebox OS, in the access
management settings (applications tab).

## 10. Commit

`secrets/` and `devices.toml` must **not** appear in `git status`:

```bash
git add -A
git status
git commit -m "feat: Freebox authorization and device mapping draft"
```

Expected new files: `assets/freebox/root-ca.pem`, `docs/guide-04-…`,
`src/devices.rs`, `src/freebox.rs`, `src/freebox_client.rs`,
`tests/fixtures/lan_browser_pub.json`, plus `Cargo.toml`, `Cargo.lock`,
`src/lib.rs`, `src/main.rs`.

## Report

Share the output of section 8, what `authorize` and `devices` print, and the
number of devices found (not the file itself: it holds MAC addresses).
