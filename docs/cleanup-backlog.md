# Snowbar cleanup backlog

Collected during the 2026-11-17 Rune upstream integration session:
- Visual/VM review of `snowbar` built with Rune `09677837047eefdbae27bd4690a6a7ea9453aaf1`.
- Code review by `opencode-go/kimi-k3.1-pro`.
- Rune debug runtime performance hint from the VM preview output.

These are investigation leads from a source-only review and a debug hint, not
confirmed defects or prescribed fixes. Preserve the approved visual behavior.

## Status after cleanup

- Event filtering, paint-only workspace hover, and graceful worker spawn failure
  were implemented with focused tests. The user reviewed the candidate in the
  niri VM and reported it worked as expected.
- Blocking socket cancellation and worker teardown remain unresolved. Queue
  bounds and coalescing were deferred pending evidence and a safe design.
- Heavy all-target and Rune test links were not run; see
  `docs/rune-integration.md` for resource limits.

## 1. Filter irrelevant niri IPC events and wakeups (recommended first)
- **Files:** `src/niri.rs`, `src/workspaces.rs`, `src/main.rs`
- **What:** `session_once` forwards niri stream events via `publish`, including events unrelated to workspaces, over an unbounded channel and wakes the UI.
- **Also:** `WorkspaceUrgencyChanged` and `WorkspaceActiveWindowChanged` mutate `WorkspaceState` (`src/workspaces.rs`) and trigger view redraws, but neither field is rendered in the bar.
- **Investigate:** filter irrelevant events before enqueueing and waking; separately assess redraws for workspace updates that do not change rendered output. Verify reducer state and ordering requirements before dropping any events, and measure the effect.

## 2. `on_hover_change` view-rebuild hint from Rune
- **Files:** `src/main.rs` (around `workspace-{i}` `.on_hover_change`, Rune reported `src/main.rs:154:10` in the debug preview)
- **Runtime hint (verbatim):**
  ```
  rune: `on_hover_change` set at src/main.rs:154:10 committed the view for 8
  elements (as "#0/#0/workspaces-scroll/#0/workspace-9"): each one the pointer
  crosses runs the whole view; paint hover with `El::hover`, `hover_group`
  and `group_*` styles (`hover_alone` for a target inside a lit row) instead
  of state, or say `Tx::live_only` when the handler only sets live values.
  (debug builds only; RUNE_PERF_WARN=0 silences)
  ```
- **Investigate:** whether `El::hover` / `hover_group` styles can replace the `State.hovered_workspace` handler without changing the approved warm-gold highlight or interaction. Element keys already use stable workspace IDs. `Tx::live_only` applies only when a handler changes live values; it is not appropriate for ordinary `State.hovered_workspace` mutations. Verify both appearance and redraw behavior.

## 3. Socket read/write timeouts, worker lifecycle, and thread spawn errors
- **Files:** `src/niri.rs`
- **What:** `niri-ipc::Socket::send` / `read_events` use blocking `read_line` with no socket read/write timeouts; a hung compositor socket could block the detached worker threads past `Drop`'s atomic shutdown flag (normal socket-close/EOF unwinds cleanly). Worker `.expect("...spawn")` panics on OS thread-exhaustion instead of marking workspaces unavailable.
- **Investigate:** available timeout and cancellation mechanisms, explicit worker ownership and teardown, and graceful handling of spawn failures. Idle event streams are normal, so timeout behavior needs care. After an observed disconnect, preserve generation gating: already-authorized in-flight actions cannot reliably be recalled.

## 4. Bounded IPC queues (only if needed after event filtering)
- **Files:** `src/niri.rs`, `src/main.rs`
- **What:** event and focus-intent channels are unbounded; if the UI event loop stalls while the compositor is very busy, event backlog can accumulate.
- **Investigate:** after filtering (item 1), assess bounded channels or snapshot coalescing if needed. Neither is a prescribed fix: analyze ordering, disconnect and generation transitions, dropped intents, blocking and deadlock risks before choosing a design. Preserve `FocusGate` semantics.

## Explicitly NOT requested in cleanup
- No heavy full-Rune test binaries on the Mac or VM (memory policy in `AGENTS.md`).
- Keep verified workspace focus-by-stable-ID, generation gate, single-output mapping, glass visuals, and Escape/menu toggle behavior intact and re-verify the VM checklist (`docs/rune-integration.md`) before committing any behavioral changes.
- Hotplug, exact surface placement and real reconnect behavior remain unverified; do not imply the previous visual review established them. See `docs/workspace-policy.md` and `docs/rune-layer-scroll-investigation.md`.
