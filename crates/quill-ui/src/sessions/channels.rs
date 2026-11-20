//! Naming channels (3f-3): a new channel named as typed ("World building"
//! is `world-building`, its tab "World building"), or a channel described
//! again. Either writer can do both; the other sees it live.

use dioxus::prelude::*;
use quill_proto::limits::SUMMARY_MAX_CHARS;

use super::{reload, tab_label};
use crate::{
    app::{ProblemLine, use_ui},
    backend::Problem,
    checks,
    field::{Field, Kind},
};

/// What the form is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Naming {
    New,
    /// Describing the channel `name` again, from `about`.
    Describe {
        name: String,
        about: Option<String>,
    },
}

/// The form. `taken` are the session's channels; `on_done` gets the
/// channel to show, or `None` on Cancel.
#[component]
pub fn ChannelForm(
    id: String,
    naming: Naming,
    taken: Vec<String>,
    on_done: EventHandler<Option<String>>,
) -> Element {
    let (backend, model) = use_ui();
    let start = match &naming {
        Naming::New => String::new(),
        Naming::Describe { about, .. } => about.clone().unwrap_or_default(),
    };
    let mut typed = use_signal(String::new);
    let mut about = use_signal(|| start.clone());
    let mut busy = use_signal(|| false);
    let mut problem = use_signal(|| None::<Problem>);

    let (title, button, fixed) = match &naming {
        Naming::New => ("A new channel".to_owned(), "Make the channel", None),
        Naming::Describe { name, .. } => (
            format!("Describe {}", tab_label(name)),
            "Save the description",
            Some(name.clone()),
        ),
    };
    let name = fixed
        .clone()
        .unwrap_or_else(|| checks::channel_name(&typed()));
    let name_check = checks::channel(&typed(), &taken);
    let about_check = checks::length(&about(), SUMMARY_MAX_CHARS);
    let ready = !busy()
        && !about_check.invalid
        && (fixed.is_some() || (!name.is_empty() && !name_check.invalid));

    let b = backend.clone();
    let sid = id.clone();
    let submit = move || {
        if !ready {
            return;
        }
        busy.set(true);
        let (b, sid, name) = (b.clone(), sid.clone(), name.clone());
        let text = about.peek().trim().to_owned();
        spawn(async move {
            let about = (!text.is_empty()).then_some(text.as_str());
            let result = b.name_channel(&sid, &name, about).await;
            busy.set(false);
            match result {
                Ok(()) => {
                    reload(b, model);
                    on_done.call(Some(name));
                }
                Err(p) => problem.set(Some(p)),
            }
        });
    };
    let (mut on_enter, mut on_about) = (submit.clone(), submit.clone());
    let mut on_click = submit;
    rsx! {
        section { class: "panel channel-form", aria_labelledby: "channel-form-title",
            h3 { id: "channel-form-title", "{title}" }
            if fixed.is_none() {
                Field {
                    label: "Name",
                    id: "channel-name",
                    kind: Kind::Name,
                    hint: name_check.hint.clone(),
                    invalid: name_check.invalid,
                    autofocus: true,
                    oninput: move |t| {
                        typed.set(t);
                        problem.set(None);
                    },
                    onenter: move |_| on_enter(),
                }
            }
            Field {
                label: "What it's for (optional)",
                id: "channel-about",
                kind: Kind::Text,
                initial: start.clone(),
                hint: about_check.hint.clone(),
                invalid: about_check.invalid,
                autofocus: fixed.is_some(),
                oninput: move |t| {
                    about.set(t);
                    problem.set(None);
                },
                onenter: move |_| on_about(),
            }
            if let Some(p) = problem() {
                ProblemLine { problem: p }
            }
            div { class: "actions",
                button {
                    class: "primary",
                    disabled: !ready,
                    aria_busy: if busy() { "true" } else { "false" },
                    onclick: move |_| on_click(),
                    "{button}"
                }
                button { class: "quiet", onclick: move |_| on_done.call(None), "Cancel" }
            }
        }
    }
}
