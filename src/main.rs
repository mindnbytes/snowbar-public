use rune::tok::motion;
use rune::{
    Align, Anim, App, AppSpec, Color, Cx, DisplayTarget, El, Image, ImageCache, Key, KeyEvent,
    KeyPolicy, NativeDisplayId, OverlayInput, OverlayOptions, OverlayPlacement, OverlaySpec,
    ScreenEdge, TextStyle, Tx, WindowSpec, col, el, row, text,
};
use snowbar::clock::{self, ClockTime};
use snowbar::niri::NiriClient;
use snowbar::power::{
    Dispatch, MenuState, PowerAction, PowerError, PowerEvent, PowerMenu, PowerOutcome,
};
use snowbar::workspaces::{DisplaySnapshot, WorkspaceItem, WorkspaceState};
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

/// Production-only effects. The `cfg` keeps the real niri/logind runner out of
/// the test build entirely; tests drive the UI with fake channels instead.
#[cfg(not(test))]
mod power_os;

const INK: Color = Color {
    r: 0.161,
    g: 0.180,
    b: 0.169,
    a: 1.0,
};
const MUTED: Color = Color {
    r: 0.48,
    g: 0.50,
    b: 0.49,
    a: 1.0,
};
const PAPER: Color = Color {
    r: 0.98,
    g: 0.965,
    b: 0.933,
    a: 1.0,
};
const STONE: Color = Color {
    r: 0.87,
    g: 0.86,
    b: 0.82,
    a: 1.0,
};
const MOSS: Color = Color {
    r: 0.35,
    g: 0.43,
    b: 0.36,
    a: 1.0,
};
// Warm brown from the VM's Mist & Stone palette.
const WARM: Color = Color {
    r: 0.604,
    g: 0.459,
    b: 0.298,
    a: 1.0,
};
const LABEL: TextStyle = TextStyle::new(15.5, 700.0);
const SECONDARY: TextStyle = TextStyle::new(15.5, 400.0);
const FALLBACK_DIGIT: TextStyle = TextStyle::new(13.0, 700.0);
const WORKSPACE_AREA_WIDTH: f32 = 300.0;
/// Approved clock glyph slots, left to right: H tens, H ones, M tens, M ones.
/// The colon sits at x = 40 between the second and third slots.
const CLOCK_DIGIT_X: [f32; 4] = [0.0, 17.0, 43.0, 60.0];

/// Power popover geometry, shared by the view, its overlay and headless tests. Every
/// sentence is wrapped to `MENU_MESSAGE_WIDTH`, so a long backend error cannot run
/// under the right edge or push the footer out of the popover.
const MENU_WIDTH: f32 = 246.0;
const MENU_MIN_HEIGHT: f32 = 222.0;
const MENU_SIDE_INSET: f32 = 8.0;
const MENU_BOTTOM_INSET: f32 = 12.0;
const MENU_CONTENT_PAD: f32 = 14.0;
const MENU_GAP: f32 = 8.0;
const MENU_ACTION_HEIGHT: f32 = 34.0;
const MENU_FOOTER_HEIGHT: f32 = 30.0;
/// One line of body text plus rounding. A fractional output scale can re-break a
/// wrapped line by a few device pixels, so reserve a whole extra line rather than
/// guessing a sub-pixel margin.
const MENU_HEIGHT_SLACK: f32 = 20.0;
/// Content width after the overlay inset and the card padding.
const MENU_MESSAGE_WIDTH: f32 = MENU_WIDTH - 2.0 * (MENU_SIDE_INSET + MENU_CONTENT_PAD);

#[derive(Default)]
struct State {
    hovered_clock: bool,
    hovered_power: bool,
    cache: Option<ImageCache>,
    glass: Option<Image>,
    clock_glass: Option<Image>,
    power_art: Option<Image>,
    /// Committed local time and the deadline of its next minute sample.
    clock: clock::Display,
    /// Artwork for indices 0..=9; higher indices use a readable text fallback.
    numerals: Vec<Image>,
    workspaces: WorkspaceState,
    focus_tx: Option<mpsc::Sender<snowbar::workspaces::FocusIntent>>,
    /// Owns the IPC worker threads and the ordered message channel.
    ipc: Option<NiriClient>,
    /// Pure selection/confirmation/pending/result machine.
    power: PowerMenu,
    /// On-demand dispatch channel to the production worker.
    power_tx: Option<mpsc::Sender<Dispatch>>,
    /// Worker completions drained by the power overlay's poll.
    power_rx: Option<mpsc::Receiver<PowerEvent>>,
    /// Set when the production worker could not start; confirmation then
    /// fails visibly instead of pretending to dispatch.
    power_start_error: Option<PowerError>,
    /// Fixed observations for headless tests; production samples the system.
    #[cfg(test)]
    clock_samples: std::collections::VecDeque<clock::Sample>,
}

static MENU: OnceLock<Mutex<Option<rune::OverlayHandle<State>>>> = OnceLock::new();
static MENU_OPEN: AtomicBool = AtomicBool::new(false);
/// Rune's startup display snapshot, handed to the IPC worker by `setup`.
static DISPLAYS: OnceLock<Vec<DisplaySnapshot>> = OnceLock::new();

fn menu_handle() -> Option<rune::OverlayHandle<State>> {
    MENU.get()?.lock().ok()?.clone()
}

fn clock_digit(image: Option<&Image>) -> El {
    match image {
        Some(image) => el().size(25.0, 27.5).pixels(image.pixels()),
        None => el().size(25.0, 27.5),
    }
}

/// One approved clock slot. Without a committed sample the slot stays an empty
/// 25×27.5 box, so the capsule and spacing never move. The digits reuse the
/// 0–9 artwork; no clock-specific zero or nine is cached.
fn clock_slot(state: &State, slot: usize) -> El {
    let digit = state
        .clock
        .shown()
        .map(ClockTime::digits)
        .map(|digits| digits[slot]);
    clock_digit(digit.and_then(|digit| state.numerals.get(usize::from(digit))))
}

/// The approved numeral artwork for 0..=9, or a readable numeric label for
/// multi-digit/missing glyphs. No workspace is ever dropped for lack of art.
fn workspace_numeral(state: &State, index: u8) -> El {
    match state.numerals.get(usize::from(index)) {
        Some(image) => el().size(25.0, 27.5).pixels(image.pixels()),
        None => el()
            .size(30.0, 27.5)
            .center()
            .child(text(index.to_string()).font(FALLBACK_DIGIT).color(INK)),
    }
}

fn workspace_button(state: &State, item: WorkspaceItem) -> El {
    let id = item.id;
    let active = item.is_active;
    let glass = match &state.glass {
        Some(image) => el()
            .size(38.0, 38.0)
            .pixels(image.pixels())
            .absolute(0.0, 0.0),
        None => el().size(38.0, 38.0).absolute(0.0, 0.0),
    };
    // Hover is paint-only here: state would commit the whole view as the
    // pointer crosses each capsule. Group-opacity layers draw the same warm
    // shadow and wash the state-driven version drew, over the same 38×38
    // target, so the edges still light. `INSTANT` keeps the landed look of the
    // approved highlight instead of adding a fade.
    let mut button = el()
        .key(format!("workspace-{id}"))
        .size(38.0, 38.0)
        .center()
        .radius(10.0)
        .fill(Color::TRANSPARENT)
        .shadow(
            0.0,
            5.0,
            if active {
                WARM.alpha(0.50)
            } else {
                Color::TRANSPARENT
            },
        )
        .press_scale(0.95)
        .motion(motion::SNAPPY)
        .hover_group()
        .hover_motion_within(Anim::INSTANT, Anim::INSTANT);
    if !active {
        // A selected capsule already casts the warm shadow; a second layer
        // would double it, so this is only added while it would change paint.
        button = button.child(
            el().size(38.0, 38.0)
                .radius(10.0)
                .fill(Color::TRANSPARENT)
                .shadow(0.0, 5.0, WARM.alpha(0.50))
                .group_opacity(0.0, 1.0)
                .absolute(0.0, 0.0),
        );
    }
    button
        .child(
            el().size(34.0, 34.0)
                .radius(10.0)
                .fill(if active {
                    WARM.alpha(0.72)
                } else {
                    Color::TRANSPARENT
                })
                .absolute(2.0, 2.0),
        )
        .child(glass)
        .child(
            el().size(30.0, 30.0)
                .radius(9.0)
                .fill(WARM.alpha(0.18))
                .group_opacity(0.0, 1.0)
                .absolute(4.0, 4.0),
        )
        .child(workspace_numeral(state, item.index))
        .on_press(move |s: &mut State| {
            if let (Some(tx), Some(intent)) = (s.focus_tx.as_ref(), s.workspaces.focus_action(id)) {
                let _ = tx.send(intent);
            }
        })
}

/// Keep the scrollable workspace strip bounded away from the clock.
fn workspace_area(state: &State) -> El {
    if !state.workspaces.is_available() {
        return el()
            .key("workspaces-unavailable")
            .w(WORKSPACE_AREA_WIDTH)
            .h(40.0)
            .center()
            .child(text("niri unavailable").font(SECONDARY).color(MUTED));
    }
    let mut strip = row().gap(2.0).h(40.0).items_center();
    for item in state.workspaces.view() {
        strip = strip.child(workspace_button(state, item));
    }
    row()
        .key("workspaces-scroll")
        .w(WORKSPACE_AREA_WIDTH)
        .h(40.0)
        .scroll_x()
        .scroll_edge_fade(12.0)
        .child(strip)
}

fn bar(state: &State, _: &mut Cx<State>) -> El {
    let power = el()
        .key("power")
        .size(38.0, 38.0)
        .center()
        .radius(10.0)
        .fill(Color::TRANSPARENT)
        .shadow(
            0.0,
            5.0,
            if state.hovered_power {
                WARM.alpha(0.50)
            } else {
                Color::TRANSPARENT
            },
        )
        .press_scale(0.95)
        .motion(motion::SNAPPY)
        .child(match &state.glass {
            Some(image) => el()
                .size(38.0, 38.0)
                .pixels(image.pixels())
                .absolute(0.0, 0.0),
            None => el().size(38.0, 38.0).absolute(0.0, 0.0),
        })
        .child(
            el().size(30.0, 30.0)
                .radius(9.0)
                .fill(if state.hovered_power {
                    WARM.alpha(0.18)
                } else {
                    Color::TRANSPARENT
                })
                .absolute(4.0, 4.0),
        )
        .child(match &state.power_art {
            Some(image) => el().size(28.0, 30.8).pixels(image.pixels()),
            None => el().size(28.0, 30.8),
        })
        .on_hover_change(|s: &mut State, hovered, _| s.hovered_power = hovered)
        .on_press(|_: &mut State| toggle_power_menu());

    row()
        .w_full()
        .h_full()
        .px(18.0)
        .items_center()
        .fill(Color::TRANSPARENT)
        .child(row().w(WORKSPACE_AREA_WIDTH).child(workspace_area(state)))
        .child(
            row().grow(1.0).items_center().justify_center().child(
                el().key("clock")
                    .size(112.0, 38.0)
                    .center()
                    .radius(19.0)
                    .hit()
                    .fill(Color::TRANSPARENT)
                    .shadow(
                        0.0,
                        5.0,
                        if state.hovered_clock {
                            WARM.alpha(0.50)
                        } else {
                            Color::TRANSPARENT
                        },
                    )
                    .child(match &state.clock_glass {
                        Some(image) => el()
                            .size(112.0, 38.0)
                            .pixels(image.pixels())
                            .absolute(0.0, 0.0),
                        None => el().size(112.0, 38.0).absolute(0.0, 0.0),
                    })
                    .child(
                        el().size(102.0, 30.0)
                            .radius(15.0)
                            .fill(if state.hovered_clock {
                                WARM.alpha(0.18)
                            } else {
                                Color::TRANSPARENT
                            })
                            .absolute(5.0, 4.0),
                    )
                    .child(
                        el().size(85.0, 27.5)
                            .child(clock_slot(state, 0).absolute(CLOCK_DIGIT_X[0], 0.0))
                            .child(clock_slot(state, 1).absolute(CLOCK_DIGIT_X[1], 0.0))
                            .child(
                                col()
                                    .w(4.0)
                                    .gap(4.0)
                                    .items_center()
                                    .child(el().size(3.0, 3.0).radius(1.5).fill(PAPER))
                                    .child(el().size(3.0, 3.0).radius(1.5).fill(PAPER))
                                    .absolute(40.0, 10.0),
                            )
                            .child(clock_slot(state, 2).absolute(CLOCK_DIGIT_X[2], 0.0))
                            .child(clock_slot(state, 3).absolute(CLOCK_DIGIT_X[3], 0.0)),
                    )
                    .on_hover_change(|s: &mut State, hovered, _| s.hovered_clock = hovered),
            ),
        )
        .child(row().w(WORKSPACE_AREA_WIDTH).justify_end().child(power))
}

/// Wrapped size of one power sentence at the shared popover width, in the exact
/// style the view draws. `measure_with` walks Rune's actual shaping/layout
/// (including the wrap-anywhere option the element uses), so the height the
/// overlay reserves and the height the regression test checks are the same
/// metric.
fn menu_text_height(content: &str, style: TextStyle) -> f32 {
    rune::text::measure_with(
        content,
        &[],
        style,
        Some(MENU_MESSAGE_WIDTH),
        rune::text::RichLayoutOptions::new().wrap_anywhere(),
    )
    .1
}

/// The popover's size for the current state. The common states stay at the approved 246×222;
/// a long error summary grows the surface instead of clipping the sentence or covering the
/// footer. The view and overlay both use this function, and the headless tests build the
/// runtime at its result.
fn menu_size_for(state: &MenuState) -> (f32, f32) {
    let header = menu_text_height("Power", LABEL);
    let (body, gaps) = match state {
        MenuState::List => (3.0 * MENU_ACTION_HEIGHT, 3.0),
        MenuState::Confirming(action) => (
            menu_text_height(action.prompt(), SECONDARY)
                + menu_text_height(action.detail(), SECONDARY)
                + MENU_FOOTER_HEIGHT,
            4.0,
        ),
        MenuState::Pending(action) => (
            menu_text_height(action.pending_line(), SECONDARY)
                + menu_text_height("Closing this menu does not cancel the request.", SECONDARY)
                + MENU_ACTION_HEIGHT,
            4.0,
        ),
        MenuState::Result { action, outcome } => (
            menu_text_height(&outcome.summary(*action), SECONDARY) + MENU_ACTION_HEIGHT,
            3.0,
        ),
    };
    let content = 2.0 * MENU_CONTENT_PAD + header + gaps * MENU_GAP + body;
    let height = (MENU_BOTTOM_INSET + content + MENU_HEIGHT_SLACK)
        .max(MENU_MIN_HEIGHT)
        .ceil();
    (MENU_WIDTH, height)
}

/// A body sentence in the popover. The explicit width and wrap-anywhere policy are shared by
/// confirmation, pending and result copy so an unbroken sanitized error still wraps inside.
fn menu_message(content: impl Into<String>) -> El {
    text(content)
        .font(SECONDARY)
        .w(MENU_MESSAGE_WIDTH)
        .wrap_anywhere()
}

fn menu(state: &State, _: &mut Cx<State>) -> El {
    let mut content = col()
        .w_full()
        .padding(MENU_CONTENT_PAD)
        .gap(MENU_GAP)
        .radius(14.0)
        .fill(PAPER.alpha(0.97))
        .border(1.5, MOSS.alpha(0.55))
        .shadow(3.0, 12.0, INK.alpha(0.25))
        .child(text("Power").font(LABEL).color(INK));

    match state.power.state() {
        MenuState::List => {
            for action in PowerAction::ALL {
                content = content.child(button(action.label()).key(action_key(action)).on_press(
                    move |s: &mut State| {
                        if s.power.select(action) {
                            s.sync_power_menu_size();
                        }
                    },
                ));
            }
        }
        MenuState::Confirming(action) => {
            content = content
                .h_full()
                .child(menu_message(action.prompt()).key("power-prompt").color(INK))
                .child(
                    menu_message(action.detail())
                        .key("power-detail")
                        .color(MUTED),
                )
                .child(el().grow(1.0))
                .child(
                    row()
                        .w_full()
                        .gap(MENU_GAP)
                        .child(
                            footer_button("Cancel", "power-cancel").on_press(|s: &mut State| {
                                s.power.cancel();
                                s.sync_power_menu_size();
                            }),
                        )
                        .child(
                            footer_button("Confirm", "power-confirm")
                                .on_press(|s: &mut State| s.confirm_power()),
                        ),
                );
        }
        MenuState::Pending(action) => {
            content = content
                .h_full()
                .child(
                    menu_message(action.pending_line())
                        .key("power-pending")
                        .color(INK),
                )
                .child(
                    menu_message("Closing this menu does not cancel the request.")
                        .key("power-pending-note")
                        .color(MUTED),
                )
                .child(el().grow(1.0))
                .child(button("Hide").key("power-hide").on_press(|_: &mut State| {
                    hide_power_menu();
                }));
        }
        MenuState::Result { action, outcome } => {
            content = content.h_full().child(
                menu_message(outcome.summary(*action))
                    .key(if outcome.is_accepted() {
                        "power-accepted"
                    } else {
                        "power-error"
                    })
                    .color(INK),
            );
            content = content.child(el().grow(1.0));
            content = content.child(if outcome.is_accepted() {
                button("Close")
                    .key("power-close")
                    .on_press(|_: &mut State| hide_power_menu())
            } else {
                button("Back to actions")
                    .key("power-back")
                    .on_press(|s: &mut State| {
                        s.power.cancel();
                        s.sync_power_menu_size();
                    })
            });
        }
    }

    col()
        .w_full()
        .h_full()
        .px(MENU_SIDE_INSET)
        .pb(MENU_BOTTOM_INSET)
        .child(content)
}

fn action_key(action: PowerAction) -> &'static str {
    match action {
        PowerAction::LogOut => "power-action-logout",
        PowerAction::Restart => "power-action-restart",
        PowerAction::ShutDown => "power-action-shutdown",
    }
}

fn button(label: &'static str) -> El {
    row()
        .w_full()
        .h(34.0)
        .px(10.0)
        .center_y()
        .radius(8.0)
        .fill(STONE.alpha(0.35))
        .hover(STONE.alpha(0.70))
        .hit()
        .child(text(label).font(SECONDARY).color(INK))
}

/// Footer controls are placed after a growing spacer, deliberately outside
/// every initial action row. A quick second click where an action row used to
/// be cannot land on Confirm.
fn footer_button(label: &'static str, key: &'static str) -> El {
    row()
        .key(key)
        .grow(1.0)
        .h(30.0)
        .center()
        .radius(8.0)
        .fill(STONE.alpha(0.35))
        .hover(STONE.alpha(0.70))
        .hit()
        .child(text(label).font(SECONDARY).color(INK))
}

fn menu_key(state: &mut State, event: &KeyEvent, _: &mut Tx) -> bool {
    if event.key == Key::Escape {
        // Only an unconfirmed selection is cleared. A pending request stays
        // pending and is shown on the next open.
        state.power.dismiss();
        hide_power_menu();
        true
    } else {
        false
    }
}

fn hide_power_menu() {
    if let Some(menu) = menu_handle() {
        MENU_OPEN.store(false, Ordering::Relaxed);
        menu.hide();
    }
}

/// The state half of the bar's power toggle, kept separate so the production
/// transition is exercised without a native overlay.
fn set_power_menu_open(state: &mut State, open: bool) -> bool {
    if open {
        state.power.on_open()
    } else {
        state.power.dismiss()
    }
}

fn toggle_power_menu() {
    let Some(menu) = menu_handle() else {
        return;
    };
    if MENU_OPEN.fetch_xor(true, Ordering::Relaxed) {
        menu.update(|state, _| {
            set_power_menu_open(state, false);
        });
        menu.hide();
    } else {
        menu.update(|state, tx| {
            set_power_menu_open(state, true);
            state.drain_power(tx);
            state.sync_power_menu_size();
        });
        menu.show();
        menu.request_keys();
    }
}

fn setup(state: &mut State, wake: rune::Waker) {
    let cache = state.cache.insert(ImageCache::new(wake.clone()));
    state.glass = Some(cache.svg(
        &include_bytes!("../examples/assets/workspace-glass.svg")[..],
        320.0,
        2.0,
    ));
    state.clock_glass = Some(cache.svg(
        &include_bytes!("../examples/assets/clock-glass.svg")[..],
        600.0,
        2.0,
    ));
    state.power_art = Some(cache.svg(
        &include_bytes!("../examples/assets/power.svg")[..],
        220.0,
        2.0,
    ));
    state.numerals = [
        &include_bytes!("../examples/assets/workspace-zero.svg")[..],
        &include_bytes!("../examples/assets/workspace-one.svg")[..],
        &include_bytes!("../examples/assets/workspace-two.svg")[..],
        &include_bytes!("../examples/assets/workspace-three.svg")[..],
        &include_bytes!("../examples/assets/workspace-four.svg")[..],
        &include_bytes!("../examples/assets/workspace-five.svg")[..],
        &include_bytes!("../examples/assets/workspace-six.svg")[..],
        &include_bytes!("../examples/assets/workspace-seven.svg")[..],
        &include_bytes!("../examples/assets/workspace-eight.svg")[..],
        &include_bytes!("../examples/assets/workspace-nine.svg")[..],
    ]
    .into_iter()
    .map(|svg| cache.svg(svg, 220.0, 2.0))
    .collect();

    if let Some(displays) = DISPLAYS.get() {
        // A worker that cannot be spawned leaves the UI unavailable instead of
        // panicking inside setup.
        if let Some(client) = NiriClient::start(displays.clone(), wake) {
            state.focus_tx = Some(client.focus_sender());
            state.ipc = Some(client);
        }
    }
}

/// Start the production power worker when the power overlay is created. The
/// test build deliberately has no runner: tests install fake channels instead.
fn menu_setup(state: &mut State, wake: rune::Waker) {
    #[cfg(not(test))]
    match power_os::start(wake) {
        Ok((tx, rx)) => {
            state.power_tx = Some(tx);
            state.power_rx = Some(rx);
        }
        Err(error) => state.power_start_error = Some(error),
    }
    #[cfg(test)]
    {
        let _ = (state, wake);
    }
}

/// The power overlay's on-demand poll. A completion also wakes the overlay;
/// draining on open is what makes feedback visible after a hidden response.
fn power_poll(state: &mut State, _app_clock: f64, tx: &mut Tx) -> bool {
    state.drain_power(tx)
}

/// Apply IPC messages and, when its minute deadline has arrived, one fresh clock
/// sample. Rune calls `poll` at the start of a frame; IPC workers and the clock
/// wake it through Rune. Neither source short-circuits the other, so a frame that
/// carries both applies both. No clock read happens on frames that only carry
/// input or unrelated IPC.
fn poll(state: &mut State, app_clock: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    if let Some(ipc) = state.ipc.as_ref() {
        let messages = ipc.drain();
        if !messages.is_empty() {
            changed |= apply_ipc_messages(state, messages);
        }
    }
    if state.clock.is_due(app_clock) {
        let sample = state.next_clock_sample();
        let advance = state.clock.advance(app_clock, sample);
        // Schedules the poll, not a view commit: an unchanged HH:MM returns
        // false below and runs no view.
        tx.wake_after(advance.wait);
        changed |= advance.changed;
    }
    if changed {
        tx.instant();
    }
    changed
}

impl State {
    /// Re-size the native popover after a state change. Test runtimes have no overlay
    /// handle; their viewport is built from `menu_size_for` instead.
    fn sync_power_menu_size(&self) {
        if let Some(menu) = menu_handle() {
            menu.resize(menu_size_for(self.power.state()));
        }
    }

    /// The explicit confirmation control's handler. `PowerMenu::confirm`
    /// enters pending before this method can touch the channel, so a reentrant
    /// click or reopen cannot dispatch twice.
    fn confirm_power(&mut self) {
        let Some(dispatch) = self.power.confirm() else {
            return;
        };
        self.sync_power_menu_size();
        self.dispatch_power(dispatch);
    }

    /// Hand one authorized dispatch to the worker. A missing or disconnected
    /// worker becomes a visible failure without blocking or retrying.
    fn dispatch_power(&mut self, dispatch: Dispatch) {
        let unavailable = self.power_start_error.clone();
        let result = match self.power_tx.as_ref() {
            Some(tx) => tx
                .send(dispatch)
                .map_err(|_| PowerError::WorkerDisconnected),
            None => Err(unavailable.unwrap_or(PowerError::WorkerDisconnected)),
        };
        if let Err(error) = result {
            self.power.apply_result(PowerEvent {
                attempt: dispatch.attempt,
                outcome: PowerOutcome::Failed(error),
            });
            self.sync_power_menu_size();
        }
    }

    /// Apply every queued completion in order. The attempt number in the
    /// event makes a late result from an abandoned attempt harmless.
    fn drain_power(&mut self, tx: &mut Tx) -> bool {
        let mut changed = false;
        if let Some(rx) = self.power_rx.as_ref() {
            while let Ok(event) = rx.try_recv() {
                changed |= self.power.apply_result(event);
            }
        }
        if changed {
            self.sync_power_menu_size();
            tx.instant();
        }
        changed
    }

    /// The next observation: a fixed one in headless tests, otherwise the
    /// system's current wall and local time.
    fn next_clock_sample(&mut self) -> clock::Sample {
        #[cfg(test)]
        if let Some(sample) = self.clock_samples.pop_front() {
            return sample;
        }
        clock::sample()
    }
}

/// What the workspace area actually draws. A snapshot can change names,
/// urgency or active-window IDs, and those must update state without asking
/// Rune to re-run the view: `poll` returns whether a frame needs one.
fn workspace_render_key(state: &State) -> (bool, Vec<(u64, u8, bool)>) {
    (
        state.workspaces.is_available(),
        state
            .workspaces
            .view()
            .into_iter()
            .map(|item| (item.id, item.index, item.is_active))
            .collect(),
    )
}

fn apply_ipc_messages(
    state: &mut State,
    messages: impl IntoIterator<Item = snowbar::workspaces::IpcMessage>,
) -> bool {
    let before = workspace_render_key(state);
    for message in messages {
        state.workspaces.apply(message);
    }
    workspace_render_key(state) != before
}

fn main() {
    rune::run_windows(|displays| {
        let wayland_displays: Vec<_> = displays
            .iter()
            .filter(|display| matches!(display.native_id(), Some(NativeDisplayId::Wayland(_))))
            .collect();
        // Strict single-output policy: only one targetable Rune display may
        // host the bar. Otherwise fail closed and show no bar at all.
        let single_target = (wayland_displays.len() == 1)
            .then(|| wayland_displays[0].id.clone())
            .flatten();
        let snapshots: Vec<DisplaySnapshot> = displays
            .iter()
            .map(|display| DisplaySnapshot {
                wayland_id: match display.native_id() {
                    Some(NativeDisplayId::Wayland(id)) => Some(id),
                    _ => None,
                },
            })
            .collect();

        let mut app =
            App::new(AppSpec::new("Snowbar", (900.0, 40.0), State::default, bar)).windowless();
        let options = OverlayOptions {
            input: OverlayInput::All,
            display: match &single_target {
                Some(id) => DisplayTarget::Id(id.clone()),
                None => DisplayTarget::Primary,
            },
            ..Default::default()
        };
        let bar_overlay = app.overlay(
            OverlaySpec::new("snowbar", (900.0, 40.0), bar)
                .options(options)
                .setup(setup)
                .poll(poll)
                .wayland_top_reserved(40),
            State::default(),
        );
        let menu_display = match &single_target {
            Some(id) => DisplayTarget::Id(id.clone()),
            None => DisplayTarget::Primary,
        };
        let menu_overlay = app.overlay(
            OverlaySpec::new("snowbar-power", menu_size_for(&MenuState::List), menu)
                .options(OverlayOptions {
                    // Only the keyed controls carry `.hit()`, so the menu's
                    // blank area and padding pass clicks through instead of
                    // covering the bar's power toggle.
                    input: OverlayInput::Regions,
                    display: menu_display,
                    keys: KeyPolicy::OnDemand,
                    placement: Some(OverlayPlacement::Edge {
                        edge: ScreenEdge::Top,
                        margin: 0.0,
                        align: Align::End,
                        inset: 8.0,
                    }),
                    ..Default::default()
                })
                .setup(menu_setup)
                .poll(power_poll)
                .on_key(menu_key),
            State::default(),
        );
        let _ = MENU.set(Mutex::new(Some(menu_overlay)));

        if single_target.is_some() {
            let _ = DISPLAYS.set(snapshots);
            bar_overlay.show();
        }
        vec![WindowSpec::from_app("snowbar", app)]
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use niri_ipc::Workspace;
    use rune::{Env, Runtime};
    use snowbar::workspaces::IpcMessage;
    use std::sync::Arc;

    fn fixed_sample(second: u8, nanos: u32, hour: u8, minute: u8) -> clock::Sample {
        clock::Sample {
            time: ClockTime::new(hour, minute).expect("valid clock time"),
            second,
            nanos,
        }
    }

    /// The production bar, setup and clock path with fixed observations. Image
    /// assets are decoded once so the paint comparison is deterministic; no
    /// timer is slept on and no screenshot is taken.
    fn clock_runtime(samples: impl IntoIterator<Item = clock::Sample>) -> Runtime<State> {
        let mut state = State {
            clock_samples: samples.into_iter().collect(),
            ..State::default()
        };
        setup(&mut state, Arc::new(|| {}));
        let assets = state
            .numerals
            .iter()
            .chain(state.glass.iter())
            .chain(state.clock_glass.iter())
            .chain(state.power_art.iter());
        for image in assets {
            image.wait().expect("decode approved artwork");
        }
        let mut runtime = Runtime::new(state, bar, (900.0, 40.0), Env::default());
        runtime.poll = Some(poll);
        runtime
    }

    #[test]
    fn clock_shows_initial_minute_then_updates_once_at_the_boundary() {
        // Local instants: 09:05:30.250, 09:06:00.000, 09:06:30.000 and
        // 09:07:00.000. The third repeats the displayed minute.
        let mut runtime = clock_runtime([
            fixed_sample(30, 250_000_000, 9, 5),
            fixed_sample(0, 0, 9, 6),
            fixed_sample(30, 0, 9, 6),
            fixed_sample(0, 0, 9, 7),
        ]);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.clock.shown(),
            ClockTime::new(9, 5),
            "the first frame shows the current minute, not a mock or a tick"
        );
        assert_eq!(runtime.state.clock.due(), Some(29.75));
        assert_eq!(
            runtime.state.clock_samples.len(),
            3,
            "the first frame samples the clock once"
        );
        let initial_paint = format!("{:?}", runtime.instances);
        let initial_commits = runtime.stats.commits;
        let capsule = runtime.presented("clock").expect("clock capsule").0;

        runtime.frame(29.0);
        assert_eq!(
            runtime.state.clock_samples.len(),
            3,
            "frames before the deadline must not read the clock"
        );
        assert_eq!(runtime.stats.commits, initial_commits);
        assert_eq!(format!("{:?}", runtime.instances), initial_paint);

        // The coarse step crosses 09:05:30.250 + 29.750 s, the exact boundary.
        runtime.frame(1.0);
        assert_eq!(runtime.state.clock.shown(), ClockTime::new(9, 6));
        assert_eq!(runtime.state.clock.due(), Some(89.75));
        assert!(
            runtime.stats.commits > initial_commits,
            "new digits re-run the view"
        );
        assert_ne!(
            format!("{:?}", runtime.instances),
            initial_paint,
            "the updated digits reach the painted view"
        );
        assert_eq!(
            runtime.presented("clock").expect("clock capsule").0,
            capsule,
            "the 112×38 capsule keeps its geometry"
        );

        let updated_paint = format!("{:?}", runtime.instances);
        let updated_commits = runtime.stats.commits;
        // A fresh sample that repeats 09:06 still schedules, but commits nothing.
        runtime.frame(60.0);
        assert_eq!(runtime.state.clock.shown(), ClockTime::new(9, 6));
        assert_eq!(runtime.state.clock.due(), Some(119.75));
        assert_eq!(
            runtime.stats.commits, updated_commits,
            "an unchanged HH:MM must not rebuild the view"
        );
        assert_eq!(format!("{:?}", runtime.instances), updated_paint);
        assert_eq!(
            runtime.state.clock_samples.len(),
            1,
            "the unchanged sample consumed only its own observation"
        );

        // The unchanged poll still armed the next boundary: 09:07 at 119.75.
        runtime.frame(60.0);
        assert_eq!(runtime.state.clock.shown(), ClockTime::new(9, 7));
        assert_eq!(runtime.state.clock.due(), Some(179.75));
        assert!(runtime.stats.commits > updated_commits);
        assert!(runtime.state.clock_samples.is_empty());
    }

    #[test]
    fn clock_follows_discontinuous_resume_and_time_zone_samples() {
        let mut runtime = clock_runtime([
            fixed_sample(30, 250_000_000, 9, 5),
            fixed_sample(0, 0, 10, 0), // forward: 09:05 -> 10:00
            fixed_sample(0, 0, 9, 58), // backward: 10:00 -> 09:58
            fixed_sample(0, 0, 0, 0),  // resume-like: next local midnight
            fixed_sample(0, 0, 1, 1),  // zone-like: +01:00 offset, not replayed
        ]);
        runtime.frame(0.0);
        assert_eq!(runtime.state.clock.shown(), ClockTime::new(9, 5));
        for expected in [
            ClockTime::new(10, 0),
            ClockTime::new(9, 58),
            ClockTime::new(0, 0),
            ClockTime::new(1, 1),
        ] {
            let before = runtime.stats.commits;
            runtime.frame(60.0);
            assert_eq!(
                runtime.state.clock.shown(),
                expected,
                "each absolute sample is shown as it is, never replayed"
            );
            assert!(runtime.stats.commits > before);
        }
        assert!(runtime.state.clock_samples.is_empty());
    }

    fn ready_state(entries: &[(u64, u8, bool)]) -> State {
        let mut state = State::default();
        state.workspaces.apply(IpcMessage::Ready {
            generation: 1,
            output: "Virtual-1".to_owned(),
            workspaces: entries
                .iter()
                .map(|(id, idx, active)| Workspace {
                    id: *id,
                    idx: *idx,
                    name: None,
                    output: Some("Virtual-1".to_owned()),
                    is_urgent: false,
                    is_active: *active,
                    is_focused: *active,
                    active_window_id: None,
                })
                .collect(),
        });
        state
    }

    #[test]
    fn clicking_a_workspace_sends_its_stable_id_from_a_fresh_snapshot() {
        let (tx, rx) = mpsc::channel();
        let mut state = ready_state(&[(7, 1, true), (8, 2, false)]);
        state.focus_tx = Some(tx);
        let mut runtime = Runtime::new(state, bar, (900.0, 40.0), Env::default());
        runtime.frame(0.0);
        let (x, y) = runtime.find("workspace-8").expect("workspace button");
        runtime.pointer_move(x, y);
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);
        let intent = rx.try_recv().expect("focus intent");
        assert_eq!(intent.workspace_id, 8);
        assert_eq!(intent.generation, 1);
    }

    #[test]
    fn hovering_a_workspace_paints_without_rebuilding_the_view() {
        let (tx, rx) = mpsc::channel();
        let mut state = ready_state(&[(7, 1, true), (8, 2, false)]);
        state.focus_tx = Some(tx);
        let mut runtime = Runtime::new(state, bar, (900.0, 40.0), Env::default());
        runtime.frame(0.0);
        let rest = format!("{:?}", runtime.instances);
        let commits = runtime.stats.commits;
        let (x, y) = runtime.find("workspace-8").expect("workspace button");

        runtime.pointer_move(x, y);
        runtime.frame(1.0 / 60.0);

        assert_eq!(
            runtime.stats.commits, commits,
            "hover is paint-only and must not re-run the view"
        );
        assert_ne!(
            format!("{:?}", runtime.instances),
            rest,
            "hover must still paint the warm highlight"
        );
        assert!(rx.try_recv().is_err(), "hover alone must not focus");

        // The whole 38×38 capsule is one hover target; only the 30×30 wash is
        // inset. Moving inside that outer ring must not drop the highlight.
        let hovered = format!("{:?}", runtime.instances);
        let (capsule, _) = runtime.presented("workspace-8").expect("workspace button");
        runtime.pointer_move(capsule[0] + 19.0, capsule[1] + 3.0);
        runtime.frame(1.0 / 60.0);
        assert_eq!(
            format!("{:?}", runtime.instances),
            hovered,
            "the capsule edge stays lit"
        );
        assert_eq!(runtime.stats.commits, commits);
    }

    #[test]
    fn only_rendered_workspace_changes_ask_for_a_frame() {
        let mut state = ready_state(&[(7, 1, true), (8, 2, false)]);
        let revision = |name: &str, urgent: bool, active_window_id: Option<u64>| Workspace {
            id: 7,
            idx: 1,
            name: Some(name.to_owned()),
            output: Some("Virtual-1".to_owned()),
            is_urgent: urgent,
            is_active: true,
            is_focused: true,
            active_window_id,
        };
        let same_render = IpcMessage::Event {
            generation: state.workspaces.generation(),
            event: niri_ipc::Event::WorkspacesChanged {
                workspaces: vec![
                    revision("renamed", true, Some(99)),
                    Workspace {
                        id: 8,
                        idx: 2,
                        name: None,
                        output: Some("Virtual-1".to_owned()),
                        is_urgent: false,
                        is_active: false,
                        is_focused: false,
                        active_window_id: None,
                    },
                ],
            },
        };
        assert!(
            !apply_ipc_messages(&mut state, [same_render]),
            "names, urgency and active windows are not drawn"
        );
        assert_eq!(
            state.workspaces.by_id(7).unwrap().name.as_deref(),
            Some("renamed")
        );
        assert!(state.workspaces.by_id(7).unwrap().is_urgent);

        let generation = state.workspaces.generation();
        assert!(apply_ipc_messages(
            &mut state,
            [IpcMessage::Event {
                generation,
                event: niri_ipc::Event::WorkspaceActivated {
                    id: 8,
                    focused: true,
                },
            }],
        ));
        assert!(state.workspaces.by_id(8).unwrap().is_active);

        assert!(
            apply_ipc_messages(&mut state, [IpcMessage::Session { generation: 2 }]),
            "losing the snapshot clears the bar"
        );
    }

    #[test]
    fn unavailable_state_shows_no_clickable_workspaces() {
        let (tx, rx) = mpsc::channel();
        let state = State {
            focus_tx: Some(tx),
            ..State::default()
        };
        let mut runtime = Runtime::new(state, bar, (900.0, 40.0), Env::default());
        runtime.frame(0.0);
        assert!(runtime.find("workspaces-unavailable").is_some());
        assert!(runtime.find("workspace-1").is_none());
        assert!(rx.try_recv().is_err());
    }

    fn fake_power_state() -> (State, mpsc::Receiver<Dispatch>, mpsc::Sender<PowerEvent>) {
        let (dispatch_tx, dispatch_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let state = State {
            power_tx: Some(dispatch_tx),
            power_rx: Some(result_rx),
            ..State::default()
        };
        (state, dispatch_rx, result_tx)
    }

    /// Production menu view and handlers with a fixed fake backend and no
    /// setup path, so no image is decoded and no real action can run. The
    /// viewport is the same state-dependent size the production overlay uses.
    fn power_runtime(state: State) -> Runtime<State> {
        power_runtime_at_scale(state, 1.0)
    }

    fn power_runtime_at_scale(state: State, scale: f32) -> Runtime<State> {
        let size = menu_size_for(state.power.state());
        let mut runtime = Runtime::new(
            state,
            menu,
            size,
            Env {
                scale,
                ..Env::default()
            },
        );
        runtime.poll = Some(power_poll);
        runtime.on_key = Some(menu_key);
        runtime.frame(0.0);
        runtime
    }

    fn click(runtime: &mut Runtime<State>, key: &str) -> (f32, f32) {
        let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("missing {key}"));
        click_at(runtime, x, y);
        (x, y)
    }

    fn click_at(runtime: &mut Runtime<State>, x: f32, y: f32) {
        runtime.pointer_move(x, y);
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);
    }

    fn rect(runtime: &Runtime<State>, key: &str) -> [f32; 4] {
        runtime
            .presented(key)
            .unwrap_or_else(|| panic!("missing presented rect for {key}"))
            .0
    }

    fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    fn no_dispatch(rx: &mpsc::Receiver<Dispatch>) {
        assert!(
            rx.try_recv().is_err(),
            "selection, dismissal or a reentrant click must not dispatch"
        );
    }

    fn xyxy(rect: [f32; 4]) -> [f32; 4] {
        [rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]]
    }

    fn contains(region: [f32; 4], x: f32, y: f32) -> bool {
        x >= region[0] && x < region[2] && y >= region[1] && y < region[3]
    }

    fn assert_region(runtime: &Runtime<State>, key: &str) {
        let expected = xyxy(rect(runtime, key));
        assert!(
            runtime.hit_regions().iter().any(|region| {
                region
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| (*actual - expected).abs() <= 1.0)
            }),
            "missing native hit region for {key}; regions: {:?}",
            runtime.hit_regions()
        );
    }

    fn escape(runtime: &mut Runtime<State>) {
        runtime.key(KeyEvent {
            key: Key::Escape,
            mods: Default::default(),
        });
        runtime.frame(1.0 / 60.0);
    }

    #[test]
    fn power_selection_shows_confirmation_without_dispatching() {
        for action in PowerAction::ALL {
            let (state, dispatch_rx, _result_tx) = fake_power_state();
            let mut runtime = power_runtime(state);
            for key in [
                "power-action-logout",
                "power-action-restart",
                "power-action-shutdown",
            ] {
                assert!(runtime.find(key).is_some(), "initial row {key}");
            }
            click(&mut runtime, action_key(action));
            assert_eq!(runtime.state.power.state(), &MenuState::Confirming(action));
            assert!(runtime.find("power-confirm").is_some());
            assert!(runtime.find("power-cancel").is_some());
            no_dispatch(&dispatch_rx);
        }
    }

    #[test]
    fn confirm_is_disjoint_from_all_action_rows_and_double_click_cannot_confirm() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for action in PowerAction::ALL {
                let (state, dispatch_rx, _result_tx) = fake_power_state();
                let mut runtime = power_runtime_at_scale(state, scale);
                let rows = PowerAction::ALL.map(|row| rect(&runtime, action_key(row)));
                let (x, y) = click(&mut runtime, action_key(action));
                assert_eq!(
                    runtime.state.power.state(),
                    &MenuState::Confirming(action),
                    "scale {scale}"
                );

                let confirm = rect(&runtime, "power-confirm");
                let cancel = rect(&runtime, "power-cancel");
                for row in rows {
                    assert!(
                        !overlaps(confirm, row),
                        "scale {scale}: Confirm at {confirm:?} overlaps an initial action row at {row:?}"
                    );
                    assert!(
                        !overlaps(cancel, row),
                        "scale {scale}: Cancel at {cancel:?} overlaps an initial action row at {row:?}"
                    );
                }

                // A quick second click at the old row location must hit neither
                // footer control.
                click_at(&mut runtime, x, y);
                assert_eq!(
                    runtime.state.power.state(),
                    &MenuState::Confirming(action),
                    "scale {scale}: same-coordinate double click must not confirm {action:?}"
                );
                no_dispatch(&dispatch_rx);
            }
        }
    }

    #[test]
    fn regions_cover_only_menu_controls_and_leave_blank_area_passthrough() {
        let (state, dispatch_rx, result_tx) = fake_power_state();
        let mut runtime = power_runtime(state);
        assert_eq!(
            runtime.hit_regions().len(),
            3,
            "three list rows must be the only hit regions"
        );
        for action in PowerAction::ALL {
            assert_region(&runtime, action_key(action));
        }
        assert!(
            !runtime
                .hit_regions()
                .iter()
                .any(|region| contains(*region, 2.0, 2.0)),
            "menu background must not intercept the pointer"
        );
        // Every point outside the keyed controls—including the blank background,
        // the card padding, and this approximation of the bar power toggle area—
        // must remain pass-through.
        assert!(
            !runtime
                .hit_regions()
                .iter()
                .any(|region| contains(*region, 210.0, 20.0)),
            "menu must not intercept the bar power toggle area"
        );

        let (x, y) = click(&mut runtime, action_key(PowerAction::Restart));
        assert_eq!(
            runtime.hit_regions().len(),
            2,
            "only Cancel and Confirm may intercept in confirmation"
        );
        assert_region(&runtime, "power-cancel");
        assert_region(&runtime, "power-confirm");
        assert!(
            !runtime
                .hit_regions()
                .iter()
                .any(|region| contains(*region, x, y)),
            "the old action row must not remain a native hit region"
        );
        assert!(
            !runtime
                .hit_regions()
                .iter()
                .any(|region| contains(*region, 2.0, 2.0))
        );

        click(&mut runtime, "power-confirm");
        let dispatch = dispatch_rx.try_recv().expect("one dispatch");
        assert_eq!(runtime.hit_regions().len(), 1);
        assert_region(&runtime, "power-hide");

        result_tx
            .send(PowerEvent {
                attempt: dispatch.attempt,
                outcome: PowerOutcome::Accepted,
            })
            .unwrap();
        runtime.frame(0.0);
        assert_eq!(runtime.hit_regions().len(), 1);
        assert_region(&runtime, "power-close");
    }

    #[test]
    fn uncertain_result_is_visible_and_not_shown_as_accepted() {
        let (state, dispatch_rx, result_tx) = fake_power_state();
        let mut runtime = power_runtime(state);
        click(&mut runtime, action_key(PowerAction::LogOut));
        click(&mut runtime, "power-confirm");
        let dispatch = dispatch_rx.try_recv().expect("one dispatch");
        result_tx
            .send(PowerEvent {
                attempt: dispatch.attempt,
                outcome: PowerOutcome::Uncertain(PowerError::Timeout),
            })
            .unwrap();
        runtime.frame(0.0);
        assert!(matches!(
            runtime.state.power.state(),
            MenuState::Result {
                action: PowerAction::LogOut,
                outcome: PowerOutcome::Uncertain(PowerError::Timeout)
            }
        ));
        assert!(runtime.find("power-error").is_some());
        assert!(runtime.find("power-accepted").is_none());
        no_dispatch(&dispatch_rx);
    }

    #[test]
    fn explicit_confirm_dispatches_once_and_pending_survives_close() {
        let (state, dispatch_rx, result_tx) = fake_power_state();
        let mut runtime = power_runtime(state);
        click(&mut runtime, action_key(PowerAction::Restart));
        let confirm = click(&mut runtime, "power-confirm");

        let dispatch = dispatch_rx.try_recv().expect("one authorized dispatch");
        assert_eq!(dispatch.action, PowerAction::Restart);
        assert_eq!(
            dispatch.effect,
            snowbar::power::Effect::Logind(snowbar::power::LogindMethod::Reboot)
        );
        assert_eq!(
            runtime.state.power.state(),
            &MenuState::Pending(PowerAction::Restart)
        );

        // Reentrant click at the old confirm location while pending.
        click_at(&mut runtime, confirm.0, confirm.1);
        no_dispatch(&dispatch_rx);

        // Escape and reopen may hide the menu, but cannot cancel or rearm it.
        escape(&mut runtime);
        assert_eq!(
            runtime.state.power.state(),
            &MenuState::Pending(PowerAction::Restart)
        );
        no_dispatch(&dispatch_rx);
        runtime.state.power.on_open();
        runtime.frame(1.0 / 60.0);
        assert_eq!(
            runtime.state.power.state(),
            &MenuState::Pending(PowerAction::Restart)
        );
        no_dispatch(&dispatch_rx);

        result_tx
            .send(PowerEvent {
                attempt: dispatch.attempt,
                outcome: PowerOutcome::Accepted,
            })
            .unwrap();
        runtime.frame(0.0);
        assert!(matches!(
            runtime.state.power.state(),
            MenuState::Result {
                action: PowerAction::Restart,
                outcome: PowerOutcome::Accepted
            }
        ));
        assert!(runtime.find("power-accepted").is_some());

        // A duplicate completion and another reopen do not dispatch again.
        result_tx
            .send(PowerEvent {
                attempt: dispatch.attempt,
                outcome: PowerOutcome::Accepted,
            })
            .unwrap();
        runtime.frame(0.0);
        assert!(matches!(
            runtime.state.power.state(),
            MenuState::Result {
                action: PowerAction::Restart,
                outcome: PowerOutcome::Accepted
            }
        ));
        no_dispatch(&dispatch_rx);

        // The stable accepted result has a Close control, not another action.
        assert!(runtime.find("power-close").is_some());
        click(&mut runtime, "power-close");
        assert!(matches!(
            runtime.state.power.state(),
            MenuState::Result {
                action: PowerAction::Restart,
                outcome: PowerOutcome::Accepted
            }
        ));
        runtime.state.power.on_open();
        runtime.frame(1.0 / 60.0);
        assert!(runtime.find("power-accepted").is_some());
        no_dispatch(&dispatch_rx);
    }

    #[test]
    fn failure_is_visible_persists_and_retry_needs_a_fresh_confirmation() {
        let (state, dispatch_rx, result_tx) = fake_power_state();
        let mut runtime = power_runtime(state);
        click(&mut runtime, action_key(PowerAction::LogOut));
        click(&mut runtime, "power-confirm");
        let first = dispatch_rx.try_recv().expect("first dispatch");

        result_tx
            .send(PowerEvent {
                attempt: first.attempt,
                outcome: PowerOutcome::Failed(PowerError::PermissionDenied),
            })
            .unwrap();
        // The completion arrives while the menu is hidden; the next open
        // drains it and shows the operation-specific failure.
        runtime.state.power.on_open();
        runtime.frame(0.0);
        assert!(runtime.find("power-error").is_some());
        assert!(matches!(
            runtime.state.power.state(),
            MenuState::Result {
                action: PowerAction::LogOut,
                outcome: PowerOutcome::Failed(PowerError::PermissionDenied)
            }
        ));

        // No automatic retries, and the failure survives hide/reopen.
        for _ in 0..3 {
            runtime.frame(1.0);
        }
        no_dispatch(&dispatch_rx);
        escape(&mut runtime);
        assert!(matches!(
            runtime.state.power.state(),
            MenuState::Result { .. }
        ));
        runtime.state.power.on_open();
        runtime.frame(1.0 / 60.0);
        assert!(runtime.find("power-error").is_some());
        no_dispatch(&dispatch_rx);

        // Back, select and confirm are all explicit; the retry has a new attempt.
        click(&mut runtime, "power-back");
        assert!(runtime.state.power.is_list());
        click(&mut runtime, action_key(PowerAction::LogOut));
        assert_eq!(
            runtime.state.power.state(),
            &MenuState::Confirming(PowerAction::LogOut)
        );
        click(&mut runtime, "power-confirm");
        let second = dispatch_rx.try_recv().expect("explicit retry dispatch");
        assert!(second.attempt > first.attempt);

        // A stale completion from the first attempt cannot finish the retry.
        result_tx
            .send(PowerEvent {
                attempt: first.attempt,
                outcome: PowerOutcome::Accepted,
            })
            .unwrap();
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.power.state(),
            &MenuState::Pending(PowerAction::LogOut)
        );

        result_tx
            .send(PowerEvent {
                attempt: second.attempt,
                outcome: PowerOutcome::Accepted,
            })
            .unwrap();
        runtime.frame(0.0);
        assert!(runtime.find("power-accepted").is_some());
    }

    #[test]
    fn cancel_escape_and_toggle_clear_only_unconfirmed_actions() {
        for action in PowerAction::ALL {
            let (state, dispatch_rx, _result_tx) = fake_power_state();
            let mut runtime = power_runtime(state);

            click(&mut runtime, action_key(action));
            click(&mut runtime, "power-cancel");
            assert!(runtime.state.power.is_list());
            no_dispatch(&dispatch_rx);

            click(&mut runtime, action_key(action));
            escape(&mut runtime);
            assert!(runtime.state.power.is_list());
            no_dispatch(&dispatch_rx);

            // The bar's toggle state path is exercised without a native
            // overlay; show/hide and the global MENU_OPEN flag remain manual.
            click(&mut runtime, action_key(action));
            runtime.update(|state, _| {
                set_power_menu_open(state, false);
            });
            runtime.frame(1.0 / 60.0);
            assert!(runtime.state.power.is_list());
            no_dispatch(&dispatch_rx);

            click(&mut runtime, action_key(action));
            runtime.update(|state, _| {
                set_power_menu_open(state, true);
            });
            runtime.frame(1.0 / 60.0);
            assert!(runtime.state.power.is_list());
            no_dispatch(&dispatch_rx);
        }
    }

    #[test]
    fn enter_does_not_select_or_confirm() {
        let (state, dispatch_rx, _result_tx) = fake_power_state();
        let mut runtime = power_runtime(state);
        let enter = KeyEvent {
            key: Key::Enter,
            mods: Default::default(),
        };
        runtime.key(enter);
        runtime.frame(1.0 / 60.0);
        assert!(runtime.state.power.is_list());
        no_dispatch(&dispatch_rx);

        click(&mut runtime, action_key(PowerAction::ShutDown));
        runtime.key(KeyEvent {
            key: Key::Enter,
            mods: Default::default(),
        });
        runtime.frame(1.0 / 60.0);
        assert_eq!(
            runtime.state.power.state(),
            &MenuState::Confirming(PowerAction::ShutDown)
        );
        no_dispatch(&dispatch_rx);
    }

    #[test]
    fn missing_or_disconnected_worker_is_a_visible_failure() {
        let mut state = State {
            power_start_error: Some(PowerError::MissingBackend("niri socket")),
            ..State::default()
        };
        let mut runtime = power_runtime(std::mem::take(&mut state));
        click(&mut runtime, action_key(PowerAction::Restart));
        click(&mut runtime, "power-confirm");
        assert!(matches!(
            runtime.state.power.state(),
            MenuState::Result {
                action: PowerAction::Restart,
                outcome: PowerOutcome::Failed(PowerError::MissingBackend("niri socket"))
            }
        ));
        assert!(runtime.find("power-error").is_some());

        let (dispatch_tx, dispatch_rx) = mpsc::channel();
        drop(dispatch_rx);
        let mut state = State {
            power_tx: Some(dispatch_tx),
            ..State::default()
        };
        let mut runtime = power_runtime(std::mem::take(&mut state));
        click(&mut runtime, action_key(PowerAction::LogOut));
        click(&mut runtime, "power-confirm");
        assert!(matches!(
            runtime.state.power.state(),
            MenuState::Result {
                action: PowerAction::LogOut,
                outcome: PowerOutcome::Failed(PowerError::WorkerDisconnected)
            }
        ));
        assert!(runtime.find("power-error").is_some());
    }

    #[test]
    fn power_copy_wraps_inside_the_popover_for_confirm_pending_and_results() {
        let long = PowerError::rejected("W".repeat(240));
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for action in PowerAction::ALL {
                let (state, _dispatch_rx, _result_tx) = fake_power_state();
                let mut runtime = power_runtime_at_scale(state, scale);
                click(&mut runtime, action_key(action));

                assert_menu_text_fits(&runtime, "power-prompt", action.prompt());
                assert_menu_text_fits(&runtime, "power-detail", action.detail());
                assert_text_above_footer(&runtime, "power-prompt", "power-confirm");
                assert_text_above_footer(&runtime, "power-detail", "power-confirm");
                assert_footer_inside_menu(&runtime, "power-cancel");
                assert_footer_inside_menu(&runtime, "power-confirm");
            }

            for action in PowerAction::ALL {
                let (state, dispatch_rx, _result_tx) = fake_power_state();
                let mut runtime = power_runtime_at_scale(state, scale);
                click(&mut runtime, action_key(action));
                click(&mut runtime, "power-confirm");
                assert!(dispatch_rx.try_recv().is_ok(), "confirm dispatches once");

                assert_menu_text_fits(&runtime, "power-pending", action.pending_line());
                assert_menu_text_fits(
                    &runtime,
                    "power-pending-note",
                    "Closing this menu does not cancel the request.",
                );
                assert_text_above_footer(&runtime, "power-pending", "power-hide");
                assert_text_above_footer(&runtime, "power-pending-note", "power-hide");
                assert_footer_inside_menu(&runtime, "power-hide");
            }

            for action in PowerAction::ALL {
                for outcome in [
                    PowerOutcome::Accepted,
                    PowerOutcome::Failed(long.clone()),
                    PowerOutcome::Uncertain(long.clone()),
                ] {
                    let (mut state, _dispatch_rx, _result_tx) = fake_power_state();
                    assert!(state.power.select(action));
                    let dispatch = state.power.confirm().expect("synthetic dispatch");
                    assert!(state.power.apply_result(PowerEvent {
                        attempt: dispatch.attempt,
                        outcome: outcome.clone(),
                    }));
                    let size = menu_size_for(state.power.state());
                    let runtime = power_runtime_at_scale(state, scale);
                    assert_eq!(size, menu_size_for(runtime.state.power.state()));

                    let summary = outcome.summary(action);
                    let result_key = if outcome.is_accepted() {
                        "power-accepted"
                    } else {
                        "power-error"
                    };
                    assert_menu_text_fits(&runtime, result_key, &summary);
                    let footer = if outcome.is_accepted() {
                        "power-close"
                    } else {
                        "power-back"
                    };
                    assert_text_above_footer(&runtime, result_key, footer);
                    assert_footer_inside_menu(&runtime, footer);
                }
            }
        }
    }

    #[test]
    fn long_error_copy_grows_the_popover_instead_of_clipping() {
        let long = PowerError::rejected("W".repeat(240));
        let result = MenuState::Result {
            action: PowerAction::ShutDown,
            outcome: PowerOutcome::Uncertain(long),
        };
        let size = menu_size_for(&result);
        assert!(
            size.1 > MENU_MIN_HEIGHT,
            "a worst-case result must grow the surface: {size:?}"
        );
        assert_eq!(size.0, MENU_WIDTH);
    }

    fn assert_menu_text_fits(runtime: &Runtime<State>, key: &str, expected: &str) {
        let elements = runtime.elements();
        let element = elements
            .iter()
            .find(|element| element.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing rendered text {key}"));
        assert_eq!(
            element.text.as_deref(),
            Some(expected),
            "{key} must draw the whole sentence"
        );
        let [x, y, w, h] = element.rect;
        assert!(w > 0.0 && h > 0.0, "{key} has no text box: {element:?}");
        for (actual, wanted) in element.visible.iter().zip([x, y, w, h]) {
            assert!(
                (*actual - wanted).abs() <= 0.5,
                "{key} is clipped: rect {:?}, visible {:?}",
                element.rect,
                element.visible
            );
        }
        assert!(
            x >= 0.0 && x + w <= MENU_WIDTH - MENU_SIDE_INSET + 0.5,
            "{key} leaves the popover horizontally: {element:?}"
        );
        let (line_width, line_height) = rune::text::measure_with(
            expected,
            &[],
            SECONDARY,
            Some(w),
            rune::text::RichLayoutOptions::new().wrap_anywhere(),
        );
        assert!(
            line_width <= w + 0.75,
            "{key} has a line wider than its box ({line_width} > {w})"
        );
        assert!(
            line_height <= h + 0.75,
            "{key} needs more lines than its box ({line_height} > {h})"
        );
    }

    fn assert_text_above_footer(runtime: &Runtime<State>, text: &str, footer: &str) {
        let text = rect(runtime, text);
        let footer = rect(runtime, footer);
        assert!(
            text[1] + text[3] <= footer[1] + 0.5,
            "{text:?} reaches into footer {footer:?}"
        );
    }

    fn assert_footer_inside_menu(runtime: &Runtime<State>, key: &str) {
        let [x, y, w, h] = rect(runtime, key);
        assert!(
            x >= 0.0 && y >= 0.0 && x + w <= MENU_WIDTH + 0.5,
            "{key} leaves the popover: {:?}",
            [x, y, w, h]
        );
        let menu_height = menu_size_for(runtime.state.power.state()).1;
        assert!(
            y + h <= menu_height + 0.5,
            "{key} leaves the popover vertically: {:?} in {menu_height}",
            [x, y, w, h]
        );
    }

    #[test]
    fn scrolling_reaches_trailing_workspace_without_overlapping_clock() {
        let entries: Vec<(u64, u8, bool)> = (1..=12).map(|i| (i as u64, i as u8, i == 1)).collect();
        let (tx, rx) = mpsc::channel();
        let mut state = ready_state(&entries);
        state.focus_tx = Some(tx);
        let mut runtime = Runtime::new(state, bar, (900.0, 40.0), Env::default());
        runtime.frame(0.0);
        let (strip, _) = runtime.presented("workspaces-scroll").unwrap();
        let (clock, _) = runtime.presented("clock").unwrap();
        assert!(
            strip[0] + strip[2] <= clock[0],
            "strip must not overlap clock"
        );
        let before = runtime.find("workspace-12").unwrap().0;
        runtime.wheel_xy(strip[0] + 50.0, strip[1] + 20.0, -600.0, 0.0);
        runtime.frame(1.0 / 60.0);
        let (x, y) = runtime.find("workspace-12").expect("trailing workspace");
        assert!(x < before && x >= strip[0] && x < strip[0] + strip[2]);
        runtime.pointer_move(x, y);
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);
        assert_eq!(rx.try_recv().unwrap().workspace_id, 12);
    }
}
