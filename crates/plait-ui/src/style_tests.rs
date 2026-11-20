//! Checks on the stylesheet's tokens (docs/design/gui-design-language.md):
//! contrast in both themes, and the three copies of each theme agreeing.

use std::collections::HashMap;

/// The `--name: #rrggbb;` colours in the block that starts with `selector {`.
fn colours(selector: &str) -> HashMap<String, String> {
    let css = crate::STYLE;
    let start = css
        .find(&format!("{selector} {{"))
        .unwrap_or_else(|| panic!("no {selector} block"));
    let body = &css[start..];
    let body = &body[..body.find('}').unwrap_or(body.len())];
    body.lines()
        .filter_map(|l| {
            let (name, value) = l.trim().strip_prefix("--")?.split_once(':')?;
            let value = value.trim().trim_end_matches(';');
            value
                .starts_with('#')
                .then(|| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

fn luminance(hex: &str) -> f64 {
    let hex = hex.trim_start_matches('#');
    let channel = |i: usize| {
        let c = f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0)) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
}

fn contrast(a: &str, b: &str) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Text colours and what they sit on; control edges and what they sit on.
const TEXT: [(&str, &str); 13] = [
    ("ink", "paper"),
    ("ink", "surface"),
    ("ink", "accent-soft"),
    ("ink", "warn-soft"),
    ("muted", "paper"),
    ("muted", "surface"),
    ("muted", "accent-soft"),
    ("accent", "paper"),
    ("accent", "surface"),
    ("accent-ink", "accent"),
    ("warn", "warn-soft"),
    ("bad", "surface"),
    ("bad-ink", "bad"),
];
const EDGES: [(&str, &str); 4] = [
    ("edge", "paper"),
    ("edge", "surface"),
    ("accent", "paper"),
    ("accent", "surface"),
];

fn check(theme: &str, c: &HashMap<String, String>) {
    for (fg, bg) in TEXT {
        let r = contrast(&c[fg], &c[bg]);
        assert!(
            r >= 4.5,
            "{theme}: --{fg} on --{bg} is {r:.2}:1; text needs 4.5:1"
        );
    }
    for (fg, bg) in EDGES {
        let r = contrast(&c[fg], &c[bg]);
        assert!(
            r >= 3.0,
            "{theme}: --{fg} on --{bg} is {r:.2}:1; a control's edge needs 3:1"
        );
    }
}

#[test]
fn colours_meet_wcag_aa_in_both_themes() {
    check("light", &colours(":root"));
    check("dark", &colours(".plait[data-theme=\"dark\"]"));
}

#[test]
fn each_theme_says_the_same_wherever_it_is_written() {
    // Light is on :root and again for the light setting; dark is under the
    // system's dark setting and again for the dark setting.
    assert_eq!(colours(":root"), colours(".plait[data-theme=\"light\"]"));
    assert_eq!(
        colours("  :root, .plait:not([data-theme=\"light\"])"),
        colours(".plait[data-theme=\"dark\"]")
    );
}

/// Found by hand (#69): `.sr-only` text inside the scrolling `.main` was
/// placed against the window, not `.main`, so it stretched the window and
/// the whole frame slid up past the end of a long page. `.main` must be
/// the containing block for anything positioned inside it.
#[test]
fn hidden_text_stays_inside_the_scrolling_page() {
    let rule = crate::STYLE
        .lines()
        .find(|l| l.starts_with(".main {"))
        .expect("a .main rule");
    assert!(rule.contains("position: relative"), "{rule}");
}

/// Found by hand (3f-1): a channel tab's unread badge sat flush against
/// its name ("Planning1"), since the badge's leading space collapses.
/// The badge needs its own space before it.
#[test]
fn a_tabs_unread_badge_keeps_apart_from_its_name() {
    let rule = crate::STYLE
        .lines()
        .find(|l| l.starts_with(".channels .badge {"))
        .expect("a .channels .badge rule");
    assert!(rule.contains("margin-left"), "{rule}");
}

/// Found by hand (3f-2): the Live mirror's `code` span took the app's
/// `.code` style (smaller, monospace), so its characters were narrower than
/// the text area's and the two drifted apart. The mirror's classes are its
/// own: the stylesheet styles them only inside `.mirror`.
#[test]
fn the_live_mirror_s_classes_are_styled_only_inside_it() {
    let classes = crate::sessions::live::CLASSES;
    for class in classes {
        let needle = format!(".{class}");
        let mut at = 0;
        while let Some(i) = crate::STYLE[at..].find(&needle) {
            let start = at + i;
            let end = start + needle.len();
            let next = crate::STYLE[end..].chars().next().unwrap_or(' ');
            if !(next.is_alphanumeric() || next == '-' || next == '_') {
                let before = &crate::STYLE[..start];
                assert!(
                    before.ends_with(".mirror "),
                    "`{needle}` is styled outside the mirror"
                );
            }
            at = end;
        }
    }
}

/// Found by hand (3f-2): in focus mode the editor sat at the left of the
/// window, held there by the page column's width. The page widens, so the
/// editor's own width and auto margins centre it.
#[test]
fn focus_mode_centres_the_editor() {
    let rule = |sel: &str| {
        crate::STYLE
            .lines()
            .find(|l| l.starts_with(&format!("{sel} {{")))
            .unwrap_or_else(|| panic!("a `{sel}` rule"))
    };
    assert!(rule(".frame.focus .page").contains("max-width: none"));
    assert!(rule(".frame.focus .composer").contains("margin: var(--space-6) auto 0"));
}

/// Found by hand (3f-2): on a narrow window, focus mode hid the side bar
/// but kept the narrow layout's three rows, so the status bar took the
/// stretching one. Focus mode sets its own two rows.
#[test]
fn focus_mode_keeps_the_status_bar_small_when_narrow() {
    let rule = crate::STYLE
        .lines()
        .find(|l| l.starts_with(".frame.focus {"))
        .expect("a .frame.focus rule");
    assert!(
        rule.contains("grid-template-rows: minmax(0, 1fr) auto"),
        "{rule}"
    );
}

/// Found by hand (3f-2): a wide window left the page at its left edge, and
/// a session no wider than any page, so Write and preview had no room for
/// its two columns. Pages centre; a session takes more width.
#[test]
fn pages_centre_and_a_session_has_room() {
    let rule = |sel: &str| {
        crate::STYLE
            .lines()
            .find(|l| l.starts_with(&format!("{sel} {{")))
            .unwrap_or_else(|| panic!("a `{sel}` rule"))
    };
    assert!(rule(".page").contains("margin-inline: auto"));
    assert!(rule(".page.session").contains("var(--width-write)"));
}

/// Found by hand (3f-2): in Write and preview the preview's first paragraph
/// kept its top margin, so its first line sat lower than the editor's
/// beside it. The first block inside it starts flush.
#[test]
fn the_side_by_side_preview_lines_up_with_the_editor() {
    assert!(
        crate::STYLE.lines().any(
            |l| l.starts_with(".split .preview .markdown > :first-child {")
                && l.contains("margin-top: 0")
        ),
        "no first-child rule for the side-by-side preview"
    );
}

/// Found by hand (3f-2): Write and preview split Live's width in two. When
/// there's room, the composer and the session page widen so each box is as
/// wide as Live's widest, in focus mode too.
#[test]
fn write_and_preview_boxes_are_each_as_wide_as_live() {
    let rule = |sel: &str| {
        crate::STYLE
            .lines()
            .find(|l| l.starts_with(&format!("{sel} {{")))
            .unwrap_or_else(|| panic!("a `{sel}` rule"))
    };
    assert!(rule(".composer").contains("max-width: var(--width-write)"));
    assert!(rule(".composer:has(.split)").contains("2 * var(--width-write)"));
    assert!(rule(".page.session:has(.composer .split)").contains("2 * var(--width-write)"));
    assert!(rule(".frame.focus .composer").contains("var(--width-focus)"));
    assert!(rule(".frame.focus .composer:has(.split)").contains("2 * var(--width-focus)"));
}

/// Found by hand (3f-2): Write and preview's text area stayed a fixed
/// height while Live grew with its text. The webview sizes it, up to
/// Live's height, so typing does no more work than before.
#[test]
fn the_plain_editor_grows_like_live() {
    let rule = crate::STYLE
        .lines()
        .find(|l| l.starts_with(".split textarea {"))
        .expect("a `.split textarea` rule");
    assert!(rule.contains("field-sizing: content"), "{rule}");
    assert!(rule.contains("max-height: 60vh"), "{rule}");
}

/// Found by hand (3f-3): the session page's new controls crowded their
/// neighbours (a channel's description and its form on the first note,
/// the history's caption on the undo line) or didn't match them ("Change
/// the description" larger than the description; "Leave the session…"
/// indented and larger than the safety-numbers link above it).
#[test]
fn the_session_pages_controls_keep_their_place() {
    let rule = |sel: &str| {
        crate::STYLE
            .lines()
            .find(|l| l.starts_with(&format!("{sel} {{")))
            .unwrap_or_else(|| panic!("a `{sel}` rule"))
    };
    assert!(rule(".channel-about").contains("margin-bottom"));
    assert!(rule(".channel-form").contains("margin-bottom"));
    assert!(rule(".channel-about .link").contains("font-size: var(--text-sm)"));
    assert!(rule(".versions-caption").contains("margin-top"));
    let leave = rule(".session-actions .quiet");
    assert!(
        leave.contains("padding: 0") && leave.contains("font-size: inherit"),
        "{leave}"
    );
    assert!(rule(".session-actions").contains("font-size: var(--text-sm)"));
}
