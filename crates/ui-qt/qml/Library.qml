// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The library grid (the cull task): a filter bar with how many photos it lists, the photos as
// thumbnails from `image://thumbs`, and a strip that describes the selected one. The selection is the
// `GridView`'s current index; it stays on its photo when the list is read again.
FocusScope {
    id: root
    required property var photoGrid

    // The rating's star (a literal U+2605: a regression test reads it back, since a compiler that
    // took the source for a legacy code page once turned it into mojibake on Windows).
    readonly property string star: "★"

    property alias grid: grid
    property alias filterBar: filterBar
    property alias filterButtons: filterButtons
    // What the strip under the grid says about the selected photo.
    property string summary: ""
    readonly property string status: qsTr("%n photo(s)", "", photoGrid.count)

    // Selects a photo (-1 for none) and brings it into view.
    function select(index) {
        grid.currentIndex = index
        if (index >= 0)
            grid.positionViewAtIndex(index, GridView.Contain)
        updateSummary()
    }

    function updateSummary() {
        summary = grid.currentIndex >= 0 ? photoGrid.summaryAt(grid.currentIndex) : ""
    }

    // Reads the list again (photos arrived): the selection stays on its photo if it is still
    // listed, and the view where it was.
    function reload() {
        const selected = grid.currentIndex >= 0 ? photoGrid.idAt(grid.currentIndex) : ""
        const scrolled = grid.contentY
        photoGrid.load()
        grid.currentIndex = selected !== "" ? photoGrid.rowOf(selected) : -1
        grid.contentY = scrolled
        grid.returnToBounds()
        updateSummary()
    }

    // Lists the photos rated `minRating` or more; nothing is selected any more.
    function filterBy(minRating) {
        photoGrid.filterBy(minRating)
        grid.currentIndex = -1
        grid.contentY = 0
        updateSummary()
    }

    // The engine says a photo changed: its cell and, when selected, the strip follow.
    function photoChanged(photoId) {
        photoGrid.refreshPhoto(photoId)
        if (grid.currentIndex >= 0 && photoGrid.idAt(grid.currentIndex) === photoId)
            updateSummary()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            id: filterBar
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            color: root.palette.window
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 6
                anchors.rightMargin: 6
                spacing: 6
                Repeater {
                    id: filterButtons
                    model: [
                        { min: 0, name: qsTr("All") },
                        { min: 1, name: qsTr("1+") },
                        { min: 2, name: qsTr("2+") },
                        { min: 3, name: qsTr("3+") },
                        { min: 4, name: qsTr("4+") },
                        { min: 5, name: qsTr("5") }
                    ]
                    Button {
                        required property var modelData
                        text: modelData.name
                        highlighted: root.photoGrid.minRating === modelData.min
                        focusPolicy: Qt.NoFocus
                        onClicked: {
                            root.filterBy(modelData.min)
                            grid.forceActiveFocus()
                        }
                    }
                }
                Label {
                    text: root.status
                    color: Theme.quiet
                    Layout.leftMargin: 6
                }
                Item { Layout.fillWidth: true }
            }
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            focus: true
            model: root.photoGrid
            cellWidth: 164
            cellHeight: 124
            currentIndex: -1
            // The arrows are ours, so that a step down from above a short last row lands on its
            // last photo (the Slint shell's rule, `gridmath::step`).
            keyNavigationEnabled: false
            readonly property int columns: Math.max(1, Math.floor(width / cellWidth))
            readonly property int visibleRows: Math.max(1, Math.floor(height / cellHeight))

            ScrollBar.vertical: ScrollBar {}

            // The window was resized and the rows re-flowed: the selection stays in view (once the
            // view has laid its cells out again).
            onColumnsChanged: Qt.callLater(keepSelectionInView)

            function keepSelectionInView() {
                if (currentIndex >= 0)
                    positionViewAtIndex(currentIndex, GridView.Contain)
            }

            function move(dx, dy) {
                if (count === 0)
                    return
                // With nothing selected, any move selects the first photo.
                root.select(currentIndex < 0 ? 0 : root.photoGrid.step(currentIndex, dx, dy, columns))
            }

            function jump(kind) {
                if (count === 0)
                    return
                root.select(root.photoGrid.jump(kind, Math.max(currentIndex, 0), columns, visibleRows))
            }

            Keys.onPressed: event => {
                if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier))
                    return
                if (event.key >= Qt.Key_0 && event.key <= Qt.Key_5) {
                    if (currentIndex >= 0) {
                        root.photoGrid.setRating(currentIndex, event.key - Qt.Key_0)
                        root.updateSummary()
                    }
                } else if (event.key === Qt.Key_Left) {
                    move(-1, 0)
                } else if (event.key === Qt.Key_Right) {
                    move(1, 0)
                } else if (event.key === Qt.Key_Up) {
                    move(0, -1)
                } else if (event.key === Qt.Key_Down) {
                    move(0, 1)
                } else if (event.key === Qt.Key_PageUp) {
                    jump("page-up")
                } else if (event.key === Qt.Key_PageDown) {
                    jump("page-down")
                } else if (event.key === Qt.Key_Home) {
                    jump("home")
                } else if (event.key === Qt.Key_End) {
                    jump("end")
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
                // No thumbnail can be made for this photo (it says so instead of staying empty).
                readonly property bool unavailable: thumbnail.status === Image.Error
                readonly property bool shown: thumbnail.status === Image.Ready
                width: grid.cellWidth
                height: grid.cellHeight
                Accessible.role: Accessible.ListItem
                Accessible.name: cell.rating > 0 ? qsTr("Photo, %n star(s)", "", cell.rating) : qsTr("Photo")

                Rectangle {
                    x: 4
                    width: 160
                    height: 120
                    color: root.palette.dark

                    Image {
                        id: thumbnail
                        anchors.fill: parent
                        source: "image://thumbs/" + cell.photoId
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                    }
                    // A photo no thumbnail can be made for (an unreadable file, a RAW without a preview).
                    Label {
                        anchors.centerIn: parent
                        visible: cell.unavailable
                        text: qsTr("No preview")
                        color: Theme.grey.placeholder
                    }
                    // The rating, on a dark chip so that it reads over any picture.
                    Rectangle {
                        x: 4
                        y: 4
                        visible: cell.rating > 0
                        width: stars.implicitWidth + 8
                        height: stars.implicitHeight + 2
                        radius: 3
                        color: "#a0000000"
                        Text {
                            id: stars
                            anchors.centerIn: parent
                            text: cell.rating + root.star
                            color: Theme.rating
                        }
                    }
                    // The selection's frame, over the picture.
                    Rectangle {
                        anchors.fill: parent
                        color: "transparent"
                        border.width: cell.GridView.isCurrentItem ? 2 : 0
                        border.color: root.palette.highlight
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: {
                            root.select(cell.index)
                            grid.forceActiveFocus()
                        }
                    }
                }
            }
        }

        // The selected photo: a minimal metadata strip (the panels come with a later work package).
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 28
            color: root.palette.window
            Label {
                x: 6
                anchors.verticalCenter: parent.verticalCenter
                text: root.summary
                color: Theme.quiet
                elide: Text.ElideRight
                width: parent.width - 12
            }
        }
    }
}
