// SPDX-License-Identifier: GPL-3.0-or-later
//! The command set (spec §3: "every action is a command reachable by keyboard, menu and later a
//! palette"): a small, explicit table so "every command has a keyboard shortcut" is something a
//! test can check by listing the table, rather than something only a person clicking through the
//! shell could notice was missing (testing strategy §6).
//!
//! Grows with later work packages: WP9's cull mode adds flags, labels and series commands to this
//! same table; a menu and a command palette read it too, once built.

/// One command the shell can carry out and how a person reaches it from the keyboard.
pub struct CommandSpec {
    /// A stable identifier: what the menus and the shortcuts send to the interface's command entry
    /// point.
    pub id: &'static str,
    /// A name, for a future palette.
    pub name: &'static str,
    /// The keyboard shortcut, as shown to a person (not parsed; `ui/shell.slint`'s own key handling
    /// recognises it).
    pub shortcut: &'static str,
    /// Whether the hamburger menu lists it (`ui/menu.slint`).
    pub menu: bool,
}

const fn command(
    id: &'static str,
    name: &'static str,
    shortcut: &'static str,
    menu: bool,
) -> CommandSpec {
    CommandSpec {
        id,
        name,
        shortcut,
        menu,
    }
}

/// Every command the shell currently exposes.
pub const COMMANDS: &[CommandSpec] = &[
    command("grid.rate-0", "Rate 0 (clear)", "0", false),
    command("grid.rate-1", "Rate 1", "1", false),
    command("grid.rate-2", "Rate 2", "2", false),
    command("grid.rate-3", "Rate 3", "3", false),
    command("grid.rate-4", "Rate 4", "4", false),
    command("grid.rate-5", "Rate 5", "5", false),
    command("grid.up", "Move selection up", "Up", false),
    command("grid.down", "Move selection down", "Down", false),
    command("grid.left", "Move selection left", "Left", false),
    command("grid.right", "Move selection right", "Right", false),
    command("file.new-workspace", "New workspace", "Ctrl+N", true),
    command("file.open-workspace", "Open workspace", "Ctrl+O", true),
    command("file.settings", "Settings", "Ctrl+,", true),
    command("file.import", "Import", "Ctrl+I", true),
    command("file.quit", "Quit", "Ctrl+Q", true),
    command("edit.undo", "Undo", "Ctrl+Z", true),
    command("edit.redo", "Redo", "Ctrl+Y", true),
    command("edit.cut", "Cut", "Ctrl+X", true),
    command("edit.copy", "Copy", "Ctrl+C", true),
    command("edit.paste", "Paste", "Ctrl+V", true),
    command("edit.select-all", "Select all", "Ctrl+A", true),
    command("edit.delete", "Delete", "Del", true),
    command("help.about", "About", "F1", true),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn slint_file(name: &str) -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("ui")
                .join(name),
        )
        .unwrap()
    }

    /// Every shortcut the shell itself handles is mentioned in its key handling, so this table
    /// cannot drift from what `ui/shell.slint` actually recognises (testing strategy §6: "checked by
    /// listing the command set against the shortcut table"). A textual check, not a semantic one: it
    /// would not catch a shortcut wired to the wrong action, only one this crate claims exists but
    /// the shell never mentions at all. The Edit shortcuts (Ctrl+Z, Y, X, C, V, A and Del) are not
    /// the shell's: a text field handles them itself, and the Edit menu sends them to it.
    #[test]
    fn every_shortcut_is_mentioned_in_the_shells_key_handling() {
        let shell = slint_file("shell.slint");
        for command in COMMANDS {
            if command.id.starts_with("edit.") {
                continue;
            }
            let needle = match command.shortcut {
                "Up" => "Key.UpArrow".to_string(),
                "Down" => "Key.DownArrow".to_string(),
                "Left" => "Key.LeftArrow".to_string(),
                "Right" => "Key.RightArrow".to_string(),
                "F1" => "Key.F1".to_string(),
                shortcut if shortcut.starts_with("Ctrl+") => format!(
                    "event.text == \"{}\"",
                    shortcut.trim_start_matches("Ctrl+").to_lowercase()
                ),
                digit => format!("\"{digit}\""),
            };
            assert!(
                shell.contains(&needle),
                "{}: shortcut {:?} ({needle:?}) not found in shell.slint",
                command.name,
                command.shortcut
            );
            if command.shortcut.starts_with("Ctrl+") || command.shortcut == "F1" {
                assert!(
                    shell.contains(&format!("root.command(\"{}\")", command.id)),
                    "{}: the shell's key handling does not send {:?}",
                    command.name,
                    command.id
                );
            }
        }
    }

    /// The menu lists exactly the commands the table says it does.
    #[test]
    fn the_menu_and_the_table_agree() {
        let menu = slint_file("menu.slint");
        for command in COMMANDS {
            let listed = menu.contains(&format!("id: \"{}\"", command.id));
            assert_eq!(
                listed, command.menu,
                "{}: menu = {} in the table, but listed in menu.slint = {listed}",
                command.id, command.menu
            );
        }
        let ids: Vec<&str> = menu
            .match_indices("id: \"")
            .map(|(at, needle)| {
                let rest = &menu[at + needle.len()..];
                &rest[..rest.find('"').unwrap()]
            })
            .collect();
        assert!(!ids.is_empty());
        for id in ids {
            assert!(
                COMMANDS.iter().any(|c| c.id == id),
                "menu.slint lists {id:?}, which is not a command"
            );
        }
    }

    #[test]
    fn every_command_has_a_keyboard_shortcut() {
        for command in COMMANDS {
            assert!(
                !command.shortcut.is_empty(),
                "{} has no keyboard shortcut (spec §3: keyboard first)",
                command.name
            );
        }
    }

    #[test]
    fn no_two_commands_share_a_shortcut_or_an_id() {
        for (i, a) in COMMANDS.iter().enumerate() {
            for b in &COMMANDS[i + 1..] {
                assert_ne!(
                    a.shortcut, b.shortcut,
                    "{} and {} both claim {:?}",
                    a.name, b.name, a.shortcut
                );
                assert_ne!(a.id, b.id);
            }
        }
    }
}
