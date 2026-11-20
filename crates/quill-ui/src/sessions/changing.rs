//! Changing your own notes (3f-3): editing one in place, deleting it, and
//! undoing either, rather than asking first (design language, "Anything
//! that can be undone doesn't ask"). A delete hides a note but erases
//! nothing, so its history stays on request for both writers, and undoing
//! it sends the last text again (#104; docs/plan.md, "Full edit history").

use dioxus::prelude::*;

use super::{compose::limit_line, reading::when, reload};
use crate::{
    app::{ProblemLine, use_ui},
    backend::{NoteVersion, Problem},
    markdown::Markdown,
};

/// The writer's last change to a note on this page, to undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Last {
    /// Edited from `before`; undoing sends `before` again.
    Edited {
        before: String,
    },
    Deleted,
    Undone,
}

/// One of your notes, open to change: Save, Cancel, or Delete. Ctrl+Enter
/// saves and Esc cancels. `on_done` gets the change made, or `None`.
#[component]
pub fn NoteEditor(
    id: String,
    seq: u64,
    content: String,
    on_done: EventHandler<Option<Last>>,
) -> Element {
    let (backend, model) = use_ui();
    let mut text = use_signal(|| content.clone());
    let mut busy = use_signal(|| false);
    let mut problem = use_signal(|| None::<Problem>);
    let before = content.clone();
    let trimmed = text().trim().to_owned();
    let limit = limit_line(trimmed.chars().count());
    let over = limit.as_ref().is_some_and(|(over, _)| *over);
    let can_save = !busy() && !trimmed.is_empty() && trimmed != before.trim() && !over;

    let b = backend.clone();
    let sid = id.clone();
    let was = before.clone();
    let save = move || {
        if !can_save {
            return;
        }
        busy.set(true);
        let (b, sid, was) = (b.clone(), sid.clone(), was.clone());
        let new = text.peek().trim().to_owned();
        spawn(async move {
            let result = b.edit_note(&sid, seq, &new).await;
            busy.set(false);
            match result {
                Ok(()) => {
                    reload(b, model);
                    on_done.call(Some(Last::Edited { before: was }));
                }
                Err(p) => problem.set(Some(p)),
            }
        });
    };
    let b = backend.clone();
    let sid = id.clone();
    let delete = move |_| {
        busy.set(true);
        let (b, sid) = (b.clone(), sid.clone());
        spawn(async move {
            let result = b.delete_note(&sid, seq).await;
            busy.set(false);
            match result {
                Ok(()) => {
                    reload(b, model);
                    on_done.call(Some(Last::Deleted));
                }
                Err(p) => problem.set(Some(p)),
            }
        });
    };
    let field = format!("edit-{seq}");
    let mut on_key = save.clone();
    let mut on_click = save;
    rsx! {
        div { class: "note-change",
            textarea {
                id: "{field}",
                class: "writing plain",
                aria_label: "Change your note",
                spellcheck: "true",
                "writingsuggestions": "false",
                autocomplete: "off",
                initial_value: "{content}",
                onmounted: move |e: MountedEvent| async move {
                    let _ = e.data().set_focus(true).await;
                },
                oninput: move |e| {
                    text.set(e.value());
                    problem.set(None);
                },
                onkeydown: move |e: KeyboardEvent| {
                    if e.key() == Key::Escape {
                        e.prevent_default();
                        on_done.call(None);
                    } else if e.key() == Key::Enter && e.modifiers().ctrl() {
                        e.prevent_default();
                        on_key();
                    }
                },
            }
            if let Some((over, line)) = limit {
                p { class: if over { "limit over" } else { "limit" }, role: "status", "{line}" }
            }
            if let Some(p) = problem() {
                ProblemLine { problem: p }
            }
            div { class: "actions",
                button {
                    class: "primary",
                    disabled: !can_save,
                    aria_busy: if busy() { "true" } else { "false" },
                    onclick: move |_| on_click(),
                    "Save the change"
                }
                button { class: "quiet", onclick: move |_| on_done.call(None), "Cancel" }
                button { class: "quiet", disabled: busy(), onclick: delete, "Delete this note" }
            }
        }
    }
}

/// What the writer just did to a note, with Undo. Undoing an edit sends
/// the words before it again; undoing a delete restores the note.
#[component]
pub fn ChangeLine(id: String, seq: u64, last: Signal<Option<Last>>) -> Element {
    let (backend, model) = use_ui();
    let mut busy = use_signal(|| false);
    let mut problem = use_signal(|| None::<Problem>);
    let Some(l) = last() else {
        return rsx! {};
    };
    let b = backend.clone();
    let sid = id.clone();
    let what = l.clone();
    let undo = move |_| {
        busy.set(true);
        let (b, sid, what) = (b.clone(), sid.clone(), what.clone());
        spawn(async move {
            let result = match &what {
                Last::Edited { before } => b.edit_note(&sid, seq, before).await,
                Last::Deleted => b.restore_note(&sid, seq).await,
                Last::Undone => Ok(()),
            };
            busy.set(false);
            match result {
                Ok(()) => {
                    problem.set(None);
                    last.set(Some(Last::Undone));
                    reload(b, model);
                }
                Err(p) => problem.set(Some(p)),
            }
        });
    };
    let line = match l {
        Last::Edited { .. } => "Edited.",
        Last::Deleted => "Deleted. It stays in its history for you both.",
        Last::Undone => "Undone.",
    };
    rsx! {
        div { class: "change-line", role: "status",
            span { "{line}" }
            if l != Last::Undone {
                button {
                    class: "link",
                    disabled: busy(),
                    aria_busy: if busy() { "true" } else { "false" },
                    onclick: undo,
                    "Undo"
                }
            }
        }
        if let Some(p) = problem() {
            ProblemLine { problem: p }
        }
    }
}

/// A note's every version, oldest first, read again whenever `stamp`
/// (the note's current state) changes.
#[component]
pub fn History(id: String, seq: u64, stamp: String) -> Element {
    let (backend, _) = use_ui();
    let mut versions = use_signal(|| None::<Result<Vec<NoteVersion>, Problem>>);
    let b = backend.clone();
    use_effect(use_reactive!(|(id, stamp)| {
        let _ = stamp;
        let (b, id) = (b.clone(), id.clone());
        spawn(async move {
            versions.set(Some(b.note_versions(&id, seq).await));
        });
    }));
    let offset = backend.utc_offset();
    match versions() {
        None => rsx! { p { class: "coming", "Reading its history…" } },
        Some(Err(p)) => rsx! { ProblemLine { problem: p } },
        Some(Ok(v)) => rsx! {
            p { class: "hint versions-caption", id: "versions-{seq}", "Every version, oldest first." }
            ol { class: "versions", aria_labelledby: "versions-{seq}",
                for (i, version) in v.into_iter().enumerate() {
                    li { key: "{i}",
                        p { class: "hint", "{when(offset, version.at)}" }
                        match version.content {
                            Some(text) => rsx! { div { class: "pitch", Markdown { source: text } } },
                            None => rsx! { p { class: "hint", "Deleted." } },
                        }
                    }
                }
            }
        },
    }
}
