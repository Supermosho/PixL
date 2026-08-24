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

# Getting keystrokes into the app under Xvfb takes more care than it looks.
# There is no window manager, so `windowactivate` fails and X input focus has
# to be set by hand; and `xdotool key --window` is *not* a substitute, because
# it uses XSendEvent and GTK4 ignores synthetic key events. Setting focus once
# is not enough either — it does not stick until the window is viewable — so
# this retries until X agrees, and gives up loudly rather than running the rest
# of the script against a window that cannot hear it.
focus_app() {
    for _ in $(seq 1 15); do
        xdotool windowactivate "$WID" 2>/dev/null
        xdotool windowfocus "$WID" 2>/dev/null
        # Click the canvas too, so the key controller on the window sees the
        # event rather than a focused header button swallowing it.
        xdotool mousemove 700 520 click 1 2>/dev/null
        sleep 1
        [ "$(xdotool getwindowfocus 2>/dev/null)" = "$WID" ] && return 0
    done
    return 1
}

if ! focus_app; then
    echo "FAIL: could not give the window keyboard focus; keystroke checks would be meaningless"
    exit 1
fi
echo "window has keyboard focus"

shot() { sleep 2; import -window root "$OUT/$1.png" 2>/dev/null; echo "  captured $1"; }

# Capturing a screenshot proves the app did not crash; it does not prove the
# thing we asked for happened. `changed` fails when two frames are identical,
# which is what a keystroke that went nowhere looks like.
FAILURES=0
changed() {
    local a="$OUT/$1.png" b="$OUT/$2.png"
    local sa sb
    sa=$(identify -quiet -format "%#" "$a" 2>/dev/null)
    sb=$(identify -quiet -format "%#" "$b" 2>/dev/null)
    if [ -z "$sa" ] || [ -z "$sb" ]; then
        echo "  ?? could not hash $1/$2, skipping the check"
        return
    fi
    if [ "$sa" = "$sb" ]; then
        echo "  FAIL: $3 (frames $1 and $2 are identical)"
        FAILURES=$((FAILURES + 1))
    else
        echo "  ok: $3"
    fi
}

shot 01-open

echo "== tool shortcuts and panels =="
# The Color Adjustments pane computes a histogram, which is the only thing in
# the app that uses a compute shader and a storage-buffer readback. It is
# therefore the step that exercises the GL/GLES entry points the headless tests
# (desktop GL only) cannot reach.
xdotool key a; shot 02-adjustments
changed 01-open 02-adjustments "'a' opened the Color Adjustments panel"
xdotool key f; shot 03-effects
changed 02-adjustments 03-effects "'f' opened the Effects panel"
xdotool key b; shot 04-paint-tool
changed 03-effects 04-paint-tool "'b' selected the Paint tool"

echo "== painting =="
xdotool mousemove 300 380 mousedown 1
for x in 340 380 420 460 500 540 580 620; do
    xdotool mousemove "$x" $((360 + x / 8))
    sleep 0.1
done
xdotool mouseup 1
shot 05-painted

echo "== undo =="
xdotool key ctrl+z; shot 06-undone
changed 05-painted 06-undone "Ctrl+Z undid the stroke"

echo "== selection tool =="
# Coordinates must land on the canvas, not on a floating panel. The Layers
# panel occupies roughly x < 300, and a drag starting inside it selects
# nothing at all — which is how this step spent a while "passing" while the
# marching-ants check below failed for a reason that had nothing to do with
# the ants.
xdotool key m
xdotool mousemove 500 350 mousedown 1
xdotool mousemove 650 450
xdotool mousemove 800 600
xdotool mouseup 1
shot 07-selection
changed 06-undone 07-selection "dragging the marquee made a selection"

echo "== marching ants =="
# Two captures of the same selection a moment apart. If they are identical the
# ants are not animating, which is the difference between a selection outline
# and a static dotted rectangle.
sleep 1
import -window root "$OUT/08-ants-a.png" 2>/dev/null
sleep 1
import -window root "$OUT/08-ants-b.png" 2>/dev/null
changed 08-ants-a 08-ants-b "the marching ants animate"

echo "== quick selection =="
xdotool key q; shot 09-quick-select
changed 08-ants-a 09-quick-select "'q' opened the Quick Selection panel"
# Hovering must paint the preview without any click.
xdotool mousemove 700 520; sleep 2
import -window root "$OUT/10-quick-hover.png" 2>/dev/null
changed 09-quick-select 10-quick-hover "hovering shows the Quick Selection preview"

echo "== zoom =="
xdotool key ctrl+0; shot 11-zoom-fit

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

if [ "$FAILURES" -gt 0 ]; then
    echo "FAIL: $FAILURES interaction check(s) did not change anything on screen"
    exit 1
fi

echo
echo "screenshots in $OUT"
ls -1 "$OUT"/*.png
