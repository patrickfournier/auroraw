// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The keyword panel (spec §5.7, D-098), on the right of the grid: the vocabulary as a tree, each keyword
// with a check that says whether the selected photos carry it none, some or all, and the number of photos
// that have it. A click on the check gives it to the whole selection, or takes it off; the field above types
// ahead (it filters the tree), Enter assigns the best match, and creates the keyword when nothing matches
// (Shift+Enter creates even when something does). Every assignment is one action, one step of the history.
// It is the first of the panels the inspector will hold (metadata comes with WP10).
Rectangle {
    id: panel
    required property var keywords
    required property var photoGrid
    required property var library

    property bool expanded: true
    // New keywords go under this one (an identifier), if a keyword was clicked.
    property string createUnder: ""
    property string createUnderName: ""
    property alias filterField: field
    property alias renameDialog: renameDialog
    property alias tree: tree
    property alias collapseButton: collapseButton

    Layout.preferredWidth: expanded ? 280 : 30
    color: palette.window

    Rectangle {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: palette.dark
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
    // typed becomes a keyword, under the one that was clicked if any, and is assigned.
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
        if (id === "") {
            id = keywords.create(typed, createUnder)
            if (id.indexOf("error:") === 0) {
                note = id.substring(6)
                return
            }
        }
        note = ""
        assign(id, true)
        field.text = ""
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
                            anchors.fill: parent
                            acceptedButtons: Qt.LeftButton | Qt.RightButton
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
}
