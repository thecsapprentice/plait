//! The Storage view (3g-1): what the login's sessions take on the server,
//! reached from Settings, as `plait storage` shows it.

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use plait_ui::testing::{Driver, Sim};

const SERVER: &str = "writers.example";
const PASSWORD: &str = "correct horse battery";
const MIB: u64 = 1024 * 1024;

/// A fresh install signed in as `ink`, writing as `@ink_fox`, on Home.
async fn signed_in(sim: &Rc<Sim>) -> Driver {
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
    d
}

async fn open_storage(d: &mut Driver) {
    d.click("Settings").await;
    d.click("See storage use").await;
    d.wait_until("the totals", |d| {
        !d.has_text("Loading what your sessions take")
    })
    .await;
}

#[tokio::test]
async fn storage_shows_the_allowance_totals_and_each_session() {
    let sim = Sim::new().with_server(SERVER).rc();
    let mut d = signed_in(&sim).await;
    let id = sim.session_with(SERVER, "ink_fox", "wren", "The ledger");
    sim.uploaded(SERVER, &id, "ink_fox", 3 * MIB);
    sim.uploaded(SERVER, &id, "wren", MIB);
    open_storage(&mut d).await;
    assert_eq!(d.heading().as_deref(), Some("Storage"));
    assert!(d.has_text("What your sessions take on writers.example."));
    // Only what this login uploaded counts against its allowance.
    assert!(d.has_text("3.0 MB of 500 MB"));
    assert!(!d.has_text("Nearly full"));
    assert!(d.has_text("Keeping your devices in step"));
    assert!(d.has_text("@ink_fox with @wren"));
    assert!(d.has_text("1 message, 265 bytes."));
    assert!(d.has_text("2 attachments, 4.0 MB (you uploaded 3.0 MB)."));
    assert!(!d.has_text("Ended"));
    d.assert_accessible("Storage");
}

#[tokio::test]
async fn a_nearly_full_allowance_says_so() {
    let sim = Sim::new().with_server(SERVER).rc();
    let mut d = signed_in(&sim).await;
    let id = sim.session_with(SERVER, "ink_fox", "wren", "The ledger");
    sim.uploaded(SERVER, &id, "ink_fox", 460 * MIB);
    open_storage(&mut d).await;
    assert!(d.has_text("460 MB of 500 MB"));
    assert!(d.has_text("Nearly full."));
}

#[tokio::test]
async fn with_no_sessions_nothing_is_stored_for_them() {
    let sim = Sim::new().with_server(SERVER).rc();
    let mut d = signed_in(&sim).await;
    open_storage(&mut d).await;
    assert!(d.has_text("0 bytes of 500 MB"));
    assert!(d.has_text("No sessions yet, so nothing stored for them."));
}

#[tokio::test]
async fn offline_it_says_so_and_tries_again() {
    let sim = Sim::new().with_server(SERVER).rc();
    let mut d = signed_in(&sim).await;
    sim.session_with(SERVER, "ink_fox", "wren", "The ledger");
    d.click("Settings").await;
    sim.server(SERVER, |s| s.reachable = false);
    d.click("See storage use").await;
    d.wait_for_text("Try again").await;
    assert!(!d.has_text("of 500 MB"));
    sim.server(SERVER, |s| s.reachable = true);
    d.click("Try again").await;
    d.wait_for_text("@ink_fox with @wren").await;
}

#[tokio::test]
async fn an_ended_session_still_takes_space_and_says_ended() {
    let sim = Sim::new().with_server(SERVER).rc();
    let mut d = signed_in(&sim).await;
    let id = sim.session_with(SERVER, "ink_fox", "wren", "The ledger");
    sim.leave_as(SERVER, &id, "wren");
    open_storage(&mut d).await;
    assert!(d.has_text("@ink_fox with @wren"));
    assert!(d.has_text("Ended"));
}
