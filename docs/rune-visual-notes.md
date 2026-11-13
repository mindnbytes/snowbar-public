# Rune visual notes

Research and comparison: 2026-11-13. Applies to Snowbar's pinned Rune fork,
`90ce05442d379849679ab435950c86babb6ba9e7`.

- **Direction:** Rune expects apps to have their own design language. Our
  state-driven views, custom layered artwork, cached SVGs and spring interaction
  are valid Rune usage; Rune Kit is optional, not a required appearance.
- **Visual comparison:** The user preferred both the approved SVG capsule and
  numeral over the Rune geometry alternatives in `examples/material_comparison.rs`.
  Keep the approved artwork as the baseline. This was an exploratory comparison,
  not proof that a more refined geometry approach cannot match or exceed it.
- **Theme and scale:** Gather the few actual palette and size decisions in one
  coherent place. SVG gradients and highlights are baked in: investigate safe,
  explicit semantic recoloring outside the render loop, preserving the approved
  default and comparing a contrasting palette. Use Rune's image cache and
  appropriate drawing size/output scale; verify changes at fractional scale.
- **Layout and accessibility:** Prefer Rune layout and stable keys for variable
  workspace counts and multi-digit labels; preserve readable text for names and
  localization. Give custom controls meaningful roles, labels, state and focus
  behavior, and test what the niri overlay actually exposes.
- **Interaction and verification:** Use Rune's hover/press, motion, scenes and
  headless screenshots where they preserve the visual reference. Compare rest,
  hover, press and selected states at study and bar sizes; check the result in
  niri. Consider Kit for conventional menu controls only when it helps; its
  in-window popovers do not replace our separate Wayland overlay.
- **Glass limitation:** `backdrop_blur` blurs Rune content already drawn in the
  same window, not the desktop behind Snowbar. Native system material is macOS
  specific at this revision. Our reflections are artwork, not live blur or
  refraction. Niri 26.04 separately supports background blur, but our VM study's
  whole-surface blur was too strong and extended outside the capsule; the user
  chose not to pursue per-capsule blur now. See `docs/niri-blur-study.md`.

Sources: [Rune's application example](https://github.com/mindnbytes/rune/blob/90ce05442d379849679ab435950c86babb6ba9e7/examples/hello/src/main.rs),
[core visual builders](https://github.com/mindnbytes/rune/blob/90ce05442d379849679ab435950c86babb6ba9e7/crates/rune/src/el/visuals.rs),
[image cache](https://github.com/mindnbytes/rune/blob/90ce05442d379849679ab435950c86babb6ba9e7/crates/rune/src/image.rs),
[backdrop semantics](https://github.com/mindnbytes/rune/blob/90ce05442d379849679ab435950c86babb6ba9e7/crates/rune/src/el/backdrop.rs),
[Rune Kit](https://github.com/mindnbytes/rune/blob/90ce05442d379849679ab435950c86babb6ba9e7/crates/rune-kit/README.md),
and [Hokum's panel](https://github.com/anomalyco/hokum/blob/e2e31b146792da21b9c6c691703433a6d2d72bbe/src/panel.rs).
