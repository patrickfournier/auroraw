// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// A menu row that shows its command's shortcut on the right (Qt Quick Controls' own rows do not),
// written the way the platform writes it (Ctrl+N, ⌘N).
MenuItem {
    id: item

    contentItem: RowLayout {
        spacing: 24
        Label {
            Layout.fillWidth: true
            text: item.text
            elide: Text.ElideRight
            color: item.enabled ? item.palette.windowText : item.palette.placeholderText
        }
        Label {
            text: {
                const key = item.action ? item.action.shortcut : undefined
                return typeof key === "number" ? Shortcuts.text(key, "") : Shortcuts.text(-1, key ? String(key) : "")
            }
            color: item.palette.placeholderText
        }
    }
}
