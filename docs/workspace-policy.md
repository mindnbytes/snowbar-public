# Workspace display policy

Review checkpoint for the real niri workspace slice; not yet implemented.

- Display niri's per-output workspace indices, never workspace names. Keep
  workspace IDs separately for identity and focus actions; indices can change.
- Show all workspaces reported for the bar's output in index order, including
  niri's trailing empty workspace. Do not assume four workspaces is a limit.
- Preserve the approved glass capsule, selected and hover treatments. Supply
  matching artwork for digits 5–8. Use a readable numeric text fallback for
  missing artwork and multi-digit indices; never omit a workspace for lack of a
  glyph. Keep the strip within its own bounded area so it cannot overlap the
  clock. When the workspaces exceed that area, allow horizontal scrolling with
  the trackpad or Shift+mouse wheel; do not discard hidden workspaces.
- Highlight the workspace active on the bar's output. Send focus requests by
  stable workspace ID, not by displayed index.
- On disconnect, do not permit clicks on stale workspace IDs; show an explicit
  unavailable state and reconnect with bounded backoff. A new event stream's
  initial workspace snapshot replaces old state before clicks are enabled.
- The clock and power actions remain mocked during this slice.

The initial target is a single output. How to identify the output actually
hosting the Rune layer surface, including after output changes, must be verified
before choosing a multi-output fallback. Following the focused output alone can
display workspaces belonging to a different monitor than the bar.

The existing numeral SVGs bake in colors and effects. Runtime recoloring is a
later configuration concern: preserve the approved default artwork now, and do
not claim recoloring support until a safe approach has been visually validated.
The user reviewed the 1–9 mockup in the VM: the new glyphs are acceptable for
now, but numeral shapes and bevel treatments are not entirely consistent.
Research a more consistent, configurable approach to the artwork in a later
slice, with a visual comparison against the approved glass mockup.

For an isolated visual review of the candidate digits, run
`cargo run --locked --example nine_workspace_bar` in the niri session. This is
a separate mock bar with nine clickable workspaces and mocked clock and power
actions, not the live workspace implementation. It reserves its own top bar
space, so stop it after reviewing; do not replace the installed bar. The nine
buttons are displayed together for review rather than testing overflow.
