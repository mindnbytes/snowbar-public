# Niri background-blur study

## Result: 2026-11-13

In the niri 26.04 VM, a temporary layer rule with `blur true` and `xray false`
visibly blurred terminal text behind the right-hand surface. Initially both
surfaces appeared sharp; the blur became visible after reloading the study rule
during a diagnostic, for an unconfirmed reason. A static screenshot showed the
effect; response to moving content and performance were not verified. The user
found the blur too strong. The rule also blurred the rectangular surface outside
the capsule. Per-capsule blur is not worth pursuing for Snowbar right now;
retain the approved illustrated glass. Refraction was not tested.

The study process was stopped and the managed niri configuration reloaded after
the review. No permanent VM configuration was changed.

## Study details

This disposable example puts two copies of the approved capsule on separate,
non-exclusive layer-shell surfaces. Left: `snowbar-glass-control`. Right:
`snowbar-glass-blur-study`. Hover and click either independently; stop the
process with Ctrl-C. It does not affect the installed bar.

**Before running in the VM:** ask for approval for the live session and for any
temporary compositor configuration. The study itself does not request blur:
our pinned Rune layer-shell implementation does not expose the niri-supported
`ext-background-effect-v1` request. Without a matching rule, both sides are
ordinary transparent surfaces and no blur comparison has been made.

Niri 26.04 documents the following *optional temporary rule*, shown for review,
not applied by this project:

```kdl
layer-rule {
    match namespace="^snowbar-glass-blur-study$"
    background-effect {
        blur true
        xray false
    }
}
```

Run `cargo run --example niri_blur_study` in the
[pinned Nix shell](../README.md#developing-with-nix) only after the rule and
session procedure are approved. A rule blurs the entire rectangular
surface, including transparent margins: it is a capability/appearance probe,
not a correct capsule-shaped blur. `xray false` asks for windows behind the
surface; niri documents it as experimental and more expensive. Its default xray
mode samples the wallpaper instead. Compare both against moving windows and
wallpaper, observe the 1.75× output, and record screenshots, performance and
limitations. Do not interpret a static screenshot alone as proof of live blur.

Correctly shaped regions would require Rune's layer surface to request the
protocol (or another proven integration). Refraction is not provided by this
blur request and is not implemented or claimed here.

Source: [niri 26.04 window effects](https://github.com/niri-wm/niri/blob/v26.04/docs/wiki/Window-Effects.md)
and [layer rules](https://github.com/niri-wm/niri/blob/v26.04/docs/wiki/Configuration%3A-Layer-Rules.md).
