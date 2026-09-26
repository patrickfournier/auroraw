// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import org.auroraw.ui

// The keyword panel (spec §5.7, D-098): a keyword made from the field and given to the selection, the
// tri-state check (none, some, all), a click taking it off, undo and redo with the panel following, the
// hierarchy and its collapse, type-ahead, Shift+Enter, the list filtered by a keyword, rename, and French.
// The 40-photo machine, shared by the tests: each one uses names of its own, and gives back what it gave.
AppTestCase {
    name: "Keywords"

    property var grid: null

    function init() {
        launch("")
        app.width = 1680
        wait(300)
        tryCompare(app.photos, "count", 40)
        grid = app.library.grid
    }

    // Every keyword is taken off every photo, and the language put back (the vocabulary itself stays).
    function cleanup() {
        if (app) {
            app.launcher.chooseLanguage("en")
            app.library.filterKeyword("", "")
            app.keywordPanel.filterField.text = ""
            app.library.selectAll()
            for (let i = 0; i < app.library.keywords.count; i++)
                app.photos.keywordSelection(app.library.keywords.idAt(i), false)
            wait(300)
        }
        quit()
    }

    function cell(index) {
        grid.positionViewAtIndex(index, GridView.Contain)
        wait(30)
        const item = grid.itemAtIndex(index)
        verify(item, "the cell " + index + " exists")
        return item
    }

    function click(index, modifiers) {
        const item = cell(index)
        mouseClick(item, item.width / 2, item.height / 2, Qt.LeftButton, modifiers === undefined ? Qt.NoModifier : modifiers)
        wait(40)
    }

    // The first `count` photos, selected.
    function selectFirst(count) {
        click(0)
        if (count > 1)
            click(count - 1, Qt.ShiftModifier)
        compare(app.photos.selectedCount, count)
    }

    // Types in the panel's field and presses Enter (Shift when asked).
    function typeKeyword(text, modifiers) {
        app.library.focusKeywords()
        tryVerify(() => app.keywordPanel.filterField.activeFocus)
        app.keywordPanel.filterField.text = text
        keyClick(Qt.Key_Return, modifiers === undefined ? Qt.NoModifier : modifiers)
        wait(150)
    }

    function rowOf(name) {
        for (let i = 0; i < app.library.keywords.count; i++)
            if (app.library.keywords.nameAt(i) === name)
                return i
        return -1
    }

    function item(name) {
        const row = rowOf(name)
        verify(row >= 0, "the keyword " + name + " is listed")
        const tree = app.keywordPanel.tree
        tree.positionViewAtIndex(row, ListView.Contain)
        wait(30)
        const it = tree.itemAtIndex(row)
        verify(it, "the row of " + name + " exists")
        return it
    }

    // The check of a keyword's row, clicked.
    function clickCheck(name) {
        const it = item(name)
        mouseClick(it, it.depth * 14 + 16 + 2 + 8, it.height / 2)
        wait(150)
    }

    function test_a_keyword_typed_in_the_field_is_made_and_given_to_the_selection() {
        selectFirst(3)
        typeKeyword("Peru")
        tryCompare(item("Peru"), "carried", 2)
        compare(item("Peru").photos, 3)
        compare(app.keywordPanel.filterField.text, "", "the field is emptied")
        tryVerify(() => app.actions.undo.enabled, 5000)
        compare(app.actions.undo.text, "Undo keywords of 3 photos")
    }

    function test_typing_a_keyword_that_exists_gives_it_and_does_not_make_another() {
        selectFirst(2)
        typeKeyword("Lima")
        const before = app.library.keywords.count
        click(5)
        typeKeyword("lima")
        compare(app.library.keywords.count, before, "the same keyword")
        tryCompare(item("Lima"), "photos", 3)
    }

    function test_the_check_says_none_some_or_all_and_a_click_gives_or_takes_off() {
        selectFirst(3)
        typeKeyword("Tri")
        tryCompare(item("Tri"), "carried", 2)
        selectFirst(5)
        tryCompare(item("Tri"), "carried", 1) // some of the five
        compare(item("Tri").photos, 3)
        clickCheck("Tri")
        tryCompare(item("Tri"), "carried", 2)
        compare(item("Tri").photos, 5)
        clickCheck("Tri")
        tryCompare(item("Tri"), "carried", 0)
        compare(item("Tri").photos, 0)
    }

    function test_undo_and_redo_bring_the_panel_along() {
        selectFirst(3)
        typeKeyword("Undoable")
        tryCompare(item("Undoable"), "carried", 2)
        wait(300)
        keyClick(Qt.Key_Escape)
        verify(app.library.grid.activeFocus, "Escape gives the keyboard back to the grid")
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        tryCompare(item("Undoable"), "carried", 0)
        compare(item("Undoable").photos, 0)
        tryVerify(() => app.actions.redo.enabled)
        compare(app.actions.redo.text, "Redo keywords of 3 photos")
        keyClick(Qt.Key_Y, Qt.ControlModifier)
        tryCompare(item("Undoable"), "carried", 2)
        compare(item("Undoable").photos, 3)
    }

    function test_nothing_is_given_when_nothing_is_selected() {
        typeKeyword("Orphan")
        compare(app.photos.selectedCount, 0)
        tryCompare(item("Orphan"), "photos", 0)
        compare(item("Orphan").carried, 0)
    }

    function test_a_keyword_made_after_a_click_goes_under_that_keyword_and_can_be_collapsed() {
        selectFirst(2)
        typeKeyword("Places")
        const places = item("Places")
        mouseClick(places, 100, places.height / 2)
        compare(app.keywordPanel.createUnderName, "Places")
        typeKeyword("Cusco")
        tryVerify(() => rowOf("Cusco") === rowOf("Places") + 1)
        compare(item("Cusco").depth, 1)
        compare(item("Places").hasChildren, true)
        compare(item("Places").expanded, true)
        app.library.keywords.toggleExpanded(rowOf("Places"))
        compare(rowOf("Cusco"), -1, "the child is out of sight when its parent is collapsed")
        snapshot("keywords-collapsed-en")
        app.library.keywords.toggleExpanded(rowOf("Places"))
        verify(rowOf("Cusco") > 0)
        app.keywordPanel.createUnder = ""
    }

    function test_the_field_filters_the_tree_and_keeps_the_ancestors() {
        selectFirst(2)
        typeKeyword("Europe")
        mouseClick(item("Europe"), 100, 14)
        typeKeyword("Paris")
        mouseClick(item("Europe"), 100, 14)
        typeKeyword("Rome")
        app.keywordPanel.createUnder = ""
        app.library.focusKeywords()
        app.keywordPanel.filterField.text = "pari"
        compare(rowOf("Paris") >= 0, true)
        compare(rowOf("Europe") >= 0, true, "the parent stays to show where it is")
        compare(rowOf("Rome"), -1)
        snapshot("keywords-filtered-en")
        app.keywordPanel.filterField.text = ""
        verify(rowOf("Rome") >= 0)
    }

    function test_shift_enter_makes_the_keyword_even_when_another_matches() {
        selectFirst(2)
        typeKeyword("Sea")
        const before = app.library.keywords.count
        click(4)
        typeKeyword("Se", Qt.ShiftModifier)
        compare(app.library.keywords.count, before + 1)
        verify(rowOf("Se") >= 0)
        tryCompare(item("Se"), "photos", 1)
    }

    function test_the_list_can_be_filtered_by_a_keyword() {
        selectFirst(4)
        typeKeyword("Filtered")
        tryCompare(item("Filtered"), "photos", 4)
        const id = app.library.keywords.idAt(rowOf("Filtered"))
        app.library.filterKeyword(id, "Filtered")
        tryCompare(app.photos, "count", 4)
        compare(app.library.status, "4 photos")
        compare(app.photos.selectedCount, 0, "a change of list lets go of the selection")
        app.library.filterKeyword("", "")
        tryCompare(app.photos, "count", 40)
    }

    function test_a_keyword_is_renamed() {
        selectFirst(2)
        typeKeyword("Draft")
        const dialog = app.keywordPanel.renameDialog
        dialog.openFor(rowOf("Draft"), "Draft")
        tryVerify(() => dialog.visible)
        verify(app.dialogOpen, "the window's commands wait")
        dialog.nameField.text = "Final"
        dialog.tryRename()
        tryVerify(() => !dialog.visible)
        verify(rowOf("Final") >= 0)
        compare(rowOf("Draft"), -1)
        compare(item("Final").photos, 2)
        dialog.openFor(rowOf("Final"), "Final")
        dialog.nameField.text = "a|b"
        dialog.tryRename()
        verify(dialog.error !== "", "a name with | is refused and says why")
        dialog.close()
    }

    function test_the_panel_can_be_folded_away_and_brought_back() {
        const panel = app.keywordPanel
        compare(panel.width, 280)
        mouseClick(panel.collapseButton)
        tryCompare(panel, "width", 30)
        app.library.focusKeywords()
        tryCompare(panel, "width", 280)
    }

    function test_the_panel_speaks_french() {
        selectFirst(3)
        typeKeyword("Vue")
        app.launcher.chooseLanguage("fr")
        wait(250)
        compare(app.keywordPanel.filterField.placeholderText, "Chercher ou ajouter un mot-clé…")
        tryVerify(() => app.actions.undo.text === "Annuler les mots-clés de 3 photos", 5000, app.actions.undo.text)
        compare(app.library.summary, "3 photos sélectionnées", "what is already on screen changes language")
        snapshot("keywords-fr")
    }
}
