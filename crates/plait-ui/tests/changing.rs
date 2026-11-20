//! Changing notes (3f-3), driven as a writer would against the simulated
//! server: editing one of your notes and undoing it, deleting one and
//! undoing that (#104), each note's history on request, and what can't be
//! changed (someone else's note, an ended session, an empty note, an
//! outage mid-edit).

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use dioxus_html::Key;
use plait_ui::testing::{Driver, Sim};

const SERVER: &str = "writers.example";
const PASSWORD: &str = "correct horse battery";

fn sim() -> Rc<Sim> {
    Sim::new().with_server(SERVER).rc()
}

/// Signed in as `ink` writing as `@ink_fox`, with a note of their own
/// ("The lamp went out.", note 2) in a session with `@wren` on "The
/// ledger", reading Story. Returns the session's id.
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
    sim.write_as(SERVER, &id, "ink_fox", "story", "The lamp went out.");
    d.click("Sessions").await;
    d.wait_for_text("On “The ledger”").await;
    d.click("Open the session with @wren on “The ledger”").await;
    d.wait_for_text("The lamp went out.").await;
    (d, id)
}

/// The note's text on the server, and whether it's deleted (shown empty).
fn on_server(sim: &Sim, id: &str, nth: usize) -> String {
    sim.notes_in(SERVER, id)[nth].2.clone()
}

#[tokio::test]
async fn an_edit_shows_at_once_and_can_be_undone() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Edit your note").await;
    assert_eq!(
        d.attr("edit-2", "value").as_deref(),
        Some("The lamp went out.")
    );
    assert!(d.button_disabled("Save the change"), "nothing changed yet");
    d.type_into("edit-2", "The lamp went out at last.").await;
    d.click("Save the change").await;
    d.wait_for_text("Edited.").await;
    assert!(d.has_text("The lamp went out at last."));
    assert_eq!(on_server(&sim, &id, 1), "The lamp went out at last.");

    // Undone, not confirmed: the old words go back as a new edit.
    d.click("Undo").await;
    d.wait_for_text("Undone.").await;
    assert_eq!(on_server(&sim, &id, 1), "The lamp went out.");
    d.wait_for_text("The lamp went out.").await;
    assert!(!d.has_text("The lamp went out at last."));

    // Every version stays, shown on request.
    d.click("Edited").await;
    d.wait_for_text("Every version, oldest first.").await;
    assert!(d.has_text("The lamp went out at last."));
    d.click("Edited").await;
    assert!(!d.has_text("Every version, oldest first."));
}

#[tokio::test]
async fn the_keyboard_saves_and_cancels_an_edit() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Edit your note").await;
    d.type_into("edit-2", "Changed my mind.").await;
    d.press_in("edit-2", Key::Escape).await;
    assert!(!d.has_text("Changed my mind."));
    assert_eq!(on_server(&sim, &id, 1), "The lamp went out.");

    d.click("Edit your note").await;
    assert_eq!(
        d.attr("edit-2", "value").as_deref(),
        Some("The lamp went out.")
    );
    d.type_into("edit-2", "Out, at last.").await;
    d.press_command("edit-2", Key::Enter).await;
    d.wait_for_text("Edited.").await;
    assert_eq!(on_server(&sim, &id, 1), "Out, at last.");
}

#[tokio::test]
async fn a_deletion_is_undone_and_stays_in_the_history() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Edit your note").await;
    d.click("Delete this note").await;
    d.wait_for_text("This note was deleted.").await;
    assert!(d.has_text("It stays in its history for you both."));
    assert_eq!(on_server(&sim, &id, 1), "", "deleted for both");

    d.click("Show its history").await;
    d.wait_for_text("Every version, oldest first.").await;
    assert!(d.has_text("The lamp went out."), "the text is kept");

    d.click("Undo").await;
    d.wait_for_text("Undone.").await;
    assert!(!d.has_text("This note was deleted."));
    assert_eq!(on_server(&sim, &id, 1), "The lamp went out.");
}

#[tokio::test]
async fn the_other_writers_changes_show_with_their_history() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    let theirs = sim.write_as(SERVER, &id, "wren", "story", "Who lit it?");
    d.wait_for_text("Who lit it?").await;
    // Only your own notes can be changed.
    assert!(!d.has_button("Edit @wren's note"));
    sim.edit_note(SERVER, &id, theirs, "Who lit it first?");
    d.wait_for_text("Who lit it first?").await;
    sim.delete_note(SERVER, &id, theirs);
    d.wait_for_text("This note was deleted.").await;
    d.click("Show its history").await;
    d.wait_for_text("Every version, oldest first.").await;
    assert!(d.has_text("Who lit it?") && d.has_text("Who lit it first?"));
    assert!(d.has_text("Deleted."));
    // Their later edit brings it back (#104).
    sim.edit_note(SERVER, &id, theirs, "Who lit it, then?");
    d.wait_for_text("Who lit it, then?").await;
    assert!(!d.has_text("This note was deleted."));
}

#[tokio::test]
async fn an_empty_note_or_an_ended_session_cant_be_saved() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Edit your note").await;
    d.type_into("edit-2", "   ").await;
    assert!(d.button_disabled("Save the change"), "an empty note");
    d.type_into("edit-2", "Still here.").await;
    // The other writer leaves while this is being written: the session
    // ends, and saving says so, keeping the words.
    sim.leave_as(SERVER, &id, "wren");
    d.click("Save the change").await;
    d.wait_for_text("the session has ended").await;
    assert_eq!(d.attr("edit-2", "value").as_deref(), Some("Still here."));
    assert_eq!(on_server(&sim, &id, 1), "The lamp went out.");
    d.click("Cancel").await;
    d.wait_for_text("This session has ended.").await;
    assert!(
        !d.has_button("Edit your note"),
        "nothing changes once it ends"
    );
}

#[tokio::test]
async fn an_outage_mid_edit_keeps_the_words_to_try_again() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Edit your note").await;
    d.type_into("edit-2", "Written offline.").await;
    sim.server(SERVER, |s| s.reachable = false);
    d.click("Save the change").await;
    d.wait_for_text("Can't reach").await;
    assert_eq!(
        d.attr("edit-2", "value").as_deref(),
        Some("Written offline.")
    );
    sim.server(SERVER, |s| s.reachable = true);
    d.click("Save the change").await;
    d.wait_for_text("Edited.").await;
    assert_eq!(on_server(&sim, &id, 1), "Written offline.");
}
