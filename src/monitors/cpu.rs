use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::core::{Metric, MetricSnapshot, MetricValue, MonitorError, MonitorSource};
use crate::platform::linux::{
    cpu_usage_percent, read_cpu_times, read_powercap_energy_counters, CpuTimes,
};

pub struct CpuMonitor {
    previous_times: Option<CpuTimes>,
    previous_energy: HashMap<PathBuf, (u64, Instant)>,
}

impl CpuMonitor {
    pub fn new() -> Self {
        Self {
            previous_times: None,
            previous_energy: HashMap::new(),
        }
    }
}

impl Default for CpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorSource for CpuMonitor {
    fn name(&self) -> &'static str {
        "CPU"
    }

    fn sample(&mut self) -> Result<MetricSnapshot, MonitorError> {
        let current = read_cpu_times().map_err(|err| MonitorError::new(err.to_string()))?;
        let usage = self
            .previous_times
            .and_then(|previous| cpu_usage_percent(previous, current));
        self.previous_times = Some(current);

        let mut snapshot = MetricSnapshot::new("CPU");
        snapshot.subtitle = Some("Aggregate processor usage and package power".to_string());
        snapshot.metrics.push(Metric::new(
            "Usage",
            usage
                .map(MetricValue::Percent)
                .unwrap_or(MetricValue::Unavailable),
        ));

        let package_power = self.sample_package_power();
        snapshot.metrics.push(Metric::new(
            "CPU package power",
            package_power
                .map(MetricValue::Watts)
                .unwrap_or(MetricValue::Unavailable),
        ));
        snapshot
            .graph_points
            .push(("Usage".to_string(), usage.unwrap_or(0.0).clamp(0.0, 100.0)));

        Ok(snapshot)
    }
}

impl CpuMonitor {
    fn sample_package_power(&mut self) -> Option<f64> {
        let now = Instant::now();
        let counters =
            read_powercap_energy_counters(PathBuf::from("/sys/class/powercap").as_path()).ok()?;
        let mut total_watts = 0.0;
        let mut found = false;

        for counter in counters {
            let Some((previous_energy, previous_time)) = self
                .previous_energy
                .insert(counter.path, (counter.energy_uj, now))
            else {
                continue;
            };
            let elapsed = now.duration_since(previous_time).as_secs_f64();
            if elapsed <= 0.0 || counter.energy_uj < previous_energy {
                continue;
            }

            let energy_delta_joules = (counter.energy_uj - previous_energy) as f64 / 1_000_000.0;
            total_watts += energy_delta_joules / elapsed;
            found = true;
        }

        found.then_some(total_watts)
    }
}
