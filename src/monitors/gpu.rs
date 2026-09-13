use std::path::Path;

use crate::core::{Metric, MetricSnapshot, MetricValue, MonitorError, MonitorSource};
use crate::platform::linux::{
    nvidia_devices_present, read_amd_gpus, NvidiaGpuInfo, NvidiaGpuReader,
};

const PROC_ROOT: &str = "/proc";
const SYS_ROOT: &str = "/sys";
const DEV_ROOT: &str = "/dev";

pub struct GpuMonitor {
    nvidia: Option<NvidiaGpuReader>,
    nvidia_error: Option<String>,
}

impl GpuMonitor {
    pub fn new() -> Self {
        let (nvidia, nvidia_error) = if nvidia_devices_present(
            Path::new(PROC_ROOT),
            Path::new(DEV_ROOT),
        ) {
            match NvidiaGpuReader::new() {
                Ok(reader) => (Some(reader), None),
                Err(err) => (None, Some(err.to_string())),
            }
        } else {
            (None, None)
        };

        Self {
            nvidia,
            nvidia_error,
        }
    }
}

impl Default for GpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorSource for GpuMonitor {
    fn name(&self) -> &'static str {
        "GPU"
    }

    fn sample(&mut self) -> Result<MetricSnapshot, MonitorError> {
        if let Some(reader) = &self.nvidia {
            match reader.query() {
                Ok(nvidia_gpus) => {
                    if let Some(gpu) = nvidia_gpus.first() {
                        return Ok(nvidia_snapshot(gpu));
                    }
                }
                Err(err) => {
                    self.nvidia_error = Some(err.to_string());
                }
            }
        }

        let amd_gpus = read_amd_gpus(Path::new(&format!("{SYS_ROOT}/class/drm")))
            .map_err(|err| MonitorError::new(err.to_string()))?;
        let mut snapshot = MetricSnapshot::new("GPU");

        if let Some(gpu) = amd_gpus.first() {
            snapshot.subtitle = Some(format!("AMD {}", gpu.card));
            snapshot.metrics.push(Metric::new(
                "Usage",
                gpu.utilization_percent
                    .map(MetricValue::Percent)
                    .unwrap_or(MetricValue::Unavailable),
            ));
            snapshot.metrics.push(Metric::new(
                "GPU power",
                gpu.power_watts
                    .map(MetricValue::Watts)
                    .unwrap_or(MetricValue::Unavailable),
            ));
            snapshot.metrics.push(Metric::new(
                "Temperature",
                gpu.temperature_celsius
                    .map(|value| MetricValue::Text(format!("{value:.1} C")))
                    .unwrap_or(MetricValue::Unavailable),
            ));
            snapshot.graph_points.push((
                "Usage".to_string(),
                gpu.utilization_percent.unwrap_or(0.0).clamp(0.0, 100.0),
            ));
            return Ok(snapshot);
        }

        if nvidia_devices_present(Path::new(PROC_ROOT), Path::new(DEV_ROOT)) {
            snapshot.subtitle = Some(
                self.nvidia_error
                    .as_deref()
                    .map(|err| format!("NVIDIA device detected; NVML unavailable: {err}"))
                    .unwrap_or_else(|| "NVIDIA device detected; NVML unavailable".to_string()),
            );
            snapshot
                .metrics
                .push(Metric::new("Usage", MetricValue::Unavailable));
            snapshot
                .metrics
                .push(Metric::new("GPU power", MetricValue::Unavailable));
            snapshot
                .metrics
                .push(Metric::new("Temperature", MetricValue::Unavailable));
            return Ok(snapshot);
        }

        snapshot.subtitle = Some("No supported AMD or NVIDIA GPU telemetry found".to_string());
        snapshot
            .metrics
            .push(Metric::new("Usage", MetricValue::Unavailable));
        snapshot
            .metrics
            .push(Metric::new("GPU power", MetricValue::Unavailable));
        snapshot
            .metrics
            .push(Metric::new("Temperature", MetricValue::Unavailable));
        Ok(snapshot)
    }
}

fn nvidia_snapshot(gpu: &NvidiaGpuInfo) -> MetricSnapshot {
    let mut snapshot = MetricSnapshot::new("GPU");
    snapshot.subtitle = Some(format!("NVIDIA GPU {}", gpu.index));
    snapshot.metrics.push(Metric::new(
        "Usage",
        gpu.utilization_percent
            .map(MetricValue::Percent)
            .unwrap_or(MetricValue::Unavailable),
    ));
    snapshot.metrics.push(Metric::new(
        "GPU power",
        gpu.power_watts
            .map(MetricValue::Watts)
            .unwrap_or(MetricValue::Unavailable),
    ));
    snapshot.metrics.push(Metric::new(
        "Temperature",
        gpu.temperature_celsius
            .map(|value| MetricValue::Text(format!("{value:.1} C")))
            .unwrap_or(MetricValue::Unavailable),
    ));
    snapshot.graph_points.push((
        "Usage".to_string(),
        gpu.utilization_percent.unwrap_or(0.0).clamp(0.0, 100.0),
    ));
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    // Characterization tests: pin GPU snapshot layout ahead of the
    // deduplication of the NVIDIA/AMD/unavailable branches.

    fn info(
        utilization: impl Into<Option<f64>>,
        power: impl Into<Option<f64>>,
        temperature: impl Into<Option<f64>>,
    ) -> NvidiaGpuInfo {
        NvidiaGpuInfo {
            index: 2,
            utilization_percent: utilization.into(),
            power_watts: power.into(),
            temperature_celsius: temperature.into(),
        }
    }

    #[test]
    fn nvidia_snapshot_layout() {
        let snapshot = nvidia_snapshot(&info(42.0, 112.345, 61.4));
        assert_eq!(snapshot.title, "GPU");
        assert_eq!(snapshot.subtitle, Some("NVIDIA GPU 2".to_string()));
        let names: Vec<&str> = snapshot.metrics.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["Usage", "GPU power", "Temperature"]);
        assert_eq!(snapshot.metrics[0].value, MetricValue::Percent(42.0));
        assert_eq!(snapshot.metrics[1].value, MetricValue::Watts(112.345));
        assert_eq!(
            snapshot.metrics[2].value,
            MetricValue::Text("61.4 C".to_string())
        );
        assert_eq!(snapshot.graph_points, vec![("Usage".to_string(), 42.0)]);
    }

    #[test]
    fn nvidia_snapshot_partially_unavailable() {
        let snapshot = nvidia_snapshot(&info(None, Some(80.0), None));
        assert_eq!(snapshot.metrics[0].value, MetricValue::Unavailable);
        assert_eq!(snapshot.metrics[1].value, MetricValue::Watts(80.0));
        assert_eq!(snapshot.metrics[2].value, MetricValue::Unavailable);
        assert_eq!(snapshot.graph_points, vec![("Usage".to_string(), 0.0)]);
    }

    #[test]
    fn graph_utilization_clamps_above_100() {
        let snapshot = nvidia_snapshot(&info(150.0, None, None));
        assert_eq!(snapshot.graph_points, vec![("Usage".to_string(), 100.0)]);
    }

    #[test]
    fn temperature_format_is_one_decimal_celsius() {
        let snapshot = nvidia_snapshot(&info(None, None, Some(9.25)));
        assert_eq!(
            snapshot.metrics[2].value,
            MetricValue::Text("9.2 C".to_string())
        );
    }
}
