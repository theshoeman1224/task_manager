// Facade over the platform modules. Existing call sites still reach
// everything through `platform::linux::...`; the concrete implementations
// live in sibling modules split by source (procfs, sysfs power, AMD sysfs,
// NVML FFI). This file only re-exports; it must contain no logic, so the
// split can never drift from behavior.

pub use super::{
    amd::{read_amd_gpus, AmdGpuInfo},
    nvidia::{nvidia_devices_present, read_nvidia_gpus, NvidiaGpuInfo, NvidiaGpuReader},
    power::{read_power_supplies, read_powercap_energy_counters, EnergyCounter, PowerSupply},
    proc::{
        cpu_usage_percent, parse_proc_net_dev, parse_proc_stat_cpu, read_cpu_times,
        read_network_counters, CpuTimes, NetworkCounters,
    },
};

#[cfg(test)]
mod tests {
    // Smoke test: the facade exposes the full surface, matching the
    // pre-split public API. Compilation of these references is the test.
    #[allow(unused_imports)]
    use super::{
        cpu_usage_percent, nvidia_devices_present, parse_proc_net_dev, parse_proc_stat_cpu,
        read_amd_gpus, read_cpu_times, read_network_counters, read_nvidia_gpus,
        read_power_supplies, read_powercap_energy_counters, AmdGpuInfo, CpuTimes,
        EnergyCounter, NetworkCounters, NvidiaGpuInfo, NvidiaGpuReader, PowerSupply,
    };
}
