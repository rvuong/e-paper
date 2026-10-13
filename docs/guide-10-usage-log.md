# Guide 10: usage log (threshold check)

Implemented by Claude at Rémy's request (2026-10-13), in TDD. The code is
done; what remains is the activation on the Raspberry Pi (section "On the Pi")
and, after a week, the chart.

## Why

The activity mark's 5 Mb/s threshold was never checked against real usage.
For about a week, every cycle logs each assigned device's download. Then
Claude reads the log and draws a private interactive chart: one curve per
person, each a distinct colour with a legend, rate per 5 min, the 5 Mb/s
threshold as a line, away periods shaded, and a table of the biggest peaks
(when, how long, how much).

## The log

`run --usage-log <file>` appends to a CSV file, with the header first when
the file is new or empty. One line per assigned device seen by the Freebox,
per cycle:

```text
timestamp,elapsed_secs,person,device,home,delta_bytes,counter
1791879390,20,L,Laptop,true,6291565,355865570
```

| Column | Meaning |
|---|---|
| `timestamp` | Unix time of the cycle, in seconds |
| `elapsed_secs` | Time since the previous run; empty on the first run. A large gap is a downtime, not a peak |
| `person` | R, M, H or L |
| `device` | Name from `devices.toml` (quoted if it holds a comma or a quote). **Never the MAC address** |
| `home` | Whether the person is home (their smartphone), as shown on the panel |
| `delta_bytes` | Download since the previous run (`counter_delta`: a counter that went down counts from 0). Empty without a previous reading, to avoid a fake peak |
| `counter` | Raw `tx_bytes`, to spot counter resets afterwards |

Shared and unassigned devices are not logged. Wired devices have no counter,
so no line. About 2 MB a week.

The log is personal data (who downloads when): it stays on the Pi and the dev
PC, never committed (`/usage.csv` is in `.gitignore`). A write failure prints
a warning and never stops the cycle.

## Code

| Module | Change | New tests |
|---|---|---|
| `devices` | `Assignment::name` (no longer `Copy`) | 1 |
| `usage` (new) | `HEADER`, `usage_lines`, `csv_field` | 7 |
| `main.rs` | `run --usage-log`, `append_usage` | — |

Tests:

1. `assignment_keeps_the_device_name`: `parse_mapping` keeps `name`.
2. `one_line_per_assigned_device_seen`: unassigned devices give no line.
3. `delta_is_measured_since_the_previous_run`: delta over one cycle, not
   over the 15-min rate window, to date peaks to the minute.
4. `counter_reset_gives_the_current_counter`: same guard as the rate.
5. `no_previous_reading_leaves_the_delta_empty`: no fake peak.
6. `home_column_follows_presence`.
7. `elapsed_is_the_gap_since_the_previous_run`: empty on the first run.
8. `csv_header_and_quoting`.

121 unit tests plus the golden image test. Checked on the dev PC against the
real Freebox: two runs 20 s apart, the second with deltas.

## Repeater check (`check-counters`)

The first log lines had no device of R nor H: they go through the Wi-Fi
repeater, whose clients are missing from `wifi/ap/{id}/stations/`.
`e-paper check-counters` prints, for each assigned device (never its MAC):
whether the LAN browser sees it, how it is connected (`access_point` type and
connectivity), whether `access_point` has a `tx_bytes` counter, and whether
`wifi/ap` has one; then the names of the `access_point` fields.

```bash
cargo run -- check-counters
```

Result on 2026-10-13: devices on the repeater have `access_point.tx_bytes`,
and for devices on the box it matches the `wifi/ap` counter.

**Fix:** `run` now merges both sources (`freebox::download_counters`,
highest value per device), so the activity mark and the log cover the
repeater. Tests: `host_counters_come_from_access_point_tx_bytes`,
`wifi_and_host_counters_are_merged_keeping_the_highest` (`freebox`),
`repeater_device_gets_a_rate_in_the_cycle` (`cycle`). Checked on the dev PC:
R's devices on the repeater now have lines in the log.

Tests: `host_access_point_is_parsed`, `missing_access_point_fields_are_tolerated`
(`freebox`), `report_line_per_assigned_device_without_mac`,
`device_absent_from_lan_browser_is_reported_not_seen`,
`wifi_counter_presence_is_reported`, `access_point_field_names_are_listed_once`
(`diagnose`, new). 130 unit tests in all, with the fix below.

## On the Pi

Deploy the new binary:

```bash
make deploy
```

Turn the log on with a drop-in override of the service:

```bash
ssh -t epaper 'sudo systemctl edit e-paper.service'
```

with:

```ini
[Service]
ExecStart=
ExecStart=/usr/local/bin/e-paper run --epd --token %d/freebox_app_token --devices %d/devices.toml --state /var/lib/e-paper/state.json --usage-log /var/lib/e-paper/usage.csv
```

Check after two or three minutes (one line per device and per minute):

```bash
ssh epaper 'sudo tail -n 5 /var/lib/e-paper/usage.csv'
make logs
```

## After a week

Copy the log to the dev PC (project root, ignored by git):

```bash
ssh epaper 'sudo cat /var/lib/e-paper/usage.csv' > usage.csv
```

Then ask Claude for the chart. Turn the log off (back to the shipped unit),
and delete the file on the Pi once the chart is done:

```bash
ssh epaper 'sudo systemctl revert e-paper.service'
ssh epaper 'sudo rm /var/lib/e-paper/usage.csv'
```
