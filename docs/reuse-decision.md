# Reuse and feasibility decision

Assessment: 2026-11-10. This is a source review, not a successful VM test.

## Recommendation

Build a small, single-process Rune application with a fresh adapter around the
official `niri-ipc` event stream and actions. Study Hokum's integration rather
than importing it; use Ashell as a behavioral reference, not a code source.
This recommendation is **conditional** on the layer-shell feasibility gate:
the inspected Rune revision does not expose the required reserved-space bar
through its existing overlay implementation. If a small, maintainable change
cannot provide it, stop and reconsider Rune rather than build a large fork.

## Sources and findings

- **Hokum**, [`e2e31b146792da21b9c6c691703433a6d2d72bbe`](https://github.com/anomalyco/hokum/tree/e2e31b146792da21b9c6c691703433a6d2d72bbe): [`src/panel.rs`](https://github.com/anomalyco/hokum/blob/e2e31b146792da21b9c6c691703433a6d2d72bbe/src/panel.rs) shows application use of Rune overlays. It is a useful example, but its platform machinery lives inside Rune, not a standalone Hokum bar library. Hokum is MIT; its [manifest](https://github.com/anomalyco/hokum/blob/e2e31b146792da21b9c6c691703433a6d2d72bbe/Cargo.toml) also pulls in PipeWire, D-Bus, scripting and other control-center dependencies that a bar does not need. Selective adaptation is possible under MIT with notice retention, but no specific Hokum code has been selected for copying.
- **Rune**, [`45eb5cab82400f8bdc4d494573af133af701619c`](https://github.com/anomalyco/rune/tree/45eb5cab82400f8bdc4d494573af133af701619c): its MIT workspace includes Rune and Rune Kit. [`crates/rune/src/platform/layer_shell.rs`](https://github.com/anomalyco/rune/blob/45eb5cab82400f8bdc4d494573af133af701619c/crates/rune/src/platform/layer_shell.rs) creates an overlay-layer surface with `set_exclusive_zone(0)`; its placement logic does not establish the required full-width top-anchored reserved bar. It handles pointer input and supports on-demand keyboard interactivity when layer-shell version 4 is available. [`crates/rune-kit/src/popover.rs`](https://github.com/anomalyco/rune/blob/45eb5cab82400f8bdc4d494573af133af701619c/crates/rune-kit/src/popover.rs) provides in-window popover building blocks, not proof that content can extend outside a bar surface or that a separate interactive popover works in niri. The platform implementation is `pub(crate)`, so a small upstream API extension may be needed; the size and maintainability of that change are unproven. Hokum's manifest instead pins Rune `49acd141a7839c30f9fed5b18499547b71c70887`; the two revisions must not be conflated.
- **Ashell**, [`8946a44458a9932146ece12ff0d4e95962131dd7`](https://github.com/MalpenZibo/ashell/tree/8946a44458a9932146ece12ff0d4e95962131dd7): [`src/services/compositor/niri.rs`](https://github.com/MalpenZibo/ashell/blob/8946a44458a9932146ece12ff0d4e95962131dd7/src/services/compositor/niri.rs) separates transport and mapping from widgets, but depends on Ashell compositor types and service events; [`src/services/compositor/mod.rs`](https://github.com/MalpenZibo/ashell/blob/8946a44458a9932146ece12ff0d4e95962131dd7/src/services/compositor/mod.rs) connects the service to Iced subscriptions/tasks. Extraction is not a simple independent library, and the listener does not itself implement reconnect. Its [manifest](https://github.com/MalpenZibo/ashell/blob/8946a44458a9932146ece12ff0d4e95962131dd7/Cargo.toml) includes Iced layer-shell, Tokio, `niri-ipc` and broader desktop services. GPL-3.0-or-later code reuse would carry corresponding distribution obligations and constrain licensing; do not copy it without a deliberate license decision. A fresh small adapter is likely cheaper, though not yet measured.
- **niri**, [`7f81a7f77b0e93464645232bd15ba2dc43ca3782`](https://github.com/niri-wm/niri/tree/7f81a7f77b0e93464645232bd15ba2dc43ca3782): [IPC documentation](https://github.com/niri-wm/niri/blob/7f81a7f77b0e93464645232bd15ba2dc43ca3782/docs/wiki/IPC.md) describes `$NIRI_SOCKET`, initial event-stream state followed by updates, non-atomic related updates, and JSON compatibility expectations. `niri-ipc` is versioned with niri, not Rust-semver stable. Pin a version compatible with the actual VM compositor; the reviewed upstream commit is a reference, not a claim about the installed version. Use stable workspace IDs in `Action::FocusWorkspace`, not display indices.

Hokum and Ashell both had upstream commits on 2026-11-10; that indicates recent activity, not a guarantee of long-term maintenance. These dependency observations are from manifests and relevant source paths, **not** a resolved transitive dependency or license audit. The VM configuration locks Hokum at `f30af6ae2762bf9a3b41b9cbccac6e01cbed08ab`, different from the source reviewed above, and uses NixOS/Home Manager on an ARM64 VMware niri desktop. The running VM's exact versions and capabilities remain unverified.

## Smallest viable design

- One process; Rune views plus a small typed state reducer, IPC reader and command sender. Do not import a whole bar or add a generic widget framework.
- Initially display the selected output's workspaces in niri order, including the trailing empty workspace, with the active workspace distinguished. Retain IDs for clicks; do not assume indices are stable. Output selection and behavior when the output disappears must be made explicit during implementation.
- Connect to the event stream, apply its initial state and subsequent events, and use a separate nonblocking command path for focus requests. On missing socket, EOF or error, mark workspace state unavailable, disable stale clicks, retry with bounded backoff, and replace state from the next stream's initial events. Tolerate transiently inconsistent related updates and unknown future protocol data where practical.
- Schedule the clock at minute boundaries, with resume/time-change refresh; invoke typed power actions only after confirmation and show failures. Determine actual logout and logind behavior in the target session before choosing interfaces. Tests must use mocks and never execute disruptive actions.

## Next review gate

First investigate whether a narrowly scoped Rune API change can create a top/left/right anchored surface with a positive exclusive zone and usable pointer input. Then run a temporary application in the existing VM to verify reserved space, clicks, fullscreen behavior, and menu keyboard/pointer focus, including dismissal. Establish whether the menu needs a separate surface. No successful rendering, screenshots, build, or power-interface validation is claimed by this assessment.

### Follow-up source and VM check

The VM was reached read-only over SSH: the running compositor reports niri
26.04, and IPC reports one `Virtual-1` output at logical 1728 × 1084 with
scale 1.75. This is useful for a real fractional-scale test, but does not
validate any Rune surface. No application was run in the graphical session.

At the pinned Rune revision, the smallest candidate extension is a dedicated
reserved top-bar placement: in `crates/rune/src/overlay/options.rs`, expose
the placement; in `crates/rune/src/platform/layer_shell.rs`, anchor top, left
and right, request compositor-determined width with a fixed logical height,
and set a positive exclusive zone for that placement only. Preserve zero-zone
behavior for ordinary overlays and reset the zone if placement changes.
`crates/rune/src/overlay.rs::layer` already consumes the compositor's initial
configured size. This is a **candidate**, not a tested patch; runtime resize,
fractional scaling, fullscreen stacking, disconnect handling, and behavior on
other platforms still need investigation. In particular, the current code
selects `Layer::Overlay`; whether that is suitable for fullscreen windows must
be checked rather than assumed.

A separate existing `Rect` or edge-positioned Rune overlay is a plausible
menu surface, but its stacking relative to the bar, input, keyboard focus,
dismissal, and output-relative positioning are also unverified. A prototype
must test these instead of treating an in-window popover as proof.

### Reserved-bar prototype

A disposable modification of Rune at the revision above was built on the
ARM64 VM with Rust 1.99.0 and run in niri 26.04. The test used the top layer,
top/left/right anchors, compositor-determined width and a 40-logical-pixel
exclusive zone. The Wayland log recorded a 1728 × 40 configure and click
events. The user confirmed that the bar was visible across the top, the click
counter increased, and tiled windows started below it. The output was at
1.75× scale. This supports the narrow reserved-bar approach without proving
the prototype is production-ready; the temporary patch is not part of this
repository.

The first launch failed because its temporary runtime environment lacked the
Wayland library; adding the library path allowed the test to start. No
screenshot was captured. At that point, fullscreen behavior, a separate menu,
keyboard focus and dismissal, resizing, disconnects and packaging were untested.

### Menu and fullscreen follow-up

An extended disposable probe used a separate, non-exclusive Rune overlay for
the menu. The user observed the menu opening and dismissing on click, but with
a large gap below the bar. Escape did **not** dismiss it. The Wayland log
shows on-demand keyboard focus and Escape key events arriving at the menu
surface, but no probe key-handler log; the cause remains unconfirmed. The menu
was requested with a 40-point top margin in addition to the bar's reserved
space, which may explain the gap; this needs a positioning retest.

The user also observed that the bar disappeared when the window entered
fullscreen, with some delay. The delay's source and behavior across other
conditions are unverified. These results demonstrate pointer interaction and
the observed fullscreen transition, but the keyboard/dismissal part of the
feasibility gate remains open. No screenshot was captured.

### Menu retest

The user confirmed that removing the menu's extra top margin eliminated the
gap. With the original Rune keymap read, diagnostics showed the menu had
keyboard focus and received Escape, but `xkb` state was absent and libxkbcommon
reported a parse error. In the disposable Rune checkout, changing the keymap
read to an offset-based `read_exact_at(..., 0)` produced a valid keymap; the
user confirmed Escape dismissed the menu, and the log recorded the Escape
callback. This isolates a working workaround, not a proven root cause or a
production-ready upstream fix. The user also confirmed the menu still
dismissed on click. The probe and diagnostic changes remain outside the
project repository.

The feasibility prototype has now demonstrated reservation, clicks, an
interactive separate menu, on-demand keyboard focus, Escape dismissal, and
the observed fullscreen transition on this VM. It does not establish
click-outside dismissal, other keyboard layouts, correct behavior across
output changes, loss of Wayland, or a reproducible application build. Before
implementation, decide how to maintain or upstream the necessary small Rune
changes; do not silently depend on the disposable checkout.

### Fork maintenance decision

Use [mindnbytes/rune](https://github.com/mindnbytes/rune) as a temporary fork,
keeping our changes small and preserving existing overlay behavior by default.
The application will pin a tested fork commit, not follow a moving branch.
Check upstream regularly, ideally daily, but integrate changes deliberately
and rerun the relevant build and VM checks before updating the pin; do not
automatically merge untested upstream changes. Once our changes have been
tested for this use case and their behavior is satisfactory, propose them
upstream. Rune's contribution guidance currently requests issues rather than
pull requests; follow its guidance at the time. The experimental changes are
published on [`snowbar/wayland-bar` at
`bd4c25c3bcda14b4d4c2344c1cac7d921ef50192`](https://github.com/mindnbytes/rune/commit/bd4c25c3bcda14b4d4c2344c1cac7d921ef50192),
based on Rune `45eb5cab82400f8bdc4d494573af133af701619c`. The additive
builder leaves existing overlay options unchanged. The included temporary
probe built on the VM, and the user confirmed reservation, menu placement and
Escape dismissal. Full compatibility and failure handling have not been
established; the application dependency has not yet been pinned.
