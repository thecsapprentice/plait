//! Safety numbers for a session (3f-3): offered, never forced. The 12
//! identity words are the ones from requests and Personas (#72); the 6
//! follow the session's keys now. "They match" marks the other writer
//! verified on this computer. A key changed since it was pinned is checked
//! first, as in requests (3e, `KeyCheck`).

use dioxus::prelude::*;
use uuid::Uuid;

use super::reload;
use crate::{
    app::{ProblemLine, use_ui},
    backend::{Problem, SafetyNumbers},
    requests::KeyCheck,
};

/// The panel, opened from the session's heading. `as_persona` is the
/// writer's persona in the session, for checking a changed key.
#[component]
pub fn SafetyPanel(
    id: String,
    handle: String,
    as_persona: Option<Uuid>,
    on_close: EventHandler<()>,
) -> Element {
    let (backend, model) = use_ui();
    let mut numbers = use_signal(|| None::<Result<SafetyNumbers, Problem>>);
    let mut busy = use_signal(|| false);
    let mut again = use_signal(|| 0u32);

    let b = backend.clone();
    let sid = id.clone();
    use_effect(move || {
        let _ = again();
        let (b, sid) = (b.clone(), sid.clone());
        spawn(async move {
            numbers.set(Some(b.safety_numbers(&sid).await));
        });
    });
    let b = backend.clone();
    let sid = id.clone();
    let verify = move |_| {
        busy.set(true);
        let (b, sid) = (b.clone(), sid.clone());
        spawn(async move {
            let result = b.confirm_verified(&sid).await;
            busy.set(false);
            numbers.set(Some(result));
            reload(b, model);
        });
    };
    let body = match numbers() {
        None => rsx! { p { class: "coming", "Working out the words…" } },
        Some(Err(Problem::KeyChanged(_))) => match as_persona {
            Some(persona) => rsx! {
                KeyCheck {
                    handle: handle.clone(),
                    as_persona: persona,
                    on_done: move |trusted: bool| {
                        if trusted {
                            again += 1;
                        } else {
                            on_close.call(());
                        }
                    },
                }
            },
            None => rsx! { ProblemLine { problem: Problem::KeyChanged(handle.clone()) } },
        },
        Some(Err(p)) => rsx! { ProblemLine { problem: p } },
        Some(Ok(n)) => rsx! {
            p {
                "Compare these with @{handle} some other way than Plait, like a call. "
                "If they match, no one is passing as either of you, and no one is reading between you."
            }
            p { class: "hint", "Your 12 words with @{handle}" }
            ol { class: "words", aria_label: "Your 12 words with @{handle}",
                for (i, w) in n.identity.iter().enumerate() {
                    li { key: "{i}", "{w}" }
                }
            }
            p { class: "hint",
                "And 6 for the session's keys now. They change when either of you adds or removes a device, so compare them at the same time."
            }
            ol { class: "words", aria_label: "The session's 6 words",
                for (i, w) in n.check.iter().enumerate() {
                    li { key: "{i}", "{w}" }
                }
            }
            if n.verified {
                p { class: "verified", role: "status", "✓ You marked @{handle} verified on this computer." }
            } else {
                div { class: "actions",
                    button {
                        class: "primary",
                        disabled: busy(),
                        aria_busy: if busy() { "true" } else { "false" },
                        onclick: verify,
                        "They match"
                    }
                }
            }
        },
    };
    rsx! {
        section { class: "panel safety", aria_labelledby: "safety-title",
            h3 { id: "safety-title", "Safety numbers with @{handle}" }
            {body}
            div { class: "actions",
                button { class: "quiet", onclick: move |_| on_close.call(()), "Hide the safety numbers" }
            }
        }
    }
}
