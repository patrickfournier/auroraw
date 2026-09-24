# Qt Quick spike: the first screens of the interface, through cxx-qt

A time-boxed port of the Slint shell's welcome list, New workspace dialog, library grid and menus to
Qt Quick, driven by the same engine. Not a product crate: it has its own `[workspace]`, so the product
workspace, its lockfile and its CI are untouched. Results and reading: `docs/spikes/05-qt-quick-through-rust.md`.

Needs Qt 6.4 or later with its QML modules (on Ubuntu 24.04: `qt6-base-dev qt6-declarative-dev`,
`qml6-module-qtquick{,-controls,-dialogs,-layouts,-templates,-window}`, `qml6-module-qtqml{,-models}`,
`qml6-module-qttest`), cmake 3.24 or later, `qmake6` (`QMAKE=/usr/bin/qmake6`).

```sh
export QMAKE=/usr/bin/qmake6
cargo build

# A machine with one workspace of generated photos (nothing outside $SPIKE_HOME is touched).
export SPIKE_HOME=/tmp/spike-home
SPIKE_PHOTOS=80 target/debug/qt-ui-spike --make-fixture

# Offscreen (no display, no window): a picture, and the tests with real input events.
export QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software
target/debug/qt-ui-spike --snapshot=grid.png
SPIKE_TESTS=$PWD/tests target/debug/qt-ui-spike --quicktest -input tests/tst_grid.qml
SPIKE_HOME=/tmp/empty target/debug/qt-ui-spike --quicktest -input tests/tst_modality.qml   # empty folder
SPIKE_QM=spike_fr.qm ...    # a translation: lrelease i18n/spike_fr.ts -qm spike_fr.qm
```

Layout: `src/launcher.rs`, `src/grid.rs` (cxx-qt objects), `src/session.rs` (what they share),
`src/thumbs.rs` and `src/thumbs.cpp` (the image provider, the test runner entry and the translation
loader: the only C++), `qml/*.qml` (the interface), `tests/tst_*.qml` (QtQuickTest), `i18n/*.ts`.
