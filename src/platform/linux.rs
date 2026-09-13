use std::fs;
use std::io;
use std::os::raw::{c_char, c_uint, c_void};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub idle: u64,
    pub total: u64,
}

pub fn parse_proc_stat_cpu(line: &str) -> Option<CpuTimes> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }

    let values: Vec<u64> = parts.filter_map(|part| part.parse::<u64>().ok()).collect();
    if values.len() < 4 {
        return None;
    }

    let idle = values.get(3).copied().unwrap_or(0) + values.get(4).copied().unwrap_or(0);
    let total = values.iter().sum();
    Some(CpuTimes { idle, total })
}

pub fn cpu_usage_percent(previous: CpuTimes, current: CpuTimes) -> Option<f64> {
    let total_delta = current.total.checked_sub(previous.total)?;
    if total_delta == 0 {
        return None;
    }

    let idle_delta = current.idle.saturating_sub(previous.idle);
    Some(((total_delta.saturating_sub(idle_delta)) as f64 / total_delta as f64) * 100.0)
}

pub fn read_cpu_times() -> io::Result<CpuTimes> {
    let contents = fs::read_to_string("/proc/stat")?;
    contents
        .lines()
        .find_map(parse_proc_stat_cpu)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing aggregate cpu line"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkCounters {
    pub interface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

pub fn parse_proc_net_dev(contents: &str) -> Vec<NetworkCounters> {
    contents
        .lines()
        .skip(2)
        .filter_map(|line| {
            let (interface, counters) = line.split_once(':')?;
            let values: Vec<u64> = counters
                .split_whitespace()
                .filter_map(|value| value.parse::<u64>().ok())
                .collect();
            Some(NetworkCounters {
                interface: interface.trim().to_string(),
                rx_bytes: *values.get(0)?,
                tx_bytes: *values.get(8)?,
            })
        })
        .collect()
}

pub fn read_network_counters() -> io::Result<Vec<NetworkCounters>> {
    let contents = fs::read_to_string("/proc/net/dev")?;
    Ok(parse_proc_net_dev(&contents))
}

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

pub fn nvidia_devices_present() -> bool {
    Path::new("/proc/driver/nvidia/gpus").exists() || Path::new("/dev/nvidiactl").exists()
}

#[derive(Debug, Clone, PartialEq)]
pub struct NvidiaGpuInfo {
    pub index: u32,
    pub utilization_percent: Option<f64>,
    pub power_watts: Option<f64>,
    pub temperature_celsius: Option<f64>,
}

pub fn read_nvidia_gpus() -> io::Result<Vec<NvidiaGpuInfo>> {
    let nvml = NvmlLibrary::load()?;
    nvml.query()
}

pub struct NvidiaGpuReader {
    nvml: NvmlLibrary,
}

impl NvidiaGpuReader {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            nvml: NvmlLibrary::load()?,
        })
    }

    pub fn query(&self) -> io::Result<Vec<NvidiaGpuInfo>> {
        self.nvml.query()
    }
}

fn read_u64(path: &Path) -> io::Result<u64> {
    let contents = fs::read_to_string(path)?;
    contents
        .trim()
        .parse::<u64>()
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn read_micro_value(path: PathBuf) -> Option<f64> {
    read_u64(&path).ok().map(|value| value as f64 / 1_000_000.0)
}

fn read_milli_value(path: PathBuf) -> Option<f64> {
    read_u64(&path).ok().map(|value| value as f64 / 1_000.0)
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

type NvmlReturn = c_uint;
type NvmlDevice = *mut c_void;

const RTLD_LAZY: i32 = 1;
const NVML_SUCCESS: NvmlReturn = 0;
const NVML_TEMPERATURE_GPU: c_uint = 0;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct NvmlUtilization {
    gpu: c_uint,
    memory: c_uint,
}

type NvmlInit = unsafe extern "C" fn() -> NvmlReturn;
type NvmlShutdown = unsafe extern "C" fn() -> NvmlReturn;
type NvmlDeviceGetCount = unsafe extern "C" fn(*mut c_uint) -> NvmlReturn;
type NvmlDeviceGetHandleByIndex = unsafe extern "C" fn(c_uint, *mut NvmlDevice) -> NvmlReturn;
type NvmlDeviceGetUtilizationRates =
    unsafe extern "C" fn(NvmlDevice, *mut NvmlUtilization) -> NvmlReturn;
type NvmlDeviceGetPowerUsage = unsafe extern "C" fn(NvmlDevice, *mut c_uint) -> NvmlReturn;
type NvmlDeviceGetTemperature = unsafe extern "C" fn(NvmlDevice, c_uint, *mut c_uint) -> NvmlReturn;

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
}

struct NvmlLibrary {
    handle: *mut c_void,
    shutdown: NvmlShutdown,
    device_get_count: NvmlDeviceGetCount,
    device_get_handle_by_index: NvmlDeviceGetHandleByIndex,
    device_get_utilization_rates: NvmlDeviceGetUtilizationRates,
    device_get_power_usage: NvmlDeviceGetPowerUsage,
    device_get_temperature: NvmlDeviceGetTemperature,
}

unsafe impl Send for NvmlLibrary {}

impl NvmlLibrary {
    fn load() -> io::Result<Self> {
        let names = [
            b"libnvidia-ml.so.1\0".as_ptr().cast::<c_char>(),
            b"libnvidia-ml.so\0".as_ptr().cast::<c_char>(),
        ];

        let handle = names
            .iter()
            .find_map(|name| {
                let handle = unsafe { dlopen(*name, RTLD_LAZY) };
                (!handle.is_null()).then_some(handle)
            })
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "NVML library not found"))?;

        let init: NvmlInit = unsafe { load_symbol(handle, b"nvmlInit_v2\0")? };
        let library = Self {
            handle,
            shutdown: unsafe { load_symbol(handle, b"nvmlShutdown\0")? },
            device_get_count: unsafe { load_symbol(handle, b"nvmlDeviceGetCount_v2\0")? },
            device_get_handle_by_index: unsafe {
                load_symbol(handle, b"nvmlDeviceGetHandleByIndex_v2\0")?
            },
            device_get_utilization_rates: unsafe {
                load_symbol(handle, b"nvmlDeviceGetUtilizationRates\0")?
            },
            device_get_power_usage: unsafe { load_symbol(handle, b"nvmlDeviceGetPowerUsage\0")? },
            device_get_temperature: unsafe { load_symbol(handle, b"nvmlDeviceGetTemperature\0")? },
        };

        let init_result = unsafe { init() };
        if init_result != NVML_SUCCESS {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("NVML initialization failed with code {init_result}"),
            ));
        }

        Ok(library)
    }

    fn query(&self) -> io::Result<Vec<NvidiaGpuInfo>> {
        let mut count = 0;
        let count_result = unsafe { (self.device_get_count)(&mut count) };
        if count_result != NVML_SUCCESS {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("NVML device count failed with code {count_result}"),
            ));
        }

        let mut gpus = Vec::new();
        for index in 0..count {
            let mut device = std::ptr::null_mut();
            let handle_result = unsafe { (self.device_get_handle_by_index)(index, &mut device) };
            if handle_result != NVML_SUCCESS || device.is_null() {
                continue;
            }

            let mut utilization = NvmlUtilization { gpu: 0, memory: 0 };
            let utilization_percent =
                (unsafe { (self.device_get_utilization_rates)(device, &mut utilization) }
                    == NVML_SUCCESS)
                    .then_some(utilization.gpu as f64);

            let mut power_milliwatts = 0;
            let power_watts =
                (unsafe { (self.device_get_power_usage)(device, &mut power_milliwatts) }
                    == NVML_SUCCESS)
                    .then_some(power_milliwatts as f64 / 1000.0);

            let mut temperature = 0;
            let temperature_celsius = (unsafe {
                (self.device_get_temperature)(device, NVML_TEMPERATURE_GPU, &mut temperature)
            } == NVML_SUCCESS)
                .then_some(temperature as f64);

            gpus.push(NvidiaGpuInfo {
                index,
                utilization_percent,
                power_watts,
                temperature_celsius,
            });
        }

        Ok(gpus)
    }
}

impl Drop for NvmlLibrary {
    fn drop(&mut self) {
        unsafe {
            let _ = (self.shutdown)();
            let _ = dlclose(self.handle);
        }
    }
}

unsafe fn load_symbol<T: Copy>(handle: *mut c_void, name: &[u8]) -> io::Result<T> {
    let symbol = dlsym(handle, name.as_ptr().cast::<c_char>());
    if symbol.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "missing NVML symbol {}",
                String::from_utf8_lossy(&name[..name.len().saturating_sub(1)])
            ),
        ));
    }
    Ok(std::mem::transmute_copy(&symbol))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Characterization tests: pin parser/Math behavior ahead of the
    // platform/linux.rs split. See BASELINE.md.

    #[test]
    fn parses_proc_stat_cpu() {
        let times = parse_proc_stat_cpu("cpu  100 0 50 850 10 0 0 0 0 0").unwrap();
        assert_eq!(times.idle, 860);
        assert_eq!(times.total, 1010);
    }

    #[test]
    fn calculates_cpu_usage_delta() {
        let previous = CpuTimes {
            idle: 80,
            total: 100,
        };
        let current = CpuTimes {
            idle: 90,
            total: 200,
        };
        assert_eq!(cpu_usage_percent(previous, current), Some(90.0));
    }

    #[test]
    fn rejects_per_core_stat_lines() {
        // Only "cpu" aggregate lines parse; "cpu0 ..." returns None.
        assert!(parse_proc_stat_cpu("cpu0 100 0 50 850 10 0 0 0 0 0").is_none());
        assert!(parse_proc_stat_cpu("garbage 1 2 3 4").is_none());
    }

    #[test]
    fn rejects_stat_lines_with_fewer_than_four_fields() {
        assert!(parse_proc_stat_cpu("cpu 1 2 3").is_none());
        // Four fields is the minimum.
        assert!(parse_proc_stat_cpu("cpu 1 2 3 4").is_some());
    }

    #[test]
    fn ignores_non_numeric_extra_fields() {
        // Non-numeric fields are filtered out positionally, shifting the
        // remaining values left. 10 0 0 90 0 0 -> values.len() == 6,
        // idle = values[3] + values[4] = 90, total = 100.
        let times = parse_proc_stat_cpu("cpu 10 0 0 90 x y 0 0").unwrap();
        assert_eq!(times.idle, 90);
        assert_eq!(times.total, 100);
    }

    #[test]
    fn usage_is_none_when_no_time_elapses() {
        let previous = CpuTimes { idle: 80, total: 100 };
        let current = CpuTimes { idle: 80, total: 100 };
        assert_eq!(cpu_usage_percent(previous, current), None);
    }

    #[test]
    fn usage_is_none_when_total_counter_goes_backwards() {
        // checked_sub on total_delta: backwards counters yield None, not panic.
        let previous = CpuTimes { idle: 80, total: 100 };
        let current = CpuTimes { idle: 90, total: 50 };
        assert_eq!(cpu_usage_percent(previous, current), None);
    }

    #[test]
    fn usage_clamps_idle_backslide_to_100_percent() {
        // saturating_sub on idle_delta: idle counter resets clamp busy to 100%.
        let previous = CpuTimes { idle: 90, total: 100 };
        let current = CpuTimes { idle: 0, total: 200 };
        assert_eq!(cpu_usage_percent(previous, current), Some(100.0));
    }

    #[test]
    fn idle_only_delta_reports_zero_usage() {
        let previous = CpuTimes { idle: 80, total: 100 };
        let current = CpuTimes { idle: 180, total: 200 };
        assert_eq!(cpu_usage_percent(previous, current), Some(0.0));
    }

    #[test]
    fn parses_proc_net_dev() {
        let contents = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1000 1 0 0 0 0 0 0 2000 2 0 0 0 0 0 0
  eth0: 4096 4 0 0 0 0 0 0 8192 8 0 0 0 0 0 0
";
        let parsed = parse_proc_net_dev(contents);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].interface, "eth0");
        assert_eq!(parsed[1].rx_bytes, 4096);
        assert_eq!(parsed[1].tx_bytes, 8192);
    }

    #[test]
    fn net_dev_skips_malformed_lines() {
        // Lines without a colon are dropped entirely. But non-numeric counter
        // fields are only filtered positionally, not by rejecting the line:
        // "1 2 not-a-number 4..." parses the remaining 9 values as if the
        // gap never existed (rx=1, tx=values[8]=10). Same for short rows
        // with fewer than 9 numeric values being silently dropped.
        let contents = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
bad line without colon
eth floppy: 1 2 not-a-number 4 5 6 7 8 9 10
  eth0: 4096 4 0 0 0 0 0 0 8192 8 0 0 0 0 0 0
";
        let parsed = parse_proc_net_dev(contents);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].interface, "eth floppy");
        assert_eq!(parsed[0].rx_bytes, 1);
        assert_eq!(parsed[0].tx_bytes, 10);
        assert_eq!(parsed[1].interface, "eth0");
    }

    #[test]
    fn net_dev_trims_interface_whitespace() {
        let contents = "\
header one
header two
     eth0    : 1 2 3 4 5 6 7 8 9
";
        let parsed = parse_proc_net_dev(contents);
        assert_eq!(parsed[0].interface, "eth0");
    }

    #[test]
    fn net_dev_counts_exactly_sixteen_fields() {
        // A 16-field row parses; the 18-field `lo`-style row also parses but
        // bytes are read positionally (0 and 8).
        let contents = "\
header one
header two
eth0: 10 1 0 0 0 0 0 0 100 2 0 0 0 0 0 0
eth1: 20 1 0 0 0 0 0 0 0 1 0 0 0 0 0 0
";
        let parsed = parse_proc_net_dev(contents);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].tx_bytes, 100);
        assert_eq!(parsed[1].tx_bytes, 0);
    }
}
