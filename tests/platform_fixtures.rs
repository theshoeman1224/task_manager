// Fixture-driven tests for the platform readers. The fixtures under
// tests/fixtures/ mirror procfs/sysfs layouts (no symlinks; the powercap
// walker canonicalizes and then only descends into real directories, so
// plain nested directories reproduce a live tree faithfully). These tests
// pin parsing and fallback behavior ahead of splitting platform/linux.rs.
// See BASELINE.md.

use std::path::PathBuf;

use linux_task_manager::platform::linux::{
    parse_proc_net_dev, read_amd_gpus, read_cpu_times, read_network_counters,
    read_power_supplies, read_powercap_energy_counters, NetworkCounters,
};
use linux_task_manager::platform::linux::EnergyCounter;

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(rel)
}

#[test]
fn proc_stat_fixture_yields_aggregate_only() {
    // Fixture contains the aggregate "cpu" line and a "cpu0" core line;
    // read_cpu_times returns only the aggregate.
    let times = read_cpu_times(&fixture("proc")).expect("fixture /proc/stat parses");
    assert_eq!(times.idle, 860);
    assert_eq!(times.total, 1010);
}

#[test]
fn proc_net_dev_fixture_matches_parse() {
    let counters = read_network_counters(&fixture("proc")).expect("fixture /proc/net/dev reads");
    let raw = std::fs::read_to_string(fixture("proc/net/dev")).unwrap();
    let parsed: Vec<NetworkCounters> = parse_proc_net_dev(&raw);
    assert_eq!(counters, parsed);
    assert_eq!(counters.len(), 2);
    assert_eq!(counters[0].interface, "lo");
    assert_eq!(counters[0].rx_bytes, 1000);
    assert_eq!(counters[0].tx_bytes, 2000);
    assert_eq!(counters[1].interface, "eth0");
    assert_eq!(counters[1].rx_bytes, 4096);
    assert_eq!(counters[1].tx_bytes, 8192);
}

#[test]
fn proc_stat_missing_root_errors() {
    let path = PathBuf::from("/nonexistent-proc-root-definitely-missing");
    assert!(read_cpu_times(&path).is_err());
    assert!(read_network_counters(&path).is_err());
}

#[test]
fn powercap_fixture_walks_zone_tree() {
    let counters =
        read_powercap_energy_counters(&fixture("sys/class/powercap")).expect("fixture reads");
    assert_eq!(counters.len(), 2, "only zones with parseable energy_uj; the bogus zone has garbage energy so is excluded");
    let by_name = |suffix: &str| -> &EnergyCounter {
        counters
            .iter()
            .find(|c| c.path.ends_with(suffix))
            .unwrap_or_else(|| panic!("missing counter ending in {suffix}: {counters:?}"))
    };
    let main = by_name("zone0");
    assert_eq!(main.name, "intel-rapl-main");
    assert_eq!(main.energy_uj, 1234567890);
    let subzone = by_name("subzone");
    assert_eq!(subzone.name, "intel-rapl-core");
    assert_eq!(subzone.energy_uj, 987654321);
}

#[test]
fn powercap_missing_root_is_empty_not_error() {
    // The canonicalize fallback treats an unreadable/missing root as zero
    // counters (this is the behavior the runaway-walk fix kept identical).
    let path = PathBuf::from("/nonexistent-sys-root-definitely-missing/class/powercap");
    let counters = read_powercap_energy_counters(&path).expect("missing root is not an error");
    assert!(counters.is_empty());
}

#[test]
fn amd_drm_fixture_reads_direct_and_hwmon_files() {
    let gpus = read_amd_gpus(&fixture("sys/class/drm")).expect("fixture reads");
    assert_eq!(
        gpus.len(),
        1,
        "cardNoHwmon has a garbage gpu_busy_percent and no fallback readings, so it is excluded"
    );
    let gpu = &gpus[0];
    assert_eq!(gpu.card, "card1");
    assert_eq!(gpu.utilization_percent, Some(42.0));
    // device/power1_average wins (35123000 micro-watts -> 35.123 W);
    // hwmon's 51230000 would be a different value, so assert exactly.
    assert_eq!(gpu.power_watts, Some(35.123));
    // device/temp1_input is absent; hwmon2/temp1_input (36500 milli) -> 36.5 C.
    assert_eq!(gpu.temperature_celsius, Some(36.5));
}

#[test]
fn amd_missing_root_is_empty_not_error() {
    let path = PathBuf::from("/nonexistent-sys-root-definitely-missing/class/drm");
    let gpus = read_amd_gpus(&path).expect("missing root is not an error");
    assert!(gpus.is_empty());
}

#[test]
fn power_supplies_fixture_units_and_fallbacks() {
    let supplies =
        read_power_supplies(&fixture("sys/class/power_supply")).expect("fixture reads");
    let by_name = |name: &str| {
        supplies
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("missing supply {name} in {supplies:?}"))
    };

    let bat = by_name("BAT0");
    assert_eq!(bat.kind, "Battery");
    // power_now is the primary source: 12000000 micro-watts -> 12.0 W.
    assert_eq!(bat.power_watts, Some(12.0));
    assert_eq!(bat.capacity_percent, Some(73));

    let ac = by_name("AC");
    // No power_now, so the fallback multiplies current_now by voltage_now
    // after both are already scaled by 1e-6: 2000e-6 amps * 240000e-6 volts
    // = 0.002 A * 0.24 V = 0.00048 W. (Unit mismatch is a real quirk of the
    // current code; characterizing it, not endorsing it.)
    assert_eq!(ac.power_watts, Some(0.00048));
    assert!(ac.capacity_percent.is_none());

    let weird = by_name("WEIRD");
    // Neither power_now nor current_now exists -> no power reading.
    assert_eq!(weird.power_watts, None);
    // Capacity is whitespace-only -> u64 parse fails -> None.
    assert_eq!(weird.capacity_percent, None);
    assert_eq!(weird.kind, "WeirdThing");
}

#[test]
fn power_supplies_missing_root_is_empty_not_error() {
    let path = PathBuf::from("/nonexistent-sys-root-definitely-missing/class/power_supply");
    let supplies = read_power_supplies(&path).expect("missing root is not an error");
    assert!(supplies.is_empty());
}
