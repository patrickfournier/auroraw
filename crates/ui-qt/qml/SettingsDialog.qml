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
                // The model holds no text: it would be rebuilt (and its buttons with it) by every
                // change of language, which Qt 6.4 lays out badly when the dialog is closed.
                model: ["system", "en", "fr"]
                delegate: Button {
                    required property string modelData
                    text: modelData === "system" ? qsTr("System")
                          : modelData === "en" ? "English" : "Français"
                    checkable: true
                    autoExclusive: true
                    checked: dialog.launcher.language === modelData
                    onClicked: dialog.launcher.chooseLanguage(modelData)
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
