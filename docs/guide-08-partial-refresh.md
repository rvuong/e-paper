# Guide 08: partial refresh, one cycle a minute

Implemented by Claude at Rémy's request (2026-10-10), in TDD. The code is
done; what remains is the activation on the Raspberry Pi (section "On the Pi").

## What changes

- **One cycle a minute** (`systemd/e-paper.timer`: `OnCalendar=*:*:00`).
- **Partial refresh of the slots that changed**: when only some people change,
  only their slots are refreshed, without flashing. The panel's controller is
  given a window (the union of the changed slots, 178 × 178 px each, widened to
  multiples of 8 horizontally) and compares the previous and the new image
  pixel by pixel: only the pixels that differ move.
- **Full refresh** when a partial one is not possible or not wise:
  `--force`, the daily refresh (first cycle after 03:00 UTC), no saved state,
  no saved image, or `PARTIALS_BEFORE_FULL` (10) partial refreshes in a row
  (partial refreshes leave a faint trace of the previous image: ghosting).
- **Presence window** (option `--interval`): independent of the cycle period,
  so a phone asleep for a few minutes still counts as home. Lowered from 15 to
  5 minutes on 2026-10-10 (departures took too long to show).
- **Rates averaged over 15 min** (`RATE_WINDOW_SECS`): the state keeps the
  readings of the counters of the last 15 minutes, and the rate is measured
  from the oldest one. Otherwise the activity mark would follow every pause of
  a video, minute by minute.

## How the partial refresh works

- The Raspberry Pi has nothing to enable: same SPI, same pins. Everything is in
  the commands sent to the panel's controller.
- `init_part` (driver): reset, full register setup, then **partial waveforms
  written into the controller's registers** (`0x20` to `0x25`, panel setting
  `0x3F` = waveforms from the registers), taken from the GxEPD2 library: white
  to white and black to black are empty, so the pixels that keep their colour
  are not driven at all. `0x50` = `0x39 0x07` (bit 1 is white in both images).
- `display_partial` (driver): `0x91` (partial mode), `0x90` (window), `0x10`
  (previous image), `0x13` (new image), `0x12` (refresh), wait for BUSY, `0x92`
  (partial out).
- **The previous image** must be sent again (`0x10`): the controller forgets it
  when the program cuts the panel's power at the end of each run. It is saved
  as `frame.png` next to `state.json` after each redraw.

## Fixed after the first tests on the panel (2026-10-10)

1. **Polarity.** The previous image was first sent with 1 for black. With DDX
   = 01 (`0x50`), bit 1 is white in both images: the controller saw every pixel
   of the window as changed. Fixed: both images with 1 for white.
2. **Waveform.** The window still flashed: the partial waveform built into the
   controller (forced temperature `0xE5 = 0x6E`, as in the Waveshare driver)
   pulses every pixel of the window. Replaced by the GxEPD2 register waveforms,
   which leave unchanged pixels alone. GxEPD2 calls them "experimental": watch
   for ghosting, and tune `PARTIALS_BEFORE_FULL` if needed.

Checked on the panel after both fixes: no flashing, only the changed pixels
move.

## References

- Waveshare, 7.5inch e-Paper HAT manual (official): partial refresh needs a
  full refresh "after refreshing partially several times", sleep or power off
  between refreshes; links the V2 specification.
  <https://www.waveshare.com/wiki/7.5inch_e-Paper_HAT_Manual>
- Waveshare, 7.5inch e-Paper V2 specification (official PDF):
  <https://files.waveshare.com/upload/6/60/7.5inch_e-Paper_V2_Specification.pdf>
- GxEPD2, the reference Arduino library for these panels (GDEW075T7 = the
  Waveshare 7.5" V2): `GxEPD2_750_T7.cpp`, partial waveforms in registers,
  `0x50 = 0x39`, both images with 1 for white, `0x91`/`0x90`/`0x92`.
  <https://github.com/ZinggJM/GxEPD2/blob/master/src/epd/GxEPD2_750_T7.cpp>
- UC8179 driver of the idfxx project (Apache-2.0, ESP-IDF component): commands
  0x90/0x91/0x92, 0x50 (DDX polarity, N2OCP copy of the new image into the
  previous one), 0xE0/0xE5 waveform bank, a full refresh every ~10 partial ones.
  <https://components.espressif.com/components/cleishm/idfxx_epaper_uc8179/versions/1.0.0/readme>,
  source: <https://github.com/cleishm/idfxx/tree/main/components/idfxx_epaper_uc8179>

## Code

| Module | Change | New tests |
|---|---|---|
| `layout` | `Window`, `slot_window`, `Window::union` | 2 |
| `update` (new) | `plan_update`: none / full / partial(window); `changed_window` | 7 |
| `cycle` | `UpdateKind` instead of `refresh`; partial count; rate window | 4 |
| `saved` | `partials_since_full`, `earlier` readings (old files still load) | — |
| `epd` | `init_part`, `display_partial`, `window_buffer` | 3 |
| `display` | `Update::Full` / `Update::Partial { previous, window }` | 2 |
| `main.rs` | `frame.png`, `run --full-only`, `partial-test` | — |

113 unit tests plus the golden image test. Checked on the dev PC against the
real Freebox: first run full (no saved image), second run unchanged.

## On the Pi

Deploy (binary and the new timer):

```bash
make deploy
```

Stop the timer during the check, so that no cycle drives the panel at the same
time, then run the partial refresh check:

```bash
ssh epaper 'sudo systemctl stop e-paper.timer'
ssh epaper 'e-paper partial-test --epd'
```

Expected on the panel: one full refresh (everybody away), then four partial
refreshes 5 s apart (R home; H home with the ring; R away and L home;
everybody away), **without flashing**, only the changed circles moving.

After `partial-test` (or `test-pattern`), the panel no longer shows the image
saved by the service (`frame.png`): a partial refresh would diff against the
wrong image. Delete it, so that the next cycle does a full refresh, then
restart the timer:

```bash
ssh epaper 'sudo rm -f /var/lib/e-paper/frame.png'
ssh epaper 'sudo systemctl start e-paper.timer'
make logs
```

Expected in the logs, one cycle a minute: `No change: display not touched`
most of the time, `Display redrawn (partial)` when someone arrives or leaves.

**If the partial refresh looks wrong** (inverted circles, flashing, traces),
keep the dashboard on full refreshes while it is fixed: add `--full-only` to
the `ExecStart` line, with a drop-in override:

```bash
ssh -t epaper 'sudo systemctl edit e-paper.service'
```

with:

```ini
[Service]
ExecStart=
ExecStart=/usr/local/bin/e-paper run --epd --full-only --token %d/freebox_app_token --devices %d/devices.toml --state /var/lib/e-paper/state.json
```

Remove it later with `ssh epaper 'sudo systemctl revert e-paper.service'`.
