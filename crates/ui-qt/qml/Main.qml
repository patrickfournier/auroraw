// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import org.auroraw.ui

ApplicationWindow {
    id: window
    visible: true
    width: 1400
    height: 900
    minimumWidth: 640
    minimumHeight: 420
    color: palette.window
    background: Rectangle { color: window.palette.window }
    // The grey and its accents come from `Theme` (D-094); dialogs and menus inherit this palette.
    palette {
        window: Theme.grey.window
        windowText: Theme.grey.text
        base: Theme.grey.base
        alternateBase: Theme.grey.window
        text: Theme.grey.text
        button: Theme.grey.button
        buttonText: Theme.grey.text
        light: Theme.grey.light
        midlight: Theme.grey.button
        mid: Theme.grey.dark
        dark: Theme.grey.dark
        shadow: "#000000"
        highlight: Theme.accent
        highlightedText: "#ffffff"
        placeholderText: Theme.grey.placeholder
        toolTipBase: Theme.grey.base
        toolTipText: Theme.grey.text
    }
    title: launcher.screen === "workspace" ? launcher.workspaceName + " — Auroraw" : "Auroraw"

    // What the tests reach into (tests/tst_*.qml).
    property alias welcome: welcomeView
    property alias library: libraryView
    property alias newDialog: newDialog
    property alias openDialog: openDialog
    property alias waiting: waiting
    property alias launcher: launcher
    property alias photos: photoGrid

    Launcher { id: launcher }
    PhotoGrid { id: photoGrid }

    // A dialog is open: every command that opens a window waits (the mouse is blocked by the
    // dialog's own modality; shortcuts are not, so the actions below check this).
    // (`guardShortcuts` exists so that the self-test can ask what Qt does by itself.)
    property bool guardShortcuts: true
    property int newCount: 0
    // A native folder dialog is another window: Qt cannot block this one for it, so while one is open
    // a modal popup covers everything (menu bar included) and the commands wait.
    // (`nativeDialogForced` stands for one in the tests: none can open off screen.)
    property bool nativeDialogForced: false
    readonly property bool nativeDialogOpen: nativeDialogForced || openDialog.visible || sourceDialog.visible || newDialog.browsing
    readonly property bool dialogOpen: guardShortcuts && (newDialog.visible || nativeDialogOpen)
    readonly property bool inWorkspace: launcher.screen === "workspace"
    readonly property Item focusItem: window.activeFocusItem

    Component.onCompleted: {
        Theme.fontFamily = launcher.env("AURORAW_FONT")
        Theme.fontSize = parseInt(launcher.env("AURORAW_FONT_SIZE")) || 0
        if (Theme.fontFamily !== "")
            window.font.family = Theme.fontFamily
        if (Theme.fontSize > 0)
            window.font.pointSize = Theme.fontSize
        launcher.start()
    }
    Connections {
        target: launcher
        function onScreenChanged() {
            if (launcher.screen === "workspace")
                photoGrid.load()
        }
    }
    // What the engine reports arrives on the Bus (a singleton, created here at the latest).
    Connections {
        target: Bus
        function onIndexFinished() { photoGrid.load() }
    }

    menuBar: MenuBar {
        Menu {
            title: qsTr("File")
            Action {
                text: qsTr("New workspace…")
                shortcut: StandardKey.New
                enabled: !window.dialogOpen
                onTriggered: {
                    window.newCount++
                    newDialog.openWith()
                }
            }
            Action {
                text: qsTr("Open workspace…")
                shortcut: StandardKey.Open
                enabled: !window.dialogOpen
                onTriggered: openDialog.open()
            }
            Action {
                text: qsTr("Add a source…")
                enabled: window.inWorkspace && !window.dialogOpen
                onTriggered: sourceDialog.open()
            }
            MenuSeparator {}
            Action {
                text: qsTr("Import…")
                shortcut: "Ctrl+I"
                enabled: window.inWorkspace && !window.dialogOpen
            }
            MenuSeparator {}
            Action {
                text: qsTr("Settings…")
                shortcut: "Ctrl+,"
                enabled: !window.dialogOpen
            }
            MenuSeparator {}
            Action {
                text: qsTr("Quit")
                shortcut: StandardKey.Quit
                onTriggered: Qt.quit()
            }
        }
        Menu {
            title: qsTr("Edit")
            // The text field that has the keyboard does the work: Qt Quick's own text editing.
            Action {
                text: qsTr("Undo")
                shortcut: StandardKey.Undo
                enabled: window.focusItem && window.focusItem.canUndo === true
                onTriggered: window.focusItem.undo()
            }
            Action {
                text: qsTr("Redo")
                shortcut: StandardKey.Redo
                enabled: window.focusItem && window.focusItem.canRedo === true
                onTriggered: window.focusItem.redo()
            }
            MenuSeparator {}
            Action {
                text: qsTr("Cut")
                shortcut: StandardKey.Cut
                enabled: window.focusItem && window.focusItem.selectedText !== undefined
                onTriggered: window.focusItem.cut()
            }
            Action {
                text: qsTr("Copy")
                shortcut: StandardKey.Copy
                enabled: window.focusItem && window.focusItem.selectedText !== undefined
                onTriggered: window.focusItem.copy()
            }
            Action {
                text: qsTr("Paste")
                shortcut: StandardKey.Paste
                enabled: window.focusItem && window.focusItem.canPaste === true
                onTriggered: window.focusItem.paste()
            }
            Action {
                text: qsTr("Delete")
                shortcut: StandardKey.Delete
                enabled: window.focusItem && window.focusItem.selectedText !== undefined
                onTriggered: window.focusItem.remove(window.focusItem.selectionStart,
                                                      window.focusItem.selectionEnd)
            }
            MenuSeparator {}
            Action {
                text: qsTr("Select all")
                shortcut: StandardKey.SelectAll
                enabled: window.focusItem && window.focusItem.selectedText !== undefined
                onTriggered: window.focusItem.selectAll()
            }
        }
        Menu {
            title: qsTr("Help")
            Action {
                text: qsTr("About Auroraw")
                shortcut: "F1"
                enabled: !window.dialogOpen
            }
        }
    }

    Welcome {
        id: welcomeView
        anchors.fill: parent
        visible: launcher.screen === "welcome"
        launcher: launcher
        onNewRequested: newDialog.openWith()
        onOpenRequested: openDialog.open()
    }

    Library {
        id: libraryView
        anchors.fill: parent
        launcher: launcher
        visible: window.inWorkspace
        photoGrid: photoGrid
    }

    NewWorkspaceDialog {
        id: newDialog
        launcher: launcher
        hostWindow: window
    }

    Popup {
        id: waiting
        parent: Overlay.overlay
        x: 0
        y: 0
        width: parent ? parent.width : 0
        height: parent ? parent.height : 0
        modal: true
        dim: true
        closePolicy: Popup.NoAutoClose
        visible: window.nativeDialogOpen
        padding: 0
        background: Rectangle { color: "#66000000" }
        contentItem: Label {
            text: qsTr("Choose a folder in the folder dialog…")
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: 16
        }
    }

    FolderDialog {
        id: sourceDialog
        parentWindow: window
        title: qsTr("Add a source")
        onAccepted: libraryView.addSource(selectedFolder.toString().replace(/^file:\/\//, ""))
    }

    // The system's own folder dialog (a Qt Quick one where the platform has none), modal to the window.
    FolderDialog {
        id: openDialog
        parentWindow: window
        title: qsTr("Open a workspace")
        onAccepted: launcher.openPath(selectedFolder.toString().replace(/^file:\/\//, ""))
    }
}
