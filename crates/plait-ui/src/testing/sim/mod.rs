//! A simulated Plait server and client store behind the `Backend` trait,
//! for interaction tests. It keeps state like the real thing (logins,
//! personas, keys on this computer, rules and their versions, locked
//! stores) and a test can change it mid-flow, as an administrator or a
//! network outage would. It follows `plait-gui`'s `NativeBackend` closely;
//! the end-to-end tests check the real one against a real server.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    rc::Rc,
    time::Duration,
};

use plait_proto::board::Maturity;
use uuid::Uuid;

mod board;
mod devices;
mod requests;
mod sessions;

use sessions::Change;

pub use requests::State as RequestState;

use crate::backend::{
    AccountNotice, AccountSettings, CreatePrompt, CreateReport, Prompt, PromptPage, PromptQuery,
    Tag, TagNamespace, TagPage, TagPageQuery, TagPolicy, TagQuery, UpdatePrompt,
};
use crate::backend::{
    AppSettings, Backend, BoardSettings, Call, Details, Keys, PASSPHRASE_MIN_CHARS, Persona,
    PersonaState, Problem, Protection, ServerEntry, ServerInfo, ServerRules, Standing, Step, Theme,
};
use crate::backend::{ErrorCode, codes};

/// When accounts must accept a server's rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Acceptance {
    #[default]
    Never,
    OnJoin,
    OnChange,
    Both,
}

impl Acceptance {
    fn on_join(self) -> bool {
        matches!(self, Self::OnJoin | Self::Both)
    }
    fn on_change(self) -> bool {
        matches!(self, Self::OnChange | Self::Both)
    }
}

#[derive(Debug, Clone)]
struct Account {
    password: String,
    accepted: u32,
    /// The board's maturity cap for this login.
    maturity: Maturity,
}

/// One simulated server.
#[derive(Debug, Clone)]
pub struct SimServer {
    pub domain: String,
    pub reachable: bool,
    pub registration_open: bool,
    rules: Option<(String, u32)>,
    pub acceptance: Acceptance,
    /// Fetching the rules fails (a flaky connection, say).
    pub rules_fail: bool,
    /// Whether sessions here take attachments (the administrators' switch).
    pub attachments: bool,
    /// The largest attachment it takes.
    pub max_attachment_bytes: u64,
    accounts: HashMap<String, Account>,
    /// Personas by login, with whether their identity key is published.
    personas: Vec<(String, Persona, bool)>,
    board: board::Board,
    devices: devices::Devices,
    requests: requests::Requests,
    sessions: sessions::Sessions,
}

impl SimServer {
    fn new(domain: &str) -> Self {
        Self {
            domain: domain.into(),
            reachable: true,
            registration_open: true,
            rules: None,
            acceptance: Acceptance::Never,
            rules_fail: false,
            attachments: true,
            max_attachment_bytes: 10 * 1024 * 1024,
            accounts: HashMap::new(),
            personas: Vec::new(),
            board: board::Board::default(),
            devices: devices::Devices::default(),
            requests: requests::Requests::default(),
            sessions: sessions::Sessions::default(),
        }
    }
}

/// This computer's copy for one server.
#[derive(Debug, Clone)]
struct Store {
    address: String,
    login: Option<String>,
    signed_in: bool,
    passphrase: Option<String>,
    unlocked: bool,
    /// Personas whose keys are on this computer.
    keys: Vec<Uuid>,
    /// This computer among the login's devices.
    here: devices::Here,
    /// Pinned keys and requests read here.
    requests: requests::Here,
    /// How far each session's channels are shown here, and standings.
    sessions: sessions::Here,
    /// Held elsewhere in this process (work from a window just closed), so
    /// opening it fails as `StoreError::InUse` does (#103).
    busy: bool,
}

#[derive(Debug)]
struct State {
    servers: HashMap<String, SimServer>,
    entries: Vec<ServerEntry>,
    stores: HashMap<Uuid, Store>,
    current: Option<Uuid>,
    keychain: bool,
    /// Wakes waits of a minute or more (`time_passes`).
    clock: Rc<tokio::sync::Notify>,
    /// The server's live notices for the open login (a request arrived).
    live: Rc<tokio::sync::Notify>,
    /// While set, loading the session list waits for it (a slow network).
    sessions_held: Option<Rc<tokio::sync::Notify>>,
    requests_held: Option<Rc<tokio::sync::Notify>>,
    /// Calls whose reply waits (`hold_reply`), by backend method name.
    replies_held: std::collections::HashMap<&'static str, Rc<tokio::sync::Notify>>,
    /// The server whose store couldn't be opened, as native has no store
    /// open then: `standing` fails until it's opened.
    unopened: Option<Uuid>,
    settings: AppSettings,
    /// What the system's folder dialog answers (`folder_picks`).
    folder_pick: Option<String>,
    /// Files saved on this computer, by path.
    disk: std::collections::BTreeMap<String, Vec<u8>>,
    /// Attachments this computer doesn't have, by key.
    missing_here: HashSet<String>,
    /// What the system's file dialog answers (`files_pick`).
    files_pick: Vec<String>,
    /// Files attached to the next note.
    attached: HashMap<Uuid, (String, Vec<u8>)>,
    /// How the files of a note being sent are getting on.
    going: Option<crate::backend::FilesGoing>,
}

/// The simulated world: servers out there and this computer's app state.
/// Clone the `Rc` to keep a handle for changing it during a test.
pub struct Sim(RefCell<State>);

/// A server address as the app stores it.
fn address(domain: &str) -> String {
    format!("https://{domain}")
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    /// Marks the store of the server at `domain` held elsewhere in this
    /// process (`busy`), or free again.
    pub fn store_busy(&self, domain: &str, busy: bool) {
        let mut s = self.0.borrow_mut();
        let addr = address(domain);
        for store in s.stores.values_mut().filter(|st| st.address == addr) {
            store.busy = busy;
        }
    }

    pub fn new() -> Self {
        Self(RefCell::new(State {
            servers: HashMap::new(),
            entries: Vec::new(),
            stores: HashMap::new(),
            current: None,
            keychain: true,
            clock: Rc::default(),
            live: Rc::default(),
            sessions_held: None,
            requests_held: None,
            replies_held: Default::default(),
            unopened: None,
            folder_pick: None,
            disk: Default::default(),
            missing_here: HashSet::new(),
            files_pick: Vec::new(),
            attached: HashMap::new(),
            going: None,
            settings: AppSettings {
                theme: Theme::System,
                text_size: 100,
                advanced: false,
                stores_folder: "/home/writer/.local/share/plait-gui/stores".into(),
            },
        }))
    }

    pub fn rc(self) -> Rc<Self> {
        Rc::new(self)
    }

    /// Adds a server out there, reachable at `domain`.
    pub fn with_server(self, domain: &str) -> Self {
        self.0
            .borrow_mut()
            .servers
            .insert(address(domain), SimServer::new(domain));
        self
    }

    /// No OS keychain: stores need a passphrase.
    pub fn without_keychain(self) -> Self {
        self.0.borrow_mut().keychain = false;
        self
    }

    /// Loading the session list waits from now until `release_sessions`,
    /// as on a slow network: for clicks that come before a list arrives.
    pub fn hold_sessions(&self) {
        self.0.borrow_mut().sessions_held = Some(Rc::default());
    }

    /// Session lists held by `hold_sessions` arrive, and later ones don't wait.
    pub fn release_sessions(&self) {
        if let Some(n) = self.0.borrow_mut().sessions_held.take() {
            n.notify_waiters();
        }
    }

    /// Loading the requests lists (waiting and sent) waits from now until
    /// `release_requests`, as on a slow network.
    pub fn hold_requests(&self) {
        self.0.borrow_mut().requests_held = Some(Rc::default());
    }

    /// Requests lists held by `hold_requests` arrive, and later ones don't wait.
    pub fn release_requests(&self) {
        if let Some(n) = self.0.borrow_mut().requests_held.take() {
            n.notify_waiters();
        }
    }

    /// From now, the backend call `method` (`"sign_out"`, say) takes effect
    /// at once, but its reply waits until `release_reply`: a slow network,
    /// where the server has acted and the app hasn't heard yet.
    pub fn hold_reply(&self, method: &'static str) {
        self.0
            .borrow_mut()
            .replies_held
            .insert(method, Rc::default());
    }

    /// Replies held by `hold_reply` for `method` arrive; later ones don't wait.
    pub fn release_reply(&self, method: &'static str) {
        if let Some(n) = self.0.borrow_mut().replies_held.remove(method) {
            n.notify_waiters();
        }
    }

    /// `result`, after any hold on `method`'s reply.
    fn reply<'a, T: 'a>(&'a self, method: &'static str, result: Result<T, Problem>) -> Call<'a, T> {
        let held = self.0.borrow().replies_held.get(method).cloned();
        Box::pin(async move {
            if let Some(n) = held {
                n.notified().await;
            }
            result
        })
    }

    /// Every wait of a minute or more (periodic checks, upkeep) ends now,
    /// as if that long had passed. Such waits otherwise last until called.
    pub fn time_passes(&self) {
        self.0.borrow().clock.notify_waiters();
    }

    /// A live notice for the open login: its upkeep runs, as on an event.
    pub fn live(&self) {
        self.0.borrow().live.notify_one();
    }

    /// Changes a server (an administrator's settings, an outage).
    pub fn server<T>(&self, domain: &str, change: impl FnOnce(&mut SimServer) -> T) -> T {
        let mut s = self.0.borrow_mut();
        let server = s
            .servers
            .get_mut(&address(domain))
            .unwrap_or_else(|| panic!("no server {domain}"));
        change(server)
    }

    /// Sets or changes a server's rules; each change is a new version.
    pub fn set_rules(&self, domain: &str, markdown: &str, acceptance: Acceptance) {
        self.server(domain, |s| {
            let version = s.rules.as_ref().map_or(1, |(_, v)| v + 1);
            s.rules = Some((markdown.into(), version));
            s.acceptance = acceptance;
        });
    }

    /// A login that already exists on a server, with personas whose keys
    /// are on another device.
    pub fn existing_login(&self, domain: &str, login: &str, password: &str, handles: &[&str]) {
        self.server(domain, |s| {
            s.accounts.insert(
                login.into(),
                Account {
                    password: password.into(),
                    accepted: 0,
                    maturity: Maturity::DEFAULT_MAX,
                },
            );
            for h in handles {
                s.personas.push((login.into(), persona(h), true));
            }
        });
    }

    /// Another writer whose persona has no identity key published yet.
    pub fn writer_without_keys(&self, domain: &str, handle: &str) {
        self.server(domain, |s| {
            s.personas
                .push((format!("owner_of_{handle}"), persona(handle), false));
        });
    }

    /// Takes a handle on a server, as another writer would.
    pub fn take_handle(&self, domain: &str, handle: &str) {
        self.existing_login(
            domain,
            &format!("owner_of_{handle}"),
            "a long password",
            &[handle],
        );
    }

    /// The login names a server has.
    pub fn logins(&self, domain: &str) -> Vec<String> {
        let s = self.0.borrow();
        let mut v: Vec<String> = s.servers[&address(domain)]
            .accounts
            .keys()
            .cloned()
            .collect();
        v.sort();
        v
    }

    /// Which rules version `login` has accepted on a server (0: none).
    pub fn accepted(&self, domain: &str, login: &str) -> u32 {
        self.0.borrow().servers[&address(domain)].accounts[login].accepted
    }

    fn open_store(&self) -> Result<(Uuid, Store, SimServer), Problem> {
        let s = self.0.borrow();
        let id = s.current.ok_or_else(|| Problem::Failed {
            code: codes::UNCLASSIFIED,
            detail: "no server is open".into(),
        })?;
        let store = s.stores[&id].clone();
        let server = s.servers[&store.address].clone();
        Ok((id, store, server))
    }

    fn reach(&self) -> Result<(Uuid, Store, SimServer), Problem> {
        let (id, store, server) = self.open_store()?;
        if !server.reachable {
            return Err(Problem::Unreachable(server.domain));
        }
        Ok((id, store, server))
    }

    fn standing_now(&self) -> Result<Standing, Problem> {
        let (_, store, server) = self.open_store()?;
        if store.passphrase.is_some() && !store.unlocked {
            return Ok(Standing::Locked);
        }
        if !store.signed_in {
            return Ok(Standing::SignedOut);
        }
        if !server.reachable {
            return Err(Problem::Unreachable(server.domain));
        }
        let login = store.login.clone().unwrap_or_default();
        let personas = server
            .personas
            .iter()
            .filter(|(owner, _, _)| *owner == login)
            .map(|(_, p, published)| PersonaState {
                persona: p.clone(),
                keys: if store.keys.contains(&p.id) {
                    Keys::Ready
                } else if *published {
                    Keys::Elsewhere
                } else {
                    Keys::ToSetUp
                },
            })
            .collect();
        Ok(Standing::SignedIn { login, personas })
    }

    fn with_server_mut<T>(&self, f: impl FnOnce(&mut SimServer, &mut Store) -> T) -> T {
        let mut s = self.0.borrow_mut();
        let id = s.current.expect("a server is open");
        let st = &mut *s;
        let store = st.stores.get_mut(&id).expect("its store");
        let server = st.servers.get_mut(&store.address).expect("its server");
        f(server, store)
    }
}

/// A persona as the server returns it.
pub fn persona(handle: &str) -> Persona {
    Persona {
        id: Uuid::new_v4(),
        handle: handle.into(),
        actor_uri: format!("https://sim.example/actors/{handle}"),
        display_name: None,
        summary: None,
        created_at: time::OffsetDateTime::UNIX_EPOCH,
    }
}

fn now<'a, T: 'a>(v: Result<T, Problem>) -> Call<'a, T> {
    Box::pin(async move {
        // A moment's latency, as a network would have, so views see their
        // in-between states.
        tokio::time::sleep(Duration::from_millis(5)).await;
        v
    })
}

/// A refusal with the code the real server or client gives that case.
fn refused(code: ErrorCode, text: &str) -> Problem {
    Problem::Refused(code, text.into())
}

/// The address as typed, as the app normalizes it (scheme optional).
fn normalize(typed: &str) -> String {
    let t = typed.trim().trim_end_matches('/');
    let bare = t
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    address(bare)
}

impl Backend for Sim {
    fn servers(&self) -> Vec<ServerEntry> {
        self.0.borrow().entries.clone()
    }

    fn current(&self) -> Option<Uuid> {
        self.0.borrow().current
    }

    fn check_server<'a>(&'a self, typed: &'a str) -> Call<'a, ServerInfo> {
        let result = (|| {
            if typed.trim().is_empty() {
                return Err(refused(codes::BAD_ADDRESS, "Type the server's address."));
            }
            let addr = normalize(typed);
            let s = self.0.borrow();
            let server = s
                .servers
                .get(&addr)
                .filter(|s| s.reachable)
                .ok_or_else(|| Problem::Unreachable(addr.clone()))?;
            Ok(ServerInfo {
                address: addr.clone(),
                domain: server.domain.clone(),
                registration_open: server.registration_open,
                has_rules: server.rules.is_some(),
                rules_to_accept: server.rules.is_some() && server.acceptance != Acceptance::Never,
                software: "plait".into(),
                version: "0.0.0-sim".into(),
            })
        })();
        now(result)
    }

    fn add_server<'a>(
        &'a self,
        typed: &'a str,
        passphrase: Option<&'a str>,
    ) -> Call<'a, ServerEntry> {
        let result = (|| {
            let info = {
                let addr = normalize(typed);
                let s = self.0.borrow();
                let server = s
                    .servers
                    .get(&addr)
                    .filter(|s| s.reachable)
                    .ok_or_else(|| Problem::Unreachable(addr.clone()))?;
                (addr, server.domain.clone())
            };
            let mut s = self.0.borrow_mut();
            if s.entries.iter().any(|e| e.address == info.0) {
                return Err(refused(
                    codes::SERVER_ALREADY_ADDED,
                    &format!("{} is already in your list of servers.", info.1),
                ));
            }
            match passphrase {
                Some(p) if p.chars().count() < PASSPHRASE_MIN_CHARS => {
                    return Err(refused(
                        codes::PASSPHRASE_TOO_SHORT,
                        "Use at least 12 characters.",
                    ));
                }
                None if !s.keychain => return Err(Problem::NeedsPassphrase),
                _ => {}
            }
            let entry = ServerEntry {
                id: Uuid::new_v4(),
                address: info.0.clone(),
                name: info.1,
                login: None,
            };
            s.entries.push(entry.clone());
            s.stores.insert(
                entry.id,
                Store {
                    address: info.0,
                    login: None,
                    signed_in: false,
                    passphrase: passphrase.map(str::to_owned),
                    unlocked: true,
                    keys: Vec::new(),
                    here: devices::Here::default(),
                    requests: requests::Here::default(),
                    sessions: sessions::Here::default(),
                    busy: false,
                },
            );
            s.current = Some(entry.id);
            Ok(entry)
        })();
        now(result)
    }

    fn open_server(&self, id: Uuid) -> Call<'_, Standing> {
        let mut s = self.0.borrow_mut();
        s.current = Some(id);
        if s.stores.get(&id).is_some_and(|st| st.busy) {
            s.unopened = Some(id);
            return now(Err(refused(
                codes::STORE_BUSY,
                "Plait is still using this server's store. Try again in a moment.",
            )));
        }
        s.unopened = None;
        drop(s);
        now(self.standing_now())
    }

    fn unlock<'a>(&'a self, passphrase: &'a str) -> Call<'a, Standing> {
        let ok = self.with_server_mut(|_, store| {
            let ok = store.passphrase.as_deref() == Some(passphrase);
            if ok {
                store.unlocked = true;
            }
            ok
        });
        if !ok {
            return now(Err(refused(
                codes::WRONG_PASSPHRASE,
                "That isn't the passphrase.",
            )));
        }
        now(self.standing_now())
    }

    fn remove_server(&self, id: Uuid) -> Call<'_, ()> {
        {
            let mut s = self.0.borrow_mut();
            s.entries.retain(|e| e.id != id);
            s.stores.remove(&id);
            if s.current == Some(id) {
                s.current = None;
            }
        }
        self.reply("remove_server", Ok(()))
    }

    fn rename_server(&self, id: Uuid, name: &str) -> Result<(), Problem> {
        let name = name.trim();
        if name.is_empty() {
            return Err(refused(codes::SERVER_NAME_BAD, "Use 1 to 60 characters."));
        }
        if let Some(e) = self.0.borrow_mut().entries.iter_mut().find(|e| e.id == id) {
            e.name = name.into();
        }
        Ok(())
    }

    fn keychain_available(&self) -> bool {
        self.0.borrow().keychain
    }

    fn protection(&self) -> Option<Protection> {
        let (_, store, _) = self.open_store().ok()?;
        Some(if store.passphrase.is_some() {
            Protection::Passphrase
        } else {
            Protection::Keychain
        })
    }

    fn set_protection<'a>(&'a self, passphrase: Option<&'a str>) -> Call<'a, ()> {
        if passphrase.is_none() && !self.0.borrow().keychain {
            return now(Err(Problem::NeedsPassphrase));
        }
        if passphrase.is_some_and(|p| p.chars().count() < PASSPHRASE_MIN_CHARS) {
            return now(Err(refused(
                codes::PASSPHRASE_TOO_SHORT,
                "Use at least 12 characters.",
            )));
        }
        self.with_server_mut(|_, store| store.passphrase = passphrase.map(str::to_owned));
        now(Ok(()))
    }

    fn standing(&self) -> Call<'_, Standing> {
        let s = self.0.borrow();
        if s.unopened.is_some() && s.unopened == s.current {
            return now(Err(Problem::Failed {
                code: codes::UNCLASSIFIED,
                detail: "no server is open".into(),
            }));
        }
        drop(s);
        now(self.standing_now())
    }

    fn register<'a>(
        &'a self,
        login: &'a str,
        password: &'a str,
        accept_rules: Option<u32>,
    ) -> Call<'a, Standing> {
        if let Err(p) = self.reach() {
            return now(Err(p));
        }
        let login = login.trim().to_ascii_lowercase();
        let result = self.with_server_mut(|server, store| {
            if !server.registration_open {
                return Err(refused(
                    codes::REGISTRATION_CLOSED,
                    "This server isn't taking new logins.",
                ));
            }
            if server.accounts.contains_key(&login) {
                return Err(Problem::Taken(codes::LOGIN_NAME_TAKEN));
            }
            let mut accepted = 0;
            if let Some((_, version)) = &server.rules {
                match accept_rules {
                    Some(v) if v == *version => accepted = v,
                    Some(_) if server.acceptance.on_join() => {
                        return Err(Problem::RulesNotAccepted(
                            codes::RULES_CHANGED_AT_SIGNUP,
                            "The server rules changed while you were signing up. Read them again."
                                .into(),
                        ));
                    }
                    None if server.acceptance.on_join() => {
                        return Err(Problem::RulesNotAccepted(
                            codes::RULES_REQUIRED_AT_SIGNUP,
                            "Accept the server rules to create an account.".into(),
                        ));
                    }
                    _ => {}
                }
            }
            server.accounts.insert(
                login.clone(),
                Account {
                    password: password.into(),
                    accepted,
                    maturity: Maturity::DEFAULT_MAX,
                },
            );
            store.login = Some(login.clone());
            store.signed_in = true;
            Ok(())
        });
        if let Err(p) = result {
            return now(Err(p));
        }
        self.set_entry_login(Some(login));
        now(self.standing_now())
    }

    fn sign_in<'a>(&'a self, login: &'a str, password: &'a str) -> Call<'a, Standing> {
        if let Err(p) = self.reach() {
            return now(Err(p));
        }
        let login = login.trim().to_ascii_lowercase();
        let ok = self.with_server_mut(|server, store| {
            let ok = server
                .accounts
                .get(&login)
                .is_some_and(|a| a.password == password);
            if ok {
                store.login = Some(login.clone());
                store.signed_in = true;
            }
            ok
        });
        if !ok {
            return now(Err(refused(
                codes::WRONG_LOGIN,
                "That login name and password don't match.",
            )));
        }
        self.set_entry_login(Some(login));
        now(self.standing_now())
    }

    fn sign_out(&self) -> Call<'_, ()> {
        self.with_server_mut(|_, store| store.signed_in = false);
        self.reply("sign_out", Ok(()))
    }

    fn rules(&self) -> Call<'_, Option<ServerRules>> {
        if self.open_store().is_ok_and(|(_, _, s)| s.rules_fail) {
            return now(Err(Problem::Failed {
                code: codes::SERVER_FAULT,
                detail: "simulated failure".into(),
            }));
        }
        let result = self.reach().map(|(_, store, server)| {
            server
                .rules
                .as_ref()
                .map(|(markdown, version)| ServerRules {
                    markdown: markdown.clone(),
                    version: *version,
                    must_accept: store.signed_in
                        && server.acceptance.on_change()
                        && store
                            .login
                            .as_ref()
                            .and_then(|l| server.accounts.get(l))
                            .is_some_and(|a| a.accepted < *version),
                })
        });
        now(result)
    }

    fn accept_rules(&self, version: u32) -> Call<'_, ()> {
        if let Err(p) = self.reach() {
            return self.reply("accept_rules", Err(p));
        }
        let result = self.with_server_mut(|server, store| {
            let current = server.rules.as_ref().map_or(0, |(_, v)| *v);
            if version != current {
                return Err(Problem::RulesNotAccepted(
                    codes::RULES_CHANGED,
                    "The rules changed again while you were reading. Read the new version.".into(),
                ));
            }
            let login = store.login.clone().unwrap_or_default();
            if let Some(a) = server.accounts.get_mut(&login) {
                a.accepted = version;
            }
            Ok(())
        });
        self.reply("accept_rules", result)
    }

    fn create_persona<'a>(&'a self, handle: &'a str, display_name: &'a str) -> Call<'a, Persona> {
        if let Err(p) = self.reach() {
            return now(Err(p));
        }
        let handle = handle.trim().to_ascii_lowercase();
        let result = self.with_server_mut(|server, store| {
            if server.personas.iter().any(|(_, p, _)| p.handle == handle) {
                return Err(Problem::Taken(codes::HANDLE_TAKEN));
            }
            let mut p = persona(&handle);
            p.display_name = Some(display_name.trim().to_owned()).filter(|d| !d.is_empty());
            server
                .personas
                .push((store.login.clone().unwrap_or_default(), p.clone(), false));
            Ok(p)
        });
        now(result)
    }

    fn update_persona<'a>(
        &'a self,
        id: Uuid,
        display_name: &'a str,
        summary: &'a str,
    ) -> Call<'a, Persona> {
        let result = self.with_server_mut(|server, _| {
            let (_, p, _) = server
                .personas
                .iter_mut()
                .find(|(_, p, _)| p.id == id)
                .ok_or_else(|| refused(codes::PERSONA_GONE, "That persona is gone."))?;
            p.display_name = Some(display_name.trim().to_owned()).filter(|d| !d.is_empty());
            p.summary = Some(summary.trim().to_owned()).filter(|d| !d.is_empty());
            Ok(p.clone())
        });
        self.reply("update_persona", result)
    }

    fn delete_persona(&self, id: Uuid) -> Call<'_, ()> {
        self.with_server_mut(|server, _| server.personas.retain(|(_, p, _)| p.id != id));
        self.reply("delete_persona", Ok(()))
    }

    fn prepare<'a>(&'a self, persona: Uuid, progress: &'a dyn Fn(Step)) -> Call<'a, ()> {
        Box::pin(async move {
            for step in Step::ALL {
                progress(step);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            self.reach()?;
            self.with_server_mut(|server, store| {
                let (_, _, published) = server
                    .personas
                    .iter_mut()
                    .find(|(_, p, _)| p.id == persona)
                    .ok_or_else(|| refused(codes::PERSONA_GONE, "That persona is gone."))?;
                if *published && !store.keys.contains(&persona) {
                    return Err(refused(
                        codes::KEYS_ELSEWHERE,
                        "This persona's keys are on another of your devices. Link this computer to it, or restore a backup.",
                    ));
                }
                *published = true;
                store.keys.push(persona);
                Ok(())
            })
        })
    }

    fn start_over<'a>(&'a self, persona: Uuid, progress: &'a dyn Fn(Step)) -> Call<'a, ()> {
        Box::pin(async move {
            for step in Step::ALL {
                progress(step);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            self.sim_start_over(persona)
        })
    }

    fn device_name(&self) -> Option<String> {
        self.sim_device_name()
    }

    fn removed_here(&self) -> bool {
        self.open_store().is_ok_and(|(_, s, _)| s.here.removed)
    }

    fn suggested_device_name(&self) -> String {
        "writing-laptop".into()
    }

    fn rename_device<'a>(&'a self, name: &'a str) -> Call<'a, ()> {
        now(self.sim_rename_device(name))
    }

    fn devices(&self) -> Call<'_, Vec<crate::backend::Device>> {
        now(self.sim_devices())
    }

    fn storage(&self) -> Call<'_, plait_proto::sessions::AccountStorage> {
        now(self.sim_storage())
    }

    fn remove_device(&self, id: Uuid, reason: crate::backend::RemoveReason) -> Call<'_, ()> {
        now(self.sim_remove_device(id, reason))
    }

    fn start_link<'a>(&'a self, name: &'a str) -> Call<'a, crate::backend::LinkStart> {
        now(self.sim_start_link(name))
    }

    fn link_answer(&self) -> Call<'_, Option<Vec<String>>> {
        // As the native one, a look waits a little before saying "not yet".
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            self.sim_link_answer()
        })
    }

    fn finish_link(&self) -> Call<'_, Standing> {
        self.reply(
            "finish_link",
            self.sim_finish_link().and_then(|()| self.standing_now()),
        )
    }

    fn abandon_link(&self) -> Result<(), Problem> {
        self.sim_abandon_link()
    }

    fn approve_link<'a>(&'a self, typed: &'a str) -> Call<'a, crate::backend::Approval> {
        now(self.sim_approve_link(typed))
    }

    fn waiting_links(&self) -> Call<'_, usize> {
        now(self.sim_waiting_links())
    }

    fn key_details(&self) -> Call<'_, Vec<crate::backend::PersonaKeys>> {
        now(self.sim_key_details())
    }

    fn upkeep(&self) -> Call<'_, Option<Problem>> {
        // A removal by another device is learnt here, as plait-core's
        // catch-up learns it: the keys go then.
        // Notes written offline go too, as plait-core's `flush_all` sends them.
        now(self.reach().map(|_| {
            let removed = self.sim_catch_up();
            if !removed {
                self.sim_flush_all();
            }
            removed.then(|| {
                refused(
                    codes::REMOVED_HERE,
                    "This computer was removed from your devices. Link it again to keep writing here.",
                )
            })
        }))
    }

    fn next_notice(&self) -> Call<'_, ()> {
        // As the server's event stream: it can't open to a server that's
        // down, and it breaks when the server goes down.
        let live = self.0.borrow().live.clone();
        Box::pin(async move {
            self.reach()?;
            let notified = live.notified();
            tokio::pin!(notified);
            loop {
                tokio::select! {
                    () = &mut notified => return Ok(()),
                    () = tokio::time::sleep(Duration::from_millis(50)) => {
                        self.reach()?;
                    }
                }
            }
        })
    }

    fn send_request<'a>(
        &'a self,
        persona: Uuid,
        prompt: Uuid,
        pitch: &'a str,
        days: u32,
    ) -> Call<'a, crate::backend::SentRequest> {
        now(self.sim_send_request(persona, prompt, pitch, days))
    }

    fn sent_requests(&self) -> Call<'_, Vec<crate::backend::SentRequest>> {
        let held = self.0.borrow().requests_held.clone();
        Box::pin(async move {
            if let Some(n) = held {
                n.notified().await;
            }
            now(self.sim_sent_requests()).await
        })
    }

    fn withdraw_request(&self, id: Uuid) -> Call<'_, ()> {
        now(self.sim_withdraw(id))
    }

    fn waiting_requests(&self) -> Call<'_, Vec<crate::backend::WaitingPrompt>> {
        let held = self.0.borrow().requests_held.clone();
        Box::pin(async move {
            if let Some(n) = held {
                n.notified().await;
            }
            now(self.sim_waiting()).await
        })
    }

    fn inbox(&self, prompt: Uuid, held: bool) -> Call<'_, crate::backend::Inbox> {
        now(self.sim_inbox(prompt, held))
    }

    fn accept_request(&self, id: Uuid) -> Call<'_, ()> {
        now(self.sim_answer(id, true))
    }

    fn decline_request(&self, id: Uuid) -> Call<'_, ()> {
        now(self.sim_answer(id, false))
    }

    fn key_change<'a>(
        &'a self,
        handle: &'a str,
        as_persona: Uuid,
    ) -> Call<'a, Option<crate::backend::KeyChange>> {
        now(self.sim_key_change(handle, as_persona))
    }

    fn identity_words<'a>(
        &'a self,
        handle: &'a str,
        as_persona: Uuid,
    ) -> Call<'a, Option<Vec<String>>> {
        now(self.sim_identity_words(handle, as_persona))
    }

    fn accept_key_change<'a>(&'a self, handle: &'a str, fingerprint: &'a str) -> Call<'a, ()> {
        now(self.sim_accept_key_change(handle, fingerprint))
    }

    fn sessions(&self) -> Call<'_, Vec<crate::backend::SessionSummary>> {
        let held = self.0.borrow().sessions_held.clone();
        Box::pin(async move {
            if let Some(n) = held {
                n.notified().await;
            }
            now(self.sim_sessions()).await
        })
    }

    fn session<'a>(&'a self, id: &'a str) -> Call<'a, crate::backend::SessionView> {
        now(self.sim_session(id))
    }

    fn mark_shown<'a>(&'a self, id: &'a str, channel: &'a str, upto: u64) -> Result<(), Problem> {
        self.sim_mark_shown(id, channel, upto)
    }

    fn mark_opened(&self, id: &str) -> Result<(), Problem> {
        self.sim_mark_opened(id)
    }

    fn draft(&self, id: &str, channel: &str) -> Result<crate::backend::Draft, Problem> {
        self.sim_draft(id, channel)
    }

    fn drafted(&self, id: &str) -> Result<Vec<String>, Problem> {
        self.sim_drafted(id)
    }

    fn waiting(&self, id: &str) -> Result<Vec<crate::backend::QueuedNote>, Problem> {
        self.sim_waiting_notes(id)
    }

    fn save_draft(
        &self,
        id: &str,
        channel: &str,
        d: &crate::backend::Draft,
    ) -> Result<(), Problem> {
        self.sim_save_draft(id, channel, d)
    }

    fn send_note<'a>(
        &'a self,
        id: &'a str,
        channel: &'a str,
        text: &'a str,
        reply_to: Option<u64>,
        files: &'a [Uuid],
    ) -> Call<'a, crate::backend::Sending> {
        let ids = files.to_vec();
        let files: Vec<(String, Vec<u8>)> = {
            let st = self.0.borrow();
            files
                .iter()
                .filter_map(|f| st.attached.get(f).cloned())
                .collect()
        };
        // While a test holds the reply, the first file is still going up.
        if let Some((name, _)) = files.first() {
            self.0.borrow_mut().going = Some(crate::backend::FilesGoing::Uploading {
                n: 1,
                of: files.len(),
                name: name.clone(),
            });
        }
        let result = self.sim_send_note(id, channel, text, reply_to, files);
        // In the outbox now, so off the composer, as native does.
        if result.is_ok() {
            let mut st = self.0.borrow_mut();
            for id in &ids {
                st.attached.remove(id);
            }
        }
        let held = self.0.borrow().replies_held.get("send_note").cloned();
        Box::pin(async move {
            if let Some(n) = held {
                n.notified().await;
            }
            self.0.borrow_mut().going = None;
            result
        })
    }

    fn attachment_rules(&self) -> Call<'_, crate::backend::AttachmentRules> {
        now(self
            .reach()
            .map(|(_, _, server)| crate::backend::AttachmentRules {
                allowed: server.attachments,
                max_bytes: server.max_attachment_bytes,
            }))
    }

    fn choose_files(&self) -> Call<'_, Vec<String>> {
        now(Ok(self.0.borrow().files_pick.clone()))
    }

    fn attach<'a>(&'a self, path: &'a str) -> Call<'a, crate::backend::Attached> {
        now(self.sim_attach(path))
    }

    fn detach(&self, id: Uuid) {
        self.0.borrow_mut().attached.remove(&id);
    }

    fn files_going(&self) -> Option<crate::backend::FilesGoing> {
        self.0.borrow().going.clone()
    }

    fn send_waiting<'a>(&'a self, id: &'a str) -> Call<'a, crate::backend::Sending> {
        now(self.sim_flush(id, None))
    }

    fn change_waiting(&self, id: &str, note: Uuid, text: &str) -> Result<(), Problem> {
        self.sim_change_waiting(id, note, text)
    }

    fn withdraw_waiting(&self, id: &str, note: Uuid) -> Result<(), Problem> {
        self.sim_withdraw_waiting(id, note)
    }

    fn note_versions<'a>(
        &'a self,
        id: &'a str,
        seq: u64,
    ) -> Call<'a, Vec<crate::backend::NoteVersion>> {
        now(self.sim_note_versions(id, seq))
    }

    fn edit_note<'a>(&'a self, id: &'a str, seq: u64, text: &'a str) -> Call<'a, ()> {
        now(self.sim_change_note(id, seq, Change::Edit(text)))
    }

    fn delete_note<'a>(&'a self, id: &'a str, seq: u64) -> Call<'a, ()> {
        now(self.sim_change_note(id, seq, Change::Delete))
    }

    fn restore_note<'a>(&'a self, id: &'a str, seq: u64) -> Call<'a, ()> {
        now(self.sim_change_note(id, seq, Change::Restore))
    }

    fn file_src(&self, file: &crate::backend::NoteFile) -> String {
        format!("sim-file:{}", file.key)
    }

    fn choose_folder(&self) -> Call<'_, Option<String>> {
        now(Ok(self.0.borrow().folder_pick.clone()))
    }

    fn save_files<'a>(
        &'a self,
        files: &'a [crate::backend::NoteFile],
        folder: &'a str,
    ) -> Call<'a, Vec<String>> {
        now(self.sim_save_files(files, folder))
    }

    fn name_channel<'a>(
        &'a self,
        id: &'a str,
        name: &'a str,
        about: Option<&'a str>,
    ) -> Call<'a, ()> {
        now(self.sim_name_channel(id, name, about))
    }

    fn leave_session<'a>(&'a self, id: &'a str) -> Call<'a, ()> {
        now(self.sim_leave(id))
    }

    fn safety_numbers<'a>(&'a self, id: &'a str) -> Call<'a, crate::backend::SafetyNumbers> {
        now(self.sim_safety_numbers(id, false))
    }

    fn confirm_verified<'a>(&'a self, id: &'a str) -> Call<'a, crate::backend::SafetyNumbers> {
        now(self.sim_safety_numbers(id, true))
    }

    fn answers_seen(&self) -> Vec<Uuid> {
        self.open_store()
            .map(|(_, s, _)| s.requests.answers_seen)
            .unwrap_or_default()
    }

    fn set_answers_seen(&self, ids: &[Uuid]) -> Result<(), Problem> {
        self.with_server_mut(|_, store| store.requests.answers_seen = ids.to_vec());
        Ok(())
    }

    fn board_settings(&self) -> Call<'_, BoardSettings> {
        now(self.signed_in().map(|(login, server)| BoardSettings {
            maturity_max: server.account_of(&login).maturity_max,
        }))
    }

    fn set_board_settings(&self, settings: BoardSettings) -> Call<'_, BoardSettings> {
        self.with_server_mut(|server, store| {
            if let Some(a) = store
                .login
                .as_ref()
                .and_then(|l| server.accounts.get_mut(l))
            {
                a.maturity = settings.maturity_max;
            }
        });
        now(Ok(settings))
    }

    fn account(&self) -> Call<'_, AccountSettings> {
        now(self
            .signed_in()
            .map(|(login, server)| server.account_of(&login)))
    }

    fn prompts(&self, query: PromptQuery) -> Call<'_, PromptPage> {
        now(self.reach().and_then(|(_, store, server)| {
            server.search(
                store.signed_in.then_some(store.login.as_deref()).flatten(),
                &query,
            )
        }))
    }

    fn prompt(&self, id: Uuid) -> Call<'_, Prompt> {
        now(self.reach().and_then(|(_, store, server)| {
            server.one(
                store.signed_in.then_some(store.login.as_deref()).flatten(),
                id,
            )
        }))
    }

    fn create_prompt(&self, body: CreatePrompt) -> Call<'_, Prompt> {
        now(self.writing(|server, login| server.post(login, &body)))
    }

    fn update_prompt(&self, id: Uuid, body: UpdatePrompt) -> Call<'_, Prompt> {
        now(self.writing(|server, login| server.edit(login, id, &body)))
    }

    fn delete_prompt(&self, id: Uuid) -> Call<'_, ()> {
        now(self.writing(|server, login| server.delete(login, id)))
    }

    fn prompt_link(&self, id: Uuid) -> Option<String> {
        let (_, store, _) = self.open_store().ok()?;
        Some(format!("{}/p/{id}", store.address))
    }

    fn tag_types(&self) -> Call<'_, Vec<TagNamespace>> {
        now(self.reach().map(|(_, _, server)| server.tag_types()))
    }

    fn tag_policy(&self) -> Call<'_, TagPolicy> {
        now(self.reach().map(|(_, _, server)| server.board.policy))
    }

    fn tag_page<'a>(&'a self, namespace: &'a str, query: TagPageQuery) -> Call<'a, TagPage> {
        now(self
            .reach()
            .and_then(|(_, _, server)| server.tag_page(namespace, &query)))
    }

    fn find_tags(&self, query: TagQuery) -> Call<'_, Vec<Tag>> {
        now(self.reach().map(|(_, _, server)| server.find(&query)))
    }

    fn frequent_tags(&self, persona: Option<Uuid>) -> Call<'_, Vec<Tag>> {
        now(self
            .signed_in()
            .map(|(login, server)| server.frequent(&login, persona)))
    }

    fn report(&self, body: CreateReport) -> Call<'_, ()> {
        if let Err(p) = self.reach() {
            return now(Err(p));
        }
        now(self.with_server_mut(|server, store| {
            let login = store.signed_in.then(|| store.login.clone()).flatten();
            server.file_report(login.as_deref(), body)
        }))
    }

    fn notices(&self, before: Option<i64>) -> Call<'_, Vec<AccountNotice>> {
        now(self
            .signed_in()
            .map(|(login, server)| server.notices_of(&login, before)))
    }

    fn mark_notices_read(&self) -> Call<'_, ()> {
        self.reply(
            "mark_notices_read",
            self.writing(|server, login| {
                server.read_notices(login);
                Ok(())
            }),
        )
    }

    fn app_settings(&self) -> AppSettings {
        self.0.borrow().settings.clone()
    }

    fn set_app_settings(&self, settings: AppSettings) -> Result<(), Problem> {
        self.0.borrow_mut().settings = settings;
        Ok(())
    }

    fn move_stores<'a>(&'a self, folder: &'a str) -> Call<'a, ()> {
        self.0.borrow_mut().settings.stores_folder = folder.trim().into();
        self.reply("move_stores", Ok(()))
    }

    fn details(&self) -> Call<'_, Details> {
        let rows = match self.open_store() {
            Ok((id, store, _)) => vec![
                ("Server id (this app's)".to_owned(), id.to_string()),
                ("Store".to_owned(), format!("/stores/{id}.plait")),
                (
                    "Store key".to_owned(),
                    if store.passphrase.is_some() {
                        "passphrase"
                    } else {
                        "system keychain"
                    }
                    .into(),
                ),
            ],
            Err(_) => Vec::new(),
        };
        now(Ok(Details { rows }))
    }

    fn sleep(&self, ms: u64) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        if ms >= 60_000 {
            let clock = self.0.borrow().clock.clone();
            return Box::pin(async move { clock.notified().await });
        }
        Box::pin(tokio::time::sleep(Duration::from_millis(ms)))
    }
}

impl Sim {
    /// The open server and its signed-in login, or why there's none.
    fn signed_in(&self) -> Result<(String, SimServer), Problem> {
        let (_, store, server) = self.reach()?;
        match store.login {
            Some(login) if store.signed_in => Ok((login, server)),
            _ => Err(Problem::SignedOut),
        }
    }

    /// Runs a change on the open server as its signed-in login.
    fn writing<T>(
        &self,
        f: impl FnOnce(&mut SimServer, &str) -> Result<T, Problem>,
    ) -> Result<T, Problem> {
        self.signed_in()?;
        self.with_server_mut(|server, store| {
            let login = store.login.clone().unwrap_or_default();
            f(server, &login)
        })
    }

    /// The app closed: stores with a passphrase are locked again.
    pub fn relock(&self) {
        for store in self.0.borrow_mut().stores.values_mut() {
            if store.passphrase.is_some() {
                store.unlocked = false;
            }
        }
    }

    fn set_entry_login(&self, login: Option<String>) {
        let mut s = self.0.borrow_mut();
        if let Some(id) = s.current
            && let Some(e) = s.entries.iter_mut().find(|e| e.id == id)
        {
            e.login = login;
        }
    }
}
