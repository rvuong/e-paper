# Development environment setup

Commands to prepare the dev PC (Ubuntu) and the SSH access to the Raspberry Pi.
Run them yourself, in order. Nothing here touches the project files: the Cargo
project skeleton comes afterwards.

Already done: VS Code (Microsoft apt repository) with the rust-analyzer,
CodeLLDB and Even Better TOML extensions.

## 1. System packages

Compiler toolchain for native dependencies, `pkg-config`, and SDL2 for the
desktop simulator (`embedded-graphics-simulator`).

```bash
sudo apt update
sudo apt install -y build-essential pkg-config curl git libsdl2-dev
```

Check:

```bash
pkg-config --modversion sdl2
```

## 2. Rust toolchain (rustup)

User-level install in `~/.rustup` and `~/.cargo`, no `sudo`. Accept the default
installation (option 1) when prompted.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

The installer adds `~/.cargo/bin` to the `PATH` for new shells. Open a new
terminal, then check:

```bash
rustc --version
cargo --version
rustup component list --installed
```

`clippy` and `rustfmt` come with the default profile. If they are missing:

```bash
rustup component add clippy rustfmt
```

Keep the toolchain up to date later with:

```bash
rustup update
```

## 3. Cross-compilation for the Pi

The Pi target depends on its OS architecture (see section 5). Install both
targets now; only one will be used.

```bash
rustup target add aarch64-unknown-linux-musl armv7-unknown-linux-musleabihf
```

### Zig (used as the cross linker)

Check the latest stable version on <https://ziglang.org/download/> and set it below.

```bash
ZIG_VERSION=0.14.1
mkdir -p "$HOME/.local/opt" "$HOME/.local/bin"
curl -fL "https://ziglang.org/download/${ZIG_VERSION}/zig-x86_64-linux-${ZIG_VERSION}.tar.xz" \
  -o "/tmp/zig-${ZIG_VERSION}.tar.xz"
tar -xf "/tmp/zig-${ZIG_VERSION}.tar.xz" -C "$HOME/.local/opt"
ln -sf "$HOME/.local/opt/zig-x86_64-linux-${ZIG_VERSION}/zig" "$HOME/.local/bin/zig"
```

If the archive name differs on the download page (older releases use
`zig-linux-x86_64-<version>`), adapt the two paths accordingly.

`~/.local/bin` is in the default Ubuntu `PATH` once it exists (open a new
terminal). Check:

```bash
zig version
```

### cargo-zigbuild

```bash
cargo install --locked cargo-zigbuild
cargo-zigbuild --version
```

## 4. SSH access to the Pi

First, give the Pi a fixed address so the SSH alias keeps working: in Freebox
OS, "Paramètres de la Freebox" → "DHCP" → "Baux statiques", add a lease for the
Pi's MAC address (shown in "Périphériques réseau") with its current IPv4
address. If the static lease cannot be added, the "Serveur DHCP" tab option
"Assignation fixe par machine" (enabled by default) already gives each MAC the
same address every time. Do not write the address or the MAC in this repository: they only go
in `~/.ssh/config`.

Use a dedicated key for the Pi, with the personal e-mail address as comment
(never a work key):

```bash
ssh-keygen -t ed25519 -f ~/.ssh/id_ed25519_epaper -C "<personal-email>"
```

Copy only that public key to the Pi. Replace `<pi-user>` and `<pi-address>`
(IP address, or `raspberrypi.local` if mDNS works on the network). Without
`-i`, `ssh-copy-id` copies every key found in `~/.ssh/`.

```bash
ssh-copy-id -i ~/.ssh/id_ed25519_epaper.pub <pi-user>@<pi-address>
```

Add a short alias to `~/.ssh/config`:

```bash
cat >> "$HOME/.ssh/config" <<'EOF'

Host epaper
    HostName <pi-address>
    User <pi-user>
    IdentityFile ~/.ssh/id_ed25519_epaper
    IdentitiesOnly yes
EOF
chmod 600 "$HOME/.ssh/config"
```

Edit the placeholders in `~/.ssh/config`, then check that the login works
without a password:

```bash
ssh epaper 'hostname; uptime'
```

### Lost user name or password

First try `pi` with `raspberry` (default account of Raspberry Pi OS images
older than April 2022), and look in the password manager.

Otherwise, install the public key directly on the SD card. Shut the Pi down
(unplug it if there is no other way), put its SD card in the dev PC: Ubuntu
mounts the system partition as `rootfs`.

```bash
R=/media/$USER/rootfs
ls "$R/etc/passwd"
awk -F: '$3>=1000 && $3<60000 {print $1, $3, $6}' "$R/etc/passwd"
```

The last command lists the Pi's accounts: name, user ID, home. Set them below
(usually ID `1000`):

```bash
U=<pi-user>
ID=1000
sudo mkdir -p "$R/home/$U/.ssh"
cat ~/.ssh/id_ed25519_epaper.pub | sudo tee -a "$R/home/$U/.ssh/authorized_keys" > /dev/null
sudo chown -R "$ID:$ID" "$R/home/$U/.ssh"
sudo chmod 700 "$R/home/$U/.ssh"
sudo chmod 600 "$R/home/$U/.ssh/authorized_keys"
sync
```

Eject the card from the file manager, put it back in the Pi, power it on, then
log in with the key (no password asked):

```bash
ssh <pi-user>@<pi-address>
```

On the Pi, set a new password and store it in the password manager.
Raspberry Pi OS usually allows `sudo` without a password for the first account:

```bash
sudo passwd <pi-user>
```

Then go on with the alias above (`ssh-copy-id` is no longer needed).

## 5. Pi architecture (decides the Rust target)

```bash
ssh epaper 'uname -m; getconf LONG_BIT'
```

| Output | Rust target |
|---|---|
| `aarch64` / `64` | `aarch64-unknown-linux-musl` |
| `armv7l` / `32` | `armv7-unknown-linux-musleabihf` |

Report the result: it decides the build target.

## 6. VS Code extensions (for reference)

Already installed. To reinstall on another machine:

```bash
code --install-extension rust-lang.rust-analyzer
code --install-extension vadimcn.vscode-lldb
code --install-extension tamasfe.even-better-toml
```

## Final check

```bash
rustc --version && cargo --version && cargo clippy --version && cargo fmt --version
pkg-config --modversion sdl2
zig version
cargo-zigbuild --version
rustup target list --installed
ssh epaper 'uname -m'
```
