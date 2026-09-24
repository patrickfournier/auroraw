// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import org.auroraw.spike

ApplicationWindow {
    id: window
    visible: true
    width: 1400
    height: 900
    minimumWidth: 640
    minimumHeight: 420
    color: "#1e1e1e"
    palette.windowText: "#dddddd"
    palette.text: "#dddddd"
    title: launcher.screen === "workspace" ? launcher.workspaceName + " — Auroraw" : "Auroraw"

    // What the tests reach into (tests/tst_*.qml).
    property alias welcome: welcomeView
    property alias library: libraryView
    property alias newDialog: newDialog
    property alias launcher: launcher
    property alias photos: photoGrid

    Launcher { id: launcher }
    PhotoGrid { id: photoGrid }

    // A dialog is open: every command that opens a window waits (the mouse is blocked by the
    // dialog's own modality; shortcuts are not, so the actions below check this).
    // (`guardShortcuts` exists so that the self-test can ask what Qt does by itself.)
    property bool guardShortcuts: true
    property int newCount: 0
    readonly property bool dialogOpen: guardShortcuts && (newDialog.visible || openDialog.visible)
    readonly property bool inWorkspace: launcher.screen === "workspace"
    readonly property Item focusItem: window.activeFocusItem

    // Self-driving for the offscreen runs: `--scenario=<name>` and `--snapshot=<png>` (no display is used).
    function argument(name) {
        const args = Qt.application.arguments
        for (let i = 0; i < args.length; i++) {
            if (args[i].indexOf(name + "=") === 0)
                return args[i].substring(name.length + 1)
        }
        return ""
    }
    readonly property string scenario: argument("--scenario")
    readonly property string snapshotPath: argument("--snapshot")

    Timer {
        interval: 500
        running: window.scenario !== ""
        onTriggered: {
            if (window.scenario === "dialog")
                newDialog.openWith()
        }
    }
    // The attached Overlay.overlay is only reachable from an Item.
    Item {
        id: probe
        readonly property Item overlayRoot: Overlay.overlay
    }
    Timer {
        interval: 1800
        running: window.snapshotPath !== ""
        onTriggered: {
            const ok = probe.overlayRoot.parent.grabToImage(function (result) {
                console.log("snapshot saved:", result.saveToFile(window.snapshotPath))
                Qt.quit()
            })
            console.log("grab started:", ok)
        }
    }

    Component.onCompleted: launcher.start()
    Connections {
        target: launcher
        function onScreenChanged() {
            if (launcher.screen === "workspace")
                photoGrid.load()
        }
        function onScanFinished() { photoGrid.load() }
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
    }

    FolderDialog {
        id: sourceDialog
        title: qsTr("Add a source")
        onAccepted: launcher.addSource(selectedFolder.toString().replace(/^file:\/\//, ""))
    }

    // The system's own folder dialog (a Qt Quick one where the platform has none), modal to the window.
    FolderDialog {
        id: openDialog
        title: qsTr("Open a workspace")
        onAccepted: launcher.openKnown(-1)
    }
}
