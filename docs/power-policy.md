# Power menu policy

Status: **implemented and headless-tested, including bounded popover text;
fullscreen layering deferred.** In the live VM review the user reported that
power functionality worked; individual action outcomes were not recorded.
Automated tests never execute logout, restart or shutdown.

## Working on this slice

- `src/power.rs` owns typed actions, menu transitions, and result classification.
- `src/main.rs` owns the Rune popover, layout, input and headless tests.
- `src/power_os.rs` owns the real Linux effects; it is excluded from test builds.

Run `env CARGO_BUILD_JOBS=2 cargo test --locked --lib --bin snowbar` in the
[pinned Nix shell](../README.md#developing-with-nix) for the focused suite.
Keep new tests on the fake backend; use the manual checklist below for
compositor and system behavior.

## Actions and exact mappings

| Action | Effect after Snowbar's explicit confirmation |
|---|---|
| Log out | current `NIRI_SOCKET` receives `Request::Action(Action::Quit { skip_confirmation: true })` |
| Restart | `busctl --system --timeout=60 call org.freedesktop.login1 /org/freedesktop/login1 org.freedesktop.login1.Manager Reboot b true` |
| Shut down | same fixed call with `PowerOff` |

niri's own confirmation is skipped only because the menu already obtained an
explicit confirmation. logind uses the normal system-bus/polkit path: no
`sudo`, force flag, inhibitor bypass, shell, fallback command or user-supplied
argument. The menu does not target another socket path or a guessed backend.

## Interaction safety

- Clicking a row only opens an action-specific confirmation. The initial list
  never dispatches.
- Confirm is an explicit footer control (Cancel left, Confirm right), spatially
  below every initial action row. A quick double click on a row (notably
  Restart) cannot land on Confirm; the production headless test checks Confirm
  and Cancel against all three rows and replays a same-coordinate double click.
- No Enter/default binding confirms, and no destructive control is auto-focused.
- Cancel returns to the list. Escape and the bar's power toggle dismiss only an
  unconfirmed selection; the next open is fresh.
- Confirm atomically leaves confirmation and enters single-flight pending
  before the worker is handed the dispatch. Repeated clicks, Escape, toggle
  close and reopen cannot dispatch again while it is in flight.
- Pending and results are preserved across hide/reopen and drained by the
  overlay poll or the next open. Escape/toggle hide pending work without
  pretending to cancel it.
- Failure is operation-specific, visible and persistent. Returning to the list
  and selecting/confirming again is the only retry; there is no automatic
  retry. A stale completion from an earlier attempt cannot finish a later one.
- A timeout is reported as uncertain because the request may already have been
  executed. An accepted result says the request was accepted, not that the
  session ended, the computer restarted or it powered off. Accepted is terminal
  for this bar session and offers only Close, so a duplicate disruptive request
  cannot be armed accidentally.

## Effect boundaries

The production worker is compiled only outside the test build, is created with
the overlay and runs dispatches on demand. The UI never blocks: it sends a typed
dispatch and applies queued completions in the power overlay's poll.

The niri transport uses a standard Unix socket through the already-locked
`socket2`: connect has a 3 s timeout, send and reply each have 3 s deadlines,
and the newline-framed reply is capped at 8 KiB. Its phase bound is therefore
about 9 s plus scheduling. The logind path uses a fixed `busctl` call with
`--timeout=60` and a 70 s outer wait; stderr is drained concurrently with a
bounded collection, so its realistic bound is the outer wait plus at most 1 s
for stderr. Missing tools/socket, authorization denial, connection/response
errors, worker spawn failure, disconnected worker and helper panics become
visible, distinct results. Non-Linux builds fail closed.

## Native input and geometry

At the pinned Rune revision, the reserved bar layer is full output width while
the power menu is a zero-exclusive overlay anchored to the output edge and
targeted at the same display when available. The menu uses
`OverlayInput::Regions`: only its explicit `.hit()` list/footer controls become
native input regions. Its background and padding pass pointer input through to
what is underneath, so the bar's power toggle can still receive clicks there.
No global outside-click interception was added; Escape and the power toggle
remain the dismissal paths. Headless tests cover the state half of the toggle
and the native hit regions; native show/hide, the global open flag and the
actual toggle click under narrow or scaled outputs are not live-verified.

Accepted limitations: the review did not record individual `busctl`/polkit
or niri quit outcomes. A slow or absent polkit prompt can reach the bounded
timeout and is reported uncertain; the authentication dialog may outlive
`busctl`. Niri's socket EOF after a quit is uncertain because the action may
already have been processed. Signaled or otherwise ambiguous `busctl` failures
are uncertain; only a valid niri rejection, a clear denial/inhibitor/connection
failure, or a pre-connect/spawn failure is a definite failure. The VM reference
runs niri through greetd with a polkit agent, but this slice did not verify that
its agent authorizes these calls. Recreating the overlay or exiting the process
drops the worker/result channel, so an already-dispatched effect cannot be
recalled or reported afterward.

## Popover text layout (fixed in headless tests)

On 2026-11-18, the live niri VM preview showed the Log out confirmation's
supporting text running past the right edge of the popover and being clipped.
The user then reported the same overflow for the Restart and Shut down
confirmations. Pending and result copy had the same layout risk.

- [x] Confirmation prompt and detail, pending copy, and results share one
      bounded wrap-anywhere width inside the card. Long sanitized or unbroken
      backend errors wrap rather than run under the edge or over the footer.
- [x] The popover height is derived from the same Rune text measurements the
      view uses, so a worst-case result grows the surface instead of clipping.
      Accepted/Close, Cancel/Confirm and Hide remain explicit footer controls;
      geometry stays disjoint from the initial action rows for the double-click
      guard.
- [x] `power_copy_wraps_inside_the_popover_for_confirm_pending_and_results`
      drives the production view with a fake backend at scales 1, 1.25, 1.5
      and 2. It asserts each text node contains the whole sentence, its visible
      rect is unclipped, measured wrapped lines fit its box, and no text reaches
      the footer. `long_error_copy_grows_the_popover_instead_of_clipping` covers
      the maximum 80-character sanitized unbroken error.
- [x] The user approved the corrected confirmation layout in the single-output
      niri VM preview on 2026-11-18.
- [ ] Live verification of pending and long result copy at other output sizes
      and scales remains open. Headless tests cover layout bounds, not native
      presentation or GPU rasterization. The original overflow capture was
      not committed.

## Deferred: fullscreen layering

The live VM review found that entering terminal fullscreen while the power
menu is already open hides the bar but leaves the menu above the terminal.
We are keeping the current behavior and pin: entering fullscreen with the
menu open is uncommon, and the bar’s hidden power button cannot open a new
menu while fullscreen. Do not treat this as a release blocker.

Exact pinned source (`09677837047eefdbae27bd4690a6a7ea9453aaf1`,
`crates/rune/src/platform/layer_shell.rs:500-523`) chooses the layer from the
reservation alone:

```rust
let layer = shell.get_layer_surface(..., if reserved.is_some() {
    Layer::Top
} else {
    Layer::Overlay
}, ...);
layer.set_exclusive_zone(reserved.map_or(0, ...));
```

The bar calls `wayland_top_reserved(40)`, so it is Top with a 40-pixel
exclusive zone. The menu passes no reservation, so it is Overlay: the layer
above fullscreen windows. This is compositor layering, not niri IPC state; a
popover must choose Top with exclusive zone 0 to follow the bar behind
fullscreen.

A prototype is preserved on the Rune fork branch
[`snowbar/prototype-wayland-top-unreserved`](https://github.com/mindnbytes/rune/tree/snowbar/prototype-wayland-top-unreserved)
at `4a4fbc0687b1c6bfe151a5db59e8771abf994452`. Its
`ConfiguredOverlay::wayland_top_unreserved()` adds an independent unreserved
Top choice while preserving `wayland_top_reserved` and the menu’s placement,
size, input and key requests. Focused tests and a small layer-policy check
passed on the macOS host.
The primary also ran `cargo check --locked --lib --bin snowbar` against the
exact isolated prototype on the Linux VM with the pinned shell and two jobs:
success, with only pre-existing dependency warnings. That was a check only: no
Linux test link, app launch, power action or native compositor result. The
production pin is **unchanged** and Snowbar does not call the prototype API.

Pin fidelity: the `rune-integration` checkout status is clean, and
`git -C ../rune-integration diff --name-only 09677837047eefdbae27bd4690a6a7ea9453aaf1 HEAD`
lists only `crates/rune/README.md`; the prototype source was exported from the
exact pinned commit, not from that checkout's HEAD.

If revisiting the prototype, check it in niri before updating the pin. Do
not activate any power action:

1. Open the menu and enter a confirmation.
2. Make a terminal fullscreen (for example with niri's `Ctrl+Enter`).
3. Both bar and menu must be behind the fullscreen window.
4. Leave fullscreen; both must return and normal clicks, Escape and the power
   toggle must still work.
5. The menu must keep the approved top-right placement, with no new vertical
   gap or overlap against the bar, and must not add reserved space.
6. The bar's power toggle and menu keyboard focus/Escape must still work.

Accepted versus reserved geometry: the candidate keeps exclusive zone 0.
Zone 0 asks the compositor to avoid other surfaces' exclusive zones, while
`-1` would ignore exclusive zones and place at the requested coordinates.
Because the placement request alone does **not** establish observed geometry,
keep zone 0 pending this native check; do not switch the prototype policy to
`-1` without evidence.

Headless tests cannot prove compositor stacking order or the resulting
exclusive-zone geometry; do not mark these passed from `cargo test` or a Linux
`cargo check` alone.

## User-only live checklist

Use a disposable VM/session; do not run this checklist unattended or on a
shared machine.

- [ ] The menu lists exactly Log out, Restart and Shut down; selecting each
      shows its own prompt and sends nothing.
- [ ] Cancel, Escape and the power toggle clear the unconfirmed prompt; reopen
      shows the list again.
- [ ] With the menu open, the bar power toggle still receives clicks and only
      the menu's buttons intercept the pointer; check narrow/scaled output
      geometry natively.
- [ ] Each confirmation, pending copy and a long failure stays inside the
      popover's right edge without clipping, with the footer fully visible, at
      the supported scale(s).
- [ ] A quick double click on each row, especially Restart, never confirms.
- [ ] Log out (only in a disposable session) shows pending/uncertain/accepted
      feedback without claiming the session already ended.
- [ ] Restart and shut down are checked only by a user who intends to perform
      them; record whether logind/polkit accepted or denied the request,
      including a slow polkit prompt.
- [ ] If practical, make the backend unavailable and confirm the failure is
      visible and reopening still shows it, with no automatic retry.
