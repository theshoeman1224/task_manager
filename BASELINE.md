# Baseline ref

Tag `pre-refactor-baseline` on commit `65d7c16` ("Initial Linux task manager app") is the
pre-refactor truth. All tests below passed against it on the initial run (2026-09-13),
before any refactor work landed.

Command record from the tagged commit:

- `cargo test`: 6 passed
  - `platform::linux::tests::parses_proc_stat_cpu`
  - `platform::linux::tests::calculates_cpu_usage_delta`
  - `platform::linux::tests::parses_proc_net_dev`
  - `core::formatting::tests::formats_units`
  - `core::series::tests::keeps_bounded_history`
  - `monitors::network::tests::prefers_selected_interface`
- `cargo build`: success
- `cargo build --features gui`: success
- `cargo clippy --all-features --all-targets`: 6 warnings, 4 distinct lints
  - `std::io::Error::other(_)` construction x2 (NVML failure paths, `src/platform/linux.rs:352, :365`)
  - manually built nul-terminated strings x2 (NVML dlopen names, `src/platform/linux.rs:323-324`)
  - `clippy::forget_non_drop` on `std::mem::forget(source_id)` (`src/ui/mod.rs:73`)
  - `values.get(0)` instead of `values.first()` (`src/platform/linux.rs:65`)

Allowed, explicit output/API changes during rework (each must be called out in review; everything
else must hold output byte-identical):
1. Removing `read_nvidia_gpus()` (`src/platform/linux.rs:235`) — unused free function.
2. Removing the `graph_points` field from `MetricSnapshot` if the UI keeps reading only its own
   `MetricRow` series — currently every monitor writes the field and nothing reads it.

Anything else (label strings, subtitle strings, formats like `"%.1f C"`, clamping, availability
fallback order) must flow through the split code untouched; the characterization and fixture
tests pin this.

## Bug found while writing the baseline: /sys/class/powercap runaway walk

`cpu_snapshot_shape` (tests/monitors_structural.rs) uncovered a pre-existing defect at the
tagged commit: `collect_powercap_energy_counters` (then at `src/platform/linux.rs:84-124`)
recursed over `fs::read_dir` entries using `path.is_dir()`, which follows symlinks. On real
hardware the zone dirs expose `device` and `subsystem` symlinks that loop back into the zone
tree, so the walk cycles (`.../device/.../device/.../subsystem/...`) and only stops when a
single path resolution crosses the ELOOP symlink-depth ceiling. Measured with strace: a
multi-second traversal in which `statx` returns ELOOP only after 40 resolutions, repeated
for thousands of directories. This machine reproduces it; the GUI runs the same walk every
sampling tick in a background thread.

Fix applied during baseline (documented here since it deviates from "no changes before
refactor"): the walk now canonicalizes the root once, then descends only into real
directories (`entry.file_type()` matches symlinks explicitly and skips them). Error results
from `read_dir`/`entry` are skipped instead of propagated; the only difference that could
surface is on a mid-directory IO error, and the only caller (`CpuMonitor::sample`) already
swallows such errors via `.ok()?`, so output is unchanged. The set of counters reported on
real systems is the same: energy zones live in real directories below the canonicalized root.

After the fix the full suite (35 lib tests + 5 structural tests) passes repeatedly and takes
about 0.07 s for the integration tests.
