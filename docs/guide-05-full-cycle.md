# Guide 05: one full cycle

Exceptionally implemented by Claude at Rémy's request (2026-10-09), in TDD
(19 new tests red first, then green). This page records what was built and how
to use it.

## What it does

`e-paper run` performs what the program will do every 15 minutes: poll the
Freebox, compute the state of each person, redraw the display if something
changed, save the state for the next run. Until the e-paper driver exists, the
display is drawn into a PNG file (`target/e-paper.png` by default).

`e-paper check-devices` validates `devices.toml` and prints one line per person
(number of devices, smartphone or not).

## Approved decisions

- One timestamp per poll in the saved state; a device's rate uses the time
  since the previous poll, also for a device seen for the first time.
- First run (no saved state): every rate is 0. Presence is already right.
- Unreadable or corrupt state file: ignored with a warning, display redrawn.
- State in `state.json` (ignored by git), path set by `--state`.
- Only the counters of assigned devices are saved.
- A device seen on two access points keeps its highest counter.

## Modules

| Module | Role | Tests |
|---|---|---|
| `freebox` | Wi-Fi access points, stations, merge of all access points; empty lists | 4 |
| `saved` | `SavedState` JSON file: `load` (missing or corrupt = no state), `save` (write then rename) | 3 |
| `cycle` | Pure cycle: mapping + Freebox data + previous state → household state, refresh decision, state to save | 10 |
| `devices` | `summary` for `check-devices` | 2 |
| `freebox_client` | `wifi_counters`: `GET wifi/ap/`, then `GET wifi/ap/{id}/stations/` for each | none |
| `main.rs` | `run` and `check-devices` | none |

`Person` and `PersonState` now derive `Serialize` and `Deserialize`.

**Added test** (beyond the 18 approved): an access point without clients. The
Freebox omits `result` for an empty list; `parse_response` now relies on
`parse_optional`, and lists go through `parse_list`, which turns a missing
`result` into an empty list.

## Checked on the real Freebox (2026-10-09)

- `check-devices`: the mapping is valid, every person has a smartphone.
- First `run`: `R home, M home, H home, L home`, display drawn.
- Second `run`, one minute later: `No change: display not touched`.
- The app token has the rights needed to read the Wi-Fi stations.
- Only 3 of the assigned devices were seen as Wi-Fi stations at that time
  (the others were offline or wired). Wired devices never count for the mark.

## Use

```bash
cargo run -- check-devices
cargo run -- run
cargo run -- run --force          # redraw even without change
cargo run -- run --interval 300   # shorter presence window, for tests
```

Options of `run`: `--token`, `--devices`, `--state`, `--png`, `--interval`
(seconds, default 900), `--force`.

## Next

- Log per-device rates for a few days to check the 5 Mb/s threshold.
- The daily forced refresh, and the systemd timer on the Pi.
- The e-paper driver, replacing the PNG output on the Pi.
