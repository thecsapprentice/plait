//! Naming channels (3f-3), driven as a writer would against the simulated
//! server: a new channel named as typed and opened, a channel described
//! again, names that can't be used, the other writer naming one, and an
//! outage or an ended session mid-form.

// Test code: panicking on failure is intended.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use plait_ui::testing::{Driver, Sim};

const SERVER: &str = "writers.example";
const PASSWORD: &str = "correct horse battery";

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

#[tokio::test]
async fn a_new_channel_is_named_as_typed_and_opened() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("New channel").await;
    assert!(d.button_disabled("Make the channel"), "no name yet");
    d.type_into("channel-name", "World building").await;
    assert!(d.has_text("The tab will say World building."));
    d.type_into("channel-about", "Places, maps and weather.")
        .await;
    d.click("Make the channel").await;
    d.wait_until("it's made", |_| !sim.channels_in(SERVER, &id).is_empty())
        .await;
    assert_eq!(
        sim.channels_in(SERVER, &id),
        [(
            "world-building".to_owned(),
            Some("Places, maps and weather.".to_owned())
        )]
    );
    // It opens, ready to write in.
    d.wait_for_text("Nothing in World building yet.").await;
    assert!(d.has_text("Places, maps and weather."));
    assert_eq!(
        d.attr("channel-tab-world-building", "aria-selected")
            .as_deref(),
        Some("true")
    );
}

#[tokio::test]
async fn names_that_cant_be_used_are_said_as_typed() {
    let sim = sim();
    let (mut d, _) = reading(&sim).await;
    d.click("New channel").await;
    d.type_into("channel-name", "Story").await;
    assert!(d.has_text("There's already a channel called Story."));
    assert!(d.button_disabled("Make the channel"));
    d.type_into("channel-name", "Café!").await;
    assert!(d.has_text("Only the letters a to z, numbers and spaces work here."));
    assert!(d.button_disabled("Make the channel"));
    d.click("Cancel").await;
    assert!(!d.has_field("channel-name"));
}

#[tokio::test]
async fn a_channel_is_described_again() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("Describe this channel").await;
    d.type_into("channel-about", "The story itself, in order.")
        .await;
    d.click("Save the description").await;
    d.wait_for_text("The story itself, in order.").await;
    assert_eq!(
        sim.channels_in(SERVER, &id),
        [(
            "story".to_owned(),
            Some("The story itself, in order.".to_owned())
        )]
    );
    // Still one Story tab, first.
    assert_eq!(d.text_within("channels").matches("Story").count(), 1);
    d.click("Change the description").await;
    assert_eq!(
        d.attr("channel-about", "value").as_deref(),
        Some("The story itself, in order.")
    );
}

#[tokio::test]
async fn the_other_writers_channel_appears_live() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    sim.name_channel(SERVER, &id, "research", "Sources and notes.");
    d.wait_until("the tab appears", |d| d.has_button("Research"))
        .await;
    d.click("Research").await;
    d.wait_for_text("Sources and notes.").await;
}

#[tokio::test]
async fn an_outage_or_an_ended_session_keeps_the_form() {
    let sim = sim();
    let (mut d, id) = reading(&sim).await;
    d.click("New channel").await;
    d.type_into("channel-name", "Maps").await;
    sim.server(SERVER, |s| s.reachable = false);
    d.click("Make the channel").await;
    d.wait_for_text("Can't reach").await;
    assert_eq!(d.attr("channel-name", "value").as_deref(), Some("Maps"));
    sim.server(SERVER, |s| s.reachable = true);
    sim.leave_as(SERVER, &id, "wren");
    d.click("Make the channel").await;
    d.wait_for_text("the session has ended").await;
    assert!(sim.channels_in(SERVER, &id).is_empty());
}
