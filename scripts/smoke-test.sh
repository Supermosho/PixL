#!/usr/bin/env bash
# Headless smoke test: launch Pixelmagic under Xvfb with software GL, drive it
# with xdotool, and capture screenshots.
#
# This is not a substitute for the unit and render tests — it is the thing that
# catches what those cannot: a window that never maps, a panel that crashes when
# it is built, a shortcut wired to nothing. Every bug it has found so far was
# invisible to `cargo test`.
#
# Usage: scripts/smoke-test.sh [binary] [output-dir]
set -uo pipefail

BIN="${1:-target/release/pixelmagic}"
OUT="${2:-/tmp/pixelmagic-smoke}"
DISPLAY_NUM=":99"
SAMPLE="$OUT/sample.png"

mkdir -p "$OUT"

for cmd in Xvfb xdotool import; do
    command -v "$cmd" >/dev/null || { echo "missing: $cmd"; exit 127; }
done

if ! pgrep -f "Xvfb $DISPLAY_NUM" >/dev/null; then
    Xvfb "$DISPLAY_NUM" -screen 0 1600x1000x24 >"$OUT/xvfb.log" 2>&1 &
    sleep 3
fi
export DISPLAY="$DISPLAY_NUM"
export LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe GDK_BACKEND=x11
export RUST_LOG="${RUST_LOG:-pixelmagic=info}"

# A test image with a known gradient, so a colour-space or orientation
# regression is visible at a glance rather than needing a reference file.
python3 - "$SAMPLE" <<'PY'
import struct, sys, zlib
w, h = 640, 420
rows = []
for y in range(h):
    row = bytearray([0])
    for x in range(w):
        row += bytes((int(255 * x / w), int(255 * y / h),
                      128 + 127 * ((x // 40 + y // 40) % 2), 255))
    rows.append(bytes(row))
def chunk(tag, data):
    body = tag + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xffffffff)
png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(b"".join(rows), 6))
       + chunk(b"IEND", b""))
open(sys.argv[1], "wb").write(png)
PY

echo "== shader self-check =="
"$BIN" --check-shaders || exit 1

echo "== launching =="
"$BIN" "$SAMPLE" >"$OUT/app.log" 2>&1 &
APP=$!
trap 'kill $APP 2>/dev/null' EXIT

for _ in $(seq 1 30); do
    WID=$(xdotool search --class pixelmagic 2>/dev/null | tail -1)
    [ -n "${WID:-}" ] && break
    sleep 1
done
[ -n "${WID:-}" ] || { echo "FAIL: no window appeared"; sed -n 1,20p "$OUT/app.log"; exit 1; }
echo "window $WID mapped"
sleep 4

xdotool windowactivate "$WID" 2>/dev/null
xdotool windowfocus "$WID" 2>/dev/null
sleep 1

shot() { sleep 2; import -window root "$OUT/$1.png" 2>/dev/null; echo "  captured $1"; }

shot 01-open

echo "== tool shortcuts and panels =="
# The Color Adjustments pane computes a histogram, which is the only thing in
# the app that uses a compute shader and a storage-buffer readback. It is
# therefore the step that exercises the GL/GLES entry points the headless tests
# (desktop GL only) cannot reach.
xdotool key --window "$WID" a; shot 02-adjustments
xdotool key --window "$WID" f; shot 03-effects
xdotool key --window "$WID" b; shot 04-paint-tool

echo "== painting =="
xdotool mousemove 300 380 mousedown 1
for x in 340 380 420 460 500 540 580 620; do
    xdotool mousemove "$x" $((360 + x / 8))
    sleep 0.1
done
xdotool mouseup 1
shot 05-painted

echo "== undo =="
xdotool key --window "$WID" ctrl+z; shot 06-undone

echo "== selection tool =="
xdotool key --window "$WID" m
xdotool mousemove 250 300 mousedown 1
xdotool mousemove 550 550
xdotool mouseup 1
shot 07-selection

echo "== zoom =="
xdotool key --window "$WID" ctrl+0; shot 08-zoom-fit

kill $APP 2>/dev/null
wait $APP 2>/dev/null

echo
echo "== log =="
grep -vE 'dbus|Gtk-WARNING|^$' "$OUT/app.log" | head -20

if grep -qiE 'panicked|SIGSEGV|Segmentation' "$OUT/app.log"; then
    echo "FAIL: the app reported a crash"
    exit 1
fi

# libepoxy aborts rather than returning an error when a GL entry point is
# missing from the current context — the failure mode for calling a desktop-only
# function on a GLES context. It prints this first, so catch it by name.
if grep -qiE 'No provider of' "$OUT/app.log"; then
    echo "FAIL: the app called a GL entry point this context does not have"
    grep -A3 -i 'No provider of' "$OUT/app.log"
    exit 1
fi

echo
echo "screenshots in $OUT"
ls -1 "$OUT"/*.png
