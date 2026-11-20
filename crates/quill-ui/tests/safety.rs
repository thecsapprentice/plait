//! Safety numbers in a session (3f-3), driven as a writer would against
//! the simulated server: offered, never forced; "They match" marks the
//! other writer verified on this computer; a verified writer's changed key
//! is said plainly, and checked with the words as in requests (3e).

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
/// `@wren` on "The ledger". Returns the session's id.
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

#[tokio::test]
async fn comparing_is_offered_and_they_match_verifies() {
    let sim = sim();
    let (mut d, _) = reading(&sim).await;
    // Never forced: writing works without comparing.
    assert!(d.has_field("note-editor"));
    d.click("Compare safety numbers").await;
    d.wait_for_text("Your 12 words with @wren").await;
    assert!(d.has_text("And 6 for the session's keys now"));
    d.click("They match").await;
    d.wait_for_text("You marked @wren verified on this computer.")
        .await;
    d.click("Hide the safety numbers").await;
    d.wait_for_text("Verified on this computer").await;
    assert!(!d.has_button("Compare safety numbers"));

    // It stays verified when the session is opened again.
    d.click("‹ All sessions").await;
    d.click("Open the session with @wren on “The ledger”").await;
    d.wait_for_text("Verified on this computer").await;
}

#[tokio::test]
async fn a_verified_writers_changed_key_is_said_plainly() {
    let sim = sim();
    let (mut d, _) = reading(&sim).await;
    d.click("Compare safety numbers").await;
    d.click("They match").await;
    d.wait_for_text("You marked @wren verified on this computer.")
        .await;
    d.click("Hide the safety numbers").await;

    sim.change_key(SERVER, "wren");
    sim.live();
    d.wait_for_text("@wren's identity key changed since you compared safety numbers.")
        .await;
    assert!(!d.has_text("Verified on this computer"));
    d.click("Check the new key").await;
    d.wait_for_text("@wren's identity key changed").await;
    d.click("The words match: carry on").await;
    // Trusted again, but not verified: that needs comparing again.
    d.wait_until("the notice goes", |d| {
        !d.has_text("since you compared safety numbers")
    })
    .await;
    assert!(d.has_button("Compare safety numbers"));
}

#[tokio::test]
async fn a_key_changed_before_comparing_is_checked_first() {
    let sim = sim();
    let (mut d, _) = reading(&sim).await;
    // This computer pins @wren's key when it first sees it.
    d.click("Compare safety numbers").await;
    d.wait_for_text("Your 12 words with @wren").await;
    d.click("Hide the safety numbers").await;
    sim.change_key(SERVER, "wren");
    d.click("Compare safety numbers").await;
    d.wait_for_text("@wren's identity key changed").await;
    assert!(!d.has_button("They match"), "not until the key is checked");
    d.click("The words match: carry on").await;
    d.wait_for_text("Your 12 words with @wren").await;
    assert!(d.has_button("They match"));
}

#[tokio::test]
async fn an_outage_says_so_and_a_left_writer_has_nothing_to_compare() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    sim.server(SERVER, |s| s.reachable = false);
    d.click("Compare safety numbers").await;
    d.wait_for_text("Can't reach").await;
    sim.server(SERVER, |s| s.reachable = true);
    d.click("Hide the safety numbers").await;
    sim.leave_as(SERVER, &id, "wren");
    d.wait_for_text("This session has ended.").await;
    assert!(!d.has_button("Compare safety numbers"));
}
