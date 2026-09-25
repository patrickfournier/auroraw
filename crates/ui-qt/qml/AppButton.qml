// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls

// A button that looks disabled when it is (Fusion, on our palette, only darkens its background): the
// application's buttons are all this one.
Button {
    opacity: enabled ? 1 : 0.4
}
