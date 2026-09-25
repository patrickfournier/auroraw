// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// The library grid: a GridView over the Rust model, thumbnails from image://thumbs.
FocusScope {
    id: root
    required property var photoGrid
    required property var launcher

    // The scan of a source added to the workspace: progress and the closing sentence.
    // The rating's star (a literal U+2605: a regression test reads it back, since a compiler that
    // took the source for a legacy code page once turned it into mojibake on Windows).
    readonly property string star: "★"

    property string scanJob: ""
    property real scanProgress: 0
    property string scanStatus: ""

    function addSource(folder) {
        const job = launcher.addSource(folder)
        if (job.indexOf("error:") === 0) {
            scanStatus = qsTr("Cannot add the source: %1").arg(job.substring(6))
            return false
        }
        scanJob = job
        scanProgress = 0
        scanStatus = qsTr("Reading photos…")
        return true
    }

    Connections {
        target: Bus
        function onJobProgress(job, done, total) {
            if (job === root.scanJob && total > 0)
                root.scanProgress = done / total
        }
        function onIndexFinished(job, added, restored, known, failed) {
            if (job === root.scanJob) {
                root.scanProgress = 1
                root.scanStatus = qsTr("Done: %1 added.").arg(added)
            }
        }
    }
    property alias grid: grid

    // The scan of a source added to the workspace: progress and the closing sentence.
    Rectangle {
        id: strip
        anchors.bottom: parent.bottom
        width: parent.width
        height: 28
        z: 1
        color: root.palette.dark
        visible: root.scanStatus !== ""
        Row {
            anchors.verticalCenter: parent.verticalCenter
            x: 8
            spacing: 10
            ProgressBar {
                width: 160
                value: root.scanProgress
                anchors.verticalCenter: parent.verticalCenter
            }
            Label { text: root.scanStatus }
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
            const jumps = {}
            jumps[Qt.Key_PageDown] = "page-down"
            jumps[Qt.Key_PageUp] = "page-up"
            jumps[Qt.Key_Home] = "home"
            jumps[Qt.Key_End] = "end"
            if (event.key >= Qt.Key_0 && event.key <= Qt.Key_5 && currentIndex >= 0) {
                root.photoGrid.setRating(currentIndex, event.key - Qt.Key_0)
            } else if (jumps[event.key] !== undefined) {
                moveTo(root.photoGrid.jump(jumps[event.key], from, columns, visibleRows))
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
                color: root.palette.dark
                border.width: cell.GridView.isCurrentItem ? 2 : 0
                border.color: root.palette.highlight

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
                    text: cell.rating + root.star
                    color: Theme.rating
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
