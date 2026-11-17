# Rune layer-shell scrolling investigation

The Snowbar VM review on 2026-11-17 found that the workspace strip could not
be scrolled. In pinned Rune revision `96001408366ff226181b18ce69f904a897e5b3ee`,
`platform/layer_shell.rs` ignores Wayland pointer axis events; `overlay/draw.rs`
therefore cannot forward them to Rune's existing wheel/scroll runtime. Clicks
and pointer motion do work. This source finding is consistent with the observed
failure, but the exact events emitted by the VM were not captured.

On the same date, upstream Rune `dev` at
`3d037b49e459cd45d9438a46d50be3b20d53542a` was inspected: its layer-shell
pointer handler likewise did not forward axis events. This is a point-in-time
observation, not a statement about later upstream revisions.

## Isolated prototype

An experimental worktree based on the pinned revision was created outside
this repository. The initial prototype modified `platform/layer_shell.rs` and
`overlay/draw.rs`, grouping axis events by Wayland pointer frame and forwarding
a wheel event into Rune's existing runtime. It was subsequently refined and
promoted as described below.

On the Linux VM, the revised prototype's seven pure conversion tests passed
when compiled directly with `rustc --test`, and
`cargo check --locked -p rune --lib` passed in the pinned shell with two jobs;
existing Rune/vendor warnings were emitted. These tests do not exercise the
full Wayland event path. Incidental formatting changes in the revised prototype
were removed before commit. A temporary Snowbar
copy using the earlier prototype was built on Linux and run in the existing
niri VM on `Virtual-1`. The user reported that
horizontal scrolling worked and looked good, though it felt a little slow.
The user also reported that Shift plus vertical trackpad scrolling did not
move the strip; no physical mouse wheel was available for testing. This
observation does not establish wheel behaviour, gesture correctness across
devices, or passing automated input tests.

Open questions include correct signs, units and detent scaling; finger gesture
phases and per-axis stop events; multiple axis values in one frame; older
pointer protocol versions; and modifier availability. In particular, the bar
does not request keyboard focus, so Shift state must not be assumed available
to its layer-shell input handler. The prototype's current scaling and
Shift-wheel implementation need validation, not adoption as established
behaviour. Tune speed only after checking how device deltas are represented,
so a trackpad fix does not make a mouse wheel jump excessively. These concerns
and Linux/VM tests must be resolved before changing
the Rune pin or removing Snowbar's fallback controls.

The likely maintenance surface is localized to Wayland layer input and overlay
forwarding, and the feature is broadly relevant to Rune overlays. An additive,
upstreamable fix may reduce long-term fork drift, but code size, compatibility
and maintenance cost remain provisional until a correct implementation is
compiled and tested on Linux and reviewed against upstream.

## Candidate scope

The user approved a narrow candidate for ordinary layer-shell scroll events,
with focused deterministic tests and Linux/VM verification. Shift plus
vertical scrolling and any keyboard-focus changes are explicitly deferred.
The candidate was committed and pushed as Rune
`99fa00ea05e46c6f6dbe0118f9ad07957d4cca93` and is pinned by Snowbar. Shift
and keyboard-focus changes remain deferred.

## Candidate review result

The reviewed candidate initially changed four Rune files relative to fork
revision `9600140`. Before commit, tests moved to a sibling file, a changelog
entry was added, and gesture handling was refactored to follow Rune's state
machine guidance; the final commit changes six files (+466/-1). Nine
standalone conversion tests and the Linux `cargo check -p rune --lib` passed
in the pinned shell. The user reported that horizontal scrolling and workspace
clicks looked good in the VM with the final pinned Snowbar build. A physical mouse wheel,
Shift plus vertical scrolling, the full Rune test binary and full Wayland
event-path automation were not verified.

For context, the existing fork revision differs from its upstream merge parent
`55ef5268f22038a95344b3e9d2b4a25ddba731b1` by five files (+74/-12),
including the existing Snowbar probe. This historical comparison is not a
comparison with current upstream. Neither the candidate nor the existing fork
should be described as a measured diff against today's upstream without
performing that separate comparison.
