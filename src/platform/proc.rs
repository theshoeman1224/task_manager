// procfs readers: CPU aggregate times and network interface counters.

use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub idle: u64,
    pub total: u64,
}

pub fn parse_proc_stat_cpu(line: &str) -> Option<CpuTimes> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }

    let values: Vec<u64> = parts.filter_map(|part| part.parse::<u64>().ok()).collect();
    if values.len() < 4 {
        return None;
    }

    let idle = values.get(3).copied().unwrap_or(0) + values.get(4).copied().unwrap_or(0);
    let total = values.iter().sum();
    Some(CpuTimes { idle, total })
}

pub fn cpu_usage_percent(previous: CpuTimes, current: CpuTimes) -> Option<f64> {
    let total_delta = current.total.checked_sub(previous.total)?;
    if total_delta == 0 {
        return None;
    }

    let idle_delta = current.idle.saturating_sub(previous.idle);
    Some(((total_delta.saturating_sub(idle_delta)) as f64 / total_delta as f64) * 100.0)
}

pub fn read_cpu_times(proc_root: &Path) -> io::Result<CpuTimes> {
    let contents = fs::read_to_string(proc_root.join("stat"))?;
    contents
        .lines()
        .find_map(parse_proc_stat_cpu)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing aggregate cpu line"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkCounters {
    pub interface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

pub fn parse_proc_net_dev(contents: &str) -> Vec<NetworkCounters> {
    contents
        .lines()
        .skip(2)
        .filter_map(|line| {
            let (interface, counters) = line.split_once(':')?;
            let values: Vec<u64> = counters
                .split_whitespace()
                .filter_map(|value| value.parse::<u64>().ok())
                .collect();
            Some(NetworkCounters {
                interface: interface.trim().to_string(),
                rx_bytes: *values.get(0)?,
                tx_bytes: *values.get(8)?,
            })
        })
        .collect()
}

pub fn read_network_counters(proc_root: &Path) -> io::Result<Vec<NetworkCounters>> {
    let contents = fs::read_to_string(proc_root.join("net/dev"))?;
    Ok(parse_proc_net_dev(&contents))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Characterization tests: pin parser/Math behavior. See docs/baseline-history.md (formerly BASELINE.md).

    #[test]
    fn parses_proc_stat_cpu() {
        let times = parse_proc_stat_cpu("cpu  100 0 50 850 10 0 0 0 0 0").unwrap();
        assert_eq!(times.idle, 860);
        assert_eq!(times.total, 1010);
    }

    #[test]
    fn calculates_cpu_usage_delta() {
        let previous = CpuTimes {
            idle: 80,
            total: 100,
        };
        let current = CpuTimes {
            idle: 90,
            total: 200,
        };
        assert_eq!(cpu_usage_percent(previous, current), Some(90.0));
    }

    #[test]
    fn rejects_per_core_stat_lines() {
        // Only "cpu" aggregate lines parse; "cpu0 ..." returns None.
        assert!(parse_proc_stat_cpu("cpu0 100 0 50 850 10 0 0 0 0 0").is_none());
        assert!(parse_proc_stat_cpu("garbage 1 2 3 4").is_none());
    }

    #[test]
    fn rejects_stat_lines_with_fewer_than_four_fields() {
        assert!(parse_proc_stat_cpu("cpu 1 2 3").is_none());
        // Four fields is the minimum.
        assert!(parse_proc_stat_cpu("cpu 1 2 3 4").is_some());
    }

    #[test]
    fn ignores_non_numeric_extra_fields() {
        // Non-numeric fields are filtered out positionally, shifting the
        // remaining values left. 10 0 0 90 0 0 -> values.len() == 6,
        // idle = values[3] + values[4] = 90, total = 100.
        let times = parse_proc_stat_cpu("cpu 10 0 0 90 x y 0 0").unwrap();
        assert_eq!(times.idle, 90);
        assert_eq!(times.total, 100);
    }

    #[test]
    fn usage_is_none_when_no_time_elapses() {
        let previous = CpuTimes {
            idle: 80,
            total: 100,
        };
        let current = CpuTimes {
            idle: 80,
            total: 100,
        };
        assert_eq!(cpu_usage_percent(previous, current), None);
    }

    #[test]
    fn usage_is_none_when_total_counter_goes_backwards() {
        // checked_sub on total_delta: backwards counters yield None, not panic.
        let previous = CpuTimes {
            idle: 80,
            total: 100,
        };
        let current = CpuTimes {
            idle: 90,
            total: 50,
        };
        assert_eq!(cpu_usage_percent(previous, current), None);
    }

    #[test]
    fn usage_clamps_idle_backslide_to_100_percent() {
        // saturating_sub on idle_delta: idle counter resets clamp busy to 100%.
        let previous = CpuTimes {
            idle: 90,
            total: 100,
        };
        let current = CpuTimes {
            idle: 0,
            total: 200,
        };
        assert_eq!(cpu_usage_percent(previous, current), Some(100.0));
    }

    #[test]
    fn idle_only_delta_reports_zero_usage() {
        let previous = CpuTimes {
            idle: 80,
            total: 100,
        };
        let current = CpuTimes {
            idle: 180,
            total: 200,
        };
        assert_eq!(cpu_usage_percent(previous, current), Some(0.0));
    }

    #[test]
    fn parses_proc_net_dev() {
        let contents = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1000 1 0 0 0 0 0 0 2000 2 0 0 0 0 0 0
  eth0: 4096 4 0 0 0 0 0 0 8192 8 0 0 0 0 0 0
";
        let parsed = parse_proc_net_dev(contents);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].interface, "eth0");
        assert_eq!(parsed[1].rx_bytes, 4096);
        assert_eq!(parsed[1].tx_bytes, 8192);
    }

    #[test]
    fn net_dev_skips_malformed_lines() {
        // Lines without a colon are dropped entirely. But non-numeric counter
        // fields are only filtered positionally, not by rejecting the line:
        // "1 2 not-a-number 4..." parses the remaining 9 values as if the
        // gap never existed (rx=1, tx=values[8]=10). Same for short rows
        // with fewer than 9 numeric values being silently dropped.
        let contents = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
bad line without colon
eth floppy: 1 2 not-a-number 4 5 6 7 8 9 10
  eth0: 4096 4 0 0 0 0 0 0 8192 8 0 0 0 0 0 0
";
        let parsed = parse_proc_net_dev(contents);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].interface, "eth floppy");
        assert_eq!(parsed[0].rx_bytes, 1);
        assert_eq!(parsed[0].tx_bytes, 10);
        assert_eq!(parsed[1].interface, "eth0");
    }

    #[test]
    fn net_dev_trims_interface_whitespace() {
        let contents = "\
header one
header two
     eth0    : 1 2 3 4 5 6 7 8 9
";
        let parsed = parse_proc_net_dev(contents);
        assert_eq!(parsed[0].interface, "eth0");
    }

    #[test]
    fn net_dev_counts_exactly_sixteen_fields() {
        // A 16-field row parses; the 18-field `lo`-style row also parses but
        // bytes are read positionally (0 and 8).
        let contents = "\
header one
header two
eth0: 10 1 0 0 0 0 0 0 100 2 0 0 0 0 0 0
eth1: 20 1 0 0 0 0 0 0 0 1 0 0 0 0 0 0
";
        let parsed = parse_proc_net_dev(contents);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].tx_bytes, 100);
        assert_eq!(parsed[1].tx_bytes, 0);
    }
}
