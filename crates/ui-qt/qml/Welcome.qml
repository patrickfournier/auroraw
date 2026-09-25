// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

Item {
    id: root
    required property var launcher
    signal newRequested
    signal openRequested
    property alias newButton: newButton
    property alias openButton: openButton

    ColumnLayout {
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(720, parent.width - 64)
        y: 32
        spacing: 14

        Label {
            text: qsTr("Welcome to Auroraw")
            font.pixelSize: 24
            font.bold: true
        }
        Label {
            text: qsTr("Open a workspace, or create a new one.")
            color: Theme.quiet
        }
        Label {
            visible: root.launcher.note !== ""
            text: root.launcher.note
            color: Theme.warning
            wrapMode: Text.Wrap
            Layout.fillWidth: true
        }
        RowLayout {
            spacing: 8
            Button {
                id: newButton
                text: qsTr("New workspace…")
                highlighted: true
                onClicked: root.newRequested()
            }
            Button {
                id: openButton
                text: qsTr("Open a workspace…")
                onClicked: root.openRequested()
            }
        }
        Label {
            visible: root.launcher.knownNames.length === 0
            text: qsTr("No workspace yet.")
            font.bold: true
        }
        Repeater {
            model: root.launcher.knownNames
            delegate: ItemDelegate {
                required property string modelData
                required property int index
                Layout.fillWidth: true
                text: modelData + "\n" + root.launcher.knownPaths[index]
                onClicked: root.launcher.openKnown(index)
            }
        }
    }
}
