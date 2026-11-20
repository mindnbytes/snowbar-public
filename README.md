# Snowbar

A minimal, single-output top bar for NixOS + niri, built with Rust and Rune.
It shows niri workspaces, a local clock, and a confirmed log out/restart/shut
down menu.

## Build and run

The `snowbar` binary takes no command-line arguments. Run it from the niri
Wayland session; it uses `WAYLAND_DISPLAY` and `NIRI_SOCKET` from the
environment and runs until terminated.

With Nix on a Linux host, try Snowbar or add it to your user profile:

```sh
nix run github:mindnbytes/snowbar-public
nix profile install github:mindnbytes/snowbar-public
```

To build and run from a checkout, no development shell is needed:

```sh
nix build .#snowbar
./result/bin/snowbar
```

For a plain Cargo build on Linux, use Rust 1.99 (the version in the pinned
dev shell), `pkg-config`, and the Wayland development libraries. The runtime
needs Wayland, xkbcommon, fontconfig, and working graphics drivers and loaders.
Power actions also need `busctl` from systemd on `PATH`. The Nix package wraps
the binary with these runtime paths; a plain Cargo build relies on the host
environment.

```sh
cargo build --locked --bin snowbar
./target/debug/snowbar
```

## Single-output limitation

Snowbar supports exactly one Wayland output. It shows nothing when Rune reports
more than one targetable display, and the workspace area becomes unavailable
when niri does not report exactly one output. Restart the bar after changing
the display setup; hotplug and multi-monitor behavior are not supported. See
[`docs/workspace-policy.md`](docs/workspace-policy.md) for the exact policy.

## Optional niri startup

Snowbar does not configure niri automatically. If `snowbar` is on niri's
`PATH`, add this to your niri config:

```kdl
spawn-at-startup "snowbar"
```

Otherwise pass an absolute path to the installed executable.

## Documentation

- [`niri-rune-bar.md`](niri-rune-bar.md): project brief and visual baseline.
- [`docs/`](docs/README.md): behavior notes, Rune integration checks and
  verification history.
