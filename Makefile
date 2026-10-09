# Build and deploy to the Raspberry Pi (SSH alias `epaper`, passwordless sudo).
# One-time setup on the Pi first: see docs/guide-07-autonomy.md.

PI := epaper
TARGET := aarch64-unknown-linux-musl
BINARY := target/$(TARGET)/release/e-paper

.PHONY: build deploy status logs

build:
	cargo zigbuild --release --target $(TARGET)

deploy: build
	scp $(BINARY) systemd/e-paper.service systemd/e-paper.timer $(PI):/tmp/
	ssh $(PI) 'sudo install -m 755 /tmp/e-paper /usr/local/bin/e-paper \
		&& sudo install -m 644 /tmp/e-paper.service /tmp/e-paper.timer /etc/systemd/system/ \
		&& rm /tmp/e-paper /tmp/e-paper.service /tmp/e-paper.timer \
		&& sudo systemctl daemon-reload \
		&& sudo systemctl enable --now e-paper.timer'

status:
	ssh $(PI) 'systemctl list-timers e-paper.timer; systemctl status e-paper.service --no-pager'

logs:
	ssh $(PI) 'journalctl -u e-paper.service -n 50 --no-pager'
