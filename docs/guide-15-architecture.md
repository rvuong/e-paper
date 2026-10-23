# Guide 15: code architecture (functional core, imperative shell)

**Apply it after the phase 2 deployment (2026-10-20),** on a branch, one
commit per step. Approved tests: 10 (2026-10-14), with the compromises of
section 2.

Checked while writing this guide: it was replayed step by step on a copy of
the project (each red step fails as described, each green step passes; 190
library tests, 3 architecture tests and the 2 golden tests at the end,
clippy clean, `make build` fine), and a real `run` on the dev PC drew the
screen from the Freebox, then left the display alone on the next run.

Run every command from the project root; edit the files in VS Code.

Decided 2026-10-14: target B of the architecture comparison (functional core,
imperative shell, made explicit), and Freebox `Host` translated into a domain
type before it reaches the core. Discarded: layered architecture,
hexagonal architecture (one trait per external system), clean architecture
in a multi-crate workspace. Effort budget: two evenings.

## 1. Goal

The code already mostly follows this style: the logic is pure and the I/O
sits in `main.rs` and in the two HTTP clients. This guide makes it visible
and enforced:

- the crate is split into four layers, one folder each;
- dependencies point inwards only, and a test checks it;
- the core no longer knows the Freebox or PRIM formats;
- the orchestration of `e-paper run` leaves `main.rs`, and its decisions
  (what to draw, which refresh, what to save) get tests.

No behaviour change: same commands, same options, same `state.json`, same
screen. The golden images and the existing tests guard every step.

## 2. Target

```text
            main.rs (clap, wiring)
                   │
                   ▼
   ┌──────────── app ────────────┐   orchestration: pure `compose` + the I/O shell
   │               │             │
   ▼               ▼             │
adapters ──────► render          │   adapters: I/O and external formats
   │               │             │   render: pixels, pure
   └──────►  domain  ◄───────────┘   domain: rules, pure, no format, no pixel
```

Allowed dependencies (arrow = "may use"):

| Layer | May use | Never |
|---|---|---|
| `domain` | `std` (no I/O), `jiff`, `serde` derives | `render`, `adapters`, `app`, `std::fs`, `ureq`, `serde_json`, `toml`, `embedded-graphics`, `fontdue` |
| `render` | `domain`, `embedded-graphics`, `fontdue`, `png` | `adapters`, `app`, `std::fs`, `ureq` |
| `adapters` | `domain`, `render`, anything external | `app` |
| `app` | everything below | — |
| `main.rs` | `app` (and the types its options need) | — |

Accepted compromises:

- **`serde` derives stay on domain types** (`SavedState`, `BusState`,
  `Answer`, `Person`…). They describe the data, not a format; reading and
  writing `state.json` is in `adapters`. Mirror types for the file would
  double the code for no gain here.
- **Tests may cross layers.** The domain tests keep parsing the PRIM
  fixtures with `adapters::prim::parse`: rewriting a dozen tests with hand-built
  answers is not worth it. The architecture tests only check the code
  before `#[cfg(test)]`.
- **No new trait.** `Display` stays the only port; the Freebox and PRIM
  clients stay concrete, called from the shell only.

### Module mapping

| Today | Target | Change |
|---|---|---|
| `state.rs` | `domain/household.rs` | also receives `PEOPLE` (from `layout`) and `HouseholdState` (from `refresh`) |
| `presence.rs` | `domain/presence.rs` | moved |
| `rate.rs` | `domain/rate.rs` | moved |
| `cycle.rs` | `domain/cycle.rs` | `hosts: &[Host]` becomes `seen: &[DeviceSeen]` |
| — | `domain/devices.rs` | new: `DeviceSeen`, plus `Assignment` and `Mapping` (from `devices.rs`) |
| `saved.rs` (types) | `domain/saved.rs` | `SavedState`, `Snapshot` |
| `departures.rs` | `domain/departures.rs` | without `parse` and the SIRI structs; `selected` and `serves` become `pub` |
| `bus.rs` | `domain/bus.rs` | without `read_answer` |
| `board.rs` (data) | `domain/screen.rs` | `Time`, `Block`, `Board`: what the screen says, without pixels |
| `refresh.rs` | — | removed: `needs_refresh` is unused since guide 14 (4 tests removed) |
| `board.rs` (geometry) | `render/board.rs` | `board_layout`, `BoardLayout`, `NO_DEPARTURE`… |
| `layout.rs` | `render/layout.rs` | without `PEOPLE` |
| `render.rs` | `render/draw.rs` | moved (`render/mod.rs` holds the module list) |
| `frame.rs`, `icons.rs`, `update.rs`, `mockups.rs` | `render/` | moved |
| `freebox.rs` | `adapters/freebox.rs` | adds `devices_seen` (Host → DeviceSeen) |
| `freebox_client.rs` | `adapters/freebox_client.rs` | moved |
| `prim.rs` + `departures::parse` + `bus::read_answer` | `adapters/prim.rs` | the PRIM format in one place |
| `prim_client.rs` | `adapters/prim_client.rs` | moved |
| `devices.rs` (file) | `adapters/mapping_file.rs` | `parse_mapping`, `draft`, `write_new`, `summary` |
| `saved.rs` (I/O) | `adapters/state_file.rs` | `load`, `save` |
| `usage.rs` + `main::append_usage` | `adapters/usage_log.rs` | the CSV format and the file |
| `display.rs` | `adapters/display.rs` | moved (`Display` trait, PNG and panel) |
| `diagnose.rs` | `adapters/diagnose.rs` | moved (reads Freebox `Host`s on purpose) |
| `epd/` | `adapters/epd/` | moved |
| `main::run_cycle`, `poll_prim` | `app/run.rs` | split into `compose` (pure, tested) and `run` (I/O) |
| `main::Output`, `read_mapping`, `read_token`, `read_prim_key` | `app/mod.rs` | shared by the commands |
| other `main.rs` commands | `app/tools.rs` | `authorize`, `devices`, `check-*`, `bus-log`, `test-pattern`, `partial-test`, `clear` |
| `main.rs` | `main.rs` | clap and dispatch only |

Moving a file one folder deeper changes its relative paths:
`include_bytes!("../assets/…")` and `include_str!("../tests/fixtures/…")`
become `"../../assets/…"` and `"../../tests/fixtures/…"`.

## 3. Key types

`DeviceSeen` (domain): what the core needs from a device the box knows.

```rust
/// A device the box knows, as the core needs it.
pub struct DeviceSeen {
    /// Lowercase MAC address.
    pub mac: String,
    /// Unix timestamp in seconds.
    pub last_activity: Option<i64>,
}
```

The download counters stay a `BTreeMap<String, u64>` by MAC: already
format-free.

`compose` (app): everything `e-paper run` decides, once the Freebox and PRIM
data are in hand. The shell (`run`) reads the files and the clock, polls,
calls `compose`, then shows the frame and writes `frame.png`, `state.json`
and the usage log.

```rust
pub struct Inputs<'a> {
    pub now: Timestamp,
    pub presence: CycleInput<'a>,
    /// The bus state of this run: polled now, or carried over.
    pub bus: BusState,
    pub previous_frame: Option<&'a Frame>,
    pub force: bool,
    pub partial_enabled: bool,
}

pub struct Outcome {
    pub frame: Frame,
    pub update: UpdateKind,
    /// To save once the display is updated; `last_full_refresh` already set
    /// if `update` is a full refresh.
    pub saved: SavedState,
    /// What the run prints: the people, then the two blocks.
    pub summary: Vec<String>,
}

pub fn compose(inputs: &Inputs) -> Outcome
```

## 4. Tests

**A. Freebox translation** (`adapters/freebox.rs`)

1. `devices_seen_keep_mac_and_last_activity`: the hosts of the
   `lan_browser_pub.json` fixture give one `DeviceSeen` each, same order,
   same MAC and last activity (a host without one gives `None`). Checks that
   the core gets what presence needs, nothing more.

**B. Run decisions** (`app/run.rs`, `compose`)

2. `first_run_is_a_full_refresh_and_records_it`: no saved state, no
   previous frame: full refresh, `saved.last_full_refresh` is now,
   `saved.bus` is the bus state given.
3. `same_screen_leaves_the_display_alone`: same inputs as the previous run
   (same minute, frame of that run): no update, `last_full_refresh`
   unchanged.
4. `clock_change_is_a_partial_refresh`: previous run at 8:04, now 8:05,
   same hour: partial refresh, `last_full_refresh` unchanged (it is only set
   by a full refresh).
5. `first_cycle_of_the_hour_is_a_full_refresh`: previous run at 8:59 (a
   full refresh, the first one), now 9:00: full refresh, recorded.
6. `force_and_full_only_are_passed_on`: `force` with an unchanged screen
   gives a full refresh; `partial_enabled = false` with a clock change gives
   a full refresh.
7. `summary_describes_each_person_and_block`: the lines printed by the run
   today, unchanged (`R home, M away, H home (active), L away`, then
   `Rungis: 8:12 8:31; Antony: service over (moon)`, at 8:04).

Tests 2–6 check the wiring between the pieces (`update::plan_display`,
`hourly_full_due`, the saved state), not the pieces themselves: those
already have tests.

**C. Architecture** (`tests/architecture.rs`, reads `src/` as text)

8. `domain_uses_nothing_outside_it`: no file of `src/domain/` mentions
   `crate::render`, `crate::adapters`, `crate::app`, `std::fs`, `ureq`,
   `serde_json`, `toml::`, `embedded_graphics` or `fontdue` before its
   `#[cfg(test)]` line.
9. `render_uses_only_the_domain`: same for `src/render/` with
   `crate::adapters`, `crate::app`, `std::fs`, `ureq`.
10. `adapters_do_not_use_the_app`: same for `src/adapters/` with
    `crate::app`.

Written right after the files are moved (step 2), test 8 fails and lists
the faulty imports: steps 3–5 make it pass. Tests 9 and 10 pass from the
start: they guard the rule from then on.

**Existing tests**

- Moved with their code, unchanged except their `use` lines. Those of
  `parse` go to `adapters/prim.rs`, `http_error_is_a_failure` (bus) too;
  those of `load`/`save` to `adapters/state_file.rs`; those of the mapping
  file to `adapters/mapping_file.rs`.
- Rewritten: the `cycle` tests and the `usage` tests build `DeviceSeen`
  instead of `Host` (test helper only).
- Removed: the 4 `refresh` tests, with `needs_refresh`.

## 5. Steps

Each step ends with `cargo test` and `cargo clippy --all-targets` green, and
a commit. Exception: from step 2 to step 4, the architecture test fails on
purpose, listing what remains. The work can stop after any step; the branch
is merged at the end.

| Step | Content | Tests | Estimate |
|---|---|---|---|
| 1 | Branch `refactor/architecture`; folders `domain/`, `render/`, `adapters/` with their `mod.rs`; `git mv` of whole files, `use` paths fixed (compiler-driven), `include_*!` paths fixed | existing, green | 1 h |
| 2 | `tests/architecture.rs` | C (8–10), 8 red | 20 min |
| 3 | Types to their layer: `PEOPLE` and `HouseholdState` to `household`, `refresh.rs` removed, `Time`/`Block`/`Board` to `screen`, `Assignment`/`Mapping` to `domain/devices`, `SavedState`/`Snapshot` to `domain/saved` | 4 removed | 1 h |
| 4 | PRIM format out of the domain: `parse`, the SIRI structs and `read_answer` to `adapters/prim.rs` | moved | 45 min |
| 5 | `DeviceSeen` and `devices_seen`; `CycleInput` takes it | A (1), cycle and usage tests rewritten; C green | 45 min |
| 6 | `app/`: `run.rs` (`compose` and `run`), `tools.rs`; `main.rs` reduced to clap | B (2–7) | 1 h 30 |
| 7 | Checks: golden images, `cargo run -- run` on the dev PC (Freebox and PRIM, PNG output), `make build`; merge, `make deploy`, one cycle watched with `make logs` | — | 30 min |

Evening 1: steps 1–5. Evening 2: steps 6–7.

## 6. Step 1: layer folders, files moved

No code change: whole files move, then their paths are fixed.

```bash
git switch -c refactor/architecture
```

```bash
mkdir src/domain src/render src/adapters
```

The domain (`refresh.rs` and `saved.rs` go there for now, step 3 sorts
them out):

```bash
git mv src/state.rs src/domain/household.rs
```

```bash
git mv src/presence.rs src/rate.rs src/cycle.rs src/saved.rs src/departures.rs src/bus.rs src/refresh.rs src/domain/
```

The render:

```bash
git mv src/render.rs src/render/draw.rs
```

```bash
git mv src/board.rs src/layout.rs src/frame.rs src/icons.rs src/update.rs src/mockups.rs src/render/
```

The adapters (`devices.rs` goes there for now, step 3 takes its types out):

```bash
git mv src/devices.rs src/adapters/mapping_file.rs
```

```bash
git mv src/usage.rs src/adapters/usage_log.rs
```

```bash
git mv src/freebox.rs src/freebox_client.rs src/prim.rs src/prim_client.rs src/display.rs src/diagnose.rs src/epd src/adapters/
```

Replace the content of `src/lib.rs` with:

```rust
//! Household e-paper dashboard: functional core (`domain`, `render`),
//! imperative shell (`adapters`, `app`). See docs/guide-15-architecture.md.

pub mod adapters;
pub mod domain;
pub mod render;
```

Create the file `src/domain/mod.rs` with this content:

```rust
//! The rules: presence, activity, departures. Pure: no I/O, no external
//! format, no pixel.

pub mod bus;
pub mod cycle;
pub mod departures;
pub mod household;
pub mod presence;
pub mod rate;
pub mod refresh;
pub mod saved;
```

Create the file `src/render/mod.rs` with this content:

```rust
//! What the screen looks like, down to the pixel. Pure.

pub mod board;
pub mod draw;
pub mod frame;
pub mod icons;
pub mod layout;
pub mod mockups;
pub mod update;
```

Create the file `src/adapters/mod.rs` with this content:

```rust
//! The outside world: Freebox, PRIM, files, the panel, and their formats.

pub mod diagnose;
pub mod display;
pub mod epd;
pub mod freebox;
pub mod freebox_client;
pub mod mapping_file;
pub mod prim;
pub mod prim_client;
pub mod usage_log;
```

Then fix every path at once. This `sed` rewrites `crate::x` (and
`e_paper::x` in `main.rs`, the tests and the example) to the new module
path, and adds one `../` to the `include_str!`/`include_bytes!` paths of the
moved files. `render` comes first, so that it is not rewritten twice:

```bash
sed -i -E -e 's/\b(crate|e_paper)::render\b/\1::render::draw/g' -e 's/\b(crate|e_paper)::state\b/\1::domain::household/g' -e 's/\b(crate|e_paper)::(presence|rate|cycle|saved|departures|bus|refresh)\b/\1::domain::\2/g' -e 's/\b(crate|e_paper)::(board|layout|frame|icons|update|mockups)\b/\1::render::\2/g' -e 's/\b(crate|e_paper)::(freebox|freebox_client|prim|prim_client|display|diagnose|epd)\b/\1::adapters::\2/g' -e 's/\b(crate|e_paper)::devices\b/\1::adapters::mapping_file/g' -e 's/\b(crate|e_paper)::usage\b/\1::adapters::usage_log/g' -e 's#include_(str|bytes)!\("\.\./#include_\1!("../../#g' $(git ls-files '*.rs')
```

`main.rs` also calls `devices::…` and `usage::…` through their module name:

```bash
sed -i -E -e 's/([^_:a-z])devices::/\1mapping_file::/g' -e 's/([^_:a-z])usage::/\1usage_log::/g' src/main.rs
```

```bash
cargo fmt
```

```bash
cargo test
```

Expected: 187 library tests and the 2 golden tests pass, as before.

```bash
cargo clippy --all-targets
```

```bash
git add -A
```

```bash
git commit -m "refactor: layer folders (domain, render, adapters)"
```

## 7. Step 2: the dependency rule (tests 8–10)

### Red

Create the file `tests/architecture.rs` with this content:

```rust
//! Dependency rule: each layer only uses the layers below it (see
//! docs/guide-15-architecture.md). Checked on the source text, before each
//! file's tests: tests may cross layers.

use std::fs;
use std::path::Path;

/// Every `.rs` file under `dir`, with its code before `#[cfg(test)]`.
fn sources(dir: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(sources(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let text = fs::read_to_string(&path).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap().to_string();
            files.push((path.display().to_string(), code));
        }
    }
    files
}

/// The files of `layer` that mention one of the forbidden names, with it.
fn violations(layer: &str, forbidden: &[&str]) -> Vec<String> {
    sources(&Path::new("src").join(layer))
        .into_iter()
        .flat_map(|(path, code)| {
            forbidden
                .iter()
                .filter(|name| code.contains(*name))
                .map(|name| format!("{path}: {name}"))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn domain_uses_nothing_outside_it() {
    let found = violations(
        "domain",
        &[
            "crate::render",
            "crate::adapters",
            "crate::app",
            "std::fs",
            "ureq",
            "serde_json",
            "toml::",
            "embedded_graphics",
            "fontdue",
        ],
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn render_uses_only_the_domain() {
    let found = violations(
        "render",
        &["crate::adapters", "crate::app", "std::fs", "ureq"],
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn adapters_do_not_use_the_app() {
    let found = violations("adapters", &["crate::app"]);
    assert!(found.is_empty(), "{found:#?}");
}
```

```bash
cargo test --test architecture
```

Expected: `domain_uses_nothing_outside_it` fails and lists:

```text
"src/domain/departures.rs: crate::render",
"src/domain/departures.rs: serde_json",
"src/domain/saved.rs: std::fs",
"src/domain/saved.rs: serde_json",
"src/domain/cycle.rs: crate::render",
"src/domain/cycle.rs: crate::adapters",
"src/domain/bus.rs: crate::render",
```

The two other tests pass. Steps 3–5 clear the list.

```bash
git add -A
```

```bash
git commit -m "test: dependency rule between layers (fails until step 5)"
```

## 8. Step 3: core types in the domain

Each part below ends compiling; the existing tests guard it.

### 3a. `PEOPLE` and `HouseholdState` in `household`

In `src/domain/household.rs`, replace the `use serde…` line with:

```rust
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
```

before `/// What the display shows for one person.`, add:

```rust
/// Display order: left to right in phase 1, top to bottom in phase 2.
pub const PEOPLE: [Person; 4] = [Person('R'), Person('M'), Person('H'), Person('L')];

```

and before `/// Activity mark threshold: 5 Mb/s.`, add:

```rust
/// State of the whole household, as shown on the display.
pub type HouseholdState = BTreeMap<Person, PersonState>;

```

In `src/render/layout.rs`, delete `PEOPLE` and its comment
(`/// Display order, left to right.`), and import it instead: the line
`use crate::domain::household::{Person, PersonState};` becomes

```rust
use crate::domain::household::{PEOPLE, Person, PersonState};
```

`refresh.rs` only held `HouseholdState` and the unused `needs_refresh`:

```bash
git rm src/domain/refresh.rs
```

In `src/domain/mod.rs`, delete the line `pub mod refresh;`. Then:

```bash
sed -i 's/domain::refresh::HouseholdState/domain::household::HouseholdState/' $(git ls-files '*.rs')
```

```bash
sed -i 's/crate::render::layout::PEOPLE;/crate::domain::household::PEOPLE;/' $(git ls-files 'src/*.rs')
```

Three imports remain, with `PEOPLE` among other names; replace them:

- `src/render/board.rs`: `use crate::render::layout::{Ink, PEOPLE, Slot, SlotSizes, slot};` becomes

  ```rust
  use crate::domain::household::PEOPLE;
  use crate::render::layout::{Ink, Slot, SlotSizes, slot};
  ```

- `src/render/draw.rs`, in the tests: `use crate::render::layout::{PEOPLE, layout};` becomes

  ```rust
      use crate::domain::household::PEOPLE;
      use crate::render::layout::layout;
  ```

- `src/main.rs`: `use e_paper::render::layout::{PEOPLE, layout};` becomes

  ```rust
  use e_paper::domain::household::PEOPLE;
  use e_paper::render::layout::layout;
  ```

```bash
cargo build --all-targets
```

### 3b. `Time`, `Block`, `Board` in `domain/screen.rs`

Create the file `src/domain/screen.rs` with this content (the three types
are cut from `src/render/board.rs`, unchanged):

```rust
//! What the screen says, without pixels: the clock and what each direction
//! block shows. `render::board` decides where it goes.

use std::fmt;

/// A time of day, local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
}

impl fmt::Display for Time {
    /// "8:04": no leading zero on the hour, two digits for the minutes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{:02}", self.hour, self.minute)
    }
}

/// What a direction block shows below its header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// The next departures, already chosen and sorted; only the first two
    /// are shown, a missing one as a dash.
    Times(Vec<Time>),
    /// The service is over, or not started yet.
    Moon,
    /// The bus data is unavailable.
    Wrench,
}

/// What the bus part shows: the clock and each direction's block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub clock: Time,
    pub rungis: Block,
    pub antony: Block,
}
```

In `src/render/board.rs`, delete the three types (from `/// A time of
day, local time.` to the end of `Board`) and the line `use std::fmt;`, and
add after `use crate::domain::household::PEOPLE;`:

```rust
use crate::domain::screen::{Block, Board, Time};
```

In `src/domain/mod.rs`, add `pub mod screen;` after `pub mod saved;`.

Then point the imports to the new module:

| File | Before | After |
|---|---|---|
| `src/domain/bus.rs` | `use crate::render::board::Board;` | `use crate::domain::screen::Board;` |
| `src/domain/bus.rs` (tests) | `use crate::render::board::{Block, Time};` | `use crate::domain::screen::{Block, Time};` |
| `src/domain/departures.rs` | `use crate::render::board::{Block, Time};` | `use crate::domain::screen::{Block, Time};` |
| `src/render/mockups.rs` | `use crate::render::board::{Block, Board, Time};` | `use crate::domain::screen::{Block, Board, Time};` |
| `src/render/update.rs` (tests) and `src/render/draw.rs` (tests) | `use crate::render::board::{Block, Board, Time, board_layout};` | `use crate::domain::screen::{Block, Board, Time};` and `use crate::render::board::board_layout;` |
| `src/main.rs` | `use e_paper::render::board::{Block, Time, board_layout};` | `use e_paper::domain::screen::{Block, Time};` and `use e_paper::render::board::board_layout;` |

```bash
cargo build --all-targets
```

### 3c. `Assignment` and `Mapping` in `domain/devices.rs`

Create the file `src/domain/devices.rs` with this content (the two types
are cut from `src/adapters/mapping_file.rs`, unchanged):

```rust
//! The household's devices, as the core sees them.

use std::collections::BTreeMap;

use crate::domain::household::Person;

/// The person a device belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub person: Person,
    /// The device name, for logs (never the MAC address).
    pub name: String,
    /// The phone used for presence.
    pub smartphone: bool,
}

/// Assigned devices by lowercase MAC address. Shared devices are not in it.
pub type Mapping = BTreeMap<String, Assignment>;
```

In `src/adapters/mapping_file.rs`, delete `Assignment` and `Mapping` (from
`/// The person a device belongs to.` to `pub type Mapping…`), replace
`use std::collections::{BTreeMap, BTreeSet};` with
`use std::collections::BTreeSet;`, and add before
`use crate::domain::household::Person;`:

```rust
use crate::domain::devices::{Assignment, Mapping};
```

In `src/domain/mod.rs`, add `pub mod devices;` after `pub mod departures;`.

| File | Before | After |
|---|---|---|
| `src/domain/cycle.rs` | `use crate::adapters::mapping_file::Mapping;` | `use crate::domain::devices::Mapping;` |
| `src/domain/cycle.rs` (tests) | `use crate::adapters::mapping_file::Assignment;` | `use crate::domain::devices::Assignment;` |
| `src/adapters/diagnose.rs` | `use crate::adapters::mapping_file::Mapping;` | `use crate::domain::devices::Mapping;` |
| `src/adapters/diagnose.rs` (tests) | `use crate::adapters::mapping_file::Assignment;` | `use crate::domain::devices::Assignment;` |
| `src/adapters/usage_log.rs` (tests) | `use crate::adapters::mapping_file::{Assignment, Mapping};` | `use crate::domain::devices::{Assignment, Mapping};` |

In `src/main.rs`, add `use e_paper::domain::devices::Mapping;` after the
`cycle` import, and in `read_mapping` replace
`Result<mapping_file::Mapping,` with `Result<Mapping,`.

```bash
cargo build --all-targets
```

### 3d. `SavedState` in the domain, its file in the adapters

The file reading and writing, and all the tests, become
`adapters/state_file.rs`; the two types stay in the domain:

```bash
git mv src/domain/saved.rs src/adapters/state_file.rs
```

Create the file `src/domain/saved.rs` with this content (the two types,
unchanged):

```rust
//! State kept between two runs. `adapters::state_file` reads and writes it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::bus::BusState;
use crate::domain::household::HouseholdState;

/// What a run leaves for the next one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedState {
    /// When the Freebox was polled, Unix timestamp in seconds.
    pub polled_at: i64,
    /// What the display shows.
    pub household: HouseholdState,
    /// Download counter (`tx_bytes`) of each assigned device seen, by MAC.
    pub counters: BTreeMap<String, u64>,
    /// Last full refresh of the display, Unix timestamp in seconds. Absent
    /// from state files written before it existed.
    #[serde(default)]
    pub last_full_refresh: Option<i64>,
    /// Earlier readings of the counters, still within the rate window, oldest
    /// first (rates are averaged over that window).
    #[serde(default)]
    pub earlier: Vec<Snapshot>,
    /// Last PRIM answers and failed polls (phase 2). Absent from state files
    /// written before it existed.
    #[serde(default)]
    pub bus: Option<BusState>,
}

/// The download counters of the assigned devices at one poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unix timestamp in seconds.
    pub polled_at: i64,
    /// `tx_bytes` by MAC.
    pub counters: BTreeMap<String, u64>,
}
```

In `src/adapters/state_file.rs`, delete `SavedState` and `Snapshot` (from
`/// What a run leaves for the next one.` to the end of `Snapshot`), and
replace everything above `/// Reads the saved state.` with:

```rust
//! The state kept between two runs, in a small JSON file.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

use crate::domain::saved::SavedState;
```

In its tests, replace `use crate::domain::bus::Pole;` with
`use crate::domain::bus::{BusState, Pole};`, and add after the last `use`:

```rust
    use crate::domain::saved::Snapshot;
```

In `src/adapters/mod.rs`, add `pub mod state_file;` after
`pub mod prim_client;`. In `src/main.rs`:

```bash
sed -i -e 's/^use e_paper::domain::saved;/use e_paper::adapters::state_file;/' -e 's/\bsaved::load(/state_file::load(/' -e 's/\bsaved::save(/state_file::save(/' src/main.rs
```

```bash
cargo fmt
```

```bash
cargo test
```

Expected: 183 library tests pass (the 4 `refresh` tests are gone), the
golden tests pass; the architecture test still lists
`departures.rs: serde_json` and `cycle.rs: crate::adapters`.

```bash
git add -A
```

```bash
git commit -m "refactor: core types in the domain"
```

## 9. Step 4: the PRIM format in the adapters

`departures` keeps the rules (selection, next departures, Paris time);
`adapters/prim.rs` gets the SIRI Lite answer format.

In `src/domain/departures.rs`:

- replace the first two lines of the module comment with:

  ```rust
  //! Phase 2 departures: from the departures announced at a pole to the next
  //! catchable ones of its direction. `adapters::prim` reads the PRIM answers.
  ```

- **cut** the SIRI structs, from `// The few SIRI Lite fields we read.` to
  the end of `struct ServiceException`, and the function `parse` with its
  comment (`/// Reads the answer of the pole serving…`); keep them for
  `prim.rs`;
- make `selected` and `serves` public, with a comment:

  ```rust
  /// Whether a departure of `line` towards `destination` belongs to the
  /// direction.
  pub fn selected(direction: Direction, line: &str, destination: &str) -> bool {
  ```

  ```rust
  /// Whether `line` serves the direction (its service status counts).
  pub fn serves(direction: Direction, line: &str) -> bool {
  ```

- in the tests, add after `use super::*;`:

  ```rust
      use crate::adapters::prim::parse;
  ```

In `src/domain/bus.rs`:

- **cut** `read_answer` with its comment (`/// The answer of a pole, or why
  there is none.`), and the test `http_error_is_a_failure`;
- the departures import becomes
  `use crate::domain::departures::{Answer, block, in_paris, paris_time};`;
- in the tests, add after `use super::*;`:

  ```rust
      use crate::adapters::prim::parse;
  ```

  and add `Direction` to the `crate::domain::departures` import of the tests:
  `use crate::domain::departures::{Departure, Direction, LINE_297, LINE_319, ServiceStatus};`.

In `src/adapters/prim.rs`:

- replace everything above `/// SIRI Lite stop monitoring` with:

  ```rust
  //! Île-de-France Mobilités PRIM API: requests, answers and log lines,
  //! without any network access.
  //!
  //! The HTTP calls live in `prim_client`. See CLAUDE.md, "Phase 2".

  use jiff::Timestamp;
  use serde::Deserialize;
  use serde_json::{Value, json};

  use crate::domain::departures::{Answer, Departure, Direction, ServiceStatus, selected, serves};
  ```

- before `#[cfg(test)]`, paste what was cut, in this order: the SIRI
  structs, `read_answer`, `parse`. The result:

  ```rust
  // The few SIRI Lite fields we read. SIRI wraps every value in
  // `{"value": …}`.

  #[derive(Deserialize)]
  struct Wrapped<T> {
      value: T,
  }

  #[derive(Deserialize)]
  #[serde(rename_all = "PascalCase")]
  struct Body {
      siri: Siri,
  }

  #[derive(Deserialize)]
  #[serde(rename_all = "PascalCase")]
  struct Siri {
      service_delivery: ServiceDelivery,
  }

  #[derive(Deserialize)]
  #[serde(rename_all = "PascalCase")]
  struct ServiceDelivery {
      stop_monitoring_delivery: Vec<StopMonitoringDelivery>,
  }

  #[derive(Deserialize)]
  #[serde(rename_all = "PascalCase")]
  struct StopMonitoringDelivery {
      #[serde(default)]
      monitored_stop_visit: Vec<Visit>,
      #[serde(default)]
      service_exception: Vec<ServiceException>,
  }

  #[derive(Deserialize)]
  #[serde(rename_all = "PascalCase")]
  struct Visit {
      monitored_vehicle_journey: Journey,
  }

  #[derive(Deserialize)]
  #[serde(rename_all = "PascalCase")]
  struct Journey {
      line_ref: Wrapped<String>,
      destination_ref: Option<Wrapped<String>>,
      monitored_call: Call,
  }

  #[derive(Deserialize)]
  #[serde(rename_all = "PascalCase")]
  struct Call {
      expected_departure_time: Option<String>,
      aimed_departure_time: Option<String>,
  }

  #[derive(Deserialize)]
  #[serde(rename_all = "PascalCase")]
  struct ServiceException {
      line_ref: Wrapped<String>,
      service_status: Wrapped<String>,
  }

  /// The answer of a pole, or why there is none.
  pub fn read_answer(status: u16, body: &str, direction: Direction) -> Result<Answer, String> {
      if status != 200 {
          return Err(format!("HTTP {status}"));
      }
      parse(body, direction)
  }

  /// Reads the answer of the pole serving `direction`: the selected
  /// departures, and the service status of the direction's lines.
  pub fn parse(body: &str, direction: Direction) -> Result<Answer, String> {
      let body: Body = serde_json::from_str(body).map_err(|e| format!("unexpected answer: {e}"))?;
      let mut answer = Answer::default();
      for delivery in body.siri.service_delivery.stop_monitoring_delivery {
          for visit in delivery.monitored_stop_visit {
              let journey = visit.monitored_vehicle_journey;
              let Some(destination) = journey.destination_ref else {
                  continue;
              };
              if !selected(direction, &journey.line_ref.value, &destination.value) {
                  continue;
              }
              let call = journey.monitored_call;
              let time = call.expected_departure_time.or(call.aimed_departure_time);
              let Some(at) = time.and_then(|t| t.parse::<Timestamp>().ok()) else {
                  continue;
              };
              answer.departures.push(Departure {
                  line: journey.line_ref.value,
                  at,
              });
          }
          for exception in delivery.service_exception {
              let status = match exception.service_status.value.as_str() {
                  "afterLastJourney" => ServiceStatus::AfterLastJourney,
                  "beforeFirstJourney" => ServiceStatus::BeforeFirstJourney,
                  _ => continue,
              };
              if serves(direction, &exception.line_ref.value) {
                  answer.statuses.push((exception.line_ref.value, status));
              }
          }
      }
      Ok(answer)
  }
  ```

- in the tests, after `use super::*;`, add:

  ```rust

      const RUNGIS_EVENING: &str =
          include_str!("../../tests/fixtures/prim/rungis-20261013-2040.json");
  ```

  and at the end of the tests, before their closing `}`, paste the cut test:

  ```rust

      // C. Answers

      #[test]
      fn http_error_is_a_failure() {
          assert!(read_answer(503, RUNGIS_EVENING, Direction::Rungis).is_err());
          assert!(read_answer(200, RUNGIS_EVENING, Direction::Rungis).is_ok());
      }
  ```

In `src/main.rs`, `poll_prim` calls `prim::read_answer(` instead of
`bus::read_answer(`.

```bash
cargo fmt
```

```bash
cargo test
```

Expected: 183 library tests pass; the architecture test only lists
`cycle.rs: crate::adapters`.

```bash
git add -A
```

```bash
git commit -m "refactor: PRIM answer format in the adapters"
```

## 10. Step 5: `DeviceSeen` (test 1)

### Red

In `src/adapters/freebox.rs`, in the tests, before
`fn host_access_point_is_parsed`, add:

```rust
    #[test]
    fn devices_seen_keep_mac_and_last_activity() {
        let hosts = parse_hosts(LAN_BROWSER).unwrap();
        let seen = devices_seen(&hosts);
        assert_eq!(seen.len(), hosts.len());
        let find = |mac: &str| seen.iter().find(|device| device.mac == mac).unwrap();
        assert_eq!(find("02:00:00:00:00:02").last_activity, Some(1_760_000_000));
        assert_eq!(find("02:00:00:00:00:04").last_activity, None);
    }
```

```bash
cargo test --lib freebox
```

Expected: does not compile, `devices_seen` is not found.

### Green

At the end of `src/domain/devices.rs`, add:

```rust
/// A device the box knows, as the core needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSeen {
    /// Lowercase MAC address.
    pub mac: String,
    /// Unix timestamp in seconds.
    pub last_activity: Option<i64>,
}
```

In `src/adapters/freebox.rs`, after `use sha1::Sha1;`, add:

```rust

use crate::domain::devices::DeviceSeen;
```

and before `/// Identifiers of the Wi-Fi access points`, add:

```rust
/// The hosts as the core needs them: MAC and last activity only.
pub fn devices_seen(hosts: &[Host]) -> Vec<DeviceSeen> {
    hosts
        .iter()
        .map(|host| DeviceSeen {
            mac: host.mac.clone(),
            last_activity: host.last_activity,
        })
        .collect()
}
```

```bash
cargo test --lib freebox
```

Expected: 22 tests pass.

### The cycle takes `DeviceSeen`

First rename the test helper's calls in `src/domain/cycle.rs`:

```bash
sed -i -e 's/\[host(/[seen(/g' -e 's/, host(/, seen(/g' src/domain/cycle.rs
```

Then, in `src/domain/cycle.rs`:

- the imports `use crate::adapters::freebox::Host;` and
  `use crate::domain::devices::Mapping;` become
  `use crate::domain::devices::{DeviceSeen, Mapping};`;
- in `CycleInput`, the `hosts` field becomes:

  ```rust
      /// Devices known to the box, for the last activity.
      pub seen: &'a [DeviceSeen],
  ```

- in `smartphone_activity`, the comment ends with "known to the box", and
  the lookup becomes:

  ```rust
      input
          .seen
          .iter()
          .find(|device| &device.mac == mac)?
          .last_activity
  ```

- in the tests, add after `use super::*;`:

  ```rust
      use crate::adapters::freebox::{Host, HostAccessPoint, devices_seen, download_counters};
  ```

- the helper `host` becomes:

  ```rust
      fn seen(mac: &str, last_activity: i64) -> DeviceSeen {
          DeviceSeen {
              mac: mac.into(),
              last_activity: Some(last_activity),
          }
      }
  ```

- in the helper `run`, `hosts: &[Host],` becomes `seen: &[DeviceSeen],` and
  `hosts,` becomes `seen,`;
- in `repeater_device_gets_a_rate_in_the_cycle`, the console still needs a
  Freebox `Host` (its counter comes from `access_point`): replace the lines
  from `let mut console = …` to `let counters = …` with:

  ```rust
          let host = |mac: &str, access_point| Host {
              mac: mac.into(),
              name: String::new(),
              vendor: String::new(),
              active: true,
              last_activity: Some(NOW - 60),
              access_point,
          };
          let console = HostAccessPoint {
              kind: Some("repeater".into()),
              connectivity: Some("wifi".into()),
              tx_bytes: Some(SIX_MBPS),
              fields: Vec::new(),
          };
          let hosts = [host(H_PHONE, None), host(H_CONSOLE, Some(console))];
          let counters = download_counters(BTreeMap::new(), &hosts);
  ```

  and `hosts: &hosts,` becomes `seen: &devices_seen(&hosts),`.

In `src/adapters/usage_log.rs`, in the tests, `hosts: &[],` becomes
`seen: &[],`. In `src/main.rs`, in `run_cycle`, `hosts: &hosts,` becomes:

```rust
        seen: &freebox::devices_seen(&hosts),
```

```bash
cargo fmt
```

```bash
cargo test
```

Expected: 184 library tests and the 3 architecture tests pass: the domain
no longer depends on anything outside it.

```bash
cargo clippy --all-targets
```

```bash
git add -A
```

```bash
git commit -m "refactor: the core sees DeviceSeen, not Freebox hosts"
```

## 11. Step 6: the `app` layer (tests 2–7)

### Preparation (no new behaviour)

At the end of `src/adapters/usage_log.rs`, before `/// A CSV field,
quoted…`, add the former `main::append_usage`, renamed:

```rust
/// Appends lines to the usage log, with the header first if the file is new
/// or empty.
pub fn append(path: &Path, lines: &[String]) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut text = String::new();
    if file.metadata()?.len() == 0 {
        text.push_str(HEADER);
        text.push('\n');
    }
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    file.write_all(text.as_bytes())
}
```

and at the top, before `use crate::domain::cycle::CycleInput;`:

```rust
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

```

In `src/lib.rs`, add `pub mod app;` after `pub mod adapters;`.

Create the file `src/app/mod.rs` with this content (`Output` and the three
`read_*` functions come from `main.rs`):

```rust
//! The commands: each one reads, calls the core, then shows or writes.
//! `main.rs` only parses the command line.

pub mod run;
pub mod tools;

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::display::{Display, PngDisplay};
use crate::adapters::{epd, mapping_file, prim};
use crate::domain::devices::Mapping;

/// Where to show the display.
#[derive(clap::Args)]
pub struct Output {
    /// Draw on the e-paper panel (on the Raspberry Pi) instead of a PNG file.
    #[arg(long)]
    pub epd: bool,
    /// The PNG file, when not drawing on the panel.
    #[arg(long, default_value = "target/e-paper.png")]
    pub png: PathBuf,
}

impl Output {
    pub fn open(&self) -> Result<Box<dyn Display>, Box<dyn Error>> {
        if self.epd {
            Ok(Box::new(epd::linux::open()?))
        } else {
            Ok(Box::new(PngDisplay {
                path: self.png.clone(),
            }))
        }
    }

    pub fn describe(&self) -> String {
        if self.epd {
            "e-paper panel".into()
        } else {
            self.png.display().to_string()
        }
    }
}

fn read_mapping(path: &Path) -> Result<Mapping, Box<dyn Error>> {
    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run `e-paper devices` first)",
            path.display()
        )
    })?;
    Ok(mapping_file::parse_mapping(&text)?)
}

fn read_prim_key(path: &Path) -> Result<String, Box<dyn Error>> {
    let key = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(prim::api_key(&key)?)
}

fn read_token(path: &Path) -> Result<String, Box<dyn Error>> {
    let token = fs::read_to_string(path).map_err(|error| {
        format!(
            "cannot read {}: {error} (run `e-paper authorize` first)",
            path.display()
        )
    })?;
    Ok(token.trim().to_string())
}
```

Create the file `src/app/tools.rs` with this content (the commands come from
`main.rs`, made public, with the comment of their subcommand):

```rust
//! The setup and diagnostic commands: everything but `run`.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::adapters::display::Update;
use crate::adapters::freebox_client::Client;
use crate::adapters::prim::{self, Stop};
use crate::adapters::{diagnose, epd, mapping_file, prim_client};
use crate::app::{Output, read_mapping, read_prim_key, read_token};
use crate::domain::household::PersonState::{Away, HomeActive, HomePassive};
use crate::domain::household::{HouseholdState, PEOPLE, PersonState};
use crate::render::draw::{atkinson_bold, render};
use crate::render::frame::Frame;
use crate::render::layout::layout;
use crate::render::mockups::phase1_states;
use crate::render::update::changed_window;

/// One-time authorization on the Freebox: confirm on its screen.
pub fn authorize(token_path: &Path) -> Result<(), Box<dyn Error>> {
    if token_path.exists() {
        return Err(format!(
            "{} already exists: delete it to authorize again",
            token_path.display()
        )
        .into());
    }
    let client = Client::connect()?;
    let app_token = client.authorize(&device_name())?;
    save_secret(token_path, &app_token)?;
    println!("Authorized. App token saved in {}", token_path.display());
    Ok(())
}

/// Writes the devices known to the Freebox into a draft mapping file.
pub fn list_devices(token_path: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let app_token = read_token(token_path)?;
    let mut client = Client::connect()?;
    client.login(&app_token)?;
    let hosts = client.lan_hosts()?;
    mapping_file::write_new(output, &mapping_file::draft(&hosts)).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            format!("{} already exists: not overwritten", output.display())
        } else {
            format!("cannot write {}: {error}", output.display())
        }
    })?;
    println!("{} devices written to {}", hosts.len(), output.display());
    Ok(())
}

/// Checks the mapping file and prints a summary per person.
pub fn check_devices(path: &Path) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(path)?;
    for line in mapping_file::summary(&mapping) {
        println!("{line}");
    }
    Ok(())
}

/// Shows where each assigned device's download counter comes from.
pub fn check_counters(token_path: &Path, devices_path: &Path) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(devices_path)?;
    let app_token = read_token(token_path)?;
    let mut client = Client::connect()?;
    client.login(&app_token)?;
    let hosts = client.lan_hosts()?;
    let counters = client.wifi_counters()?;
    for line in diagnose::counters_report(&mapping, &hosts, &counters) {
        println!("{line}");
    }
    let fields = diagnose::access_point_fields(&hosts);
    println!("access_point fields: {}", fields.join(", "));
    Ok(())
}

/// Shows the phase 1 mockup with an activity ring (legibility test).
pub fn test_pattern(output: &Output) -> Result<(), Box<dyn Error>> {
    let (_, state) = phase1_states()
        .into_iter()
        .find(|(name, _)| *name == "phase1-activity-ring")
        .expect("mockup exists");
    let mut frame = Frame::new();
    render(&mut frame, &layout(&state), &atkinson_bold())?;
    output.open()?.show(&frame, Update::Full)?;
    println!("Test pattern shown: {}", output.describe());
    Ok(())
}

/// Checks partial refresh on the panel: one full refresh, then a few
/// partial ones, 5 s apart.
pub fn partial_test(output: &Output) -> Result<(), Box<dyn Error>> {
    let household =
        |states: [PersonState; 4]| -> HouseholdState { PEOPLE.into_iter().zip(states).collect() };
    let steps = [
        ("R home", household([HomePassive, Away, Away, Away])),
        (
            "H home (active)",
            household([HomePassive, Away, HomeActive, Away]),
        ),
        (
            "R away, L home",
            household([Away, Away, HomeActive, HomePassive]),
        ),
        ("everybody away", household([Away; 4])),
    ];
    let font = atkinson_bold();
    let mut display = output.open()?;
    let mut shown = Frame::new();
    render(&mut shown, &layout(&household([Away; 4])), &font)?;
    display.show(&shown, Update::Full)?;
    println!("Full refresh: everybody away");
    let mut previous_state = household([Away; 4]);
    for (name, state) in steps {
        thread::sleep(Duration::from_secs(5));
        let window = changed_window(&previous_state, &state).expect("each step changes a slot");
        let mut frame = Frame::new();
        render(&mut frame, &layout(&state), &font)?;
        display.show(
            &frame,
            Update::Partial {
                previous: &shown,
                window,
            },
        )?;
        println!("Partial refresh: {name}");
        shown = frame;
        previous_state = state;
    }
    Ok(())
}

/// Clears the e-paper panel to white.
pub fn clear() -> Result<(), Box<dyn Error>> {
    epd::linux::open()?.clear()?;
    println!("Panel cleared");
    Ok(())
}

/// Logs the raw answers of the PRIM next departures API, one JSON line per
/// stop and poll.
pub fn bus_log(key_path: &Path, stops: &[Stop], dir: &Path) -> Result<(), Box<dyn Error>> {
    let client = prim_client::Client::new(read_prim_key(key_path)?);
    fs::create_dir_all(dir)?;
    for stop in stops {
        let polled_at = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let line = match client.stop_monitoring(&stop.monitoring_ref) {
            Ok(answer) => {
                println!(
                    "{}: HTTP {}, {} bytes",
                    stop.label,
                    answer.status,
                    answer.body.len()
                );
                prim::log_line(polled_at, answer.status, &answer.body)
            }
            Err(error) => {
                eprintln!("{}: {error}", stop.label);
                prim::log_line(polled_at, 0, &error.to_string())
            }
        };
        let path = dir.join(format!("{}.jsonl", stop.label));
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{line}")?;
    }
    Ok(())
}

/// Name shown on the Freebox screen during the authorization.
fn device_name() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_string())
        .unwrap_or_else(|_| "e-paper".into())
}

/// Writes a secret readable by its owner only; never overwrites.
fn save_secret(path: &Path, secret: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(secret.as_bytes())
}
```

### Red

Create the file `src/app/run.rs` with this content. `RunArgs` and `Prim`
are the options of `e-paper run`, moved from `main.rs`; `run` and
`poll_prim` are the former `run_cycle` and `poll_prim`; `compose` is still
to be written:

```rust
//! `e-paper run`: one cycle. `compose` decides everything from the data in
//! hand (pure); `run` reads the files and the clock, polls the Freebox and
//! PRIM, then shows the frame and writes what the next run needs.

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use jiff::Timestamp;

use crate::adapters::display::Update;
use crate::adapters::{freebox, freebox_client, prim, prim_client, state_file, usage_log};
use crate::app::{Output, read_mapping, read_prim_key, read_token};
use crate::domain::bus::{self, BusState};
use crate::domain::cycle::{CycleInput, cycle};
use crate::domain::departures::Direction;
use crate::domain::household::{ACTIVITY_THRESHOLD_BPS, PEOPLE, PersonState};
use crate::domain::saved::SavedState;
use crate::domain::screen::{Block, Time};
use crate::render::board::board_layout;
use crate::render::draw::{atkinson_bold, render_board};
use crate::render::frame::Frame;
use crate::render::update::{UpdateKind, hourly_full_due, plan_display};

/// The options of `e-paper run`.
#[derive(clap::Args)]
pub struct RunArgs {
    /// The app token written by `authorize`.
    #[arg(long, default_value = "secrets/freebox_app_token")]
    pub token: PathBuf,
    /// The mapping file.
    #[arg(long, default_value = "devices.toml")]
    pub devices: PathBuf,
    /// The state kept between runs.
    #[arg(long, default_value = "state.json")]
    pub state: PathBuf,
    #[command(flatten)]
    pub output: Output,
    #[command(flatten)]
    pub prim: Prim,
    /// Presence window in seconds: a smartphone active this recently is
    /// home. Independent of the cycle period.
    #[arg(long, default_value_t = 300)]
    pub interval: i64,
    /// Full refresh even if nothing changed.
    #[arg(long)]
    pub force: bool,
    /// Never use partial refreshes.
    #[arg(long)]
    pub full_only: bool,
    /// Appends each assigned device's download to this CSV file (dev
    /// feature, to check the activity threshold).
    #[arg(long)]
    pub usage_log: Option<PathBuf>,
}

/// The PRIM API, for the bus part.
#[derive(clap::Args)]
pub struct Prim {
    /// The PRIM API key (a secret).
    #[arg(long = "prim-key", default_value = "secrets/prim_api_key")]
    pub key: PathBuf,
    /// SIRI reference of the home pole towards Rungis, e.g.
    /// `STIF:StopPoint:Q:12345:`. Kept out of git: set it in the environment.
    #[arg(long, env = "PRIM_STOP_RUNGIS")]
    pub stop_rungis: String,
    /// SIRI reference of the home pole towards Antony RER.
    #[arg(long, env = "PRIM_STOP_ANTONY")]
    pub stop_antony: String,
}

/// What a run decides from, once the Freebox and PRIM data are in hand.
pub struct Inputs<'a> {
    pub now: Timestamp,
    /// The presence part; its `now` is the same instant, in seconds.
    pub presence: CycleInput<'a>,
    /// The bus state of this run: polled now, or carried over.
    pub bus: BusState,
    /// The image on the display, if saved.
    pub previous_frame: Option<&'a Frame>,
    pub force: bool,
    pub partial_enabled: bool,
}

/// What a run decides.
pub struct Outcome {
    pub frame: Frame,
    pub update: UpdateKind,
    /// To save once the display is updated; `last_full_refresh` already set
    /// if `update` is a full refresh.
    pub saved: SavedState,
    /// What the run prints: the people, then the two blocks.
    pub summary: Vec<String>,
}

pub fn compose(inputs: &Inputs) -> Outcome {
    todo!()
}

/// Runs one cycle: polls the Freebox (and PRIM on its own cadence),
/// computes the state, redraws if needed.
pub fn run(args: &RunArgs) -> Result<(), Box<dyn Error>> {
    let mapping = read_mapping(&args.devices)?;
    let app_token = read_token(&args.token)?;
    let (previous, warning) = state_file::load(&args.state);
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }
    // The image on the display, kept next to the state: a partial refresh
    // sends it again, as the panel forgets it between runs.
    let frame_path = args.state.with_file_name("frame.png");
    let previous_frame = fs::read(&frame_path)
        .ok()
        .and_then(|bytes| Frame::from_png(&bytes).ok());

    let mut client = freebox_client::Client::connect()?;
    client.login(&app_token)?;
    let hosts = client.lan_hosts()?;
    // Devices on the repeater only have the LAN browser's counter.
    let counters = freebox::download_counters(client.wifi_counters()?, &hosts);
    let seen = freebox::devices_seen(&hosts);
    let now = Timestamp::now();

    // PRIM is polled on its own cadence; a failed poll only shows as the
    // wrench, the presence part is still drawn.
    let previous_bus = previous.as_ref().and_then(|state| state.bus.as_ref());
    let bus = match previous_bus.filter(|bus| !bus::poll_due(now, Some(bus.polled_at))) {
        Some(bus) => bus.clone(),
        None => poll_prim(&args.prim, previous_bus, now)?,
    };

    let inputs = Inputs {
        now,
        presence: CycleInput {
            now: now.as_second(),
            interval_secs: args.interval,
            threshold_bps: ACTIVITY_THRESHOLD_BPS,
            mapping: &mapping,
            seen: &seen,
            counters: &counters,
            previous: previous.as_ref(),
        },
        bus,
        previous_frame: previous_frame.as_ref(),
        force: args.force,
        partial_enabled: !args.full_only,
    };
    let outcome = compose(&inputs);
    // The display matters more than the log: a write failure is only a warning.
    if let Some(path) = &args.usage_log {
        let lines = usage_log::usage_lines(&inputs.presence, &outcome.saved.household);
        if let Err(error) = usage_log::append(path, &lines) {
            eprintln!("warning: cannot write {}: {error}", path.display());
        }
    }
    for line in &outcome.summary {
        println!("{line}");
    }

    let update = match (outcome.update, &previous_frame) {
        (UpdateKind::None, _) => None,
        (UpdateKind::Partial(window), Some(previous)) => Some(Update::Partial { previous, window }),
        _ => Some(Update::Full),
    };
    match update {
        None => println!("No change: display not touched"),
        Some(update) => {
            args.output.open()?.show(&outcome.frame, update)?;
            fs::write(&frame_path, outcome.frame.to_png())
                .map_err(|error| format!("cannot write {}: {error}", frame_path.display()))?;
            let kind = match update {
                Update::Full => "full",
                Update::Partial { .. } => "partial",
            };
            println!("Display redrawn ({kind}): {}", args.output.describe());
        }
    }
    state_file::save(&args.state, &outcome.saved)
        .map_err(|error| format!("cannot write {}: {error}", args.state.display()))?;
    Ok(())
}

/// Polls both home poles. A failed poll is a warning, counted in the state.
fn poll_prim(
    args: &Prim,
    previous: Option<&BusState>,
    now: Timestamp,
) -> Result<BusState, Box<dyn Error>> {
    let client = prim_client::Client::new(read_prim_key(&args.key)?);
    let poll = |label: &str, stop: &str, direction| {
        let result = match client.stop_monitoring(stop) {
            Ok(answer) => prim::read_answer(answer.status, &answer.body, direction),
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = &result {
            eprintln!("warning: PRIM {label}: {error}");
        }
        result
    };
    let rungis = poll("rungis", &args.stop_rungis, Direction::Rungis);
    let antony = poll("antony", &args.stop_antony, Direction::Antony);
    Ok(bus::after_poll(previous, now, rungis, antony))
}

fn describe_block(block: &Block) -> String {
    match block {
        Block::Times(times) if times.is_empty() => "no departure".into(),
        Block::Times(times) => times
            .iter()
            .map(Time::to_string)
            .collect::<Vec<_>>()
            .join(" "),
        Block::Moon => "service over (moon)".into(),
        Block::Wrench => "bus data unavailable (wrench)".into(),
    }
}

fn describe(state: PersonState) -> &'static str {
    match state {
        PersonState::Away => "away",
        PersonState::HomePassive => "home",
        PersonState::HomeActive => "home (active)",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::bus::Pole;
    use crate::domain::departures::{Answer, Departure, LINE_297, LINE_319, ServiceStatus};
    use crate::domain::devices::{Assignment, DeviceSeen, Mapping};
    use crate::domain::household::Person;
    use crate::domain::household::PersonState::Away;
    use crate::domain::saved::Snapshot;

    const R_PHONE: &str = "02:00:00:00:00:01";
    const H_PHONE: &str = "02:00:00:00:00:03";
    const H_CONSOLE: &str = "02:00:00:00:00:04";

    fn at(time: &str) -> Timestamp {
        time.parse().unwrap()
    }

    fn mapping() -> Mapping {
        let assign = |person, name: &str, smartphone| Assignment {
            person: Person(person),
            name: name.into(),
            smartphone,
        };
        [
            (R_PHONE, assign('R', "Pixel-R", true)),
            (H_PHONE, assign('H', "Phone-H", true)),
            (H_CONSOLE, assign('H', "PS5", false)),
        ]
        .into_iter()
        .map(|(mac, assignment)| (mac.to_string(), assignment))
        .collect()
    }

    /// Rungis: 8:12 and 8:31 (Paris time); Antony: service over.
    fn bus() -> BusState {
        let departure = |time| Departure {
            line: LINE_319.into(),
            at: at(time),
        };
        BusState {
            polled_at: at("2026-10-14T06:00:00Z"),
            rungis: Pole {
                answer: Answer {
                    departures: vec![
                        departure("2026-10-14T06:12:00Z"),
                        departure("2026-10-14T06:31:00Z"),
                    ],
                    statuses: vec![],
                },
                failures: 0,
            },
            antony: Pole {
                answer: Answer {
                    departures: vec![],
                    statuses: vec![(LINE_297.into(), ServiceStatus::AfterLastJourney)],
                },
                failures: 0,
            },
        }
    }

    /// A run at `time` (UTC), R's phone active a minute ago, after the
    /// `previous` run if any (its saved state and frame).
    fn run_at(
        time: &str,
        previous: Option<&Outcome>,
        force: bool,
        partial_enabled: bool,
    ) -> Outcome {
        let now = at(time);
        let mapping = mapping();
        let seen = [DeviceSeen {
            mac: R_PHONE.into(),
            last_activity: Some(now.as_second() - 60),
        }];
        compose(&Inputs {
            now,
            presence: CycleInput {
                now: now.as_second(),
                interval_secs: 300,
                threshold_bps: ACTIVITY_THRESHOLD_BPS,
                mapping: &mapping,
                seen: &seen,
                counters: &BTreeMap::new(),
                previous: previous.map(|outcome| &outcome.saved),
            },
            bus: bus(),
            previous_frame: previous.map(|outcome| &outcome.frame),
            force,
            partial_enabled,
        })
    }

    /// 8:04 in Paris.
    const AT_0804: &str = "2026-10-14T06:04:00Z";

    #[test]
    fn first_run_is_a_full_refresh_and_records_it() {
        let outcome = run_at(AT_0804, None, false, true);
        assert_eq!(outcome.update, UpdateKind::Full);
        assert_eq!(
            outcome.saved.last_full_refresh,
            Some(at(AT_0804).as_second())
        );
        assert_eq!(outcome.saved.bus, Some(bus()));
    }

    #[test]
    fn same_screen_leaves_the_display_alone() {
        let first = run_at(AT_0804, None, false, true);
        let second = run_at(AT_0804, Some(&first), false, true);
        assert_eq!(second.update, UpdateKind::None);
        assert_eq!(
            second.saved.last_full_refresh,
            first.saved.last_full_refresh
        );
    }

    #[test]
    fn clock_change_is_a_partial_refresh() {
        let first = run_at(AT_0804, None, false, true);
        let second = run_at("2026-10-14T06:05:00Z", Some(&first), false, true);
        assert!(
            matches!(second.update, UpdateKind::Partial(_)),
            "{:?}",
            second.update
        );
        assert_eq!(
            second.saved.last_full_refresh,
            first.saved.last_full_refresh
        );
    }

    #[test]
    fn first_cycle_of_the_hour_is_a_full_refresh() {
        // 8:59 in Paris, then 9:00.
        let first = run_at("2026-10-14T06:59:00Z", None, false, true);
        let second = run_at("2026-10-14T07:00:00Z", Some(&first), false, true);
        assert_eq!(second.update, UpdateKind::Full);
        assert_eq!(
            second.saved.last_full_refresh,
            Some(at("2026-10-14T07:00:00Z").as_second())
        );
    }

    #[test]
    fn force_and_full_only_are_passed_on() {
        let first = run_at(AT_0804, None, false, true);
        let forced = run_at(AT_0804, Some(&first), true, true);
        assert_eq!(forced.update, UpdateKind::Full);
        let full_only = run_at("2026-10-14T06:05:00Z", Some(&first), false, false);
        assert_eq!(full_only.update, UpdateKind::Full);
    }

    #[test]
    fn summary_describes_each_person_and_block() {
        // R's and H's phones active; H's console downloaded 6 Mb/s over the
        // last 15 minutes.
        let now = at(AT_0804);
        let six_mbps = 6_000_000 / 8 * 900;
        let previous = SavedState {
            polled_at: now.as_second() - 900,
            household: PEOPLE.into_iter().map(|person| (person, Away)).collect(),
            counters: [(H_CONSOLE.to_string(), 0)].into_iter().collect(),
            last_full_refresh: None,
            earlier: Vec::<Snapshot>::new(),
            bus: None,
        };
        let seen = [R_PHONE, H_PHONE].map(|mac| DeviceSeen {
            mac: mac.into(),
            last_activity: Some(now.as_second() - 60),
        });
        let outcome = compose(&Inputs {
            now,
            presence: CycleInput {
                now: now.as_second(),
                interval_secs: 300,
                threshold_bps: ACTIVITY_THRESHOLD_BPS,
                mapping: &mapping(),
                seen: &seen,
                counters: &[(H_CONSOLE.to_string(), six_mbps)].into_iter().collect(),
                previous: Some(&previous),
            },
            bus: bus(),
            previous_frame: None,
            force: false,
            partial_enabled: true,
        });
        assert_eq!(
            outcome.summary,
            [
                "R home, M away, H home (active), L away",
                "Rungis: 8:12 8:31; Antony: service over (moon)",
            ]
        );
    }
}
```

In `src/main.rs`:

- replace everything above `#[derive(Parser)]` with:

  ```rust
  //! `e-paper`: household e-paper dashboard. Parses the command line; the
  //! commands live in `app`.

  use std::error::Error;
  use std::path::PathBuf;
  use std::process::ExitCode;

  use clap::{Parser, Subcommand};
  use e_paper::adapters::prim::{self, Stop};
  use e_paper::app::Output;
  use e_paper::app::run::{self, RunArgs};
  use e_paper::app::tools;
  ```

- replace the `Run { … }` variant of `Command` with:

  ```rust
      /// Runs one cycle: polls the Freebox (and PRIM on its own cadence),
      /// computes the state, redraws if needed.
      Run(RunArgs),
  ```

- replace everything from `/// The PRIM API, for the bus part.` to the end
  of the file with:

  ```rust
  fn main() -> ExitCode {
      match dispatch(Cli::parse()) {
          Ok(()) => ExitCode::SUCCESS,
          Err(error) => {
              eprintln!("error: {error}");
              ExitCode::FAILURE
          }
      }
  }

  fn dispatch(cli: Cli) -> Result<(), Box<dyn Error>> {
      match cli.command {
          Command::Authorize { token } => tools::authorize(&token),
          Command::Devices { token, output } => tools::list_devices(&token, &output),
          Command::CheckDevices { devices } => tools::check_devices(&devices),
          Command::CheckCounters { token, devices } => tools::check_counters(&token, &devices),
          Command::Run(args) => run::run(&args),
          Command::TestPattern { output } => tools::test_pattern(&output),
          Command::PartialTest { output } => tools::partial_test(&output),
          Command::BusLog { key, stops, dir } => tools::bus_log(&key, &stops, &dir),
          Command::Clear => tools::clear(),
      }
  }
  ```

  (`run` becomes `dispatch`: `run` is now the module of `e-paper run`.)

```bash
cargo test --lib app::run
```

Expected: the 6 tests fail with "not yet implemented".

### Green

In `src/app/run.rs`, replace the body of `compose` with:

```rust
    let presence = cycle(&inputs.presence);
    let board = bus::board(inputs.now, &inputs.bus);
    let people: Vec<String> = PEOPLE
        .iter()
        .map(|person| format!("{} {}", person.0, describe(presence.household[person])))
        .collect();
    let summary = vec![
        people.join(", "),
        format!(
            "Rungis: {}; Antony: {}",
            describe_block(&board.rungis),
            describe_block(&board.antony)
        ),
    ];

    let font = atkinson_bold();
    let mut frame = Frame::new();
    // Drawing into a frame cannot fail: its error type is `Infallible`.
    let Ok(()) = render_board(
        &mut frame,
        &board_layout(&board, &presence.household, &font),
        &font,
    );
    let mut saved = presence.saved;
    let now = inputs.presence.now;
    let full_required = inputs.force || hourly_full_due(now, saved.last_full_refresh);
    let update = plan_display(
        inputs.previous_frame,
        &frame,
        full_required,
        inputs.partial_enabled,
    );
    if update == UpdateKind::Full {
        saved.last_full_refresh = Some(now);
    }
    saved.bus = Some(inputs.bus.clone());
    Outcome {
        frame,
        update,
        saved,
        summary,
    }
```

```bash
cargo fmt
```

```bash
cargo test
```

Expected: 190 library tests, the 3 architecture tests and the 2 golden
tests pass.

```bash
cargo clippy --all-targets
```

```bash
git add -A
```

```bash
git commit -m "refactor: app layer, compose tested, main reduced to clap"
```

## 12. Step 7: checks and deployment

A real cycle on the dev PC, on a copy of the state (the home stop
references are those of `/etc/e-paper/prim.env` on the Pi):

```bash
mkdir -p /tmp/e-paper-check && cp state.json frame.png /tmp/e-paper-check/
```

```bash
PRIM_STOP_RUNGIS='…' PRIM_STOP_ANTONY='…' cargo run -- run --state /tmp/e-paper-check/state.json --png /tmp/e-paper-check/e-paper.png
```

Expected: the presence line, the two blocks, then `Display redrawn (full)`
(first cycle of the hour) or `(partial)`; the PNG shows the phase 2 screen.
Run the same command again within the minute: `No change: display not
touched`.

One visible difference, on purpose: a PRIM warning (`warning: PRIM …`) now
comes before the presence line, as PRIM is polled before `compose`.

```bash
make build
```

Then merge and deploy:

```bash
git switch main
```

```bash
git merge --ff-only refactor/architecture
```

```bash
make deploy
```

```bash
make logs
```

Expected: the next cycles log as before (presence line, blocks, refresh
kind), no error. The branch can then be deleted:

```bash
git branch -d refactor/architecture
```

## 13. Out of scope

- Traits for the Freebox and PRIM clients, fakes of the whole run: kept
  for later if testing the shell becomes a need (one `Sources` trait would
  be enough).
- Splitting the crate into several crates.
- Any behaviour change, any new feature.
- The phase 1 screens (`test-pattern`, `partial-test`, `changed_window`,
  phase 1 golden images): they stay, as diagnostic tools.
