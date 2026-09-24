// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.spike

// What Qt's own modality gives, with real input events, offscreen. Run on an empty machine
// (SPIKE_HOME empty): the welcome screen is showing.
TestCase {
    id: tc
    name: "Modality"
    when: windowShown

    // (QML files are not types of the module for an outside importer: loaded by URL.)
    property var appComponent: Qt.createComponent("qrc:/qt/qml/org/auroraw/spike/qml/Main.qml")
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
}
