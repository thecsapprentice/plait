//! `plait-seed-sessions`: fills a seeded dev server with requests and
//! sessions, for trying the desktop app by hand (#100).
//!
//! Requests and sessions are end-to-end encrypted, so this can't write them
//! into the database as `plait-seed` does. It signs in as the demo writers,
//! makes their keys, and acts them out against the running server. Each
//! app window's writer gets a store made by the app's own backend, so the
//! window opens signed in. See docs/plan.md, "Seeding requests and
//! sessions (#100)".
//!
//! A development tool: `scripts/dev-gui.py` runs it after `plait-seed`.

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use plait_core::{
    Client, Store, history, keys, requests, sessions,
    store::{
        Unlock,
        vault::{OsKeychain, Vault},
    },
};
use plait_gui::{NativeBackend, settings::Paths};
use plait_proto::{
    api::{Credentials, Persona},
    board::{CreatePrompt, Maturity},
};
use plait_ui::backend::{Backend, Problem, Standing};
use uuid::Uuid;

/// Acts out requests and sessions among the demo writers that `plait-seed
/// --cast` listed, and signs each app window in as one of them.
#[derive(Debug, Parser)]
#[command(name = "plait-seed-sessions", version)]
struct Args {
    /// The server, as the app would type it.
    #[arg(long, default_value = "localhost:8090")]
    server: String,
    /// The demo accounts' shared password.
    #[arg(
        long,
        env = "PLAIT_DEMO_PASSWORD",
        default_value = "plait demo password"
    )]
    password: String,
    /// A file of `PLAIT_CAST login handle…` lines from `plait-seed --cast`.
    #[arg(long)]
    cast: PathBuf,
    /// An app window's folder (PLAIT_GUI_DIR); repeat for each window. Each
    /// is signed in as the next demo writer.
    #[arg(long = "window", required = true)]
    windows: Vec<PathBuf>,
    /// Locks the windows' stores where there's no keychain to keep their
    /// keys in.
    #[arg(long, default_value = "plait demo passphrase")]
    passphrase: String,
}

/// A demo writer: signed in, with keys for the persona it writes as.
struct Writer {
    login: String,
    client: Client,
    store: Store,
    persona: Persona,
    prompts: Vec<Uuid>,
}

impl Writer {
    fn handle(&self) -> &str {
        &self.persona.handle
    }
}

#[tokio::main]
async fn main() {
    if let Err(err) = run(Args::parse()).await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run(args: Args) -> Result<()> {
    let url = server_url(&args.server);
    // The cast, less the admin: windows take the first writers, in order.
    let cast: Vec<(String, Vec<String>)> = std::fs::read_to_string(&args.cast)
        .with_context(|| format!("reading {}", args.cast.display()))?
        .lines()
        .filter_map(|l| l.strip_prefix("PLAIT_CAST "))
        .map(|l| {
            let mut words = l.split_whitespace().map(str::to_owned);
            (words.next().unwrap_or_default(), words.collect())
        })
        .filter(|(login, _)| login != "admin")
        .collect();
    let others = cast.len().saturating_sub(args.windows.len());
    if others < 3 {
        bail!(
            "{} demo writers for {} window(s): seed at least {} accounts",
            cast.len(),
            args.windows.len(),
            args.windows.len() + 4
        );
    }

    let vault = OsKeychain::new()
        .ok()
        .map(|k| Arc::new(k) as Arc<dyn Vault>);
    let mut featured = Vec::new();
    for ((login, _), dir) in cast.iter().zip(&args.windows) {
        let w = window_writer(&args, &url, login, dir, vault.as_ref()).await?;
        println!(
            "    window {}: {login} as @{}",
            featured.len() + 1,
            w.handle()
        );
        featured.push(w);
    }
    let mut partners = Vec::new();
    for (login, _) in &cast[args.windows.len()..] {
        partners.push(demo_writer(&url, login, &args.password).await?);
    }
    let mut titles = PROMPTS.iter().cycle();
    for w in featured.iter_mut().chain(partners.iter_mut()) {
        for (title, body) in titles.by_ref().take(2) {
            w.prompts.push(post(w, title, body).await?);
        }
    }

    for w in &featured {
        act_out(w, &partners).await?;
    }
    if let [a, b] = &featured[..] {
        between_windows(a, b).await?;
    }
    // Each window's device catches up, then reads some of it.
    for w in &featured {
        sessions::sync(&w.store, &w.client).await?;
        read_some(w).await?;
    }
    if vault.is_none() {
        println!("    The windows' stores unlock with: {}", args.passphrase);
    }
    Ok(())
}

/// The address the app would use: `localhost:8090` is plain HTTP.
fn server_url(typed: &str) -> String {
    if typed.contains("://") {
        typed.to_owned()
    } else {
        format!("http://{typed}")
    }
}

/// Signs a window's folder in as `login`, through the app's own backend,
/// with every persona's keys ready. Then opens the store it made.
async fn window_writer(
    args: &Args,
    url: &str,
    login: &str,
    dir: &std::path::Path,
    vault: Option<&Arc<dyn Vault>>,
) -> Result<Writer> {
    let problem = |p: Problem| anyhow!("{p:?}");
    let paths = Paths::in_dir(dir);
    let stores = paths.default_stores.clone();
    let backend = NativeBackend::new(paths, vault.cloned(), tokio::runtime::Handle::current())?;
    let passphrase = vault.is_none().then_some(args.passphrase.as_str());
    match backend.add_server(&args.server, passphrase).await {
        Err(Problem::NeedsPassphrase) => backend
            .add_server(&args.server, Some(&args.passphrase))
            .await
            .map_err(problem)?,
        r => r.map_err(problem)?,
    };
    let Standing::SignedIn { personas, .. } = backend
        .sign_in(login, &args.password)
        .await
        .map_err(problem)?
    else {
        bail!("{login} didn't sign in");
    };
    for p in &personas {
        backend
            .prepare(p.persona.id, &|_| {})
            .await
            .map_err(problem)?;
    }
    // The store is locked while the backend has it open.
    drop(backend);

    let path = std::fs::read_dir(&stores)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "plait"))
        .with_context(|| format!("no store in {}", stores.display()))?;
    let store = match vault {
        Some(v) => Store::open(&path, Unlock::Keychain(v.as_ref())),
        None => Store::open(&path, Unlock::Passphrase(&args.passphrase)),
    }?;
    let token = store.token()?.context("the store has no token")?;
    let client = Client::new(url)?.with_token(token.as_str());
    let persona = client
        .personas()
        .await?
        .into_iter()
        .next()
        .context("no persona")?;
    Ok(Writer {
        login: login.to_owned(),
        client,
        store,
        persona,
        prompts: Vec::new(),
    })
}

/// Another demo writer, on a device that lives only while this runs.
async fn demo_writer(url: &str, login: &str, password: &str) -> Result<Writer> {
    let client = Client::new(url)?;
    let token = client
        .login(&Credentials {
            username: login.to_owned(),
            password: password.to_owned(),
        })
        .await
        .with_context(|| format!("signing in as {login}"))?
        .token;
    let client = client.with_token(token);
    let persona = client
        .personas()
        .await?
        .into_iter()
        .next()
        .context("no persona")?;
    let store = Store::in_memory();
    keys::init(&store, &client, &persona, false).await?;
    keys::publish(&store, &client, &persona, 20).await?;
    Ok(Writer {
        login: login.to_owned(),
        client,
        store,
        persona,
        prompts: Vec::new(),
    })
}

/// Prompts the writers post for requests to land on, two each in turn.
const PROMPTS: &[(&str, &str)] = &[
    (
        "The lantern keeper",
        "Two keepers share one lighthouse and never meet. They write to each other in the log.",
    ),
    (
        "Letters from the drowned town",
        "A reservoir is drained after fifty years, and the post office is still standing.",
    ),
    (
        "The night ferry",
        "The last ferry of the night takes one more passenger than it has seats for.",
    ),
    (
        "A map with one road missing",
        "An old surveyor's map leaves out a road that everyone in the valley still uses.",
    ),
    (
        "The orchard at the end of the line",
        "The railway closed in 1962. Someone still picks the apples at the last stop.",
    ),
    (
        "Borrowed weather",
        "A village rents its rain from the next valley over, and the bill has come due.",
    ),
    (
        "The understudy",
        "The lead actor vanished on opening night. The understudy knows exactly where.",
    ),
    (
        "Salt and lantern oil",
        "Two smugglers, one cargo, and a harbourmaster who writes everything down.",
    ),
    (
        "The last bookbinder",
        "A bookbinder is asked to repair a book that hasn't been written yet.",
    ),
    (
        "Thirteen bells",
        "The clock tower strikes thirteen once a year, and only children hear it.",
    ),
    (
        "The cartographer's daughter",
        "She inherits her mother's maps and finds herself drawn in the margins.",
    ),
    (
        "Snow on the island",
        "It has never snowed on the island. This winter it does, and the ferry stops.",
    ),
];
async fn post(w: &Writer, title: &str, body: &str) -> Result<Uuid> {
    let prompt = w
        .client
        .create_prompt(&CreatePrompt {
            persona: w.persona.id,
            title: title.into(),
            body: body.into(),
            maturity: Maturity::General,
            tags: vec![
                "genre:mystery".into(),
                "format:prose".into(),
                "length:short".into(),
            ],
            state: None,
        })
        .await
        .with_context(|| format!("{} posting {title:?}", w.login))?;
    Ok(prompt.id)
}

/// `from` asks to write on `prompt`; returns the request's id.
async fn ask(from: &Writer, prompt: Uuid, pitch: &str) -> Result<Uuid> {
    let sent = requests::send(
        &from.store,
        &from.client,
        &from.persona,
        prompt,
        pitch,
        None,
    )
    .await
    .with_context(|| format!("{} asking", from.login))?;
    Ok(sent.request.id)
}

/// `author` reads the requests on `prompt`, then accepts `id`. Returns the
/// session, which the requester then joins.
async fn accept(author: &Writer, prompt: Uuid, id: Uuid, requester: &Writer) -> Result<String> {
    requests::inbox(&author.store, &author.client, prompt, false).await?;
    let session = requests::accept(&author.store, &author.client, id).await?;
    sessions::sync(&requester.store, &requester.client).await?;
    Ok(session)
}

async fn decline(author: &Writer, prompt: Uuid, id: Uuid) -> Result<()> {
    requests::inbox(&author.store, &author.client, prompt, false).await?;
    requests::decline(&author.store, &author.client, id).await?;
    Ok(())
}

async fn say(w: &Writer, session: &str, channel: &str, text: &str) -> Result<u64> {
    sessions::write(&w.store, &w.client, session, text, Some(channel))
        .await
        .with_context(|| format!("{} writing in {channel}", w.login))
}

/// One window's writer and the others: requests every way they can end,
/// and sessions in several states.
async fn act_out(me: &Writer, others: &[Writer]) -> Result<()> {
    let [a, b, c, rest @ ..] = others else {
        bail!("needs three other writers");
    };
    let (mine_a, mine_b) = (me.prompts[0], me.prompts[1]);

    // Asked by me, accepted: a long session, partly read.
    let id = ask(
        me,
        a.prompts[0],
        "I'd write the keeper who **never sleeps**. Alternate nights?",
    )
    .await?;
    let long = accept(a, a.prompts[0], id, me).await?;
    long_session(me, a, &long).await?;

    // Asked by me, accepted: never opened here, with notes waiting.
    let id = ask(
        me,
        b.prompts[0],
        "I'd like to write the *other* keeper, through the log only.",
    )
    .await?;
    let fresh = accept(b, b.prompts[0], id, me).await?;
    say(b, &fresh, "planning", "Thanks for asking! Shall I start?").await?;
    say(
        b,
        &fresh,
        "story",
        "The log had been kept in the same hand for eleven years. Then, one Tuesday, it wasn't.",
    )
    .await?;

    // Asked by me: declined, waiting, and withdrawn.
    let id = ask(
        me,
        c.prompts[0],
        "A lighthouse ghost story, told by the lamp.",
    )
    .await?;
    decline(c, c.prompts[0], id).await?;
    ask(
        me,
        a.prompts[1],
        "I'd write the postmistress, reading letters nobody sent.",
    )
    .await?;
    let id = ask(me, b.prompts[1], "The town's dentist, who stayed.").await?;
    requests::withdraw(&me.store, &me.client, id).await?;

    // Asked of me, accepted: the other writer left, so it has ended.
    let id = ask(
        c,
        mine_a,
        "I'd write the keeper's daughter, who reads the log *after*.",
    )
    .await?;
    let ended = accept(me, mine_a, id, c).await?;
    say(
        me,
        &ended,
        "planning",
        "Glad you asked. I'll start with the storm.",
    )
    .await?;
    say(
        me,
        &ended,
        "story",
        "The storm came in sideways, as they do here.",
    )
    .await?;
    say(
        c,
        &ended,
        "story",
        "She found the log under the stairs, swollen with sea air.",
    )
    .await?;
    say(
        c,
        &ended,
        "planning",
        "I have to stop here, I'm afraid. Thank you for this.",
    )
    .await?;
    sessions::leave(&c.store, &c.client, &ended).await?;

    // Asked of me: declined, and several waiting.
    let id = ask(
        a,
        mine_a,
        "A keeper who counts ships, and the one that never arrives.",
    )
    .await?;
    decline(me, mine_a, id).await?;
    ask(
        a,
        mine_b,
        "The postmaster's last delivery, fifty years late. I'd love to write it with you.",
    )
    .await?;
    ask(
        b,
        mine_b,
        "I'd write the girl who swam down to the church roof every summer.",
    )
    .await?;
    ask(
        c,
        mine_b,
        "A *letter* in each chapter, and the town answers back.",
    )
    .await?;

    // Anyone else: one more session each, short and unread.
    for d in rest {
        let id = ask(me, d.prompts[0], "Could I write the lighthouse itself?").await?;
        let s = accept(d, d.prompts[0], id, me).await?;
        say(
            d,
            &s,
            "planning",
            "Yes! The lighthouse as narrator is perfect.",
        )
        .await?;
    }
    Ok(())
}

/// A long session: every channel, a named one, answers, an edit and a
/// deletion.
async fn long_session(me: &Writer, a: &Writer, s: &str) -> Result<()> {
    let plan = say(
        a,
        s,
        "planning",
        "Welcome! Shall we alternate scenes, a few hundred words each?",
    )
    .await?;
    say(
        me,
        s,
        "planning",
        "Yes. I'll take the odd scenes. Any rules for the narrator?",
    )
    .await?;
    sessions::reply(
        &a.store,
        &a.client,
        s,
        "Close third person, past tense. Nothing the keepers couldn't know.",
        Some("planning"),
        plan,
    )
    .await?;
    sessions::name_channel(
        &me.store,
        &me.client,
        s,
        "research",
        Some("Places, dates and things we've looked up."),
    )
    .await?;
    say(
        me,
        s,
        "research",
        "Keepers kept a **daily log**: weather, ships sighted, hours the lamp burned.",
    )
    .await?;
    say(
        a,
        s,
        "research",
        "Fog signals were tested weekly. That could be our clock.",
    )
    .await?;
    say(me, s, "story", "# One\n\nThe lamp was lit at dusk, as it had been for forty years. Ada wrote the time, the wind, and nothing else.").await?;
    say(
        a,
        s,
        "story",
        "# Two\n\nThe morning keeper found the entry and, under it, in pencil: *who are you?*",
    )
    .await?;
    let draft = say(me, s, "story", "# Three\n\nAda didn't answer for a week.").await?;
    sessions::edit(
        &me.store,
        &me.client,
        s,
        draft,
        "# Three\n\nAda didn't answer for a week. Then she wrote one word: *Ada.*",
    )
    .await?;
    let gone = say(a, s, "story", "(Scratch that, I'll rewrite it tonight.)").await?;
    sessions::delete(&a.store, &a.client, s, gone).await?;
    say(a, s, "story", "# Four\n\nThe morning keeper had never had a name in the log before. He took a long time choosing one.").await?;
    say(
        a,
        s,
        "research",
        "Found a real keeper's log from 1894. Mostly weather, one marriage proposal.",
    )
    .await?;
    Ok(())
}

/// With two windows: a session between them, and a request each can answer
/// live.
async fn between_windows(one: &Writer, two: &Writer) -> Result<()> {
    let id = ask(
        one,
        two.prompts[0],
        "We should write this one together. I'd take the night watch.",
    )
    .await?;
    let s = accept(two, two.prompts[0], id, one).await?;
    say(two, &s, "planning", "Yes! You take nights, I'll take days.").await?;
    say(
        one,
        &s,
        "story",
        "Night, and the lamp turning. Somewhere below, the log.",
    )
    .await?;
    ask(two, one.prompts[1], "Your turn to answer one of mine.").await?;
    Ok(())
}

/// The window's device has read some of what's there: the long session
/// up to its last two story notes, the ended one all through. The rest
/// stays new.
async fn read_some(w: &Writer) -> Result<()> {
    for l in sessions::list(&w.store, &w.client).await? {
        let (id, ended) = (l.session.id.clone(), l.session.ended);
        let gid = plait_proto::keys::unb64(&id).context("bad session id")?;
        let notes = history::messages(&w.store, &gid)?;
        let leave_unread = if ended {
            0
        } else if notes.len() > 8 {
            2
        } else {
            continue;
        };
        history::mark_opened(&w.store, &gid)?;
        for channel in ["story", "planning", "research"] {
            let seqs: Vec<u64> = notes
                .iter()
                .filter(|n| n.channel == channel)
                .map(|n| n.seq)
                .collect();
            let keep = if channel == "story" { leave_unread } else { 0 };
            if let Some(&upto) = seqs.len().checked_sub(keep + 1).and_then(|i| seqs.get(i)) {
                history::mark_shown(&w.store, &gid, channel, upto)?;
            }
        }
    }
    Ok(())
}
