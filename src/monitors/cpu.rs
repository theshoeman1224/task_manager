use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::core::{
    DeltaSample, DeltaTracker, Metric, MetricSnapshot, MetricValue, MonitorError, MonitorSource,
};
use crate::platform::linux::{cpu_usage_percent, read_cpu_times, CpuTimes};
use crate::platform::power::read_powercap_energy_counters;

const POWERCAP_ENERGY_ROOT: &str = "/sys/class/powercap";

pub struct CpuMonitor {
    previous_times: Option<CpuTimes>,
    previous_energy: DeltaTracker<PathBuf>,
}

// Monitors have no Default use site; construction goes through new() only.
#[allow(clippy::new_without_default)]
impl CpuMonitor {
    pub fn new() -> Self {
        Self {
            previous_times: None,
            previous_energy: DeltaTracker::new(),
        }
    }
}

impl MonitorSource for CpuMonitor {
    fn name(&self) -> &'static str {
        "CPU"
    }

    fn sample(&mut self) -> Result<MetricSnapshot, MonitorError> {
        let current =
            read_cpu_times(Path::new("/proc")).map_err(|err| MonitorError::new(err.to_string()))?;
        let usage = self
            .previous_times
            .and_then(|previous| cpu_usage_percent(previous, current));
        self.previous_times = Some(current);

        let mut snapshot = MetricSnapshot::new("CPU");
        snapshot.subtitle = Some("Aggregate processor usage and package power".to_string());
        snapshot
            .metrics
            .push(Metric::new("Usage", MetricValue::percentage(usage)));

        let package_power = self.sample_package_power();
        snapshot.metrics.push(Metric::new(
            "CPU package power",
            MetricValue::watts(package_power),
        ));
        Ok(snapshot)
    }
}

impl CpuMonitor {
    fn sample_package_power(&mut self) -> Option<f64> {
        let counters = read_powercap_energy_counters(Path::new(POWERCAP_ENERGY_ROOT)).ok()?;
        let now = Instant::now();
        let mut total_watts = 0.0;
        let mut found = false;

        for counter in counters {
            match self
                .previous_energy
                .record(counter.path, counter.energy_uj, now)
            {
                // First tick (or a reset/zero-elapsed tick) contributes
                // nothing, exactly as the pre-refactor per-domain else/continue
                // did.
                DeltaSample::First | DeltaSample::Invalid => continue,
                DeltaSample::Changed(delta_uj, elapsed_secs) => {
                    total_watts += delta_uj as f64 / 1_000_000.0 / elapsed_secs;
                    found = true;
                }
            }
        }

        found.then_some(total_watts)
    }
}
