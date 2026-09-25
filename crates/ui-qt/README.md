# auroraw-ui-qt

The Qt Quick user interface (decision D-094). It replaces the Slint `crates/ui` at cutover (milestones
Q1 to Q7 of the port, `docs/m1-plan.md`), and until then `cargo run -p auroraw-app --features qt` runs it.

- `src/`: thin Rust objects over the engine (`launcher`, `grid`, the event `bus`), the C++ glue
  (`glue.cpp`: the thumbnail image provider and the translation loader; `quicktest.cpp`: the test
  runner's entry), and the pure logic (`app_settings`, `gridmath`).
- `qml/`: the screens. `Theme.qml` holds the neutral grey and the accent colours; the style is Fusion
  everywhere.
- `i18n/`: Qt Linguist `.ts` files, compiled by `lrelease` in `build.rs`. Update them after changing a
  `qsTr` string: `lupdate qml/*.qml -no-obsolete -ts i18n/auroraw_fr.ts -source-language en
  -target-language fr`, then translate what is new. `tests/translations.rs` checks them.
- `tests/qml/`: QtQuickTest suites, run offscreen with real input events by `tests/qml.rs`, one process
  per suite, each on a throwaway machine (`AURORAW_TEST_HOME`). To look at what a run draws, add a
  `grabImage(...).save(...)` to a test.

Trying a typeface without a rebuild: `AURORAW_FONT="Inter" AURORAW_FONT_SIZE=13 cargo run -p auroraw-app
--features qt`.
