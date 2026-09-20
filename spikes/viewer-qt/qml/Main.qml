import QtQuick
import QtQuick.Controls
import Spike 1.0

ApplicationWindow {
    id: win
    visible: true
    width: winW
    height: winH
    title: qsTr("Auroraw spike 2")
    color: "#1e1e1e"

    function caption() { return qsTr("Keywords") }

    // Sidebar: the keyword tree.
    Rectangle {
        x: 0; y: 0; width: 260; height: win.height; color: "#262626"
        Text { x: 8; y: 6; text: qsTr("Keywords"); color: "#dddddd"; font.bold: true }
        TreeView {
            x: 0; y: 30; width: 260; height: parent.height - 30
            clip: true
            model: treeModel
            delegate: Item {
                required property TreeView treeView
                required property bool isTreeNode
                required property bool expanded
                required property int hasChildren
                required property int depth
                required property int row
                required property string display
                implicitWidth: 260
                implicitHeight: 22
                Text {
                    x: 8 + depth * 14
                    anchors.verticalCenter: parent.verticalCenter
                    text: (hasChildren ? (expanded ? "▾ " : "▸ ") : "   ") + display
                    color: "#cccccc"
                }
                TapHandler { onTapped: treeView.toggleExpanded(row) }
            }
        }
    }

    // Top bar.
    Rectangle {
        x: 260; y: 0; width: win.width - 260; height: 36; color: "#2b2b2b"
        TextField { objectName: "search"; x: 6; y: 4; width: 320; height: 28; placeholderText: qsTr("Search") }
        Button { x: 340; y: 4; height: 28; text: qsTr("Switch language"); onClicked: bench.toggleLanguage() }
    }

    // The image view.
    ViewportItem {
        objectName: "viewport"
        visible: showView
        x: 260; y: 36; width: vw; height: vh
    }

    // The grid of thumbnails, virtualised by GridView.
    GridView {
        objectName: "grid"
        visible: showGrid
        x: 260
        y: 36 + (showView ? vh : 0)
        width: win.width - 260
        height: win.height - y
        cellWidth: 164; cellHeight: 124
        clip: true
        model: gridItems
        delegate: Item {
            width: 164; height: 124
            Image {
                width: 160; height: 120
                source: "image://thumbs/" + ((index * 2654435761) % 256)
                asynchronous: false
                cache: true
            }
            Text { x: 6; y: 4; text: (index % 6) > 0 ? (index % 6) + "★" : ""; color: "#ffd24a" }
        }
    }
}
