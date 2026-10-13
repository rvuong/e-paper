//! `check-counters`: where each assigned device's download counter comes from.
//! Never prints a MAC address.

use std::collections::{BTreeMap, BTreeSet};

use crate::devices::Mapping;
use crate::freebox::Host;
use crate::layout::PEOPLE;

/// One line per assigned device, in display order of the people, then by name.
pub fn counters_report(
    mapping: &Mapping,
    hosts: &[Host],
    counters: &BTreeMap<String, u64>,
) -> Vec<String> {
    let yes_no = |flag: bool| if flag { "yes" } else { "no" };
    let mut devices: Vec<_> = mapping.iter().collect();
    devices.sort_by_key(|(_, assignment)| {
        let rank = PEOPLE
            .iter()
            .position(|&person| person == assignment.person);
        (rank, assignment.name.to_lowercase())
    });
    devices
        .into_iter()
        .map(|(mac, assignment)| {
            let host = hosts.iter().find(|host| &host.mac == mac);
            let access_point = host.and_then(|host| host.access_point.as_ref());
            let via = access_point.map_or("-".to_string(), |ap| {
                let unknown = || "?".to_string();
                format!(
                    "{}/{}",
                    ap.kind.clone().unwrap_or_else(unknown),
                    ap.connectivity.clone().unwrap_or_else(unknown)
                )
            });
            format!(
                "{}  {}  lan: {}  via: {via}  ap tx_bytes: {}  wifi/ap counter: {}",
                assignment.person.0,
                assignment.name,
                if host.is_some() { "seen" } else { "not seen" },
                yes_no(access_point.is_some_and(|ap| ap.tx_bytes.is_some())),
                yes_no(counters.contains_key(mac)),
            )
        })
        .collect()
}

/// Names of the fields of every `access_point` seen, sorted, without values.
pub fn access_point_fields(hosts: &[Host]) -> Vec<String> {
    let fields: BTreeSet<&String> = hosts
        .iter()
        .filter_map(|host| host.access_point.as_ref())
        .flat_map(|ap| &ap.fields)
        .collect();
    fields.into_iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::Assignment;
    use crate::freebox::HostAccessPoint;
    use crate::state::Person;

    const R_PHONE: &str = "02:00:00:00:00:01";
    const H_CONSOLE: &str = "02:00:00:00:00:02";
    const H_PHONE: &str = "02:00:00:00:00:03";

    fn mapping() -> Mapping {
        let assign = |person, name: &str| Assignment {
            person: Person(person),
            name: name.into(),
            smartphone: false,
        };
        [
            (H_CONSOLE, assign('H', "PS5")),
            (H_PHONE, assign('H', "iPhone H")),
            (R_PHONE, assign('R', "iPhone R")),
        ]
        .into_iter()
        .map(|(mac, assignment)| (mac.to_string(), assignment))
        .collect()
    }

    fn host(mac: &str, access_point: Option<HostAccessPoint>) -> Host {
        Host {
            mac: mac.into(),
            name: String::new(),
            vendor: String::new(),
            active: true,
            last_activity: None,
            access_point,
        }
    }

    fn access_point(kind: &str, tx_bytes: Option<u64>, fields: &[&str]) -> HostAccessPoint {
        HostAccessPoint {
            kind: Some(kind.into()),
            connectivity: Some("wifi".into()),
            tx_bytes,
            fields: fields.iter().map(|field| field.to_string()).collect(),
        }
    }

    fn hosts() -> Vec<Host> {
        vec![
            host(
                R_PHONE,
                Some(access_point("gateway", Some(5), &["tx_bytes", "type"])),
            ),
            host(
                H_CONSOLE,
                Some(access_point("repeater", None, &["type", "uid"])),
            ),
        ]
    }

    fn counters() -> BTreeMap<String, u64> {
        [(R_PHONE.to_string(), 5)].into_iter().collect()
    }

    #[test]
    fn report_line_per_assigned_device_without_mac() {
        let report = counters_report(&mapping(), &hosts(), &counters());
        assert_eq!(
            report,
            [
                "R  iPhone R  lan: seen  via: gateway/wifi  ap tx_bytes: yes  wifi/ap counter: yes",
                "H  iPhone H  lan: not seen  via: -  ap tx_bytes: no  wifi/ap counter: no",
                "H  PS5  lan: seen  via: repeater/wifi  ap tx_bytes: no  wifi/ap counter: no",
            ]
        );
        assert!(report.iter().all(|line| !line.contains("02:00")));
    }

    #[test]
    fn device_absent_from_lan_browser_is_reported_not_seen() {
        let report = counters_report(&mapping(), &[], &counters());
        assert_eq!(report.len(), 3);
        assert!(report.iter().all(|line| line.contains("lan: not seen")));
    }

    #[test]
    fn wifi_counter_presence_is_reported() {
        let report = counters_report(&mapping(), &hosts(), &BTreeMap::new());
        assert!(report[0].ends_with("wifi/ap counter: no"));
    }

    #[test]
    fn access_point_field_names_are_listed_once() {
        let mut hosts = hosts();
        hosts.push(host(H_PHONE, None));
        assert_eq!(access_point_fields(&hosts), ["tx_bytes", "type", "uid"]);
    }
}
