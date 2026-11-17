# Rune output safety: contained investigation

Investigation based on local source, not a live test. Snowbar's Rune fork was
pinned at `96001408366ff226181b18ce69f904a897e5b3ee` and `niri-ipc` at
`26.4.0`. No fork or VM changes were made for this investigation.

## V1 direction

The user prefers a simple bar for their own single-monitor setup and does not
want to add substantial fork maintenance for multi-monitor or hotplug support
now. Treat display changes and multiple monitors as unsupported; stop and restart
the bar when the display setup changes. This is an operational restriction,
not a mechanism that guarantees fail-closed behaviour if displays change.
The workspace candidate remains unreviewed. Snowbar's separate disconnect
coordination fix and its in-flight limitation are described in
`workspace-policy.md`. Do not interpret this preference as proof of output
placement or as approval to send stale actions.

## Findings

- `crates/rune/src/overlay/lifetime.rs::layer` resolves explicit display IDs
  against available monitors. If the target is absent, it passes no output.
  `platform/layer_shell.rs::LayerSurface::new` also passes no output when its
  registry lookup fails. In either case, the compositor chooses an output:
  explicit targeting alone is not fail-closed.
- The startup display list is a snapshot. The inspected layer-shell code does
  not expose surface `enter`/`leave` or registry removal notifications to the
  application. It handles layer-surface `closed`, but that is not a general
  output-inventory notification or placement confirmation.
- Rune's layer-shell backend wraps winit's existing Wayland display connection,
  so its lookup and winit's registry ID are in the same connection's namespace.
  IDs are not persistent and can become stale. Surface membership events can
  provide evidence of compositor-reported association, not independent proof
  of physical placement.
- Niri's output query and workspace events are not an atomic snapshot, and
  `niri-ipc` 26.4.0 has no output-inventory event. Workspace drift and stream
  loss do not detect every display change.

## Possible later fork change

A narrow, additive, potentially upstreamable API could reject unresolved
**explicit** targets rather than falling back, and report selected-output
removal or inconsistent surface membership to callers. Snowbar could then
invalidate its mapping and actions, and hide/destroy the surface. The existing
`closed` path is another invalidation signal, but is insufficient alone.

Early estimate: roughly 2–4 Rune files and 50–120 lines plus tests and API
documentation; this is not a validated implementation estimate. Likely areas
are `overlay/lifetime.rs`, `platform/layer_shell.rs`, and overlay lifecycle/API
plumbing. Tests should cover missing explicit targets, removal, membership and
close propagation; compositor integration tests would still be needed.

This would reduce known fail-open behaviour, not establish an atomic hotplug
guarantee: notifications arrive asynchronously and compositor correctness
cannot be independently proven by the app. Carrying the patch would require
rebasing and regression testing overlay/Wayland paths on Rune updates. An
optional general-purpose strict-target API is preferable to Snowbar-specific
behaviour if proposed upstream. The local comparison used the available fork
history; a full current-upstream comparison was not established.
