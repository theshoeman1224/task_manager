# GUI baseline checklist

Run `cargo run --features gui` on the tagged commit (`pre-refactor-baseline`) and after the
UI split (`ui/tabs.rs`, `ui/graph.rs`), then diff what you see against the items below. The
checklist captures labels and mechanics that the automated tests cannot touch while the UI
is behind an interactive GTK process.

Captured on commit `65d7c16`, machine without NVIDIA/AMD GPU, with `intel-rapl` powercap
zones that lack readable `energy_uj` permissions for unprivileged users.

## Startup

- Window: width 900, height 620 default, title "Linux Task Manager".
- Vertical layout, margin 8: update-frequency control on top, Notebook below.
- Four tabs in this order, tab labels exactly: CPU, GPU, Network, Power.
- Before any sampler tick, each tab shows heading (tab title), subtitle
  "Waiting for telemetry..." (dimmed), and an empty scroller.
- No tabs beyond the four; no window resize weirdness.

## Update-frequency control

- Row margin: top 8, bottom 0, start 12, end 12; controls spacing 8.
- Label "Update frequency" left-aligned, then SpinButton, then dimmed "seconds" label.
- SpinButton range 0.25 to 10.00 step 0.25, two decimal digits, initial value equals the
  config sample interval (1.00 from AppConfig default 1s).
- Tooltip: "Seconds between telemetry samples".
- Dragging below 0.25 clamps back to 0.25; above 10 clamps to 10. The sampler thread
  additionally enforces a 250ms floor (`.max(250)` in `start_monitor_thread`), so values
  below 0.25 seconds cannot speed up sampling. Redraw tick is a separate fixed 250ms
  glib timer: changing the spin changes data sampling cadence, not redraw cadence,
  so the graph visibly updates in 250ms steps regardless.

## Per-tab content (each tab)

- Heading styled "title-1"; subtitle styled "dim-label".
- Row spacing 16 between the sections, tab margins 18 on all sides; row boxes spaced 10
  vertically in the scroller.
- Metric rows appear lazily as metrics arrive; height 118 each; left name/value column
  width 220; name styled "heading", value styled "title-3 monospace".
- Graph panel top label shows scale text, styled "dim-label", right-aligned;
  DrawingArea content height 82.

## CPU tab

- Subtitle: "Aggregate processor usage and package power"
- Rows in order: "Usage" (rows never exist during the first sample only; after tick 2,
  percent 0-100 with scale "Scale: 0 - 100.0%"), "CPU package power" (watts or
  "Unavailable", scale "Scale: 0 - <w>.00 W").

## GPU tab

- With no NVIDIA and no AMD telemetry (this machine): subtitle
  "No supported AMD or NVIDIA GPU telemetry found" and three rows each showing
  "Unavailable": Usage, GPU power, Temperature. Scale text "Scale: unavailable".

## Network tab

- Subtitle: selected interface name (busiest non-lo interface by rx+tx bytes).
- Rows in order: Receive (B/s), Transmit (B/s), Received total (bytes),
  Transmitted total (bytes). Scale lines use 1000-based units (KB, MB...).
- No eligible interface: subtitle "No network interfaces found", Receive and Transmit
  rows show "Unavailable".

## Power tab

- Subtitle: "System power supply telemetry"
- If supplies exist: rows named "<name> <kind> power" and "<name> <kind> capacity".
- Without supplies: subtitle "No system power supply telemetry found", rows
  "System power" and "Battery" both "Unavailable".

## Graph drawing

- Background rgb(0.10, 0.12, 0.14); 3 horizontal grid lines at 1/4, 2/4, 3/4 of height
  in rgb(0.22, 0.25, 0.28); data line rgb(0.30, 0.76, 0.96) 2.0 wide.
- Fewer than two points: grid only, no line.
- Line x spans full width, y clamped to the drawing area.

## Error path

- If a sampler returns an error, that tab's subtitle becomes the error string; the row
  labels do not change.
