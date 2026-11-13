# Rune and niri output diagnostic

Diagnostic checkpoint, not an implemented output-selection policy. Rune is
pinned at `90ce05442d379849679ab435950c86babb6ba9e7` and `niri-ipc` at
`26.4.0`. `examples/display_diagnostic.rs` prints Rune's startup display
snapshot and creates no window or layer surface. Run it in a graphical session
with `nix develop --command cargo run --locked --example display_diagnostic`;
compare separately with `niri msg --json outputs` and `niri msg --json workspaces`.

## Source findings

- Rune `crates/rune/src/window.rs::Windows::resumed` passes a snapshot from
  winit's `available_monitors()` to `run_windows`. `Display` includes an
  optional name and `DisplayId`; on Wayland its `native_id()` is the numeric
  `wl_output` registry-global name, not a niri IPC name or a persistent ID.
  Rune's pinned winit gets the name from Wayland output information and reports
  no primary monitor on Wayland.
- `OverlayOptions.display` accepts `DisplayTarget::Id` or a native Wayland ID.
  Rune's `overlay/lifetime.rs::layer` checks whether that ID is still present;
  `platform/layer_shell.rs::LayerSurface::new` binds its corresponding
  `wl_output` for `get_layer_surface`. If no matching ID is found, Rune passes
  no output and the compositor chooses one. Explicit selection is therefore
  possible in source, but a stale target is not fail-closed by itself.
- Niri's `Outputs` reply is keyed by output name and `Workspace.output` is an
  optional output name. The event stream supplies replacement workspace
  snapshots, but `niri-ipc` 26.4.0 exposes no dedicated output-inventory change
  event. A separate `Outputs` query and workspace events are not one atomic
  snapshot; the stream's related updates can also be non-atomic.
- The inspected public Rune startup callback is a snapshot, not demonstrated
  as a subscription to display changes. No output-loss or reconnect behavior
  of the actual layer surface was tested here.

## VM observation

On the existing niri 26.04 VM, one diagnostic invocation returned one Rune
display: `name=Some("Virtual-1")`, `native_id=Some(Wayland(44))`, position
`(0, 0)`, physical size `(5120, 2880)`, scale `2`, and `primary=false`.
Niri's separately queried output inventory had one output, `Virtual-1`, with
physical current mode `5120×2880` and logical scale `1.75`; its workspace
snapshot used `output: "Virtual-1"`. The two names agreed **in this one
observation**. Rune's numeric Wayland ID has no demonstrated relationship to
the IPC name. Its reported scale differs from niri's fractional logical scale,
so neither scale nor calculated geometry should be used as a mapping key.

The diagnostic created no surface; it did **not** verify that an explicitly
targeted layer appears on the intended output. The registry ID is specific to
that session and should not be stored as a persistent configuration value.
Multiple outputs, disconnects, hotplug, changed scales, and restart recovery
were not tested.

## Proposed v1 policy for review

Start only when Rune's startup snapshot has exactly one usable Wayland display
with an ID and niri reports exactly one output. Explicitly target that Rune ID
and associate it with the sole niri output by cardinality, not by assuming name
equality. If either side is missing or ambiguous, show workspace state as
unavailable and do not send focus commands. Where safely possible, retain the
bar's mock clock and power UI; do not silently create a bar on an unrelated
output if targeting cannot be validated.

For multiple outputs, do not select by position, scale, or focused workspace.
Only permit a mapping after a matching method and explicit surface placement
have been independently verified; otherwise report unavailable. This does not
claim that names always match.

Output loss, inventory changes, stream loss, or uncertainty must invalidate the
mapping and queued focus intents immediately; fresh state alone must not revive
an old session's IDs. How to detect output changes reliably, prevent Rune's
fallback from retargeting a surface, and safely re-establish a mapping remains
an implementation and VM-test gate. If the necessary change notifications or
surface placement confirmation cannot be obtained, stop or hide the workspace
UI instead of asserting hotplug support.
