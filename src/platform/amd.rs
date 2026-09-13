// AMD GPU telemetry from the DRM sysfs tree (utilization/power/temperature
// with hwmon fallback).

use std::fs;
use std::io;
use std::path::Path;

use super::sysfs::{read_micro_value, read_milli_value, read_u64};

#[derive(Debug, Clone, PartialEq)]
pub struct AmdGpuInfo {
    pub card: String,
    pub utilization_percent: Option<f64>,
    pub power_watts: Option<f64>,
    pub temperature_celsius: Option<f64>,
}

pub fn read_amd_gpus(drm_root: &Path) -> io::Result<Vec<AmdGpuInfo>> {
    if !drm_root.exists() {
        return Ok(Vec::new());
    }

    let mut gpus = Vec::new();
    for entry in fs::read_dir(drm_root)? {
        let entry = entry?;
        let path = entry.path();
        let card = entry.file_name().to_string_lossy().to_string();
        if !card.starts_with("card") || !path.join("device").is_dir() {
            continue;
        }

        let device = path.join("device");
        let utilization_percent = read_u64(&device.join("gpu_busy_percent"))
            .ok()
            .map(|v| v as f64);
        let mut power_watts = read_micro_value(device.join("power1_average"));
        let mut temperature_celsius = read_milli_value(device.join("temp1_input"));

        if power_watts.is_none() || temperature_celsius.is_none() {
            for hwmon in fs::read_dir(device.join("hwmon"))
                .into_iter()
                .flatten()
                .flatten()
            {
                let hwmon_path = hwmon.path();
                power_watts =
                    power_watts.or_else(|| read_micro_value(hwmon_path.join("power1_average")));
                temperature_celsius = temperature_celsius
                    .or_else(|| read_milli_value(hwmon_path.join("temp1_input")));
            }
        }

        if utilization_percent.is_some() || power_watts.is_some() || temperature_celsius.is_some() {
            gpus.push(AmdGpuInfo {
                card,
                utilization_percent,
                power_watts,
                temperature_celsius,
            });
        }
    }

    Ok(gpus)
}
