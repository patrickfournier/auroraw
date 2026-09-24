# Spike 5: the interface slice on Qt Quick, through cxx-qt

> **Status: measured; adopted by D-094 (2026-09-24), which supersedes D-072.** The code is on the
> branch `spike/qt-quick`, in `spikes/qt-ui/`, not merged. Developed on Linux with the Qt 6.4.2 of
> Ubuntu 24.04, then built and tested on **Linux, Windows and macOS in CI with Qt 6.8.3** (see
> "Three platforms"); offscreen and software-rendered throughout.

## Question

Spike 2 chose Slint and kept Qt Quick as the fallback "if Slint shows a blocking problem". Two things
were left open there: the cost of driving Qt from a Rust core ("Qt through Rust"), and how the two
toolkits compare once the application is more than a viewer. WP8 then showed what Slint lacks: modal
dialogs, a native folder dialog, menus with shortcuts, layout that survives a resizable window. None was
blocking, and each cost code. Patrick asked whether Qt Quick would spare that, and pointed at Qt's own
Rust bridge. This spike measures the cost of the same first screens on Qt Quick.

## Method

The first screens of the Slint shell, ported and driven by the same engine (no product crate modified):
the welcome list, the New workspace dialog (name, folder, preview, errors), the library grid over the
catalogue with thumbnails from the engine's `ThumbnailService`, the menus, adding a source with its
progress, translations, and tests. The bridge is **cxx-qt 0.10.0** (KDAB, MIT or Apache-2.0, Rust 1.85
or later, cmake 3.24 or later, `qmake` on the path) on the system's **Qt 6.4.2**: nothing was downloaded
beyond cargo crates and six small QML module packages (`qml6-module-qtquick` and its neighbours, 260 KB).

The reading rule was written before the work (see the plan of 2026-09-23): reopen D-072 only if (1) the
C++ needed is about 100 lines or fewer and ordinary features need none; (2) an incremental rebuild stays
near 30 s and CI needs one Qt install step per platform; (3) the slice is not longer than the Slint one;
(4) headless tests work, modality included; (5) it clearly removes the custom parts Slint made us write.

Everything ran with `QT_QPA_PLATFORM=offscreen` and the software renderer: no window on anyone's
display, pictures from `grabToImage`, input from QtQuickTest's real events. So **no frame rate, colour,
HiDPI or Wayland claim is made**, and memory and start-up figures are relative.

## Results

### The slice works, in Rust and QML, with almost no C++

Welcome list, modal New workspace dialog, a `GridView` of thumbnails over a Rust `QAbstractListModel`
(thumbnails through `image://thumbs/<photo id>`), `MenuBar` with separators, shortcuts, and the Edit items
on the focused text field, French through `qsTr` and `.ts`, a background scan whose engine events reach
the interface. 14 tests pass (8 on modality and menus, 6 on the grid), with real key and mouse events.

### Measures

| Measure | Qt Quick + cxx-qt | Slint (same slice, approximate) |
| --- | --- | --- |
| Lines of the slice (no tests) | QML 507, Rust 749, C++ 61: **1,317** | `.slint` about 415 and Rust about 760: **about 1,180**, counted by eye from the files that hold the same features (the Qt side includes about 60 lines for the scan's events, counted elsewhere for Slint) |
| C++ | **61 lines**: 41 for the image provider and its registration, 12 to load a translation, 8 for the test runner's entry | none |
| `unsafe` | 19 occurrences: 11 in the C ABI glue of the image provider, 6 for the model's begin/end reset (required by cxx-qt), 2 elsewhere. The product's workspace lint is `unsafe_code = "deny"` | none |
| Clean debug build, with the engine's crates | 140 s (peak memory of the build 2.4 GB) | not measured |
| Incremental rebuild | 9.3 s after a QML edit, a bridge edit or a plain Rust edit | 8.3 s after a `.slint` edit, 4.6 s after a Rust edit (product binary) |
| Resident memory, offscreen, software, debug | 87 MB with 80 photos, 92 MB with 2,000 | not comparable here |
| Qt libraries linked | 12 libraries, 30 MB (0.7 MB of it test-only); QML modules used 20 MB (QtQuick 10, Controls 8.7); the system's ICU adds 35 MB (29 MB of data) | statically linked, the executable was 40.5 MB stripped in spike 2 |
| Tests | 190 lines of QML for 14 tests, real input events, headless | 2,138 lines in `headless_tests.rs` (54 tests) for the whole shell, Slint's preliminary testing backend |
| Translations | 34 strings, `.ts` and `lupdate`/`lrelease`, 12 lines of C++ to load them; runtime language switching (`retranslate`) not built | gettext `.po`, bundled, switch at run time |

### What Slint made us build, and what Qt Quick does

| | Slint (WP8) | Qt Quick |
| --- | --- | --- |
| Modal dialog | `Modal` component, dimming layer, Rust guards for commands, tabs, banner | `Dialog { modal: true }`: **clicks behind it are blocked, the menu bar too** (tested). **Shortcuts are not blocked** (tested): a guard is still needed, as with Slint |
| Native folder dialog | `rfd` crate, window handle, `wayland` feature, our own waiting layer | `FolderDialog` (native where the platform has one, a Qt Quick one otherwise; only the fallback was seen here) |
| Menus: shortcuts, separators | custom hamburger overlay (Slint's context menus cannot show shortcuts) | `MenuBar`, `Action`, `MenuSeparator`; in-window on Linux and Windows, native on macOS through another module (not tried) |
| Edit menu on the focused field | deliver the platform's shortcut to the field | the field's own `undo()`, `cut()`, `paste()`...: one line each (tested) |
| Resizable window, columns from the width | layout loops to fix, columns computed in Slint and Rust | `GridView` and a binding; scroll into view and arrow keys are its own |
| PageUp, PageDown, Home, End | written in Rust (`grid::jump`) | written in QML, the same (the `GridView` has arrows only) |
| Image bytes from Rust | `Image::from_rgba8`, no C++ | **an image provider in C++** (or a data URL): needed for the developed image view too |
| Testing without a display | Slint's testing backend (preliminary, version-pinned) | QtQuickTest with real events; needs a C++ entry (`quick_test_main`) |

### What went wrong or surprised

- `TestCase` does not run in a plain application: the QtQuickTest runner must be called, from C++ (8
  lines, reached through the C ABI).
- The runner makes its own QML engine, so the image provider is not registered there (the tests see no
  thumbnails; registering it would need a `QObject` with `qmlEngineAvailable`, meaning `moc`).
- The `.qml` files of a cxx-qt module are not types for an outside importer (the tests load `Main.qml`
  by URL).
- `Overlay.overlay` is reachable only from an `Item`.
- A custom dark palette set on the window is inherited by dialogs, which stay light: theming Controls is
  its own work (not done).
- cxx-qt-lib has no `QTranslator`, no `QQuickImageProvider` and no `retranslate`.
- The first cxx-qt build needed no fight; the API of the book matched 0.10, but the crate is pre-1.0.
- An open workspace holds its folder: the previous session has to be dropped before another opens
  (true of the Slint shell too, where it took a bug fix).

### Three platforms (CI, Qt 6.8.3)

A workflow that runs only on the branch (`.github/workflows/spike-qt.yml`) installs Qt with **one step**
(`jurplel/install-qt-action`, pinned by hash, 6.8.3, cached), builds the spike and runs both suites
offscreen. Third run, all green:

| | Linux x64 (GCC) | Windows x64 (MSVC 2022) | macOS arm64 (Apple clang) |
| --- | --- | --- | --- |
| Tests (14) | 14 passed | 14 passed | 14 passed |
| Debug build, cold Rust cache (first run) | 324 s | 599 s | 378 s |
| Debug build, later runs (partly cached) | 173 s | 219 s | 323 s |
| Qt payload | 39.5 MB of libraries linked | **121 MB** `windeployqt` bundle (release DLLs, QML modules, no executable) | not measured |

So the same code builds against Qt 6.4.2 (Ubuntu) and Qt 6.8.3 (all three), with no source change. The
run also gave:

- **macOS crashed under the offscreen platform with the default (native macOS) style**: `objc_msgSend`
  on a bad object when the first control was drawn (`EXC_BAD_ACCESS`, address 0x2). The native styles
  draw through the system and expect a real window server. The tests now force `Fusion`; with it all
  three pass. Consequence: **the native styles are not exercised by these tests**, and how they look and
  behave stays to be checked on real machines.
- On Windows the first run gave no test output in the log (the runner's own output was lost); the script
  now has each suite write its results to a file and requires a clean totals line.
- Windows and macOS builds print warnings (ranlib, duplicate rpath) that are harmless.
- The 173 to 310 s "tests" step includes a needless rebuild caused by `QMAKE` being exported by the
  script; the tests themselves take 6 s and 3 s.

### Not measured

A release build and its real bundle (macOS not measured at all; the Windows figure is a debug
executable's `windeployqt` output), frame rate, colour and wide-gamut display, HiDPI, Wayland, the
native folder dialog, input methods, accessibility of this prototype (Qt's is said to work; spike 2 could
not confirm it with the test script), the macOS native menu, and how Qt's native Controls styles look
and behave on Windows and macOS (the tests use Fusion).

## Qt Bridge for Rust

Qt's own bridge (crate `qtbridge`, repository `qt/qtbridge-rust`) was left out at Patrick's decision:
too early while in beta. What is known, from its blog posts and repository: public beta, version 0.3
("Technology Preview" next); `#[qobject]`, `#[qproperty]`, `#[qslot]`, `#[qsignal]` over
`Rc<RefCell<T>>`; wrappers generated at build time; **needs Qt 6.10 or later**, Rust 1.88 or later,
`qmake` on the path; Linux x86_64, Windows x64, macOS arm64 (experimental); pre-release code under the Qt
licence terms for commercial licensees; compile times "increased substantially" in 0.3; since 0.3 it
shares its build utilities and basic types with CXX-Qt and interoperability is planned. **Re-check when
it leaves the beta.** The QML written here is independent of the binding, so changing bridge would mean
rewriting `launcher.rs` and `grid.rs` (519 lines), not the interface.

## Reading against the rule

1. **C++**: 61 lines, none for ordinary features (models, properties, signals, threads, invokables are
   Rust). The provider is a fixed cost that the developed image view needs anyway. Met, with the 19
   `unsafe` occurrences and the workspace's `unsafe_code = "deny"` as a policy point.
2. **Rebuild and CI**: 9.3 s incremental, met. One Qt install step per platform: **met** (one action
   step on Linux, Windows and macOS; the build takes 3 to 10 minutes in CI).
3. **Size**: about 10 % larger than the Slint slice, about the same. Met.
4. **Headless tests, modality included**: met, with a C++ entry.
5. **Removes the custom parts**: modal dialogs (mouse), the folder dialog, menu shortcuts and separators,
   the Edit menu, resizing: yes. Left: the shortcut guard, the image provider, dark theming.

By the rule, reopening D-072 is justified: every point is met, including the one that was open.
What the rule did not include, and the spike could not measure, remains: real displays (colour, HiDPI,
Wayland, the native styles and dialogs), which neither toolkit has passed yet.

## Recommendation

This is Patrick's decision. The spike removes the doubts that Qt through Rust is costly, fragile to
work with, or hard to build on the three platforms. What it leaves, and what would weigh on the choice:

- **Weight**: the Qt libraries (30 to 40 MB), the QML modules (20 MB) and ICU (35 MB on Linux) come to
  about 85 MB on Linux and **121 MB on Windows** (`windeployqt`), next to a Slint executable of about
  40 MB that links everything statically.
- **cxx-qt is pre-1.0**, and Qt Bridge, the vendor's own, is in beta: the bridge is the least mature part
  of the Qt option, more than Qt itself. It kept working from Qt 6.4 to 6.8 unchanged.
- **Two languages in the interface** (QML and Rust), against one in Slint, and 61 lines of C++ that a
  developed-image view needs anyway.
- **The native styles are unverified** (they crash offscreen on macOS), as is everything on a real
  display, for both toolkits.
- **`unsafe`**: the product's workspace denies it; a Qt crate needs an exception.

My reading: Qt Quick removes real, recurring costs (modality, dialogs, menus, richer controls for the
develop module, a large ecosystem) at a modest and bounded price, and every criterion set in advance is
met, including the three-platform build. The cost of switching is smallest now, before WP9: the
interface so far is about 1,460 lines of `.slint`, 2,560 of Rust and 2,140 of tests, the engine's 14,000
lines do not change. **I would switch**, in a branch that keeps the Slint shell on `dev` until the Qt one
reaches parity, and I would first put the two real-display checks (a native folder dialog and the
native style on macOS and Windows, on your machines) at the head of the port. If the bundle weight is a
dealbreaker, the answer is to stay with Slint and add the gaps above to spike 2's "what to watch".

## What remains

- [x] The same spike built and tested on Windows and macOS (CI), on a newer Qt (6.8.3).
- [ ] A release build and a real bundle on each platform (Linux, macOS not measured).
- [ ] The native Controls styles on real Windows and macOS machines.
- [ ] Colour, HiDPI and Wayland on a real display, for both toolkits (as spike 2).
- [ ] Qt Bridge, when out of beta.
- [ ] Theming Controls with a dark palette; runtime language switch.

## Running it

See `spikes/qt-ui/README.md` on the branch: `./run-tests.sh` builds, makes throwaway fixtures under a
temporary folder and runs both suites offscreen.
