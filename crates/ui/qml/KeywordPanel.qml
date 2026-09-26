// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The keyword panel (spec §5.7, D-098), on the right of the grid: the vocabulary as a tree, each keyword
// with a check that says whether the selected photos carry it none, some or all, and the number of photos
// that have it. A click on the check gives it to the whole selection, or takes it off; the field above types
// ahead (it filters the tree), Enter assigns the best match, and creates the keyword when nothing matches
// (Shift+Enter creates even when something does). Every assignment is one action, one step of the history,
// and so is making a keyword (with the photos that first get it), renaming, moving and deleting one. A keyword is
// moved by dragging it onto another (or onto the strip that appears for the top level), or from its menu.
// It is the first of the panels the inspector will hold (metadata comes with WP10).
Rectangle {
    id: panel
    required property var keywords
    required property var photoGrid
    required property var library
    required property var launcher

    property bool expanded: true
    // New keywords go under this one (an identifier), if a keyword was clicked.
    property string createUnder: ""
    property string createUnderName: ""
    property alias filterField: field
    property alias renameDialog: renameDialog
    property alias tree: tree
    property alias collapseButton: collapseButton
    property alias moveDialog: moveDialog
    property alias deleteDialog: deleteDialog
    property alias topLevelStrip: topLevelStrip
    property alias ghost: ghost
    // A keyword is being dragged (the top-level strip shows).
    property bool dragging: false

    // The width the person dragged the panel to (double-click on the edge gives back the default).
    readonly property int defaultWidth: 280
    readonly property int minimumWidth: 200
    readonly property int maximumWidth: 640
    property int panelWidth: defaultWidth
    // The remembered width is read when the library is shown (the launcher knows its folders by then).
    Connections {
        target: panel.library
        function onVisibleChanged() {
            if (panel.library.visible && !edge.pressed)
                panel.panelWidth = panel.launcher.keywordPanelWidth()
        }
    }
    property alias edge: edge

    Layout.preferredWidth: expanded ? panelWidth : 30
    color: palette.window

    Rectangle {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: edge.containsMouse || edge.pressed ? palette.highlight : palette.dark
    }

    // The edge beside the grid: drag it to widen or narrow the panel.
    MouseArea {
        id: edge
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 6
        visible: panel.expanded
        hoverEnabled: true
        cursorShape: Qt.SizeHorCursor
        property real startX: 0
        property int startWidth: 0
        onPressed: mouse => {
            startX = mapToItem(null, mouse.x, 0).x
            startWidth = panel.panelWidth
        }
        onPositionChanged: mouse => {
            if (pressed) {
                const dx = startX - mapToItem(null, mouse.x, 0).x
                panel.panelWidth = Math.max(panel.minimumWidth, Math.min(panel.maximumWidth, Math.round(startWidth + dx)))
            }
        }
        // The width is remembered once the drag is over, not at every pixel.
        onReleased: panel.launcher.setKeywordPanelWidth(panel.panelWidth)
        onDoubleClicked: {
            panel.panelWidth = panel.defaultWidth
            panel.launcher.setKeywordPanelWidth(panel.panelWidth)
        }
    }

    // Shows the panel and puts the keyboard in its field.
    function open() {
        expanded = true
        field.forceActiveFocus()
        field.selectAll()
    }

    // Gives a keyword to the selection, or takes it off; false when nothing is selected.
    function assign(id, add) {
        if (photoGrid.selectedCount === 0)
            return false
        photoGrid.keywordSelection(id, add)
        return true
    }

    // Enter in the field: the best match is assigned; when nothing matches (or with `forceCreate`), what was
    // typed becomes a keyword, under the one that was clicked if any, and is assigned: making it and giving it
    // to the selection is one step of the history.
    function commit(forceCreate) {
        const typed = field.text.trim()
        if (typed === "")
            return
        let id = ""
        if (!forceCreate) {
            const row = keywords.bestMatch(typed)
            if (row >= 0)
                id = keywords.idAt(row)
        }
        if (id === "")
            id = keywords.findSibling(typed, createUnder)
        if (id !== "") {
            note = ""
            assign(id, true)
        } else {
            const made = photoGrid.selectedCount > 0 ? photoGrid.createKeywordSelection(typed, createUnder)
                                                      : keywords.create(typed, createUnder)
            if (made.indexOf("error:") === 0) {
                note = made.substring(6)
                return
            }
            note = ""
            keywords.refresh()
        }
        field.text = ""
    }

    // The pointer was released: the ghost drops (on the row or the strip under it) and goes.
    function endDrag() {
        ghost.Drag.drop()
        ghost.Drag.active = false
        ghost.visible = false
        dragging = false
    }

    // A keyword was dropped on `parent` (an identifier; empty for the top level).
    function dropOn(id, parent) {
        if (id === parent)
            return
        note = keywords.moveKeyword(id, parent)
    }

    property string note: ""

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: 6
        anchors.topMargin: 6
        anchors.bottomMargin: 6
        spacing: 6
        visible: panel.expanded

        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTr("Keywords")
                font.bold: true
                Layout.fillWidth: true
            }
            ToolButton {
                id: collapseButton
                text: "»"
                focusPolicy: Qt.NoFocus
                Accessible.name: qsTr("Hide the keyword panel")
                onClicked: panel.expanded = false
            }
        }

        TextField {
            id: field
            Layout.fillWidth: true
            placeholderText: qsTr("Find or add a keyword…")
            Accessible.name: qsTr("Find or add a keyword")
            onTextChanged: {
                panel.keywords.setFilter(text)
                panel.note = ""
            }
            Keys.onReturnPressed: event => panel.commit((event.modifiers & Qt.ShiftModifier) !== 0)
            Keys.onEnterPressed: event => panel.commit((event.modifiers & Qt.ShiftModifier) !== 0)
            Keys.onEscapePressed: {
                text = ""
                panel.library.grid.forceActiveFocus()
            }
        }

        // Where a new keyword will go, once a keyword was clicked.
        RowLayout {
            Layout.fillWidth: true
            visible: panel.createUnder !== ""
            Label {
                Layout.fillWidth: true
                text: qsTr("New keywords go under %1").arg(panel.createUnderName)
                color: Theme.quiet
                elide: Text.ElideRight
            }
            ToolButton {
                text: "×"
                focusPolicy: Qt.NoFocus
                Accessible.name: qsTr("New keywords go at the top level")
                onClicked: panel.createUnder = ""
            }
        }

        Label {
            Layout.fillWidth: true
            visible: panel.photoGrid.selectedCount === 0
            text: qsTr("Select photos to give them keywords.")
            color: Theme.quiet
            wrapMode: Text.Wrap
        }
        Label {
            Layout.fillWidth: true
            visible: panel.note !== ""
            text: panel.note
            color: Theme.danger
            wrapMode: Text.Wrap
        }

        ListView {
            id: tree
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: panel.keywords
            ScrollBar.vertical: ScrollBar {}

            delegate: Item {
                id: row
                required property int index
                required property string keywordId
                required property string name
                required property int depth
                required property int photos
                required property int carried
                required property bool hasChildren
                required property bool expanded
                width: ListView.view.width
                height: 28

                // Dropping a keyword here makes it a child of this one (when that is possible).
                Rectangle {
                    anchors.fill: parent
                    visible: rowDrop.containsDrag && rowDrop.allowed
                    color: palette.highlight
                    opacity: 0.35
                    border.color: palette.highlight
                }
                DropArea {
                    id: rowDrop
                    property bool allowed: false
                    anchors.fill: parent
                    keys: ["keyword"]
                    onEntered: drag => allowed = drag.source.keywordId !== row.keywordId
                                              && panel.keywords.canMove(drag.source.keywordId, row.keywordId)
                    onDropped: drop => panel.dropOn(drop.source.keywordId, row.keywordId)
                }

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: row.depth * 14
                    spacing: 2
                    Label {
                        Layout.preferredWidth: 16
                        horizontalAlignment: Text.AlignHCenter
                        text: row.hasChildren ? (row.expanded ? "▾" : "▸") : ""
                        color: Theme.quiet
                        MouseArea {
                            anchors.fill: parent
                            enabled: row.hasChildren
                            onClicked: panel.keywords.toggleExpanded(row.index)
                        }
                    }
                    CheckBox {
                        id: check
                        tristate: true
                        padding: 0
                        focusPolicy: Qt.NoFocus
                        checkState: row.carried === 2 ? Qt.Checked : row.carried === 1 ? Qt.PartiallyChecked : Qt.Unchecked
                        enabled: panel.photoGrid.selectedCount > 0
                        Accessible.name: row.name
                        // The state comes from the selection, not from the click: the click asks for it.
                        nextCheckState: function () { return checkState }
                        onClicked: panel.assign(row.keywordId, row.carried !== 2)
                    }
                    Label {
                        Layout.fillWidth: true
                        text: row.name
                        elide: Text.ElideRight
                        MouseArea {
                            id: nameArea
                            anchors.fill: parent
                            acceptedButtons: Qt.LeftButton | Qt.RightButton
                            drag.target: ghost
                            drag.threshold: 8
                            onPressed: mouse => {
                                if (mouse.button !== Qt.LeftButton)
                                    return
                                const at = mapToItem(panel, mouse.x, mouse.y)
                                ghost.x = at.x - ghost.Drag.hotSpot.x
                                ghost.y = at.y - ghost.Drag.hotSpot.y
                                ghost.keywordId = row.keywordId
                                ghost.label = row.name
                            }
                            drag.onActiveChanged: {
                                if (drag.active) {
                                    ghost.visible = true
                                    ghost.Drag.active = true
                                    panel.dragging = true
                                } else {
                                    // (Not here: the drop can move the keyword and the list then rebuilds its rows,
                                    // this one included.)
                                    panel.endDrag()
                                }
                            }
                            onClicked: mouse => {
                                panel.createUnder = row.keywordId
                                panel.createUnderName = row.name
                                if (mouse.button === Qt.RightButton) {
                                    menu.row = row.index
                                    menu.keywordId = row.keywordId
                                    menu.keywordName = row.name
                                    menu.popup()
                                }
                            }
                        }
                    }
                    Label {
                        text: row.photos
                        color: Theme.quiet
                        Layout.rightMargin: 6
                    }
                }
            }
        }

        // While a keyword is dragged: where to drop it to make it a top-level keyword.
        Rectangle {
            id: topLevelStrip
            Layout.fillWidth: true
            Layout.preferredHeight: panel.dragging ? 30 : 0
            visible: panel.dragging
            color: topLevelDrop.containsDrag ? palette.highlight : palette.base
            border.color: palette.mid
            Label {
                anchors.centerIn: parent
                text: qsTr("Drop here for the top level")
                color: topLevelDrop.containsDrag ? palette.highlightedText : Theme.quiet
            }
            DropArea {
                id: topLevelDrop
                property alias strip: topLevelStrip
                anchors.fill: parent
                keys: ["keyword"]
                onDropped: drop => panel.dropOn(drop.source.keywordId, "")
            }
        }
    }

    // What follows the pointer while a keyword is dragged (it lives here, not in a row, which the list clips).
    Rectangle {
        id: ghost
        property string keywordId: ""
        property alias label: ghostLabel.text
        visible: false
        z: 100
        width: 180
        height: 26
        radius: 3
        color: palette.highlight
        opacity: 0.85
        Drag.keys: ["keyword"]
        Drag.source: ghost
        Drag.hotSpot.x: 12
        Drag.hotSpot.y: height / 2
        Label {
            id: ghostLabel
            anchors.fill: parent
            anchors.leftMargin: 8
            verticalAlignment: Text.AlignVCenter
            color: palette.highlightedText
            elide: Text.ElideRight
        }
    }

    // Collapsed: a strip with the button that brings the panel back.
    ToolButton {
        anchors.top: parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        visible: !panel.expanded
        text: "«"
        focusPolicy: Qt.NoFocus
        Accessible.name: qsTr("Show the keyword panel")
        onClicked: panel.expanded = true
    }

    Menu {
        id: menu
        property int row: -1
        property string keywordId: ""
        property string keywordName: ""
        AppMenuItem {
            text: qsTr("Show the photos with this keyword")
            onTriggered: panel.library.filterKeyword(menu.keywordId, menu.keywordName)
        }
        AppMenuItem {
            text: qsTr("Rename…")
            onTriggered: renameDialog.openFor(menu.row, menu.keywordName)
        }
        MenuSeparator {}
        AppMenuItem {
            text: qsTr("Move to…")
            onTriggered: moveDialog.openFor(menu.keywordId, menu.keywordName)
        }
        AppMenuItem {
            text: qsTr("Move to the top level")
            enabled: panel.keywords.canMove(menu.keywordId, "")
            onTriggered: panel.dropOn(menu.keywordId, "")
        }
        MenuSeparator {}
        AppMenuItem {
            text: qsTr("Delete…")
            onTriggered: deleteDialog.openFor(menu.keywordId)
        }
    }

    AppDialog {
        id: renameDialog
        property int row: -1
        property string error: ""
        property alias nameField: nameField
        preferredWidth: 420
        title: qsTr("Rename the keyword")

        function openFor(keywordRow, name) {
            row = keywordRow
            nameField.text = name
            error = ""
            open()
            nameField.forceActiveFocus()
            nameField.selectAll()
        }

        function tryRename() {
            const reason = panel.keywords.rename(row, nameField.text)
            if (reason === "")
                close()
            else
                error = reason
        }

        contentItem: ColumnLayout {
            spacing: 8
            TextField {
                id: nameField
                Layout.fillWidth: true
                Accessible.name: qsTr("Name")
                onAccepted: renameDialog.tryRename()
            }
            Label {
                Layout.fillWidth: true
                visible: renameDialog.error !== ""
                text: renameDialog.error
                color: Theme.danger
                wrapMode: Text.Wrap
            }
        }

        footer: DialogButtonBox {
            AppButton {
                text: qsTr("Rename")
                highlighted: true
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: renameDialog.tryRename()
            }
            AppButton {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: renameDialog.close()
            }
        }
    }

    AppDialog {
        id: moveDialog
        property string keywordId: ""
        property string keywordName: ""
        property var targets: []
        property alias targetBox: targetBox
        preferredWidth: 460
        title: qsTr("Move the keyword")

        function openFor(id, name) {
            keywordId = id
            keywordName = name
            targets = JSON.parse(panel.keywords.moveTargets(id))
            targetBox.currentIndex = targets.length > 0 ? 0 : -1
            open()
        }

        function tryMove() {
            if (targetBox.currentIndex < 0)
                return
            panel.note = panel.keywords.moveKeyword(keywordId, targets[targetBox.currentIndex].id)
            close()
        }

        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: moveDialog.targets.length > 0 ? qsTr("Move “%1” under:").arg(moveDialog.keywordName)
                                                    : qsTr("There is nowhere to move “%1”.").arg(moveDialog.keywordName)
                wrapMode: Text.Wrap
            }
            ComboBox {
                id: targetBox
                Layout.fillWidth: true
                model: moveDialog.targets
                textRole: "path"
                enabled: moveDialog.targets.length > 0
                Accessible.name: qsTr("New parent")
            }
        }

        footer: DialogButtonBox {
            AppButton {
                text: qsTr("Move")
                highlighted: true
                enabled: targetBox.currentIndex >= 0
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: moveDialog.tryMove()
            }
            AppButton {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: moveDialog.close()
            }
        }
    }

    // Deleting takes the keyword and its branch off every photo: the numbers are said, and it can be undone.
    AppDialog {
        id: deleteDialog
        property string keywordId: ""
        property string keywordName: ""
        property int branchKeywords: 1
        property int branchPhotos: 0
        preferredWidth: 480
        title: qsTr("Delete the keyword")

        function openFor(id) {
            const info = JSON.parse(panel.keywords.branch(id))
            keywordId = id
            keywordName = info.name
            branchKeywords = info.keywords
            branchPhotos = info.photos
            open()
        }

        function confirm() {
            panel.note = panel.keywords.remove(keywordId)
            close()
        }

        contentItem: ColumnLayout {
            spacing: 8
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: deleteDialog.branchKeywords > 1
                      ? qsTr("Delete “%1” and the %n keyword(s) under it?", "", deleteDialog.branchKeywords - 1).arg(deleteDialog.keywordName)
                      : qsTr("Delete “%1”?").arg(deleteDialog.keywordName)
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: deleteDialog.branchPhotos > 0
                      ? qsTr("%n photo(s) will lose it. You can undo this.", "", deleteDialog.branchPhotos)
                      : qsTr("No photo has it. You can undo this.")
                color: Theme.quiet
            }
        }

        footer: DialogButtonBox {
            AppButton {
                text: qsTr("Delete")
                highlighted: true
                DialogButtonBox.buttonRole: DialogButtonBox.ActionRole
                onClicked: deleteDialog.confirm()
            }
            AppButton {
                text: qsTr("Cancel")
                DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
                onClicked: deleteDialog.close()
            }
        }
    }
}
