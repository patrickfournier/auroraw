// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls

// The library grid: a GridView over the Rust model, thumbnails from image://thumbs.
FocusScope {
    id: root
    required property var photoGrid
    required property var launcher
    property alias grid: grid

    // The scan of a source added to the workspace: progress and the closing sentence.
    Rectangle {
        id: strip
        anchors.bottom: parent.bottom
        width: parent.width
        height: 28
        z: 1
        color: "#262626"
        visible: root.launcher.scanStatus !== ""
        Row {
            anchors.verticalCenter: parent.verticalCenter
            x: 8
            spacing: 10
            ProgressBar {
                width: 160
                value: root.launcher.scanProgress
                anchors.verticalCenter: parent.verticalCenter
            }
            Label { text: root.launcher.scanStatus; color: "#cccccc" }
        }
    }

    GridView {
        id: grid
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: strip.visible ? strip.top : parent.bottom
        clip: true
        focus: true
        model: root.photoGrid
        cellWidth: 164
        cellHeight: 124
        currentIndex: -1
        readonly property int columns: Math.max(1, Math.floor(width / cellWidth))
        readonly property int visibleRows: Math.max(1, Math.floor(height / cellHeight))

        ScrollBar.vertical: ScrollBar {}

        function moveTo(index) {
            if (count === 0)
                return
            currentIndex = Math.max(0, Math.min(count - 1, index))
            positionViewAtIndex(currentIndex, GridView.Contain)
        }

        // Arrows are the GridView's own; the rest is the same as the Slint shell's key handling.
        Keys.onPressed: event => {
            const from = Math.max(currentIndex, 0)
            if (event.key >= Qt.Key_0 && event.key <= Qt.Key_5 && currentIndex >= 0) {
                root.photoGrid.setRating(currentIndex, event.key - Qt.Key_0)
            } else if (event.key === Qt.Key_PageDown) {
                moveTo(from + visibleRows * columns)
            } else if (event.key === Qt.Key_PageUp) {
                moveTo(from - visibleRows * columns)
            } else if (event.key === Qt.Key_Home) {
                moveTo(0)
            } else if (event.key === Qt.Key_End) {
                moveTo(count - 1)
            } else {
                return
            }
            event.accepted = true
        }

        delegate: Item {
            id: cell
            required property int index
            required property string photoId
            required property int rating
            width: grid.cellWidth
            height: grid.cellHeight

            Rectangle {
                x: 4
                width: 160
                height: 120
                color: "#333333"
                border.width: cell.GridView.isCurrentItem ? 2 : 0
                border.color: "#ffd24a"

                Image {
                    anchors.fill: parent
                    source: "image://thumbs/" + cell.photoId
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                }
                Text {
                    x: 6
                    y: 4
                    visible: cell.rating > 0
                    text: cell.rating + "★"
                    color: "#ffd24a"
                }
                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        grid.currentIndex = cell.index
                        grid.forceActiveFocus()
                    }
                }
            }
        }
    }
}
