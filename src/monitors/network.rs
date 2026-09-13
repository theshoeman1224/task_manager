use std::path::Path;
use std::time::Instant;

use crate::core::{
    DeltaSample, DeltaTracker, Metric, MetricSnapshot, MetricValue, MonitorError, MonitorSource,
};
use crate::platform::linux::{read_network_counters, NetworkCounters};

const PROC_ROOT: &str = "/proc";

pub struct NetworkMonitor {
    selected_interface: Option<String>,
    previous_rx: DeltaTracker<String>,
    previous_tx: DeltaTracker<String>,
}

impl NetworkMonitor {
    pub fn new(selected_interface: Option<String>) -> Self {
        Self {
            selected_interface,
            previous_rx: DeltaTracker::new(),
            previous_tx: DeltaTracker::new(),
        }
    }
}

impl MonitorSource for NetworkMonitor {
    fn name(&self) -> &'static str {
        "Network"
    }

    fn sample(&mut self) -> Result<MetricSnapshot, MonitorError> {
        let counters = read_network_counters(Path::new(PROC_ROOT))
            .map_err(|err| MonitorError::new(err.to_string()))?;
        let active = choose_interface(&counters, self.selected_interface.as_deref());

        let mut snapshot = MetricSnapshot::new("Network");
        let Some(current) = active else {
            snapshot.subtitle = Some("No network interfaces found".to_string());
            snapshot
                .metrics
                .push(Metric::new("Receive", MetricValue::Unavailable));
            snapshot
                .metrics
                .push(Metric::new("Transmit", MetricValue::Unavailable));
            return Ok(snapshot);
        };

        snapshot.subtitle = Some(current.interface.clone());
        let now = Instant::now();
        let interface = current.interface.clone();

        let rx_rate = match self
            .previous_rx
            .record(interface.clone(), current.rx_bytes, now)
        {
            // First tick and reset/zero-elapsed ticks show zero rates,
            // exactly like the pre-refactor `unwrap_or((0.0, 0.0))` fallback.
            DeltaSample::First | DeltaSample::Invalid => None,
            DeltaSample::Changed(bytes, elapsed_secs) => Some(bytes as f64 / elapsed_secs),
        };
        let tx_rate = match self.previous_tx.record(interface, current.tx_bytes, now) {
            DeltaSample::First | DeltaSample::Invalid => None,
            DeltaSample::Changed(bytes, elapsed_secs) => Some(bytes as f64 / elapsed_secs),
        };
        let (rx_rate, tx_rate) = (rx_rate.unwrap_or(0.0), tx_rate.unwrap_or(0.0));

        snapshot
            .metrics
            .push(Metric::new("Receive", MetricValue::BytesPerSecond(rx_rate)));
        snapshot.metrics.push(Metric::new(
            "Transmit",
            MetricValue::BytesPerSecond(tx_rate),
        ));
        snapshot.metrics.push(Metric::new(
            "Received total",
            MetricValue::Bytes(current.rx_bytes),
        ));
        snapshot.metrics.push(Metric::new(
            "Transmitted total",
            MetricValue::Bytes(current.tx_bytes),
        ));
        Ok(snapshot)
    }
}

fn choose_interface<'a>(
    counters: &'a [NetworkCounters],
    selected_interface: Option<&str>,
) -> Option<&'a NetworkCounters> {
    if let Some(selected_interface) = selected_interface {
        return counters
            .iter()
            .find(|counter| counter.interface == selected_interface);
    }

    counters
        .iter()
        .filter(|counter| counter.interface != "lo")
        .max_by_key(|counter| counter.rx_bytes + counter.tx_bytes)
        .or_else(|| counters.first())
}

#[cfg(test)]
mod tests {
    use super::choose_interface;
    use crate::platform::linux::NetworkCounters;

    // Characterization tests: pin interface selection. See BASELINE.md.

    fn counter(interface: &str, rx: u64, tx: u64) -> NetworkCounters {
        NetworkCounters {
            interface: interface.to_string(),
            rx_bytes: rx,
            tx_bytes: tx,
        }
    }

    #[test]
    fn prefers_selected_interface() {
        let counters = vec![counter("eth0", 10, 10), counter("wlan0", 1, 1)];

        assert_eq!(
            choose_interface(&counters, Some("wlan0"))
                .unwrap()
                .interface,
            "wlan0"
        );
    }

    #[test]
    fn selected_interface_missing_returns_none() {
        // An explicit selection that isn't present yields None; no fallback
        // to the busiest interface happens.
        let counters = vec![counter("eth0", 10, 10)];
        assert!(choose_interface(&counters, Some("ppp0")).is_none());
    }

    #[test]
    fn skips_loopback_for_auto_selection() {
        let counters = vec![counter("lo", 1000, 1000), counter("eth0", 10, 20)];
        let chosen = choose_interface(&counters, None).unwrap();
        assert_eq!(chosen.interface, "eth0");
    }

    #[test]
    fn picks_busiest_interface_by_rx_plus_tx() {
        let counters = vec![counter("eth0", 10, 10), counter("wlan0", 1, 100)];
        let chosen = choose_interface(&counters, None).unwrap();
        assert_eq!(chosen.interface, "wlan0");
    }

    #[test]
    fn all_loopback_is_still_returned_as_fallback() {
        // "lo" is filtered for auto-selection, but counters.first() catches
        // the only-interfaces-are-lo case instead of returning None.
        let counters = vec![counter("lo", 1000, 1000)];
        let chosen = choose_interface(&counters, None).unwrap();
        assert_eq!(chosen.interface, "lo");
    }

    #[test]
    fn empty_counters_have_no_selection() {
        let counters: Vec<NetworkCounters> = Vec::new();
        assert!(choose_interface(&counters, None).is_none());
        assert!(choose_interface(&counters, Some("eth0")).is_none());
    }

    #[test]
    fn tie_keeps_first_max() {
        // max_by_key keeps the last maximal element on ties; both counters
        // tie, so wlan0 (later in the vec) is picked over eth0.
        let counters = vec![counter("eth0", 100, 100), counter("wlan0", 100, 100)];
        let chosen = choose_interface(&counters, None).unwrap();
        assert_eq!(chosen.interface, "wlan0");
    }
}
