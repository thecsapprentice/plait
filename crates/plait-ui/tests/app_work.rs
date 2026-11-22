//! #111: a server's answer reaches the app even when the screen that asked
//! has closed first. On a slow network the server acts at once and its
//! reply comes late (`Sim::hold_reply`); each test leaves the screen
//! before the reply, then checks the app caught up.

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use plait_ui::testing::{Acceptance, Driver, Sim};

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

// ---- Personas -------------------------------------------------------------------

#[tokio::test]
async fn an_edited_persona_shows_its_new_name_even_if_personas_closes_first() {
    let sim = Sim::new().with_server(SERVER).rc();
    let mut d = signed_in(&sim).await;
    d.click("Personas").await;
    d.click("Edit").await;
    let display = d.field_id_with_prefix("display-");
    d.type_into(&display, "Ink Fox").await;
    sim.hold_reply("update_persona");
    d.click("Save").await;
    d.click("Home").await;
    sim.release_reply("update_persona");
    d.settle().await;
    d.click("Personas").await;
    d.wait_for_text("Ink Fox @ink_fox").await;
}

#[tokio::test]
async fn a_deleted_persona_goes_even_if_personas_closes_first() {
    let sim = Sim::new().with_server(SERVER).rc();
    let mut d = signed_in(&sim).await;
    d.click("Personas").await;
    sim.hold_reply("delete_persona");
    d.click("Delete…").await;
    d.click("Delete @ink_fox").await;
    d.click("Home").await;
    sim.release_reply("delete_persona");
    d.wait_until("the persona gone", |d| !d.has_text("@ink_fox"))
        .await;
}

// ---- Rules ----------------------------------------------------------------------

#[tokio::test]
async fn accepted_rules_clear_the_banner_even_if_the_rules_close_first() {
    let sim = Sim::new().with_server(SERVER).rc();
    let d = signed_in(&sim).await;
    drop(d);
    sim.set_rules(SERVER, "# New rules\n\nBe kind.", Acceptance::OnChange);
    let mut d = Driver::new(sim.clone());
    d.wait_for_text("The server rules have changed").await;
    d.click("Read the rules").await;
    d.wait_for_text("writers.example's rules").await;
    sim.hold_reply("accept_rules");
    d.click("I accept the rules").await;
    d.click("Home").await;
    sim.release_reply("accept_rules");
    d.wait_until("the banner gone", |d| {
        !d.has_text("The server rules have changed")
    })
    .await;
    assert_eq!(sim.accepted(SERVER, "ink"), 1);
}
