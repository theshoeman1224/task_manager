use crate::config::AppConfig;
use crate::core::MonitorSource;

pub mod cpu;
pub mod gpu;
pub mod network;
pub mod power;

pub use cpu::CpuMonitor;
pub use gpu::GpuMonitor;
pub use network::NetworkMonitor;
pub use power::PowerMonitor;

/// Single point where the application's monitors are registered. The UI
/// derives both the notebook tabs and the sampler threads from this list, so
/// adding a monitor means implementing MonitorSource and appending one entry
/// (it used to require edits in four places).
///
/// Tab titles come from `MonitorSource::name()`, the CPU/GPU/Network/Power
/// column headings stay in the order below.
pub fn build_sources(config: &AppConfig) -> Vec<Box<dyn MonitorSource>> {
    vec![
        Box::new(CpuMonitor::new()),
        Box::new(GpuMonitor::new()),
        Box::new(NetworkMonitor::new(
            config.default_network_interface.clone(),
        )),
        Box::new(PowerMonitor::new()),
    ]
}
