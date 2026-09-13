// NVIDIA telemetry via the dlopen'd NVML C library. Everything here is raw
// FFI: loading the shared object, resolving symbols, calling the stable
// NVML C API, and shutting the library handle down.

use std::io;
use std::os::raw::{c_char, c_uint, c_void};
use std::path::Path;

pub fn nvidia_devices_present(sys_root: &Path, dev_root: &Path) -> bool {
    sys_root.join("driver/nvidia/gpus").exists() || dev_root.join("nvidiactl").exists()
}

#[derive(Debug, Clone, PartialEq)]
pub struct NvidiaGpuInfo {
    pub index: u32,
    pub utilization_percent: Option<f64>,
    pub power_watts: Option<f64>,
    pub temperature_celsius: Option<f64>,
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
