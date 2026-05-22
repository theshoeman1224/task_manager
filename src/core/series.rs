use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct MetricSeries {
    capacity: usize,
    points: VecDeque<f64>,
}

impl MetricSeries {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            points: VecDeque::new(),
        }
    }

    pub fn push(&mut self, value: f64) {
        if self.points.len() == self.capacity {
            self.points.pop_front();
        }
        self.points.push_back(value);
    }

    pub fn values(&self) -> Vec<f64> {
        self.points.iter().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::MetricSeries;

    #[test]
    fn keeps_bounded_history() {
        let mut series = MetricSeries::new(3);
        series.push(1.0);
        series.push(2.0);
        series.push(3.0);
        series.push(4.0);

        assert_eq!(series.values(), vec![2.0, 3.0, 4.0]);
    }
}
