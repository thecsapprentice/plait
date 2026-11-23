//! A note's files (3g-2): images shown on the note, with a larger view;
//! every file saved to a folder chosen in the system's dialog. Nothing is
//! opened in another app: a file came from another writer and could be
//! anything (decided 2026-11-21, `docs/roadmap.md`).

use dioxus::prelude::*;
use plait_board_ui::{art, icons::Icon};

use crate::{
    app::{ProblemLine, use_ui},
    backend::{NoteFile, Problem},
    storage::size,
};

/// "notes.txt, 12 KB".
fn described(f: &NoteFile) -> String {
    format!("{}, {}", f.name, size(f.size))
}

/// What saving said: where things went.
fn saved_line(saved: &[String], folder: &str) -> String {
    match saved {
        [] => "Nothing was saved: none of these files are on this computer.".into(),
        [one] => format!("Saved to {one}."),
        many => format!("Saved {} files to {folder}.", many.len()),
    }
}

#[component]
pub fn NoteFiles(files: Vec<NoteFile>) -> Element {
    let (backend, _) = use_ui();
    let mut busy = use_signal(|| false);
    let mut said = use_signal(|| None::<String>);
    let mut problem = use_signal(|| None::<Problem>);
    let mut larger = use_signal(|| None::<NoteFile>);
    // Saving is this note's: its answer is only for this note to say.
    let save = use_callback(move |chosen: Vec<NoteFile>| {
        let b = backend.clone();
        busy.set(true);
        problem.set(None);
        said.set(None);
        spawn(async move {
            let folder = match b.choose_folder().await {
                Ok(Some(f)) => f,
                Ok(None) => {
                    busy.set(false);
                    return;
                }
                Err(p) => {
                    busy.set(false);
                    problem.set(Some(p));
                    return;
                }
            };
            let result = b.save_files(&chosen, &folder).await;
            busy.set(false);
            match result {
                Ok(saved) => said.set(Some(saved_line(&saved, &folder))),
                Err(p) => problem.set(Some(p)),
            }
        });
    });
    let (backend, _) = use_ui();
    let images: Vec<NoteFile> = files.iter().filter(|f| f.shown_inline()).cloned().collect();
    let others: Vec<NoteFile> = files
        .iter()
        .filter(|f| !f.shown_inline())
        .cloned()
        .collect();
    let savable: Vec<NoteFile> = files.iter().filter(|f| f.here).cloned().collect();

    rsx! {
        div { class: "note-files",
            if !images.is_empty() {
                div { class: "note-images",
                    for f in images {
                        figure { class: "note-image", key: "{f.key}",
                            button {
                                class: "image-open",
                                aria_label: "See {f.name} larger",
                                onclick: {
                                    let f = f.clone();
                                    move |_| larger.set(Some(f.clone()))
                                },
                                img { src: backend.file_src(&f), alt: "{f.name}" }
                            }
                            figcaption {
                                span { "{described(&f)}" }
                                button {
                                    class: "link",
                                    disabled: busy(),
                                    aria_label: "Save {f.name}",
                                    onclick: {
                                        let f = f.clone();
                                        move |_| save.call(vec![f.clone()])
                                    },
                                    "Save…"
                                }
                            }
                        }
                    }
                }
            }
            if !others.is_empty() {
                ul { class: "file-list",
                    for f in others {
                        li { key: "{f.key}",
                            {art::icon(Icon::Attach, "file-icon")}
                            span { class: "file-name", "{described(&f)}" }
                            if f.here {
                                button {
                                    class: "secondary",
                                    disabled: busy(),
                                    aria_label: "Save {f.name}",
                                    onclick: {
                                        let f = f.clone();
                                        move |_| save.call(vec![f.clone()])
                                    },
                                    "Save…"
                                }
                            } else {
                                span { class: "hint", "Not on this computer yet." }
                            }
                        }
                    }
                }
            }
            if savable.len() > 1 {
                div { class: "actions",
                    button {
                        class: "quiet",
                        disabled: busy(),
                        onclick: move |_| save.call(savable.clone()),
                        "Save all {savable.len()} files…"
                    }
                }
            }
            if let Some(line) = said() {
                p { class: "hint", role: "status", "{line}" }
            }
            if let Some(p) = problem() {
                ProblemLine { problem: p }
            }
        }
        if let Some(f) = larger() {
            LargerImage {
                file: f.clone(),
                on_save: move |f: NoteFile| save.call(vec![f]),
                on_close: move |()| larger.set(None),
            }
        }
    }
}

/// An image across the window, over everything else, until closed.
#[component]
fn LargerImage(
    file: NoteFile,
    on_save: EventHandler<NoteFile>,
    on_close: EventHandler<()>,
) -> Element {
    let (backend, _) = use_ui();
    let f = file.clone();
    rsx! {
        div {
            class: "lightbox",
            role: "dialog",
            aria_modal: "true",
            aria_label: "{file.name}",
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape {
                    on_close.call(());
                }
            },
            img { src: backend.file_src(&file), alt: "{file.name}" }
            div { class: "lightbox-bar",
                span { "{described(&file)}" }
                button { class: "secondary", onclick: move |_| on_save.call(f.clone()), "Save…" }
                button {
                    class: "primary",
                    autofocus: true,
                    onmounted: move |e| async move {
                        let _ = e.data().set_focus(true).await;
                    },
                    onclick: move |_| on_close.call(()),
                    "Close"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_says_where_things_went() {
        assert_eq!(saved_line(&["/s/a.txt".into()], "/s"), "Saved to /s/a.txt.");
        assert_eq!(
            saved_line(&["/s/a".into(), "/s/b".into()], "/s"),
            "Saved 2 files to /s."
        );
        assert!(saved_line(&[], "/s").starts_with("Nothing was saved"));
    }
}
