// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls

// A modal dialog centred on the window, as wide as it asks up to the window's width. Every dialog of
// the application is one, so that they look and behave alike (Escape closes it).
Dialog {
    property int preferredWidth: 640

    modal: true
    anchors.centerIn: Overlay.overlay
    width: Math.min(preferredWidth, (parent ? parent.width : preferredWidth) - 32)
    closePolicy: Popup.CloseOnEscape
}
