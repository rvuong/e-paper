# Decommissioning inventory (Raspberry Pi)

Read-only commands to inventory the old `rpi_quote` setup before decommissioning it.
Nothing below modifies the Pi. Commands prefixed with `sudo` only read
root-owned files.

Run them on the Pi over SSH. Replace `<path>` with the location found in step 4.

## 1. System and target architecture

Decides the Rust target (`aarch64` or `armv7`).

```bash
uname -m
getconf LONG_BIT
cat /etc/os-release
hostname -I
free -h
df -h /
```

## 2. Scheduled jobs

```bash
crontab -l
sudo crontab -l
ls -la /etc/cron.d/ /etc/cron.hourly/ /etc/cron.daily/
sudo grep -rniE "quote|epd|epaper|main\.py" /etc/crontab /etc/cron.d/ /var/spool/cron/ 2>/dev/null
```

## 3. Services and boot-time scripts

```bash
systemctl list-units --type=service --all | grep -iE "quote|epaper|epd"
systemctl list-timers --all
ls -la /etc/systemd/system/
cat /etc/rc.local 2>/dev/null
grep -niE "quote|epd|python" ~/.bashrc ~/.profile 2>/dev/null
```

## 4. Project files

```bash
sudo find / -xdev \( -iname "*rpi_quote*" -o -name "epd7in5_V2.py" \) 2>/dev/null
find ~ -maxdepth 4 \( -name ".env" -o -name "venv" -o -name ".venv" \) 2>/dev/null
```

Once the project path is known:

```bash
ls -la <path>
git -C <path> remote -v
git -C <path> status
git -C <path> log --oneline -5
sed 's/=.*/=<redacted>/' <path>/.env
ls <path>/venv/bin/ 2>/dev/null && <path>/venv/bin/pip list 2>/dev/null
```

`git status` reveals local changes that are not on GitHub. The `.env` is shown
with values masked.

## 5. Running processes and recent executions

```bash
ps aux | grep -iE "python|quote" | grep -v grep
journalctl -u cron --since "3 days ago" --no-pager | grep -iE "quote|python"
grep -i "CRON" /var/log/syslog 2>/dev/null | grep -iE "quote|python" | tail -20
```

## 6. Hardware interfaces (to keep)

```bash
ls -l /dev/spidev* /dev/gpiomem /dev/gpiochip*
grep -nE "^dtparam=spi" /boot/firmware/config.txt /boot/config.txt 2>/dev/null
groups
```

## Saving the output on the dev PC

To keep a trace of the inventory, run any block from the dev PC and save its
output locally, for example:

```bash
ssh epaper 'uname -m; getconf LONG_BIT; cat /etc/os-release' > inventory-system.txt
```
