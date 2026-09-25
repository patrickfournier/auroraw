// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The catalogue task: where sources are added and removed. (A placeholder until milestone Q4 of the
// port; a new workspace opens on it, because it is where a workspace gets its photos.)
Item {
    ColumnLayout {
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(720, parent.width - 64)
        y: 32
        spacing: 14
        Label {
            text: qsTr("Sources")
            font.pixelSize: 24
            font.bold: true
        }
        Label {
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.quiet
            text: qsTr("A source is a folder whose photos are in the catalogue.")
        }
    }
}
