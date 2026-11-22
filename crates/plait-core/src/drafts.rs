//! What this device keeps while its writer writes (3f-2): a draft per
//! session and channel, and an outbox of notes waiting to be sent.
//!
//! Both live in the encrypted store beside the session's history (keys
//! `0x07` and `0x08` in [`Table::Messages`], see [`crate::history`]), so a
//! draft survives the window closing and a note written offline survives
//! until it can go. Neither leaves this device until a note is sent.
//!
//! Notes go out in the order they were queued. [`flush`] stops at the first
//! one the server can't take yet (offline), or that it refused, so a later
//! note never overtakes an earlier one. The sessions with notes waiting are
//! listed in [`Table::Meta`], so [`flush_all`] needn't search every session. See docs/plan.md, "Writing in the
//! desktop app (3f-2)".

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Client, CoreError, Store,
    history::{get, key, prefix, put},
    mls::Provider,
    sessions,
    store::kv::{Op, Table},
};

const DRAFT: u8 = 7;
const QUEUED: u8 = 8;
/// A waiting note's files, each under its own key (queue id, then its
/// place), so a note's record stays small.
const QUEUED_FILE: u8 = 9;
/// The group ids (base64url) of sessions with notes waiting.
const WAITING: &[u8] = b"drafts:waiting";

fn waiting(store: &Store) -> Result<BTreeSet<String>, CoreError> {
    Ok(match store.get(Table::Meta, WAITING)? {
        Some(v) => serde_json::from_slice(&v).map_err(|_| err("a stored list is damaged"))?,
        None => BTreeSet::new(),
    })
}

/// Lists or unlists `gid` among the sessions with notes waiting, by
/// whether any are.
fn note_waiting(store: &Store, gid: &[u8]) -> Result<(), CoreError> {
    let mut all = waiting(store)?;
    let id = plait_proto::keys::b64(gid);
    let changed = if queued(store, gid)?.is_empty() {
        all.remove(&id)
    } else {
        all.insert(id)
    };
    if changed {
        let v = serde_json::to_vec(&all).map_err(|_| err("encoding"))?;
        store.apply(&[Op::Put {
            table: Table::Meta,
            key: WAITING,
            value: &v,
        }])?;
    }
    Ok(())
}

fn err(msg: impl Into<String>) -> CoreError {
    CoreError::Keys(plait_proto::codes::HISTORY_CHECK_FAILED, msg.into())
}

/// A note being written in one channel: its text, and the note it answers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<u64>,
}

impl Draft {
    fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.reply_to.is_none()
    }
}

/// Keeps `channel`'s draft; an empty one (no text, answering nothing) is
/// removed.
pub fn save(store: &Store, gid: &[u8], channel: &str, draft: &Draft) -> Result<(), CoreError> {
    let p = Provider::new(store);
    let k = key(gid, DRAFT, channel.as_bytes());
    if draft.is_empty() {
        p.set(Table::Messages, &k, None)
    } else {
        put(&p, &k, draft)
    }
}

/// `channel`'s draft, if there is one.
pub fn draft(store: &Store, gid: &[u8], channel: &str) -> Result<Option<Draft>, CoreError> {
    get(&Provider::new(store), &key(gid, DRAFT, channel.as_bytes()))
}

/// The channels with a draft, by name.
pub fn drafted(store: &Store, gid: &[u8]) -> Result<Vec<String>, CoreError> {
    let start = prefix(gid, DRAFT);
    Ok(store
        .scan(Table::Messages, &start)?
        .into_iter()
        .filter_map(|(k, _)| String::from_utf8(k.get(start.len()..)?.to_vec()).ok())
        .collect())
}

/// Why the server refused a queued note: its code and words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refused {
    pub code: u16,
    pub detail: String,
}

/// A note waiting to be sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Queued {
    /// This device's own id for it, in the order it was queued.
    pub id: Uuid,
    pub channel: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<u64>,
    /// Set when the server refused it; it waits to be changed or withdrawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<Refused>,
    /// The names of the files it carries, in order; their contents wait
    /// beside it (3g-3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

fn file_key(gid: &[u8], id: Uuid, n: usize) -> Vec<u8> {
    let mut at = id.as_bytes().to_vec();
    at.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_be_bytes());
    key(gid, QUEUED_FILE, &at)
}

/// A waiting note's files, in order, with their names.
pub fn queued_files(
    store: &Store,
    gid: &[u8],
    q: &Queued,
) -> Result<Vec<(String, Vec<u8>)>, CoreError> {
    q.files
        .iter()
        .enumerate()
        .map(|(n, name)| {
            let contents = store
                .get(Table::Messages, &file_key(gid, q.id, n))?
                .ok_or_else(|| err("a waiting note's file is missing"))?;
            Ok((name.clone(), contents.to_vec()))
        })
        .collect()
}

fn drop_files(store: &Store, gid: &[u8], q: &Queued) -> Result<(), CoreError> {
    let p = Provider::new(store);
    for n in 0..q.files.len() {
        p.set(Table::Messages, &file_key(gid, q.id, n), None)?;
    }
    Ok(())
}

/// Queues a note behind any already waiting. It's checked as sending
/// would check it (not empty, not too long) before it's kept.
pub fn queue(
    store: &Store,
    gid: &[u8],
    channel: &str,
    text: &str,
    reply_to: Option<u64>,
) -> Result<Queued, CoreError> {
    queue_with_files(store, gid, channel, text, reply_to, Vec::new())
}

/// As [`queue`], with files: a name and the contents each. A note with
/// files may have no text, as `sessions::post` allows. The server's switch
/// and size limit are checked when it's sent, as it may be offline now.
pub fn queue_with_files(
    store: &Store,
    gid: &[u8],
    channel: &str,
    text: &str,
    reply_to: Option<u64>,
    files: Vec<(String, Vec<u8>)>,
) -> Result<Queued, CoreError> {
    let text = if files.is_empty() {
        sessions::check_text(text)?
    } else {
        text.trim()
    };
    if files.len() > sessions::MAX_FILES {
        return Err(err(format!(
            "at most {} files per note",
            sessions::MAX_FILES
        )));
    }
    let q = Queued {
        id: Uuid::now_v7(),
        channel: channel.to_owned(),
        text: text.to_owned(),
        reply_to,
        refused: None,
        files: files.iter().map(|(name, _)| name.clone()).collect(),
    };
    let p = Provider::new(store);
    for (n, (_, contents)) in files.into_iter().enumerate() {
        p.set(Table::Messages, &file_key(gid, q.id, n), Some(contents))?;
    }
    put(&p, &key(gid, QUEUED, q.id.as_bytes()), &q)?;
    note_waiting(store, gid)?;
    Ok(q)
}

/// The notes waiting, oldest first.
pub fn queued(store: &Store, gid: &[u8]) -> Result<Vec<Queued>, CoreError> {
    store
        .scan(Table::Messages, &prefix(gid, QUEUED))?
        .into_iter()
        .map(|(_, v)| serde_json::from_slice(&v).map_err(|_| err("a queued note is damaged")))
        .collect()
}

/// Changes a waiting note's text, keeping its place. A refusal is cleared,
/// so it's tried again. Fails if it has already gone.
pub fn change(store: &Store, gid: &[u8], id: Uuid, text: &str) -> Result<(), CoreError> {
    let text = sessions::check_text(text)?;
    let p = Provider::new(store);
    let k = key(gid, QUEUED, id.as_bytes());
    let mut q: Queued = get(&p, &k)?.ok_or_else(|| err("that note has already been sent"))?;
    q.text = text.to_owned();
    q.refused = None;
    put(&p, &k, &q)
}

/// Takes a waiting note back. Returns whether it was still waiting.
pub fn withdraw(store: &Store, gid: &[u8], id: Uuid) -> Result<bool, CoreError> {
    let p = Provider::new(store);
    let k = key(gid, QUEUED, id.as_bytes());
    let was = get::<Queued>(&p, &k)?;
    if let Some(q) = &was {
        drop_files(store, gid, q)?;
    }
    let was = was.is_some();
    p.set(Table::Messages, &k, None)?;
    note_waiting(store, gid)?;
    Ok(was)
}

/// What [`flush`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Flushed {
    /// Each note sent: its queue id and its place in the log.
    pub sent: Vec<(Uuid, u64)>,
    /// It stopped because the server couldn't be reached.
    pub offline: bool,
    /// It stopped at this note, which the server refused.
    pub refused: Option<Uuid>,
}

/// Sends the waiting notes of session `id` (group `gid`) in order, until
/// one can't go yet or is refused. A refused note keeps its place, marked,
/// and later ones wait behind it.
pub async fn flush(
    store: &Store,
    client: &Client,
    id: &str,
    gid: &[u8],
) -> Result<Flushed, CoreError> {
    flush_reporting(store, client, id, gid, &|_| {}).await
}

/// As [`flush`], saying how a note's files are getting on as they go.
pub async fn flush_reporting(
    store: &Store,
    client: &Client,
    id: &str,
    gid: &[u8],
    report: &(dyn Fn(sessions::Posting) + Send + Sync),
) -> Result<Flushed, CoreError> {
    let mut done = Flushed::default();
    for q in queued(store, gid)? {
        if q.refused.is_some() {
            done.refused = Some(q.id);
            break;
        }
        let sent = if q.files.is_empty() {
            match q.reply_to {
                Some(to) => sessions::reply(store, client, id, &q.text, Some(&q.channel), to).await,
                None => sessions::write(store, client, id, &q.text, Some(&q.channel)).await,
            }
        } else {
            let draft = sessions::Draft {
                text: &q.text,
                channel: Some(&q.channel),
                reply_to: q.reply_to,
                files: queued_files(store, gid, &q)?,
            };
            sessions::post_reporting(store, client, id, draft, report).await
        };
        let p = Provider::new(store);
        let k = key(gid, QUEUED, q.id.as_bytes());
        match sent {
            Ok(seq) => {
                drop_files(store, gid, &q)?;
                p.set(Table::Messages, &k, None)?;
                done.sent.push((q.id, seq));
            }
            Err(e) if e.temporary() => {
                done.offline = true;
                break;
            }
            Err(e) => {
                let refused = Queued {
                    refused: Some(Refused {
                        code: e.code().0,
                        detail: e.to_string(),
                    }),
                    ..q
                };
                put(&p, &k, &refused)?;
                done.refused = Some(refused.id);
                break;
            }
        }
    }
    note_waiting(store, gid)?;
    Ok(done)
}

/// Sends what's waiting in every session, in each one's order. Stops at
/// the first session found offline; a refusal stops only its session.
pub async fn flush_all(store: &Store, client: &Client) -> Result<Flushed, CoreError> {
    let mut all = Flushed::default();
    for id in waiting(store)? {
        let Some(gid) = plait_proto::keys::unb64(&id) else {
            continue;
        };
        let done = flush(store, client, &id, &gid).await?;
        all.sent.extend(done.sent);
        all.refused = all.refused.or(done.refused);
        if done.offline {
            all.offline = true;
            break;
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const GID: &[u8] = &[7u8; 32];

    #[test]
    fn a_queued_notes_files_wait_beside_it_and_go_with_it() {
        let store = Store::in_memory();
        let files = vec![
            ("cove.png".to_owned(), vec![1u8; 300]),
            ("notes.txt".to_owned(), b"ledger".to_vec()),
        ];
        // Files alone are a note, as sending allows.
        let q = queue_with_files(&store, GID, "story", "  ", None, files.clone()).unwrap();
        assert_eq!(q.files, ["cove.png", "notes.txt"]);
        assert_eq!(q.text, "");
        assert_eq!(queued(&store, GID).unwrap(), [q.clone()]);
        assert_eq!(queued_files(&store, GID, &q).unwrap(), files);
        // Too many is refused before anything is kept.
        let many = vec![("x".to_owned(), vec![0u8]); sessions::MAX_FILES + 1];
        assert!(queue_with_files(&store, GID, "story", "", None, many).is_err());
        assert_eq!(queued(&store, GID).unwrap().len(), 1);
        // Taking it back takes its files too.
        assert!(withdraw(&store, GID, q.id).unwrap());
        assert!(queued_files(&store, GID, &q).is_err());
        assert!(waiting(&store).unwrap().is_empty());
    }

    #[test]
    fn drafts_are_kept_per_channel_and_empty_ones_go() {
        let store = Store::in_memory();
        let d = Draft {
            text: "The lamp".into(),
            reply_to: Some(4),
        };
        save(&store, GID, "story", &d).unwrap();
        save(
            &store,
            GID,
            "planning",
            &Draft {
                text: "Notes".into(),
                reply_to: None,
            },
        )
        .unwrap();
        assert_eq!(draft(&store, GID, "story").unwrap(), Some(d));
        assert_eq!(drafted(&store, GID).unwrap(), ["planning", "story"]);
        // Another session's are its own.
        assert_eq!(draft(&store, &[8u8; 32], "story").unwrap(), None);

        save(&store, GID, "story", &Draft::default()).unwrap();
        assert_eq!(draft(&store, GID, "story").unwrap(), None);
        assert_eq!(drafted(&store, GID).unwrap(), ["planning"]);
    }

    #[test]
    fn queued_notes_keep_their_order_and_can_change_or_go() {
        let store = Store::in_memory();
        let a = queue(&store, GID, "story", "First", None).unwrap();
        let b = queue(&store, GID, "planning", "Second", Some(1)).unwrap();
        let c = queue(&store, GID, "story", "Third", None).unwrap();
        let ids = |s: &Store| {
            queued(s, GID)
                .unwrap()
                .into_iter()
                .map(|q| q.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&store), [a.id, b.id, c.id]);

        change(&store, GID, b.id, "Second, changed").unwrap();
        assert_eq!(queued(&store, GID).unwrap()[1].text, "Second, changed");
        assert_eq!(ids(&store), [a.id, b.id, c.id]);

        assert_eq!(waiting(&store).unwrap().len(), 1);
        assert!(withdraw(&store, GID, a.id).unwrap());
        assert!(!withdraw(&store, GID, a.id).unwrap());
        assert!(change(&store, GID, a.id, "x").is_err());
        assert_eq!(ids(&store), [b.id, c.id]);
        // The last one gone, the session isn't listed as waiting.
        withdraw(&store, GID, b.id).unwrap();
        withdraw(&store, GID, c.id).unwrap();
        assert!(waiting(&store).unwrap().is_empty());
    }

    #[test]
    fn nothing_empty_or_too_long_is_queued() {
        let store = Store::in_memory();
        assert!(queue(&store, GID, "story", "  \n", None).is_err());
        let long = "x".repeat(sessions::MAX_MESSAGE_CHARS + 1);
        assert!(queue(&store, GID, "story", &long, None).is_err());
        assert!(queued(&store, GID).unwrap().is_empty());
    }
}
