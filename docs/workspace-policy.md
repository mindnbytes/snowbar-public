# Workspace display policy

Status: **single-monitor candidate reviewed in the niri VM**. The user prefers
a simple v1 for their own single-monitor setup rather than expanding the Rune
fork for output tracking now.
Output changes and multiple monitors are not supported; stop and restart the
bar after changing the display setup. This operational restriction does not
itself enforce safety when a change happens. Power actions are outside this
note and follow `docs/power-policy.md`; this slice's automated checks execute
no power action.

## Implemented behaviour

- Display niri's per-output workspace indices, never workspace names. Workspace
  IDs are kept separately for identity and focus actions, because indices change.
- Show every workspace reported for the bar's output in index order, including
  niri's trailing empty workspace. There is no four-workspace limit.
- The approved glass capsule, selected and hover treatments are preserved.
  Artwork exists for digits 0–9; a multi-digit or missing glyph uses a readable
  numeric text label instead of dropping the workspace.
- The strip uses a bounded 300 pt horizontal scroll area; the clock has its own
  space. The Rune fork forwards layer-shell scroll events to the viewport.
  Horizontal trackpad scrolling and workspace clicks were reviewed in the VM;
  a physical mouse wheel was unavailable for testing. Shift plus vertical
  scrolling is deferred because the bar does not take keyboard focus.
- The workspace active on the bar's output is highlighted. Clicks enqueue a
  focus request keyed by stable workspace ID on a worker thread, off the UI
  thread. The action worker checks the session generation before connecting
  and again after connecting, before authorizing dispatch. Socket operations
  do not hold the generation lock, so a stalled connection or reply cannot
  block invalidation. An action already authorized for dispatch is in flight:
  it may still be sent or processed after disconnect and cannot reliably be
  recalled. This boundary is authorization, not confirmed receipt by niri.
- On detected stream loss, output drift, or an ambiguous display/output mapping,
  the code attempts to invalidate the session and show `niri unavailable` with
  no clickable capsules once the UI processes the update. Already-authorized
  actions remain subject to the in-flight limitation above.
- Reconnect uses capped exponential backoff (250 ms → 8 s). A new stream's
  initial workspace snapshot replaces all old state before clicks are enabled.

## Output selection

The startup policy is deliberately strict:

1. Rune's startup snapshot must contain exactly one usable Wayland display with
   a native ID. That display is targeted explicitly with `DisplayTarget::Id`.
   If this is not true, the bar is not shown at all.
2. The niri `Outputs` query must return exactly one output. It is associated
   with the Rune display by cardinality, not by name equality. If this fails,
   the bar stays visible but the workspace area is unavailable and no focus
   commands are sent.
3. A global workspace snapshot naming a different output is treated as output
   drift: the session is closed and re-validated from scratch. Following the
   focused output alone would be wrong for a multi-monitor setup.

## Verified and not verified

Deterministic tests cover display-policy cardinality, session/snapshot
transitions, dynamic workspace events, drift, stale-generation and queued-action
rejection including blocked-connect and stalled-action interleavings, backoff
bounds, unavailable UI, focus-by-stable-ID, and scroll-viewport navigation.
They do not establish all possible disconnect/action interleavings or hotplug
safety.

On 2026-11-17, the user reviewed the final Snowbar build pinned to Rune
`99fa00ea05e46c6f6dbe0118f9ad07957d4cca93` in the single-output VM and
confirmed that its appearance, horizontal scrolling and workspace clicks
looked good. Niri reported the layer on `Virtual-1`; the temporary bar was
stopped after review. This does not establish hotplug or physical mouse-wheel
behaviour, or recovery from a real niri restart.

On 2026-11-18, the user tested the cleanup candidate in the VM through a
temporary IPC proxy. After the proxy closed and blocked connections, the bar
showed `niri unavailable` without clickable workspaces. When connections were
allowed again, workspaces returned and clicking to focus worked. Niri itself
remained running: this verifies recovery from an interrupted IPC connection,
not a compositor restart, a hung socket, or every in-flight action race.

The following remain **unverified** and require the niri VM checklist in
`docs/rune-integration.md`:

- Rune does not expose a post-creation query for the output a layer surface was
  actually placed on, and its layer-shell code falls back to compositor choice
  when a target ID is missing. Explicit targeting is therefore best effort, not
  a placement guarantee.
- Rune's startup display list is a one-time snapshot and its `wl_output` event
  handler is a no-op; niri-ipc 26.4.0 has no output-inventory change event.
  Output hotplug during a session is detected only indirectly (stream loss or
  workspace snapshot drift), not as a confirmed event.
- Rune's layer-shell backend wraps winit's existing Wayland display connection;
  the registry name is scoped to that connection and can become stale. Live
  placement must still be observed.

No hotplug safety or surface-placement confirmation is claimed.
See `docs/rune-output-investigation.md` for the contained fork investigation
and the v1 trade-off.

## Artwork

The existing numeral SVGs bake in colors and effects. Runtime recoloring is a
later configuration concern: preserve the approved default artwork now, and do
not claim recoloring support until a safe approach has been visually validated.
The user reviewed the 1–9 mockup in the VM: the new glyphs are acceptable for
now, but numeral shapes and bevel treatments are not entirely consistent.
Research a more consistent, configurable approach in a later slice.

## Later: follow the active workspace

Consider automatically bringing the active workspace into view when it changes
outside the bar, including when niri reorders or removes workspaces. This
complements manual overflow navigation; it does not replace the ability to browse
and click an inactive off-screen workspace. Decide how to avoid fighting a
user's manual scrolling when implementing it. This is a future task, not part
of the current scroll-input prototype.

For an isolated visual review of the candidate digits, run
`cargo run --locked --example nine_workspace_bar` in the niri session. This is
a separate mock bar with nine clickable workspaces and mocked clock and power
actions, not the live workspace implementation. It reserves its own top bar
space, so stop it after reviewing; do not replace the installed bar. The nine
buttons are displayed together for review rather than testing overflow.
