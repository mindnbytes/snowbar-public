use rune::tok::motion;
use rune::{
    Align, App, AppSpec, Color, Cx, El, Icon, Key, KeyEvent, KeyPolicy, OverlayInput,
    OverlayOptions, OverlayPlacement, OverlaySpec, ScreenEdge, TextStyle, Tx, WindowSpec, col, el,
    icon, row, text,
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
const POWER_ICON: Icon = Icon {
    name: "power",
    view: 24.0,
    body: r#"<path d="M12 2v9M6.3 5.7a8 8 0 1 0 11.4 0" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>"#,
};
const LABEL: TextStyle = TextStyle::new(15.5, 700.0);
const SECONDARY: TextStyle = TextStyle::new(15.5, 400.0);
const CLOCK: TextStyle = TextStyle::new(18.0, 700.0).mono().tracking(0.5);

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
static MENU_OPEN: AtomicBool = AtomicBool::new(false);

fn menu_handle() -> Option<rune::OverlayHandle<State>> {
    MENU.get()?.lock().ok()?.clone()
}

fn bar(state: &State, _: &mut Cx<State>) -> El {
    let mut workspaces = row()
        .gap(3.0)
        .px(4.0)
        .h(34.0)
        .items_center()
        .radius(17.0)
        .gradient(Color::WHITE.alpha(0.27), INK.alpha(0.24))
        .border(1.0, Color::WHITE.alpha(0.38));
    for i in 1..=4 {
        let active = state.workspace == i;
        workspaces = workspaces.child(
            el().key(format!("workspace-{i}"))
                .size(29.0, 27.0)
                .center()
                .radius(13.5)
                .fill(if active {
                    WARM.alpha(0.90)
                } else {
                    Color::WHITE.alpha(0.0)
                })
                .hover(Color::WHITE.alpha(0.34))
                .motion(motion::SNAPPY)
                .child(text(i.to_string()).font(LABEL).color(Color::WHITE))
                .on_press(move |s: &mut State| s.workspace = i),
        );
    }

    let power = row()
        .size(34.0, 34.0)
        .center()
        .radius(17.0)
        .gradient(Color::WHITE.alpha(0.27), INK.alpha(0.24))
        .border(1.0, Color::WHITE.alpha(0.38))
        .hover(Color::WHITE.alpha(0.34))
        .child(icon(&POWER_ICON, 21.0).color(Color::WHITE))
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
                el().h(34.0)
                    .px(16.0)
                    .center()
                    .radius(17.0)
                    .gradient(Color::WHITE.alpha(0.30), INK.alpha(0.27))
                    .border(1.0, Color::WHITE.alpha(0.42))
                    .hover(Color::WHITE.alpha(0.34))
                    .child(text("09:41").font(CLOCK).color(Color::WHITE)),
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
