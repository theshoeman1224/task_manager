use crate::core::{Metric, MetricSnapshot, MetricValue, MonitorError, MonitorSource};
use crate::platform::linux::{
    nvidia_devices_present, read_amd_gpus, NvidiaGpuInfo, NvidiaGpuReader,
};

pub struct GpuMonitor {
    nvidia: Option<NvidiaGpuReader>,
    nvidia_error: Option<String>,
}

impl GpuMonitor {
    pub fn new() -> Self {
        let (nvidia, nvidia_error) = if nvidia_devices_present() {
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

        let amd_gpus = read_amd_gpus(std::path::Path::new("/sys/class/drm"))
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

        if nvidia_devices_present() {
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
