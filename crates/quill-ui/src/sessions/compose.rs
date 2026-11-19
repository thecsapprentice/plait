//! Writing in a session (3f-2): the editor, sending through the outbox,
//! drafts kept as you type, and notes waiting to be sent.
//!
//! The editor follows the 3a spike (spikes/gui/FINDINGS.md): the text area
//! is uncontrolled and only replaced by remounting it (findings 1 and 2);
//! the Live form draws the styled source behind it (finding 3); the preview
//! and the draft's saving run a few times a second, not per key (finding
//! 5). See docs/plan.md, "Writing in the desktop app (3f-2)".

use std::rc::Rc;

use dioxus::prelude::*;
use uuid::Uuid;

use super::{live::Mirror, reload};
use crate::{
    app::{Code, ProblemLine, use_ui},
    backend::{Draft, NOTE_MAX, Problem, QueuedNote, Sending},
    markdown::Markdown,
};

/// How often (ms) the preview catches up and the draft is saved.
const TICK_MS: u64 = 300;
/// The limit is said once this close to it.
const NEAR_LIMIT: usize = 2_000;

/// Keeps the caret in view in the Live editor: the text area grows with its
/// text inside a scrolling box, which the webview doesn't scroll for it.
/// Installed once on the document, so it survives remounts.
const FOLLOW_JS: &str = r#"
  if (!window.__quillFollow) {
    window.__quillFollow = true;
    const follow = (e) => {
      const t = e.target;
      if (!t || !t.classList || !t.classList.contains('over')) return;
      requestAnimationFrame(() => {
        const box = t.closest('.live');
        const m = box && box.querySelector('.mirror');
        if (!m) return;
        const line = t.value.slice(0, t.selectionEnd).split('\n').length - 1;
        const el = m.children[line];
        if (!el) return;
        const top = el.offsetTop, bottom = top + el.offsetHeight;
        if (top < box.scrollTop) box.scrollTop = top;
        else if (bottom > box.scrollTop + box.clientHeight) box.scrollTop = bottom - box.clientHeight + 8;
      });
    };
    for (const kind of ['keyup', 'input', 'click']) document.addEventListener(kind, follow, true);
  }
"#;

/// The command key's name here, for hints.
fn command_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl"
    }
}

/// UTF-16 offsets (what the webview reports) as byte offsets in `s`.
fn byte_at(s: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16();
    }
    s.len()
}

/// What the writer is told about the limit, if they're near or past it.
pub(crate) fn limit_line(chars: usize) -> Option<(bool, String)> {
    let fmt = |n: usize| {
        let s = n.to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        out
    };
    let count = |n: usize| {
        if n == 1 {
            "1 character".to_owned()
        } else {
            format!("{} characters", fmt(n))
        }
    };
    if chars > NOTE_MAX {
        Some((
            true,
            format!(
                "{} over the limit of {}. Split it into two notes to send it.",
                count(chars - NOTE_MAX),
                fmt(NOTE_MAX)
            ),
        ))
    } else if chars + NEAR_LIMIT > NOTE_MAX {
        Some((false, format!("{} left.", count(NOTE_MAX - chars))))
    } else {
        None
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Form {
    /// The source styled in place, behind a transparent text area.
    Live,
    /// A plain text area, with the rendered note beside it if wanted.
    Preview,
}

/// What happened to the last note sent.
#[derive(Clone, PartialEq, Debug)]
enum Said {
    Sent,
    Waiting,
    Refused,
    Failed(Problem),
}

/// The editor for one channel of a session: keyed by channel, so each
/// channel's draft loads when it's shown.
#[component]
pub fn Composer(
    id: String,
    channel: String,
    /// The note being answered, shared with the notes' "Answer" buttons.
    answering: Signal<Option<u64>>,
    /// "@wren: The lamp went out" for the note being answered.
    answering_line: Option<String>,
    /// Bumped to put the caret in the editor (after "Answer").
    focus: Signal<u64>,
    /// Bumped when what's kept here changes (a draft begun or gone, a note
    /// queued), so the page reads it again.
    here: Signal<u64>,
    /// "@wren", for the line focus mode shows instead of the page.
    with: String,
) -> Element {
    let (backend, mut model) = use_ui();
    // A memo, so other changes to the model don't re-render the editor.
    let focused = use_memo(move || model.read().sessions.focus)();
    let label = super::reading::tab_label(&channel);

    // The draft as last typed, and whether it's changed since it was kept.
    let b = backend.clone();
    let (sid, ch) = (id.clone(), channel.clone());
    let mut text = use_signal(move || {
        let d = b.draft(&sid, &ch).unwrap_or_default();
        answering.clone().set(d.reply_to);
        d.text
    });
    let mut dirty = use_signal(|| false);
    // Whether a draft is kept, to tell the page when that changes.
    let mut kept = use_signal(|| !text.peek().trim().is_empty() || answering.peek().is_some());
    // Bumped when the app replaces the text (sending, a shortcut): the text
    // area remounts with the new text.
    let mut generation = use_signal(|| 0u64);
    // The writer chooses the form; it never changes by itself (a switch
    // mid-paste remounted the editor at its starting size).
    let mut form = use_signal(|| Form::Live);
    let mut preview = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut said = use_signal(|| None::<Said>);
    let mut editor = use_signal(|| None::<Rc<MountedData>>);
    // After a shortcut remounts the text area: the selection to put back,
    // once the new one is in the page (finding 2).
    let mut reselect = use_signal(|| None::<(usize, usize)>);

    use_hook(|| {
        spawn(async move {
            let _ = document::eval(FOLLOW_JS).await;
        });
    });
    // An answer chosen or dropped is part of the draft.
    use_effect(move || {
        let _ = answering();
        dirty.set(true);
    });
    // After "Answer", the caret goes to the editor.
    use_effect(move || {
        if focus() > 0
            && let Some(el) = editor.peek().clone()
        {
            spawn(async move {
                let _ = el.set_focus(true).await;
            });
        }
    });

    // A few times a second: keep the draft, and let the preview catch up.
    let save = {
        let (b, sid, ch) = (backend.clone(), id.clone(), channel.clone());
        move || {
            if *dirty.peek() {
                let d = Draft {
                    text: text.peek().clone(),
                    reply_to: *answering.peek(),
                };
                if b.save_draft(&sid, &ch, &d).is_ok() {
                    dirty.set(false);
                    let now = !d.text.trim().is_empty() || d.reply_to.is_some();
                    if now != *kept.peek() {
                        kept.set(now);
                        here.clone().with_mut(|n| *n += 1);
                    }
                }
            }
        }
    };
    let tick_save = save.clone();
    let b = backend.clone();
    use_future(move || {
        let (mut save, b) = (tick_save.clone(), b.clone());
        async move {
            loop {
                b.sleep(TICK_MS).await;
                save();
                if *form.peek() == Form::Preview && *preview.peek() != *text.peek() {
                    preview.set(text.peek().clone());
                }
            }
        }
    });
    // Closing the window or leaving the session keeps the last keys typed.
    let drop_save = save.clone();
    use_drop(drop_save);

    let chars = text.read().chars().count();
    let over = chars > NOTE_MAX;
    let empty = text.read().trim().is_empty();

    let send = {
        let (b, sid, ch) = (backend.clone(), id.clone(), channel.clone());
        move || {
            let t = text.peek().clone();
            if t.trim().is_empty() || t.chars().count() > NOTE_MAX || *sending.peek() {
                return;
            }
            let reply = *answering.peek();
            let (b, sid, ch) = (b.clone(), sid.clone(), ch.clone());
            sending.set(true);
            said.set(None);
            spawn(async move {
                match b.send_note(&sid, &ch, &t, reply).await {
                    // It's in the outbox now, whatever came of it: the editor
                    // starts again.
                    Ok(how) => {
                        let _ = b.save_draft(&sid, &ch, &Draft::default());
                        text.set(String::new());
                        preview.set(String::new());
                        dirty.set(false);
                        kept.set(false);
                        answering.set(None);
                        *generation.write() += 1;
                        *here.write() += 1;
                        said.set(Some(match how {
                            Sending::Sent => Said::Sent,
                            Sending::Waiting => Said::Waiting,
                            Sending::Refused(..) => Said::Refused,
                        }));
                        reload(b.clone(), model);
                    }
                    Err(p) => said.set(Some(Said::Failed(p))),
                }
                sending.set(false);
            });
        }
    };

    // Wraps the selection in `mark` (bold, italic), keeping it selected.
    let wrap = move |mark: &'static str| {
        spawn(async move {
            let Ok((a, z)) = document::eval(
                "const t = document.activeElement; return t && t.selectionStart != null ? [t.selectionStart, t.selectionEnd] : [0, 0];",
            )
            .join::<(usize, usize)>()
            .await
            else {
                return;
            };
            let old = text.peek().clone();
            let (i, j) = (byte_at(&old, a), byte_at(&old, z));
            text.set(format!(
                "{}{mark}{}{mark}{}",
                &old[..i],
                &old[i..j],
                &old[j..]
            ));
            dirty.set(true);
            let n = mark.encode_utf16().count();
            reselect.set(Some((a + n, z + n)));
            *generation.write() += 1;
        });
    };
    // The text area just mounted: keep it, and after a shortcut give it
    // back the caret and the selection.
    let mounted = move |e: MountedEvent| {
        let el = e.data();
        editor.set(Some(el.clone()));
        if let Some((a, z)) = reselect.take() {
            spawn(async move {
                let _ = el.set_focus(true).await;
                let _ = document::eval(&format!(
                    "const t = document.querySelector('.note-editor'); if (t) t.setSelectionRange({a}, {z});"
                ))
                .await;
            });
        }
    };

    let mut send_on_key = send.clone();
    let on_keys = move |e: KeyboardEvent| {
        let m = e.modifiers();
        if e.key() == Key::Escape && model.peek().sessions.focus {
            e.prevent_default();
            model.write().sessions.focus = false;
            return;
        }
        if !(m.ctrl() || m.meta()) {
            return;
        }
        match e.key() {
            Key::Enter => {
                e.prevent_default();
                send_on_key();
            }
            Key::Character(c) if c.eq_ignore_ascii_case("b") => {
                e.prevent_default();
                wrap("**");
            }
            Key::Character(c) if c.eq_ignore_ascii_case("i") => {
                e.prevent_default();
                wrap("*");
            }
            _ => {}
        }
    };
    let on_input = move |e: FormEvent| {
        text.set(e.value());
        dirty.set(true);
        // "Sent." was about the last note; this is a new one.
        if matches!(*said.peek(), Some(Said::Sent)) {
            said.set(None);
        }
    };

    let key = format!("{}-{:?}", generation(), form());
    let placeholder = format!(
        "Write in {label}… Markdown works; {}+Enter sends.",
        command_key()
    );
    let editor_id = "note-editor";
    let limit = limit_line(chars);
    let mut send_now = send.clone();
    rsx! {
        section { class: "composer", aria_label: "Write in {label}",
            if focused {
                p { class: "focus-where", "Writing in {label} with {with}" }
            }
            if let Some(line) = answering_line {
                div { class: "answering-now",
                    span { "In answer to {line}" }
                    button {
                        class: "quiet",
                        onclick: move |_| answering.set(None),
                        "Stop answering"
                    }
                }
            }
            if form() == Form::Live {
                div { class: "live",
                    Mirror { source: text() }
                    // A list of one, keyed: a new key remounts it (finding 2).
                    for k in [key.clone()] {
                        textarea {
                            key: "{k}",
                            id: editor_id,
                            class: "writing over note-editor",
                            aria_label: "Note in {label}",
                            placeholder: "{placeholder}",
                            spellcheck: "true",
                            // No browser writing suggestions, which may send
                            // what's typed to their maker (finding 9).
                            "writingsuggestions": "false",
                            autocomplete: "off",
                            initial_value: "{text.peek()}",
                            oninput: on_input,
                            onkeydown: on_keys.clone(),
                            onmounted: mounted,
                        }
                    }
                }
            } else {
                div { class: "split",
                    for k in [key.clone()] {
                        textarea {
                            key: "{k}",
                            id: editor_id,
                            class: "writing plain note-editor",
                            aria_label: "Note in {label}",
                            placeholder: "{placeholder}",
                            spellcheck: "true",
                            "writingsuggestions": "false",
                            autocomplete: "off",
                            initial_value: "{text.peek()}",
                            oninput: on_input,
                            onkeydown: on_keys.clone(),
                            onmounted: mounted,
                        }
                    }
                    div { class: "preview pitch", aria_label: "Preview", Markdown { source: preview() } }
                }
            }
            div { class: "composer-bar",
                div { class: "composer-tools",
                    button {
                        class: "quiet",
                        id: "writing-focus",
                        aria_pressed: if focused { "true" } else { "false" },
                        title: if focused { "Esc leaves focus" } else { "Only the editor" },
                        onclick: move |_| {
                            let f = !model.peek().sessions.focus;
                            model.write().sessions.focus = f;
                            if let Some(el) = editor.peek().clone() {
                                spawn(async move {
                                    let _ = el.set_focus(true).await;
                                });
                            }
                        },
                        if focused { "Leave focus" } else { "Focus" }
                    }
                    if form() == Form::Live {
                        button {
                            class: "quiet",
                            onclick: move |_| {
                                preview.set(text.peek().clone());
                                form.set(Form::Preview);
                            },
                            "Write and preview"
                        }
                    } else {
                        button {
                            class: "quiet",
                            onclick: move |_| form.set(Form::Live),
                            "Write in place"
                        }
                    }
                }
                if let Some((bad, line)) = limit {
                    span { class: if bad { "limit over" } else { "limit" }, role: "status", "{line}" }
                }
                button {
                    class: "primary",
                    disabled: empty || over || sending(),
                    onclick: move |_| send_now(),
                    if sending() { "Sending…" } else { "Send" }
                }
            }
            match said() {
                Some(Said::Sent) => rsx! { p { class: "hint", role: "status", "Sent." } },
                Some(Said::Waiting) => rsx! {
                    p { class: "hint", role: "status",
                        "Not sent yet: it waits on this computer and goes once the server can be reached."
                    }
                },
                Some(Said::Refused) => rsx! {
                    p { class: "hint", role: "status",
                        "The server didn't take it. It waits below, to change or withdraw."
                    }
                },
                Some(Said::Failed(p)) => rsx! { ProblemLine { problem: p } },
                None => rsx! {},
            }
        }
    }
}

/// A note written here that hasn't gone yet: offline, or refused. It can
/// be changed or withdrawn until it goes.
#[component]
pub fn WaitingNote(id: String, note: QueuedNote, here: Signal<u64>) -> Element {
    let mut here = here;
    let (backend, model) = use_ui();
    let advanced = model.read().settings.advanced;
    let mut changing = use_signal(|| false);
    let mut withdrawing = use_signal(|| false);
    let mut edited = use_signal(|| note.text.clone());
    let mut problem = use_signal(|| None::<Problem>);
    let qid: Uuid = note.id;

    let b = backend.clone();
    let sid = id.clone();
    let save = move |_| match b.change_waiting(&sid, qid, &edited.peek()) {
        Ok(()) => {
            changing.set(false);
            *here.write() += 1;
        }
        Err(p) => problem.set(Some(p)),
    };
    let b = backend.clone();
    let sid = id.clone();
    let withdraw = move |_| {
        match b.withdraw_waiting(&sid, qid) {
            Ok(()) | Err(Problem::Gone) => {}
            Err(p) => {
                problem.set(Some(p));
                return;
            }
        }
        *here.write() += 1;
    };
    let b = backend.clone();
    let sid = id.clone();
    let retry = move |_| {
        let (b, sid) = (b.clone(), sid.clone());
        spawn(async move {
            if let Err(p) = b.send_waiting(&sid).await {
                problem.set(Some(p));
            }
            *here.write() += 1;
            reload(b, model);
        });
    };
    let field = format!("waiting-{qid}");
    rsx! {
        li { class: if note.refused.is_some() { "note mine waiting refused" } else { "note mine waiting" },
            header {
                span { class: "name", "You" }
                span { class: "tag", if note.refused.is_some() { "Not sent" } else { "Waiting to send" } }
            }
            if let Some((code, detail)) = &note.refused {
                p { class: "hint",
                    "The server didn't take it: {detail}"
                    if advanced {
                        " "
                        Code { code: *code }
                    }
                }
            }
            if changing() {
                textarea {
                    id: "{field}",
                    class: "writing plain",
                    aria_label: "Change the waiting note",
                    spellcheck: "true",
                    "writingsuggestions": "false",
                    autocomplete: "off",
                    initial_value: "{note.text}",
                    oninput: move |e| edited.set(e.value()),
                }
                div { class: "actions",
                    button { class: "primary", onclick: save, "Save the change" }
                    button { class: "quiet", onclick: move |_| changing.set(false), "Cancel" }
                }
            } else if withdrawing() {
                div { class: "pitch", Markdown { source: note.text.clone() } }
                p { "Withdraw this note? It won't be sent, and its text goes." }
                div { class: "actions",
                    button { class: "danger", onclick: withdraw, "Withdraw it" }
                    button { class: "quiet", onclick: move |_| withdrawing.set(false), "Keep it" }
                }
            } else {
                div { class: "pitch", Markdown { source: note.text.clone() } }
                div { class: "actions",
                    button { class: "secondary", onclick: retry, "Try again" }
                    button { class: "quiet", onclick: move |_| changing.set(true), "Change" }
                    button { class: "quiet", onclick: move |_| withdrawing.set(true), "Withdraw…" }
                }
            }
            if let Some(p) = problem() {
                ProblemLine { problem: p }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_said_near_it_and_past_it() {
        assert_eq!(limit_line(100), None);
        assert_eq!(
            limit_line(NOTE_MAX - 1),
            Some((false, "1 character left.".into()))
        );
        assert_eq!(
            limit_line(NOTE_MAX - 500),
            Some((false, "500 characters left.".into()))
        );
        assert_eq!(
            limit_line(NOTE_MAX + 1_234),
            Some((
                true,
                "1,234 characters over the limit of 20,000. Split it into two notes to send it."
                    .into()
            ))
        );
    }

    #[test]
    fn selections_count_in_utf16_units() {
        let s = "🎃 ink";
        assert_eq!(byte_at(s, 2), "🎃".len());
        assert_eq!(byte_at(s, 99), s.len());
    }
}
