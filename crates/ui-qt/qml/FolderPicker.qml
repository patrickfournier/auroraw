// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Dialogs
import org.auroraw.ui

// The system's folder dialog for a text field: it opens where the field points (the closest folder
// that exists), and its answer fills the field. It is a window of its own, which Qt cannot make modal
// to ours: `Main` covers the window while `visible` (see `nativeDialogOpen`).
FolderDialog {
    id: picker
    // The text field the dialog is for, if any (its text says where to open, the answer fills it).
    property var field: null
    signal chosen(string path)
    property var hostWindow: null
    parentWindow: hostWindow

    // A `file:` URL for a path (`file:///C:/photos` on Windows, `file:///home/...` elsewhere).
    function urlFor(path) {
        const slashes = path.replace(/\\/g, "/")
        return encodeURI("file://" + (slashes.charAt(0) === "/" ? "" : "/") + slashes)
    }

    // The path of a `file:` URL, the way the application writes it.
    function pathOf(url) {
        let path = decodeURIComponent(url.toString()).replace(/^file:\/\//, "")
        if (/^\/[A-Za-z]:/.test(path))
            path = path.substring(1)
        return path
    }

    // Where the dialog opens for what the field says: a path, empty for the system's own default.
    function startPath() { return field ? Folders.closest(field.text) : "" }

    function pick() {
        const start = startPath()
        if (start !== "")
            currentFolder = urlFor(start)
        open()
    }

    // The answer, as the dialog gives it (tests give it directly: no native dialog opens off screen).
    function choose(path) {
        if (field)
            field.text = path
        chosen(path)
    }

    onAccepted: choose(pathOf(selectedFolder))
}
