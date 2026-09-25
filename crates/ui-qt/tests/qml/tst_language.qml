// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest

// Choosing a language translates what is on screen at once (an empty machine).
TestCase {
    id: tc
    name: "Language"
    when: windowShown

    property var appComponent: Qt.createComponent("qrc:/qt/qml/org/auroraw/ui/qml/Main.qml")
    property var app

    function init() {
        app = createTemporaryObject(appComponent, tc)
        verify(app)
        wait(400)
    }

    function cleanup() {
        if (app) {
            app.close()
            app.destroy()
        }
    }

    function test_the_language_can_be_switched_and_switched_back_live() {
        compare(app.welcome.newButton.text, "New workspace…")
        app.launcher.chooseLanguage("fr")
        wait(200)
        compare(app.launcher.language, "fr")
        compare(app.launcher.effectiveLanguage, "fr")
        compare(app.welcome.newButton.text, "Nouveau workspace…")
        app.launcher.chooseLanguage("en")
        wait(200)
        compare(app.welcome.newButton.text, "New workspace…")
        app.launcher.chooseLanguage("system")
        verify(["en", "fr"].indexOf(app.launcher.effectiveLanguage) >= 0)
        app.launcher.chooseLanguage("en")
    }
}
