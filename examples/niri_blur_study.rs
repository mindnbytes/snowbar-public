//! Two isolated layer surfaces for an opt-in niri blur comparison.
//! See `docs/niri-blur-study.md` before running in a graphical session.
use rune::tok::motion;
use rune::{
    App, AppSpec, Color, Cx, El, Image, ImageCache, OverlayInput, OverlayOptions, OverlayPlacement,
    OverlaySpec, WindowSpec, el,
};

#[derive(Default)]
struct State {
    active: bool,
    hovered: bool,
    cache: Option<ImageCache>,
    glass: Option<Image>,
    numeral: Option<Image>,
}

const GOLD: Color = Color {
    r: 0.89,
    g: 0.65,
    b: 0.36,
    a: 1.0,
};

fn view(s: &State, _: &mut Cx<State>) -> El {
    el().key("capsule")
        .role("button")
        .label("Workspace study")
        .size(280.0, 280.0)
        .center()
        .radius(68.0)
        .fill(Color::TRANSPARENT)
        .shadow(
            0.0,
            30.0,
            if s.hovered || s.active {
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
                        .fill(if s.active {
                            GOLD.alpha(0.72)
                        } else {
                            Color::TRANSPARENT
                        })
                        .absolute(16.0, 16.0),
                )
                .child(
                    match &s.glass {
                        Some(image) => el().size(280.0, 280.0).pixels(image.pixels()),
                        None => el().size(280.0, 280.0),
                    }
                    .absolute(0.0, 0.0),
                )
                .child(
                    el().size(248.0, 248.0)
                        .radius(70.0)
                        .fill(if s.hovered {
                            GOLD.alpha(0.18)
                        } else {
                            Color::TRANSPARENT
                        })
                        .absolute(16.0, 16.0),
                )
                .child(match &s.numeral {
                    Some(image) => el().size(180.0, 198.0).pixels(image.pixels()),
                    None => el().size(180.0, 198.0),
                }),
        )
        .on_hover_change(|s: &mut State, inside, _| s.hovered = inside)
        .on_press(|s: &mut State| s.active = !s.active)
}

fn setup(s: &mut State, waker: rune::Waker) {
    let cache = s.cache.insert(ImageCache::new(waker));
    s.glass = Some(cache.svg(
        &include_bytes!("assets/workspace-glass.svg")[..],
        320.0,
        2.0,
    ));
    s.numeral = Some(cache.svg(&include_bytes!("assets/workspace-one.svg")[..], 220.0, 2.0));
}

fn main() {
    rune::run_windows(|_| {
        let mut app = App::new(AppSpec::new(
            "Niri blur study",
            (280.0, 280.0),
            State::default,
            view,
        ))
        .windowless();
        for (name, x) in [
            ("snowbar-glass-control", 120.0),
            ("snowbar-glass-blur-study", 440.0),
        ] {
            let overlay = app.overlay(
                OverlaySpec::new(name, (280.0, 280.0), view)
                    .options(OverlayOptions {
                        input: OverlayInput::All,
                        placement: Some(OverlayPlacement::Rect {
                            x,
                            y: 100.0,
                            width: 280.0,
                            height: 280.0,
                        }),
                        ..Default::default()
                    })
                    .setup(setup),
                State::default(),
            );
            overlay.show();
        }
        vec![WindowSpec::from_app("snowbar-blur-study", app)]
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn capsule_toggles_selection() {
        let mut rt = Runtime::new(State::default(), view, (280.0, 280.0), Env::default());
        rt.frame(0.0);
        let (x, y) = rt.find("capsule").expect("capsule");
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        assert!(rt.state.active);
    }
}
