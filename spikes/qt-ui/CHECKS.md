# What to look at on a real machine (Linux and Windows)

This is the check kit of milestone Q0 (D-094). The spike runs offscreen in CI; nothing there shows how
it looks and behaves on a screen. Run it, look at the points below, and tell what you see, in your own
words: no need to match the wording.

## Run it

**Linux** (your session, Qt 6.4.2 of the system, the QML module packages installed):

```sh
cd spikes/qt-ui
QMAKE=/usr/bin/qmake6 cargo build --release
export SPIKE_HOME=$HOME/qt-spike-home        # a throwaway machine: nothing else is touched
SPIKE_PHOTOS=2000 target/release/qt-ui-spike --make-fixture
SPIKE_THEME=neutral target/release/qt-ui-spike
echo "$XDG_SESSION_TYPE"                      # x11 or wayland: please tell which
```

Then again with `SPIKE_THEME=graphite` and `SPIKE_THEME=mid` (three neutral greys to compare), and once with
`QT_QPA_PLATFORMTHEME=xdgdesktopportal` in front to see whether the folder dialog is the desktop's.

**Windows**: download the artifact `qt-ui-spike-windows` of the latest run of the workflow "Spike Qt Quick"
(GitHub, Actions), unzip it, and in PowerShell inside the folder:

```powershell
$env:SPIKE_HOME = "$env:USERPROFILE\qt-spike-home"
$env:SPIKE_PHOTOS = "2000"
.\qt-ui-spike.exe --make-fixture
$env:SPIKE_THEME = "neutral"     # graphite | neutral | mid
.\qt-ui-spike.exe
```

Delete `qt-spike-home` when done. The first launch opens the workspace it just made (a grid of 2,000
generated, very colourful pictures: they are test patterns, not photographs).

## What to look at

1. **The folder dialog.** File, Open workspace… and, in File, New workspace…, Browse…: is it the system's
   own dialog or a Qt one? Does it stay above the window, and can the window behind it be clicked?
2. **The grey.** Which of `graphite`, `neutral` and `mid` do you prefer, and what is wrong with the
   others? Anything hard to read (labels, disabled items, the selection, focus rings)? Are the blue
   accent on the selection and the gold rating star too much, or right?
3. **Scale and fonts.** Is the text the right size? Blurry? Do the cells and the buttons look the size
   you expect on your screen (and, if you have a second screen of another density, on it too)?
4. **Typing accents.** In the New workspace fields: é, è, à, ç, ô, dead keys, Compose sequences, an
   emoji or a non-Latin script if your input method has one. Copy, cut and paste with the keyboard and
   with Edit.
5. **Menus.** Alt+F and the menu bar, Ctrl+N, Ctrl+O, Ctrl+Z; do the shortcuts work, and do the
   labels show them?
6. **The window.** Resize it down to the minimum and up to full screen: does the grid re-flow, does
   anything overlap or stay cut?
7. **Scrolling.** With 2,000 pictures: drag the scroll bar, use the wheel, hold Page Down and End. Does
   it stay smooth and does the picture fill in quickly? Roughly how many seconds to fill a screen?
8. **Anything odd.** A crash, a freeze, a warning printed in the terminal.

Please do not run anything else than what is above on your display while I am not with you; I only read
what you report.
