//! State kept between two runs, in a small JSON file.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::refresh::HouseholdState;

/// What a run leaves for the next one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedState {
    /// When the Freebox was polled, Unix timestamp in seconds.
    pub polled_at: i64,
    /// What the display shows.
    pub household: HouseholdState,
    /// Download counter (`tx_bytes`) of each assigned device seen, by MAC.
    pub counters: BTreeMap<String, u64>,
    /// Last time the display was redrawn, Unix timestamp in seconds. Absent
    /// from state files written before it existed.
    #[serde(default)]
    pub last_full_refresh: Option<i64>,
    /// Partial refreshes since the last full one.
    #[serde(default)]
    pub partials_since_full: u32,
    /// Earlier readings of the counters, still within the rate window, oldest
    /// first (rates are averaged over that window).
    #[serde(default)]
    pub earlier: Vec<Snapshot>,
}

/// The download counters of the assigned devices at one poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unix timestamp in seconds.
    pub polled_at: i64,
    /// `tx_bytes` by MAC.
    pub counters: BTreeMap<String, u64>,
}

/// Reads the saved state. A missing file means no previous state. An
/// unreadable or corrupt file too, with a warning: it never blocks the display.
pub fn load(path: &Path) -> (Option<SavedState>, Option<String>) {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return (None, None),
        Err(error) => {
            return (
                None,
                Some(format!("cannot read {}: {error}", path.display())),
            );
        }
    };
    match serde_json::from_str(&text) {
        Ok(state) => (Some(state), None),
        Err(error) => (
            None,
            Some(format!("ignoring corrupt {}: {error}", path.display())),
        ),
    }
}

/// Writes the state. The file is replaced in one step (write, then rename), so
/// a crash never leaves half a file.
pub fn save(path: &Path, state: &SavedState) -> io::Result<()> {
    let json = serde_json::to_string_pretty(state).expect("state serializes to JSON");
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, json)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Person;
    use crate::state::PersonState::{Away, HomeActive};

    fn example() -> SavedState {
        SavedState {
            polled_at: 1_760_000_000,
            household: [(Person('R'), HomeActive), (Person('M'), Away)]
                .into_iter()
                .collect(),
            counters: [("02:00:00:00:00:02".to_string(), 1_200_000_000)]
                .into_iter()
                .collect(),
            last_full_refresh: Some(1_759_990_000),
            partials_since_full: 3,
            earlier: vec![Snapshot {
                polled_at: 1_759_999_400,
                counters: [("02:00:00:00:00:02".to_string(), 1_100_000_000)]
                    .into_iter()
                    .collect(),
            }],
        }
    }

    #[test]
    fn saved_state_reads_back_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        save(&path, &example()).unwrap();
        assert_eq!(load(&path), (Some(example()), None));
    }

    #[test]
    fn missing_file_means_no_previous_state() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("state.json")), (None, None));
    }

    #[test]
    fn corrupt_file_means_no_previous_state_with_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, "{ not json").unwrap();
        let (state, warning) = load(&path);
        assert_eq!(state, None);
        assert!(warning.unwrap().contains("corrupt"));
    }

    #[test]
    fn state_file_without_last_full_refresh_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(
            &path,
            r#"{"polled_at": 1760000000, "household": {"R": "Away"}, "counters": {}}"#,
        )
        .unwrap();
        let (state, warning) = load(&path);
        assert_eq!(warning, None);
        assert_eq!(state.unwrap().last_full_refresh, None);
    }
}
