// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.spike

// The grid's keyboard and layout, offscreen, on the fixture workspace (SPIKE_HOME made by
// `--make-fixture`): the last workspace opens on the grid.
TestCase {
    id: tc
    name: "Grid"
    when: windowShown

    // (QML files are not types of the module for an outside importer: loaded by URL.)
    property var appComponent: Qt.createComponent("qrc:/qt/qml/org/auroraw/spike/qml/Main.qml")
    property var app

    function init() {
        app = createTemporaryObject(appComponent, tc)
        verify(app)
        wait(500)
        app.requestActivate()
        wait(100)
    }

    function cleanup() {
        if (app) {
            app.close()
            app.destroy()
        }
    }

    function test_paging_keys_home_and_end_move_the_selection_and_keep_it_in_view() {
        const grid = app.library.grid
        verify(grid.count >= 60, "the fixture's photos are listed")
        grid.currentIndex = 1
        grid.forceActiveFocus()
        const page = grid.visibleRows * grid.columns
        keyClick(Qt.Key_PageDown)
        compare(grid.currentIndex, 1 + page)
        keyClick(Qt.Key_End)
        compare(grid.currentIndex, grid.count - 1)
        verify(grid.contentY > 0, "the last row was scrolled into view")
        keyClick(Qt.Key_PageUp)
        compare(grid.currentIndex, grid.count - 1 - page)
        keyClick(Qt.Key_Home)
        compare(grid.currentIndex, 0)
        compare(grid.contentY, 0)
        keyClick(Qt.Key_Right)
        compare(grid.currentIndex, 1, "arrows are the GridView's own")
        keyClick(Qt.Key_Down)
        compare(grid.currentIndex, 1 + grid.columns)
    }

    function test_a_rating_key_reaches_the_model_and_the_cell() {
        const grid = app.library.grid
        grid.currentIndex = 2
        grid.forceActiveFocus()
        keyClick(Qt.Key_4)
        wait(200)
        compare(grid.itemAtIndex(2).rating, 4)
    }

    function test_a_resized_window_reflows_the_columns() {
        const grid = app.library.grid
        const before = grid.columns
        app.width = 1100
        wait(300)
        compare(grid.columns, 6)
        verify(grid.columns < before)
    }

    // The engine's events reach the interface: a folder added as a source is scanned in the
    // background, progress and the closing sentence arrive as property changes, the grid reloads.
    function test_adding_a_source_reports_progress_and_reloads_the_grid() {
        const before = app.photos.count
        // SPIKE_EXTRA: a folder of 5 more photos, next to the fixture's own source.
        const reason = app.launcher.addSource(app.launcher.env("SPIKE_EXTRA"))
        compare(reason, "")
        tryVerify(function () { return app.launcher.scanStatus.indexOf("Done") === 0 }, 20000)
        compare(app.launcher.scanProgress, 1.0)
        tryVerify(function () { return app.photos.count === before + 5 }, 5000)
    }
}
