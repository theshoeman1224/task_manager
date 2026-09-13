// Graph rendering for metric rows: pure value/scale mapping plus the cairo
// draw function. Kept free of GTK widget construction so the mapping rules
// have characterization tests (docs/baseline-history.md (formerly BASELINE.md)).

use std::cell::RefCell;
use std::rc::Rc;

use crate::core::{format_bytes, format_bytes_per_sec, format_percent, format_watts, MetricValue};

pub(crate) type SharedGraphState = Rc<RefCell<GraphState>>;

#[derive(Debug, Default)]
pub(crate) struct GraphState {
    pub values: Vec<f64>,
    pub max_value: f64,
}

pub(crate) fn graph_value(value: &MetricValue) -> Option<f64> {
    match value {
        MetricValue::Percent(value)
        | MetricValue::Watts(value)
        | MetricValue::BytesPerSecond(value) => value.is_finite().then_some(*value),
        MetricValue::Bytes(value) | MetricValue::Count(value) => Some(*value as f64),
        MetricValue::Text(_) | MetricValue::Unavailable => None,
    }
}

pub(crate) fn graph_max(value: &MetricValue, values: &[f64]) -> f64 {
    let observed_max = values.iter().copied().fold(0.0_f64, f64::max);
    match value {
        MetricValue::Percent(_) => 100.0,
        MetricValue::Watts(_) => observed_max.max(1.0).ceil(),
        MetricValue::BytesPerSecond(_) => observed_max.max(1.0),
        MetricValue::Bytes(_) | MetricValue::Count(_) => observed_max.max(1.0),
        MetricValue::Text(_) | MetricValue::Unavailable => 1.0,
    }
}

pub(crate) fn scale_text(value: &MetricValue, max_value: f64) -> String {
    match value {
        MetricValue::Percent(_) => format!("Scale: 0 - {}", format_percent(100.0)),
        MetricValue::Watts(_) => format!("Scale: 0 - {}", format_watts(max_value)),
        MetricValue::BytesPerSecond(_) => format!("Scale: 0 - {}", format_bytes_per_sec(max_value)),
        MetricValue::Bytes(_) => format!("Scale: 0 - {}", format_bytes(max_value)),
        MetricValue::Count(_) => format!("Scale: 0 - {:.0}", max_value),
        MetricValue::Text(_) | MetricValue::Unavailable => "Scale: unavailable".to_string(),
    }
}

pub(crate) fn draw_graph(
    cr: &gtk4::cairo::Context,
    width: f64,
    height: f64,
    values: &[f64],
    max_value: f64,
) {
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
    // MetricRow before the ui split. Pure functions only; no GTK state
    // needed. See docs/baseline-history.md (formerly BASELINE.md).

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
        assert_eq!(graph_max(&MetricValue::BytesPerSecond(4.5), &values), 500.0);
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
        assert_eq!(scale_text(&MetricValue::Bytes(0), 10.0), "Scale: 0 - 10 B");
        assert_eq!(scale_text(&MetricValue::Count(0), 7.0), "Scale: 0 - 7");
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
