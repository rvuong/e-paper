//! Device-to-person mapping (`devices.toml`). It holds MAC addresses: never
//! commit it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::freebox::Host;
use crate::layout::PEOPLE;
use crate::state::Person;

/// One `[[device]]` entry of the mapping file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceEntry {
    pub mac: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    /// R, M, H or L; empty for a shared device.
    #[serde(default)]
    pub person: String,
    #[serde(default)]
    pub smartphone: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeviceFile {
    #[serde(default, rename = "device")]
    devices: Vec<DeviceEntry>,
}

const DRAFT_HEADER: &str = "\
# Device-to-person mapping. It holds MAC addresses: never commit it.
#
# For each device, set `person` to R, M, H or L, or leave it empty for a shared
# device (ignored). Set `smartphone = true` on the phone of each person used
# for presence (one per person). `name` and `vendor` are for information only.

";

/// Draft mapping file listing every device, none assigned yet.
pub fn draft(hosts: &[Host]) -> String {
    let file = DeviceFile {
        devices: hosts
            .iter()
            .map(|host| DeviceEntry {
                mac: host.mac.clone(),
                name: host.name.clone(),
                vendor: host.vendor.clone(),
                person: String::new(),
                smartphone: false,
            })
            .collect(),
    };
    DRAFT_HEADER.to_string() + &toml::to_string(&file).expect("device list serializes to TOML")
}

/// Writes a new file; fails with `AlreadyExists` instead of overwriting.
pub fn write_new(path: &Path, content: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(content.as_bytes())
}

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

/// Why a mapping file is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingError {
    Toml(String),
    UnknownPerson { mac: String, person: String },
    TwoSmartphones { person: Person },
    DuplicateMac { mac: String },
}

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(detail) => write!(f, "invalid mapping file: {detail}"),
            Self::UnknownPerson { mac, person } => {
                write!(
                    f,
                    "{mac}: unknown person {person:?} (expected R, M, H, L or empty)"
                )
            }
            Self::TwoSmartphones { person } => {
                write!(f, "{}: more than one smartphone", person.0)
            }
            Self::DuplicateMac { mac } => write!(f, "{mac}: listed more than once"),
        }
    }
}

impl std::error::Error for MappingError {}

/// Reads and validates a mapping file.
pub fn parse_mapping(text: &str) -> Result<Mapping, MappingError> {
    let file: DeviceFile = toml::from_str(text).map_err(|e| MappingError::Toml(e.to_string()))?;
    let mut mapping = Mapping::new();
    let mut seen = BTreeSet::new();
    for entry in file.devices {
        let mac = entry.mac.to_lowercase();
        if !seen.insert(mac.clone()) {
            return Err(MappingError::DuplicateMac { mac });
        }
        if entry.person.is_empty() {
            continue;
        }
        let person = person_from(&entry.person).ok_or_else(|| MappingError::UnknownPerson {
            mac: mac.clone(),
            person: entry.person.clone(),
        })?;
        let has_smartphone = mapping
            .values()
            .any(|assignment: &Assignment| assignment.person == person && assignment.smartphone);
        if entry.smartphone && has_smartphone {
            return Err(MappingError::TwoSmartphones { person });
        }
        mapping.insert(
            mac,
            Assignment {
                person,
                name: entry.name,
                smartphone: entry.smartphone,
            },
        );
    }
    Ok(mapping)
}

/// The household member with this initial, if any.
fn person_from(text: &str) -> Option<Person> {
    PEOPLE
        .into_iter()
        .find(|person| text == person.0.to_string())
}

/// One line per person, in display order: number of devices and smartphone.
pub fn summary(mapping: &Mapping) -> Vec<String> {
    PEOPLE
        .into_iter()
        .map(|person| {
            let devices: Vec<&Assignment> = mapping
                .values()
                .filter(|assignment| assignment.person == person)
                .collect();
            let smartphone = if devices.iter().any(|assignment| assignment.smartphone) {
                "smartphone OK"
            } else {
                "no smartphone: always shown away"
            };
            format!("{}: {} device(s), {smartphone}", person.0, devices.len())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(mac: &str, name: &str, vendor: &str) -> Host {
        Host {
            mac: mac.into(),
            name: name.into(),
            vendor: vendor.into(),
            active: true,
            last_activity: None,
            access_point: None,
        }
    }

    fn device(mac: &str, person: &str, smartphone: bool) -> String {
        format!("[[device]]\nmac = \"{mac}\"\nperson = \"{person}\"\nsmartphone = {smartphone}\n\n")
    }

    #[test]
    fn draft_lists_every_device_unassigned() {
        let hosts = [
            host("02:00:00:00:00:01", "Pixel-R", "Google, Inc."),
            host("02:00:00:00:00:02", "PS5", ""),
        ];
        let file: DeviceFile = toml::from_str(&draft(&hosts)).unwrap();
        assert_eq!(
            file.devices,
            [
                DeviceEntry {
                    mac: "02:00:00:00:00:01".into(),
                    name: "Pixel-R".into(),
                    vendor: "Google, Inc.".into(),
                    person: String::new(),
                    smartphone: false,
                },
                DeviceEntry {
                    mac: "02:00:00:00:00:02".into(),
                    name: "PS5".into(),
                    vendor: String::new(),
                    person: String::new(),
                    smartphone: false,
                },
            ]
        );
    }

    #[test]
    fn draft_reads_back_as_a_valid_mapping() {
        let hosts = [host("02:00:00:00:00:01", "Pixel-R", "Google, Inc.")];
        assert_eq!(parse_mapping(&draft(&hosts)), Ok(Mapping::new()));
    }

    #[test]
    fn existing_file_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.toml");
        write_new(&path, "first").unwrap();

        let error = write_new(&path, "second").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first");
    }

    #[test]
    fn valid_file_gives_each_device_its_person() {
        let text =
            device("02:00:00:00:00:01", "R", true) + &device("02:00:00:00:00:02", "H", false);
        let mapping = parse_mapping(&text).unwrap();
        assert_eq!(
            mapping["02:00:00:00:00:01"],
            Assignment {
                person: Person('R'),
                name: String::new(),
                smartphone: true
            }
        );
        assert_eq!(
            mapping["02:00:00:00:00:02"],
            Assignment {
                person: Person('H'),
                name: String::new(),
                smartphone: false
            }
        );
    }

    #[test]
    fn assignment_keeps_the_device_name() {
        let text = "[[device]]\nmac = \"02:00:00:00:00:01\"\nname = \"PS5\"\nperson = \"H\"\n";
        let mapping = parse_mapping(text).unwrap();
        assert_eq!(mapping["02:00:00:00:00:01"].name, "PS5");
    }

    #[test]
    fn empty_person_is_a_shared_device() {
        let mapping = parse_mapping(&device("02:00:00:00:00:01", "", false)).unwrap();
        assert!(!mapping.contains_key("02:00:00:00:00:01"));
    }

    #[test]
    fn unknown_person_is_an_error() {
        assert_eq!(
            parse_mapping(&device("02:00:00:00:00:01", "X", false)),
            Err(MappingError::UnknownPerson {
                mac: "02:00:00:00:00:01".into(),
                person: "X".into()
            })
        );
    }

    #[test]
    fn two_smartphones_for_one_person_is_an_error() {
        let text = device("02:00:00:00:00:01", "R", true) + &device("02:00:00:00:00:02", "R", true);
        assert_eq!(
            parse_mapping(&text),
            Err(MappingError::TwoSmartphones {
                person: Person('R')
            })
        );
    }

    #[test]
    fn same_mac_twice_is_an_error() {
        let text =
            device("02:00:00:00:00:01", "R", false) + &device("02:00:00:00:00:01", "H", false);
        assert_eq!(
            parse_mapping(&text),
            Err(MappingError::DuplicateMac {
                mac: "02:00:00:00:00:01".into()
            })
        );
    }

    #[test]
    fn mac_addresses_ignore_case() {
        let mapping = parse_mapping(&device("AA:BB:CC:DD:EE:01", "R", false)).unwrap();
        assert!(mapping.contains_key("aa:bb:cc:dd:ee:01"));

        let text =
            device("AA:BB:CC:DD:EE:01", "R", false) + &device("aa:bb:cc:dd:ee:01", "H", false);
        assert_eq!(
            parse_mapping(&text),
            Err(MappingError::DuplicateMac {
                mac: "aa:bb:cc:dd:ee:01".into()
            })
        );
    }

    #[test]
    fn summary_counts_devices_and_smartphone_per_person() {
        let text = device("02:00:00:00:00:01", "R", true)
            + &device("02:00:00:00:00:02", "R", false)
            + &device("02:00:00:00:00:03", "M", true)
            + &device("02:00:00:00:00:04", "H", true)
            + &device("02:00:00:00:00:05", "", false);
        let lines = summary(&parse_mapping(&text).unwrap());
        assert_eq!(lines[0], "R: 2 device(s), smartphone OK");
        assert_eq!(lines[1], "M: 1 device(s), smartphone OK");
        assert_eq!(lines[2], "H: 1 device(s), smartphone OK");
    }

    #[test]
    fn summary_warns_about_a_person_without_smartphone() {
        let text = device("02:00:00:00:00:01", "L", false);
        let lines = summary(&parse_mapping(&text).unwrap());
        assert_eq!(lines[3], "L: 1 device(s), no smartphone: always shown away");
    }
}
