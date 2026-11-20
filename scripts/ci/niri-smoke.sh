#!/bin/sh
# Experimental nested-compositor smoke check for a packaged Snowbar binary.
#
# This script is intentionally explicit about what it has and has not proven.
# It runs the Nix-packaged binary (not `nix develop`) inside a disposable
# nested niri session hosted by headless Weston with Mesa software rendering.
# It asserts:
#   - the bar's layer-surface namespace appears on niri's top layer;
#   - the client received a layer-shell configure;
#   - the client requested its expected positive exclusive zone;
#   - the bar's Wayland surface attached a non-null buffer and committed it.
#
# It does NOT prove niri's normal niri-session behaviour, input, focus,
# fullscreen stacking, multi-output handling, suspend/resume, or appearance.
# The headless Weston + wgpu + Mesa combination is the least proven part and
# must be checked in the first hosted-runner run. If the nested compositor or
# software rendering fails, keep this job experimental instead of weakening
# the assertions.
#
# Never runs a power action and never touches a live session.

set -eu

snowbar=${1:-}
if [ -z "$snowbar" ] || [ ! -x "$snowbar" ]; then
    echo "usage: $0 /absolute/path/to/packaged/snowbar" >&2
    exit 2
fi

artifact_dir=${SNOWBAR_SMOKE_DIR:-${TMPDIR:-/tmp}/snowbar-smoke}
mkdir -p "$artifact_dir"
logs=$(mktemp -d "$artifact_dir/logs.XXXXXX")
# Keep the runtime paths short enough for Unix-domain display and IPC sockets.
workdir=$(mktemp -d "${TMPDIR:-/tmp}/snowbar.XXXXXX")

weston_pid=
niri_pid=
snowbar_pid=

cleanup() {
    status=$?
    for pid in "$snowbar_pid" "$niri_pid" "$weston_pid"; do
        if [ -n "$pid" ]; then
            kill "$pid" 2>/dev/null || true
        fi
    done
    sleep 1
    for pid in "$snowbar_pid" "$niri_pid" "$weston_pid"; do
        if [ -n "$pid" ]; then
            kill -9 "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
    rm -rf -- "$workdir"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$workdir/home" "$workdir/config" "$workdir/cache"
mkdir -m 700 "$workdir/host-runtime" "$workdir/guest-runtime"
config_file="$workdir/config/niri.kdl"
host_socket="$workdir/host-runtime/wayland-host"

fail() {
    printf '::error::%s\n' "$*" >&2
    for name in weston niri snowbar; do
        if [ -f "$logs/$name.log" ]; then
            printf '\n--- %s.log (tail) ---\n' "$name" >&2
            tail -n 40 "$logs/$name.log" >&2 || true
        fi
    done
    if [ -f "$logs/weston.stdout.log" ]; then
        printf '\n--- weston.stdout.log (tail) ---\n' >&2
        tail -n 40 "$logs/weston.stdout.log" >&2 || true
    fi
    if [ -f "$logs/layers.json" ]; then
        printf '\n--- layers.json ---\n' >&2
        cat "$logs/layers.json" >&2 || true
    fi
    exit 1
}

for tool in niri weston jq; do
    command -v "$tool" >/dev/null 2>&1 ||
        { echo "::error::required smoke tool not found on PATH: $tool" >&2; exit 2; }
done

# Pin the smoke's software-rendering inputs to the nixpkgs revision from the
# project's flake. The workflow passes SNOWBAR_SMOKE_MESA from
# `nix build --inputs-from . nixpkgs#mesa`.
if [ -n "${SNOWBAR_SMOKE_MESA:-}" ] && [ -d "$SNOWBAR_SMOKE_MESA/lib/dri" ]; then
    LIBGL_DRIVERS_PATH="$SNOWBAR_SMOKE_MESA/lib/dri${LIBGL_DRIVERS_PATH:+:$LIBGL_DRIVERS_PATH}"
    export LIBGL_DRIVERS_PATH
fi
if [ -n "${SNOWBAR_SMOKE_MESA:-}" ]; then
    for vendor in "$SNOWBAR_SMOKE_MESA"/share/glvnd/egl_vendor.d/*mesa*.json \
                  "$SNOWBAR_SMOKE_MESA"/etc/glvnd/egl_vendor.d/*mesa*.json; do
        [ -f "$vendor" ] || continue
        __EGL_VENDOR_LIBRARY_FILENAMES="$vendor"
        export __EGL_VENDOR_LIBRARY_FILENAMES
        break
    done
fi
vulkan_icds=
if [ -n "${SNOWBAR_SMOKE_MESA:-}" ] && [ -d "$SNOWBAR_SMOKE_MESA/share/vulkan/icd.d" ]; then
    for icd in "$SNOWBAR_SMOKE_MESA"/share/vulkan/icd.d/*lvp*.json; do
        [ -f "$icd" ] || continue
        vulkan_icds="${vulkan_icds}${vulkan_icds:+:}$icd"
    done
fi
if [ -n "$vulkan_icds" ]; then
    VK_ICD_FILENAMES="$vulkan_icds"
    export VK_ICD_FILENAMES
fi

LIBGL_ALWAYS_SOFTWARE=1
GALLIUM_DRIVER=llvmpipe
WGPU_POWER_PREF=low
RUNE_GPU_LOG=1
TZ=UTC
XDG_SESSION_TYPE=wayland
XDG_CURRENT_DESKTOP=niri
export LIBGL_ALWAYS_SOFTWARE GALLIUM_DRIVER
export WGPU_POWER_PREF RUNE_GPU_LOG TZ XDG_SESSION_TYPE XDG_CURRENT_DESKTOP
# A hosted runner has no X server; do not let winit fall back to X11 or an
# inherited WAYLAND_SOCKET.
unset DISPLAY WAYLAND_SOCKET

cat >"$config_file" <<'EOF'
layout {
    gaps 0
}

animations {
    off
}
EOF

if ! niri validate --config "$config_file" >"$logs/niri-validate.log" 2>&1; then
    cat "$logs/niri-validate.log" >&2
    fail "niri rejected the smoke config"
fi

# The outer compositor needs only CPU rendering; nested niri can use Mesa
# software EGL without requiring Weston itself to provide surfaceless EGL.
XDG_RUNTIME_DIR="$workdir/host-runtime" \
HOME="$workdir/home" \
XDG_CONFIG_HOME="$workdir/config" \
XDG_CACHE_HOME="$workdir/cache" \
    weston \
        --backend=headless \
        --renderer=pixman \
        --width=1280 \
        --height=800 \
        --socket=wayland-host \
        --idle-time=0 \
        --no-config \
        --log="$logs/weston.log" \
        >"$logs/weston.stdout.log" 2>&1 &
weston_pid=$!

i=0
while [ ! -S "$host_socket" ]; do
    kill -0 "$weston_pid" 2>/dev/null || fail "Weston exited before creating $host_socket"
    i=$((i + 1))
    [ "$i" -lt 300 ] || fail "timed out waiting for Weston's Wayland socket"
    sleep 0.2
done

# Nested niri gets its own runtime directory, but connects to Weston through
# the absolute socket path (Wayland supports an absolute WAYLAND_DISPLAY).
XDG_RUNTIME_DIR="$workdir/guest-runtime" \
WAYLAND_DISPLAY="$host_socket" \
HOME="$workdir/home" \
XDG_CONFIG_HOME="$workdir/config" \
XDG_CACHE_HOME="$workdir/cache" \
    niri --config "$config_file" \
        >"$logs/niri.log" 2>&1 &
niri_pid=$!

niri_wayland=
niri_socket=
i=0
while :; do
    niri_wayland=$(find "$workdir/guest-runtime" -maxdepth 1 -type s -name 'wayland-*' -print -quit)
    niri_socket=$(find "$workdir/guest-runtime" -maxdepth 1 -type s -name 'niri.*.sock' -print -quit)
    if [ -n "$niri_wayland" ] && [ -n "$niri_socket" ] &&
        NIRI_SOCKET="$niri_socket" niri msg version >/dev/null 2>&1; then
        break
    fi
    kill -0 "$niri_pid" 2>/dev/null || fail "niri exited before its sockets became ready"
    i=$((i + 1))
    [ "$i" -lt 200 ] || fail "timed out waiting for nested niri"
    sleep 0.2
done

NIRI_SOCKET="$niri_socket" niri msg version >"$logs/versions.txt" 2>&1 || true
niri --version >>"$logs/versions.txt" 2>&1 || true
weston --version >>"$logs/versions.txt" 2>&1 || true

NIRI_SOCKET="$niri_socket" niri msg outputs --json >"$logs/outputs.json" 2>"$logs/outputs.err" || {
    cat "$logs/outputs.err" >&2
    fail "niri outputs query failed"
}
output_name=$(jq -er '
    to_entries | map(select(.value.logical != null)) | select(length == 1) | .[0].key
' "$logs/outputs.json") || fail "nested niri did not report exactly one logical output"
output_width=$(jq -r --arg name "$output_name" '.[$name].logical.width // empty' "$logs/outputs.json")
output_height=$(jq -r --arg name "$output_name" '.[$name].logical.height // empty' "$logs/outputs.json")
[ -n "$output_width" ] && [ -n "$output_height" ] || fail "could not read the nested output size"
printf 'nested output: %s %sx%s\n' "$output_name" "$output_width" "$output_height"

# Run the packaged binary directly. This is deliberately not `nix develop`.
XDG_RUNTIME_DIR="$workdir/guest-runtime" \
WAYLAND_DISPLAY="$niri_wayland" \
NIRI_SOCKET="$niri_socket" \
HOME="$workdir/home" \
XDG_CONFIG_HOME="$workdir/config" \
XDG_CACHE_HOME="$workdir/cache" \
WAYLAND_DEBUG=1 \
    "$snowbar" >"$logs/snowbar.log" 2>&1 &
snowbar_pid=$!

i=0
while ! (
    NIRI_SOCKET="$niri_socket" niri msg layers --json >"$logs/layers.json" 2>/dev/null &&
    jq -e 'any(.[]; .namespace == "snowbar")' "$logs/layers.json" >/dev/null 2>&1
); do
    kill -0 "$snowbar_pid" 2>/dev/null || fail "snowbar exited before mapping its bar layer"
    i=$((i + 1))
    [ "$i" -lt 300 ] || fail "timed out waiting for the snowbar layer surface"
    sleep 0.2
done

sleep 0.5
NIRI_SOCKET="$niri_socket" niri msg layers --json >"$logs/layers.json" 2>"$logs/layers.err" || {
    cat "$logs/layers.err" >&2
    fail "niri layers query failed"
}
jq -e --arg out "$output_name" '
    any(.[];
        .namespace == "snowbar" and
        ((.layer | ascii_downcase) == "top") and
        .output == $out)
' "$logs/layers.json" >/dev/null || fail "the snowbar surface is not on the top layer of $output_name"

bar_line=$(grep -F 'get_layer_surface' "$logs/snowbar.log" | grep -F '"snowbar"' | tail -n 1 || true)
[ -n "$bar_line" ] || fail "Wayland debug log has no get_layer_surface request for namespace \"snowbar\""
layer_id=$(printf '%s\n' "$bar_line" | sed -n 's/.*zwlr_layer_surface_v1@\([0-9][0-9]*\).*/\1/p')
surface_id=$(printf '%s\n' "$bar_line" | sed -n 's/.*wl_surface@\([0-9][0-9]*\).*/\1/p')
[ -n "$layer_id" ] && [ -n "$surface_id" ] || fail "could not parse the bar's Wayland object IDs"

i=0
while ! grep -qE "zwlr_layer_surface_v1@${layer_id}\.configure\(" "$logs/snowbar.log"; do
    kill -0 "$snowbar_pid" 2>/dev/null || fail "snowbar exited before receiving a layer-shell configure"
    i=$((i + 1))
    [ "$i" -lt 150 ] || fail "timed out waiting for a layer-shell configure"
    sleep 0.2
done
# Prefer a settled configure if the compositor sent more than one.
sleep 0.5
configure_lines=$(grep -E "zwlr_layer_surface_v1@${layer_id}\.configure\(" "$logs/snowbar.log" || true)
configure_line=$(printf '%s\n' "$configure_lines" | grep -E "configure\([0-9]+, ${output_width}, 40\)" | tail -n 1 || true)
if [ -z "$configure_line" ]; then
    printf 'configures seen:\n%s\n' "$configure_lines" >&2
    fail "the bar did not receive a ${output_width}x40 configure"
fi

configure_args=$(printf '%s\n' "$configure_line" | sed 's/.*configure(//; s/).*//')
configure_width=$(printf '%s\n' "$configure_args" | cut -d, -f2 | tr -d ' ')
configure_height=$(printf '%s\n' "$configure_args" | cut -d, -f3 | tr -d ' ')
[ -n "$configure_width" ] && [ -n "$configure_height" ] || fail "could not parse the layer-shell configure arguments"
[ "$configure_width" -eq "$output_width" ] ||
    fail "bar configure width $configure_width does not span the ${output_width}px output"
[ "$configure_height" -eq 40 ] ||
    fail "bar configure height $configure_height is not the expected 40px"

grep -qF "zwlr_layer_surface_v1@${layer_id}.set_exclusive_zone(40)" "$logs/snowbar.log" ||
    fail "the bar did not request its expected positive 40px exclusive zone"
grep -qF "zwlr_layer_surface_v1@${layer_id}.set_anchor(13)" "$logs/snowbar.log" ||
    fail "the bar is not anchored to the top, left and right edges (anchor bits 13)"

i=0
while ! awk -v surface="wl_surface@$surface_id" '
    index($0, surface ".attach(") {
        attached = index($0, surface ".attach(wl_buffer@") != 0
    }
    attached && index($0, surface ".commit()") { mapped = 1; exit }
    END { exit !mapped }
' "$logs/snowbar.log"; do
    kill -0 "$snowbar_pid" 2>/dev/null || fail "snowbar exited before committing a non-null buffer"
    i=$((i + 1))
    [ "$i" -lt 300 ] || fail "timed out waiting for the bar to commit a non-null buffer"
    sleep 0.2
done

# Best-effort visual artifact; a missing screenshot must not hide a passing
# structural check.
if NIRI_SOCKET="$niri_socket" timeout 15 niri msg action screenshot-screen \
    --write-to-disk true --show-pointer false --path "$logs/screenshot.png" \
    >"$logs/screenshot.log" 2>&1; then
    sleep 1
fi

NIRI_SOCKET="$niri_socket" niri msg layers --json >"$logs/layers.final.json" 2>/dev/null || true

printf 'smoke passed: namespace=snowbar layer=top output=%s configure=%sx%s exclusive_zone=40\n' \
    "$output_name" "$configure_width" "$configure_height"
