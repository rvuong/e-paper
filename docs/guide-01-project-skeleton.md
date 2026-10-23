# Guide 01: Cargo project skeleton

Creates the Rust project in the existing folder, without any behaviour yet, and
checks the whole toolchain end to end: native build, lints, cross-compilation,
run on the Pi.

No tests in this guide: the skeleton has no behaviour to test. The first tests
come with guide 02 (first render), defined and approved before any code.

Run every command from the project root on the dev PC
(`~/projects/personal/e-paper`). Files are created and edited in VS Code.

## 1. Pin the toolchain

`rust-toolchain.toml` makes every build (dev PC, CI) use the same Rust version,
with the components and the Pi target. rustup reads it automatically and
installs what is missing on the first `cargo` command (a one-time download, as
`1.99.0` is a separate toolchain from `stable` for rustup). No command generates
this file.

Create the file `rust-toolchain.toml` with this content:

```toml
[toolchain]
channel = "1.99.0"
components = ["clippy", "rustfmt"]
targets = ["aarch64-unknown-linux-musl"]
```

Bump the version deliberately later (one-line change, then `cargo build`).

## 2. Create the Cargo project and the git repository

`cargo init` creates `Cargo.toml`, `src/main.rs` (a "Hello, world!"), a
`.gitignore` and runs `git init`. It leaves existing files alone.

```bash
cargo init --name e-paper --vcs git
```

The global git identity is the work one. Set the personal identity for this
repository only (adapt the name):

```bash
git config user.name "Rémy Vuong"
git config user.email "4727510+rvuong@users.noreply.github.com"
git config user.email
```

Open `Cargo.toml` and complete the `[package]` section so it looks like this
(keep the `edition` cargo wrote, `2024` expected):

```toml
[package]
name = "e-paper"
version = "0.1.0"
edition = "2024"
rust-version = "1.99"
description = "Household e-paper dashboard on a Raspberry Pi"
publish = false

[dependencies]
```

- `rust-version`: the minimum Rust version, checked by cargo.
- `publish = false`: the crate is never published to crates.io by mistake.
- No dependency yet: each one is added by the guide that needs it.

## 3. `.gitignore`

Secrets and personal data must never reach the public repository. The file
names below are provisional: they are adjusted when the tools that produce them
are written.

Replace the whole content of `.gitignore` (written by `cargo init`) with:

```gitignore
# Build output
/target

# Secrets and personal data: never committed (only *.example.* files are)
/.env
/config.toml
/devices.toml
/secrets/

# IDE
/.idea/

# Local files used by tools/ (font, Python virtualenv)
/tools/.venv/
/tools/*.ttf
```

Paths start with `/` so they only match at the project root (for example,
`/config.toml` does not hide a future `.cargo/config.toml`).

## 4. VS Code recommended extensions

VS Code suggests them when the folder is opened on another machine.

Create the file `.vscode/extensions.json` with this content:

```json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "vadimcn.vscode-lldb",
    "tamasfe.even-better-toml"
  ]
}
```

## 5. Check the native build

```bash
cargo run
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Expected: `Hello, world!`, `running 0 tests` (no error), no clippy warning, no
`fmt` output. The first command also downloads the pinned toolchain.

## 6. Check the cross-compilation and the Pi

This validates Zig 0.17 with `cargo-zigbuild`, and the binary on the real
hardware. The automatic SSH `runner` comes in a later guide.

```bash
cargo zigbuild --release --target aarch64-unknown-linux-musl
file target/aarch64-unknown-linux-musl/release/e-paper
scp target/aarch64-unknown-linux-musl/release/e-paper epaper:/tmp/
ssh epaper '/tmp/e-paper; rm /tmp/e-paper'
```

Expected: `file` reports an `ARM aarch64` `statically linked` executable, and
the Pi prints `Hello, world!`.

If `cargo zigbuild` fails with a Zig error, Zig 0.17 is probably too recent for
`cargo-zigbuild`: install the previous Zig release (section 3 of
[`dev-environment-setup.md`](dev-environment-setup.md)) and retry.

## 7. First commit

Check what is about to be committed: no secret, no personal file, no `target/`
nor `.idea/`.

```bash
git add -A
git status
```

Expected files: `.gitignore`, `.vscode/extensions.json`, `CLAUDE.md` (project
notes, local only: ignored by git since 2026-10-23),
`Cargo.lock`, `Cargo.toml`, `docs/…`, `rust-toolchain.toml`, `src/main.rs`,
`tools/export_mockups.py`.

```bash
git commit -m "chore: initial project skeleton"
```

No GitHub remote yet: the repository is public, so its content is reviewed
before the first push.

## Report

Share the output of sections 5 and 6, and `git status` from section 7.
