use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum MetricValue {
    Percent(f64),
    Watts(f64),
    BytesPerSecond(f64),
    Bytes(u64),
    Count(u64),
    Text(String),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub name: String,
    pub value: MetricValue,
}

impl Metric {
    pub fn new(name: impl Into<String>, value: MetricValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricSnapshot {
    pub title: String,
    pub subtitle: Option<String>,
    pub metrics: Vec<Metric>,
    pub graph_points: Vec<(String, f64)>,
}

impl MetricSnapshot {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            metrics: Vec::new(),
            graph_points: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorError {
    message: String,
}

impl MonitorError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MonitorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MonitorError {}

pub trait MonitorSource: Send {
    fn name(&self) -> &'static str;
    fn sample(&mut self) -> Result<MetricSnapshot, MonitorError>;
}
