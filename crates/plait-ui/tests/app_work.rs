//! #111: a server's answer reaches the app even when the screen that asked
//! has closed first. On a slow network the server acts at once and its
//! reply comes late (`Sim::hold_reply`); each test leaves the screen
//! before the reply, then checks the app caught up.

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use plait_ui::testing::{Driver, Sim};

const SERVER: &str = "writers.example";
const OTHER: &str = "other.example";
const PASSWORD: &str = "correct horse battery";

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

// ---- Settings -------------------------------------------------------------------

#[tokio::test]
async fn signing_out_lands_even_if_settings_closes_first() {
    let sim = Sim::new().with_server(SERVER).rc();
    let mut d = signed_in(&sim).await;
    d.click("Settings").await;
    sim.hold_reply("sign_out");
    d.click("Sign out of writers.example").await;
    d.click("Home").await;
    sim.release_reply("sign_out");
    // Signed out on the server, so the app says so too.
    d.wait_for_text("Sign in to writers.example").await;
}

#[tokio::test]
async fn a_removed_server_goes_even_if_settings_closes_first() {
    let sim = Sim::new().with_server(SERVER).with_server(OTHER).rc();
    let mut d = signed_in(&sim).await;
    d.choose("server-switch", "new").await;
    d.type_into("server-address", OTHER).await;
    d.wait_for_text("Takes new logins").await;
    d.click("Continue").await;
    d.type_into("login-name", "plait").await;
    d.type_into("login-password", PASSWORD).await;
    d.click("Make my login").await;
    d.click("Not now").await;
    d.wait_for_text("Connected to other.example").await;
    d.click("Settings").await;
    sim.hold_reply("remove_server");
    d.click_nth("Remove…", 1).await;
    d.click("Remove other.example").await;
    d.click("Home").await;
    sim.release_reply("remove_server");
    d.wait_for_text("Connected to writers.example").await;
    assert!(!d.has_text("other.example"), "no longer listed");
}

#[tokio::test]
async fn moved_stores_show_their_new_folder_even_if_settings_closes_first() {
    let sim = Sim::new().with_server(SERVER).rc();
    let mut d = signed_in(&sim).await;
    d.click("Settings").await;
    d.set_checked("advanced", true).await;
    sim.hold_reply("move_stores");
    d.type_into("stores-folder", "/elsewhere").await;
    d.click("Move them").await;
    d.click("Home").await;
    sim.release_reply("move_stores");
    d.settle().await;
    d.click("Settings").await;
    assert_eq!(
        d.attr("stores-folder", "value").as_deref(),
        Some("/elsewhere")
    );
}
