//! The Live editor's mirror (3f-2): a note's Markdown source, styled in
//! place, drawn behind a transparent text area. Every source character is
//! kept, marks included, and no style changes a character's width, so the
//! styled text lines up with the text area's (spike finding 3). Each line
//! is its own block, so typing re-lays out one line, not the note.

use dioxus::prelude::*;

/// The classes the mirror's spans use, each styled only inside `.mirror`
/// so no other rule changes a character's width.
pub const CLASSES: [&str; 7] = [
    "md-mark",
    "md-strong",
    "md-em",
    "md-code",
    "md-link",
    "md-heading",
    "md-quote",
];

/// A run of source characters and how it's styled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// One of [`CLASSES`] (`md-mark` for a mark like `**` or `#`), or
    /// empty for plain text.
    pub class: &'static str,
    pub text: String,
}

fn push(out: &mut Vec<Span>, class: &'static str, text: &str) {
    debug_assert!(class.is_empty() || CLASSES.contains(&class), "{class}");
    if text.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(last) if last.class == class => last.text.push_str(text),
        _ => out.push(Span {
            class,
            text: text.to_owned(),
        }),
    }
}

/// Inline marks in one line: `**strong**`, `*em*` or `_em_`, `` `code` ``
/// and `[links](url)`. `base` styles the text between them (a heading's
/// or a quote's colour).
fn inline(line: &str, base: &'static str, out: &mut Vec<Span>) {
    let chars: Vec<char> = line.chars().collect();
    let text = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let find = |from: usize, pat: &[char]| {
        (from..=chars.len().saturating_sub(pat.len())).find(|&i| chars[i..].starts_with(pat))
    };
    let mut i = 0;
    while i < chars.len() {
        let rest = &chars[i..];
        // (marks before, body class, marks after)
        let found = if rest.starts_with(&['*', '*']) {
            find(i + 2, &['*', '*'])
                .filter(|&e| e > i + 2)
                .map(|e| (2, "md-strong", e, 2))
        } else if rest.starts_with(&['`']) {
            find(i + 1, &['`'])
                .filter(|&e| e > i + 1)
                .map(|e| (1, "md-code", e, 1))
        } else if (rest[0] == '*' || rest[0] == '_')
            && chars.get(i + 1).is_some_and(|c| !c.is_whitespace())
        {
            find(i + 1, &[rest[0]])
                .filter(|&e| e > i + 1)
                .map(|e| (1, "md-em", e, 1))
        } else if rest[0] == '[' {
            find(i + 1, &[']', '('])
                .and_then(|mid| find(mid + 2, &[')']).map(|end| (1, "md-link", mid, end + 1 - mid)))
        } else {
            None
        };
        match found {
            Some((open, class, body_end, close)) => {
                push(out, "md-mark", &text(i, i + open));
                push(out, class, &text(i + open, body_end));
                push(out, "md-mark", &text(body_end, body_end + close));
                i = body_end + close;
            }
            None => {
                push(out, base, &rest[0].to_string());
                i += 1;
            }
        }
    }
}

fn style_line(line: &str, fenced: &mut bool) -> Vec<Span> {
    let mut out = Vec::new();
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    if trimmed.starts_with("```") {
        *fenced = !*fenced;
        push(&mut out, "md-mark", line);
        return out;
    }
    if *fenced {
        push(&mut out, "md-code", line);
        return out;
    }
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        push(&mut out, "md-mark", &line[..indent + hashes]);
        inline(&line[indent + hashes..], "md-heading", &mut out);
        return out;
    }
    if trimmed.starts_with('>') {
        push(&mut out, "md-mark", &line[..indent + 1]);
        inline(&line[indent + 1..], "md-quote", &mut out);
        return out;
    }
    inline(line, "", &mut out);
    out
}

/// The source styled, one list of spans per line (without its newline).
pub fn styled_lines(source: &str) -> Vec<Vec<Span>> {
    let mut fenced = false;
    source
        .split('\n')
        .map(|line| style_line(line, &mut fenced))
        .collect()
}

/// The mirror: hidden from screen readers, which read the text area.
#[component]
pub fn Mirror(source: String) -> Element {
    let lines = styled_lines(&source);
    rsx! {
        div { class: "writing mirror", aria_hidden: "true",
            for (i, line) in lines.into_iter().enumerate() {
                div { key: "{i}", class: "line",
                    if line.is_empty() {
                        // An empty line keeps its height.
                        " "
                    }
                    for (j, s) in line.into_iter().enumerate() {
                        span { key: "{j}", class: s.class, "{s.text}" }
                    }
                }
            }
            // A last newline needs something after it to take up a line.
            " "
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined(src: &str) -> String {
        styled_lines(src)
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn styling_keeps_every_character() {
        for src in [
            "# The ledger\n\nIt was **still** wet, *wasn't* it? `ink` and [a map](http://x).\n> said the keeper\n```\ncode <b>\n```\n",
            "**unclosed and * stray _ marks [ and ](",
            "émoji 🎃 and 漢字 *mixed*",
            "",
            "\n\n",
        ] {
            assert_eq!(joined(src), src, "{src:?}");
        }
    }

    #[test]
    fn marks_and_their_text_are_told_apart() {
        let line = &styled_lines("## A **bold** _turn_")[0];
        let classes: Vec<(&str, &str)> = line.iter().map(|s| (s.class, s.text.as_str())).collect();
        assert_eq!(
            classes,
            [
                ("md-mark", "##"),
                ("md-heading", " A "),
                ("md-mark", "**"),
                ("md-strong", "bold"),
                ("md-mark", "**"),
                ("md-heading", " "),
                ("md-mark", "_"),
                ("md-em", "turn"),
                ("md-mark", "_"),
            ]
        );
    }
}
