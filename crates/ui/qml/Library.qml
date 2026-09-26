// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The library grid (the cull task): a filter bar with how many photos it lists, the photos as
// thumbnails from `image://thumbs`, and a strip that describes what is selected. Several photos can be
// selected (D-097): the selection is a set the model keeps by photo (`PhotoGrid`), the cursor (where the
// keyboard is) is the `GridView`'s current index, and ranges start from the model's anchor. Click, Shift and
// Ctrl with the mouse and the keys, Space, Escape, and a rubber band, as in a file manager.
FocusScope {
    id: root
    required property var photoGrid
    required property var launcher

    // The rating's star (a literal U+2605: a regression test reads it back, since a compiler that
    // took the source for a legacy code page once turned it into mojibake on Windows).
    readonly property string star: "★"

    property alias grid: grid
    property alias filterBar: filterBar
    property alias filterButtons: filterButtons
    property alias refreshButton: refreshButton
    // What the strip under the grid says: the selected photo, or how many are selected.
    // (For several photos the text is a binding, so that a change of language reaches it.)
    property string single: ""
    readonly property string summary: photoGrid.selectedCount > 1
        ? qsTr("%n photo(s) selected", "", photoGrid.selectedCount) : single
    readonly property string status: qsTr("%n photo(s)", "", photoGrid.count)
    readonly property int selectedCount: photoGrid.selectedCount
    property alias keywords: keywordList
    property alias keywordPanel: keywordPanel
    // A dialog of the panel is open (the window's commands wait, as for every dialog).
    readonly property bool dialogOpen: keywordPanel.renameDialog.visible || keywordPanel.moveDialog.visible
                                       || keywordPanel.deleteDialog.visible
    // The name of the keyword the list is filtered by, for its chip.
    property string keywordFilterName: ""

    KeywordList { id: keywordList }

    // The keyword panel shows which keywords the selection carries: asked once a selection has settled.
    Timer {
        id: usageTimer
        interval: 60
        onTriggered: keywordList.applyUsage(photoGrid.keywordUsage(), photoGrid.selectedCount)
    }
    Connections {
        target: Bus
        // Every keyword added or removed, undone or redone, changes what the selection carries.
        function onHistoryChanged() {
            keywordList.refresh()
            usageTimer.restart()
            // The keyword the list is filtered by was deleted (or its creation undone): back to the whole list.
            if (photoGrid.keywordFilter !== "" && !keywordList.hasKeyword(photoGrid.keywordFilter))
                root.filterKeyword("", "")
        }
    }

    function flagName(index) {
        return index === 0 ? qsTr("Not rejected") : index === 1 ? qsTr("All photos")
               : index === 2 ? qsTr("Picked") : qsTr("Rejected")
    }

    function updateSummary() {
        usageTimer.restart()
        single = photoGrid.selectedCount === 1 ? photoGrid.summaryAt(photoGrid.firstSelectedRow()) : ""
    }

    // Puts the cursor on `index` and brings it into view.
    function showCursor(index) {
        grid.currentIndex = index
        if (index >= 0)
            grid.positionViewAtIndex(index, GridView.Contain)
    }

    // The cursor goes to `index`, and the gesture's modifiers say what happens to the selection: nothing
    // for Ctrl alone, the range from the anchor for Shift (added to the selection with Ctrl too), else
    // only that photo.
    function goTo(index, modifiers) {
        const shift = (modifiers & Qt.ShiftModifier) !== 0
        const ctrl = (modifiers & Qt.ControlModifier) !== 0
        if (shift) {
            // A range needs somewhere to start from: the cursor, when nothing anchors it.
            if (photoGrid.anchorRow() < 0 && grid.currentIndex >= 0)
                photoGrid.selectOnly(grid.currentIndex)
            photoGrid.extendTo(index, ctrl)
        } else if (!ctrl) {
            photoGrid.selectOnly(index)
        }
        showCursor(index)
        updateSummary()
    }

    // Selects only this photo (or nothing, for -1) and puts the cursor there.
    function select(index) {
        if (index < 0) {
            photoGrid.selectNone()
            grid.currentIndex = -1
        } else {
            goTo(index, 0)
        }
        updateSummary()
    }

    // Ctrl+click or Space: this photo joins the selection or leaves it.
    function toggle(index) {
        photoGrid.toggle(index)
        showCursor(index)
        updateSummary()
    }

    function selectAll() { photoGrid.selectAll(); updateSummary() }
    function selectNone() { photoGrid.selectNone(); updateSummary() }
    function invertSelection() { photoGrid.invert(); updateSummary() }

    // Flags what is selected (or the cursor's photo when nothing is): `pick`, `reject` or `clear`.
    function flag(kind) {
        if (photoGrid.selectedCount === 0 && grid.currentIndex >= 0)
            photoGrid.selectOnly(grid.currentIndex)
        photoGrid.flagSelection(kind)
        updateSummary()
    }

    // Puts the keyboard in the keyword field (Ctrl+K).
    function focusKeywords() {
        keywordPanel.open()
    }

    // Lists only the photos with this keyword, or under it (`""` for all).
    function filterKeyword(id, name) {
        photoGrid.filterKeyword(id)
        keywordFilterName = name
        grid.currentIndex = -1
        grid.contentY = 0
        updateSummary()
    }

    function filterFlags(flags) {
        photoGrid.filterFlags(flags)
        grid.currentIndex = -1
        grid.contentY = 0
        updateSummary()
    }

    // Rates what is selected (as one action, one step of the history), or the cursor's photo when nothing is.
    function rate(stars) {
        if (photoGrid.selectedCount > 0)
            photoGrid.rateSelection(stars)
        else if (grid.currentIndex >= 0)
            photoGrid.setRating(grid.currentIndex, stars)
        updateSummary()
    }

    // Reads the list again (photos arrived): the selection stays on its photos and the cursor on its own,
    // if they are still listed, and the view where it was.
    function reload() {
        const cursor = grid.currentIndex >= 0 ? photoGrid.idAt(grid.currentIndex) : ""
        const scrolled = grid.contentY
        photoGrid.load()
        keywordList.refresh()
        grid.currentIndex = cursor !== "" ? photoGrid.rowOf(cursor) : -1
        grid.contentY = scrolled
        grid.returnToBounds()
        updateSummary()
    }

    // Lists the photos rated `minRating` or more; nothing is selected any more.
    function filterBy(minRating) {
        photoGrid.filterBy(minRating)
        keywordList.refresh()
        grid.currentIndex = -1
        grid.contentY = 0
        updateSummary()
    }

    // An action was undone or redone (Edit menu, Ctrl+Z): the photos it touched are shown as they are now,
    // selected, and the first is brought into view, as a person expects to see what was undone.
    function historyApplied(photoIds) {
        // A step about the vocabulary touches no photo the person should be sent to: the selection stays.
        if (photoIds.length === 0) {
            updateSummary()
            return
        }
        for (const id of photoIds)
            photoGrid.syncPhoto(id)
        // A filter may now list a photo it did not, or not list one it did.
        if (photoGrid.minRating > 0)
            reload()
        photoGrid.selectPhotos(photoIds.join(","))
        showCursor(photoGrid.rowOf(photoIds[0]))
        updateSummary()
    }

    // The engine says a photo changed: its cell and, when it is what the strip describes, the strip follow.
    function photoChanged(photoId) {
        photoGrid.refreshPhoto(photoId)
        if (photoGrid.selectedCount === 1 && photoGrid.isSelected(photoGrid.rowOf(photoId)))
            updateSummary()
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            Rectangle {
                id: filterBar
                Layout.fillWidth: true
                Layout.preferredHeight: 40
                color: root.palette.window
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 6
                    anchors.rightMargin: 6
                    spacing: 6
                    Repeater {
                        id: filterButtons
                        // (The model holds no text, so that a change of language does not rebuild the buttons.)
                        model: 6
                        AppButton {
                            required property int index
                            readonly property int minRating: index
                            text: index === 0 ? qsTr("All") : index === 1 ? qsTr("1+") : index === 2 ? qsTr("2+")
                                  : index === 3 ? qsTr("3+") : index === 4 ? qsTr("4+") : qsTr("5")
                            highlighted: root.photoGrid.minRating === minRating
                            focusPolicy: Qt.NoFocus
                            onClicked: {
                                root.filterBy(minRating)
                                grid.forceActiveFocus()
                            }
                        }
                    }
                    // Which flags are shown: rejected photos are hidden unless asked for (spec §5.3).
                    ComboBox {
                        id: flagBox
                        Layout.leftMargin: 8
                        model: 4
                        focusPolicy: Qt.NoFocus
                        currentIndex: root.photoGrid.flagFilter
                        displayText: root.flagName(currentIndex)
                        Accessible.name: qsTr("Show photos by flag")
                        delegate: ItemDelegate {
                            required property int index
                            width: flagBox.width
                            text: root.flagName(index)
                            highlighted: flagBox.highlightedIndex === index
                        }
                        onActivated: index => {
                            root.filterFlags(index)
                            grid.forceActiveFocus()
                        }
                    }
                    AppButton {
                        visible: root.photoGrid.keywordFilter !== ""
                        text: qsTr("Keyword: %1").arg(root.keywordFilterName) + " ×"
                        focusPolicy: Qt.NoFocus
                        onClicked: {
                            root.filterKeyword("", "")
                            grid.forceActiveFocus()
                        }
                    }
                    Label {
                        text: root.status
                        color: Theme.quiet
                        Layout.leftMargin: 6
                    }
                    // Reads the list again: photos that arrived, and rejected ones that were left in place.
                    AppButton {
                        id: refreshButton
                        text: qsTr("Refresh")
                        focusPolicy: Qt.NoFocus
                        onClicked: {
                            root.reload()
                            grid.forceActiveFocus()
                        }
                    }
                    Item { Layout.fillWidth: true }
                }
            }

            GridView {
                id: grid
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                focus: true
                model: root.photoGrid
                cellWidth: 164
                cellHeight: 124
                currentIndex: -1
                // The arrows are ours, so that a step down from above a short last row lands on its
                // last photo (`gridmath::step`).
                keyNavigationEnabled: false
                readonly property int columns: Math.max(1, Math.floor(width / cellWidth))
                readonly property int visibleRows: Math.max(1, Math.floor(height / cellHeight))

                ScrollBar.vertical: ScrollBar {}

                // The window was resized and the rows re-flowed: the cursor stays in view (once the
                // view has laid its cells out again).
                onColumnsChanged: Qt.callLater(keepCursorInView)

                function keepCursorInView() {
                    if (currentIndex >= 0)
                        positionViewAtIndex(currentIndex, GridView.Contain)
                }

                // Moves the cursor by a step or a jump; the modifiers say what that does to the selection.
                function move(dx, dy, modifiers) {
                    if (count === 0)
                        return
                    // With no cursor, any move goes to the first photo.
                    root.goTo(currentIndex < 0 ? 0 : root.photoGrid.step(currentIndex, dx, dy, columns), modifiers)
                }

                function jump(kind, modifiers) {
                    if (count === 0)
                        return
                    root.goTo(root.photoGrid.jump(kind, Math.max(currentIndex, 0), columns, visibleRows), modifiers)
                }

                Keys.onPressed: event => {
                    if (event.modifiers & (Qt.AltModifier | Qt.MetaModifier))
                        return
                    const ctrlOrShift = event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)
                    if (event.key >= Qt.Key_0 && event.key <= Qt.Key_5) {
                        if (ctrlOrShift)
                            return
                        root.rate(event.key - Qt.Key_0)
                    } else if (event.key === Qt.Key_P || event.key === Qt.Key_X || event.key === Qt.Key_U) {
                        if (ctrlOrShift)
                            return
                        root.flag(event.key === Qt.Key_P ? "pick" : event.key === Qt.Key_X ? "reject" : "clear")
                    } else if (event.key === Qt.Key_Left) {
                        move(-1, 0, event.modifiers)
                    } else if (event.key === Qt.Key_Right) {
                        move(1, 0, event.modifiers)
                    } else if (event.key === Qt.Key_Up) {
                        move(0, -1, event.modifiers)
                    } else if (event.key === Qt.Key_Down) {
                        move(0, 1, event.modifiers)
                    } else if (event.key === Qt.Key_PageUp) {
                        jump("page-up", event.modifiers)
                    } else if (event.key === Qt.Key_PageDown) {
                        jump("page-down", event.modifiers)
                    } else if (event.key === Qt.Key_Home) {
                        jump("home", event.modifiers)
                    } else if (event.key === Qt.Key_End) {
                        jump("end", event.modifiers)
                    } else if (event.key === Qt.Key_Space) {
                        if (currentIndex >= 0)
                            root.toggle(currentIndex)
                    } else if (event.key === Qt.Key_Escape) {
                        if (root.selectedCount === 0)
                            return
                        root.selectNone()
                    } else {
                        return
                    }
                    event.accepted = true
                }

                // Every click and drag on the grid, under the cells (they are not clickable themselves): a press
                // on a photo selects it as the modifiers say, a press on empty space clears the selection, and
                // a drag from anywhere is a rubber band. It does not let the view take the drag to scroll.
                MouseArea {
                    id: pointer
                    // In the view's content (so that what it reports is where the photos are, scrolled or
                    // not), under the cells.
                    parent: grid.contentItem
                    z: -1
                    width: grid.width
                    height: Math.max(grid.contentHeight, grid.height)
                    preventStealing: true
                    acceptedButtons: Qt.LeftButton

                    property real startX: 0
                    property real startY: 0
                    property real lastX: 0
                    property real lastY: 0
                    property int modifiers: 0
                    property bool onEmpty: false
                    property bool banding: false

                    // The photo under a point of the view's content, -1 for none (a gap, empty space).
                    function photoAt(x, y) {
                        const column = Math.floor(x / grid.cellWidth)
                        const row = Math.floor(y / grid.cellHeight)
                        const inside = x - column * grid.cellWidth >= 4 && x - column * grid.cellWidth < 164
                                       && y - row * grid.cellHeight < 120
                        const index = row * grid.columns + column
                        return inside && column < grid.columns && index >= 0 && index < grid.count ? index : -1
                    }

                    // The rubber band, now: the cells it covers are selected.
                    function band() {
                        const left = Math.min(startX, lastX), right = Math.max(startX, lastX)
                        const top = Math.min(startY, lastY), bottom = Math.max(startY, lastY)
                        root.photoGrid.rubberTo(Math.floor(top / grid.cellHeight), Math.floor(bottom / grid.cellHeight),
                                                Math.floor(left / grid.cellWidth), Math.floor(right / grid.cellWidth),
                                                grid.columns)
                        root.updateSummary()
                    }

                    onPressed: mouse => {
                        grid.forceActiveFocus()
                        startX = lastX = mouse.x
                        startY = lastY = mouse.y
                        modifiers = mouse.modifiers
                        banding = false
                        const index = photoAt(mouse.x, mouse.y)
                        onEmpty = index < 0
                        if (index >= 0) {
                            if ((modifiers & Qt.ControlModifier) && !(modifiers & Qt.ShiftModifier))
                                root.toggle(index)
                            else
                                root.goTo(index, modifiers)
                        }
                    }

                    onPositionChanged: mouse => {
                        if (!pressed)
                            return
                        lastX = Math.max(0, Math.min(mouse.x, width))
                        lastY = Math.max(0, Math.min(mouse.y, height))
                        if (!banding && Math.abs(lastX - startX) + Math.abs(lastY - startY) > 6) {
                            banding = true
                            root.photoGrid.rubberBegin((modifiers & Qt.ControlModifier) !== 0)
                        }
                        if (banding)
                            band()
                    }

                    onReleased: {
                        if (banding)
                            root.photoGrid.rubberEnd()
                        else if (onEmpty && !(modifiers & (Qt.ControlModifier | Qt.ShiftModifier)))
                            root.selectNone()
                        banding = false
                        autoScroll.stop()
                    }

                    // A rubber band held at an edge of the view scrolls it.
                    Timer {
                        id: autoScroll
                        interval: 30
                        repeat: true
                        running: pointer.banding && pointer.pressed
                        onTriggered: {
                            const shown = pointer.lastY - grid.contentY
                            const step = shown < 24 ? -20 : shown > grid.height - 24 ? 20 : 0
                            if (step === 0)
                                return
                            grid.contentY = Math.max(0, Math.min(grid.contentY + step,
                                                                 Math.max(0, grid.contentHeight - grid.height)))
                            pointer.lastY = Math.max(0, Math.min(pointer.lastY + step, pointer.height))
                            pointer.band()
                        }
                    }
                }

                delegate: Item {
                    id: cell
                    required property int index
                    required property string photoId
                    required property int rating
                    required property bool selected
                    required property int flag
                    // No thumbnail can be made for this photo (it says so instead of staying empty).
                    readonly property bool unavailable: thumbnail.status === Image.Error
                    readonly property bool shown: thumbnail.status === Image.Ready
                    width: grid.cellWidth
                    height: grid.cellHeight
                    Accessible.role: Accessible.ListItem
                    Accessible.selected: cell.selected
                    Accessible.name: cell.rating > 0 ? qsTr("Photo, %n star(s)", "", cell.rating) : qsTr("Photo")

                    Rectangle {
                        x: 4
                        width: 160
                        height: 120
                        color: root.palette.dark

                        Image {
                            id: thumbnail
                            anchors.fill: parent
                            source: "image://thumbs/" + cell.photoId
                            fillMode: Image.PreserveAspectFit
                            asynchronous: true
                            // A rejected photo is dimmed, and stays where it is until the list is read again.
                            opacity: cell.flag === 2 ? 0.35 : 1
                        }
                        // A photo no thumbnail can be made for (an unreadable file, a RAW without a preview).
                        Label {
                            anchors.centerIn: parent
                            visible: cell.unavailable
                            text: qsTr("No preview")
                            color: Theme.grey.placeholder
                        }
                        // What is selected is tinted, so that a set reads at a glance.
                        Rectangle {
                            anchors.fill: parent
                            visible: cell.selected
                            color: root.palette.highlight
                            opacity: 0.38
                        }
                        // The rating, on a dark chip so that it reads over any picture.
                        Rectangle {
                            x: 4
                            y: 4
                            visible: cell.rating > 0
                            width: stars.implicitWidth + 8
                            height: stars.implicitHeight + 2
                            radius: 3
                            color: "#a0000000"
                            Text {
                                id: stars
                                anchors.centerIn: parent
                                text: cell.rating + root.star
                                color: Theme.rating
                            }
                        }
                        // The flag: picked ✔, rejected ✖.
                        Rectangle {
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 4
                            visible: cell.flag !== 0
                            width: flagMark.implicitWidth + 8
                            height: flagMark.implicitHeight + 2
                            radius: 3
                            color: "#a0000000"
                            Text {
                                id: flagMark
                                anchors.centerIn: parent
                                text: cell.flag === 1 ? "✔" : "✖"
                                color: cell.flag === 1 ? Theme.picked : Theme.danger
                            }
                        }
                        // The selection's frame, over the picture.
                        Rectangle {
                            anchors.fill: parent
                            color: "transparent"
                            border.width: cell.selected ? 3 : 0
                            border.color: root.palette.highlight
                        }
                        // The cursor, when it is not the only thing selected: where the keyboard is.
                        Rectangle {
                            anchors.fill: parent
                            anchors.margins: 3
                            visible: cell.GridView.isCurrentItem && (root.selectedCount !== 1 || !cell.selected)
                            color: "transparent"
                            border.width: 1
                            border.color: root.palette.windowText
                        }
                    }
                }
            }

            // What is selected: the photo, or how many photos (the panels come with a later work package).
            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 28
                color: root.palette.window
                Label {
                    x: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.summary
                    color: Theme.quiet
                    elide: Text.ElideRight
                    width: parent.width - 12
                }
            }
        }

        KeywordPanel {
            id: keywordPanel
            Layout.fillHeight: true
            keywords: root.keywords
            photoGrid: root.photoGrid
            library: root
            launcher: root.launcher
        }
    }
}
