# Spike 2: the image view and the interface toolkit

> **Status: decided (D-072): Slint.** Slint, iced and Qt Quick measured on Linux, on a real
> display, with real RAW frames; Slint and iced also on Windows. Not blocking, to be checked during
> M1: Slint on macOS, and Qt on Windows and macOS if the fallback is ever needed.

## Question

Which toolkit gives the smoothest, most colour-accurate image view with the least weight, while
still being able to build the rest of the application? (docs/technical-spikes.md, §3)
The priorities set by Patrick (D-069): the performance and colour fidelity of the image view, and
lightness.

## Method

The same small application, three times: **Slint 1.18** (Rust, femtovg renderer), **iced 0.14**
(Rust, wgpu) and **Qt Quick 6.4.2** (C++ and QML). Each has:

- an **image view**, fed a new RGBA8 image on every frame, shown 1:1. The images are real: a
  pan across a Nikon D850 RAW file (45 MP), developed by the spike 1 pipeline, held in memory so
  the measures are those of the toolkit alone;
- a **grid of 100,000 thumbnails**, virtualised, scrolling 45 px per frame;
- a **keyword tree** of about 4,400 nodes (not in iced), a **text field**, and a **language
  switch** French and English (not in iced);
- a scripted benchmark that runs for eight seconds and writes JSON.

The measures are the intervals between presented frames on a **2560x1440 display at 59.95 Hz**
(NVIDIA GTX 1650 SUPER, X11), after 30 warm-up frames. Every frame of the view mode is a new
texture upload, 5 MB for a 1340x964 view and 12 MB for 2200x1400.

**A first attempt was made over a remote desktop session**, whose display is software rendered
and paced by the remote client. Its numbers (about 50 frames per second for Slint, with
stutters) say nothing about the toolkits, and are kept apart in `spikes/results/spike2/remote-desktop/`.
The tables below are from the real display.

The way a developed image reaches the toolkit is measured separately (see the next section).

## From the pipeline to the CPU

The toolkits take bytes. What does it cost to get a developed viewport out of the GPU as RGBA8?
GTX 1650 SUPER, panning across a real 45 MP file, median of 200 frames:

| View | GPU develop | Read back | Total | p99 |
| --- | --- | --- | --- | --- |
| 1200x800 (1.0 MP) | 0.95 ms | 0.30 ms | 1.2 ms | 2.2 ms |
| 1872x1064 (2.0 MP) | 1.9 ms | 0.9 ms | 2.8 ms | 4.9 ms |
| 2560x1440 (3.7 MP) | 3.5 ms | 1.5 ms | 4.9 ms | 8.0 ms |

On llvmpipe (software) the same views take 8.2, 16.6 and 30.1 ms. So **zero-copy sharing of the
GPU image with the toolkit is not needed**: reading back costs about 1 ms per megapixel, well
inside a frame. A toolkit that accepts a buffer of RGBA bytes is enough.

## Results

60 frames per second on this display is 16.7 ms per frame.

### The image view

| View | Toolkit | Median | p99 | Worst | Frames over 20 ms |
| --- | --- | --- | --- | --- | --- |
| 1340x964 (1.3 MP) | Slint | 16.7 ms | 21.1 ms | 24 ms | 9 of 481 |
| | iced | 16.7 ms | 19.1 ms | 20 ms | 0 of 479 |
| | **Qt Quick** | 16.7 ms | **16.9 ms** | 18 ms | **0 of 480** |
| 2200x1400 (3.1 MP) | Slint | 16.7 ms | 17.8 ms | 20 ms | 1 of 480 |
| | iced | 16.7 ms | 18.9 ms | **1,000 ms** (once) | 2 of 421 |
| | **Qt Quick** | 16.7 ms | **16.9 ms** | 18 ms | **0 of 480** |

All three hold 60 frames per second. Qt renders on a separate thread and never misses a frame;
Slint and iced are close behind. The one-second stall in iced happened once, at the creation of
the large texture.

### The grid of 100,000 thumbnails, and the whole window

| Mode | Toolkit | p99 | Frames over 20 ms | Memory |
| --- | --- | --- | --- | --- |
| Grid | Slint | 22.0 ms | 32 of 480 | 160 MB |
| | iced | 17.6 ms | 0 of 479 | 222 MB |
| | **Qt Quick** | 16.9 ms | 0 of 480 | 183 MB |
| View above the grid | Slint | 23.9 ms | 23 of 480 | 504 MB |
| | iced | 19.8 ms | 2 of 480 | 479 MB |
| | **Qt Quick** | 16.9 ms | 0 of 480 | 338 MB |

The memory figures include the 60 frames held for the view mode (about 300 MB).
iced has no virtualised list: the grid is virtualised by hand, in about 20 lines.

### Fidelity

| | Slint | iced | Qt Quick |
| --- | --- | --- | --- |
| The bytes sent are the bytes displayed | **yes**: 0 differing pixels out of 480,000 | not measured | **yes**: 0 out of 480,000 |

Both were checked by capturing the window and comparing it pixel by pixel with the frame that
was sent. This tests that the toolkit does not resample, filter or change the colours of an
image displayed 1:1.

**Colour management of the display is not provided by any of the three.** None of them reads the
monitor's profile or applies it. That belongs in the application's own display transform, the
last stage of the pipeline (specification §5.6), which needs the profile from the operating
system (colord on Linux, the Windows colour system, ColorSync on macOS). It is a separate task
for every candidate, and a toolkit must simply not alter pixels, which all pass.

### Lightness

| | Slint | iced | Qt Quick |
| --- | --- | --- | --- |
| First frame after start | 390 to 440 ms | 460 to 730 ms | 480 to 570 ms |
| Memory, grid only | 160 MB | 222 MB | 183 MB |
| Executable, stripped, with the pipeline linked in | 40.5 MB | 30.2 MB | not comparable (Qt is a shared library) |

### The rest of the application

| | Slint | iced | Qt Quick |
| --- | --- | --- | --- |
| Virtualised list of 100,000 items | built in, flat memory | by hand | built in (`GridView`) |
| Tree of 4,400 nodes | flattened list, model by hand | not built | built in (`TreeView`) |
| Text field, typing | works | works | works |
| Accents, dead keys, input method | works (checked by Patrick) | works (checked by Patrick) | not yet checked |
| Translations | gettext, built in, switch at run time | none built in | Qt Linguist, switch at run time |
| **Accessibility** | **works**: a tree of window, labels and list, read through AT-SPI | **none**: no accessibility code in the toolkit, and no application visible to a screen reader | **works in a stock Qt application**: Orca reads Qt Linguist's menus (checked by Patrick); the prototype itself not checked by hand; not visible from the test sandbox (see below) |
| Language of the code | Rust | Rust | C++ and QML |
| Licence | GPL-3.0, or royalty-free, or commercial | MIT | LGPL and GPL |

### Accessibility of Qt: works on the real session, invisible to the test script

With the script that shows a complete tree for Slint, Qt Quick shows only an empty application
node, and so does Qt Linguist, a standard Qt 6 application of the same system. This was retried
with the accessibility bus enabled and with the Orca screen reader running (`orca` running,
`IsEnabled` true, `ScreenReaderEnabled` still false).

What is established: Qt's own log shows its accessibility layer **is active** and builds an
interface for the window (`name="Auroraw spike 2" role=Window`), so the application's items are
described correctly on Qt's side. What is not: its AT-SPI bridge does not publish them on the
bus here. The empty node comes from GTK, which the session loads into every process through
`GTK_MODULES=gail:atk-bridge` (its toolkit is reported as `gtk`). The same happens with a stock Qt
application, so this points at the session or packaging, not at the prototype. Qt's accessibility
on Linux is generally reported to work; this test cannot confirm it. **Patrick then checked by hand: Orca reads the menus of Qt Linguist on his session**, so Qt's
bridge works on the real desktop and the empty tree above is an artefact of the sandbox shell.
The prototype itself has not been checked with Orca.

## Windows, on the same PC

Slint and iced were also run under Windows, on the same GTX 1650 SUPER and a real display at
60 Hz (issue #1, `spikes/results-windows/`). Qt was not built for Windows. Windows had many
updates pending and a busy disk during the run.

| Mode | Toolkit | Median | p99 | Worst | Frames over 20 ms |
| --- | --- | --- | --- | --- | --- |
| View 1340x964 | Slint | 16.6 ms | 18.7 ms | 19 ms | 0 of 480 |
| | iced | 16.7 ms | 20.8 ms | 23 ms | 8 of 479 |
| Grid | Slint | 16.7 ms | 17.4 ms | 20 ms | 0 of 481 |
| | iced | 16.7 ms | 19.4 ms | 28 ms | 4 of 479 |
| View above the grid | Slint | 16.7 ms | 18.3 ms | 22 ms | 1 of 481 |
| | iced | 18.0 ms | **33.1 ms** | 36 ms | **195 of 479** |

- **Slint holds 60 frames per second in every mode on Windows**, more evenly than on Linux, and
  displays the pixels exactly (0 differing pixels out of 480,000, as on Linux). The French and
  English switch works. Once warm, its first frame appears in 135 to 142 ms.
- **iced was uneven in one mode**: 195 of 479 frames late in the view-above-grid mode, where the
  other two modes were fine. It may be the disturbance from Windows (the run was done under
  load), or a real behaviour of iced on DirectX; it was not repeated.
- **The first run of each viewer took 16 and 26 seconds to show its first frame** (iced view,
  Slint view), against about 0.14 to 1.6 s for the others. These were the first launches of
  freshly downloaded programs on a busy disk, so an antivirus scan and the updates are the
  likely cause. It is a reminder that a first launch can be slow on Windows, not a toolkit
  measure.
- The memory figure is not measured on Windows (the benchmark reads it from a Linux-only file).

## Things met on the way

- **Slint**: an image atlas cut with `source-clip` made the memory reach 1.7 GB and the frame
  time jump to 75 ms, independent of the number of items; one small image per thumbnail fixed
  it (160 MB, 8 ms worst). The default translation context is the component name, which
  silently prevents translations from applying until it is turned off. A property was renamed
  (`viewport-y` became `content-y`).
- **Slint and quitting**: `run()` returns only when the window is closed, so the event loop
  cannot be ended by asking it to quit. It cost a hung window on the first real-display run.
- **iced**: it uses wgpu 27, and the pipeline of spike 1 uses wgpu 30, so the two cannot share
  GPU objects. The read-back path does not need to.
- **Qt**: an image provider or a scene graph texture both work; the second was used.

## What it means

1. **Performance does not decide.** Every candidate holds 60 frames per second with a new image
   of 1.3 to 3.1 MP on each frame and a grid of 100,000 items. Qt is the most regular, and Slint
   the least (still within one missed frame in fifty).
2. **Fidelity does not decide either**: the bytes come out as they went in. The display
   transform is application work whatever the toolkit.
3. **What decides is what surrounds the view.** iced lacks accessibility entirely, lists,
   trees and translations. Slint has them, with rough edges. Qt has the most complete set of
   widgets and the best frame regularity, but its accessibility could not be verified here, and
   it brings C++ into a Rust project and a heavier distribution.
4. **The tension with the language.** The core is Rust (D-068). Slint fits it naturally. Qt would
   mean a C++ interface layer, or a Rust binding (cxx-qt) that has not been tried here.

## Decision

**Slint (D-072).** Qt Quick stays as the documented fallback; iced is ruled out.

- Every candidate meets the performance target, so performance did not decide.
- iced has no accessibility, no virtualised list and no translations; it is out.
- Qt is the most regular and the most complete, but it puts C++ in a Rust project, needs its
  libraries shipped on three platforms, and its Rust binding was not measured.
- Slint fits a Rust core (D-068), is light (D-069), holds 60 frames per second in every mode on
  Linux and Windows, displays pixels exactly, exposes an accessibility tree, and has a licence
  compatible with the GPL-3.0.

What to watch, from what was met: an atlas of thumbnails cut with `source-clip` (use one image per
thumbnail), the default translation context, and the render thread (Slint renders on the UI
thread, so heavy work must stay off it). Slint's default renderer on macOS is OpenGL, which macOS
deprecates; its Skia/Metal renderer may be needed there.

## What remains

- [x] **Qt accessibility**: works in a stock Qt application on the real session (Orca reads Qt
  Linguist). Left: checking the prototype itself with Orca.
- [ ] **Qt through Rust.** These measures used C++ and QML. The Rust-side cost of driving Qt
  (cxx-qt) is unmeasured.
- [x] **Windows** (Slint and iced): done, Slint holds 60 frames per second in every mode.
- [ ] **macOS**, and **Qt on Windows and macOS** (not built there).
- [ ] **Colour on a wide-gamut display**, and the display profile from the operating system.
- [ ] **iced pixel fidelity** (not measured).
- [ ] **Slint on Wayland and with high-DPI scaling**, not measured on a real display.
- [x] **A decision**: D-072.

## Running it

```bash
cd spikes
cargo build --release                                       # Slint and iced viewers
./target/release/dumpframes --view 1340x964 --out samples/frames-1340x964.bin   # for Qt
./target/release/viewer-slint --bench view|grid|both|exact|tree [--view WxH] [--secs N]
./target/release/viewer-iced --bench view|grid|both
cd viewer-qt && mkdir build && cd build && cmake .. && make
./viewer-qt --bench view --frames ../../samples
python3 a11y-check.py viewer-slint                          # with accessibility switched on
```

Raw results are in `spikes/results/spike2/local/`.
