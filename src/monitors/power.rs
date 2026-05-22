use crate::core::{Metric, MetricSnapshot, MetricValue, MonitorError, MonitorSource};
use crate::platform::linux::read_power_supplies;

pub struct PowerMonitor;

impl PowerMonitor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PowerMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorSource for PowerMonitor {
    fn name(&self) -> &'static str {
        "Power"
    }

    fn sample(&mut self) -> Result<MetricSnapshot, MonitorError> {
        let supplies = read_power_supplies(std::path::Path::new("/sys/class/power_supply"))
            .map_err(|err| MonitorError::new(err.to_string()))?;
        let mut snapshot = MetricSnapshot::new("Power");

        if supplies.is_empty() {
            snapshot.subtitle = Some("No system power supply telemetry found".to_string());
            snapshot
                .metrics
                .push(Metric::new("System power", MetricValue::Unavailable));
            snapshot
                .metrics
                .push(Metric::new("Battery", MetricValue::Unavailable));
            return Ok(snapshot);
        }

        snapshot.subtitle = Some("System power supply telemetry".to_string());
        for supply in supplies {
            let prefix = format!("{} {}", supply.name, supply.kind);
            snapshot.metrics.push(Metric::new(
                format!("{prefix} power"),
                supply
                    .power_watts
                    .map(MetricValue::Watts)
                    .unwrap_or(MetricValue::Unavailable),
            ));
            snapshot.metrics.push(Metric::new(
                format!("{prefix} capacity"),
                supply
                    .capacity_percent
                    .map(|value| MetricValue::Percent(value as f64))
                    .unwrap_or(MetricValue::Unavailable),
            ));
            if let Some(power_watts) = supply.power_watts {
                snapshot.graph_points.push((prefix, power_watts));
            }
        }

        Ok(snapshot)
    }
}
