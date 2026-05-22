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
