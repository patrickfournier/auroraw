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
    /// A stable name, for a future menu or palette.
    pub name: &'static str,
    /// The keyboard shortcut, as shown to a person (not parsed; the grid's own `key-pressed`
    /// handler is what actually recognises it, in `ui/shell.slint`).
    pub shortcut: &'static str,
}

/// Every command the shell currently exposes.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "Rate 0 (clear)",
        shortcut: "0",
    },
    CommandSpec {
        name: "Rate 1",
        shortcut: "1",
    },
    CommandSpec {
        name: "Rate 2",
        shortcut: "2",
    },
    CommandSpec {
        name: "Rate 3",
        shortcut: "3",
    },
    CommandSpec {
        name: "Rate 4",
        shortcut: "4",
    },
    CommandSpec {
        name: "Rate 5",
        shortcut: "5",
    },
    CommandSpec {
        name: "Move selection up",
        shortcut: "Up",
    },
    CommandSpec {
        name: "Move selection down",
        shortcut: "Down",
    },
    CommandSpec {
        name: "Move selection left",
        shortcut: "Left",
    },
    CommandSpec {
        name: "Move selection right",
        shortcut: "Right",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shortcut in [`COMMANDS`] is handled somewhere in the grid's own `key-pressed`
    /// case, so this table cannot drift from what `ui/shell.slint` actually recognises (testing
    /// strategy §6: "checked by listing the command set against the shortcut table"). A textual
    /// check, not a semantic one: it would not catch a shortcut wired to the wrong action, only
    /// one this crate claims exists but the shell never mentions at all.
    #[test]
    fn every_shortcut_is_mentioned_in_the_shells_key_handling() {
        let shell = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/shell.slint"),
        )
        .unwrap();
        for command in COMMANDS {
            let needle = match command.shortcut {
                "Up" => "Key.UpArrow".to_string(),
                "Down" => "Key.DownArrow".to_string(),
                "Left" => "Key.LeftArrow".to_string(),
                "Right" => "Key.RightArrow".to_string(),
                digit => format!("\"{digit}\""),
            };
            assert!(
                shell.contains(&needle),
                "{}: shortcut {:?} ({needle:?}) not found in shell.slint",
                command.name,
                command.shortcut
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
    fn no_two_commands_share_a_shortcut() {
        for (i, a) in COMMANDS.iter().enumerate() {
            for b in &COMMANDS[i + 1..] {
                assert_ne!(
                    a.shortcut, b.shortcut,
                    "{} and {} both claim {:?}",
                    a.name, b.name, a.shortcut
                );
            }
        }
    }
}
