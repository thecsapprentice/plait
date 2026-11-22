//! The desktop app worked as a person would (by labels, through the same
//! events the window sends), on the native backend against a real server.
//! `plait-ui`'s `tests/flows.rs` covers far more against a simulated
//! server; these check the simulation holds for the paths that matter.

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{rc::Rc, sync::Arc};

use plait_core::store::vault::{MemoryVault, Vault};
use plait_e2e::TestServer;
use plait_gui::{NativeBackend, settings::Paths};
use plait_proto::{
    api::CreatePersona,
    board::{
        CreatePrompt, CreateReport, Maturity, ReportReason, Resolution, UpdateNamespace,
        UpdateServerSettings,
    },
    rules::{RulesAcceptance, UpdateRules},
};
use plait_ui::testing::Driver;

const PASSWORD: &str = "correct horse battery";

fn app(dir: &std::path::Path, vault: &Arc<MemoryVault>) -> Driver {
    let vault: Arc<dyn Vault> = vault.clone();
    let backend = NativeBackend::new(
        Paths::in_dir(dir),
        Some(vault),
        tokio::runtime::Handle::current(),
    )
    .unwrap();
    Driver::new(Rc::new(backend))
}

/// The server's address as a writer types it.
fn typed(server: &TestServer) -> String {
    server.url().trim_start_matches("http://").to_owned()
}

async fn to_sign_up(d: &mut Driver, server: &TestServer) {
    d.type_into("server-address", &typed(server)).await;
    d.wait_for_text("Takes new logins").await;
    d.click("Continue").await;
    d.wait_for_text("Make a login on").await;
}

async fn set_rules(server: &TestServer, body: &str, acceptance: RulesAcceptance) {
    let admin = server.admin_account("boss").await.unwrap();
    admin
        .admin_update_rules(&UpdateRules {
            body: body.into(),
            minor: false,
        })
        .await
        .unwrap();
    admin
        .admin_update_settings(&UpdateServerSettings {
            rules_acceptance: Some(acceptance),
            ..UpdateServerSettings::default()
        })
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_run_against_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let mut d = app(dir.path(), &vault);
    to_sign_up(&mut d, &server).await;
    d.type_into("login-name", "ink").await;
    d.type_into("login-password", PASSWORD).await;
    d.click("Make my login").await;
    d.wait_for_text("Make your first persona").await;
    d.type_into("persona-handle", "ink_fox").await;
    d.click("Make this persona").await;
    d.wait_for_text("Hello, @ink_fox").await;
    d.wait_for_text("Connected to").await;

    // Opening again goes straight to the frame.
    drop(d);
    let mut d = app(dir.path(), &vault);
    d.wait_for_text("Hello, @ink_fox").await;
    server.stop().await.unwrap();
}

/// 3g-1: the Storage view reads `GET /account/storage` from a real server.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn storage_use_against_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let mut d = app(dir.path(), &vault);
    to_sign_up(&mut d, &server).await;
    d.type_into("login-name", "ink").await;
    d.type_into("login-password", PASSWORD).await;
    d.click("Make my login").await;
    d.wait_for_text("Make your first persona").await;
    d.type_into("persona-handle", "ink_fox").await;
    d.click("Make this persona").await;
    d.wait_for_text("Hello, @ink_fox").await;
    d.click("Settings").await;
    d.click("See storage use").await;
    d.wait_for_text("Attachments you uploaded").await;
    assert!(d.has_text("0 bytes of 500 MB"));
    assert!(d.has_text("No sessions yet, so nothing stored for them."));
    server.stop().await.unwrap();
}

/// #103: the app opened while this process still held the server's store
/// (work from a window just closed, still running) said "Something went
/// wrong." and stayed stuck. It says the store is busy, and opens it once
/// it's free.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opening_while_the_store_is_still_busy_waits_for_it() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let backend = |vault: &Arc<MemoryVault>| {
        let vault: Arc<dyn Vault> = vault.clone();
        Rc::new(
            NativeBackend::new(
                Paths::in_dir(dir.path()),
                Some(vault),
                tokio::runtime::Handle::current(),
            )
            .unwrap(),
        )
    };
    let first = backend(&vault);
    let mut d = Driver::new(first.clone());
    to_sign_up(&mut d, &server).await;
    d.type_into("login-name", "ink").await;
    d.type_into("login-password", PASSWORD).await;
    d.click("Make my login").await;
    d.wait_for_text("Make your first persona").await;
    d.type_into("persona-handle", "ink_fox").await;
    d.click("Make this persona").await;
    d.wait_for_text("Hello, @ink_fox").await;
    // The window closes, but `first` still holds the store, as work it
    // started would.
    drop(d);
    let mut d = Driver::new(backend(&vault));
    d.wait_for_text("Plait is still using this server's store.")
        .await;
    assert!(!d.has_text("Something went wrong."));
    drop(first);
    d.wait_for_text("Hello, @ink_fox").await;
    server.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rules_required_after_sign_up_opened_against_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let mut d = app(dir.path(), &vault);
    to_sign_up(&mut d, &server).await;
    // The administrator requires the rules while the screen is open.
    set_rules(&server, "# Be kind\n\nNo spam.", RulesAcceptance::OnJoin).await;
    d.type_into("login-name", "ink").await;
    d.type_into("login-password", PASSWORD).await;
    d.click("Make my login").await;
    d.wait_for_text("I've read and accept the server rules")
        .await;
    assert!(d.button_disabled("Make my login"));
    d.set_checked("accept-rules", true).await;
    d.click("Make my login").await;
    d.wait_for_text("Make your first persona").await;
    server.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rules_on_change_are_accepted_at_sign_up_against_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    set_rules(&server, "# Be kind", RulesAcceptance::OnChange).await;
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let mut d = app(dir.path(), &vault);
    to_sign_up(&mut d, &server).await;
    d.wait_for_text("I've read and accept the server rules")
        .await;
    d.type_into("login-name", "ink").await;
    d.type_into("login-password", PASSWORD).await;
    d.set_checked("accept-rules", true).await;
    d.click("Make my login").await;
    d.wait_for_text("Make your first persona").await;
    d.click("Not now").await;
    d.wait_for_text("Connected to").await;
    // The server recorded it: no "rules have changed" right after joining.
    d.settle().await;
    assert!(!d.has_text("The server rules have changed"));
    server.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_add_a_server_against_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let mut d = app(dir.path(), &vault);
    to_sign_up(&mut d, &server).await;
    d.type_into("login-name", "ink").await;
    d.type_into("login-password", PASSWORD).await;
    d.click("Make my login").await;
    d.wait_for_text("Make your first persona").await;
    d.click("Not now").await;
    d.wait_for_text("Connected to").await;
    d.choose("server-switch", "new").await;
    d.wait_for_text("Add a server").await;
    d.click("Cancel").await;
    d.wait_for_text("Connected to").await;
    server.stop().await.unwrap();
}

/// Signs up as `ink` with persona `@ink_fox` and opens the Board.
async fn on_the_board(d: &mut Driver, server: &TestServer) {
    to_sign_up(d, server).await;
    d.type_into("login-name", "ink").await;
    d.type_into("login-password", PASSWORD).await;
    d.click("Make my login").await;
    d.wait_for_text("Make your first persona").await;
    d.type_into("persona-handle", "ink_fox").await;
    d.click("Make this persona").await;
    d.wait_for_text("Hello, @ink_fox").await;
    d.click("Board").await;
    d.wait_until("the board loaded", |d| !d.has_text("Loading prompts"))
        .await;
}

/// Another writer's prompt, posted through the API.
async fn posted_elsewhere(server: &TestServer, login: &str, title: &str) -> uuid::Uuid {
    let other = server.account(login).await.unwrap();
    let persona = other
        .create_persona(&CreatePersona {
            handle: format!("{login}_pen"),
            display_name: None,
            summary: None,
        })
        .await
        .unwrap();
    other
        .create_prompt(&CreatePrompt {
            persona: persona.id,
            title: title.into(),
            body: "Somebody else's text.".into(),
            maturity: Maturity::General,
            tags: vec!["genre:horror".into()],
            state: None,
        })
        .await
        .unwrap()
        .id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn posting_searching_editing_and_deleting_on_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    posted_elsewhere(&server, "marrow", "The drowned bell").await;
    // From now on every prompt needs exactly one length tag.
    let admin = server.admin_account("boss").await.unwrap();
    admin
        .admin_update_namespace(
            "length",
            &UpdateNamespace {
                min: Some(1),
                max: Some(1),
                ..UpdateNamespace::default()
            },
        )
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let mut d = app(dir.path(), &vault);
    on_the_board(&mut d, &server).await;
    d.wait_for_text("The drowned bell").await;

    d.click("Post a prompt").await;
    d.wait_for_text("Add tags").await;
    d.type_into("post-title", "The night porter").await;
    d.type_into(
        "post-body",
        "A hotel with one more *floor* than the lift admits.",
    )
    .await;
    assert!(d.has_text("1 length tag before posting"));
    assert!(d.button_disabled("Post prompt"));
    d.wait_until("the length tags", |d| d.has_button("length: Short"))
        .await;
    d.click("length: Short").await;
    d.click("Post prompt").await;
    d.wait_for_text("Prompt posted.").await;

    // Search as you type finds it, and only it.
    d.type_into("board-search", "porter").await;
    d.wait_until("the search", |d| !d.has_text("The drowned bell"))
        .await;
    assert!(d.has_text("The night porter"));

    d.click("Edit").await;
    d.wait_for_text("Edit your prompt").await;
    d.type_into("post-title", "The night porter, again").await;
    d.click("Save changes").await;
    d.wait_for_text("Changes saved.").await;
    d.choose("prompt-state", "closed").await;
    d.wait_until("the status", |d| d.has_text("· Closed ·"))
        .await;

    d.click("Delete…").await;
    d.click("Delete this prompt").await;
    d.wait_for_text("Prompt deleted.").await;
    assert!(!d.has_text("The night porter, again"));
    server.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn filtering_by_tag_and_reporting_on_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    posted_elsewhere(&server, "marrow", "The drowned bell").await;
    let other = server.account("wren").await.unwrap();
    let wren = other
        .create_persona(&CreatePersona {
            handle: "wren_pen".into(),
            display_name: None,
            summary: None,
        })
        .await
        .unwrap();
    other
        .create_prompt(&CreatePrompt {
            persona: wren.id,
            title: "A quiet mystery".into(),
            body: "Who moved the chair?".into(),
            maturity: Maturity::General,
            tags: vec!["genre:mystery".into()],
            state: None,
        })
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let mut d = app(dir.path(), &vault);
    on_the_board(&mut d, &server).await;
    d.wait_for_text("A quiet mystery").await;

    d.click("Filters").await;
    d.click("Genre").await;
    d.wait_until("the genre tags", |d| d.has_field("hide-genre-horror"))
        .await;
    d.click_id("hide-genre-horror").await;
    d.click("Show prompts").await;
    d.wait_until("the filter", |d| !d.has_text("The drowned bell"))
        .await;
    assert!(d.has_text("A quiet mystery"));
    assert!(d.has_button("Filters (1)"));

    d.click_containing("A quiet mystery").await;
    d.click("Report this prompt").await;
    d.set_checked("reason-spam", true).await;
    d.click("Send report").await;
    d.wait_for_text("Thanks. An administrator will look at your report.")
        .await;
    let admin = server.admin_account("boss").await.unwrap();
    let reports = admin.admin_reports(false).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].reason, ReportReason::Spam);
    server.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removal_is_a_notice_that_leads_to_the_prompt_on_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let mut d = app(dir.path(), &vault);
    on_the_board(&mut d, &server).await;
    d.click("Post a prompt").await;
    d.wait_for_text("Add tags").await;
    d.type_into("post-title", "Buy my book").await;
    d.type_into("post-body", "It's very good.").await;
    d.click("Post prompt").await;
    d.wait_for_text("Prompt posted.").await;

    // Reported and removed by an administrator.
    let bob = server.account("bob").await.unwrap();
    let mine = bob
        .prompts(&plait_proto::board::PromptQuery::default())
        .await
        .unwrap()
        .items[0]
        .id;
    bob.report(&CreateReport {
        prompt: mine,
        reason: ReportReason::Spam,
        details: None,
    })
    .await
    .unwrap();
    let admin = server.admin_account("boss").await.unwrap();
    let report = admin.admin_reports(false).await.unwrap()[0].id;
    admin
        .resolve_report(report, Resolution::Delete)
        .await
        .unwrap();

    // Opening the app again counts it.
    drop(d);
    let mut d = app(dir.path(), &vault);
    d.wait_until("the count", |d| d.has_button("Notices 1 unread"))
        .await;
    d.click("Notices 1 unread").await;
    d.wait_for_text("An administrator removed your prompt")
        .await;
    d.wait_until("read", |d| d.has_button("Notices")).await;
    d.click("Buy my book").await;
    d.wait_for_text("It's very good.").await;
    assert!(d.has_text("· removed"));
    server.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_passphrase_chosen_at_first_run_unlocks_on_reopening_against_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let no_keychain = || {
        let backend = NativeBackend::new(
            Paths::in_dir(dir.path()),
            None,
            tokio::runtime::Handle::current(),
        )
        .unwrap();
        Driver::new(Rc::new(backend))
    };
    let mut d = no_keychain();
    d.type_into("server-address", &typed(&server)).await;
    d.wait_for_text("Takes new logins").await;
    d.click("Continue").await;
    d.wait_for_text("choose a passphrase").await;
    d.type_into("store-passphrase", "a long passphrase here")
        .await;
    d.click("Continue").await;
    d.wait_for_text("Make a login on").await;
    d.type_into("login-name", "ink").await;
    d.type_into("login-password", PASSWORD).await;
    d.click("Make my login").await;
    d.wait_for_text("Make your first persona").await;
    d.click("Not now").await;
    d.wait_for_text("Connected to").await;

    drop(d);
    let mut d = no_keychain();
    d.wait_for_text("Unlock").await;
    d.type_into("unlock-passphrase", "a long passphrase here")
        .await;
    d.click("Unlock").await;
    d.wait_for_text("Connected to").await;
    server.stop().await.unwrap();
}

/// The six words shown after `marker` in a window's text.
fn six_words_after(text: &str, marker: &str) -> String {
    let at = text
        .find(marker)
        .unwrap_or_else(|| panic!("no {marker:?} in {text}"));
    text[at + marker.len()..]
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_computer_is_linked_and_then_removed_on_a_real_server() {
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    // The laptop: a login and a ready persona.
    let (ld, lv) = (
        tempfile::tempdir().unwrap(),
        Arc::new(MemoryVault::default()),
    );
    let mut laptop = app(ld.path(), &lv);
    on_the_board(&mut laptop, &server).await;

    // The desktop signs in and links itself.
    let (dd, dv) = (
        tempfile::tempdir().unwrap(),
        Arc::new(MemoryVault::default()),
    );
    let mut desktop = app(dd.path(), &dv);
    to_sign_up(&mut desktop, &server).await;
    desktop.click("I have a login").await;
    desktop.type_into("login-name", "ink").await;
    desktop.type_into("login-password", PASSWORD).await;
    desktop.click("Sign in").await;
    desktop
        .wait_for_text("This computer is new to your login")
        .await;
    desktop.click("Link this computer").await;
    desktop.type_into("device-name", "Study desktop").await;
    desktop.click("Show the link code").await;
    desktop
        .wait_for_text("Type these words on your other device")
        .await;
    let words = six_words_after(&desktop.text(), "Type these six words there.");

    // The laptop approves it by those words.
    laptop.click("Settings").await;
    laptop.click("Your devices").await;
    laptop.wait_for_text("Only this computer so far.").await;
    laptop.click("Link a new computer").await;
    laptop
        .wait_until("the words field", |d| d.has_field("link-words"))
        .await;
    laptop.type_into("link-words", &words).await;
    laptop.click("Link it").await;
    laptop
        .wait_for_text("The new computer should now show these words:")
        .await;
    let confirm = six_words_after(
        &laptop.text(),
        "The new computer should now show these words:",
    );

    // The desktop shows the same words, and takes the keys.
    desktop.wait_for_text("Check the words match").await;
    let shown = six_words_after(&desktop.text(), "in the same order.");
    assert_eq!(shown, confirm, "both devices show the same words");
    desktop.click("They match").await;
    desktop.wait_for_text("Hello, @ink_fox").await;

    // The laptop sees it by name, and removes it.
    laptop.click("Done").await;
    laptop.wait_for_text("Study desktop").await;
    laptop.click("Remove…").await;
    laptop.click("Remove Study desktop").await;
    laptop.wait_for_text("Study desktop was removed.").await;

    // Reopened, the desktop learns it and says so.
    drop(desktop);
    let mut desktop = app(dd.path(), &dv);
    desktop
        .wait_for_text("This computer was removed from your devices")
        .await;
    server.stop().await.unwrap();
}

/// A writer with keys, posting a prompt, through `plait-core` (3e).
async fn writer_with_prompt(
    server: &TestServer,
    login: &str,
    title: &str,
) -> (
    plait_core::Client,
    plait_core::Store,
    plait_proto::api::Persona,
    uuid::Uuid,
) {
    let client = server.account(login).await.unwrap();
    let persona = client
        .create_persona(&CreatePersona {
            handle: format!("{login}_pen"),
            display_name: None,
            summary: None,
        })
        .await
        .unwrap();
    let store = plait_core::Store::in_memory();
    plait_core::keys::init(&store, &client, &persona, false)
        .await
        .unwrap();
    plait_core::keys::publish(&store, &client, &persona, 5)
        .await
        .unwrap();
    let prompt = client
        .create_prompt(&CreatePrompt {
            persona: persona.id,
            title: title.into(),
            body: "A keeper finds a ledger.".into(),
            maturity: Maturity::General,
            tags: vec!["genre:horror".into()],
            state: None,
        })
        .await
        .unwrap()
        .id;
    (client, store, persona, prompt)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn requests_both_ways_on_a_real_server() {
    use plait_core::requests;
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    let (wren_c, wren, wren_pen, theirs) =
        writer_with_prompt(&server, "wren", "The lighthouse ledger").await;
    let dir = tempfile::tempdir().unwrap();
    let vault = Arc::new(MemoryVault::default());
    let mut d = app(dir.path(), &vault);
    on_the_board(&mut d, &server).await;

    // Asking: only Wren's device reads the pitch, and her answer shows.
    d.wait_for_text("The lighthouse ledger").await;
    d.click_containing("The lighthouse ledger").await;
    d.wait_for_text("A keeper finds a ledger.").await;
    d.click("Ask to write together").await;
    d.type_into("request-pitch", "I'd write the *keeper*.")
        .await;
    d.click("Send request").await;
    d.wait_for_text("Request sent.").await;
    // The words the app shows are the ones Wren's side derives (#72).
    d.click("Compare words with @wren_pen").await;
    d.wait_for_text("sees these same 12 words").await;
    let theirs_words = plait_core::keys::identity_words(&wren, &wren_c, "ink_fox", &wren_pen)
        .await
        .unwrap()
        .unwrap();
    let text = d.text();
    assert!(
        text.contains(&theirs_words.join(" ")),
        "{theirs_words:?} in {text}"
    );
    let inbox = requests::inbox(&wren, &wren_c, theirs, false)
        .await
        .unwrap();
    assert_eq!(
        inbox.requests[0].pitch.as_deref(),
        Ok("I'd write the *keeper*.")
    );
    requests::accept(&wren, &wren_c, inbox.requests[0].id)
        .await
        .unwrap();
    d.click("Home").await;
    d.wait_for_text("Your request on “The lighthouse ledger” was accepted")
        .await;
    // Home's word of it leads to the session (3f).
    d.click("Open the session").await;
    d.wait_for_text("Writing with @wren_pen").await;
    assert!(d.has_text("On “The lighthouse ledger”."));
    d.click("Home").await;

    // It's a session now (3f): Wren writes in it, and it arrives live.
    let session = plait_core::sessions::list(&wren, &wren_c).await.unwrap()[0]
        .session
        .id
        .clone();
    plait_core::sessions::write(&wren, &wren_c, &session, "The lamp went out.", None)
        .await
        .unwrap();
    d.wait_for_text("1 new note in your session with @wren_pen on “The lighthouse ledger”.")
        .await;
    d.click("Read the session with @wren_pen on “The lighthouse ledger”")
        .await;
    d.wait_for_text("The lamp went out.").await;
    d.click("Planning").await;
    d.wait_for_text("I'd write the").await;

    // Writing (3f-2): a note, an answer, and a draft kept in the store.
    d.type_into("note-editor", "Shall we **alternate** nights?")
        .await;
    d.click("Send").await;
    d.wait_for_text("Sent.").await;
    d.click("Story").await;
    d.click("Answer @wren_pen's note").await;
    d.type_into("note-editor", "And the bell rang twice.").await;
    d.click("Send").await;
    d.wait_until("the answer goes", |d| {
        d.attr("note-editor", "value").as_deref() == Some("")
    })
    .await;
    d.type_into("note-editor", "Half a thought").await;
    d.click("Planning").await;
    d.click("Story ✎ draft").await;
    assert_eq!(
        d.attr("note-editor", "value").as_deref(),
        Some("Half a thought")
    );
    let read = plait_core::sessions::read(&wren, &wren_c, &session)
        .await
        .unwrap();
    let (asked, said, answer) = (&read.messages[1], &read.messages[2], &read.messages[3]);
    assert_eq!(
        (said.content.as_str(), said.channel.as_str()),
        ("Shall we **alternate** nights?", "planning")
    );
    assert_eq!(answer.content, "And the bell rang twice.");
    assert_eq!(answer.in_reply_to.as_deref(), Some(asked.id.as_str()));
    let answer_at = answer.seq;

    // Changing notes (3f-3): an edit undone, a delete undone (#104).
    d.click("Edit your note").await;
    d.type_into(&format!("edit-{answer_at}"), "And the bell rang thrice.")
        .await;
    d.click("Save the change").await;
    d.wait_for_text("Edited.").await;
    d.click("Undo").await;
    d.wait_for_text("Undone.").await;
    d.click("Edit your note").await;
    d.click("Delete this note").await;
    d.wait_for_text("It stays in its history for you both.")
        .await;
    d.click("Undo").await;
    d.wait_for_text("Undone.").await;
    let back = plait_core::sessions::versions(&wren, &wren_c, &session, answer_at)
        .await
        .unwrap();
    assert_eq!(
        (back.content.as_str(), back.deleted),
        ("And the bell rang twice.", false)
    );
    assert_eq!(
        back.versions
            .iter()
            .map(|v| v.content.is_some())
            .collect::<Vec<_>>(),
        [true, true, true, false, true],
        "written, edited, undone, deleted, restored"
    );

    // A channel named as typed, seen by Wren.
    d.click("New channel").await;
    d.type_into("channel-name", "World building").await;
    d.type_into("channel-about", "Places and weather.").await;
    d.click("Make the channel").await;
    d.wait_for_text("Nothing in World building yet.").await;
    let channels = plait_core::sessions::channels(&wren, &wren_c, &session)
        .await
        .unwrap();
    assert!(
        channels
            .iter()
            .any(|c| c.name == "world-building"
                && c.summary.as_deref() == Some("Places and weather.")),
        "{channels:?}"
    );

    // Safety numbers: the same words on both sides, and "They match".
    d.click("Compare safety numbers").await;
    d.wait_for_text("Your 12 words with @wren_pen").await;
    let hers = plait_core::sessions::safety_numbers(&wren, &wren_c, &session)
        .await
        .unwrap();
    let text = d.text();
    assert!(
        text.contains(&hers.identity.join(" ")),
        "{:?} in {text}",
        hers.identity
    );
    assert!(
        text.contains(&hers.check.join(" ")),
        "{:?} in {text}",
        hers.check
    );
    d.click("They match").await;
    d.wait_for_text("You marked @wren_pen verified on this computer.")
        .await;
    d.click("Hide the safety numbers").await;
    d.wait_for_text("Verified on this computer").await;
    d.click("Home").await;

    // Answering: Wren asks on Ink's prompt, live, and Ink accepts.
    d.click("Board").await;
    d.click("Post a prompt").await;
    d.wait_for_text("Add tags").await;
    d.type_into("post-title", "The night porter").await;
    d.type_into(
        "post-body",
        "A hotel with one more floor than the lift admits.",
    )
    .await;
    d.click("Post prompt").await;
    d.wait_for_text("Prompt posted.").await;
    let mine = wren_c
        .prompts(&plait_proto::board::PromptQuery {
            q: Some("night porter".into()),
            ..plait_proto::board::PromptQuery::default()
        })
        .await
        .unwrap()
        .items[0]
        .id;
    let sent = requests::send(&wren, &wren_c, &wren_pen, mine, "**Porters** know.", None)
        .await
        .unwrap();
    d.click("Home").await;
    d.wait_for_text("A request is waiting on “The night porter”.")
        .await;
    d.click("Read them").await;
    d.wait_for_text("Porters know.").await;
    d.click("Accept").await;
    d.wait_for_text("Accepted @wren_pen's request").await;
    assert_eq!(
        wren_c.request_status(sent.request.id).await.unwrap().status,
        plait_proto::requests::RequestStatus::Session
    );
    // Accepting leads to that session, not the other one with @wren_pen.
    d.click("Open the session").await;
    d.wait_for_text("On “The night porter”.").await;

    // Leaving it (3f-3) ends it for both, and it stays readable.
    d.click("Leave the session…").await;
    d.click("Leave it").await;
    d.wait_for_text("This session has ended.").await;
    let ended = plait_core::sessions::list(&wren, &wren_c)
        .await
        .unwrap()
        .into_iter()
        .filter(|l| l.session.ended)
        .count();
    assert_eq!(ended, 1, "the night porter's session, for Wren too");
    server.stop().await.unwrap();
}

/// 3g-2: a writer's files arrive in the app over a real server. The image
/// is served to the webview from the store, as its type, and nothing else
/// is; a file is saved to the folder chosen, byte for byte.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_notes_files_are_shown_and_saved_from_a_real_server() {
    use plait_core::{requests, sessions};
    let Some(server) = TestServer::start().await.unwrap() else {
        return;
    };
    let (wren_c, wren, _, theirs) =
        writer_with_prompt(&server, "wren", "The lighthouse ledger").await;
    let dir = tempfile::tempdir().unwrap();
    let saved = tempfile::tempdir().unwrap();
    let vault: Arc<dyn Vault> = Arc::new(MemoryVault::default());
    let backend = Rc::new(
        NativeBackend::new(
            Paths::in_dir(dir.path()),
            Some(vault),
            tokio::runtime::Handle::current(),
        )
        .unwrap()
        .answer_folder_dialog_with(saved.path().to_path_buf()),
    );
    let mut d = Driver::new(backend.clone());
    on_the_board(&mut d, &server).await;
    d.wait_for_text("The lighthouse ledger").await;
    d.click_containing("The lighthouse ledger").await;
    d.click("Ask to write together").await;
    d.type_into("request-pitch", "I'd write the keeper.").await;
    d.click("Send request").await;
    d.wait_for_text("Request sent.").await;
    let inbox = requests::inbox(&wren, &wren_c, theirs, false)
        .await
        .unwrap();
    requests::accept(&wren, &wren_c, inbox.requests[0].id)
        .await
        .unwrap();
    let session = sessions::list(&wren, &wren_c).await.unwrap()[0]
        .session
        .id
        .clone();
    // A tiny real PNG, and a text file.
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89\0\0\0\rIDATx\x9cc\xf8\xff\xff?\0\x05\xfe\x02\xfe\xa7\x35\x81\x84\0\0\0\0IEND\xaeB`\x82".to_vec();
    sessions::post(
        &wren,
        &wren_c,
        &session,
        sessions::Draft {
            text: "The cove, and my notes.",
            channel: None,
            reply_to: None,
            files: vec![
                ("cove.png".into(), png.clone()),
                ("notes.txt".into(), b"the keeper's ledger".to_vec()),
            ],
        },
    )
    .await
    .unwrap();

    d.click("Home").await;
    d.wait_until("word of the note", |d| {
        d.has_button("Read the session with @wren_pen on “The lighthouse ledger”")
    })
    .await;
    d.click("Read the session with @wren_pen on “The lighthouse ledger”")
        .await;
    d.wait_for_text("The cove, and my notes.").await;
    let src = d.image_src("cove.png").expect("the image on the note");
    let path = src
        .strip_prefix("/plait-file/")
        .expect("the app's own route");
    let (bytes, kind) = backend.image(path).expect("served from the store");
    assert_eq!((bytes.as_slice(), kind), (png.as_slice(), "image/png"));
    // Only the image types notes show are served: not the text file.
    let txt = path.replace("/png", "/plain");
    assert!(backend.image(&txt).is_none());

    d.click("Save notes.txt").await;
    d.wait_for_text("Saved to").await;
    assert_eq!(
        std::fs::read(saved.path().join("notes.txt")).unwrap(),
        b"the keeper's ledger"
    );
    server.stop().await.unwrap();
}
