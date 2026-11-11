//! Oversized, standalone material study. This is not the bar's workspace widget.
use rune::tok::motion;
use rune::{AppSpec, Color, Cx, El, Image, ImageCache, TextStyle, col, el, text};

#[derive(Default)]
struct State {
    active: bool,
    hovered: bool,
    cache: Option<ImageCache>,
    numeral: Option<Image>,
    glass: Option<Image>,
}

const CREAM: Color = Color {
    r: 1.0,
    g: 0.94,
    b: 0.83,
    a: 1.0,
};
const GOLD: Color = Color {
    r: 0.89,
    g: 0.65,
    b: 0.36,
    a: 1.0,
};

fn view(state: &State, _: &mut Cx<State>) -> El {
    let button = el()
        .key("workspace")
        .size(280.0, 280.0)
        .center()
        .radius(68.0)
        .fill(Color::TRANSPARENT)
        .shadow(
            0.0,
            30.0,
            if state.hovered || state.active {
                GOLD.alpha(0.45)
            } else {
                Color::TRANSPARENT
            },
        )
        .hover(GOLD.alpha(0.25))
        .press(Color::BLACK.alpha(0.34))
        .press_scale(0.95)
        .motion(motion::SNAPPY)
        .child(
            el().size(280.0, 280.0)
                .center()
                .child(
                    el().size(248.0, 248.0)
                        .radius(70.0)
                        .fill(if state.active {
                            GOLD.alpha(0.72)
                        } else {
                            Color::TRANSPARENT
                        })
                        .absolute(16.0, 16.0),
                )
                .child(match &state.glass {
                    Some(image) => el()
                        .size(280.0, 280.0)
                        .pixels(image.pixels())
                        .absolute(0.0, 0.0),
                    None => el().size(280.0, 280.0).absolute(0.0, 0.0),
                })
                .child(
                    el().size(248.0, 248.0)
                        .radius(70.0)
                        .fill(if state.hovered {
                            GOLD.alpha(0.18)
                        } else {
                            Color::TRANSPARENT
                        })
                        .absolute(16.0, 16.0),
                )
                .child(match &state.numeral {
                    Some(image) => el().size(180.0, 198.0).pixels(image.pixels()),
                    None => el().size(180.0, 198.0),
                }),
        )
        .on_hover_change(|state: &mut State, hovered, _| state.hovered = hovered)
        .on_press(|state: &mut State| state.active = !state.active);

    col()
        .w_full()
        .h_full()
        .center()
        .gap(28.0)
        // A visible gradient behind the capsule makes translucency testable.
        .linear_gradient(
            1.5708,
            [
                (
                    0.0,
                    Color {
                        r: 0.13,
                        g: 0.17,
                        b: 0.15,
                        a: 1.0,
                    },
                ),
                (
                    0.5,
                    Color {
                        r: 0.50,
                        g: 0.35,
                        b: 0.22,
                        a: 1.0,
                    },
                ),
                (
                    1.0,
                    Color {
                        r: 0.17,
                        g: 0.21,
                        b: 0.19,
                        a: 1.0,
                    },
                ),
            ],
        )
        .child(button)
        .child(
            text(if state.active { "Selected" } else { "Inactive" })
                .font(TextStyle::new(22.0, 500.0))
                .color(CREAM),
        )
        .child(
            text("Hover · hold to press · click to select")
                .font(TextStyle::new(16.0, 400.0))
                .color(CREAM.alpha(0.65)),
        )
}

fn main() {
    rune::run(AppSpec::<State> {
        scenes: &[("active", |state| state.active = true)],
        setup: Some(|state, waker| {
            let cache = state.cache.insert(ImageCache::new(waker));
            state.numeral =
                Some(cache.svg(&include_bytes!("assets/workspace-one.svg")[..], 220.0, 2.0));
            state.glass = Some(cache.svg(
                &include_bytes!("assets/workspace-glass.svg")[..],
                320.0,
                2.0,
            ));
        }),
        ..AppSpec::new(
            "Workspace button study",
            (600.0, 540.0),
            State::default,
            view,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn clicking_toggles_selection() {
        let mut runtime = Runtime::new(State::default(), view, (600.0, 540.0), Env::default());
        runtime.frame(0.0);
        let (x, y) = runtime.find("workspace").expect("workspace button");
        runtime.pointer_move(x, y);
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);
        assert!(runtime.state.active);
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);
        assert!(!runtime.state.active);
    }
}
