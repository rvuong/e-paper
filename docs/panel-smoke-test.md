# Panel smoke test

Checks that the Pi still drives the panel, before any Rust code exists. It
reuses the Waveshare Python driver bundled in the `rpi_quote` clone on the Pi
(`waveshare_epd/epd7in5_V2.py`), which is known to work on this hardware.

Expected result: the panel flashes, turns **white**, then fully **black** for a
few seconds, then **white** again. The current quote is erased; the old cron
job will draw a new one at its next run.

Run everything from the dev PC, one block at a time.

## 1. Log in to the Pi

With the alias, once section 4 of
[`dev-environment-setup.md`](dev-environment-setup.md) is done:

```bash
ssh epaper
```

Before that, with a password: `ssh <pi-user>@<pi-address>`.

## 2. Find the old project and its Python

The rest of the commands run **on the Pi**.

The clone may have another name (on this Pi: `~/ePaper`). Look for the driver
itself: the project folder is the parent of `waveshare_epd/`.

```bash
sudo find / -xdev -name 'epd7in5_V2.py' 2>/dev/null
```

Go to the folder it prints, then look for a virtualenv (`venv`, `.venv`, `env`…):

```bash
cd <rpi_quote-folder>
ls -a
```

Set `PY` to the virtualenv's Python, or to the system one if there is no
virtualenv:

```bash
PY=./venv/bin/python   # adapt; or PY=python3
$PY -c 'import PIL, spidev, gpiozero; print("ok")'
```

## 3. Make sure nothing else is driving the panel

Two programs must never use the panel at the same time. This must print nothing:

```bash
pgrep -af 'main.py'
```

## 4. Flash the panel

```bash
cat > /tmp/flash.py <<'EOF'
import time
from PIL import Image
from waveshare_epd import epd7in5_V2

epd = epd7in5_V2.EPD()
print("init")
epd.init()
print("white")
epd.Clear()
print("black")
epd.display(epd.getbuffer(Image.new('1', (epd.width, epd.height), 0)))
time.sleep(3)
print("white")
epd.Clear()
print("sleep")
epd.sleep()
print("done")
EOF
PYTHONPATH=. $PY /tmp/flash.py
```

It takes about 20 to 30 seconds. Then clean up:

```bash
rm /tmp/flash.py
```

## If it fails

- `Permission denied` on `/dev/spidev0.0` or the GPIO: the user is not in the
  `spi` and `gpio` groups. Check with `groups`; as a one-off, retry with
  `sudo PYTHONPATH=. $PY /tmp/flash.py`.
- `No such file or directory: '/dev/spidev0.0'`: SPI is disabled. Check with
  `ls /dev/spidev*`.
- It hangs on `init` or after `white`: the driver waits for the panel's BUSY
  signal. Check the ribbon cables and the adapter board, then stop with Ctrl+C.

Share the output of step 4 in any case.
