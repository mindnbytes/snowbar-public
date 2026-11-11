# Configuration candidates

Ideas gathered during visual prototyping, not an implemented schema or a
commitment to expose every detail. Prefer a few useful options over a theme
engine; settle defaults through VM testing first.

- Font family and size, with readable defaults.
- Bar text/icon color and active-workspace accent, to match different wallpapers.
- Bar background color and opacity, including a fully transparent default;
  blur only if a real, supported compositor capability is verified.
- Menu surface, text and border colors, plus opacity, for desktop theme matching.

The current SVG artwork bakes material colors into its gradients and highlights;
these ideas are not yet runtime-configurable. Test a small semantic palette
approach against the approved default and a contrasting alternative before
choosing a format or promising recoloring of all artwork. Configuration should
be straightforward for both people and coding agents to read and edit.
