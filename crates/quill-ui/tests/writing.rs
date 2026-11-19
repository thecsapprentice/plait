//! Writing in a session (3f-2), driven as a writer would against the
//! simulated server: sending, answering, the keyboard, drafts kept across
//! channels and restarts, notes waiting while offline (changed, withdrawn,
//! sent once back), a refusal, the limit, and the long-note editor.

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use dioxus_html::Key;
use quill_ui::{
    backend::{NOTE_MAX, codes},
    testing::{Driver, Sim},
};

const SERVER: &str = "writers.example";
const PASSWORD: &str = "correct horse battery";
const EDITOR: &str = "note-editor";

fn sim() -> Rc<Sim> {
    Sim::new().with_server(SERVER).rc()
}

/// Signed in as `ink` writing as `@ink_fox`, reading a session with
/// `@wren` on "The ledger", in Story. Returns the session's id.
async fn reading(sim: &Rc<Sim>) -> (Driver, String) {
    let mut d = Driver::new(sim.clone());
    d.type_into("server-address", SERVER).await;
    d.wait_for_text("Takes new logins").await;
    d.click("Continue").await;
    d.wait_for_text("Make a login on").await;
    d.type_into("login-name", "ink").await;
    d.type_into("login-password", PASSWORD).await;
    d.click("Make my login").await;
    d.wait_for_text("Make your first persona").await;
    d.type_into("persona-handle", "ink_fox").await;
    d.click("Make this persona").await;
    d.wait_for_text("Hello, @ink_fox").await;
    sim.take_handle(SERVER, "wren");
    let id = sim.session_with(SERVER, "ink_fox", "wren", "The ledger");
    d.click("Sessions").await;
    d.wait_for_text("On “The ledger”").await;
    d.click("Open the session with @wren on “The ledger”").await;
    d.wait_for_text("Writing with @wren").await;
    (d, id)
}

/// The texts in a session on the server, newest last.
fn texts(sim: &Sim, id: &str) -> Vec<String> {
    sim.notes_in(SERVER, id).into_iter().map(|n| n.2).collect()
}

/// Lets the editor's few-times-a-second work (saving, the preview) run.
async fn a_moment(d: &mut Driver) {
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    d.settle().await;
}

#[tokio::test]
async fn a_note_is_sent_and_the_editor_starts_again() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    assert!(d.button_disabled("Send"), "nothing to send yet");
    d.type_into(EDITOR, "The lamp **went out**.").await;
    d.click("Send").await;
    d.wait_for_text("Sent.").await;
    assert_eq!(texts(&sim, &id).last().unwrap(), "The lamp **went out**.");
    d.wait_for_text("The lamp went out").await;
    assert_eq!(d.attr(EDITOR, "value").as_deref(), Some(""));
    // Ctrl+Enter sends too.
    d.type_into(EDITOR, "And the bell.").await;
    d.press_command(EDITOR, Key::Enter).await;
    d.wait_until("it's sent", |_| {
        texts(&sim, &id).last().map(String::as_str) == Some("And the bell.")
    })
    .await;
}

#[tokio::test]
async fn answering_a_note_says_which_and_can_stop() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    let asked = sim.write_as(SERVER, &id, "wren", "story", "Chapter one, at *sea*?");
    d.wait_for_text("Chapter one, at").await;
    d.click("Answer @wren's note").await;
    d.wait_for_text("In answer to @wren: Chapter one, at sea?")
        .await;
    d.click("Stop answering").await;
    assert!(!d.has_text("In answer to @wren"));

    d.click("Answer @wren's note").await;
    d.type_into(EDITOR, "Yes, and a storm.").await;
    d.click("Send").await;
    d.wait_for_text("Sent.").await;
    let last = sim.notes_in(SERVER, &id).pop().unwrap();
    assert_eq!(
        (last.2.as_str(), last.3),
        ("Yes, and a storm.", Some(asked))
    );
    assert!(!d.has_text("In answer to @wren: Chapter"));
}

#[tokio::test]
async fn drafts_stay_with_their_channel_and_the_app() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.type_into(EDITOR, "Half a sentence about the").await;
    d.click("Planning").await;
    assert_eq!(d.attr(EDITOR, "value").as_deref(), Some(""));
    // Story keeps its draft, and says so.
    assert!(d.has_button("Story ✎ draft"));
    d.type_into(EDITOR, "Shall we keep it short?").await;
    d.click("Story ✎ draft").await;
    assert_eq!(
        d.attr(EDITOR, "value").as_deref(),
        Some("Half a sentence about the")
    );
    // An answer chosen is part of the draft.
    let asked = sim.write_as(SERVER, &id, "wren", "story", "Who rang the bell?");
    d.wait_for_text("Who rang the bell?").await;
    d.click("Answer @wren's note").await;
    a_moment(&mut d).await;

    // The window closes, and opens again.
    drop(d);
    let mut d = Driver::new(sim.clone());
    d.wait_for_text("Hello, @ink_fox").await;
    d.click("Sessions").await;
    d.click("Open the session with @wren on “The ledger”").await;
    d.wait_for_text("Writing with @wren").await;
    assert_eq!(
        d.attr(EDITOR, "value").as_deref(),
        Some("Half a sentence about the")
    );
    assert!(d.has_text("In answer to @wren: Who rang the bell?"));
    d.click("Planning ✎ draft").await;
    assert_eq!(
        d.attr(EDITOR, "value").as_deref(),
        Some("Shall we keep it short?")
    );
    // Sending clears the draft for good.
    d.click("Send").await;
    d.wait_for_text("Sent.").await;
    drop(d);
    let mut d = Driver::new(sim.clone());
    d.wait_for_text("Hello, @ink_fox").await;
    d.click("Sessions").await;
    d.click("Open the session with @wren on “The ledger”").await;
    d.wait_for_text("Writing with @wren").await;
    d.click("Planning").await;
    assert_eq!(d.attr(EDITOR, "value").as_deref(), Some(""));
    let _ = asked;
}

#[tokio::test]
async fn notes_written_offline_wait_can_change_and_go_once_back() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    sim.server(SERVER, |s| s.reachable = false);
    d.type_into(EDITOR, "Written on the train.").await;
    d.click("Send").await;
    d.wait_for_text("Not sent yet").await;
    d.type_into(EDITOR, "A second thought.").await;
    d.click("Send").await;
    d.wait_until("both wait", |d| d.has_text("A second thought."))
        .await;
    assert_eq!(sim.waiting_in(&id).len(), 2);
    assert!(!texts(&sim, &id).contains(&"Written on the train.".to_owned()));
    // What was read stays on the screen.
    assert!(d.has_text("Writing with @wren"));

    // Change the first; withdraw the second.
    d.click_nth("Change", 0).await;
    let field = d.field_id_with_prefix("waiting-");
    d.type_into(&field, "Written on the late train.").await;
    d.click("Save the change").await;
    d.wait_for_text("Written on the late train.").await;
    d.click_nth("Withdraw…", 1).await;
    d.wait_until("asked to confirm", |d| d.has_button("Withdraw it"))
        .await;
    d.click("Withdraw it").await;
    d.wait_until("one waits", |_| sim.waiting_in(&id).len() == 1)
        .await;
    assert_eq!(sim.waiting_in(&id)[0].0, "Written on the late train.");

    // Back online: upkeep sends it.
    sim.server(SERVER, |s| s.reachable = true);
    sim.live();
    d.wait_until("it's sent", |_| sim.waiting_in(&id).is_empty())
        .await;
    assert_eq!(
        texts(&sim, &id).last().unwrap(),
        "Written on the late train."
    );
    d.wait_until("it's no longer waiting", |d| !d.has_text("Waiting to send"))
        .await;
}

#[tokio::test]
async fn a_refused_note_waits_with_why() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Settings").await;
    d.set_checked("advanced", true).await;
    // Back to Sessions: the session is still open.
    d.click("Sessions").await;
    d.wait_for_text("Writing with @wren").await;
    // The other writer leaves just before the note goes, before this
    // computer has heard.
    a_moment(&mut d).await;
    sim.hold_sessions();
    sim.leave_as(SERVER, &id, "wren");
    d.type_into(EDITOR, "Hello?").await;
    d.click("Send").await;
    d.wait_for_text("The server didn't take it.").await;
    sim.release_sessions();
    d.wait_for_text("the session has ended").await;
    assert!(d.has_text(&codes::SESSION_CHECK_FAILED.to_string()));
    assert_eq!(sim.waiting_in(&id), [("Hello?".to_owned(), true)]);
    // Ended: no editor, but the note can be withdrawn.
    d.wait_for_text("This session has ended.").await;
    assert!(!d.has_field(EDITOR));
    d.click("Withdraw…").await;
    d.click("Withdraw it").await;
    d.wait_until("withdrawn", |_| sim.waiting_in(&id).is_empty())
        .await;
}

#[tokio::test]
async fn the_limit_is_said_and_a_long_note_gets_the_plain_editor() {
    let sim = sim();
    let (mut d, _) = reading(&sim).await;
    d.type_into(EDITOR, &"word ".repeat(NOTE_MAX / 5 + 1)).await;
    d.wait_for_text("5 characters over the limit of 20,000")
        .await;
    assert!(d.button_disabled("Send"));
    // Too long for Live: the plain text area, with a preview to hand.
    assert!(d.has_button("Show preview"));
    assert!(!d.has_button("Write in place"));
    d.type_into(EDITOR, "Short **again**.").await;
    assert!(!d.has_text("over the limit"));
    assert!(!d.button_disabled("Send"));
    d.click("Show preview").await;
    a_moment(&mut d).await;
    assert!(d.has_text("Short again ."));
    d.click("Write in place").await;
    assert!(d.has_button("Write and preview"));
}
