// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The application's settings (D-090): for now the language, applied at once and remembered.
AppDialog {
    id: dialog
    required property var launcher
    preferredWidth: 480
    title: qsTr("Settings")
    property alias languageButtons: languages

    contentItem: ColumnLayout {
        spacing: 10
        Label {
            text: qsTr("Language")
            font.bold: true
        }
        RowLayout {
            spacing: 8
            Repeater {
                id: languages
                model: [
                    { code: "system", label: qsTr("System") },
                    { code: "en", label: "English" },
                    { code: "fr", label: "Français" }
                ]
                delegate: Button {
                    required property var modelData
                    text: modelData.label
                    checkable: true
                    autoExclusive: true
                    checked: dialog.launcher.language === modelData.code
                    onClicked: dialog.launcher.chooseLanguage(modelData.code)
                }
            }
        }
    }

    footer: DialogButtonBox {
        Button {
            text: qsTr("Close")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }
}
