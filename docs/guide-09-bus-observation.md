# Guide 09: observing the PRIM next departures

Before designing phase 2 on assumptions, look at what the PRIM API really
returns at our stops for a few days. A new subcommand logs the raw answers:

- `e-paper bus-log --stop antony=<ref> --stop rungis=<ref>`: one call to the
  SIRI Lite `stop-monitoring` API per stop, each answer appended as one JSON
  line to `bus-log/<label>.jsonl`, whatever its status (errors and quota
  refusals included).

Nothing is parsed yet: the logs will become the JSON fixtures of the next
guide, so its tests rely on real answers, not on the documentation.

Run it every 5 minutes on the Pi (always on) with a temporary systemd timer:
2 stops × 288 polls a day = 576 calls, under the 1,000 a day quota of recent
PRIM keys.

What the logs should tell us:

1. Do lines 297, 319 and 401 have real-time data at our stops?
2. How many departures each answer holds (is the second one there?).
3. Expected time (`ExpectedDepartureTime`) or only the scheduled one
   (`AimedDepartureTime`), and how far apart they are.
4. How the estimate of a given bus moves as it gets closer.
5. Errors and gaps over a few days (reliability).

Approved tests: 10, in one module.

| Module | Content | Tests |
|---|---|---|
| `prim` | stop argument, request URL, API key, log line | A, B (1–10) |
| `prim_client` | the HTTP call (thin, checked by hand) | none |

Checked while writing this guide: the code compiles, the 10 tests pass on a
copy of the project, and the endpoint answers `401 Unauthorized` with a fake
key (so the URL and the `apikey` header are the right ones).

Same TDD loop as before: red (`todo!()`), green, refactor. Run every command
from the project root; create files in VS Code.

## 1. PRIM account and API key

1. Create an account on <https://prim.iledefrance-mobilites.fr> ("Connexion").
2. In "Mes jetons d'authentification", generate an API key.
3. In "My API Usage" (or the API page "Prochains passages (plateforme
   Île-de-France Mobilités) – requête unitaire"), note the daily quota
   granted to the key.

The key is a secret. Create the file `secrets/prim_api_key` with the key as
its only content, then make it readable by you only:

```bash
chmod 600 secrets/prim_api_key
```

## 2. Find the stop references

The API takes a SIRI reference per stop point (one pole, usually one per
direction): `STIF:StopPoint:Q:<number>:`.

1. Open the dataset "Arrêts et lignes associées" (`arrets-lignes`) on PRIM or
   on <https://data.iledefrance-mobilites.fr>.
2. Filter on the line short name (`297`, then `319`, then `401`) and on the
   stop name "Montjean". Note each stop id, of the form `IDFM:<number>`; there
   are usually two per line (one per direction).
3. Try each number (replace `NNNNN`):

```bash
curl -s --compressed -H "apikey: $(cat secrets/prim_api_key)" "https://prim.iledefrance-mobilites.fr/marketplace/stop-monitoring?MonitoringRef=STIF:StopPoint:Q:NNNNN:" | python3 -m json.tool | less
```

In the answer, each `MonitoredStopVisit` is one departure: look at
`DestinationName` and `LineRef`. Keep:

- the pole whose departures go to Antony RER (297 and 401);
- the pole whose departures go to Marché International de Rungis (319).

If `MonitoredStopVisit` is empty at every hour of service, the line may have no
real-time data at this stop: note it, it is one of the answers we are after.
If the stop ids of the dataset have another form than `IDFM:<number>`, stop
there and send me one.

## 3. What the tests check (1–10)

Nothing to type in this section: it explains the tests that section 4 writes.
"Command line" means the arguments of `e-paper bus-log`, "requests" the URL
and the API key sent to PRIM.

**A. Command line and requests**

1. `a_stop_is_a_label_and_a_reference`: `antony=STIF:StopPoint:Q:12345:` gives
   the label `antony` and the reference after the first `=`.
2. `a_stop_without_equals_sign_is_refused`: a bare reference is a typing
   mistake, not a stop.
3. `a_stop_without_reference_is_refused`: `antony=` would poll nothing.
4. `a_label_that_is_not_a_plain_file_name_is_refused`: the label becomes a file
   name, so it takes letters, digits and `-` only. An empty label, `../antony`
   or `my stop` are refused (no file written outside `bus-log/`).
5. `the_url_carries_the_reference_as_is`: the exact URL of the unit request;
   the colons of the reference are valid in a query string and stay as they
   are.
6. `the_api_key_is_trimmed`: an editor often adds a final newline to the key
   file; it must not end up in the HTTP header.
7. `an_empty_api_key_is_refused`: a clear error instead of a series of
   `401` in the logs.

**B. Log lines**

8. `a_json_body_is_logged_as_json`: a JSON answer is stored as JSON, next to
   the poll time and the HTTP status, so the logs read as JSON fixtures later.
9. `a_body_that_is_not_json_is_logged_as_text`: an error page (HTML from a
   gateway, for instance) is kept as a string instead of being lost.
10. `a_log_entry_is_one_line`: one poll = one line, even when the body (or a
    network error message) spans several lines; the file stays readable line
    by line.

## 4. `prim`: requests and log lines (tests 1–10)

**Red.** Create the file `src/prim.rs` with this content:

```rust
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
    todo!()
}

/// URL of the next departures at one stop.
pub fn stop_monitoring_url(monitoring_ref: &str) -> String {
    todo!()
}

/// The API key from the content of its file.
pub fn api_key(file_content: &str) -> Result<String, String> {
    todo!()
}

/// One line of the observation log: poll time (Unix seconds), HTTP status
/// (0 when there was no answer) and the body, as JSON when it is JSON.
pub fn log_line(polled_at: u64, status: u16, body: &str) -> String {
    todo!()
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
```

In `src/lib.rs`, add after `pub mod presence;`:

```rust
pub mod prim;
pub mod prim_client;
```

`prim_client` comes in section 5: until then, comment out its line.

```bash
cargo test --lib prim
```

Expected: 10 tests, all failing on `todo!()`.

**Green.** Replace the four `todo!()` functions with:

```rust
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
```

```bash
cargo test --lib prim
```

Expected: `10 passed`.

Notes:

- `split_once('=')` cuts at the first `=` only: a reference containing `=`
  would still work.
- `serde_json::to_string` (behind `json!(…).to_string()`) writes compact JSON
  and escapes newlines inside strings: that is what makes test 10 pass, with
  no extra code.

## 5. `prim_client`: the HTTP call

Same split as `freebox` / `freebox_client`: no logic here, so no unit test.
Unlike the Freebox, PRIM has a certificate from a public authority: `ureq`'s
default root certificates (`webpki-roots`, built into the binary) are enough,
nothing to version.

Create the file `src/prim_client.rs` with this content, then uncomment its
line in `src/lib.rs`:

```rust
//! HTTP calls to the PRIM API, over HTTPS checked against the usual public
//! root certificates. Not unit-tested: the logic lives in `prim`, this module
//! only moves bytes. Checked by hand against the real API.

use std::time::Duration;

use ureq::Agent;

use crate::prim::stop_monitoring_url;

/// A connection to the PRIM API.
pub struct Client {
    agent: Agent,
    api_key: String,
}

/// An HTTP answer, whatever its status.
pub struct Answer {
    pub status: u16,
    pub body: String,
}

impl Client {
    pub fn new(api_key: String) -> Self {
        let agent: Agent = Agent::config_builder()
            // Quota and authorization errors come with a body: keep it.
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .into();
        Self { agent, api_key }
    }

    /// Next departures at one stop, as returned by the API.
    pub fn stop_monitoring(&self, monitoring_ref: &str) -> Result<Answer, ureq::Error> {
        let mut response = self
            .agent
            .get(stop_monitoring_url(monitoring_ref))
            .header("apikey", self.api_key.as_str())
            .header("Accept", "application/json")
            .call()?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string()?;
        Ok(Answer { status, body })
    }
}
```

## 6. The `bus-log` subcommand

In `src/main.rs`, four edits.

**Imports.** After `use e_paper::mockups::phase1_states;`, add:

```rust
use e_paper::prim::{self, Stop};
use e_paper::prim_client;
```

**Subcommand.** In `enum Command`, before `/// Clears the e-paper panel to
white.`, add:

```rust
    /// Logs the raw answers of the PRIM next departures API, one JSON line
    /// per stop and poll (observation before phase 2).
    BusLog {
        /// The PRIM API key (a secret).
        #[arg(long, default_value = "secrets/prim_api_key")]
        key: PathBuf,
        /// A stop to poll, as label=reference; repeat for each stop.
        #[arg(long = "stop", value_parser = prim::parse_stop, required = true)]
        stops: Vec<Stop>,
        /// Folder of the logs, one `<label>.jsonl` file per stop.
        #[arg(long, default_value = "bus-log")]
        dir: PathBuf,
    },
```

`value_parser = prim::parse_stop` makes `clap` call our function on each
`--stop`: a wrong argument is refused before anything runs, with our message.

**Dispatch.** In `fn run`, before `Command::Clear => {`, add:

```rust
        Command::BusLog { key, stops, dir } => bus_log(&key, &stops, &dir),
```

**Function.** Before `fn read_token(`, add:

```rust
fn bus_log(key_path: &Path, stops: &[Stop], dir: &Path) -> Result<(), Box<dyn Error>> {
    let key = fs::read_to_string(key_path)
        .map_err(|error| format!("cannot read {}: {error}", key_path.display()))?;
    let client = prim_client::Client::new(prim::api_key(&key)?);
    fs::create_dir_all(dir)?;
    for stop in stops {
        let polled_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let line = match client.stop_monitoring(&stop.monitoring_ref) {
            Ok(answer) => {
                println!(
                    "{}: HTTP {}, {} bytes",
                    stop.label,
                    answer.status,
                    answer.body.len()
                );
                prim::log_line(polled_at, answer.status, &answer.body)
            }
            Err(error) => {
                eprintln!("{}: {error}", stop.label);
                prim::log_line(polled_at, 0, &error.to_string())
            }
        };
        let path = dir.join(format!("{}.jsonl", stop.label));
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{line}")?;
    }
    Ok(())
}
```

A network failure is logged too (status `0`): a gap in the logs must be
visible, not silent.

**`.gitignore`.** The logs reveal the stops near home: keep them out of the
public repository. Under `/frame.png`, add:

```text
/bus-log/
```

## 7. Checks

```bash
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
cargo run -- bus-log --help
```

Expected: `140 passed` for the library plus the golden image test, no clippy
warning, and the help of `bus-log`.

## 8. First run on the dev PC

Replace the two references with the ones found in section 2:

```bash
cargo run -- bus-log --stop antony=STIF:StopPoint:Q:NNNNN: --stop rungis=STIF:StopPoint:Q:MMMMM:
```

Expected: `antony: HTTP 200, … bytes` and the same for `rungis`, and two files
`bus-log/antony.jsonl` and `bus-log/rungis.jsonl`. A `401` means a wrong key,
a `429` a quota reached.

## 9. A few days on the Pi

Deploy the new binary (the dashboard keeps running as before):

```bash
make deploy
```

Copy the key to your home folder on the Pi:

```bash
ssh epaper 'mkdir -m 700 -p secrets'
```

```bash
scp secrets/prim_api_key epaper:secrets/
```

```bash
ssh epaper 'chmod 600 secrets/prim_api_key'
```

Then, **on the Pi** (`ssh epaper`), start a temporary timer every 5 minutes,
running as you from your home folder (logs in `~/bus-log/`):

```bash
sudo systemd-run --unit=bus-log --on-calendar='*:0/5' --uid="$USER" --working-directory="$HOME" /usr/local/bin/e-paper bus-log --stop antony=STIF:StopPoint:Q:NNNNN: --stop rungis=STIF:StopPoint:Q:MMMMM:
```

Check it, on the Pi:

```bash
systemctl list-timers bus-log.timer
```

```bash
journalctl -u bus-log -n 20 --no-pager
```

The timer is transient: it disappears on reboot (start it again with the
same command). Let it run for **at least three weekdays and one weekend**,
peak hours included. Then stop it, on the Pi:

```bash
sudo systemctl stop bus-log.timer
```

And fetch the logs on the dev PC:

```bash
scp -r epaper:bus-log .
```

## 10. Commit

`secrets/` and `bus-log/` must **not** appear in `git status`:

```bash
git add -A
git status
git commit -m "feat: bus-log subcommand to observe PRIM next departures"
```

Expected new files: `docs/guide-09-bus-observation.md`, `src/prim.rs`,
`src/prim_client.rs`, plus `.gitignore`, `src/lib.rs`, `src/main.rs`.

## Report

- Section 2: which stop ids worked, and whether each line shows departures.
- Section 7: the output.
- Section 8: what `bus-log` prints.
- After the days on the Pi: the `bus-log/` folder (it stays on the dev PC, out
  of git). I will analyse it and turn a few answers into the fixtures of the
  next guide (parsing the departures).
