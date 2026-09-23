// SPDX-License-Identifier: GPL-3.0-or-later
//! Recognising a mounted removable volume (a memory card, a USB drive), one function per
//! platform (architecture §12).
//!
//! This deliberately does not use the platforms named in architecture.md (udisks2 over D-Bus on
//! Linux, `DiskArbitration` on macOS, `SetupAPI`/device notifications on Windows): those need a
//! running service or Objective-C framework bindings, a large surface for one work package, and
//! commonly do not run cleanly in a container or a CI runner anyway. Each platform here instead
//! asks the operating system directly for the one fact needed (is this mounted device
//! removable?) through a mechanism that needs no daemon and returns an empty list gracefully
//! when there is nothing removable attached, which is the common case in CI. Real hardware
//! confirms this on each platform as part of the pre-release checklist (testing strategy §11,
//! item 7), the same way WP1's Windows result did (GitHub issue #1).

use std::path::PathBuf;

/// A removable volume this machine currently has mounted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// The device's name, for display (not stable across replugging on every platform).
    pub name: String,
    /// Where it is mounted right now.
    pub mount_point: PathBuf,
}

/// Every removable volume mounted right now. Empty, never an error, when there is none or this
/// platform's mechanism cannot answer (a missing `/sys/block`, `diskutil` not on `PATH`, ...):
/// nothing here is safety-critical, so a source simply does not appear rather than the caller
/// having to handle a failure.
pub fn list_removable_volumes() -> Vec<Volume> {
    imp::list_removable_volumes()
}

/// Whether `root` looks like a camera's storage (M1 plan §6, item 6: "card detection from the
/// mounted volumes with a DCIM folder"), the DCF standard every camera and phone that writes
/// directly to a card or exposes one as mass storage follows. Checked case-insensitively (FAT and
/// exFAT, what cards almost always carry, are case-insensitive; a card reformatted by a computer
/// could still be told apart on a case-sensitive filesystem otherwise) and without reading
/// anything inside it, so an empty or write-protected card still counts. Never an error: a
/// vanished mount point (unplugged between listing and checking) simply does not look like one.
pub fn has_dcim(root: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case("DCIM"))
            && entry.path().is_dir()
    })
}

#[cfg(target_os = "linux")]
mod imp {
    use super::Volume;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    /// `/proc/mounts`' `<device> <mount point> ...`, one map entry per line. Octal-escaped bytes
    /// (space, tab, backslash, newline) in the mount point are decoded.
    pub(super) fn parse_mounts(text: &str) -> HashMap<String, PathBuf> {
        let mut map = HashMap::new();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            if let (Some(device), Some(mount_point)) = (parts.next(), parts.next()) {
                map.insert(
                    device.to_string(),
                    PathBuf::from(unescape_octal(mount_point)),
                );
            }
        }
        map
    }

    fn unescape_octal(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            let digits: String = chars.by_ref().take(3).collect();
            match u8::from_str_radix(&digits, 8) {
                Ok(byte) => out.push(byte as char),
                Err(_) => {
                    out.push('\\');
                    out.push_str(&digits);
                }
            }
        }
        out
    }

    /// Every device name under `sys_block` (normally `/sys/block`) whose own `removable` file
    /// says `1`, plus its partitions (a subdirectory of its own whose name it prefixes and which
    /// has a `partition` file). Takes the directory as a parameter so a test can point it at a
    /// fixture tree instead of the real `/sys/block`.
    pub(super) fn removable_device_names(sys_block: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(sys_block) else {
            return out;
        };
        for entry in entries.flatten() {
            let device = entry.file_name().to_string_lossy().into_owned();
            let removable =
                std::fs::read_to_string(entry.path().join("removable")).unwrap_or_default();
            if removable.trim() != "1" {
                continue;
            }
            out.push(device.clone());
            if let Ok(subs) = std::fs::read_dir(entry.path()) {
                for sub in subs.flatten() {
                    let name = sub.file_name().to_string_lossy().into_owned();
                    if name.starts_with(&device)
                        && name != device
                        && sub.path().join("partition").exists()
                    {
                        out.push(name);
                    }
                }
            }
        }
        out
    }

    pub(super) fn list_removable_volumes() -> Vec<Volume> {
        let mounts = std::fs::read_to_string("/proc/mounts")
            .map(|text| parse_mounts(&text))
            .unwrap_or_default();
        removable_device_names(Path::new("/sys/block"))
            .into_iter()
            .filter_map(|name| {
                let mount_point = mounts.get(&format!("/dev/{name}"))?.clone();
                Some(Volume { name, mount_point })
            })
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_a_mount_line_with_an_escaped_space() {
            let mounts = parse_mounts("/dev/sdb1 /media/card\\040reader vfat rw,relatime 0 0\n");
            assert_eq!(
                mounts.get("/dev/sdb1"),
                Some(&PathBuf::from("/media/card reader"))
            );
        }

        #[test]
        fn finds_a_removable_device_and_its_partition() {
            let dir = auroraw_testkit::temp_dir();
            let sdb = dir.path().join("sdb");
            std::fs::create_dir_all(sdb.join("sdb1")).unwrap();
            std::fs::write(sdb.join("removable"), "1\n").unwrap();
            std::fs::write(sdb.join("sdb1/partition"), "1\n").unwrap();
            let sda = dir.path().join("sda");
            std::fs::create_dir_all(&sda).unwrap();
            std::fs::write(sda.join("removable"), "0\n").unwrap();

            let mut names = removable_device_names(dir.path());
            names.sort();
            assert_eq!(names, vec!["sdb".to_string(), "sdb1".to_string()]);
        }

        #[test]
        fn a_missing_sys_block_gives_an_empty_list_not_an_error() {
            assert!(removable_device_names(Path::new("/does/not/exist")).is_empty());
        }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::Volume;
    use std::path::PathBuf;

    // Stable since Windows 2000; not worth a windows-sys feature for one constant
    // (`Win32_System_WindowsProgramming`) beyond the two functions this already needs.
    const DRIVE_REMOVABLE: u32 = 2;

    fn wide_null(s: &str) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    // The workspace denies `unsafe_code` project-wide (nothing has needed it before); this is the
    // one place two raw Win32 calls are worth it over pulling in a wrapper crate for a single
    // pair of functions. SAFETY: `GetLogicalDrives` takes no arguments; `GetDriveTypeW` takes a
    // pointer to a null-terminated wide string, which `root` is, kept alive for the call's
    // duration.
    #[allow(unsafe_code)]
    pub(super) fn list_removable_volumes() -> Vec<Volume> {
        let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
        let mut out = Vec::new();
        for letter in 0..26u32 {
            if mask & (1 << letter) == 0 {
                continue;
            }
            let drive = char::from(b'A' + letter as u8);
            let root = wide_null(&format!("{drive}:\\"));
            let kind =
                unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(root.as_ptr()) };
            if kind == DRIVE_REMOVABLE {
                out.push(Volume {
                    name: format!("{drive}:"),
                    mount_point: PathBuf::from(format!("{drive}:\\")),
                });
            }
        }
        out
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Volume;
    use std::path::PathBuf;
    use std::process::Command;

    /// The first `<true/>` or `<false/>` after `<key>{key}</key>` in a `diskutil info -plist`
    /// dump; `None` if the key is absent. Good enough for a hint, not parsed as general plist.
    pub(super) fn plist_bool(xml: &str, key: &str) -> Option<bool> {
        let marker = format!("<key>{key}</key>");
        let after = &xml[xml.find(&marker)? + marker.len()..];
        match (after.find("<true/>"), after.find("<false/>")) {
            (Some(t), Some(f)) => Some(t < f),
            (Some(_), None) => Some(true),
            (None, Some(_)) => Some(false),
            (None, None) => None,
        }
    }

    fn is_removable(mount_point: &std::path::Path) -> bool {
        let Ok(out) = Command::new("diskutil")
            .args(["info", "-plist"])
            .arg(mount_point)
            .output()
        else {
            return false;
        };
        if !out.status.success() {
            return false;
        }
        let xml = String::from_utf8_lossy(&out.stdout);
        plist_bool(&xml, "Internal") == Some(false)
            || plist_bool(&xml, "RemovableMedia") == Some(true)
            || plist_bool(&xml, "Ejectable") == Some(true)
    }

    pub(super) fn list_removable_volumes() -> Vec<Volume> {
        let Ok(entries) = std::fs::read_dir("/Volumes") else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| {
                let mount_point = entry.path();
                is_removable(&mount_point).then(|| Volume {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    mount_point,
                })
            })
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_internal_false_before_the_next_key() {
            let xml = "<key>Internal</key>\n\t<false/>\n\t<key>Ejectable</key>\n\t<true/>";
            assert_eq!(plist_bool(xml, "Internal"), Some(false));
            assert_eq!(plist_bool(xml, "Ejectable"), Some(true));
            assert_eq!(plist_bool(xml, "NoSuchKey"), None);
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod imp {
    use super::Volume;

    pub(super) fn list_removable_volumes() -> Vec<Volume> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_panic_on_this_machine() {
        // Whatever this CI runner or the developer's machine has attached, this must not panic
        // or error; an empty result (the common case, no removable media plugged in) is fine.
        let _ = list_removable_volumes();
    }

    #[test]
    fn a_folder_with_a_dcim_subfolder_looks_like_a_card() {
        let dir = auroraw_testkit::temp_dir();
        std::fs::create_dir_all(dir.path().join("DCIM")).unwrap();
        assert!(has_dcim(dir.path()));
    }

    #[test]
    fn the_check_is_case_insensitive() {
        let dir = auroraw_testkit::temp_dir();
        std::fs::create_dir_all(dir.path().join("dcim")).unwrap();
        assert!(has_dcim(dir.path()));
    }

    #[test]
    fn a_plain_folder_and_a_dcim_file_do_not_look_like_a_card() {
        let dir = auroraw_testkit::temp_dir();
        assert!(!has_dcim(dir.path()));
        std::fs::write(dir.path().join("DCIM"), b"not a folder").unwrap();
        assert!(!has_dcim(dir.path()));
    }

    #[test]
    fn a_missing_root_does_not_look_like_a_card() {
        assert!(!has_dcim(std::path::Path::new("/does/not/exist")));
    }
}
