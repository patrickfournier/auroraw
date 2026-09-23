// SPDX-License-Identifier: GPL-3.0-or-later
//! Turning the text a person typed, or a dialog returned, into the one path Auroraw stores and
//! compares (M1 plan, workflow revision D-092). A field is text: `~/Images/Photos` typed into one
//! used to create a folder literally named `~` under the current directory, and two spellings of the
//! same folder were two different sources. Every path that enters the engine from a person goes
//! through [`resolve`] first.

use std::path::{Component, Path, PathBuf};

/// Why a typed path cannot be understood.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// Nothing was typed.
    #[error("no folder was entered")]
    Empty,
    /// After expansion the path does not start at a root, so it would depend on the folder the
    /// application happened to be started from.
    #[error("{0} is not an absolute path; start it from the root, or with ~ for the home folder")]
    NotAbsolute(String),
    /// `$NAME`, `${NAME}` or `%NAME%` names a variable that is not set: leaving it as typed would
    /// silently create a folder called `$NAME`.
    #[error("the variable {0} is not set")]
    UnknownVariable(String),
    /// `~name` (another user's home) is not supported.
    #[error("{0} names another user's home folder, which is not supported")]
    OtherUsersHome(String),
    /// `~` was used but this machine has no home folder to expand it to.
    #[error("this machine has no home folder to expand ~ to")]
    NoHome,
}

/// The canonical form of `typed`, reading variables from the environment.
///
/// - surrounding whitespace and one pair of quotes are removed;
/// - a leading `~` becomes the home folder; `$NAME` and `${NAME}` (and `%NAME%` on Windows) become
///   the variable's value, and an unset variable is an error, never left as text;
/// - the result must be absolute;
/// - `.`, `..`, repeated and trailing separators are normalised;
/// - symlinks are resolved for the part of the path that exists, the rest is kept as typed; the
///   folder does not have to exist yet (an archive folder is created on first use);
/// - Windows' `\\?\` prefix is removed.
pub fn resolve(typed: &str) -> Result<PathBuf, PathError> {
    resolve_with(typed, &|name| std::env::var(name).ok())
}

/// As [`resolve`], reading variables through `env` (tests give their own).
pub fn resolve_with(
    typed: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<PathBuf, PathError> {
    let unquoted = unquote(typed.trim()).trim();
    if unquoted.is_empty() {
        return Err(PathError::Empty);
    }
    let expanded = expand(unquoted, env)?;
    let path = PathBuf::from(&expanded);
    if !path.is_absolute() {
        return Err(PathError::NotAbsolute(unquoted.to_string()));
    }
    Ok(strip_verbatim(follow_symlinks(&normalise(&path))))
}

/// One pair of matching quotes around the whole text (what a shell-style copy and paste leaves).
fn unquote(text: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    text
}

fn home(env: &dyn Fn(&str) -> Option<String>) -> Result<String, PathError> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env(name).filter(|h| !h.is_empty()).ok_or(PathError::NoHome)
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn expand(text: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<String, PathError> {
    let mut rest = text;
    let mut out = String::new();

    // A leading `~` is the home folder, only when alone or followed by a separator.
    if let Some(after) = rest.strip_prefix('~') {
        match after.chars().next() {
            None | Some('/') => {
                out.push_str(&home(env)?);
                rest = after;
            }
            Some('\\') if cfg!(windows) => {
                out.push_str(&home(env)?);
                rest = after;
            }
            Some(_) => {
                let name: String = after.chars().take_while(|c| !"/\\".contains(*c)).collect();
                return Err(PathError::OtherUsersHome(format!("~{name}")));
            }
        }
    }

    let mut chars = rest.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '$' => {
                let tail = &rest[i + 1..];
                let (name, consumed) = if let Some(braced) = tail.strip_prefix('{') {
                    match braced.find('}') {
                        Some(end) => (&braced[..end], end + 2),
                        None => {
                            out.push('$');
                            continue;
                        }
                    }
                } else {
                    let end = tail
                        .char_indices()
                        .take_while(|(k, ch)| {
                            if *k == 0 {
                                is_name_start(*ch)
                            } else {
                                ch.is_ascii_alphanumeric() || *ch == '_'
                            }
                        })
                        .last()
                        .map_or(0, |(k, ch)| k + ch.len_utf8());
                    (&tail[..end], end)
                };
                if name.is_empty() {
                    out.push('$');
                    continue;
                }
                let value =
                    env(name).ok_or_else(|| PathError::UnknownVariable(name.to_string()))?;
                out.push_str(&value);
                // Skip what the name used (the iterator is over char indices).
                let resume = i + 1 + consumed;
                while chars.peek().is_some_and(|(k, _)| *k < resume) {
                    chars.next();
                }
            }
            '%' if cfg!(windows) => {
                let tail = &rest[i + 1..];
                match tail.find('%') {
                    Some(end) if end > 0 => {
                        let name = &tail[..end];
                        let value = env(name)
                            .ok_or_else(|| PathError::UnknownVariable(name.to_string()))?;
                        out.push_str(&value);
                        let resume = i + 1 + end + 1;
                        while chars.peek().is_some_and(|(k, _)| *k < resume) {
                            chars.next();
                        }
                    }
                    _ => out.push('%'),
                }
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// `.` and repeated separators dropped, `..` applied lexically (never above the root).
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never pops a root or a drive prefix: `/..` is `/`.
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolves symlinks in the longest existing prefix and keeps the rest as typed.
fn follow_symlinks(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            let mut out = real;
            out.extend(missing.iter().rev());
            return out;
        }
        match (
            existing.file_name().map(|n| n.to_os_string()),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                missing.push(name);
                existing = parent.to_path_buf();
            }
            // Nothing above exists (or the path is only a root): the normalised form stands.
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(windows)]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

#[cfg(not(windows))]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    path
}

/// Whether `inner` is `outer` or lies inside it, compared component by component on canonical
/// paths (`/photos/2026-old` is not inside `/photos/2026`).
pub fn is_inside(inner: &Path, outer: &Path) -> bool {
    inner.starts_with(outer)
}

/// `inner` relative to `outer`, when it is inside it (empty for the same folder).
pub fn relative_to(inner: &Path, outer: &Path) -> Option<PathBuf> {
    inner.strip_prefix(outer).ok().map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine whose home folder is `home` (a real temporary folder, so that resolving symlinks
    /// gives the same answer on every platform) and which has one other variable, `PICTURES`.
    struct Machine {
        _dir: auroraw_testkit::TempDir,
        home: PathBuf,
    }

    impl Machine {
        fn new() -> Self {
            let dir = auroraw_testkit::temp_dir();
            let home = dir.path().canonicalize().unwrap().join("patrick");
            std::fs::create_dir_all(home.join("Images")).unwrap();
            Self { _dir: dir, home }
        }

        fn r(&self, typed: &str) -> Result<PathBuf, PathError> {
            let home = self.home.to_string_lossy().into_owned();
            let pictures = self.home.join("Images").to_string_lossy().into_owned();
            resolve_with(typed, &move |name| match name {
                "HOME" | "USERPROFILE" => Some(home.clone()),
                "PICTURES" => Some(pictures.clone()),
                _ => None,
            })
        }
    }

    #[test]
    fn nothing_typed_is_refused() {
        let m = Machine::new();
        assert_eq!(m.r(""), Err(PathError::Empty));
        assert_eq!(m.r("   "), Err(PathError::Empty));
        assert_eq!(m.r("\"\""), Err(PathError::Empty));
    }

    #[test]
    fn a_relative_path_is_refused_instead_of_depending_on_where_the_app_started() {
        let m = Machine::new();
        for typed in ["Images/Photos", "./Photos", "../Photos"] {
            assert!(
                matches!(m.r(typed), Err(PathError::NotAbsolute(_))),
                "{typed}"
            );
        }
    }

    #[test]
    fn the_home_folder_is_expanded_from_a_leading_tilde() {
        let m = Machine::new();
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            m.r(&format!("~{sep}Images{sep}Photos")).unwrap(),
            m.home.join("Images").join("Photos")
        );
        assert_eq!(m.r("~").unwrap(), m.home);
        assert!(matches!(m.r("~other/x"), Err(PathError::OtherUsersHome(_))));
        assert_eq!(
            resolve_with("~/x", &|_| None),
            Err(PathError::NoHome),
            "no home folder, no guess"
        );
    }

    #[cfg(unix)]
    #[test]
    fn variables_are_expanded_and_an_unset_one_is_an_error_not_a_folder_called_dollar() {
        let m = Machine::new();
        let expected = m.home.join("Images").join("Backup");
        assert_eq!(m.r("$PICTURES/Backup").unwrap(), expected);
        assert_eq!(m.r("${PICTURES}/Backup").unwrap(), expected);
        assert_eq!(
            m.r("/photos/$NOPE/x"),
            Err(PathError::UnknownVariable("NOPE".into()))
        );
        // A `$` that names nothing stays as a character.
        assert!(m.r("/photos/cost$").unwrap().ends_with("cost$"));
        assert!(m.r("/photos/$1").unwrap().ends_with("$1"));
    }

    #[cfg(unix)]
    #[test]
    fn dots_repeated_and_trailing_separators_are_normalised() {
        let m = Machine::new();
        let typed = format!("{}//a/./b/../c///", m.home.display());
        assert_eq!(m.r(&typed).unwrap(), m.home.join("a/c"));
        assert_eq!(
            m.r("/../..").unwrap(),
            PathBuf::from("/"),
            "never above the root"
        );
    }

    #[cfg(unix)]
    #[test]
    fn quotes_and_surrounding_spaces_are_removed() {
        let m = Machine::new();
        let expected = m.home.join("My Photos");
        assert_eq!(
            m.r(&format!("  \"{}/My Photos\"  ", m.home.display()))
                .unwrap(),
            expected
        );
        assert_eq!(
            m.r(&format!("'{}/My Photos'", m.home.display())).unwrap(),
            expected
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_resolved_and_the_part_that_does_not_exist_yet_is_kept() {
        let m = Machine::new();
        std::fs::create_dir_all(m.home.join("real")).unwrap();
        std::os::unix::fs::symlink(m.home.join("real"), m.home.join("link")).unwrap();

        assert_eq!(
            m.r(&format!("{}/link/2026/September", m.home.display()))
                .unwrap(),
            m.home.join("real/2026/September"),
            "two spellings of one folder become one path"
        );
        assert_eq!(
            m.r(&format!("{}/link/../real", m.home.display())).unwrap(),
            m.home.join("real")
        );
    }

    #[test]
    fn one_path_inside_another_is_decided_by_components_not_by_text() {
        let outer = Path::new("/photos/2026");
        assert!(is_inside(Path::new("/photos/2026"), outer));
        assert!(is_inside(Path::new("/photos/2026/September"), outer));
        assert!(!is_inside(Path::new("/photos/2026-old"), outer));
        assert!(!is_inside(Path::new("/photos"), outer));
        assert_eq!(
            relative_to(Path::new("/photos/2026/September/1"), outer),
            Some(PathBuf::from("September/1"))
        );
        assert_eq!(
            relative_to(Path::new("/photos/2026"), outer),
            Some(PathBuf::new())
        );
        assert_eq!(relative_to(Path::new("/photos/2026-old"), outer), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_variables_backslashes_and_the_verbatim_prefix() {
        let m = Machine::new();
        assert_eq!(
            m.r(r"%PICTURES%\Backup").unwrap(),
            m.home.join("Images").join("Backup")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\C:\Photos")),
            PathBuf::from(r"C:\Photos")
        );
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\UNC\server\share\x")),
            PathBuf::from(r"\\server\share\x")
        );
        assert!(matches!(
            m.r(r"Photos\2026"),
            Err(PathError::NotAbsolute(_))
        ));
    }
}
