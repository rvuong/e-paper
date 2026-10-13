# Build and deploy to the Raspberry Pi (SSH alias `epaper`, passwordless sudo).
# One-time setup on the Pi first: see docs/guide-07-autonomy.md.

PI := epaper
TARGET := aarch64-unknown-linux-musl
BINARY := target/$(TARGET)/release/e-paper

.PHONY: help build deploy status logs shell

help: ## List the available targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F ':.*## ' '{printf "  %-8s %s\n", $$1, $$2}'

build: ## Cross-compile the release binary for the Pi
	cargo zigbuild --release --target $(TARGET)

deploy: build ## Build, copy the binary and units to the Pi, restart the timer
	scp $(BINARY) systemd/e-paper.service systemd/e-paper.timer $(PI):/tmp/
	ssh $(PI) 'sudo install -m 755 /tmp/e-paper /usr/local/bin/e-paper \
		&& sudo install -m 644 /tmp/e-paper.service /tmp/e-paper.timer /etc/systemd/system/ \
		&& rm /tmp/e-paper /tmp/e-paper.service /tmp/e-paper.timer \
		&& sudo systemctl daemon-reload \
		&& sudo systemctl enable --now e-paper.timer'

status: ## Show the timer and the last cycle on the Pi
	@# `systemctl status` exits with 3 when the service is inactive, as it is between cycles.
	ssh $(PI) 'systemctl list-timers e-paper.timer; systemctl status e-paper.service --no-pager || true'

logs: ## Show the last 50 log lines of the service
	ssh $(PI) 'journalctl -u e-paper.service -n 50 --no-pager'

shell: ## Open an SSH shell on the Pi
	ssh $(PI)
