//! Leaving a session (3f-3), driven as a writer would against the
//! simulated server: one plain question in place, asked once, saying it
//! ends the session for both and what's written stays readable.

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use quill_ui::testing::{Driver, Sim};

const SERVER: &str = "writers.example";
const PASSWORD: &str = "correct horse battery";

fn sim() -> Rc<Sim> {
    Sim::new().with_server(SERVER).rc()
}

/// Signed in as `ink` writing as `@ink_fox`, reading a session with
/// `@wren` on "The ledger" that has a note in it. Returns its id.
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
    sim.write_as(SERVER, &id, "wren", "story", "The keeper woke.");
    d.click("Sessions").await;
    d.wait_for_text("On “The ledger”").await;
    d.click("Open the session with @wren on “The ledger”").await;
    d.wait_for_text("The keeper woke.").await;
    (d, id)
}

#[tokio::test]
async fn leaving_asks_once_and_ends_it_for_both() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Leave the session…").await;
    assert!(d.has_text("It ends for both of you"));
    assert!(d.has_text("What's written stays readable here."));
    d.click("Stay").await;
    assert!(!d.has_text("It ends for both of you"));
    assert!(!sim.session_ended(SERVER, &id));

    d.click("Leave the session…").await;
    d.click("Leave it").await;
    d.wait_for_text("This session has ended.").await;
    assert!(sim.session_ended(SERVER, &id));
    // Still readable; nothing more to write or leave.
    assert!(d.has_text("The keeper woke."));
    assert!(!d.has_field("note-editor"));
    assert!(!d.has_button("Leave the session…"));
}

#[tokio::test]
async fn an_outage_keeps_the_question_to_try_again() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Leave the session…").await;
    sim.server(SERVER, |s| s.reachable = false);
    d.click("Leave it").await;
    d.wait_for_text("Can't reach").await;
    assert!(d.has_text("It ends for both of you"));
    sim.server(SERVER, |s| s.reachable = true);
    d.click("Leave it").await;
    d.wait_for_text("This session has ended.").await;
    assert!(sim.session_ended(SERVER, &id));
}

#[tokio::test]
async fn the_other_writer_leaving_first_answers_the_question() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Leave the session…").await;
    sim.leave_as(SERVER, &id, "wren");
    d.wait_for_text("This session has ended.").await;
    assert!(!d.has_button("Leave it"));
}
