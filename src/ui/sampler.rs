// Sampler threads: one background thread per monitor source pushing
// MetricSnapshots into an mpsc channel. The UI drains this via a glib timer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::config::AppConfig;
use crate::core::{MetricSnapshot, MonitorSource};
use crate::monitors::{CpuMonitor, GpuMonitor, NetworkMonitor, PowerMonitor};

pub(super) const MIN_SAMPLING_INTERVAL_MS: u64 = 250;

pub(super) fn start_samplers(
    config: AppConfig,
    update_interval_ms: Arc<AtomicU64>,
) -> Receiver<MetricSnapshot> {
    let (sender, receiver) = mpsc::channel();
    let sources: Vec<Box<dyn MonitorSource>> = vec![
        Box::new(CpuMonitor::new()),
        Box::new(GpuMonitor::new()),
        Box::new(NetworkMonitor::new(
            config.default_network_interface.clone(),
        )),
        Box::new(PowerMonitor::new()),
    ];

    for source in sources {
        start_monitor_thread(source, sender.clone(), Arc::clone(&update_interval_ms));
    }

    receiver
}

pub(super) fn start_monitor_thread(
    mut source: Box<dyn MonitorSource>,
    sender: Sender<MetricSnapshot>,
    update_interval_ms: Arc<AtomicU64>,
) {
    thread::spawn(move || loop {
        let snapshot = source.sample().unwrap_or_else(|err| {
            let mut snapshot = MetricSnapshot::new(source.name());
            snapshot.subtitle = Some(err.to_string());
            snapshot
        });

        if sender.send(snapshot).is_err() {
            break;
        }

        let interval_ms = update_interval_ms
            .load(Ordering::Relaxed)
            .max(MIN_SAMPLING_INTERVAL_MS);
        thread::sleep(Duration::from_millis(interval_ms));
    });
}
