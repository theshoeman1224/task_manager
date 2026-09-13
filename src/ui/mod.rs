use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::thread;

use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, DrawingArea, Label, Notebook, Orientation,
    ScrolledWindow, SpinButton,
};

use crate::config::AppConfig;
use crate::core::{
    format_bytes, format_bytes_per_sec, format_metric_value, format_percent, format_watts,
};
use crate::core::{Metric, MetricSeries, MetricSnapshot, MetricValue, MonitorSource};
use crate::monitors::{CpuMonitor, GpuMonitor, NetworkMonitor, PowerMonitor};

const APP_ID: &str = "dev.codex.LinuxTaskManager";
const METRIC_ROW_HEIGHT: i32 = 118;

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
    let snapshots = start_samplers(config.clone(), Arc::clone(&update_interval_ms));

    let notebook = Notebook::new();
    let tabs = Rc::new(RefCell::new(HashMap::<String, MonitorTab>::new()));
    for title in ["CPU", "GPU", "Network", "Power"] {
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

fn start_samplers(
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

fn start_monitor_thread(
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

        let interval_ms = update_interval_ms.load(Ordering::Relaxed).max(250);
        thread::sleep(std::time::Duration::from_millis(interval_ms));
    });
}

struct MonitorTab {
    root: GtkBox,
    subtitle: Label,
    rows: GtkBox,
    metric_rows: HashMap<String, MetricRow>,
    history_points: usize,
}

impl MonitorTab {
    fn new(title: &str, history_points: usize) -> Self {
        let root = GtkBox::new(Orientation::Vertical, 16);
        root.set_margin_top(18);
        root.set_margin_bottom(18);
        root.set_margin_start(18);
        root.set_margin_end(18);

        let heading = Label::new(Some(title));
        heading.set_xalign(0.0);
        heading.add_css_class("title-1");
        root.append(&heading);

        let subtitle = Label::new(Some("Waiting for telemetry..."));
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("dim-label");
        root.append(&subtitle);

        let rows = GtkBox::new(Orientation::Vertical, 10);
        let scroller = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&rows)
            .build();
        root.append(&scroller);

        Self {
            root,
            subtitle,
            rows,
            metric_rows: HashMap::new(),
            history_points,
        }
    }

    fn update(&mut self, snapshot: &MetricSnapshot) {
        self.subtitle.set_text(
            snapshot
                .subtitle
                .as_deref()
                .unwrap_or("Telemetry available"),
        );

        for metric in &snapshot.metrics {
            if !self.metric_rows.contains_key(&metric.name) {
                let row = MetricRow::new(metric, self.history_points);
                self.rows.append(&row.root);
                self.metric_rows.insert(metric.name.clone(), row);
            }

            if let Some(row) = self.metric_rows.get_mut(&metric.name) {
                row.update(metric);
            }
        }
    }
}

struct MetricRow {
    root: GtkBox,
    value: Label,
    scale: Label,
    graph: DrawingArea,
    series: MetricSeries,
    graph_state: Rc<RefCell<GraphState>>,
}

#[derive(Debug, Default)]
struct GraphState {
    values: Vec<f64>,
    max_value: f64,
}

impl MetricRow {
    fn new(metric: &Metric, history_points: usize) -> Self {
        let root = GtkBox::new(Orientation::Horizontal, 14);
        root.set_height_request(METRIC_ROW_HEIGHT);
        root.set_hexpand(true);

        let instant = GtkBox::new(Orientation::Vertical, 4);
        instant.set_width_request(220);
        instant.set_height_request(METRIC_ROW_HEIGHT);

        let name = Label::new(Some(&metric.name));
        name.set_xalign(0.0);
        name.add_css_class("heading");
        instant.append(&name);

        let value = Label::new(Some(&format_metric_value(&metric.value)));
        value.set_xalign(0.0);
        value.add_css_class("title-3");
        value.add_css_class("monospace");
        instant.append(&value);

        root.append(&instant);

        let graph_panel = GtkBox::new(Orientation::Vertical, 4);
        graph_panel.set_hexpand(true);
        graph_panel.set_height_request(METRIC_ROW_HEIGHT);

        let scale = Label::new(Some(&scale_text(&metric.value, 1.0)));
        scale.set_xalign(1.0);
        scale.add_css_class("dim-label");
        graph_panel.append(&scale);

        let graph_state = Rc::new(RefCell::new(GraphState::default()));
        let graph_state_for_draw = Rc::clone(&graph_state);
        let graph = DrawingArea::new();
        graph.set_content_height(82);
        graph.set_hexpand(true);
        graph.set_vexpand(true);
        graph.set_draw_func(move |_, cr, width, height| {
            let graph_state = graph_state_for_draw.borrow();
            draw_graph(
                cr,
                width as f64,
                height as f64,
                &graph_state.values,
                graph_state.max_value,
            );
        });
        graph_panel.append(&graph);

        root.append(&graph_panel);

        Self {
            root,
            value,
            scale,
            graph,
            series: MetricSeries::new(history_points),
            graph_state,
        }
    }

    fn update(&mut self, metric: &Metric) {
        self.value.set_text(&format_metric_value(&metric.value));

        let graph_value = graph_value(&metric.value);
        if let Some(graph_value) = graph_value {
            self.series.push(graph_value);
        }

        let values = self.series.values();
        let max_value = graph_max(&metric.value, &values);
        self.scale.set_text(&scale_text(&metric.value, max_value));
        {
            let mut graph_state = self.graph_state.borrow_mut();
            graph_state.values = values;
            graph_state.max_value = max_value;
        }
        self.graph.queue_draw();
    }
}

fn graph_value(value: &MetricValue) -> Option<f64> {
    match value {
        MetricValue::Percent(value)
        | MetricValue::Watts(value)
        | MetricValue::BytesPerSecond(value) => value.is_finite().then_some(*value),
        MetricValue::Bytes(value) | MetricValue::Count(value) => Some(*value as f64),
        MetricValue::Text(_) | MetricValue::Unavailable => None,
    }
}

fn graph_max(value: &MetricValue, values: &[f64]) -> f64 {
    let observed_max = values.iter().copied().fold(0.0_f64, f64::max);
    match value {
        MetricValue::Percent(_) => 100.0,
        MetricValue::Watts(_) => observed_max.max(1.0).ceil(),
        MetricValue::BytesPerSecond(_) => observed_max.max(1.0),
        MetricValue::Bytes(_) | MetricValue::Count(_) => observed_max.max(1.0),
        MetricValue::Text(_) | MetricValue::Unavailable => 1.0,
    }
}

fn scale_text(value: &MetricValue, max_value: f64) -> String {
    match value {
        MetricValue::Percent(_) => format!("Scale: 0 - {}", format_percent(100.0)),
        MetricValue::Watts(_) => format!("Scale: 0 - {}", format_watts(max_value)),
        MetricValue::BytesPerSecond(_) => {
            format!("Scale: 0 - {}", format_bytes_per_sec(max_value))
        }
        MetricValue::Bytes(_) => format!("Scale: 0 - {}", format_bytes(max_value)),
        MetricValue::Count(_) => format!("Scale: 0 - {:.0}", max_value),
        MetricValue::Text(_) | MetricValue::Unavailable => "Scale: unavailable".to_string(),
    }
}

fn draw_graph(cr: &gtk4::cairo::Context, width: f64, height: f64, values: &[f64], max_value: f64) {
    cr.set_source_rgb(0.10, 0.12, 0.14);
    let _ = cr.paint();

    cr.set_source_rgb(0.22, 0.25, 0.28);
    for line in 1..4 {
        let y = height * line as f64 / 4.0;
        cr.move_to(0.0, y);
        cr.line_to(width, y);
    }
    let _ = cr.stroke();

    if values.len() < 2 {
        return;
    }

    cr.set_source_rgb(0.30, 0.76, 0.96);
    cr.set_line_width(2.0);
    let scale = max_value.max(1.0);
    for (point_index, value) in values.iter().enumerate() {
        let x = point_index as f64 / (values.len() - 1) as f64 * width;
        let y = height - (value / scale).clamp(0.0, 1.0) * height;
        if point_index == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    let _ = cr.stroke();
}

#[cfg(test)]
mod tests {
    use super::*;

    // Characterization tests: pin the UI-side value/scale mapping used by
    // MetricRow before the ui/mod.rs split. Pure functions only; no GTK
    // state needed. See BASELINE.md.

    #[test]
    fn graph_value_maps_only_finite_plot_units() {
        assert_eq!(graph_value(&MetricValue::Percent(50.0)), Some(50.0));
        assert_eq!(graph_value(&MetricValue::Watts(4.5)), Some(4.5));
        assert_eq!(
            graph_value(&MetricValue::BytesPerSecond(1024.0)),
            Some(1024.0)
        );
        assert_eq!(graph_value(&MetricValue::Bytes(4096)), Some(4096.0));
        assert_eq!(graph_value(&MetricValue::Count(3)), Some(3.0));
        // Text and Unavailable never feed a graph.
        assert_eq!(graph_value(&MetricValue::Text("61.4 C".to_string())), None);
        assert_eq!(graph_value(&MetricValue::Unavailable), None);
        // Non-finite is dropped rather than plotted.
        assert_eq!(graph_value(&MetricValue::Percent(f64::NAN)), None);
    }

    #[test]
    fn graph_max_pinned_per_unit() {
        let values = [500.0, 200.0];
        assert_eq!(graph_max(&MetricValue::Percent(50.0), &values), 100.0);
        assert_eq!(graph_max(&MetricValue::Watts(4.5), &values), 500.0);
        assert_eq!(
            graph_max(&MetricValue::BytesPerSecond(4.5), &values),
            500.0
        );
        assert_eq!(graph_max(&MetricValue::Bytes(5), &values), 500.0);
        assert_eq!(graph_max(&MetricValue::Text("x".to_string()), &values), 1.0);
        assert_eq!(graph_max(&MetricValue::Unavailable, &[]), 1.0);
    }

    #[test]
    fn graph_max_has_floors() {
        assert_eq!(graph_max(&MetricValue::Watts(0.0), &[0.0]), 1.0);
        // Watts floors to a whole number (ceil), rates do not.
        assert_eq!(graph_max(&MetricValue::Watts(0.0), &[1.5]), 2.0);
        assert_eq!(graph_max(&MetricValue::Watts(0.0), &[2.0]), 2.0);
        assert_eq!(graph_max(&MetricValue::BytesPerSecond(0.0), &[1.5]), 1.5);
    }

    #[test]
    fn scale_text_pinned_per_unit() {
        assert_eq!(
            scale_text(&MetricValue::Percent(0.0), f64::NAN),
            "Scale: 0 - 100.0%"
        );
        assert_eq!(
            scale_text(&MetricValue::Watts(0.0), 3.0),
            "Scale: 0 - 3.00 W"
        );
        assert_eq!(
            scale_text(&MetricValue::BytesPerSecond(0.0), 1e6),
            "Scale: 0 - 1.0 MB/s"
        );
        assert_eq!(
            scale_text(&MetricValue::Bytes(0), 10.0),
            "Scale: 0 - 10 B"
        );
        assert_eq!(
            scale_text(&MetricValue::Count(0), 7.0),
            "Scale: 0 - 7"
        );
        assert_eq!(
            scale_text(&MetricValue::Text("x".to_string()), 7.0),
            "Scale: unavailable"
        );
        assert_eq!(
            scale_text(&MetricValue::Unavailable, 7.0),
            "Scale: unavailable"
        );
    }
}
