// SPDX-License-Identifier: GPL-3.0-or-later
pragma Singleton
import QtQuick

// The look of the application (D-094): a neutral grey on Fusion, no colour cast around a photograph,
// colour only for what is selected, focused or warned about. (Patrick chose this grey among three.)
QtObject {
    readonly property var grey: ({
        window: "#3c3c3c",
        base: "#2f2f2f",
        button: "#4b4b4b",
        light: "#5c5c5c",
        dark: "#262626",
        text: "#e0e0e0",
        placeholder: "#8c8c8c"
    })

    // The colours that mean something.
    readonly property color accent: "#4a8fd0"
    readonly property color rating: "#e0b040"
    readonly property color warning: "#e0b050"
    readonly property color danger: "#e08070"
    readonly property color quiet: "#aaaaaa"

    // The typeface is still being chosen: `AURORAW_FONT` (a family) and `AURORAW_FONT_SIZE` (points)
    // try others without a rebuild. Empty means the system's own.
    property string fontFamily: ""
    property int fontSize: 0
}
