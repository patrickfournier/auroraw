// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls

// A menu as wide as its widest row is when it opens: Qt sizes a menu when its rows are made, before the
// application's typeface or a change of language have reached them, and a row is then cut short.
Menu {
    onAboutToShow: {
        let widest = 0
        for (let i = 0; i < count; i++) {
            const row = itemAt(i)
            if (row)
                widest = Math.max(widest, row.implicitWidth)
        }
        width = widest + leftPadding + rightPadding
    }
}
