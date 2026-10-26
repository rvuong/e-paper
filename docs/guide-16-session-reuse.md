# Guide 16: reuse the Freebox session

Written 2026-10-26, 5 tests approved by Rémy (the 5th after the check on
the dev PC, step 4b). Implemented by Claude at
Rémy's request (2026-10-26), in TDD. The code is done; what remains is the
check on the dev PC (step 5) and the deployment (step 6).

## Why

Each `run` cycle opened a new Freebox session (`POST login/session/`) and
never closed it. With one cycle a minute and sessions expiring after about
30 minutes, the Freebox kept about 30 "e-paper dashboard" sessions open at
any time (Freebox OS > Gestion des accès > Sessions). An API client is
expected to keep its session token and log in again only when the Freebox
asks for it.

## Behaviour

1. At the start of a cycle, `run` reads the session token kept by the
   previous run, if any, and uses it.
2. If the Freebox answers `auth_required` (session expired, Freebox
   rebooted, session deleted by hand), `run` logs in once and asks again.
3. After a login, `run` writes the new token. A write failure is only a
   warning: the cycle goes on, the next one logs in again.
4. `devices` and `check-counters` (run by hand) keep one session per command.

The token is kept in `freebox_session`, next to `state.json` (like
`frame.png`), readable by its owner only (0600). It is a short-lived secret:
it opens the API until the session expires. It stays out of `state.json`,
which gets copied around for debugging. On the Pi:
`/var/lib/e-paper/freebox_session`; no change to the systemd unit.

## Code

| Module | Change | New tests |
|---|---|---|
| `freebox` | `needs_new_session`, `stored_session`; `parse_optional` reads `result` only on success | 5 |
| `freebox_client` | `Client::resume`, `Client::session_token` | — |
| `main.rs` | `in_session`, `save_session`; `run_cycle` uses them | — |
| `.gitignore` | `/freebox_session` | — |

The retry after `auth_required` lives in `main.rs`, the imperative shell,
without unit test, like the rest of the HTTP calls: it is checked by hand
against the real Freebox (step 5).

## 1. Tests first (red)

In `src/freebox.rs`, at the end of `mod tests`, add:

```rust
    #[test]
    fn auth_required_asks_for_a_new_session() {
        let error = FreeboxError::Api {
            code: "auth_required".into(),
            message: "Invalid session token".into(),
        };
        assert!(needs_new_session(&error));
    }

    #[test]
    fn other_api_errors_keep_the_session() {
        let error = FreeboxError::Api {
            code: "insufficient_rights".into(),
            message: "Insufficient rights".into(),
        };
        assert!(!needs_new_session(&error));
    }

    #[test]
    fn network_errors_keep_the_session() {
        let error = FreeboxError::Http("connection refused".into());
        assert!(!needs_new_session(&error));
    }

    #[test]
    fn blank_session_file_means_no_session() {
        assert_eq!(stored_session(""), None);
        assert_eq!(stored_session(" \n"), None);
        assert_eq!(stored_session("abc\n"), Some("abc".to_string()));
    }
```

What each test checks:

1. `auth_required` is the Freebox's answer to an expired or unknown session
   token: the normal case, a new session is needed.
2. Any other API error (here missing rights) would come back after a login:
   logging in again would only loop and open sessions.
3. A network failure (Freebox unreachable) says nothing about the session:
   logging in again would open a session per outage.
4. An empty file, or one holding only whitespace, is no token: never send an
   empty `X-Fbx-App-Auth` header. The trailing newline of a token is dropped.

```sh
cargo test --lib freebox
```

Expected: compile errors, `needs_new_session` and `stored_session` do not
exist yet.

## 2. Pure functions (green)

In `src/freebox.rs`, after `authorization_step`, add:

```rust
/// Whether an error asks for a new session: the session token expired, or
/// the Freebox forgot it (reboot, session deleted by hand).
pub fn needs_new_session(error: &FreeboxError) -> bool {
    matches!(error, FreeboxError::Api { code, .. } if code == "auth_required")
}

/// The session token kept in a file by an earlier run, `None` if the file
/// holds nothing.
pub fn stored_session(contents: &str) -> Option<String> {
    let token = contents.trim();
    (!token.is_empty()).then(|| token.to_string())
}
```

```sh
cargo test --lib freebox
```

Expected: all `freebox` tests pass.

## 3. The client

In `src/freebox_client.rs`, after `login`, add:

```rust
    /// Uses a session token kept from an earlier run.
    pub fn resume(&mut self, session_token: String) {
        self.session_token = Some(session_token);
    }

    /// The current session token, to keep for the next run.
    pub fn session_token(&self) -> Option<&str> {
        self.session_token.as_deref()
    }
```

## 4. The cycle

In `src/main.rs`, in `run_cycle`, replace:

```rust
    let mut client = Client::connect()?;
    client.login(&app_token)?;
    let hosts = client.lan_hosts()?;
```

with:

```rust
    let mut client = Client::connect()?;
    let session_path = state_path.with_file_name("freebox_session");
    let hosts = lan_hosts_in_session(&mut client, &app_token, &session_path)?;
```

`wifi_counters` right after uses the same session, already checked by
`lan_hosts`.

Then add these two functions after `poll_prim`:

```rust
/// The LAN hosts, in the session kept from the previous run. Logs in again
/// only when the Freebox asks for it, then keeps the new session.
fn lan_hosts_in_session(
    client: &mut Client,
    app_token: &str,
    session_path: &Path,
) -> Result<Vec<freebox::Host>, Box<dyn Error>> {
    if let Some(token) = fs::read_to_string(session_path)
        .ok()
        .and_then(|contents| freebox::stored_session(&contents))
    {
        client.resume(token);
        match client.lan_hosts() {
            Err(error) if freebox::needs_new_session(&error) => {}
            result => return Ok(result?),
        }
    }
    client.login(app_token)?;
    let hosts = client.lan_hosts()?;
    if let Some(token) = client.session_token()
        && let Err(error) = save_session(session_path, token)
    {
        eprintln!("warning: cannot write {}: {error}", session_path.display());
    }
    Ok(hosts)
}

/// Writes the session token, readable by its owner only; replaces the
/// previous one.
fn save_session(path: &Path, token: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(token.as_bytes())
}
```

`mode(0o600)` only applies when the file is created; it is created by the
first run, so it keeps that mode.

In `.gitignore`, under `/frame.png`, add:

```text
/freebox_session
```

Then:

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

## 4b. Error answers carrying a `result`

Found by the check on the dev PC: an error answer may carry a `result` too
(e.g. a new `challenge`). `parse_optional` deserialized `result` into the
expected type before looking at `success`, so an `auth_required` answer to
`lan/browser` gave `Format("invalid type: map, expected a sequence")`, never
the `Api` error that triggers the login, and a refused login gave
"missing field `session_token`" instead of `invalid_token`.

Test, at the end of `mod tests` in `src/freebox.rs`:

```rust
    #[test]
    fn error_answer_with_a_result_gives_the_api_error() {
        let json = r#"{"success": false, "error_code": "auth_required", "msg": "Invalid session token", "result": {"challenge": "x", "password_salt": "y"}}"#;
        assert_eq!(
            parse_hosts(json),
            Err(FreeboxError::Api {
                code: "auth_required".into(),
                message: "Invalid session token".into()
            })
        );
    }
```

Fix: `result` is read as a `serde_json::Value`, converted only on success:

```rust
/// The `result` of a Freebox answer, `None` if absent, or its error. An error
/// answer may carry a `result` too (a new challenge): it is read only on
/// success.
fn parse_optional<T: DeserializeOwned>(json: &str) -> Result<Option<T>, FreeboxError> {
    #[derive(Deserialize)]
    struct Envelope {
        success: bool,
        result: Option<serde_json::Value>,
        error_code: Option<String>,
        msg: Option<String>,
    }
    let envelope: Envelope =
        serde_json::from_str(json).map_err(|e| FreeboxError::Format(e.to_string()))?;
    if envelope.success {
        envelope
            .result
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| FreeboxError::Format(e.to_string()))
    } else {
        Err(FreeboxError::Api {
            code: envelope.error_code.unwrap_or_default(),
            message: envelope.msg.unwrap_or_default(),
        })
    }
}
```

## 4c. Expiry between two requests

Found on the Pi on 2026-10-26: the Freebox ended the 13:51 session at
14:23, though it was used every minute (nothing deleted by hand). That cycle
failed with `auth_required` in `wifi_counters`, the request after
`lan_hosts`, which was the only one covered by the new login. The next cycle
logged in normally: one missed refresh.

`lan_hosts_in_session` becomes `in_session`, which runs a whole reading of
the Freebox and runs it again after a new login. In `run_cycle`:

```rust
    let (hosts, wifi_counters) = in_session(&mut client, &app_token, &session_path, |client| {
        Ok((client.lan_hosts()?, client.wifi_counters()?))
    })?;
    // Devices on the repeater only have the LAN browser's counter.
    let counters = freebox::download_counters(wifi_counters, &hosts);
```

```rust
/// Reads the Freebox in the session kept from the previous run. Logs in
/// again only when the Freebox asks for it, keeps the new session, then reads
/// everything again: the session may expire between two requests.
fn in_session<T>(
    client: &mut Client,
    app_token: &str,
    session_path: &Path,
    read: impl Fn(&Client) -> Result<T, freebox::FreeboxError>,
) -> Result<T, Box<dyn Error>> {
    if let Some(token) = fs::read_to_string(session_path)
        .ok()
        .and_then(|contents| freebox::stored_session(&contents))
    {
        client.resume(token);
        match read(client) {
            Err(error) if freebox::needs_new_session(&error) => {}
            result => return Ok(result?),
        }
    }
    client.login(app_token)?;
    if let Some(token) = client.session_token()
        && let Err(error) = save_session(session_path, token)
    {
        eprintln!("warning: cannot write {}: {error}", session_path.display());
    }
    Ok(read(client)?)
}
```

The token is saved right after the login, before reading: a failed reading
no longer loses a valid session.

## 5. Check on the dev PC

Run two cycles to a PNG (`target/e-paper.png`), a minute apart, with
`PRIM_STOP_RUNGIS` and `PRIM_STOP_ANTONY` set as in guide 14:

```sh
cargo run -- run
```

- First run: no `freebox_session` yet, one login; the file appears at the
  project root with mode `-rw-------` (`ls -l freebox_session`).
- Second run: no new session in Freebox OS > Gestion des accès > Sessions
  (click "Recharger").
- Delete the "e-paper dashboard" session in Freebox OS (or write a fake
  token into `freebox_session`), run again: one new session (the
  `auth_required` path), and `freebox_session` holds the new token.

Done by Claude on 2026-10-26, after re-authorizing the dev PC (its app token
had been revoked): reuse, and recovery from a fake token, both work.

## 6. Deploy

`make deploy` restarts the timer (stopped on 2026-10-26 for the Free app
test).

```sh
make deploy
```

After 10 minutes, in Freebox OS > Sessions: a single "e-paper dashboard"
session opened after the deployment. The older ones expire within 30
minutes, or delete them by hand. `make logs` shows no warning about
`freebox_session`.
