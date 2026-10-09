# Guide 07: autonomy on the Raspberry Pi

Implemented by Claude at Rémy's request (2026-10-10). The code part is done;
the Pi commands below are run by Rémy.

## What was built

- **Daily forced refresh, decided by the program.** The first cycle after
  03:00 UTC (04:00 in winter, 05:00 in summer) redraws the display even if
  nothing changed, unless it was already redrawn since. `state.json` keeps the
  time of the last redraw (`last_full_refresh`). One timer is enough, and two
  cycles can never overlap.
- **systemd units** in `systemd/`:
  - `e-paper.timer`: one cycle every 15 minutes (at :00, :15, :30, :45);
  - `e-paper.service`: one cycle (`Type=oneshot`), with a throwaway user
    created by systemd (`DynamicUser=yes`) in the `spi` and `gpio` groups,
    state in `/var/lib/e-paper/` (`StateDirectory=`), the Freebox token and
    `devices.toml` handed over as credentials from `/etc/e-paper/` (readable
    by root only), 120 s timeout, read-only system.
- **`Makefile`**: `make build`, `make deploy` (build, copy, install, reload
  systemd, enable the timer), `make status`, `make logs`.

Tests: 5 new (daily refresh due or not, `last_full_refresh` updated on redraw
and kept otherwise, an old `state.json` without the field still reads back).
95 unit tests in total, plus the golden image test.

## 1. One-time setup on the Pi

The binary copied in guide 06 (`~/e-paper` on the Pi) is enough for the setup.
If it is gone, copy it again:

```bash
make build
scp target/aarch64-unknown-linux-musl/release/e-paper epaper:~/e-paper
```

Authorize the Pi on the Freebox (its own token, written as root in
`/etc/e-paper/`), and accept on the Freebox screen:

```bash
ssh -t epaper 'sudo ~/e-paper authorize --token /etc/e-paper/freebox_app_token'
```

Install the mapping file (root only), then check it:

```bash
scp devices.toml epaper:/tmp/devices.toml
ssh epaper 'sudo install -m 600 /tmp/devices.toml /etc/e-paper/devices.toml && rm /tmp/devices.toml'
ssh epaper 'sudo ~/e-paper check-devices --devices /etc/e-paper/devices.toml'
```

Expected: one line per person, as on the dev PC.

## 2. Deploy

```bash
make deploy
```

It installs `/usr/local/bin/e-paper` and the two units, then starts the timer.
Run a first cycle right away instead of waiting for the next quarter:

```bash
ssh epaper 'sudo systemctl start e-paper.service'
make logs
make status
```

Expected in the logs: a line such as `R home, M away, H home, L away`, then
`Display redrawn: e-paper panel` (first run: no state yet). `make status`
shows the next trigger of `e-paper.timer`.

## 3. Clean up the manual test files

The files of guide 06 in the home folder of the Pi are no longer needed:

```bash
ssh epaper 'rm -rf ~/e-paper ~/secrets ~/devices.toml ~/state.json'
```

## Day-to-day

- After a code change: `make deploy`.
- After a change of `devices.toml`: run the "install the mapping file" block
  of section 1 again. The next cycle uses it, nothing to restart.
- Run a cycle now: `ssh epaper 'sudo systemctl start e-paper.service'`.
- Logs: `make logs`, or live: `ssh epaper 'journalctl -u e-paper -f'`.
- Faster cycles while testing, without editing the shipped unit:
  `ssh -t epaper 'sudo systemctl edit e-paper.timer'`, then add
  `[Timer]`, `OnCalendar=`, `OnCalendar=*:0/2` (every 2 minutes). Remove the
  override with `sudo systemctl revert e-paper.timer`.
- Stop everything: `ssh epaper 'sudo systemctl disable --now e-paper.timer'`.

## Report

Share the output of `check-devices`, `make deploy`, `make logs` and
`make status`.
