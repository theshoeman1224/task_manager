// sysfs power sources: powercap energy counters and power supplies.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::sysfs::{read_micro_value, read_trimmed, read_u64};

#[derive(Debug, Clone, PartialEq)]
pub struct EnergyCounter {
    pub path: PathBuf,
    pub name: String,
    pub energy_uj: u64,
}

pub fn read_powercap_energy_counters(root: &Path) -> io::Result<Vec<EnergyCounter>> {
    let mut counters = Vec::new();
    collect_powercap_energy_counters(root, &mut counters)?;
    Ok(counters)
}

fn collect_powercap_energy_counters(
    root: &Path,
    counters: &mut Vec<EnergyCounter>,
) -> io::Result<()> {
    // Sysfs class entries are symlinks and, once followed, expose symlinked
    // back-references (`device`, `subsystem`, ...) that form cycles. Walking
    // without resolving them means unbounded recursion (observed as a
    // multi-second runaway that eventually dies on ELOOP). Canonicalize the
    // root once and then only descend into real directories, never symlink
    // entries. On real systems every energy zone is a real directory below
    // the canonicalized root, so the set of counters reported is unchanged.
    let Ok(real_root) = root.canonicalize() else {
        return Ok(());
    };

    let mut stack = vec![real_root];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }

            let path = entry.path();
            let energy_path = path.join("energy_uj");
            if energy_path.exists() {
                if let Ok(energy_uj) = read_u64(&energy_path) {
                    let name = fs::read_to_string(path.join("name"))
                        .unwrap_or_else(|_| "powercap".to_string())
                        .trim()
                        .to_string();
                    counters.push(EnergyCounter {
                        path: path.clone(),
                        name,
                        energy_uj,
                    });
                }
            }

            stack.push(path);
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerSupply {
    pub name: String,
    pub kind: String,
    pub power_watts: Option<f64>,
    pub capacity_percent: Option<u64>,
}

pub fn read_power_supplies(root: &Path) -> io::Result<Vec<PowerSupply>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut supplies = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let kind = read_trimmed(path.join("type")).unwrap_or_else(|| "Unknown".to_string());
        let power_watts = read_micro_value(path.join("power_now")).or_else(|| {
            read_micro_value(path.join("current_now"))
                .zip(read_micro_value(path.join("voltage_now")))
                .map(|(amps, volts)| amps * volts)
        });
        let capacity_percent = read_u64(&path.join("capacity")).ok();

        supplies.push(PowerSupply {
            name,
            kind,
            power_watts,
            capacity_percent,
        });
    }

    Ok(supplies)
}
