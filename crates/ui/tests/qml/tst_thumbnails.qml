// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// Thumbnails come from `image://thumbs` without a grid waiting for them one by one, and a photo no
// thumbnail can be made for (its file is gone) says so and stays quiet about it. The harness removed
// one of the fixture's 12 files.
AppTestCase {
    name: "Thumbnails"

    function cells() {
        const found = []
        for (let i = 0; i < app.library.grid.count; i++) {
            const item = app.library.grid.itemAtIndex(i)
            if (item)
                found.push(item)
        }
        return found
    }

    function test_the_photos_show_their_thumbnails_and_the_missing_one_says_so() {
        launch("")
        tryCompare(app.photos, "count", 12)
        tryVerify(() => cells().filter(c => c.shown).length === 11, 20000, "eleven thumbnails arrived")
        tryVerify(() => cells().filter(c => c.unavailable).length === 1, 20000, "one photo has none")
        // Scrolling and reloading do not bring the failure back as work, or change what is shown.
        app.library.reload()
        wait(300)
        compare(cells().filter(c => c.shown).length, 11)
        compare(cells().filter(c => c.unavailable).length, 1)
        // The strip still describes it.
        mouseClick(cells().filter(c => c.unavailable)[0])
        wait(100)
        verify(app.library.summary.indexOf("IMG_0005") === 0, "the strip names it: " + app.library.summary)
    }
}
