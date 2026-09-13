// Shared sysfs file readers used by the power and GPU platform modules.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn read_u64(path: &Path) -> io::Result<u64> {
    let contents = fs::read_to_string(path)?;
    contents
        .trim()
        .parse::<u64>()
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub(crate) fn read_micro_value(path: PathBuf) -> Option<f64> {
    read_u64(&path).ok().map(|value| value as f64 / 1_000_000.0)
}

pub(crate) fn read_milli_value(path: PathBuf) -> Option<f64> {
    read_u64(&path).ok().map(|value| value as f64 / 1_000.0)
}

pub(crate) fn read_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}
