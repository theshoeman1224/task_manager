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
1. Removing `read_nvidia_gpus()` — unused free function. Executed during Phase 2.
2. Removing the `MetricSnapshot.graph_points` field — the UI never read it. Executed during
   Phase 3 (see the Phase 3 record).

Phase 2 record (deliberate changes beyond pure deduplication):
- `read_nvidia_gpus()` removed from `platform/nvidia.rs` and the `platform/linux` facade.
  It had no callers; `NvidiaGpuReader` remains the only NVML entry point.
- The three monitor `impl Default` boilerplates (Cpu/Gpu/PowerMonitor) were dropped;
  nothing constructed them via `default()` anywhere. Clippy's `new_without_default`
  suggestion is intentionally suppressed per monitor so the lint baseline stays identical.
- `DeltaTracker` (core/delta.rs) now backs both network rates and CPU package power.
  Its `DeltaSample` semantics were derived from the two monitors' old edge handling:
  First = no previous record; Invalid = zero elapsed or counter rollback (network maps
  both to zero rates, power skips; equal to the old `saturating_sub`/guard outcomes);
  Changed = delta with monotonic increase.
- `MetricValue::percentage/watts/celsius` helpers replaced 14 hand-rolled
  `Option.map(...).unwrap_or(Unavailable)` blocks; contents identical.
- GPU snapshot construction consolidated into one `gpu_snapshot()` builder plus
  `push_usage_graph_point()`; real branches add the graph point, fallback branches
  (all-Unavailable with error-free subtitles) deliberately do not, matching old output.
- Test counts after Phase 2: 56 headless (41 lib incl. 6 DeltaTracker tests, 5 structural,
  10 fixture), 60 with `--features gui`.

Phase 3 record (deliberate changes beyond pure wiring cleanup):
- `MetricSnapshot.graph_points` deleted. Every monitor tracked it but the UI consumed
  only `MetricRow`'s own series (via `graph_value()`), so output is unchanged. The one
  behavior nuance retired with it: gpu/cpu graph data clamped usage to [0, 100] inside
  the monitor; the visible graph still clamps in `draw_graph()` (y clamped to the
  drawing area), so rendered output is the same. Tests asserting graph_points contents
  were removed along with the field, as planned and flagged here.
- `monitors::build_sources(&AppConfig)` is now the single monitor registry. The UI's
  tab notebooks and sampler threads both derive from it (`MonitorTab` titles come from
  `MonitorSource::name()`), ending the four-place name/registration hardcoding that
  made adding a monitor cost edits in `monitors/mod.rs`, the tab array, `start_samplers`,
  and the tab lookup map.
- `GpuMonitor.nvidia_error` retyped from `Option<String>` to `Option<MonitorError>`.
  Everything now reports failure through `MonitorError`; built subtitle strings are
  byte-identical, so the GPU tab output does not change.
- `ui/sampler.rs::start_samplers` now takes the already-built source list instead of
  constructing it from `AppConfig` itself; bootstrap still passes config values
  (interval, default network interface, history point count) exactly as before.
- Test counts after Phase 3: unchanged from Phase 2 (same 56/60), the deleted
  graph_points assertions replaced by one usage-over-100 metric pin.

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

Final baseline after all characterization + fixture work landed:

- `cargo test`: 50 passed (35 lib unit tests, 5 monitors structural, 10 platform fixtures,
  including the powercap symlink-cycle regression test `powercap_symlink_cycle_terminates`)
- `cargo test --features gui`: 50 passed, plus 4 UI characterization tests (graph_value,
  graph_max floors, scale_text per-unit formats) under `src/ui/mod.rs`'s inline module
- `cargo clippy --all-features --all-targets`: same 6 warnings / 4 distinct lints as the
  tagged commit, no new warnings introduced
- The `graph_points` outputs are pinned by both the structural tests and the GPU inline
  tests, so its later removal stays a reviewed, explicit change.

Verification record for the powercap-fix regression fixture (symlinks `device -> .`,
`subsystem -> .`, `subsystem-dev -> device` inside a zone dir, committed under
`tests/fixtures/sys-cycles/class/powercap`):

- Standalone binary running the tagged commit's walker against this fixture: hangs
  (killed by timeout, exit 124). Against the live `/sys/class/powercap` as an
  unprivileged user: still burning CPU after 90 s (killed); no counter output produced
  before the kill.
- Standalone binary running the fixed walker: the fixture completes in <0.1 s and
  reports exactly one counter (the real zone, energy 777777777); the live
  `/sys/class/powercap` completes instantly with the same final output (zero counters,
  because `energy_uj` is unreadable for unprivileged users) that the old walk only
  reached after exhausting the ELOOP ceiling, had it gotten that far. Output-equal,
  runtime bounded.
