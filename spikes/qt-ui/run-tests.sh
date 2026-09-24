#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Builds, then runs both QtQuickTest suites offscreen on throwaway machines under $WORK.
set -e
export QMAKE="${QMAKE:-/usr/bin/qmake6}"
export QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="${WORK:-${TMPDIR:-/tmp}/qt-ui-spike-work}"
cargo build --manifest-path "$HERE/Cargo.toml"
BIN="${CARGO_TARGET_DIR:-$HERE/target}/debug/qt-ui-spike"
rm -rf "$WORK" && mkdir -p "$WORK/empty" "$WORK/extra"

# A machine with one workspace of 80 generated photos, and a folder of 5 more to add later.
SPIKE_HOME="$WORK/fixture" SPIKE_PHOTOS=80 "$BIN" --make-fixture
for n in 0 1 2 3 4; do cp "$WORK/fixture/Card/IMG_000$n.jpg" "$WORK/extra/IMG_000${n}_x.jpg"; done

SPIKE_TESTS="$HERE/tests" SPIKE_HOME="$WORK/empty" "$BIN" --quicktest -input "$HERE/tests/tst_modality.qml"
SPIKE_TESTS="$HERE/tests" SPIKE_HOME="$WORK/fixture" SPIKE_EXTRA="$WORK/extra" "$BIN" --quicktest -input "$HERE/tests/tst_grid.qml"
