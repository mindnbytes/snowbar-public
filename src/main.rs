use rune::tok::motion;
use rune::{
    Align, App, AppSpec, Color, Cx, El, Image, ImageCache, Key, KeyEvent, KeyPolicy, OverlayInput,
    OverlayOptions, OverlayPlacement, OverlaySpec, ScreenEdge, TextStyle, Tx, WindowSpec, col, el,
    row, text,
};
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
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

struct State {
    workspace: usize,
    confirmation: Option<&'static str>,
    hovered_workspace: Option<usize>,
    hovered_clock: bool,
    hovered_power: bool,
    cache: Option<ImageCache>,
    glass: Option<Image>,
    clock_glass: Option<Image>,
    clock_zero: Option<Image>,
    clock_nine: Option<Image>,
    power_art: Option<Image>,
    numerals: Vec<Image>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            workspace: 1,
            confirmation: None,
            hovered_workspace: None,
            hovered_clock: false,
            hovered_power: false,
            cache: None,
            glass: None,
            clock_glass: None,
            clock_zero: None,
            clock_nine: None,
            power_art: None,
            numerals: Vec::new(),
        }
    }
}

static MENU: OnceLock<Mutex<Option<rune::OverlayHandle<State>>>> = OnceLock::new();
static MENU_OPEN: AtomicBool = AtomicBool::new(false);

fn menu_handle() -> Option<rune::OverlayHandle<State>> {
    MENU.get()?.lock().ok()?.clone()
}

fn clock_digit(image: Option<&Image>) -> El {
    match image {
        Some(image) => el().size(25.0, 27.5).pixels(image.pixels()),
        None => el().size(25.0, 27.5),
    }
}

fn bar(state: &State, _: &mut Cx<State>) -> El {
    let mut workspaces = row().gap(2.0).h(40.0).items_center();
    for i in 1..=4 {
        let active = state.workspace == i;
        let hovered = state.hovered_workspace == Some(i);
        let glass = match &state.glass {
            Some(image) => el()
                .size(38.0, 38.0)
                .pixels(image.pixels())
                .absolute(0.0, 0.0),
            None => el().size(38.0, 38.0).absolute(0.0, 0.0),
        };
        let numeral = match state.numerals.get(i - 1) {
            Some(image) => el().size(25.0, 27.5).pixels(image.pixels()),
            None => el().size(25.0, 27.5),
        };
        workspaces = workspaces.child(
            el().key(format!("workspace-{i}"))
                .size(38.0, 38.0)
                .center()
                .radius(10.0)
                .fill(Color::TRANSPARENT)
                .shadow(
                    0.0,
                    5.0,
                    if active || hovered {
                        WARM.alpha(0.50)
                    } else {
                        Color::TRANSPARENT
                    },
                )
                .press_scale(0.95)
                .motion(motion::SNAPPY)
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
                        .fill(if hovered {
                            WARM.alpha(0.18)
                        } else {
                            Color::TRANSPARENT
                        })
                        .absolute(4.0, 4.0),
                )
                .child(numeral)
                .on_hover_change(move |s: &mut State, inside, _| {
                    if inside {
                        s.hovered_workspace = Some(i);
                    } else if s.hovered_workspace == Some(i) {
                        s.hovered_workspace = None;
                    }
                })
                .on_press(move |s: &mut State| s.workspace = i),
        );
    }

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
        .child(row().w(220.0).child(workspaces))
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
                            .child(clock_digit(state.numerals.get(3)).absolute(43.0, 0.0))
                            .child(clock_digit(state.numerals.first()).absolute(60.0, 0.0)),
                    )
                    .on_hover_change(|s: &mut State, hovered, _| s.hovered_clock = hovered),
            ),
        )
        .child(row().w(220.0).justify_end().child(power))
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

fn main() {
    rune::run_windows(|_| {
        let mut app =
            App::new(AppSpec::new("Snowbar", (900.0, 40.0), State::default, bar)).windowless();
        let bar_overlay = app.overlay(
            OverlaySpec::new("snowbar", (900.0, 40.0), bar)
                .options(OverlayOptions {
                    input: OverlayInput::All,
                    ..Default::default()
                })
                .setup(|state, waker| {
                    let cache = state.cache.insert(ImageCache::new(waker));
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
                        &include_bytes!("../examples/assets/workspace-one.svg")[..],
                        &include_bytes!("../examples/assets/workspace-two.svg")[..],
                        &include_bytes!("../examples/assets/workspace-three.svg")[..],
                        &include_bytes!("../examples/assets/workspace-four.svg")[..],
                    ]
                    .into_iter()
                    .map(|svg| cache.svg(svg, 220.0, 2.0))
                    .collect();
                })
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
        bar_overlay.show();
        vec![WindowSpec::from_app("snowbar", app)]
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn workspace_buttons_select_the_requested_workspace() {
        let mut runtime = Runtime::new(State::default(), bar, (900.0, 40.0), Env::default());
        runtime.frame(0.0);
        for workspace in [4, 2, 1, 3] {
            let (x, y) = runtime.find(&format!("workspace-{workspace}"))
                .expect("workspace button");
            runtime.pointer_move(x, y);
            runtime.pointer_down(x, y);
            runtime.pointer_up(x, y);
            runtime.frame(1.0 / 60.0);
            assert_eq!(runtime.state.workspace, workspace);
        }
    }
}
