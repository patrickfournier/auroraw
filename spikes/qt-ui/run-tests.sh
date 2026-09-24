#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Builds, then runs both QtQuickTest suites offscreen on throwaway machines under $WORK.
set -e
export QMAKE="${QMAKE:-$(command -v qmake6 || command -v qmake)}"
export QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software
# A platform-independent style: the native ones (macOS, Windows) draw through the system and expect a
# real window server (on macOS the first Button crashed in objc_msgSend under the offscreen platform).
export QT_QUICK_CONTROLS_STYLE="${QT_QUICK_CONTROLS_STYLE:-Fusion}"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="${WORK:-${TMPDIR:-/tmp}/qt-ui-spike-work}"
# Under Git Bash on Windows, native programs need Windows paths (D:/a/...), not /d/a/...
if command -v cygpath >/dev/null 2>&1; then
    HERE="$(cygpath -m "$HERE")"
    WORK="$(cygpath -m "$WORK")"
fi
cargo build --manifest-path "$HERE/Cargo.toml"
BIN="${CARGO_TARGET_DIR:-$HERE/target}/debug/qt-ui-spike"
rm -rf "$WORK" && mkdir -p "$WORK/empty" "$WORK/extra"

# A machine with one workspace of 80 generated photos, and a folder of 5 more to add later.
SPIKE_HOME="$WORK/fixture" SPIKE_PHOTOS=80 "$BIN" --make-fixture
for n in 0 1 2 3 4; do cp "$WORK/fixture/Card/IMG_000$n.jpg" "$WORK/extra/IMG_000${n}_x.jpg"; done

# Each suite writes its results to a file (some consoles lose the runner's own output), which is
# printed afterwards and must end in a clean totals line.
suite() {
    name="$1"
    shift
    out="$WORK/$name.txt"
    status=0
    env SPIKE_TESTS="$HERE/tests" "$@" "$BIN" --quicktest -input "$HERE/tests/tst_$name.qml" -o "$out,txt" || status=$?
    [ -f "$out" ] && cat "$out"
    if [ "$status" -ne 0 ]; then
        echo "suite $name: exit status $status"
        return 1
    fi
    grep -q "^Totals: .* 0 failed" "$out" || {
        echo "suite $name: no clean totals line"
        return 1
    }
}
suite modality SPIKE_HOME="$WORK/empty"
suite grid SPIKE_HOME="$WORK/fixture" SPIKE_EXTRA="$WORK/extra"
