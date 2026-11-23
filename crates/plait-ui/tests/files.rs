//! A note's files (3g-2): images shown on the note, with a larger view,
//! and every file saved to a folder chosen in the system's dialog, never
//! opened in another app.

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use dioxus_html::Key;
use plait_ui::testing::{Driver, Sim};

const SERVER: &str = "writers.example";
const PASSWORD: &str = "correct horse battery";
const SAVED: &str = "/home/ink/Saved";

fn sim() -> Rc<Sim> {
    Sim::new().with_server(SERVER).rc()
}

/// Signed in as `@ink_fox`, reading a session with `@wren`.
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
async fn an_image_shows_on_the_note_and_opens_larger() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    sim.write_with_files(
        SERVER,
        &id,
        "wren",
        "The cove at dawn.",
        &[("cove.jpg", &[0xff; 2048])],
    );
    d.wait_for_text("The cove at dawn.").await;
    let src = d.image_src("cove.jpg").expect("shown on the note");
    assert!(src.starts_with("sim-file:"), "{src}");
    assert!(d.has_text("cove.jpg, 2.0 KB"));
    d.click("See cove.jpg larger").await;
    assert!(d.has_element_with_class("lightbox"));
    d.assert_accessible("the larger view");
    d.press("Close", Key::Escape).await;
    assert!(!d.has_element_with_class("lightbox"));
    d.click("See cove.jpg larger").await;
    d.click("Close").await;
    assert!(!d.has_element_with_class("lightbox"));
}

#[tokio::test]
async fn other_files_are_listed_and_saved_never_opened() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    sim.write_with_files(
        SERVER,
        &id,
        "wren",
        "My notes, and a page.",
        &[
            ("notes.txt", b"the keeper's ledger"),
            ("page.svg", b"<svg/>"),
        ],
    );
    d.wait_for_text("notes.txt, 19 bytes").await;
    // SVG is a file to save, not a picture: only plain image types show.
    assert!(d.image_src("page.svg").is_none());
    assert!(d.has_text("page.svg, 6 bytes"));
    assert!(!d.has_button("Open"), "nothing opens in another app");
    sim.folder_picks(Some(SAVED));
    d.click("Save notes.txt").await;
    d.wait_for_text("Saved to /home/ink/Saved/notes.txt.").await;
    assert_eq!(
        sim.on_disk(),
        [(
            format!("{SAVED}/notes.txt"),
            b"the keeper's ledger".to_vec()
        )]
    );
}

#[tokio::test]
async fn saving_never_overwrites_and_keeps_names_safe() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    sim.put_on_disk(&format!("{SAVED}/notes.txt"), b"mine");
    sim.write_with_files(
        SERVER,
        &id,
        "wren",
        "Two files.",
        &[("../../notes.txt", b"theirs"), ("map.pdf", b"%PDF")],
    );
    d.wait_for_text("Two files.").await;
    sim.folder_picks(Some(SAVED));
    d.click("Save all 2 files…").await;
    d.wait_for_text("Saved 2 files to /home/ink/Saved.").await;
    let disk = sim.on_disk();
    assert!(
        disk.contains(&(format!("{SAVED}/notes.txt"), b"mine".to_vec())),
        "kept"
    );
    assert!(disk.contains(&(format!("{SAVED}/1-notes.txt"), b"theirs".to_vec())));
    assert!(disk.contains(&(format!("{SAVED}/map.pdf"), b"%PDF".to_vec())));
}

#[tokio::test]
async fn cancelling_the_folder_dialog_saves_nothing_and_says_nothing() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    sim.write_with_files(SERVER, &id, "wren", "A file.", &[("notes.txt", b"x")]);
    d.wait_for_text("notes.txt, 1 byte").await;
    sim.folder_picks(None);
    d.click("Save notes.txt").await;
    d.settle().await;
    assert!(sim.on_disk().is_empty());
    assert!(!d.has_text("Saved"));
    assert!(!d.button_disabled("Save notes.txt"), "ready to try again");
}

#[tokio::test]
async fn files_not_on_this_computer_say_so_and_offer_nothing() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    let seq = sim.write_with_files(
        SERVER,
        &id,
        "wren",
        "Before you linked.",
        &[("cove.jpg", &[0xff; 10]), ("notes.txt", b"x")],
    );
    sim.files_missing_here(SERVER, &id, seq);
    d.wait_for_text("Before you linked.").await;
    assert!(d.image_src("cove.jpg").is_none(), "nothing to show");
    assert!(d.has_text("cove.jpg, 10 bytes"));
    assert!(d.has_text("Not on this computer yet."));
    assert!(!d.has_button("Save notes.txt"));
    assert!(!d.has_button_containing("Save all"));
}

#[tokio::test]
async fn a_deleted_notes_files_go_with_its_text() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    let seq = sim.write_with_files(SERVER, &id, "wren", "Gone soon.", &[("notes.txt", b"x")]);
    d.wait_for_text("notes.txt, 1 byte").await;
    sim.delete_note(SERVER, &id, seq);
    d.wait_for_text("This note was deleted.").await;
    assert!(!d.has_text("notes.txt"));
}
