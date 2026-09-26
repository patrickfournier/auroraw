// SPDX-License-Identifier: GPL-3.0-or-later
// QtQuickTest's runner, so that `tests/qml/tst_*.qml` run against the real QML module (the plain
// TestCase does not run outside a runner). Compiled only with the `quicktest` feature.
#include <QtQuickTest/quicktest.h>

extern "C" int auroraw_quick_test(int argc, char **argv, const char *dir) {
    return quick_test_main(argc, argv, "auroraw", dir);
}
