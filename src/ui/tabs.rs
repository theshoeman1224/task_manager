// Tab and metric-row widgets for the notebook pages.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Box as GtkBox, DrawingArea, Label, Orientation, ScrolledWindow};

use crate::core::{format_metric_value, Metric, MetricSeries, MetricSnapshot};
use crate::ui::graph::{draw_graph, graph_max, graph_value, scale_text, GraphState, SharedGraphState};

pub(crate) const METRIC_ROW_HEIGHT: i32 = 118;

pub(crate) struct MonitorTab {
    pub(crate) root: GtkBox,
    subtitle: Label,
    rows: GtkBox,
    metric_rows: HashMap<String, MetricRow>,
    history_points: usize,
}

impl MonitorTab {
    pub(crate) fn new(title: &str, history_points: usize) -> Self {
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

    pub(crate) fn update(&mut self, snapshot: &MetricSnapshot) {
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
    graph_state: SharedGraphState,
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

        let graph_state: SharedGraphState = Rc::new(RefCell::new(GraphState::default()));
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
