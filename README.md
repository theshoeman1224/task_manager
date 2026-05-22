# Linux Task Manager

A lightweight Rust/GTK4 system monitor inspired by Windows Task Manager.

The app is organized around monitor sources, so new resources can be added as
new tabs without reshaping the rest of the codebase.

## Build

Core telemetry and tests do not require GTK:

```sh
cargo test
```

The GUI requires GTK4 development files:

```sh
sudo apt install libgtk-4-dev
cargo run --features gui
```

## Current Monitors

- CPU utilization from `/proc/stat`
- CPU package power from `/sys/class/powercap` when exposed
- GPU best-effort AMD sysfs/hwmon telemetry and NVIDIA availability detection
- Network throughput from `/proc/net/dev`
- System power from `/sys/class/power_supply`

Unavailable hardware or kernel telemetry is shown as unavailable instead of
being estimated.
