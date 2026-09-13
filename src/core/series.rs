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

    // Characterization tests: pin series behavior ahead of the refactor.

    #[test]
    fn keeps_bounded_history() {
        let mut series = MetricSeries::new(3);
        series.push(1.0);
        series.push(2.0);
        series.push(3.0);
        series.push(4.0);

        assert_eq!(series.values(), vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn capacity_of_zero_is_promoted_to_one() {
        // new() applies .max(1): the smallest real capacity is 1.
        let mut series = MetricSeries::new(0);
        series.push(1.0);
        series.push(2.0);
        assert_eq!(series.values(), vec![2.0]);
        assert_eq!(series.len(), 1);
    }

    #[test]
    fn preserves_insertion_order() {
        let mut series = MetricSeries::new(5);
        series.push(5.0);
        series.push(1.0);
        series.push(3.0);
        assert_eq!(series.values(), vec![5.0, 1.0, 3.0]);
    }

    #[test]
    fn fresh_series_is_empty() {
        let series = MetricSeries::new(4);
        assert!(series.is_empty());
        assert_eq!(series.len(), 0);
        assert!(series.values().is_empty());
    }

    #[test]
    fn keeps_last_capacity_points_not_largest_values() {
        // Eviction is FIFO on insertion, not value-based.
        let mut series = MetricSeries::new(2);
        series.push(100.0);
        series.push(0.0);
        series.push(50.0);
        assert_eq!(series.values(), vec![0.0, 50.0]);
    }
}
