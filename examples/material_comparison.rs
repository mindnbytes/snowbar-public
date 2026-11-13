//! Standalone comparison, not a replacement for the approved workspace widget.
//! Run with `cargo run --example material_comparison` or capture a scene with
//! `cargo run --example material_comparison -- --snap comparison.png --scene cool`.
use rune::tok::motion;
use rune::{
    AppSpec, Color, Cx, El, Image, ImageCache, LineCap, Path, Shape, TextStyle, canvas, col, el,
    row, text,
};

#[derive(Default)]
struct State {
    active: bool,
    hovered: Option<usize>,
    cool: bool,
    cache: Option<ImageCache>,
    glass: Option<Image>,
    numeral: Option<Image>,
}

fn color(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color { r, g, b, a }
}

fn native_one(k: f32) -> El {
    // The outline follows the study's one; native shapes cannot fill holes, so this is
    // a one-glyph experiment rather than a claim that the whole family is supported.
    let path = Path::new()
        .move_to([40.0, 70.0])
        .line_to([79.0, 35.0])
        .quad_to([83.0, 32.0], [91.0, 32.0])
        .line_to([110.0, 32.0])
        .quad_to([117.0, 32.0], [117.0, 40.0])
        .line_to([117.0, 151.0])
        .line_to([141.0, 151.0])
        .quad_to([150.0, 151.0], [150.0, 160.0])
        .line_to([150.0, 175.0])
        .quad_to([150.0, 184.0], [141.0, 184.0])
        .line_to([51.0, 184.0])
        .quad_to([42.0, 184.0], [42.0, 175.0])
        .line_to([42.0, 160.0])
        .quad_to([42.0, 151.0], [51.0, 151.0])
        .line_to([75.0, 151.0])
        .line_to([75.0, 81.0])
        .line_to([60.0, 93.0])
        .quad_to([54.0, 97.0], [49.0, 91.0])
        .line_to([37.0, 77.0])
        .quad_to([34.0, 73.0], [40.0, 70.0])
        .close();
    let shape = |c| Shape::new().fill_path(&path, c).view_box(200.0, 220.0);
    let highlight = Path::new()
        .move_to([43.0, 69.0])
        .line_to([79.0, 38.0])
        .quad_to([83.0, 35.0], [91.0, 35.0])
        .line_to([108.0, 35.0]);
    let layer = |c, y| {
        canvas(shape(c))
            .size(180.0 * k, 198.0 * k)
            .absolute(0.0, y * k)
    };
    el().size(180.0 * k, 198.0 * k)
        .child(layer(color(0.12, 0.08, 0.04, 0.40), 11.0).blur(3.0 * k))
        .child(layer(color(0.40, 0.26, 0.15, 1.0), 8.0))
        .child(layer(color(0.72, 0.62, 0.47, 1.0), 4.0))
        .child(layer(color(0.98, 0.94, 0.85, 1.0), 0.0))
        .child(
            canvas(
                Shape::new()
                    .stroke_path(&highlight, 2.0 * k, LineCap::Round, Color::WHITE.alpha(0.9))
                    .view_box(200.0, 220.0),
            )
            .size(180.0 * k, 198.0 * k)
            .absolute(0.0, 0.0),
        )
}

fn capsule(s: &State, variant: usize, size: f32, key: &str) -> El {
    let k = size / 280.0;
    let accent = if s.cool {
        color(0.28, 0.60, 0.82, 1.0)
    } else {
        color(0.89, 0.65, 0.36, 1.0)
    };
    let hover = s.hovered == Some(variant);
    let glass = if variant == 0 {
        match &s.glass {
            Some(image) => el().size(size, size).pixels(image.pixels()),
            None => el().size(size, size),
        }
    } else {
        // Layered translucent highlights: illustrated light, not desktop blur.
        el().size(size, size)
            .center()
            .child(
                el().size(248.0 * k, 248.0 * k)
                    .radius(70.0 * k)
                    .linear_gradient(
                        3.14159,
                        [
                            (0.0, color(0.90, 0.96, 0.93, 0.34)),
                            (0.40, color(0.72, 0.82, 0.78, 0.12)),
                            (1.0, color(0.35, 0.44, 0.40, 0.20)),
                        ],
                    )
                    .border_gradient(4.0 * k, Color::WHITE.alpha(0.65), Color::WHITE.alpha(0.20))
                    .shadow(3.0 * k, 12.0 * k, Color::BLACK.alpha(0.24)),
            )
            .child(
                el().size(238.0 * k, 238.0 * k)
                    .radius(67.0 * k)
                    .border_gradient(2.0 * k, Color::WHITE.alpha(0.34), Color::WHITE.alpha(0.10))
                    .absolute(21.0 * k, 21.0 * k),
            )
            .child(
                el().size(230.0 * k, 230.0 * k)
                    .radius(64.0 * k)
                    .radial_gradient(
                        [0.18, 0.05],
                        170.0 * k,
                        [
                            (0.0, Color::WHITE.alpha(0.65)),
                            (0.45, Color::WHITE.alpha(0.10)),
                            (1.0, Color::WHITE.alpha(0.0)),
                        ],
                    )
                    .absolute(25.0 * k, 25.0 * k),
            )
            .child(
                el().size(230.0 * k, 230.0 * k)
                    .radius(64.0 * k)
                    .radial_gradient(
                        [0.80, 1.0],
                        140.0 * k,
                        [(0.0, Color::WHITE.alpha(0.28)), (1.0, Color::TRANSPARENT)],
                    )
                    .absolute(25.0 * k, 25.0 * k),
            )
            .child(
                canvas(
                    Shape::new()
                        .stroke_path(
                            &Path::new()
                                .move_to([38.0, 94.0])
                                .quad_to([39.0, 37.0], [92.0, 31.0])
                                .line_to([165.0, 31.0]),
                            2.0 * k,
                            LineCap::Round,
                            Color::WHITE.alpha(0.75),
                        )
                        .view_box(280.0, 280.0),
                )
                .size(size, size)
                .absolute(0.0, 0.0),
            )
    };
    let numeral = if variant < 2 {
        match &s.numeral {
            Some(image) => el().size(180.0 * k, 198.0 * k).pixels(image.pixels()),
            None => el().size(180.0 * k, 198.0 * k),
        }
    } else {
        native_one(k)
    };
    el().key(key.to_owned())
        .role("button")
        .label(format!("Comparison {}", variant + 1))
        .size(size, size)
        .center()
        .radius(68.0 * k)
        .fill(Color::TRANSPARENT)
        .shadow(
            0.0,
            30.0 * k,
            if hover || s.active {
                accent.alpha(0.45)
            } else {
                Color::TRANSPARENT
            },
        )
        .hover(accent.alpha(0.25))
        .press(Color::BLACK.alpha(0.34))
        .press_scale(0.95)
        .motion(motion::SNAPPY)
        .child(
            el().size(size, size)
                .center()
                .child(
                    el().size(248.0 * k, 248.0 * k)
                        .radius(70.0 * k)
                        .fill(if s.active {
                            accent.alpha(0.72)
                        } else {
                            Color::TRANSPARENT
                        })
                        .absolute(16.0 * k, 16.0 * k),
                )
                .child(glass.absolute(0.0, 0.0))
                .child(
                    el().size(248.0 * k, 248.0 * k)
                        .radius(70.0 * k)
                        .fill(if hover {
                            accent.alpha(0.18)
                        } else {
                            Color::TRANSPARENT
                        })
                        .absolute(16.0 * k, 16.0 * k),
                )
                .child(numeral),
        )
        .on_hover_change(move |s: &mut State, inside, _| {
            if inside {
                s.hovered = Some(variant);
            } else if s.hovered == Some(variant) {
                s.hovered = None;
            }
        })
        .on_press(|s: &mut State| s.active = !s.active)
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let ink = color(1.0, 0.94, 0.83, 1.0);
    let labels = [
        "Approved SVG",
        "Rune glass + SVG digit",
        "Rune glass + geometry digit",
    ];
    let mut choices = row().gap(24.0).items_center();
    for (i, label) in labels.iter().enumerate() {
        choices = choices.child(
            col()
                .w(280.0)
                .items_center()
                .gap(12.0)
                .child(text(*label).font(TextStyle::new(16.0, 600.0)).color(ink))
                .child(capsule(s, i, 280.0, &format!("large-{i}")))
                .child(
                    text("Bar scale")
                        .font(TextStyle::new(13.0, 400.0))
                        .color(ink.alpha(0.75)),
                )
                .child(capsule(s, i, 38.0, &format!("small-{i}"))),
        );
    }
    col()
        .w_full()
        .h_full()
        .center()
        .gap(24.0)
        .linear_gradient(
            1.5708,
            [
                (0.0, color(0.13, 0.17, 0.15, 1.0)),
                (0.5, color(0.50, 0.35, 0.22, 1.0)),
                (1.0, color(0.17, 0.21, 0.19, 1.0)),
            ],
        )
        .child(choices)
        .child(
            text(if s.active {
                "Selected — click any capsule to deselect"
            } else {
                "Inactive — hover, hold, or click any capsule"
            })
            .font(TextStyle::new(16.0, 500.0))
            .color(ink),
        )
        .child(
            text(if s.cool {
                "Cool accent: SVG materials remain fixed; only the interaction tint changes"
            } else {
                "Approved warm accent — use --scene cool to compare a blue accent"
            })
            .font(TextStyle::new(14.0, 400.0))
            .color(ink.alpha(0.75)),
        )
}

fn main() {
    rune::run(AppSpec::<State> {
        scenes: &[
            ("active", |s| s.active = true),
            ("cool", |s| {
                s.cool = true;
                s.active = true;
            }),
        ],
        setup: Some(|s, waker| {
            let cache = s.cache.insert(ImageCache::new(waker));
            let glass = cache.svg(
                &include_bytes!("assets/workspace-glass.svg")[..],
                320.0,
                2.0,
            );
            let numeral = cache.svg(&include_bytes!("assets/workspace-one.svg")[..], 220.0, 2.0);
            if rune::is_headless() {
                let _ = glass.wait();
                let _ = numeral.wait();
            }
            s.glass = Some(glass);
            s.numeral = Some(numeral);
        }),
        ..AppSpec::new("Material comparison", (1000.0, 560.0), State::default, view)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn each_variant_toggles_the_shared_selection() {
        let mut rt = Runtime::new(State::default(), view, (1000.0, 560.0), Env::default());
        rt.frame(0.0);
        for i in 0..3 {
            let (x, y) = rt.find(&format!("large-{i}")).expect("comparison capsule");
            rt.pointer_move(x, y);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(1.0 / 60.0);
            assert_eq!(rt.state.active, i % 2 == 0);
        }
    }
}
