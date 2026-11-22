//! What the views need from a client: implemented over `plait-core` by
//! `plait-gui` (and later the web client), and by a fake in tests.
//!
//! Every call is async and returns a [`Problem`] the views can show as is:
//! the backend turns `plait-core`'s errors into plain words, so no view
//! ever shows a raw error outside advanced mode.

use std::{future::Future, pin::Pin};

use plait_proto::board::Maturity;
pub use plait_proto::codes::{self, ErrorCode};
pub use plait_proto::{
    api::Persona,
    board::{
        AccountSettings, CreatePrompt, CreateReport, Prompt, PromptPage, PromptQuery, Tag,
        TagNamespace, TagPage, TagPageQuery, TagPolicy, TagQuery, UpdatePrompt,
    },
    notices::AccountNotice,
    requests::RequestStatus,
};
use uuid::Uuid;

/// The shortest store passphrase, as in the CLI.
pub const PASSPHRASE_MIN_CHARS: usize = 12;

/// A backend call in flight. Not `Send`: the web client's futures aren't.
pub type Call<'a, T> = Pin<Box<dyn Future<Output = Result<T, Problem>> + 'a>>;

/// Something went wrong, said for the writer. Each has an error code
/// (`docs/error-codes.md`), shown beside the words in advanced mode.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Problem {
    /// The server couldn't be reached: offline, or the address is wrong.
    #[error("Can't reach {0} right now.")]
    Unreachable(String),
    /// The login's token is gone or expired: sign in again.
    #[error("You're signed out of this server.")]
    SignedOut,
    /// This computer has no keychain to keep a store's key in: the writer
    /// must choose a passphrase (adding a server) or type it (opening one).
    #[error("This computer has no keychain for Plait's key; use a passphrase.")]
    NeedsPassphrase,
    /// The name asked for (a login or a handle) isn't available.
    #[error("That name isn't available.")]
    Taken(ErrorCode),
    /// What was asked for is gone: deleted, or no longer shown to you.
    #[error("That's gone: it was deleted, or isn't shown to you any more.")]
    Gone,
    /// The server rules must be accepted first (read them, then accept):
    /// on joining, or after they change.
    #[error("{1}")]
    RulesNotAccepted(ErrorCode, String),
    /// Another writer's identity key changed since this computer last saw
    /// it: nothing goes to them until the writer checks it (3e).
    #[error("@{0}'s identity key changed since you last wrote with them.")]
    KeyChanged(String),
    /// Refused, by the server or this computer, with the reason in plain
    /// words.
    #[error("{1}")]
    Refused(ErrorCode, String),
    /// Anything else. `detail` is shown only in advanced mode.
    #[error("Something went wrong.")]
    Failed { code: ErrorCode, detail: String },
}

impl Problem {
    /// A refusal with its code.
    pub fn refused(code: ErrorCode, text: impl Into<String>) -> Self {
        Problem::Refused(code, text.into())
    }

    /// The underlying error, for advanced mode.
    pub fn detail(&self) -> Option<&str> {
        match self {
            Problem::Failed { detail, .. } => Some(detail),
            _ => None,
        }
    }

    /// Its error code (`docs/error-codes.md`).
    pub fn code(&self) -> ErrorCode {
        match self {
            Problem::Unreachable(_) => codes::UNREACHABLE,
            Problem::SignedOut => codes::SIGNED_OUT,
            Problem::NeedsPassphrase => codes::NO_KEYCHAIN,
            Problem::Gone => codes::NOT_FOUND,
            Problem::KeyChanged(_) => codes::KEY_CHANGED,
            Problem::Taken(c)
            | Problem::RulesNotAccepted(c, _)
            | Problem::Refused(c, _)
            | Problem::Failed { code: c, .. } => *c,
        }
    }
}

/// A server this app knows, from its settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerEntry {
    /// The app's own id for the entry (not the server's).
    pub id: Uuid,
    /// As typed, normalized: `https://writers.example`.
    pub address: String,
    /// The server's domain, or a name the writer gave it.
    pub name: String,
    /// The login used there, once signed in.
    pub login: Option<String>,
}

/// What a server says about itself, before signing in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub address: String,
    pub domain: String,
    pub registration_open: bool,
    /// Whether the server has rules.
    pub has_rules: bool,
    /// Whether a new login must accept them: the server asks on joining,
    /// or will ask before its first post. Sign-up asks either way.
    pub rules_to_accept: bool,
    /// For advanced mode.
    pub software: String,
    pub version: String,
}

/// The open server's rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerRules {
    /// Markdown, which the client renders itself (never the server's HTML).
    pub markdown: String,
    /// Accepting names the version read.
    pub version: u32,
    /// Signed in: whether this login must accept this version to keep
    /// posting, editing, commenting and reporting.
    pub must_accept: bool,
}

/// Where the open server's login stands, which decides what the app shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// The store is protected by a passphrase not yet typed this session.
    Locked,
    /// No token, or the server refused it.
    SignedOut,
    /// Signed in. `personas` with keys on this computer are ready to use.
    SignedIn {
        login: String,
        personas: Vec<PersonaState>,
    },
}

/// One persona and whether this computer can write as it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaState {
    pub persona: Persona,
    pub keys: Keys,
}

/// The state of a persona's keys on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keys {
    /// Ready: an identity key and a certified device here.
    Ready,
    /// Nothing published yet, or only part of it: getting ready can finish.
    ToSetUp,
    /// The persona's identity key lives on another device: link this
    /// computer to it (3d) or restore a backup (3h).
    Elsewhere,
}

/// A step of getting a persona ready, for the progress screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    IdentityKey,
    ThisComputer,
    KeyPackages,
}

impl Step {
    pub const ALL: [Step; 3] = [Step::IdentityKey, Step::ThisComputer, Step::KeyPackages];

    pub fn label(self) -> &'static str {
        match self {
            Step::IdentityKey => "Making your persona's identity key",
            Step::ThisComputer => "Certifying this computer",
            Step::KeyPackages => "Getting ready for writing sessions",
        }
    }
}

/// The app's own settings (outside any store).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    pub theme: Theme,
    /// Text size in percent of the default.
    pub text_size: u16,
    pub advanced: bool,
    /// The folder holding the stores, for advanced mode.
    pub stores_folder: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// How the open server's store is locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    /// By a key in the OS keychain: nothing to type.
    Keychain,
    /// By a passphrase typed when the app opens it.
    Passphrase,
}

/// Board settings kept on the server for the login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardSettings {
    pub maturity_max: Maturity,
}

/// What advanced mode shows about the open server and this computer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Details {
    /// Label and value pairs, in order.
    pub rows: Vec<(String, String)>,
}

/// A link this computer opened, waiting for a device already in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkStart {
    /// The whole `plait-link:` code, for the QR code and for copying.
    pub code: String,
    /// The six words to type on the device in use.
    pub words: Vec<String>,
    /// How long the link lasts.
    pub expires_in_secs: u64,
}

/// What approving a new computer's link came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    /// Linked: both devices should now show these words.
    Approved { confirm: Vec<String> },
    /// No new computer is waiting (or its link expired).
    NoneWaiting,
    /// More than one is waiting: someone else may be linking, so none is
    /// approved by words alone.
    Several(usize),
    /// The words don't match the computer that's waiting.
    WordsDiffer,
}

/// One of the login's devices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub id: Uuid,
    pub this_device: bool,
    pub name: Option<String>,
    /// When it joined the login's devices, in seconds since 1970.
    pub since: Option<u64>,
    /// The personas it can write as, by handle.
    pub personas: Vec<String>,
}

/// Why a device is removed, recorded with its persona keys' revocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemoveReason {
    Lost,
    Compromised,
    Retired,
    #[default]
    Unspecified,
}

impl RemoveReason {
    pub const ALL: [RemoveReason; 4] = [
        RemoveReason::Retired,
        RemoveReason::Lost,
        RemoveReason::Compromised,
        RemoveReason::Unspecified,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RemoveReason::Retired => "I don't use it any more",
            RemoveReason::Lost => "It was lost or stolen",
            RemoveReason::Compromised => "Someone else may have used it",
            RemoveReason::Unspecified => "I'd rather not say",
        }
    }
}

/// A persona's keys as advanced mode shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaKeys {
    pub handle: String,
    /// Label and value pairs, in order.
    pub rows: Vec<(String, String)>,
}

/// The longest pitch, in characters (as `plait-core` allows).
pub const PITCH_MAX: usize = 4000;
/// The most days a request waits for an answer.
pub const REQUEST_DAYS_MAX: u32 = plait_proto::requests::MAX_EXPIRY_DAYS;

/// One of the login's own requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentRequest {
    pub id: Uuid,
    /// The prompt and its title; `None` once it's deleted.
    pub prompt: Option<(Uuid, String)>,
    /// The asking persona's handle.
    pub persona: String,
    /// `Waiting`, `Session` or `NoResponse`: a requester never sees more.
    pub status: RequestStatus,
    pub expires_at: time::OffsetDateTime,
}

/// One of the login's prompts with requests waiting on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingPrompt {
    pub prompt: Uuid,
    pub title: String,
    pub pending: u32,
    /// Held while the prompt was busy; reading them releases them.
    pub held: u32,
    pub newest: time::OffsetDateTime,
}

/// A request in one of the login's queues, as this computer reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingRequest {
    pub id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub received_at: time::OffsetDateTime,
    pub expires_at: time::OffsetDateTime,
    /// The pitch (Markdown), or why this computer can't show it.
    pub pitch: Result<String, Unread>,
}

/// Why this computer can't show a pitch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unread {
    /// It was sent before this computer was linked.
    NotForThisComputer,
    /// This computer sent it, under another login on the same server: its
    /// note of sending stops it reading it as the author.
    SentFromHere,
    /// The requester's identity key changed: check it first.
    KeyChanged,
    /// It failed a check, and is never shown. `detail` is for advanced mode.
    Refused { detail: String },
}

impl Unread {
    /// Its error code (`docs/error-codes.md`).
    pub fn code(&self) -> ErrorCode {
        match self {
            Unread::NotForThisComputer => codes::REQUEST_NOT_SENT_HERE,
            Unread::SentFromHere => codes::REQUEST_SENT_FROM_HERE,
            Unread::KeyChanged => codes::KEY_CHANGED,
            Unread::Refused { .. } => codes::REQUEST_REFUSED,
        }
    }
}

/// A prompt's queue.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Inbox {
    pub requests: Vec<IncomingRequest>,
    /// Held requests not yet released.
    pub held: u32,
}

/// Another writer's changed identity key, to check before going on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChange {
    pub handle: String,
    /// 12 words from both personas' keys: the same for both writers.
    pub words: Vec<String>,
    /// The new key's fingerprint, which accepting names.
    pub fingerprint: String,
}

// ---- Sessions (3f) ---------------------------------------------------------

/// A writer as a session shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Writer {
    pub handle: String,
    pub display_name: Option<String>,
}

impl Writer {
    /// "Wren (@wren)", or "@wren" without a display name.
    pub fn name(&self) -> String {
        match &self.display_name {
            Some(n) => format!("{n} (@{})", self.handle),
            None => format!("@{}", self.handle),
        }
    }
}

/// Where this computer stands in a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStanding {
    /// In it, reading and writing.
    Joined,
    /// Another of the persona's devices adds this one when it next runs.
    Waiting,
    /// The persona's keys aren't on this computer.
    NoKeys,
    /// Away too long: waiting to be let back in.
    Rejoining,
    /// It stopped reading this session; `detail` for advanced mode.
    Broken { code: ErrorCode, detail: String },
}

/// One of the login's sessions, for the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    /// The session's id. Never shown outside advanced mode.
    pub id: String,
    pub me: Writer,
    /// The other writer; `None` once they've left.
    pub partner: Option<Writer>,
    /// The prompt it began from and its title; `None` once deleted.
    pub prompt: Option<(Uuid, String)>,
    pub standing: SessionStanding,
    pub ended: bool,
    pub notes: usize,
    /// Notes this computer hasn't shown yet, in every channel.
    pub unseen: usize,
    /// When the latest note was written.
    pub latest: Option<time::OffsetDateTime>,
    /// Whether this computer has opened it yet. Until then it's new here,
    /// even with nothing unread (the requester's only note is their pitch).
    pub opened: bool,
}

/// A channel in a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelView {
    pub name: String,
    pub summary: Option<String>,
    pub notes: usize,
    pub unseen: usize,
}

/// A note being written in one channel, kept on this computer as it's typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub text: String,
    /// The note it answers, by its place in the session.
    pub reply_to: Option<u64>,
}

/// A note written on this computer and not sent yet: offline, or refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedNote {
    pub id: Uuid,
    pub channel: String,
    pub text: String,
    pub reply_to: Option<u64>,
    /// Why the server refused it; it waits to be changed or withdrawn.
    pub refused: Option<(ErrorCode, String)>,
}

/// What became of a note sent: it went, it waits for the server, or the
/// server refused it (it waits to be changed or withdrawn).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sending {
    Sent,
    Waiting,
    Refused(ErrorCode, String),
}

/// The longest note, in characters (as `plait-core` allows).
pub const NOTE_MAX: usize = 20_000;

/// A note, as this computer holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Its number in the session (the log position of its first version).
    pub seq: u64,
    pub mine: bool,
    pub author: Writer,
    pub channel: String,
    /// Markdown: the latest version, or the last before it was deleted.
    pub content: String,
    pub deleted: bool,
    pub edited: bool,
    /// The number of the note it answers, if this computer has it.
    pub reply_to: Option<u64>,
    pub at: time::OffsetDateTime,
    /// Files it carries (shown in 3g).
    pub attachments: usize,
}

/// One version of a note, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteVersion {
    pub at: time::OffsetDateTime,
    /// Its text, or `None` where it was deleted.
    pub content: Option<String>,
}

/// Whether this computer has compared safety numbers with the session's
/// other writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    /// Never compared here (comparing is offered, never required).
    Unverified,
    /// Compared, and marked verified for their current identity key.
    Verified,
    /// Marked verified, but their identity key has changed since.
    KeyChanged,
}

/// The words two writers compare to be sure no one is between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetyNumbers {
    /// 12 words from both writers' identity keys: the same as on Personas
    /// and in requests, and for both writers.
    pub identity: Vec<String>,
    /// 6 words for the session's keys now. They change when its keys do
    /// (a device added or removed), so both writers compare them together.
    pub check: Vec<String>,
    /// Marked verified on this computer, for their current key.
    pub verified: bool,
}

/// A session, read on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    pub summary: SessionSummary,
    pub channels: Vec<ChannelView>,
    /// In display order.
    pub notes: Vec<Note>,
    /// Notes the server seems to have held back from this computer.
    pub held_back: bool,
    /// Part of the session's history isn't on this computer yet.
    pub history_missing: bool,
    pub verification: Verification,
}

/// The longest device name, in characters (as `plait-core` allows).
pub const DEVICE_NAME_MAX: usize = 40;

/// The client's side of the app.
pub trait Backend {
    // ---- Servers -------------------------------------------------------------------

    /// The servers this app knows, in the order added.
    fn servers(&self) -> Vec<ServerEntry>;
    /// The server open now, if any.
    fn current(&self) -> Option<Uuid>;
    /// Asks a server about itself (for the address field, as you type).
    fn check_server<'a>(&'a self, address: &'a str) -> Call<'a, ServerInfo>;
    /// Adds a server (making its store) and opens it. The store's key goes
    /// in the keychain, or under `passphrase` if one is given (after a
    /// [`Problem::NeedsPassphrase`]).
    fn add_server<'a>(
        &'a self,
        address: &'a str,
        passphrase: Option<&'a str>,
    ) -> Call<'a, ServerEntry>;
    /// Opens a known server: unlocks its store and returns where its login
    /// stands.
    fn open_server(&self, id: Uuid) -> Call<'_, Standing>;
    /// Unlocks the open server's store with its passphrase.
    fn unlock<'a>(&'a self, passphrase: &'a str) -> Call<'a, Standing>;
    /// Forgets a server and deletes its store (and its keychain key).
    fn remove_server(&self, id: Uuid) -> Call<'_, ()>;
    /// Gives a server a name of the writer's choosing.
    fn rename_server(&self, id: Uuid, name: &str) -> Result<(), Problem>;

    // ---- The open server's store -------------------------------------------------------

    /// Whether this computer has a keychain to keep stores' keys in.
    fn keychain_available(&self) -> bool;
    /// How the open server's store is locked, once open.
    fn protection(&self) -> Option<Protection>;
    /// Locks the open server's store with `passphrase`, or with the
    /// keychain when `None`.
    fn set_protection<'a>(&'a self, passphrase: Option<&'a str>) -> Call<'a, ()>;

    // ---- The open server's login -----------------------------------------------------

    fn standing(&self) -> Call<'_, Standing>;
    /// Makes a login. `accept_rules` is the rules version shown at sign-up,
    /// when the writer accepted it.
    fn register<'a>(
        &'a self,
        login: &'a str,
        password: &'a str,
        accept_rules: Option<u32>,
    ) -> Call<'a, Standing>;
    fn sign_in<'a>(&'a self, login: &'a str, password: &'a str) -> Call<'a, Standing>;
    fn sign_out(&self) -> Call<'_, ()>;
    /// The open server's rules, or `None` if it has none.
    fn rules(&self) -> Call<'_, Option<ServerRules>>;
    fn accept_rules(&self, version: u32) -> Call<'_, ()>;

    // ---- Personas ----------------------------------------------------------------------

    fn create_persona<'a>(&'a self, handle: &'a str, display_name: &'a str) -> Call<'a, Persona>;
    fn update_persona<'a>(
        &'a self,
        id: Uuid,
        display_name: &'a str,
        summary: &'a str,
    ) -> Call<'a, Persona>;
    fn delete_persona(&self, id: Uuid) -> Call<'_, ()>;
    /// Makes and publishes whatever `persona` lacks on this computer,
    /// calling `progress` as each step starts. Safe to call again after a
    /// failure: it carries on from where it stopped.
    fn prepare<'a>(&'a self, persona: Uuid, progress: &'a dyn Fn(Step)) -> Call<'a, ()>;
    /// As `prepare`, but makes a new identity key in place of one made on
    /// another device. That device stops working for the persona, and
    /// other writers are told the key changed.
    fn start_over<'a>(&'a self, persona: Uuid, progress: &'a dyn Fn(Step)) -> Call<'a, ()>;

    // ---- Devices ---------------------------------------------------------------------------

    /// What this computer is called on the login's other devices.
    fn device_name(&self) -> Option<String>;
    /// Whether another of the login's devices removed this computer.
    fn removed_here(&self) -> bool;
    /// A name to suggest for this computer: its own, as the system knows it.
    fn suggested_device_name(&self) -> String;
    /// Renames this computer, telling the other devices if it's linked.
    fn rename_device<'a>(&'a self, name: &'a str) -> Call<'a, ()>;
    /// The login's devices; empty until a second one is linked.
    fn devices(&self) -> Call<'_, Vec<Device>>;
    /// What the login's sessions take on the server (`plait storage`):
    /// sizes and counts of ciphertext only.
    fn storage(&self) -> Call<'_, plait_proto::sessions::AccountStorage>;
    /// Removes another of the login's devices; its persona keys are revoked.
    fn remove_device(&self, id: Uuid, reason: RemoveReason) -> Call<'_, ()>;
    /// On a new computer: names it and opens a link for a device in use.
    fn start_link<'a>(&'a self, name: &'a str) -> Call<'a, LinkStart>;
    /// Whether the link was answered: the words to compare, `None` while
    /// waiting (it returns within a few seconds), `Gone` once it expired.
    fn link_answer(&self) -> Call<'_, Option<Vec<String>>>;
    /// The words matched: takes the keys and gets the personas ready.
    fn finish_link(&self) -> Call<'_, Standing>;
    /// The words didn't match, or the person gave up: takes nothing.
    fn abandon_link(&self) -> Result<(), Problem>;
    /// On a device in use: approves the waiting link whose words (or whole
    /// code) are `typed`.
    fn approve_link<'a>(&'a self, typed: &'a str) -> Call<'a, Approval>;
    /// How many new computers are waiting to link to this login.
    fn waiting_links(&self) -> Call<'_, usize>;
    /// Each persona's keys, for advanced mode.
    fn key_details(&self) -> Call<'_, Vec<PersonaKeys>>;

    // ---- The board ------------------------------------------------------------------------

    /// The login's board settings and unread notice count.
    fn account(&self) -> Call<'_, AccountSettings>;
    /// One page of the board, newest first.
    fn prompts(&self, query: PromptQuery) -> Call<'_, PromptPage>;
    fn prompt(&self, id: Uuid) -> Call<'_, Prompt>;
    fn create_prompt(&self, body: CreatePrompt) -> Call<'_, Prompt>;
    fn update_prompt(&self, id: Uuid, body: UpdatePrompt) -> Call<'_, Prompt>;
    fn delete_prompt(&self, id: Uuid) -> Call<'_, ()>;
    /// The board's address for a prompt, to share.
    fn prompt_link(&self, id: Uuid) -> Option<String>;
    fn tag_types(&self) -> Call<'_, Vec<TagNamespace>>;
    /// Whether writers may add new tags, and whether those need approval.
    fn tag_policy(&self) -> Call<'_, TagPolicy>;
    /// One page of a tag type's tags with their prompt counts.
    fn tag_page<'a>(&'a self, namespace: &'a str, query: TagPageQuery) -> Call<'a, TagPage>;
    /// Tags whose names start with `query.prefix` (the tag picker).
    fn find_tags(&self, query: TagQuery) -> Call<'_, Vec<Tag>>;
    /// The tags the login uses most, for `persona`.
    fn frequent_tags(&self, persona: Option<Uuid>) -> Call<'_, Vec<Tag>>;
    fn report(&self, body: CreateReport) -> Call<'_, ()>;
    /// The login's notices, newest first, before notice `before`.
    fn notices(&self, before: Option<i64>) -> Call<'_, Vec<AccountNotice>>;
    fn mark_notices_read(&self) -> Call<'_, ()>;

    // ---- Requests (3e) ------------------------------------------------------------------

    /// Asks to write with `prompt`'s author as `persona`.
    fn send_request<'a>(
        &'a self,
        persona: Uuid,
        prompt: Uuid,
        pitch: &'a str,
        days: u32,
    ) -> Call<'a, SentRequest>;
    /// The login's own requests, newest first.
    fn sent_requests(&self) -> Call<'_, Vec<SentRequest>>;
    /// Withdraws a request, or dismisses one that got no answer.
    fn withdraw_request(&self, id: Uuid) -> Call<'_, ()>;
    /// The login's prompts with requests waiting, newest arrival first.
    fn waiting_requests(&self) -> Call<'_, Vec<WaitingPrompt>>;
    /// Reads a prompt's queue on this computer; `held` releases held ones.
    fn inbox(&self, prompt: Uuid, held: bool) -> Call<'_, Inbox>;
    /// Accepts a request read on this computer: it becomes a session.
    fn accept_request(&self, id: Uuid) -> Call<'_, ()>;
    /// Declines a request. The requester isn't told.
    fn decline_request(&self, id: Uuid) -> Call<'_, ()>;
    /// How `handle`'s identity key changed, as seen by `as_persona`, or
    /// `None` if it hasn't (any more).
    fn key_change<'a>(&'a self, handle: &'a str, as_persona: Uuid) -> Call<'a, Option<KeyChange>>;
    /// The 12 words for `as_persona` and `handle`, from the keys published
    /// now; `None` while `handle` has no identity key (#72).
    fn identity_words<'a>(
        &'a self,
        handle: &'a str,
        as_persona: Uuid,
    ) -> Call<'a, Option<Vec<String>>>;
    /// Trusts `handle`'s new key, if it's still the one with `fingerprint`.
    fn accept_key_change<'a>(&'a self, handle: &'a str, fingerprint: &'a str) -> Call<'a, ()>;
    /// The login's requests whose answer the writer has seen, kept on this
    /// computer (in the server's store) so Home can say what's new.
    fn answers_seen(&self) -> Vec<Uuid>;
    fn set_answers_seen(&self, ids: &[Uuid]) -> Result<(), Problem>;

    // ---- Sessions (3f) ------------------------------------------------------------------

    /// The login's sessions, synced, most recent activity first.
    fn sessions(&self) -> Call<'_, Vec<SessionSummary>>;
    /// One session, synced and read on this computer.
    fn session<'a>(&'a self, id: &'a str) -> Call<'a, SessionView>;
    /// Notes this computer has shown a channel's notes up to `upto`, for the
    /// unread counts. Kept only on this computer.
    fn mark_shown<'a>(&'a self, id: &'a str, channel: &'a str, upto: u64) -> Result<(), Problem>;
    /// Notes this computer has opened the session, so it's no longer new.
    fn mark_opened(&self, id: &str) -> Result<(), Problem>;
    /// The draft kept on this computer for a channel; empty if none.
    fn draft(&self, id: &str, channel: &str) -> Result<Draft, Problem>;
    /// The channels with a draft on this computer. Local, so it works
    /// offline.
    fn drafted(&self, id: &str) -> Result<Vec<String>, Problem>;
    /// The notes written here that haven't gone yet, oldest first. Local,
    /// so they show offline.
    fn waiting(&self, id: &str) -> Result<Vec<QueuedNote>, Problem>;
    /// Keeps a channel's draft on this computer, encrypted; an empty one is
    /// removed. Called as the writer types.
    fn save_draft(&self, id: &str, channel: &str, draft: &Draft) -> Result<(), Problem>;
    /// Sends a note: it's queued here first, then everything waiting goes in
    /// order, so a note written offline is never overtaken.
    fn send_note<'a>(
        &'a self,
        id: &'a str,
        channel: &'a str,
        text: &'a str,
        reply_to: Option<u64>,
    ) -> Call<'a, Sending>;
    /// Tries the notes waiting in a session again, in order.
    fn send_waiting<'a>(&'a self, id: &'a str) -> Call<'a, Sending>;
    /// Changes a waiting note's text; a refused one is tried again.
    fn change_waiting(&self, id: &str, note: Uuid, text: &str) -> Result<(), Problem>;
    /// Takes a waiting note back. Fails with `Gone` if it already went.
    fn withdraw_waiting(&self, id: &str, note: Uuid) -> Result<(), Problem>;
    /// Every version of one of the session's notes, oldest first.
    fn note_versions<'a>(&'a self, id: &'a str, seq: u64) -> Call<'a, Vec<NoteVersion>>;
    /// Changes one of the writer's own notes; every version is kept.
    fn edit_note<'a>(&'a self, id: &'a str, seq: u64, text: &'a str) -> Call<'a, ()>;
    /// Deletes one of the writer's own notes. It's hidden, not erased: both
    /// writers keep its versions (`docs/plan.md`, "Full edit history").
    fn delete_note<'a>(&'a self, id: &'a str, seq: u64) -> Call<'a, ()>;
    /// Brings back one of the writer's deleted notes, as a new edit (#104).
    fn restore_note<'a>(&'a self, id: &'a str, seq: u64) -> Call<'a, ()>;
    /// The session's safety numbers. Fails with `KeyChanged` if the other
    /// writer's identity key changed since this computer pinned it.
    fn safety_numbers<'a>(&'a self, id: &'a str) -> Call<'a, SafetyNumbers>;
    /// Marks the other writer verified on this computer, for their current
    /// identity key, once the words matched.
    fn confirm_verified<'a>(&'a self, id: &'a str) -> Call<'a, SafetyNumbers>;
    /// Leaves a session, which ends it for both writers. What's written
    /// stays readable on this computer.
    fn leave_session<'a>(&'a self, id: &'a str) -> Call<'a, ()>;
    /// Names a new channel, or describes one again, for both writers.
    /// `name` is as `plait_proto::validate::channel` allows.
    fn name_channel<'a>(
        &'a self,
        id: &'a str,
        name: &'a str,
        about: Option<&'a str>,
    ) -> Call<'a, ()>;

    // ---- Upkeep and settings --------------------------------------------------------------

    /// Background upkeep for the open server. Returns something to tell the
    /// writer, if anything needs them (with its code).
    fn upkeep(&self) -> Call<'_, Option<Problem>>;
    /// Waits for the server's next live notice for the login (a request,
    /// a session message, a device change). Upkeep runs after each.
    fn next_notice(&self) -> Call<'_, ()>;
    fn board_settings(&self) -> Call<'_, BoardSettings>;
    fn set_board_settings(&self, settings: BoardSettings) -> Call<'_, BoardSettings>;
    fn app_settings(&self) -> AppSettings;
    fn set_app_settings(&self, settings: AppSettings) -> Result<(), Problem>;
    /// Moves the stores to another folder (advanced).
    fn move_stores<'a>(&'a self, folder: &'a str) -> Call<'a, ()>;
    fn details(&self) -> Call<'_, Details>;
    /// This computer's offset from UTC, for showing times. UTC unless the
    /// platform says otherwise.
    fn utc_offset(&self) -> time::UtcOffset {
        time::UtcOffset::UTC
    }
    /// Waits `ms` milliseconds, on whatever timer the platform has.
    fn sleep(&self, ms: u64) -> Pin<Box<dyn Future<Output = ()> + '_>>;
}
