use super::metrics::MetricValue;

pub fn format_percent(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.1}%")
    } else {
        "Unavailable".to_string()
    }
}

pub fn format_watts(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.2} W")
    } else {
        "Unavailable".to_string()
    }
}

pub fn format_bytes_per_sec(value: f64) -> String {
    format_scaled_bytes(value, "/s")
}

pub fn format_bytes(value: f64) -> String {
    format_scaled_bytes(value, "")
}

fn format_scaled_bytes(value: f64, suffix: &str) -> String {
    if !value.is_finite() {
        return "Unavailable".to_string();
    }

    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut scaled = value.max(0.0);
    let mut unit = 0;
    while scaled >= 1000.0 && unit < UNITS.len() - 1 {
        scaled /= 1000.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{scaled:.0} {}{suffix}", UNITS[unit])
    } else {
        format!("{scaled:.1} {}{suffix}", UNITS[unit])
    }
}

pub fn format_metric_value(value: &MetricValue) -> String {
    match value {
        MetricValue::Percent(value) => format_percent(*value),
        MetricValue::Watts(value) => format_watts(*value),
        MetricValue::BytesPerSecond(value) => format_bytes_per_sec(*value),
        MetricValue::Bytes(value) => format_bytes(*value as f64),
        MetricValue::Count(value) => value.to_string(),
        MetricValue::Text(value) => value.clone(),
        MetricValue::Unavailable => "Unavailable".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Characterization tests: pin current formatting behavior ahead of the
    // refactor. See docs/baseline-history.md (formerly BASELINE.md). Any change here is an output change.

    #[test]
    fn formats_non_finite_as_unavailable() {
        assert_eq!(format_percent(f64::NAN), "Unavailable");
        assert_eq!(format_percent(f64::INFINITY), "Unavailable");
        assert_eq!(format_watts(f64::NAN), "Unavailable");
        assert_eq!(format_bytes_per_sec(f64::NAN), "Unavailable");
        assert_eq!(format_bytes(f64::NAN), "Unavailable");
    }

    #[test]
    fn clamps_negative_bytes_to_zero() {
        // format_scaled_bytes applies .max(0.0) before scaling.
        assert_eq!(format_bytes(-1.0), "0 B");
        assert_eq!(format_bytes_per_sec(-0.5), "0 B/s");
    }

    #[test]
    fn scales_bytes_in_three_digit_steps() {
        // 1000-based, not 1024-based. This is a landmine; pin it.
        assert_eq!(format_bytes(1024.0), "1.0 KB");
        assert_eq!(format_bytes(1_000_000.0), "1.0 MB");
        assert_eq!(format_bytes(1_000_000_000.0), "1.0 GB");
        assert_eq!(format_bytes(1_000_000_000_000.0), "1.0 TB");
        assert_eq!(format_bytes(1e15), "1.0 PB");
        // Beyond the unit list it stops scaling at PB.
        assert_eq!(format_bytes(1e18), "1000.0 PB");
    }

    #[test]
    fn bytes_per_second_keeps_rate_suffix() {
        assert_eq!(format_bytes_per_sec(0.0), "0 B/s");
        assert_eq!(format_bytes_per_sec(12_345_678.0), "12.3 MB/s");
    }

    #[test]
    fn formats_metric_value_variants() {
        assert_eq!(format_metric_value(&MetricValue::Count(42)), "42");
        assert_eq!(
            format_metric_value(&MetricValue::Text("hello".to_string())),
            "hello"
        );
        assert_eq!(format_metric_value(&MetricValue::Bytes(2048)), "2.0 KB");
        assert_eq!(format_metric_value(&MetricValue::Percent(3.21)), "3.2%");
    }

    #[test]
    fn formats_units() {
        assert_eq!(format_percent(12.345), "12.3%");
        assert_eq!(format_watts(4.5), "4.50 W");
        assert_eq!(format_bytes(999.0), "999 B");
        assert_eq!(format_bytes(1000.0), "1.0 KB");
        assert_eq!(format_bytes(999_999.0), "1000.0 KB");
        assert_eq!(format_bytes(1_000_000.0), "1.0 MB");
        assert_eq!(format_bytes_per_sec(2048.0), "2.0 KB/s");
        assert_eq!(
            format_metric_value(&MetricValue::Unavailable),
            "Unavailable"
        );
    }
}
