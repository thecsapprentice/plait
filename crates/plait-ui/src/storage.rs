//! The Storage view (3g-1): what the login's sessions take on the server,
//! as `plait storage` shows it. The server holds only ciphertext, so these
//! are sizes and counts, never contents.

use dioxus::prelude::*;
use plait_proto::sessions::{AccountStorage, SessionStorage};

use crate::{
    app::{ProblemLine, use_ui},
    backend::Problem,
};

/// From this share of the quota on, the view says it's nearly full.
const NEARLY_FULL: f64 = 0.9;

/// A size for people: KB, MB and GB in 1024s, with one decimal below 10.
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if bytes < 1024 {
        return if bytes == 1 {
            "1 byte".into()
        } else {
            format!("{bytes} bytes")
        };
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// "1 message" or "3 messages", and the same for other nouns.
fn count(n: u64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

#[component]
pub fn StoragePage() -> Element {
    let (backend, model) = use_ui();
    let mut shown = use_signal(|| None::<AccountStorage>);
    let mut problem = use_signal(|| None::<Problem>);
    // A view of the screen's own: leaving it rightly ends the load.
    let load = use_callback(move |()| {
        let b = backend.clone();
        problem.set(None);
        spawn(async move {
            match b.storage().await {
                Ok(s) => shown.set(Some(s)),
                Err(p) => problem.set(Some(p)),
            }
        });
    });
    use_hook(move || load.call(()));
    let server = model.read().server_name();

    rsx! {
        h1 { "Storage" }
        p { class: "lede",
            "What your sessions take on {server}. The server keeps only encrypted text and files, so it knows their sizes, never what they say."
        }
        if let Some(p) = problem() {
            ProblemLine { problem: p }
            div { class: "actions",
                button { class: "secondary", onclick: move |_| load.call(()), "Try again" }
            }
        }
        match shown() {
            None if problem().is_none() => rsx! { p { class: "coming", "Loading what your sessions take…" } },
            None => rsx! {},
            Some(s) => rsx! { Totals { storage: s.clone() } SessionsTaking { sessions: s.sessions } },
        }
    }
}

/// The quota, as a bar, and the other totals.
#[component]
fn Totals(storage: AccountStorage) -> Element {
    let s = storage;
    let share = if s.quota_bytes == 0 {
        0.0
    } else {
        s.quota_used_bytes as f64 / s.quota_bytes as f64
    };
    let used = size(s.quota_used_bytes);
    let allowed = size(s.quota_bytes);
    rsx! {
        section { class: "panel storage-quota",
            h2 { "Attachments you uploaded" }
            p { class: "quota-line", "{used} of {allowed}" }
            meter {
                "aria-label": "Attachments you uploaded: {used} of {allowed}",
                min: "0",
                max: "{s.quota_bytes}",
                low: "{(s.quota_bytes as f64 * NEARLY_FULL) as u64}",
                high: "{(s.quota_bytes as f64 * NEARLY_FULL) as u64}",
                optimum: "0",
                value: "{s.quota_used_bytes}",
            }
            if share >= NEARLY_FULL {
                p { class: "hint", role: "status",
                    "Nearly full. Once it's full, new attachments are refused until older ones leave the server."
                }
            }
            p { class: "hint",
                "Attachments leave the server once every device in their session has them."
            }
        }
        dl { class: "storage-totals",
            dt { "Everything stored for your sessions" }
            dd { "{size(s.total_bytes)}" }
            dt { "Keeping your devices in step" }
            dd { "{size(s.device_sync_bytes)}" }
            if s.backup_bytes > 0 {
                dt { "Your backup" }
                dd { "{size(s.backup_bytes)}" }
            }
        }
    }
}

/// Each session's share.
#[component]
fn SessionsTaking(sessions: Vec<SessionStorage>) -> Element {
    rsx! {
        h2 { "By session" }
        if sessions.is_empty() {
            p { class: "coming", "No sessions yet, so nothing stored for them." }
        }
        ul { class: "list storage-sessions",
            for x in sessions {
                li { class: "item", key: "{x.id}",
                    header {
                        span { class: "name",
                            match (&x.persona, &x.partner) {
                                (Some(me), Some(them)) => rsx! { "@{me} with @{them}" },
                                (Some(me), None) => rsx! { "@{me}" },
                                _ => rsx! { "A session" },
                            }
                        }
                        if x.ended {
                            span { class: "tag", "Ended" }
                        }
                    }
                    p { "{count(x.messages, \"message\", \"messages\")}, {size(x.message_bytes)}." }
                    if x.files > 0 {
                        p {
                            "{count(x.files, \"attachment\", \"attachments\")}, {size(x.file_bytes)}"
                            if x.own_file_bytes > 0 {
                                " (you uploaded {size(x.own_file_bytes)})"
                            }
                            "."
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_plainly() {
        assert_eq!(size(0), "0 bytes");
        assert_eq!(size(1), "1 byte");
        assert_eq!(size(1023), "1023 bytes");
        assert_eq!(size(1024), "1.0 KB");
        assert_eq!(size(48 * 1024), "48 KB");
        assert_eq!(size(1536 * 1024), "1.5 MB");
        assert_eq!(size(500 * 1024 * 1024), "500 MB");
        assert_eq!(size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
