//! Attaching files to a note (3g-3): picked in the system's dialog or
//! dropped on the editor, refused plainly when the server wouldn't take
//! them, kept with the note while it waits offline, and sent with clear
//! progress.

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use plait_ui::testing::{Driver, Interrupt, Sim};

const SERVER: &str = "writers.example";
const PASSWORD: &str = "correct horse battery";
const COVE: &str = "/home/ink/Pictures/cove.png";
const NOTES: &str = "/home/ink/notes.txt";

fn sim() -> Rc<Sim> {
    let sim = Sim::new().with_server(SERVER).rc();
    sim.put_on_disk(COVE, &[0x89; 3000]);
    sim.put_on_disk(NOTES, b"the keeper's ledger");
    sim
}

/// Signed in as `@ink_fox`, writing in a session with `@wren`.
async fn writing(sim: &Rc<Sim>) -> (Driver, String) {
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
async fn picked_files_go_with_the_note() {
    let sim = sim();
    let (mut d, _) = writing(&sim).await;
    sim.files_pick(&[COVE, NOTES]);
    d.click("Attach files…").await;
    d.wait_for_text("cove.png, 2.9 KB").await;
    assert!(d.has_text("notes.txt, 19 bytes"));
    d.type_into("note-editor", "The cove, and my notes.").await;
    d.click("Send").await;
    d.wait_for_text("Sent.").await;
    // The composer is clear, and the note carries both, as received ones do.
    assert!(!d.has_button("Remove cove.png"));
    d.wait_until("the image on the note", |d| {
        d.image_src("cove.png").is_some()
    })
    .await;
    assert!(d.has_text("notes.txt, 19 bytes"));
    d.assert_accessible("a sent note with files");
}

#[tokio::test]
async fn files_alone_are_a_note() {
    let sim = sim();
    let (mut d, _) = writing(&sim).await;
    assert!(d.button_disabled("Send"), "nothing to send yet");
    sim.files_pick(&[NOTES]);
    d.click("Attach files…").await;
    d.wait_for_text("notes.txt, 19 bytes").await;
    assert!(!d.button_disabled("Send"));
    d.click("Send").await;
    d.wait_for_text("Sent.").await;
    d.wait_until("the file on the note", |d| d.has_button("Save notes.txt"))
        .await;
}

#[tokio::test]
async fn dropped_files_are_attached_and_can_be_taken_off() {
    let sim = sim();
    let (mut d, _) = writing(&sim).await;
    d.drop_files("note-editor", &[COVE, NOTES]).await;
    d.wait_for_text("cove.png, 2.9 KB").await;
    d.click("Remove cove.png").await;
    assert!(!d.has_text("cove.png"));
    assert!(d.has_text("notes.txt, 19 bytes"));
    d.click("Send").await;
    d.wait_for_text("Sent.").await;
    d.wait_until("the file on the note", |d| d.has_button("Save notes.txt"))
        .await;
    assert!(d.image_src("cove.png").is_none(), "taken off, never sent");
}

#[tokio::test]
async fn a_file_the_server_wouldnt_take_is_said_and_the_rest_still_come() {
    let sim = sim();
    sim.put_on_disk("/home/ink/film.mov", &vec![0u8; 11 * 1024 * 1024]);
    let (mut d, _) = writing(&sim).await;
    sim.files_pick(&["/home/ink/film.mov", NOTES, "/home/ink/gone.txt"]);
    d.click("Attach files…").await;
    d.wait_for_text("notes.txt, 19 bytes").await;
    // The last problem stays said: the missing file.
    assert!(d.has_text("Couldn't read gone.txt"));
    assert!(!d.has_text("film.mov,"), "too large, so not attached");
    sim.files_pick(&["/home/ink/film.mov"]);
    d.click("Attach files…").await;
    d.wait_for_text("film.mov is too large: writers.example takes files up to 10 MB.")
        .await;
}

#[tokio::test]
async fn at_most_ten_files_go_with_one_note() {
    let sim = sim();
    let (mut d, _) = writing(&sim).await;
    let paths: Vec<String> = (0..11).map(|n| format!("/home/ink/{n}.txt")).collect();
    for p in &paths {
        sim.put_on_disk(p, b"x");
    }
    sim.files_pick(&paths.iter().map(String::as_str).collect::<Vec<_>>());
    d.click("Attach files…").await;
    d.wait_for_text("A note carries at most 10 files.").await;
    assert!(d.has_text("9.txt"));
    assert!(!d.has_text("10.txt"));
}

#[tokio::test]
async fn a_server_without_attachments_says_so_before_anything_is_attached() {
    let sim = sim();
    sim.server(SERVER, |s| s.attachments = false);
    let (mut d, _) = writing(&sim).await;
    d.wait_for_text("This server doesn't take attachments.")
        .await;
    assert!(!d.has_button("Attach files…"));
    // Dropping still answers, in the same words.
    d.drop_files("note-editor", &[NOTES]).await;
    d.wait_for_text("writers.example doesn't take attachments")
        .await;
    assert!(!d.has_button("Remove notes.txt"));
}

#[tokio::test]
async fn attachments_turned_off_after_attaching_are_refused_when_sent() {
    let sim = sim();
    let (mut d, _) = writing(&sim).await;
    d.drop_files("note-editor", &[NOTES]).await;
    d.wait_for_text("notes.txt, 19 bytes").await;
    sim.server(SERVER, |s| s.attachments = false);
    d.type_into("note-editor", "My notes.").await;
    d.click("Send").await;
    d.wait_for_text("The server didn't take it.").await;
    assert!(d.has_text("this server doesn't take attachments"));
    assert!(
        d.has_text("With 1 file: notes.txt"),
        "the waiting note keeps it"
    );
}

#[tokio::test]
async fn offline_the_files_wait_with_the_note_and_go_with_it() {
    let sim = sim();
    let (mut d, id) = writing(&sim).await;
    d.drop_files("note-editor", &[COVE, NOTES]).await;
    d.wait_for_text("notes.txt, 19 bytes").await;
    sim.server(SERVER, |s| s.reachable = false);
    d.type_into("note-editor", "Written offline.").await;
    d.click("Send").await;
    d.wait_for_text("Not sent yet").await;
    assert!(d.has_text("With 2 files: cove.png, notes.txt"));
    assert_eq!(sim.waiting_in(&id).len(), 1);
    sim.server(SERVER, |s| s.reachable = true);
    d.click("Try again").await;
    d.wait_until("the image on the note", |d| {
        d.image_src("cove.png").is_some()
    })
    .await;
    assert!(sim.waiting_in(&id).is_empty());
}

#[tokio::test]
async fn progress_says_which_file_is_going_up() {
    let sim = sim();
    let (mut d, _) = writing(&sim).await;
    d.drop_files("note-editor", &[COVE, NOTES]).await;
    d.wait_for_text("notes.txt, 19 bytes").await;
    sim.hold_reply("send_note");
    d.click("Send").await;
    d.wait_for_text("Uploading cove.png (1 of 2)…").await;
    assert!(d.has_button("Sending…"));
    sim.release_reply("send_note");
    d.wait_for_text("Sent.").await;
    assert!(!d.has_text("Uploading"));
}

// ---- Sending stopped partway (#125) ----------------------------------------------

/// What Storage says the session holds, after leaving the session.
async fn stored(d: &mut Driver) -> bool {
    d.click("Settings").await;
    d.click("See storage use").await;
    d.wait_until("the totals", |d| d.has_text("@ink_fox with @wren"))
        .await;
    // One copy of each file: the retry took only what was left.
    d.has_text("2 attachments,")
}

#[tokio::test]
async fn a_server_gone_partway_takes_only_the_rest_when_tried_again() {
    let sim = sim();
    let (mut d, _) = writing(&sim).await;
    d.drop_files("note-editor", &[COVE, NOTES]).await;
    d.wait_for_text("notes.txt, 19 bytes").await;
    sim.server(SERVER, |s| {
        s.interrupt_before_file = Some((2, Interrupt::Away))
    });
    d.type_into("note-editor", "Two files.").await;
    d.click("Send").await;
    d.wait_for_text("Not sent yet").await;
    assert!(d.has_text("With 2 files: cove.png, notes.txt"));
    sim.server(SERVER, |s| s.reachable = true);
    d.click("Try again").await;
    d.wait_until("sent", |d| d.image_src("cove.png").is_some())
        .await;
    assert!(stored(&mut d).await);
}

#[tokio::test]
async fn attachments_switched_off_partway_refuse_and_go_once_back_on() {
    let sim = sim();
    let (mut d, _) = writing(&sim).await;
    d.drop_files("note-editor", &[COVE, NOTES]).await;
    d.wait_for_text("notes.txt, 19 bytes").await;
    sim.server(SERVER, |s| {
        s.interrupt_before_file = Some((2, Interrupt::AttachmentsOff));
    });
    d.type_into("note-editor", "Two files.").await;
    d.click("Send").await;
    d.wait_for_text("The server didn't take it.").await;
    assert!(d.has_text("this server doesn't take attachments"));
    assert!(d.has_text("With 2 files: cove.png, notes.txt"));
    sim.server(SERVER, |s| s.attachments = true);
    // "Try again" sends a refused note again, now the server takes it.
    d.click("Try again").await;
    d.wait_until("sent", |d| d.image_src("cove.png").is_some())
        .await;
    assert!(stored(&mut d).await);
}
