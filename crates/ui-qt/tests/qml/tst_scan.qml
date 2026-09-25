// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The engine's events reach the grid: a folder added as a source is scanned in the background,
// progress and the closing sentence arrive as property changes, the photos get their thumbnails as
// they come in (before any grid shows them), and the grid reads the list again keeping the selection.
// The fixture is 20 photos; AURORAW_TEST_EXTRA is a folder of 5 more.
AppTestCase {
    name: "Scan"

    function test_adding_a_source_reports_progress_thumbnails_and_reloads_the_grid() {
        launch("")
        tryCompare(app.photos, "count", 20)
        const grid = app.library.grid
        mouseClick(grid.itemAtIndex(4))
        wait(60)
        compare(grid.currentIndex, 4)
        const selected = app.photos.idAt(4)

        // The catalogue task is on screen throughout: nothing but the scan asks for the thumbnails.
        app.currentTask = "catalogue"
        const before = files.previewsCount(home + "/cache")
        verify(before >= 0, "the workspace has a previews database")
        addSource(app.launcher.env("AURORAW_TEST_EXTRA"))
        waitForTheScan()
        verify(app.flow.status.indexOf("Done: 5 added") === 0, app.flow.status)
        tryVerify(() => files.previewsCount(home + "/cache") === before + 5, 20000,
                  "the new photos' thumbnails were made as they were scanned")
        compare(app.currentTask, "catalogue")

        // Back on the grid: the new photos are listed, and the selection stayed on its photo.
        app.currentTask = "cull"
        tryCompare(app.photos, "count", 25)
        compare(app.photos.idAt(grid.currentIndex), selected)
    }
}
