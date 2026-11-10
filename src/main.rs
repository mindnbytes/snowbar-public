use rune::{
    Align, App, AppSpec, Color, Cx, El, Key, KeyEvent, KeyPolicy, OverlayInput, OverlayOptions,
    OverlayPlacement, OverlaySpec, ScreenEdge, Tx, WindowSpec, col, el, row, text,
};
use std::sync::{Mutex, OnceLock};

const INK: Color = Color {
    r: 0.20,
    g: 0.23,
    b: 0.25,
    a: 1.0,
};
const MUTED: Color = Color {
    r: 0.48,
    g: 0.50,
    b: 0.49,
    a: 1.0,
};
const PAPER: Color = Color {
    r: 0.94,
    g: 0.93,
    b: 0.90,
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

struct State {
    workspace: usize,
    confirmation: Option<&'static str>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            workspace: 1,
            confirmation: None,
        }
    }
}

static MENU: OnceLock<Mutex<Option<rune::OverlayHandle<State>>>> = OnceLock::new();

fn menu_handle() -> Option<rune::OverlayHandle<State>> {
    MENU.get()?.lock().ok()?.clone()
}

fn bar(state: &State, _: &mut Cx<State>) -> El {
    let mut workspaces = row().gap(5.0).items_center();
    for i in 1..=4 {
        let active = state.workspace == i;
        workspaces = workspaces.child(
            el().size(30.0, 28.0)
                .center()
                .radius(8.0)
                .fill(if active {
                    MOSS
                } else {
                    Color::WHITE.alpha(0.0)
                })
                .child(text(i.to_string()).color(if active { PAPER } else { INK }))
                .on_press(move |s: &mut State| s.workspace = i),
        );
    }

    let power = row()
        .h(30.0)
        .px(12.0)
        .gap(7.0)
        .center_y()
        .radius(9.0)
        .fill(STONE.alpha(0.45))
        .child(text("Power").color(INK))
        .on_press(|_: &mut State| {
            if let Some(menu) = menu_handle() {
                menu.update(|state, _| state.confirmation = None);
                menu.show();
                menu.request_keys();
            }
        });

    row()
        .w_full()
        .h_full()
        .px(18.0)
        .items_center()
        .fill(PAPER.alpha(0.84))
        .child(row().w(220.0).child(workspaces))
        .child(
            row()
                .grow(1.0)
                .items_center()
                .justify_center()
                .child(text("09:41").color(INK)),
        )
        .child(row().w(220.0).justify_end().child(power))
}

fn menu(state: &State, _: &mut Cx<State>) -> El {
    let mut content = col()
        .w_full()
        .padding(14.0)
        .gap(8.0)
        .radius(14.0)
        .fill(PAPER.alpha(0.92))
        .border(1.0, STONE)
        .child(text("Power").color(INK));
    if let Some(action) = state.confirmation {
        content = content
            .child(text(format!("Confirm {action}? (mock only)")).color(MUTED))
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
    col().w_full().h_full().child(content)
}

fn button(label: &'static str) -> El {
    row()
        .w_full()
        .h(34.0)
        .px(10.0)
        .center_y()
        .radius(8.0)
        .fill(STONE.alpha(0.45))
        .child(text(label).color(INK))
}

fn menu_key(_: &mut State, event: &KeyEvent, _: &mut Tx) -> bool {
    if event.key == Key::Escape {
        if let Some(menu) = menu_handle() {
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
                .wayland_top_reserved(40),
            State::default(),
        );
        let menu_overlay = app.overlay(
            OverlaySpec::new("snowbar-power", (230.0, 210.0), menu)
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
