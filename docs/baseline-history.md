# Characterization quirks and invariants

Behavior quirks the characterization tests (`tests/monitors_structural.rs`,
`tests/platform_fixtures.rs`, inline `#[cfg(test)]` modules) pin deliberately.
These are landmines: a "fix" that changes them here is an output change and
must be reviewed as one, not folded silently into a refactor.

## Parsing quirks

- `parse_proc_net_dev` filters counter fields **positionally**, not by
  rejecting the line: a row with non-numeric gaps parses the remaining values
  as if the gap never existed (values shift left, `rx` and `tx` stay at
  indices 0 and 8). Lines without a colon drop entirely; short rows with
  fewer than 9 numeric values drop.
- `parse_proc_stat_cpu` requires the literal `cpu` prefix (per-core `cpuN`
  lines are ignored), needs >= 4 numeric fields, and ignores non-numeric
  extras by shifting.
- Byte formatting is **1000-based** (`format_scaled_bytes`), not 1024-based:
  1024 bytes renders as `1.0 KB`, and negative byte values clamp to `0 B`.

## Sysfs semantics

- `power_now` is the primary supply source; the fallback multiplies
  `current_now × voltage_now` **after** each is scaled by 1e-6, which yields
  a mismatched micro prefix (2000 µA × 240 000 µV → 0.00048 "W").
  Characterized, not endorsed — changing it changes Power-tab output.
- `choose_interface` fallbacks: an explicit-but-missing selection returns
  `None` (no fallback); auto-selection skips `lo`, picks the max of
  `rx + tx` keeping the last maximal element on ties, and falls back to
  `counters.first()` so an all-`lo` system still displays.
- Max energy counters roll over; `CpuMonitor` skips a rollback entirely
  while `NetworkMonitor` clamps it to a zero rate. `DeltaTracker`
  (`core/delta.rs`) preserves both semantics via `DeltaSample::First/Invalid`.
- The powercap walker canonicalizes the root once and descends only into real
  directories. Following symlinks cycles through `device`/`subsystem`
  back-references; the runaway cost was minutes of CPU per sample (see
  `powercap_symlink_cycle_terminates` regression test).

## Inherited clippy debt

`cargo clippy --all-features --all-targets` shows four distinct pre-existing
lints (kept unchanged through every phase):

- `values.get(0)` instead of `values.first()` (`platform/proc.rs`)
- `std::mem::forget(source_id)` (`ui/mod.rs`)
- manually nul-terminated NVML dlopen names x2 (`platform/nvidia.rs`)
- manual `std::io::Error::other(...)` construction x2 (NVML failure paths)

## Deliberate output changes (executed, each was a flagged review item)

- `read_nvidia_gpus()` — unused free function, removed (`NvidiaGpuReader`
  remains the only NVML entry point).
- `MetricSnapshot.graph_points` — written by every monitor, read by no one;
  removed. The graph still clamps in `draw_graph()`, so rendered output is
  the same. The only retired nuance: monitor-side graph-data clamping of
  usage to [0, 100] (the usage *metric* itself was never clamped).

## Historical anchor

Tag `pre-refactor-baseline` (commit `65d7c16`) is verification anchor for
everything above; the three-phase refactor (`72c50ad`, `da92c9b`,
`8e6e4a1`) kept `cargo test` + `cargo test --features gui` green
throughout, with clippy's warning set byte-identical to this tag. See
git history for the full phase-by-phase records of the execution runs.

## Architecture notes for future growth

- Monitors register in ONE place: `monitors::build_sources(&AppConfig)`.
  Tab titles derive from `MonitorSource::name()`; samplers from the built
  list. Adding a monitor = implement `MonitorSource` + one entry.
- `config.rs` (`AppConfig`) now genuinely drives the UI: sample interval,
  graph history points, default network interface. Hook up CLI/env/persistence
  by extending `AppConfig` and one spot `ui::build_ui`.
- `platform/linux.rs` is a pure re-export facade with zero logic; call
  sites may stay on `platform::linux::...` or import concrete modules
  (`platform::proc`, `platform::power`, `platform::amd`, ...) either way.
- UI modules: `ui/sampler.rs` (threads/channel), `ui/tabs.rs` (widgets),
  `ui/graph.rs` (mapping + cairo), `ui/mod.rs` (bootstrap only). The sample
  cadence (configurable, floor 250 ms) and redraw cadence (fixed 250 ms glib
  timer) are intentionally independent loops.
