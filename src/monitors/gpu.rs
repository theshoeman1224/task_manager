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
    /// Set when an NVIDIA device was detected but NVML could not be loaded
    /// or answered with an error. Failure details feed the fallback
    /// branch's subtitle; typed as MonitorError so every monitor reports
    /// failure the same way.
    nvidia_error: Option<MonitorError>,
}

// Monitors have no Default use sites; clippy's suggestion to pair each
// new() with a Default impl would just re-create the boilerplate removed
// in this phase.
#[allow(clippy::new_without_default)]
impl GpuMonitor {
    pub fn new() -> Self {
        let (nvidia, nvidia_error) =
            if nvidia_devices_present(Path::new(PROC_ROOT), Path::new(DEV_ROOT)) {
                match NvidiaGpuReader::new() {
                    Ok(reader) => (Some(reader), None),
                    Err(err) => (None, Some(MonitorError::new(err.to_string()))),
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
                    self.nvidia_error = Some(MonitorError::new(err.to_string()));
                }
            }
        }

        let amd_gpus = read_amd_gpus(Path::new(&format!("{SYS_ROOT}/class/drm")))
            .map_err(|err| MonitorError::new(err.to_string()))?;

        if let Some(gpu) = amd_gpus.first() {
            let snapshot = gpu_snapshot(
                Some(format!("AMD {}", gpu.card)),
                gpu.utilization_percent,
                gpu.power_watts,
                gpu.temperature_celsius,
            );
            return Ok(snapshot);
        }

        if nvidia_devices_present(Path::new(PROC_ROOT), Path::new(DEV_ROOT)) {
            let mut snapshot = gpu_snapshot(None, None, None, None);
            snapshot.subtitle = Some(
                self.nvidia_error
                    .as_ref()
                    .map(|err| format!("NVIDIA device detected; NVML unavailable: {err}"))
                    .unwrap_or_else(|| "NVIDIA device detected; NVML unavailable".to_string()),
            );
            return Ok(snapshot);
        }

        let mut snapshot = gpu_snapshot(None, None, None, None);
        snapshot.subtitle = Some("No supported AMD or NVIDIA GPU telemetry found".to_string());
        Ok(snapshot)
    }
}

/// Builds the GPU snapshot's metric row set (Usage / GPU power / Temperature,
/// in that order) from optional f64 readings; `None` at any slot yields
/// Unavailable, exactly as each monitor branch used to build by hand. The
/// fallback branches pass all-None for the metrics and then overwrite the
/// subtitle, matching the duplicated blocks this replaces.
fn gpu_snapshot(
    subtitle: Option<String>,
    usage: Option<f64>,
    power: Option<f64>,
    temperature: Option<f64>,
) -> MetricSnapshot {
    let mut snapshot = MetricSnapshot::new("GPU");
    snapshot.subtitle = subtitle;
    snapshot
        .metrics
        .push(Metric::new("Usage", MetricValue::percentage(usage)));
    snapshot
        .metrics
        .push(Metric::new("GPU power", MetricValue::watts(power)));
    snapshot.metrics.push(Metric::new(
        "Temperature",
        MetricValue::celsius(temperature),
    ));
    snapshot
}

fn nvidia_snapshot(gpu: &NvidiaGpuInfo) -> MetricSnapshot {
    let snapshot = gpu_snapshot(
        Some(format!("NVIDIA GPU {}", gpu.index)),
        gpu.utilization_percent,
        gpu.power_watts,
        gpu.temperature_celsius,
    );
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
    }

    #[test]
    fn nvidia_snapshot_partially_unavailable() {
        let snapshot = nvidia_snapshot(&info(None, Some(80.0), None));
        assert_eq!(snapshot.metrics[0].value, MetricValue::Unavailable);
        assert_eq!(snapshot.metrics[1].value, MetricValue::Watts(80.0));
        assert_eq!(snapshot.metrics[2].value, MetricValue::Unavailable);
    }

    #[test]
    fn usage_over_100_flows_through_metric_value() {
        // The graph_points field used to clamp here; it is gone, so only the
        // metric value exists and it is not clamped.
        let snapshot = nvidia_snapshot(&info(150.0, None, None));
        assert_eq!(snapshot.metrics[0].value, MetricValue::Percent(150.0));
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
