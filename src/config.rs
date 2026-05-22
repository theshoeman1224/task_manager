use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub sample_interval: Duration,
    pub graph_history_points: usize,
    pub default_network_interface: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            sample_interval: Duration::from_secs(1),
            graph_history_points: 60,
            default_network_interface: None,
        }
    }
}
