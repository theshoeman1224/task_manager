// Characterization tests for the monitors as a whole. These run against the
// live system (real /proc and /sys), so they assert structural invariants
// rather than exact values: label sets, metric kinds, availability fallbacks.
// They must keep passing unchanged through the refactor. See BASELINE.md.

use linux_task_manager::core::{MetricValue, MonitorSource};
use linux_task_manager::monitors::{CpuMonitor, GpuMonitor, NetworkMonitor, PowerMonitor};

const GPU_LABELS: [&str; 3] = ["Usage", "GPU power", "Temperature"];

#[test]
fn gpu_snapshot_always_exposes_the_three_labels() {
    // Every branch of GpuMonitor::sample (NVIDIA, AMD, both unavailable
    // fallbacks) produces exactly Usage / GPU power / Temperature. The
    // unavailable branches push those three metrics Unavailable in that
    // order; whichever branch fires in this environment, the label set
    // must be stable and the values must be either concrete or
    // Unavailable.
    let mut monitor = GpuMonitor::new();
    let snapshot = monitor.sample().expect("gpu sample should not error");

    let names: Vec<&str> = snapshot.metrics.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names, GPU_LABELS,
        "GPU label set/order changed: subtitle {:?}",
        snapshot.subtitle
    );
    assert!(matches!(
        snapshot.metrics[0].value,
        MetricValue::Percent(_) | MetricValue::Unavailable
    ));
    assert!(matches!(
        snapshot.metrics[1].value,
        MetricValue::Watts(_) | MetricValue::Unavailable
    ));
    assert!(matches!(
        snapshot.metrics[2].value,
        MetricValue::Text(_) | MetricValue::Unavailable
    ));
    assert!(snapshot.subtitle.is_some());
}

#[test]
fn cpu_snapshot_shape() {
    let mut monitor = CpuMonitor::new();
    let first = monitor.sample().expect("cpu sample should not error");
    assert_eq!(first.title, "CPU");
    assert_eq!(first.metrics[0].name, "Usage");
    assert_eq!(first.metrics[1].name, "CPU package power");
    assert_eq!(
        first.metrics[0].value,
        MetricValue::Unavailable,
        "first sample has no previous times, so usage is Unavailable"
    );

    let second = monitor
        .sample()
        .expect("second cpu sample should not error");
    // After a second tick the usage metric is either a real percentage in
    // [0, 100] or Unavailable if the total counter did not move.
    match &second.metrics[0].value {
        MetricValue::Percent(v) => {
            assert!((0.0..=100.0).contains(v), "usage outside [0, 100]: {v}")
        }
        MetricValue::Unavailable => {}
        other => panic!("expected Percent or Unavailable, got {other:?}"),
    }
    // Package power depends on /sys/class/powercap presence; either shape ok.
    assert!(matches!(
        second.metrics[1].value,
        MetricValue::Watts(_) | MetricValue::Unavailable
    ));
    // graph_points still carry the clamped usage point on every sample
    // (pinned: removing the field is an explicit output change, BASELINE.md).
    assert_eq!(second.graph_points.len(), 1);
    let (label, value) = &second.graph_points[0];
    assert_eq!(label, "Usage");
    assert!(
        (0.0..=100.0).contains(value),
        "graph usage clamped: {value}"
    );
}

#[test]
fn network_snapshot_shape() {
    let mut monitor = NetworkMonitor::new(None);
    let first = monitor.sample().expect("network sample should not error");
    assert_eq!(first.title, "Network");

    let names: Vec<&str> = first.metrics.iter().map(|m| m.name.as_str()).collect();
    if names == ["Receive", "Transmit"] {
        // No eligible interface found: the fixed Unavailable pair.
        assert_eq!(first.metrics[0].value, MetricValue::Unavailable);
        assert_eq!(first.metrics[1].value, MetricValue::Unavailable);
        assert!(first
            .subtitle
            .is_none_or(|s| s.contains("No network interfaces")));
        return;
    }

    assert_eq!(
        names,
        vec!["Receive", "Transmit", "Received total", "Transmitted total"],
        "network label set/order changed"
    );
    for rate in &first.metrics[0..2] {
        assert!(matches!(rate.value, MetricValue::BytesPerSecond(_)));
    }
    for total in &first.metrics[2..4] {
        assert!(matches!(total.value, MetricValue::Bytes(_)));
    }
    assert_eq!(first.graph_points.len(), 2, "RX and TX rate points");
    assert_eq!(first.graph_points[0].0, "RX");
    assert_eq!(first.graph_points[1].0, "TX");
}

#[test]
fn power_snapshot_shape() {
    let mut monitor = PowerMonitor::new();
    let snapshot = monitor.sample().expect("power sample should not error");
    assert_eq!(snapshot.title, "Power");
    assert!(snapshot.subtitle.is_some());

    let names: Vec<&str> = snapshot.metrics.iter().map(|m| m.name.as_str()).collect();
    if snapshot.metrics.len() == 2 && names == ["System power", "Battery"] {
        assert_eq!(snapshot.metrics[0].value, MetricValue::Unavailable);
        assert_eq!(snapshot.metrics[1].value, MetricValue::Unavailable);
        return;
    }

    // With supplies present: two metrics per supply, named
    // "<name> <kind> power" and "<name> <kind> capacity", plus one graph
    // point per supply that reported power.
    assert_eq!(snapshot.metrics.len() % 2, 0, "metrics pair per supply");
    let mut graph_labels: Vec<&str> = snapshot
        .graph_points
        .iter()
        .map(|(l, _)| l.as_str())
        .collect();
    graph_labels.sort_unstable();
    for label in graph_labels {
        assert!(label.ends_with(" power"), "graph label {label}");
    }
}

#[test]
fn monitor_names_match_tabs() {
    assert_eq!(CpuMonitor::new().name(), "CPU");
    assert_eq!(GpuMonitor::new().name(), "GPU");
    assert_eq!(NetworkMonitor::new(None).name(), "Network");
    assert_eq!(PowerMonitor::new().name(), "Power");
}
