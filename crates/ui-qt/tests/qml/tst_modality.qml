// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// What Qt's own modality gives, with real input events, offscreen. Run on an empty machine
// (an empty machine): the welcome screen is showing.
TestCase {
    id: tc
    name: "Modality"
    when: windowShown

    // (QML files are not types of the module for an outside importer: loaded by URL.)
    property var appComponent: Qt.createComponent("qrc:/qt/qml/org/auroraw/ui/qml/Main.qml")
    property var app

    function init() {
        app = createTemporaryObject(appComponent, tc)
        verify(app)
        wait(400)
        app.requestActivate()
        wait(100)
    }

    function cleanup() {
        if (app) {
            app.close()
            app.destroy()
        }
    }

    function test_a_click_behind_a_modal_dialog_does_nothing() {
        app.newDialog.openWith()
        wait(200)
        verify(app.newDialog.visible)
        mouseClick(app.welcome.newButton)
        wait(100)
        compare(app.newCount, 0, "the New button behind the dialog was not reached")
        app.newDialog.close()
        wait(200)
        mouseClick(app.welcome.newButton)
        wait(200)
        verify(app.newDialog.visible, "control: with no dialog the same click opens it")
    }

    function test_the_menu_bar_is_blocked_by_a_modal_dialog() {
        app.newDialog.openWith()
        wait(200)
        const fileMenu = app.menuBar.menuAt(0)
        mouseClick(app.menuBar.itemAt(0))
        wait(200)
        verify(!fileMenu.visible, "the menu bar did not open under a modal dialog")
        app.newDialog.close()
        wait(200)
        mouseClick(app.menuBar.itemAt(0))
        wait(200)
        verify(fileMenu.visible, "control: with no dialog the menu bar opens")
        fileMenu.close()
    }

    function test_qt_itself_lets_a_shortcut_through_a_modal_dialog() {
        app.guardShortcuts = false
        app.newDialog.openWith()
        wait(200)
        const before = app.newCount
        keyClick(Qt.Key_N, Qt.ControlModifier)
        wait(200)
        compare(app.newCount, before + 1, "the shortcut fired under the modal dialog")
    }

    function test_the_guard_stops_that_shortcut() {
        app.newDialog.openWith()
        wait(200)
        const before = app.newCount
        keyClick(Qt.Key_N, Qt.ControlModifier)
        wait(200)
        compare(app.newCount, before, "with the guard nothing happens")
    }

    function test_escape_closes_the_dialog() {
        app.newDialog.openWith()
        wait(200)
        verify(app.dialogOpen)
        keyClick(Qt.Key_Escape)
        wait(200)
        verify(!app.newDialog.visible)
        verify(!app.dialogOpen)
    }

    function test_the_edit_menu_acts_on_the_focused_field() {
        app.newDialog.openWith()
        wait(200)
        const field = app.activeFocusItem
        verify(field.selectedText !== undefined, "the name field has the keyboard")
        field.selectAll()
        verify(field.selectedText.length > 0)
        const cut = app.menuBar.menuAt(1).actionAt(3)
        compare(cut.text, "Cut")
        verify(cut.enabled, "Cut is enabled on a selection")
        cut.trigger()
        compare(field.text, "")
        const undo = app.menuBar.menuAt(1).actionAt(0)
        verify(undo.enabled)
        undo.trigger()
        verify(field.text.length > 0, "Undo brought it back")
    }

    // A native folder dialog is another window and Qt does not block ours for it: a modal popup
    // covers the window (menu bar included) while one is open. (None can open off screen: `nativeDialogForced` stands for one.)
    function test_a_folder_dialog_blocks_the_window_behind_it() {
        app.nativeDialogForced = true
        wait(300)
        verify(app.nativeDialogOpen, "the window knows a folder dialog is open")
        verify(app.waiting.visible, "and covers itself")
        const fileMenu = app.menuBar.menuAt(0)
        mouseClick(app.menuBar.itemAt(0))
        wait(150)
        verify(!fileMenu.visible, "the menu bar did not open under the folder dialog")
        app.nativeDialogForced = false
        wait(300)
        verify(!app.waiting.visible, "once the dialog is closed the window is usable again")
    }
}
