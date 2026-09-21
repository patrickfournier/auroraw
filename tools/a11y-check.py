#!/usr/bin/env python3
"""Lists what a screen reader could see of a running application, through AT-SPI.

Accessibility must be switched on first (it is off by default on many desktops), for example by
starting the Orca screen reader, or in GNOME:
    gsettings set org.gnome.desktop.interface toolkit-accessibility true

Then start a viewer that stays open, and run this script while it runs:
    ./target/release/viewer-slint --bench view --secs 60 &
    python3 a11y-check.py viewer-slint

Prints the tree of accessible nodes (role and name) and a summary. Needs python3-gi and
gir1.2-atspi-2.0.
"""
import sys
import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

wanted = sys.argv[1].lower() if len(sys.argv) > 1 else "viewer"
desktop = Atspi.get_desktop(0)
found = False
for i in range(desktop.get_child_count()):
    app = desktop.get_child_at_index(i)
    if app is None or wanted not in (app.get_name() or "").lower():
        continue
    found = True
    counts = {}
    named = 0

    def walk(node, depth):
        global named
        role = node.get_role_name()
        name = node.get_name() or ""
        counts[role] = counts.get(role, 0) + 1
        named += 1 if name else 0
        if depth <= 3:
            print("  " * depth + f"{role}: {name!r}")
        for j in range(min(node.get_child_count(), 200)):
            child = node.get_child_at_index(j)
            if child is not None:
                walk(child, depth + 1)

    print(f"application: {app.get_name()}")
    walk(app, 0)
    total = sum(counts.values())
    print(f"\n{total} accessible nodes, {named} with a name; roles: {dict(sorted(counts.items(), key=lambda kv: -kv[1])[:8])}")
if not found:
    print(f"no accessible application matching {wanted!r}: is accessibility switched on, and is the viewer running?")
    sys.exit(1)
