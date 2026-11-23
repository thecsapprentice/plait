//! The simulated server's sessions (3f), following `plait-server`'s
//! sessions and `plait-core`'s `sessions` and `history`. An accepted
//! request becomes a session between its two writers, starting with the
//! pitch in the planning channel. Each note has a number (its place in the
//! log); edits keep every version; a deletion keeps them too, and its
//! author's later edit restores it (#104). This
//! computer keeps how far it has shown each channel, its standing, drafts,
//! and notes waiting to be sent (`plait-core`'s `drafts`): they go in
//! order once the server can be reached, and stop at a refusal.

use std::collections::{HashMap, HashSet};

use plait_proto::sessions::{AccountStorage, SessionStorage};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use super::{Sim, SimServer, refused};
use crate::backend::{
    Attached, ChannelView, Draft, NOTE_MAX, Note, NoteFile, NoteVersion, Problem, QueuedNote,
    SafetyNumbers, Sending, SessionStanding, SessionSummary, SessionView, Verification, Writer,
    codes,
};

/// The channels every session has.
const DEFAULT_CHANNELS: [&str; 2] = ["story", "planning"];
/// What encrypting a message adds to its text, roughly.
const MESSAGE_OVERHEAD: u64 = 256;
/// A login's attachment quota, as the server's default (2i).
const QUOTA_BYTES: u64 = 500 * 1024 * 1024;
/// The device group's log, fixed here.
const DEVICE_SYNC_BYTES: u64 = 24 * 1024;

/// An upload's size on the server: the ciphertext (16 bytes of tag more)
/// padded to a power of two from 4 KiB, as 2i buckets it.
fn padded(size: u64) -> u64 {
    (size + 16).next_power_of_two().max(4096)
}

/// As `plait_core::attachments::media_type`: a label from the file name's
/// extension; nothing checks it.
fn media_type(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain",
        Some("md") => "text/markdown",
        _ => "application/octet-stream",
    }
}

/// As `plait_core::attachments::safe_name`: the last path part, without
/// control characters or colons, never empty.
fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() || c == ':' { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == ' ').to_owned();
    if cleaned.is_empty() {
        "attachment".into()
    } else {
        cleaned
    }
}

#[derive(Debug, Clone)]
pub(super) struct SimNote {
    seq: u64,
    /// The author's handle.
    author: String,
    channel: String,
    /// Every version, oldest first, each with its time: its text, or
    /// `None` where it was deleted. The last decides, as in `history`.
    versions: Vec<(Option<String>, OffsetDateTime)>,
    reply_to: Option<u64>,
    /// Its attachments, as sent.
    files: Vec<SimFile>,
}

/// What happens to a server just before a note's file goes up
/// (`SimServer::interrupt_before_file`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    /// It can't be reached.
    Away,
    /// Its administrators switch attachments off.
    AttachmentsOff,
}

/// An attachment on a note.
#[derive(Debug, Clone)]
pub(super) struct SimFile {
    key: String,
    name: String,
    media_type: String,
    bytes: Vec<u8>,
}

impl SimNote {
    fn deleted(&self) -> bool {
        self.versions.last().is_some_and(|(c, _)| c.is_none())
    }

    /// The last text, kept after a deletion.
    fn content(&self) -> String {
        self.versions
            .iter()
            .rev()
            .find_map(|(c, _)| c.clone())
            .unwrap_or_default()
    }

    fn at(&self) -> OffsetDateTime {
        self.versions.first().map_or_else(base_time, |(_, at)| *at)
    }
}

/// A change the writer makes to one of their own notes.
pub(super) enum Change<'a> {
    Edit(&'a str),
    Delete,
    Restore,
}

#[derive(Debug, Clone)]
pub(super) struct SimSession {
    pub(super) id: String,
    prompt: Uuid,
    /// Its two writers, login and handle. As on the server, leaving ends
    /// the session but keeps both.
    members: Vec<(String, String)>,
    notes: Vec<SimNote>,
    /// Named channels beyond the defaults: name and description.
    channels: Vec<(String, Option<String>)>,
    ended: bool,
    seq: u64,
    /// Attachments on the server: the uploader's login and the padded size.
    files: Vec<(String, u64)>,
}

/// One server's sessions.
#[derive(Debug, Clone, Default)]
pub(super) struct Sessions {
    pub(super) all: Vec<SimSession>,
    count: u32,
}

/// This computer's side: how far each channel is shown, and standings
/// other than joined.
#[derive(Debug, Clone, Default)]
pub(super) struct Here {
    shown: HashMap<(String, String), u64>,
    standing: HashMap<String, SessionStanding>,
    /// Sessions opened in this store, as `history::mark_opened` keeps.
    opened: HashSet<String>,
    /// Drafts by session and channel.
    drafts: HashMap<(String, String), Draft>,
    /// Notes waiting to be sent: session and note, oldest first.
    queued: Vec<(String, QueuedNote)>,
    /// The files of waiting notes, by note.
    queued_files: HashMap<Uuid, Vec<(String, Vec<u8>)>>,
    /// What the server already took of a waiting note's files (#125).
    queued_uploaded: HashMap<Uuid, Vec<String>>,
}

fn base_time() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::days(20_100)
}

impl SimServer {
    /// An accepted request becomes a session, starting with its pitch.
    pub(super) fn begin_session(
        &mut self,
        prompt: Uuid,
        requester: (String, String),
        author: (String, String),
        pitch: &str,
    ) -> String {
        self.sessions.count += 1;
        let id = format!("c2Vzc2lvbi{:04}", self.sessions.count);
        let mut s = SimSession {
            id: id.clone(),
            prompt,
            members: vec![requester.clone(), author],
            notes: Vec::new(),
            channels: Vec::new(),
            ended: false,
            seq: 0,
            files: Vec::new(),
        };
        s.write(&requester.1, "planning", pitch, None);
        self.sessions.all.push(s);
        id
    }

    fn session_mut(&mut self, id: &str) -> &mut SimSession {
        self.sessions
            .all
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("no session {id}"))
    }
}

impl SimSession {
    fn write(&mut self, author: &str, channel: &str, text: &str, reply_to: Option<u64>) -> u64 {
        self.seq += 1;
        if !DEFAULT_CHANNELS.contains(&channel) && !self.channels.iter().any(|(n, _)| n == channel)
        {
            self.channels.push((channel.to_owned(), None));
        }
        let at = self.clock();
        self.notes.push(SimNote {
            seq: self.seq,
            author: author.to_owned(),
            channel: channel.to_owned(),
            versions: vec![(Some(text.to_owned()), at)],
            reply_to,
            files: Vec::new(),
        });
        self.seq
    }

    /// The time of the change at the log's newest place.
    fn clock(&self) -> OffsetDateTime {
        base_time() + Duration::minutes(i64::try_from(self.seq).unwrap_or(0))
    }

    /// A new version of note `seq`, taking the next place in the log.
    fn change(&mut self, seq: u64, content: Option<String>) {
        self.seq += 1;
        let at = self.clock();
        self.note_mut(seq).versions.push((content, at));
    }

    fn note_mut(&mut self, seq: u64) -> &mut SimNote {
        self.notes
            .iter_mut()
            .find(|n| n.seq == seq)
            .unwrap_or_else(|| panic!("no note #{seq}"))
    }

    fn mine<'a>(&'a self, login: &str) -> Option<&'a (String, String)> {
        self.members.iter().find(|(l, _)| l == login)
    }
}

// ---- What tests do --------------------------------------------------------------------

impl Sim {
    /// A session between the open login's persona `mine` and `partner`
    /// (made if new), on a prompt of the partner's titled `title`.
    /// Returns its id.
    pub fn session_with(&self, domain: &str, mine: &str, partner: &str, title: &str) -> String {
        let prompt = self.post_as(
            domain,
            partner,
            title,
            "A prompt to write together.",
            &[],
            plait_proto::board::Maturity::General,
        );
        let login = self.login_of(domain, mine);
        let partner_login = self.login_of(domain, partner);
        let id = self.server(domain, |s| {
            s.begin_session(
                prompt,
                (login, mine.to_owned()),
                (partner_login, partner.to_owned()),
                "Shall we?",
            )
        });
        self.live();
        id
    }

    /// `handle` uploaded an attachment of `bytes` (padded) to session `id`:
    /// it counts in the session's storage, and against that login's quota.
    pub fn uploaded(&self, domain: &str, id: &str, handle: &str, bytes: u64) {
        let login = self.login_of(domain, handle);
        self.server(domain, |s| s.session_mut(id).files.push((login, bytes)));
    }

    /// What the signed-in login's sessions take, as `GET /account/storage`
    /// counts it: each stored version is a message, sized with its text
    /// plus what encryption adds.
    pub(super) fn sim_storage(&self) -> Result<AccountStorage, Problem> {
        let (login, server) = self.signed_in()?;
        let mut sessions = Vec::new();
        for s in &server.sessions.all {
            let Some((_, mine)) = s.mine(&login) else {
                continue;
            };
            let partner = s
                .members
                .iter()
                .find(|(l, _)| *l != login)
                .map(|(_, h)| h.clone());
            let versions = s.notes.iter().flat_map(|n| &n.versions);
            let message_bytes = versions
                .clone()
                .map(|(c, _)| c.as_ref().map_or(0, String::len) as u64 + MESSAGE_OVERHEAD)
                .sum();
            sessions.push(SessionStorage {
                id: s.id.clone(),
                persona: Some(mine.clone()),
                partner,
                ended: s.ended,
                messages: versions.count() as u64,
                message_bytes,
                files: s.files.len() as u64,
                file_bytes: s.files.iter().map(|(_, b)| b).sum(),
                own_file_bytes: s
                    .files
                    .iter()
                    .filter(|(l, _)| *l == login)
                    .map(|(_, b)| b)
                    .sum(),
            });
        }
        let in_sessions: u64 = sessions
            .iter()
            .map(|x| x.message_bytes + x.file_bytes)
            .sum();
        Ok(AccountStorage {
            total_bytes: in_sessions + DEVICE_SYNC_BYTES,
            quota_used_bytes: sessions.iter().map(|x| x.own_file_bytes).sum(),
            quota_bytes: QUOTA_BYTES,
            device_sync_bytes: DEVICE_SYNC_BYTES,
            backup_bytes: 0,
            sessions,
        })
    }

    fn login_of(&self, domain: &str, handle: &str) -> String {
        self.server(domain, |s| {
            s.personas
                .iter()
                .find(|(_, p, _)| p.handle == handle)
                .map(|(l, _, _)| l.clone())
                .unwrap_or_else(|| panic!("no persona @{handle}"))
        })
    }

    /// `handle` writes a note in a session; the live notice fires. Returns
    /// its number.
    pub fn write_as(
        &self,
        domain: &str,
        session: &str,
        handle: &str,
        channel: &str,
        text: &str,
    ) -> u64 {
        let seq = self.server(domain, |s| {
            s.session_mut(session).write(handle, channel, text, None)
        });
        self.live();
        seq
    }

    /// As `write_as`, with files attached: each a name and its contents.
    /// They count in the session's storage, against `handle`'s quota.
    pub fn write_with_files(
        &self,
        domain: &str,
        session: &str,
        handle: &str,
        text: &str,
        files: &[(&str, &[u8])],
    ) -> u64 {
        let login = self.login_of(domain, handle);
        let seq = self.server(domain, |s| {
            let session = s.session_mut(session);
            let seq = session.write(handle, "story", text, None);
            for (name, bytes) in files {
                let n = session.files.len();
                let key = format!(
                    "https://{domain}/api/v1/blobs/{session_id}-{n}",
                    session_id = session.id
                );
                session
                    .files
                    .push((login.clone(), padded(bytes.len() as u64)));
                session.note_mut(seq).files.push(SimFile {
                    key,
                    name: (*name).to_owned(),
                    media_type: media_type(name).to_owned(),
                    bytes: bytes.to_vec(),
                });
            }
            seq
        });
        self.live();
        seq
    }

    /// Note `seq`'s files aren't on this computer (a device linked later,
    /// say): it lists them but can't show or save them.
    pub fn files_missing_here(&self, domain: &str, session: &str, seq: u64) {
        let keys: Vec<String> = self.server(domain, |s| {
            s.session_mut(session)
                .note_mut(seq)
                .files
                .iter()
                .map(|f| f.key.clone())
                .collect()
        });
        self.0.borrow_mut().missing_here.extend(keys);
    }

    /// What the system's folder dialog answers from now: a folder, or
    /// `None` as if the writer cancelled.
    pub fn folder_picks(&self, folder: Option<&str>) {
        self.0.borrow_mut().folder_pick = folder.map(str::to_owned);
    }

    /// A file already on this computer, at `path`.
    pub fn put_on_disk(&self, path: &str, bytes: &[u8]) {
        self.0
            .borrow_mut()
            .disk
            .insert(path.to_owned(), bytes.to_vec());
    }

    /// The files saved on this computer, by path.
    pub fn on_disk(&self) -> Vec<(String, Vec<u8>)> {
        self.0
            .borrow()
            .disk
            .iter()
            .map(|(p, b)| (p.clone(), b.clone()))
            .collect()
    }

    /// Saves as `plait_core::attachments::save_to` does: under a safe name,
    /// a number in front until it's free.
    pub(super) fn sim_save_files(
        &self,
        files: &[NoteFile],
        folder: &str,
    ) -> Result<Vec<String>, Problem> {
        let mut saved = Vec::new();
        for f in files {
            let bytes = {
                let st = self.0.borrow();
                if st.missing_here.contains(&f.key) {
                    continue;
                }
                st.servers
                    .values()
                    .flat_map(|s| &s.sessions.all)
                    .flat_map(|s| &s.notes)
                    .flat_map(|n| &n.files)
                    .find(|x| x.key == f.key)
                    .map(|x| x.bytes.clone())
            };
            let Some(bytes) = bytes else { continue };
            let name = safe_name(&f.name);
            let mut n = 0;
            let path = loop {
                let p = if n == 0 {
                    format!("{folder}/{name}")
                } else {
                    format!("{folder}/{n}-{name}")
                };
                if !self.0.borrow().disk.contains_key(&p) {
                    break p;
                }
                n += 1;
            };
            self.0.borrow_mut().disk.insert(path.clone(), bytes);
            saved.push(path);
        }
        Ok(saved)
    }

    /// As `write_as`, answering note `to`.
    pub fn reply_as(
        &self,
        domain: &str,
        session: &str,
        handle: &str,
        channel: &str,
        text: &str,
        to: u64,
    ) -> u64 {
        let seq = self.server(domain, |s| {
            s.session_mut(session)
                .write(handle, channel, text, Some(to))
        });
        self.live();
        seq
    }

    /// A note is changed by its author; every version is kept. After a
    /// deletion, this restores it.
    pub fn edit_note(&self, domain: &str, session: &str, seq: u64, text: &str) {
        self.server(domain, |s| {
            s.session_mut(session).change(seq, Some(text.into()))
        });
        self.live();
    }

    /// A note is deleted by its author; it shows as deleted.
    pub fn delete_note(&self, domain: &str, session: &str, seq: u64) {
        self.server(domain, |s| s.session_mut(session).change(seq, None));
        self.live();
    }

    /// A channel is named, or described again.
    pub fn name_channel(&self, domain: &str, session: &str, name: &str, summary: &str) {
        self.server(domain, |s| {
            let session = s.session_mut(session);
            match session.channels.iter_mut().find(|(n, _)| n == name) {
                Some(c) => c.1 = Some(summary.to_owned()),
                None => session
                    .channels
                    .push((name.to_owned(), Some(summary.to_owned()))),
            }
        });
        self.live();
    }

    /// `handle` leaves: with two writers, that ends the session for both.
    /// As on the server, both stay its members, and who left isn't said.
    pub fn leave_as(&self, domain: &str, session: &str, handle: &str) {
        self.server(domain, |s| {
            let session = s.session_mut(session);
            assert!(
                session.members.iter().any(|(_, h)| h == handle),
                "@{handle} isn't in it"
            );
            session.ended = true;
        });
        self.live();
    }

    /// This computer's standing in a session becomes `standing`.
    pub fn set_session_standing(&self, session: &str, standing: SessionStanding) {
        self.with_server_mut(|_, store| {
            store.sessions.standing.insert(session.to_owned(), standing);
        });
    }

    /// The ids of the sessions a login takes part in.
    pub fn sessions_of(&self, domain: &str, login: &str) -> Vec<String> {
        self.server(domain, |s| {
            s.sessions
                .all
                .iter()
                .filter(|x| x.members.iter().any(|(l, _)| l == login))
                .map(|x| x.id.clone())
                .collect()
        })
    }

    // ---- The backend's side ---------------------------------------------------------

    fn writer_named(server: &SimServer, handle: &str) -> Writer {
        Writer {
            handle: handle.to_owned(),
            display_name: server
                .personas
                .iter()
                .find(|(_, p, _)| p.handle == handle)
                .and_then(|(_, p, _)| p.display_name.clone()),
        }
    }

    fn summary_of(&self, server: &SimServer, s: &SimSession, login: &str) -> SessionSummary {
        let here = self.sessions_here();
        let (_, me) = s.mine(login).cloned().unwrap_or_default();
        let partner = s
            .members
            .iter()
            .find(|(l, _)| l != login)
            .map(|(_, h)| h.clone());
        let channels = Self::channels_of(&here, s, &me);
        let (_, store, _) = self.open_store().expect("a store");
        let mine = server
            .personas
            .iter()
            .find(|(_, p, _)| p.handle == me)
            .map(|(_, p, _)| p.id);
        let standing = here.standing.get(&s.id).cloned().unwrap_or_else(|| {
            if mine.is_some_and(|id| store.keys.contains(&id)) {
                SessionStanding::Joined
            } else {
                SessionStanding::NoKeys
            }
        });
        SessionSummary {
            id: s.id.clone(),
            me: Self::writer_named(server, &me),
            partner: partner.map(|h| Self::writer_named(server, &h)),
            prompt: server
                .prompt_info(s.prompt)
                .map(|(title, ..)| (s.prompt, title)),
            standing,
            ended: s.ended,
            notes: s.notes.len(),
            unseen: channels.iter().map(|c| c.unseen).sum(),
            latest: s.notes.last().map(SimNote::at),
            opened: here.opened.contains(&s.id),
        }
    }

    fn channels_of(here: &Here, s: &SimSession, me: &str) -> Vec<ChannelView> {
        // Story and Planning come first, described if either was.
        let about = |n: &str| {
            s.channels
                .iter()
                .find(|(c, _)| c == n)
                .and_then(|(_, a)| a.clone())
        };
        let names = DEFAULT_CHANNELS
            .iter()
            .map(|n| ((*n).to_owned(), about(n)))
            .chain(
                s.channels
                    .iter()
                    .filter(|(n, _)| !DEFAULT_CHANNELS.contains(&n.as_str()))
                    .cloned(),
            );
        names
            .map(|(name, summary)| {
                let shown = here
                    .shown
                    .get(&(s.id.clone(), name.clone()))
                    .copied()
                    .unwrap_or(0);
                let mine = s.notes.iter().filter(|n| n.channel == name);
                ChannelView {
                    notes: mine.clone().count(),
                    unseen: mine.filter(|n| n.author != me && n.seq > shown).count(),
                    name,
                    summary,
                }
            })
            .collect()
    }

    fn sessions_here(&self) -> Here {
        self.open_store()
            .map(|(_, store, _)| store.sessions)
            .unwrap_or_default()
    }

    pub(super) fn sim_sessions(&self) -> Result<Vec<SessionSummary>, Problem> {
        let (login, server) = self.signed_in()?;
        let mut out: Vec<SessionSummary> = server
            .sessions
            .all
            .iter()
            .filter(|s| s.mine(&login).is_some())
            .map(|s| self.summary_of(&server, s, &login))
            .collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.latest));
        Ok(out)
    }

    pub(super) fn sim_session(&self, id: &str) -> Result<SessionView, Problem> {
        let (login, server) = self.signed_in()?;
        let s = server
            .sessions
            .all
            .iter()
            .find(|s| s.id == id)
            .ok_or(Problem::Gone)?;
        let summary = self.summary_of(&server, s, &login);
        let refuse = |text: &str| Err(refused(codes::SESSION_CHECK_FAILED, text));
        match &summary.standing {
            SessionStanding::Joined => {}
            SessionStanding::Waiting => {
                return refuse(
                    "this device isn't in the session yet: another of your devices for this persona adds it when it next runs",
                );
            }
            SessionStanding::NoKeys => return refuse("this persona has no keys on this device"),
            SessionStanding::Rejoining => {
                return refuse(
                    "this device was away too long to follow the session: it's waiting to be let back in by another of your devices or the other writer",
                );
            }
            SessionStanding::Broken { code, detail } => {
                return Err(refused(*code, detail));
            }
        }
        let me = summary.me.handle.clone();
        let notes = s
            .notes
            .iter()
            .map(|n| Note {
                seq: n.seq,
                mine: n.author == me,
                author: Self::writer_named(&server, &n.author),
                channel: n.channel.clone(),
                content: n.content(),
                deleted: n.deleted(),
                edited: n.versions.len() > 1,
                reply_to: n.reply_to,
                at: n.at(),
                files: n
                    .files
                    .iter()
                    .map(|f| NoteFile {
                        key: f.key.clone(),
                        name: f.name.clone(),
                        size: f.bytes.len() as u64,
                        media_type: f.media_type.clone(),
                        here: !self.0.borrow().missing_here.contains(&f.key),
                    })
                    .collect(),
            })
            .collect();
        let here = self.sessions_here();
        let verification = summary
            .partner
            .as_ref()
            .map_or(Verification::Unverified, |p| {
                self.verification(&server, &p.handle)
            });
        Ok(SessionView {
            channels: Self::channels_of(&here, s, &me),
            summary,
            notes,
            held_back: false,
            history_missing: false,
            verification,
        })
    }

    pub(super) fn sim_mark_shown(&self, id: &str, channel: &str, upto: u64) -> Result<(), Problem> {
        self.with_server_mut(|_, store| {
            let e = store
                .sessions
                .shown
                .entry((id.to_owned(), channel.to_owned()))
                .or_default();
            *e = (*e).max(upto);
        });
        Ok(())
    }

    pub(super) fn sim_mark_opened(&self, id: &str) -> Result<(), Problem> {
        self.with_server_mut(|_, store| {
            store.sessions.opened.insert(id.to_owned());
        });
        Ok(())
    }

    pub(super) fn sim_draft(&self, id: &str, channel: &str) -> Result<Draft, Problem> {
        Ok(self
            .sessions_here()
            .drafts
            .get(&(id.to_owned(), channel.to_owned()))
            .cloned()
            .unwrap_or_default())
    }

    pub(super) fn sim_drafted(&self, id: &str) -> Result<Vec<String>, Problem> {
        let mut out: Vec<String> = self
            .sessions_here()
            .drafts
            .keys()
            .filter(|(s, _)| s == id)
            .map(|(_, c)| c.clone())
            .collect();
        out.sort();
        Ok(out)
    }

    pub(super) fn sim_waiting_notes(&self, id: &str) -> Result<Vec<QueuedNote>, Problem> {
        Ok(self
            .sessions_here()
            .queued
            .into_iter()
            .filter(|(s, _)| s == id)
            .map(|(_, q)| q)
            .collect())
    }

    pub(super) fn sim_save_draft(&self, id: &str, channel: &str, d: &Draft) -> Result<(), Problem> {
        self.with_server_mut(|_, store| {
            let k = (id.to_owned(), channel.to_owned());
            if d.text.trim().is_empty() && d.reply_to.is_none() {
                store.sessions.drafts.remove(&k);
            } else {
                store.sessions.drafts.insert(k, d.clone());
            }
        });
        Ok(())
    }

    /// Queues a note as `plait-core` does: not empty, not too long.
    pub(super) fn sim_send_note(
        &self,
        id: &str,
        channel: &str,
        text: &str,
        reply_to: Option<u64>,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<Sending, Problem> {
        let text = text.trim();
        if (text.is_empty() && files.is_empty()) || text.chars().count() > NOTE_MAX {
            return Err(refused(
                codes::SESSION_CHECK_FAILED,
                "a note can't be empty or longer than the limit",
            ));
        }
        let note = QueuedNote {
            id: Uuid::now_v7(),
            channel: channel.to_owned(),
            text: text.to_owned(),
            reply_to,
            refused: None,
            files: files.iter().map(|(n, _)| n.clone()).collect(),
        };
        let mine = note.id;
        self.with_server_mut(|_, store| {
            if !files.is_empty() {
                store.sessions.queued_files.insert(mine, files);
            }
            store.sessions.queued.push((id.to_owned(), note));
        });
        self.sim_flush(id, Some(mine))
    }

    /// Reads a file from this computer's disk, as native reads one, and
    /// refuses what the server wouldn't take.
    pub(super) fn sim_attach(&self, path: &str) -> Result<Attached, Problem> {
        let (_, _, server) = self.reach()?;
        let name = path.rsplit('/').next().unwrap_or(path).to_owned();
        if !server.attachments {
            return Err(Problem::refused(
                codes::ATTACHMENTS_OFF,
                format!(
                    "{} doesn't take attachments: its administrators keep sessions to writing.",
                    server.domain
                ),
            ));
        }
        let contents = self.0.borrow().disk.get(path).cloned().ok_or_else(|| {
            Problem::refused(
                codes::ATTACHMENT_UNREADABLE,
                format!("Couldn't read {name}: No such file or directory (os error 2)."),
            )
        })?;
        if contents.len() as u64 > server.max_attachment_bytes {
            return Err(Problem::refused(
                codes::ATTACHMENT_TOO_LARGE,
                format!(
                    "{name} is too large: {} takes files up to {}.",
                    server.domain,
                    crate::storage::size(server.max_attachment_bytes)
                ),
            ));
        }
        let a = Attached {
            id: Uuid::now_v7(),
            name: name.clone(),
            size: contents.len() as u64,
        };
        self.0.borrow_mut().attached.insert(a.id, (name, contents));
        Ok(a)
    }

    /// What the system's file dialog answers from now: these paths on this
    /// computer's disk (`put_on_disk`), or none as if cancelled.
    pub fn files_pick(&self, paths: &[&str]) {
        self.0.borrow_mut().files_pick = paths.iter().map(|p| (*p).to_owned()).collect();
    }

    /// Sends a session's waiting notes in order, as `drafts::flush` does:
    /// nothing while the server is down, and it stops at a refusal.
    pub(super) fn sim_flush(&self, id: &str, mine: Option<Uuid>) -> Result<Sending, Problem> {
        let sent = self.with_server_mut(|server, store| {
            if !server.reachable {
                return None;
            }
            let login = store.login.clone().unwrap_or_default();
            let mut sent = Vec::new();
            while let Some(i) = store.sessions.queued.iter().position(|(s, _)| s == id) {
                let q = store.sessions.queued[i].1.clone();
                if let Some((code, detail)) = q.refused {
                    return Some(Err((q.id, code, detail)));
                }
                let session = server.session_mut(id);
                let Some((_, handle)) = session.mine(&login).cloned().filter(|_| !session.ended)
                else {
                    let why = (
                        codes::SESSION_CHECK_FAILED,
                        "the session has ended".to_owned(),
                    );
                    store.sessions.queued[i].1.refused = Some(why.clone());
                    return Some(Err((q.id, why.0, why.1)));
                };
                let files = store
                    .sessions
                    .queued_files
                    .get(&q.id)
                    .cloned()
                    .unwrap_or_default();
                // Each file goes up in turn, checked as the server checks it;
                // one the server took stays taken for a retry (#125).
                let domain = server.domain.clone();
                let mut up = store
                    .sessions
                    .queued_uploaded
                    .get(&q.id)
                    .cloned()
                    .unwrap_or_default();
                for (n, (_, bytes)) in files.iter().enumerate().skip(up.len()) {
                    if let Some((at, cut)) = server.interrupt_before_file
                        && at == n + 1
                    {
                        server.interrupt_before_file = None;
                        match cut {
                            Interrupt::Away => server.reachable = false,
                            Interrupt::AttachmentsOff => server.attachments = false,
                        }
                    }
                    let why = if !server.reachable {
                        store.sessions.queued_uploaded.insert(q.id, up);
                        return Some(Ok(sent));
                    } else if !server.attachments {
                        Some((
                            codes::ATTACHMENTS_OFF,
                            "this server doesn't take attachments".to_owned(),
                        ))
                    } else if bytes.len() as u64 > server.max_attachment_bytes {
                        Some((
                            codes::TOO_LARGE,
                            "an attachment is larger than this server takes".to_owned(),
                        ))
                    } else {
                        None
                    };
                    if let Some(why) = why {
                        store.sessions.queued_uploaded.insert(q.id, up);
                        store.sessions.queued[i].1.refused = Some(why.clone());
                        return Some(Err((q.id, why.0, why.1)));
                    }
                    let session = server.session_mut(id);
                    let k = session.files.len();
                    session
                        .files
                        .push((login.clone(), padded(bytes.len() as u64)));
                    up.push(format!("https://{domain}/api/v1/blobs/{id}-{k}"));
                }
                let session = server.session_mut(id);
                let seq = session.write(&handle, &q.channel, &q.text, q.reply_to);
                for ((name, bytes), key) in files.into_iter().zip(up) {
                    session.note_mut(seq).files.push(SimFile {
                        key,
                        name: name.clone(),
                        media_type: media_type(&name).to_owned(),
                        bytes,
                    });
                }
                store.sessions.queued_uploaded.remove(&q.id);
                store.sessions.queued_files.remove(&q.id);
                store.sessions.queued.remove(i);
                sent.push(q.id);
            }
            Some(Ok(sent))
        });
        Ok(match sent {
            None => Sending::Waiting,
            Some(Err((at, code, detail))) if mine.is_none_or(|m| m == at) => {
                Sending::Refused(code, detail)
            }
            Some(Err(_)) => Sending::Waiting,
            Some(Ok(sent)) => {
                self.live();
                if mine.is_none_or(|m| sent.contains(&m)) {
                    Sending::Sent
                } else {
                    Sending::Waiting
                }
            }
        })
    }

    /// "Try again" clears a refusal before sending, as `drafts::retry` does.
    pub(super) fn sim_retry(&self, id: &str) {
        self.with_server_mut(|_, store| {
            for (s, q) in &mut store.sessions.queued {
                if s == id {
                    q.refused = None;
                }
            }
        });
    }

    pub(super) fn sim_change_waiting(
        &self,
        id: &str,
        note: Uuid,
        text: &str,
    ) -> Result<(), Problem> {
        self.with_server_mut(|_, store| {
            let q = store
                .sessions
                .queued
                .iter_mut()
                .find(|(s, q)| s == id && q.id == note)
                .ok_or(Problem::Gone)?;
            q.1.text = text.trim().to_owned();
            q.1.refused = None;
            Ok(())
        })
    }

    pub(super) fn sim_withdraw_waiting(&self, id: &str, note: Uuid) -> Result<(), Problem> {
        self.with_server_mut(|_, store| {
            let before = store.sessions.queued.len();
            store
                .sessions
                .queued
                .retain(|(s, q)| !(s == id && q.id == note));
            if store.sessions.queued.len() == before {
                Err(Problem::Gone)
            } else {
                store.sessions.queued_files.remove(&note);
                store.sessions.queued_uploaded.remove(&note);
                Ok(())
            }
        })
    }

    pub(super) fn sim_note_versions(
        &self,
        id: &str,
        seq: u64,
    ) -> Result<Vec<NoteVersion>, Problem> {
        let (_, server) = self.signed_in()?;
        let s = server
            .sessions
            .all
            .iter()
            .find(|s| s.id == id)
            .ok_or(Problem::Gone)?;
        let n = s.notes.iter().find(|n| n.seq == seq).ok_or(Problem::Gone)?;
        Ok(n.versions
            .iter()
            .map(|(content, at)| NoteVersion {
                at: *at,
                content: content.clone(),
            })
            .collect())
    }

    /// The writer changes one of their own notes, refused as
    /// `sessions::edit`, `delete` and `restore` refuse.
    pub(super) fn sim_change_note(&self, id: &str, seq: u64, c: Change<'_>) -> Result<(), Problem> {
        let refuse = |text: &str| Err(refused(codes::SESSION_CHECK_FAILED, text));
        self.writing(|server, login| {
            let session = server.session_mut(id);
            let Some((_, handle)) = session.mine(login).cloned().filter(|_| !session.ended) else {
                return refuse("the session has ended");
            };
            let n = session.note_mut(seq);
            if n.author != handle {
                return refuse("only its author can change a message");
            }
            let content = match c {
                Change::Edit(_) | Change::Delete if n.deleted() => {
                    return refuse("that message was deleted");
                }
                Change::Restore if !n.deleted() => return refuse("that message isn't deleted"),
                Change::Edit(text) => {
                    let text = text.trim();
                    if text.is_empty() || text.chars().count() > NOTE_MAX {
                        return refuse("a note can't be empty or longer than the limit");
                    }
                    Some(text.to_owned())
                }
                Change::Delete => None,
                Change::Restore => Some(n.content()),
            };
            session.change(seq, content);
            Ok(())
        })?;
        self.live();
        Ok(())
    }

    /// The session's safety numbers, as `sessions::safety_numbers` gives
    /// them; `verify` marks the other writer verified, as
    /// `confirm_verified` does. The 6 words follow the session's keys,
    /// which in Sim never change.
    pub(super) fn sim_safety_numbers(
        &self,
        id: &str,
        verify: bool,
    ) -> Result<SafetyNumbers, Problem> {
        let view = self.sim_session(id)?;
        let partner = view.summary.partner.ok_or_else(|| {
            refused(
                codes::SESSION_CHECK_FAILED,
                "the other writer has left this session",
            )
        })?;
        let (identity, verified) =
            self.sim_identity_check(&view.summary.me.handle, &partner.handle, verify)?;
        let mut check = super::requests::words((id, 1), ("epoch", 1));
        check.truncate(6);
        Ok(SafetyNumbers {
            identity,
            check,
            verified,
        })
    }

    /// The writer leaves, as `sessions::leave` does: the session ends for
    /// both, and stays readable. Leaving one already ended is refused.
    pub(super) fn sim_leave(&self, id: &str) -> Result<(), Problem> {
        self.writing(|server, login| {
            let session = server.session_mut(id);
            if session.mine(login).is_none() {
                return Err(Problem::Gone);
            }
            session.ended = true;
            Ok(())
        })?;
        self.live();
        Ok(())
    }

    /// Names a channel or describes it again, refused as
    /// `sessions::name_channel` refuses.
    pub(super) fn sim_name_channel(
        &self,
        id: &str,
        name: &str,
        about: Option<&str>,
    ) -> Result<(), Problem> {
        let refuse = |text: String| Err(refused(codes::SESSION_CHECK_FAILED, &text));
        if let Err(e) = plait_proto::validate::channel(name) {
            return refuse(e.to_string());
        }
        let about = match plait_proto::validate::summary(about) {
            Ok(a) => a,
            Err(e) => return refuse(format!("description {e}")),
        };
        self.writing(|server, login| {
            let session = server.session_mut(id);
            if session.mine(login).is_none() || session.ended {
                return refuse("the session has ended".into());
            }
            session.seq += 1;
            match session.channels.iter_mut().find(|(n, _)| n == name) {
                Some(c) => c.1 = about,
                None => session.channels.push((name.to_owned(), about)),
            }
            Ok(())
        })?;
        self.live();
        Ok(())
    }

    /// What upkeep does once the server can be reached: every session's
    /// waiting notes go.
    pub(super) fn sim_flush_all(&self) {
        let ids: Vec<String> = self
            .sessions_here()
            .queued
            .iter()
            .map(|(s, _)| s.clone())
            .collect();
        for id in ids {
            let _ = self.sim_flush(&id, None);
        }
    }

    /// The notes waiting on this computer, in a session: (text, refused).
    pub fn waiting_in(&self, id: &str) -> Vec<(String, bool)> {
        self.sessions_here()
            .queued
            .iter()
            .filter(|(s, _)| s == id)
            .map(|(_, q)| (q.text.clone(), q.refused.is_some()))
            .collect()
    }

    /// Whether a session has ended, on the server.
    pub fn session_ended(&self, domain: &str, id: &str) -> bool {
        self.server(domain, |s| s.session_mut(id).ended)
    }

    /// The named channels in a session beyond Story and Planning, as the
    /// server holds them, with their descriptions.
    pub fn channels_in(&self, domain: &str, id: &str) -> Vec<(String, Option<String>)> {
        self.server(domain, |s| s.session_mut(id).channels.clone())
    }

    /// The notes in a session, as the server holds them: (author, channel,
    /// text, answering).
    pub fn notes_in(&self, domain: &str, id: &str) -> Vec<(String, String, String, Option<u64>)> {
        self.server(domain, |s| {
            s.session_mut(id)
                .notes
                .iter()
                .map(|n| {
                    (
                        n.author.clone(),
                        n.channel.clone(),
                        if n.deleted() {
                            String::new()
                        } else {
                            n.content()
                        },
                        n.reply_to,
                    )
                })
                .collect()
        })
    }
}
