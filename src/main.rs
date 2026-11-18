use rune::tok::motion;
use rune::{
    Align, Anim, App, AppSpec, Color, Cx, DisplayTarget, El, Image, ImageCache, Key, KeyEvent,
    KeyPolicy, NativeDisplayId, OverlayInput, OverlayOptions, OverlayPlacement, OverlaySpec,
    ScreenEdge, TextStyle, Tx, WindowSpec, col, el, row, text,
};
use snowbar::niri::NiriClient;
use snowbar::workspaces::{DisplaySnapshot, WorkspaceItem, WorkspaceState};
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

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

#[derive(Default)]
struct State {
    confirmation: Option<&'static str>,
    hovered_clock: bool,
    hovered_power: bool,
    cache: Option<ImageCache>,
    glass: Option<Image>,
    clock_glass: Option<Image>,
    clock_zero: Option<Image>,
    clock_nine: Option<Image>,
    power_art: Option<Image>,
    /// Artwork for indices 0..=9; higher indices use a readable text fallback.
    numerals: Vec<Image>,
    workspaces: WorkspaceState,
    focus_tx: Option<mpsc::Sender<snowbar::workspaces::FocusIntent>>,
    /// Owns the IPC worker threads and the ordered message channel.
    ipc: Option<NiriClient>,
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
        .on_press(|_: &mut State| {
            if let Some(menu) = menu_handle() {
                if MENU_OPEN.fetch_xor(true, Ordering::Relaxed) {
                    menu.hide();
                } else {
                    menu.update(|state, _| state.confirmation = None);
                    menu.show();
                    menu.request_keys();
                }
            }
        });

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
                            .child(clock_digit(state.clock_zero.as_ref()).absolute(0.0, 0.0))
                            .child(clock_digit(state.clock_nine.as_ref()).absolute(17.0, 0.0))
                            .child(
                                col()
                                    .w(4.0)
                                    .gap(4.0)
                                    .items_center()
                                    .child(el().size(3.0, 3.0).radius(1.5).fill(PAPER))
                                    .child(el().size(3.0, 3.0).radius(1.5).fill(PAPER))
                                    .absolute(40.0, 10.0),
                            )
                            .child(clock_digit(state.numerals.get(4)).absolute(43.0, 0.0))
                            .child(clock_digit(state.numerals.get(1)).absolute(60.0, 0.0)),
                    )
                    .on_hover_change(|s: &mut State, hovered, _| s.hovered_clock = hovered),
            ),
        )
        .child(row().w(WORKSPACE_AREA_WIDTH).justify_end().child(power))
}

fn menu(state: &State, _: &mut Cx<State>) -> El {
    let mut content = col()
        .w_full()
        .padding(14.0)
        .gap(8.0)
        .radius(14.0)
        .fill(PAPER.alpha(0.97))
        .border(1.5, MOSS.alpha(0.55))
        .shadow(3.0, 12.0, INK.alpha(0.25))
        .child(text("Power").font(LABEL).color(INK));
    if let Some(action) = state.confirmation {
        content = content
            .child(
                text(format!("Confirm {action}? (mock only)"))
                    .font(SECONDARY)
                    .color(MUTED),
            )
            .child(
                col()
                    .gap(8.0)
                    .child(button("Confirm (mock)").on_press(|s: &mut State| s.confirmation = None))
                    .child(button("Cancel").on_press(|s: &mut State| s.confirmation = None)),
            );
    } else {
        for action in ["Log out", "Restart", "Shut down"] {
            content = content
                .child(button(action).on_press(move |s: &mut State| s.confirmation = Some(action)));
        }
    }
    col().w_full().h_full().px(8.0).pb(12.0).child(content)
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
        .child(text(label).font(SECONDARY).color(INK))
}

fn menu_key(_: &mut State, event: &KeyEvent, _: &mut Tx) -> bool {
    if event.key == Key::Escape {
        if let Some(menu) = menu_handle() {
            MENU_OPEN.store(false, Ordering::Relaxed);
            menu.hide();
        }
        true
    } else {
        false
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
    state.clock_zero = Some(cache.svg(
        &include_bytes!("../examples/assets/workspace-zero.svg")[..],
        220.0,
        2.0,
    ));
    state.clock_nine = Some(cache.svg(
        &include_bytes!("../examples/assets/workspace-nine.svg")[..],
        220.0,
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

/// Apply IPC messages on the UI thread. Rune calls `poll` at the start of a
/// frame; the worker wakes the overlay when a message arrives, so no busy loop.
fn poll(state: &mut State, _: f64, tx: &mut Tx) -> bool {
    let Some(ipc) = state.ipc.as_ref() else {
        return false;
    };
    let messages = ipc.drain();
    if messages.is_empty() {
        return false;
    }
    let changed = apply_ipc_messages(state, messages);
    if changed {
        tx.instant();
    }
    changed
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
        let menu_overlay = app.overlay(
            OverlaySpec::new("snowbar-power", (246.0, 222.0), menu)
                .options(OverlayOptions {
                    input: OverlayInput::All,
                    keys: KeyPolicy::OnDemand,
                    placement: Some(OverlayPlacement::Edge {
                        edge: ScreenEdge::Top,
                        margin: 0.0,
                        align: Align::End,
                        inset: 8.0,
                    }),
                    ..Default::default()
                })
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
        let mut state = State::default();
        state.focus_tx = Some(tx);
        let mut runtime = Runtime::new(state, bar, (900.0, 40.0), Env::default());
        runtime.frame(0.0);
        assert!(runtime.find("workspaces-unavailable").is_some());
        assert!(runtime.find("workspace-1").is_none());
        assert!(rx.try_recv().is_err());
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
