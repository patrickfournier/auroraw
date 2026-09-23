// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendering a destination path template (M1 plan §6, item 6): `{year}`, `{month}`, `{day}`,
//! `{date}` (`{year}-{month}-{day}`), `{hour}`, `{minute}`, `{second}`, `{time}` (`{hour}{minute}{second}`),
//! `{seq}` (a sequence number, `{seq:04}` for zero-padded width 4), `{camera}`, `{original}` (the
//! source file's name without its extension), `{ext}` (its extension, lowercase) and `{shoot}` (a
//! session name, given at import time, not stored in the profile: D-029's profile schema does not
//! name one, and it is naturally a per-import choice, like a wedding's name, not a per-profile one).
//!
//! A value that is not known for a file (`{camera}` with no make or model, `{shoot}` when none was
//! given) renders as an empty piece rather than failing the whole import: a slightly odd path is
//! recoverable, a photo the profile refuses to place is not.

use time::OffsetDateTime;

/// What a template is rendered against, for one planned file.
pub struct TemplateContext<'a> {
    /// The capture time, if known; every date and time token is empty without it.
    pub capture_time: Option<OffsetDateTime>,
    /// This file's position in the import, sorted by capture time across every camera on the
    /// card (M1 plan §6, item 6: "several cameras sort by capture time").
    pub sequence: u32,
    /// The camera model, if known.
    pub camera: Option<&'a str>,
    /// The source file's name, without its extension.
    pub original_stem: &'a str,
    /// The source file's extension, as found (rendered lowercase).
    pub extension: &'a str,
    /// A session name given at import time.
    pub shoot: Option<&'a str>,
}

/// Characters not safe in a path component on at least one of the three platforms (Windows is the
/// strictest: `< > : " | ? *` and control characters, plus the separators themselves so a
/// rendered value never accidentally introduces an extra path segment).
fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*/\\".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn field(name: &str, spec: Option<&str>, ctx: &TemplateContext) -> String {
    let date = ctx.capture_time;
    match name {
        "year" => date.map(|d| format!("{:04}", d.year())).unwrap_or_default(),
        "month" => date
            .map(|d| format!("{:02}", u8::from(d.month())))
            .unwrap_or_default(),
        "day" => date.map(|d| format!("{:02}", d.day())).unwrap_or_default(),
        "date" => date
            .map(|d| format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day()))
            .unwrap_or_default(),
        "hour" => date.map(|d| format!("{:02}", d.hour())).unwrap_or_default(),
        "minute" => date
            .map(|d| format!("{:02}", d.minute()))
            .unwrap_or_default(),
        "second" => date
            .map(|d| format!("{:02}", d.second()))
            .unwrap_or_default(),
        "time" => date
            .map(|d| format!("{:02}{:02}{:02}", d.hour(), d.minute(), d.second()))
            .unwrap_or_default(),
        "seq" => {
            let width: usize = spec.and_then(|s| s.parse().ok()).unwrap_or(1);
            format!("{:0width$}", ctx.sequence, width = width)
        }
        "camera" => sanitize(ctx.camera.unwrap_or_default()),
        "original" => sanitize(ctx.original_stem),
        "ext" => ctx.extension.to_ascii_lowercase(),
        "shoot" => sanitize(ctx.shoot.unwrap_or_default()),
        // An unknown token renders as itself, wrapped, rather than silently vanishing: a typo in
        // a template should be visible in the resulting path, not swallowed.
        other => spec.map_or_else(|| format!("{{{other}}}"), |s| format!("{{{other}:{s}}}")),
    }
}

/// Turns rendered template text into a relative path that can only ever stay inside the folder it
/// is joined to: a template is text a person types, and a value it renders can be empty (a photo
/// with no capture time renders `{year}` as nothing, so `{year}/{original}` would become
/// `/IMG.cr3`, an absolute path that `join` would let replace the whole destination). Empty, `.`
/// and `..` components are dropped, every remaining component is sanitised (which also removes a
/// Windows drive prefix's `:`), and if nothing is left `fallback` is used.
pub fn safe_relative(rendered: &str, fallback: &str) -> std::path::PathBuf {
    let path: std::path::PathBuf = rendered
        .split(['/', '\\'])
        .map(|component| sanitize(component).trim_end_matches('.').trim().to_string())
        .filter(|component| !component.is_empty() && component != "." && component != "..")
        .collect();
    if path.as_os_str().is_empty() {
        std::path::PathBuf::from(fallback)
    } else {
        path
    }
}

/// Renders `template` against `ctx`. `{name}` and `{name:spec}` tokens are replaced; text outside
/// braces, including `/` path separators, passes through unchanged.
pub fn render(template: &str, ctx: &TemplateContext) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '{' {
            out.push(c);
            continue;
        }
        let mut token = String::new();
        let mut closed = false;
        for c in chars.by_ref() {
            if c == '}' {
                closed = true;
                break;
            }
            token.push(c);
        }
        if !closed {
            out.push('{');
            out.push_str(&token);
            continue;
        }
        let (name, spec) = match token.split_once(':') {
            Some((n, s)) => (n, Some(s)),
            None => (token.as_str(), None),
        };
        out.push_str(&field(name, spec, ctx));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn ctx(capture_time: Option<OffsetDateTime>) -> TemplateContext<'static> {
        TemplateContext {
            capture_time,
            sequence: 7,
            camera: Some("EOS R5 Mark II"),
            original_stem: "IMG_0042",
            extension: "CR3",
            shoot: Some("Marie wedding"),
        }
    }

    #[test]
    fn renders_the_default_destination_shape() {
        let c = ctx(Some(datetime!(2026-09-23 14:05:09 UTC)));
        assert_eq!(
            render("{year}/{date}/{camera}_{seq:04}_{original}.{ext}", &c),
            "2026/2026-09-23/EOS R5 Mark II_0007_IMG_0042.cr3"
        );
    }

    #[test]
    fn a_missing_capture_time_renders_date_tokens_empty_not_a_failure() {
        let c = ctx(None);
        assert_eq!(render("{year}-{month}-{day}", &c), "--");
    }

    #[test]
    fn sequence_without_a_width_pads_to_one_digit() {
        let c = ctx(Some(datetime!(2026-01-01 0:00 UTC)));
        assert_eq!(render("{seq}", &c), "7");
    }

    #[test]
    fn an_unknown_token_is_left_visible_rather_than_silently_dropped() {
        let c = ctx(None);
        assert_eq!(render("{nope}", &c), "{nope}");
    }

    #[test]
    fn a_camera_name_with_unsafe_characters_is_sanitised() {
        let mut c = ctx(None);
        c.camera = Some("A:B/C");
        assert_eq!(render("{camera}", &c), "A_B_C");
    }

    #[test]
    fn an_unclosed_brace_passes_through() {
        let c = ctx(None);
        assert_eq!(render("weird{oops", &c), "weird{oops");
    }

    #[test]
    fn a_rendered_path_can_never_leave_the_folder_it_is_joined_to() {
        let root = std::path::Path::new("/archive");
        for hostile in [
            "/etc/passwd",
            "//IMG.cr3",
            "../../outside/IMG.cr3",
            "a/../../IMG.cr3",
            "C:\\Windows\\IMG.cr3",
            "C:/IMG.cr3",
            "\\\\server\\share\\IMG.cr3",
            "./IMG.cr3",
        ] {
            let relative = safe_relative(hostile, "fallback.cr3");
            assert!(relative.is_relative(), "{hostile}: {relative:?}");
            assert!(
                relative
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))),
                "{hostile}: {relative:?}"
            );
            assert!(root.join(&relative).starts_with(root), "{hostile}");
        }
    }

    #[test]
    fn a_photo_with_no_capture_time_lands_in_the_folder_above_instead_of_the_filesystem_root() {
        let c = ctx(None);
        assert_eq!(
            safe_relative(&render("{year}/{date}/{original}.{ext}", &c), "x"),
            std::path::PathBuf::from("IMG_0042.cr3")
        );
    }

    #[test]
    fn nothing_left_after_cleaning_falls_back() {
        assert_eq!(
            safe_relative("../..//", "IMG.cr3"),
            std::path::PathBuf::from("IMG.cr3")
        );
    }
}
