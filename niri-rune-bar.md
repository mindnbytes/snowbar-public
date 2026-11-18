# Niri Rune Bar — Project Brief

**Status:** Implementation guide; workspace, clock and power menu slices implemented · **Updated:** 2026-11-18

## Current status (2026-11-18)

The initial reuse/feasibility assessment is complete; `docs/reuse-decision.md`
holds its evidence and history. It is a decision record, not the dependency pin:
`Cargo.toml` names the tested Rune fork revision, and `docs/rune-integration.md`
describes how to verify a checkout. `docs/README.md` indexes the current notes.

The feasibility gate is complete: a Rune layer-shell surface reserves top space
and receives pointer input and on-demand keyboard focus in niri. Workspaces are
implemented and reviewed for a single output, and the clock shows local time and
updates on minute boundaries. The three-action power menu now has typed effects,
explicit confirmation and single-flight feedback; the user reported that it
worked in the VM. Its confirmation, pending and result copy now share a bounded
wrapped layout tested headlessly at 1×, 1.25×, 1.5× and 2×; the user approved
the corrected confirmation layout in the VM. An open menu can
remain above a fullscreen window even when the bar hides. The Rune prototype
for that edge case is parked on a separate branch; Snowbar keeps the current
pin. See [power menu policy](docs/power-policy.md) for verification and the
deferred prototype, and [workspace policy](docs/workspace-policy.md) for
workspace and output boundaries.

## Goal

Build a beautiful, restrained, macOS-inspired **top bar** for **NixOS + niri**, using **Rust + Rune** for its UI. This is a small, self-contained desktop utility—not a desktop environment, Noctalia replacement project, or general-purpose widget framework.

The initial bar has exactly **three widgets**:

| Position | Widget | Behavior |
|---|---|---|
| Left | **Workspaces** | Display niri workspaces, distinguish the active one, click to focus a workspace. |
| Center | **Clock** | Display local time, update accurately on minute changes. |
| Right | **Power menu** | Open a compact menu with **Log out**, **Restart**, **Shut down**. Require confirmation for disruptive actions. |

No other widgets in v1. Specifically: no tray, notifications, network/audio controls, weather, calendar, application launcher, or configuration GUI.

## Desired experience

- **Workspace visual reference:** the approved oversized study in
  `examples/workspace_study.rs` and its assets is the current source of truth
  for the dimensional numerals, transparent glass-like capsules, warm hover
  and selected treatments, and press-scale interaction. Check the scaled bar
  in the VM before treating it as finished; the illustrated reflections are
  not a claim of live compositor blur or refraction.
- **Rune visual research:** The user preferred the SVG capsule and numeral in
  the exploratory geometry comparison. Keep that artwork as the baseline while
  improving semantic colors, layout and accessibility; see
  `docs/rune-visual-notes.md` for the concise findings and limitations.
- **Approved bar mockup:** the static studies remain the visual baseline for
  the functional bar. The studies use mock data and actions; the main bar
  connects all three widgets to real behavior.
- Elegant macOS-inspired appearance, **not necessarily a literal macOS clone**: carefully tuned typography, subtle translucency *if technically feasible*, spacing, rounded popover, restrained animations, polished hover/active states.
- Calm and unobtrusive; keep niri in charge of window management and workspace animations.
- Top-edge bar with appropriate Wayland layer-shell anchoring and reserved space (avoid overlapping tiled windows).
- Initially target **one monitor**; do not accidentally design against multi-monitor support, but don't implement it prematurely.
- No heavy polling, unnecessary background daemons, or excessive dependencies.
- If blur requires compositor support not available in niri/Rune, use a tasteful opaque or translucent fallback. Never fake a capability claim.

**Open visual details** (choose during visual prototyping): height, colors, typography, workspace shape/labels, shadow, whether the power menu is a popover or separate layer-shell surface. Prefer screenshots and comparison over speculative design docs.

## Architecture: minimal by design

```text
niri IPC event stream ──> workspace state ─┐
                                          │
local clock ───────────────> clock state ──┼──> Rune view / event handlers
                                          │         │
user clicks ───────────────> UI commands ─┘         │
                                                    v
                                   niri IPC / systemd-logind actions

              Wayland layer-shell: positioning, focus, exclusive zone
```

- **Single application/process initially.** Three widget modules do not imply three executables or crates.
- Keep **pure state transformations** separate from side effects (niri socket, time source, session commands).
- Use typed data and commands. The view must not spawn arbitrary shell snippets or perform blocking system calls.
- Subscribe to **niri IPC event-stream** for live workspace updates; use the documented niri IPC action for click-to-focus. Account for niri's *dynamic*, per-output workspaces and its trailing empty workspace. Define the display policy deliberately.
- Clock should wake near minute boundaries, and refresh after suspend/resume or clock/time-zone changes as needed; don't repaint every frame.
- Power menu should use explicit, safely invoked actions with user confirmation. Prefer the appropriate session/logout and system power interfaces; validate which work in the actual niri session. Never run a destructive action in automated tests.
- Handle missing niri socket, reconnects, failed power commands, and loss of Wayland connection without panicking or leaving stale UI indefinitely.

Do **not** invent a generic plugin registry, event bus, IPC daemon, elaborate configuration schema, or reusable design system for three widgets. Extract abstractions only when actual duplication warrants them.

### Clock timing

The clock reads the system wall clock and its local time once, when a
minute-boundary wake is due, and displays `HH:MM` (24-hour, zero-padded) from
that sample. The wake uses Rune's `Tx::wake_after` on the overlay's poll: no
worker thread, timer service or shell command, and frames caused by pointer or
IPC activity do no clock read. The next deadline is computed from each fresh
sample, so a late wake, forward or backward step, resume, or time-zone change is
corrected by the next absolute sample instead of replaying elapsed minutes. The
wait comes from the sample's own local second and fraction, so a POSIX zone with
a seconds offset still changes on its local minute boundaries. A sample whose
`HH:MM` is unchanged does not re-run the view. The clock never depends on niri
IPC; it displays from the first frame.

The trade-off: changes the timer is not event-driven for (NTP correction,
resume, `/etc/localtime` retargeting) can be stale for up to about one minute of
awake time, until the next boundary wake. Each sample calls `tzset()` before
`localtime_r`; glibc then re-reads `/etc/localtime` with `TZ` unset, as on
NixOS. If `TZ` names a file that is retargeted in place, glibc may keep its
cached rules until restart. The glibc path was checked against its
implementation; live time-zone changes and suspend/resume were not tested.

On 2026-11-18 the user reviewed the temporary clock bar in the single-output
niri VM and confirmed that it looked right and the time advanced. The preview
was stopped after review.

## Reuse-first investigation — before implementation

We are especially interested in retaining a **well-engineered backend** while replacing its visual presentation with Rune. But UI and application architectures can be tightly coupled; a “UI swap” is a hypothesis, **not an assumption**.

Inspect these references at pinned commits:

| Reference | Why inspect | Initial assessment |
|---|---|---|
| [Hokum](https://github.com/anomalyco/hokum) | Existing Rune + Wayland layer-shell app: platform plumbing, popovers, rendering/test patterns | **First choice for learning or selectively reusing Rune/Wayland integration**; not a niri workspace-bar backend. MIT. |
| [Ashell](https://github.com/MalpenZibo/ashell) | Mature Rust bar with niri workspaces, clock, power menu | **Strong behavioral/architectural reference**, but Iced-based UI and broader scope make a wholesale fork questionable. GPL-3.0; assess license implications before copying code. |
| [Ironbar](https://github.com/JakeStanger/ironbar) | Established Rust GTK4 bar; niri integration is described as partial | Reference for widget boundaries, less attractive for swapping to Rune. |
| [niri IPC documentation](https://github.com/niri-wm/niri/blob/main/docs/wiki/IPC.md) and [niri-ipc source](https://github.com/niri-wm/niri) | Official events, workspace IDs, commands, compatibility expectations | **Preferred source of truth** for workspace backend. Evaluate using `niri-ipc` at a compatible version instead of cloning a bar's IPC implementation. |
| [Rune](https://github.com/anomalyco/rune) | UI framework, headless rendering, Linux platform support | Pin a development revision and validate actual layer-shell support in a running niri VM. |

### First agent task: short technical assessment (completed)

Completed; see `docs/reuse-decision.md`. That note contains source-review
evidence, incremental probe results and historical commit IDs. It is not a
current-pin audit, and its early conclusions are retained as history.

Before scaffolding an application, inspect code, licenses, dependency graphs, and maintenance status of Hokum and Ashell (and the relevant niri IPC library). Answer:

1. Can Hokum's Rune/Wayland layer-shell handling be reused **without importing the entire application**?
2. Does Ashell genuinely separate niri workspace logic from Iced state/view types? If so, can that part be extracted cleanly, legally, and with less code than a fresh adapter?
3. Does Rune support the required layer-shell **bar plus interactive power popover** and exclusive-zone behavior, including keyboard/pointer focus, at the pinned revision?
4. What is the **smallest viable implementation** that still handles disconnections and dynamic workspaces correctly?

Deliver a short decision note (`docs/reuse-decision.md`) with **specific code paths and commit IDs**, a dependency/licensing assessment, and one recommended approach. **Do not fork a whole bar merely to inherit three tiny widgets.**

**Provisional preference:** write a small application using Rune + official niri IPC types/event stream; selectively adapt Hokum's proven Wayland integration where practical. Reuse source code only if the engineering and license economics are clearly favorable.

## Development process

Work in small, verifiable vertical slices:

1. **Feasibility gate:** A Rune surface renders as a top-anchored layer-shell bar in niri, respects the exclusive zone, and receives clicks. Verify in the existing NixOS VM. Stop and reconsider Rune if this requires a large framework fork.
2. **Static visual prototype:** Three widgets populated with mock data. Create a screenshot and iterate on appearance; include opened power menu.
3. **Workspaces:** Connect niri IPC event stream; render real workspaces; click to focus; test workspace creation/removal, focus changes and restart/reconnect.
4. **Clock:** Correct local display and minute-boundary update; verify without busy polling.
5. **Power menu:** Add log out/restart/shutdown with confirmations and visible failure feedback; mock the commands in tests.
6. **Packaging:** Reproducible `nix develop`, `nix build`, and an opt-in Home Manager integration/user service. Test with a clean VM session.

Each slice must leave the bar buildable and usable. No massive speculative scaffolding.

### Configuration-aware implementation

We expect a small, agent-friendly, human-readable configuration file later,
including a way to adapt the approved appearance to a user's color palette.
This is a design constraint, **not a request to implement a parser, configuration
GUI, theme engine, or extensive schema now**. Candidate settings live in
`docs/configuration-candidates.md`; decide file format, locations, defaults,
validation and reload behavior when that slice is actually planned.

- As functionality is added, keep visual decisions in a small, coherent place
  rather than spreading more literal colors and sizes through event handling.
  Prefer a few semantic palette roles (for example surface, text, accent and
  highlight) to making every gradient stop a separate setting.
- The current dimensional numeral and glass SVGs contain baked-in gradients,
  highlights and shadows. Before promising palette customization, investigate
  a minimal, safe way to derive or render those assets from semantic colors
  while preserving the approved appearance. Avoid blind replacement of arbitrary
  SVG text and avoid regenerating assets in the render loop; validate with at
  least one alternative palette and visual comparison. If fixed artwork limits
  what can be recolored, document that limitation rather than claiming full
  theming support.
- The artwork now covers every digit `0`–`9`, and the nine-workspace study
  (`examples/nine_workspace_bar.rs`) shows more than four. That does not
  establish a four-workspace limit or a real clock: its layout, sizing and
  minute updates are still mocked, and niri may expose more workspaces,
  multi-digit indices, or names. Choose a readable fallback/display policy
  before connecting those paths; never silently drop workspaces or assume four
  is a maximum.
- Keep configuration, view state and side effects separate enough that a later
  configuration layer can supply validated values without rewriting niri IPC,
  clock or power logic. Add only the small structure justified by the current
  slice and retain the current approved palette as the default.

### Verification

- Rust formatting, Clippy, focused unit tests, and a clean build.
- Deterministic state tests (workspace events, active state, empty/dynamic workspace policy, reconnection).
- Rune headless screenshots where supported by the selected revision; fixed fonts, size, and mock state.
- Integration checks **in niri** for layer-shell behavior, focus, clicks, fullscreen, and restart recovery.
- Never execute logout/reboot/shutdown as part of automated tests.
- Agent should present the screenshot or describe observed behavior and any untested limitations, not merely report that compilation passed.

## Working with OpenCode

- Read this file and relevant upstream documentation before proposing major changes.
  The initial reuse/feasibility assessment is complete (`docs/reuse-decision.md`);
  use its evidence and stated limits rather than treating it as an open prerequisite.
- Prefer the minimal working solution; explain trade-offs before adding crates/processes/abstractions.
- Prefer delegating straightforward and easily testable pieces of work to subagent, use OpenAI GPT-6 Luna.
- Pin Rune and other unstable dependencies. Keep `Cargo.lock` and `flake.lock` committed.
- Use `nix develop` + `cargo` for rapid iteration; reserve `nix build` for packaging validation. Do not rebuild all of NixOS after every UI edit.
- Work on the bar repository independently of the host's main NixOS configuration; never modify a live session or perform power actions without explicit approval.
- Update this document when decisions materially change; capture important choices in a **short** ADR only when justified.

## Initial definition of done

A cleanly packaged bar starts in niri, looks polished, shows and switches the correct workspaces, shows local time, and provides a functioning three-action power menu with confirmation. It survives routine workspace changes and reports errors sensibly. No additional features are needed.

## References

- NixOS VM (existing reference implementation, **not an extension of this project**): https://github.com/mindnbytes/nixos-vm
- Rune: https://github.com/anomalyco/rune
- Hokum: https://github.com/anomalyco/hokum
- Ashell: https://github.com/MalpenZibo/ashell
- Ironbar: https://github.com/JakeStanger/ironbar
- niri IPC: https://github.com/niri-wm/niri/blob/main/docs/wiki/IPC.md
