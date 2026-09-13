// Application bootstrap: GTK app setup, window, notebook tabs, and the
// update-frequency control. The wiring lives here only; widgets live in
// tabs.rs, graph drawing in graph.rs, sampler threads in sampler.rs.

mod graph;
mod sampler;
mod tabs;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Label, Notebook, Orientation, SpinButton,
};

use crate::config::AppConfig;
use crate::monitors::build_sources;
use sampler::start_samplers;
use tabs::MonitorTab;

const APP_ID: &str = "dev.codex.LinuxTaskManager";

pub fn run() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let config = AppConfig::default();
    let update_interval_ms = Arc::new(AtomicU64::new(
        config
            .sample_interval
            .as_millis()
            .try_into()
            .unwrap_or(1000),
    ));
    // One registry drives both the tabs and the sampler threads; the tab
    // titles are nothing more than each source's name().
    let sources = build_sources(&config);
    let tab_titles: Vec<&'static str> = sources.iter().map(|source| source.name()).collect();
    let snapshots = start_samplers(sources, Arc::clone(&update_interval_ms));

    let notebook = Notebook::new();
    let tabs = Rc::new(RefCell::new(HashMap::new()));
    for title in tab_titles {
        let tab = MonitorTab::new(title, config.graph_history_points);
        notebook.append_page(&tab.root, Some(&Label::new(Some(title))));
        tabs.borrow_mut().insert(title.to_string(), tab);
    }

    let app_root = GtkBox::new(Orientation::Vertical, 8);
    app_root.append(&build_update_frequency_control(&update_interval_ms));
    app_root.append(&notebook);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Linux Task Manager")
        .default_width(900)
        .default_height(620)
        .child(&app_root)
        .build();

    let tabs_for_timer = Rc::clone(&tabs);
    let source_id = glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
        for snapshot in snapshots.try_iter() {
            if let Some(tab) = tabs_for_timer.borrow_mut().get_mut(&snapshot.title) {
                tab.update(&snapshot);
            }
        }
        glib::ControlFlow::Continue
    });
    std::mem::forget(source_id);

    window.present();
}

fn build_update_frequency_control(update_interval_ms: &Arc<AtomicU64>) -> GtkBox {
    let controls = GtkBox::new(Orientation::Horizontal, 8);
    controls.set_margin_top(8);
    controls.set_margin_bottom(0);
    controls.set_margin_start(12);
    controls.set_margin_end(12);

    let label = Label::new(Some("Update frequency"));
    label.set_xalign(0.0);
    controls.append(&label);

    let spin = SpinButton::with_range(0.25, 10.0, 0.25);
    spin.set_digits(2);
    spin.set_value(update_interval_ms.load(Ordering::Relaxed) as f64 / 1000.0);
    spin.set_tooltip_text(Some("Seconds between telemetry samples"));
    let update_interval_ms = Arc::clone(update_interval_ms);
    spin.connect_value_changed(move |spin| {
        let seconds = spin.value().clamp(0.25, 10.0);
        update_interval_ms.store((seconds * 1000.0).round() as u64, Ordering::Relaxed);
    });
    controls.append(&spin);

    let seconds = Label::new(Some("seconds"));
    seconds.set_xalign(0.0);
    controls.append(&seconds);

    controls
}
