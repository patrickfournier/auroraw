// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// What every suite of the interface needs: the application window made on a machine of its own (a
// folder under AURORAW_TEST_HOME), a few file helpers, and the window put away after each test.
// (QML files are not types of the module for an outside importer: `Main.qml` is loaded by URL.)
TestCase {
    id: tc
    when: windowShown

    property var appComponent: Qt.createComponent("qrc:/qt/qml/org/auroraw/ui/qml/Main.qml")
    property var app: null
    property int machineCount: 0
    property alias files: files

    Files { id: files }

    readonly property string home: files.env("AURORAW_TEST_HOME")

    function machinePath(name) { return home + "/" + name }
    function freshMachine() { machineCount++; return "m" + machineCount }

    // The application on `machine`, once it has started.
    function launch(machine) {
        quit()
        app = createTemporaryObject(appComponent, tc, { machine: machine })
        verify(app, "the window was made")
        wait(300)
        app.requestActivate()
        wait(100)
        return app
    }

    function quit() {
        if (app) {
            app.close()
            app.destroy()
            app = null
            wait(150)
        }
    }

    function cleanup() { quit() }

    // Moves a folder that a workspace has just closed. Windows keeps it until the event loop has
    // destroyed the window's objects, so this retries between turns of the loop.
    function move(from, to) {
        tryVerify(() => files.exists(to) || files.rename(from, to), 10000, "the folder could be moved")
    }

    // Clicks the centre of an item as a person would.
    function click(item) {
        mouseClick(item)
        wait(60)
    }

    // The workspace folder a machine's New workspace dialog proposes for `name`.
    function workspaceFolder(machine, name) { return machinePath(machine) + "/Pictures/Auroraw/" + name }

    // Creates a workspace through the dialog, as a person does.
    function createWorkspace(name) {
        app.newDialog.openWith()
        wait(200)
        app.newDialog.nameField.text = name
        click(app.newDialog.createButton)
        wait(200)
    }
}
