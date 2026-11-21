//! The brand's art, compiled in from `assets/brand/` for both UIs (the
//! desktop app and the public board). These are our own SVGs, so inlining
//! them takes no outside input. Each is decoration: callers hide it from
//! assistive tech, and say in words whatever it shows.

use dioxus::prelude::*;

use crate::icons::Icon;

/// The welcome screens' hero: threads woven into cloth, for light themes.
pub const HERO_LIGHT: &str = include_str!("../../../assets/brand/hero/hero-light.svg");
/// The hero for dark themes. Its ids differ from the light one's, so both
/// can be on one page.
pub const HERO_DARK: &str = include_str!("../../../assets/brand/hero/hero-dark.svg");
/// A thin plait along a card's top edge.
pub const PLAIT_BAND: &str = include_str!("../../../assets/brand/illustrations/plait-band.svg");
/// A shuttle crossing the warp, while something takes a moment. It stops
/// when the system asks for reduced motion.
pub const LOADER_SHUTTLE: &str =
    include_str!("../../../assets/brand/illustrations/loader-shuttle.svg");

/// An illustration for an empty or unreachable state. The words beside it
/// say plainly what's missing; the drawing only adds the weaving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Illustration {
    /// No prompts yet: warp threads hanging, waiting.
    EmptyBoard,
    /// No requests: a shuttle at rest.
    EmptyRequests,
    /// No sessions: an empty loom.
    EmptySessions,
    /// Nothing matches a search or filter: a loose thread.
    NoResults,
    /// The server can't be reached: a broken thread.
    Offline,
}

impl Illustration {
    pub const fn svg(self) -> &'static str {
        match self {
            Illustration::EmptyBoard => {
                include_str!("../../../assets/brand/illustrations/empty-board.svg")
            }
            Illustration::EmptyRequests => {
                include_str!("../../../assets/brand/illustrations/empty-requests.svg")
            }
            Illustration::EmptySessions => {
                include_str!("../../../assets/brand/illustrations/empty-sessions.svg")
            }
            Illustration::NoResults => {
                include_str!("../../../assets/brand/illustrations/no-results.svg")
            }
            Illustration::Offline => {
                include_str!("../../../assets/brand/illustrations/offline.svg")
            }
        }
    }

    /// The class naming which one is shown, for styles and tests.
    pub const fn class(self) -> &'static str {
        match self {
            Illustration::EmptyBoard => "illustration empty-board",
            Illustration::EmptyRequests => "illustration empty-requests",
            Illustration::EmptySessions => "illustration empty-sessions",
            Illustration::NoResults => "illustration no-results",
            Illustration::Offline => "illustration offline",
        }
    }
}

/// An illustration, hidden from assistive tech (its words are beside it).
pub fn illustration(which: Illustration) -> Element {
    rsx! {
        div { class: which.class(), "aria-hidden": "true", dangerous_inner_html: which.svg() }
    }
}

/// A line icon beside a text label, so hidden from assistive tech. An icon
/// alone on a button needs that button's `aria-label` set to
/// `Icon::meaning()` instead.
pub fn icon(which: Icon, class: &'static str) -> Element {
    rsx! {
        svg { class, "viewBox": "0 0 24 24", "aria-hidden": "true",
            "fill": "none", "stroke": "currentColor", "stroke-width": "2",
            "stroke-linecap": "round", "stroke-linejoin": "round",
            path { "d": which.path() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ILLUSTRATIONS: [Illustration; 5] = [
        Illustration::EmptyBoard,
        Illustration::EmptyRequests,
        Illustration::EmptySessions,
        Illustration::NoResults,
        Illustration::Offline,
    ];

    #[test]
    fn the_art_is_inline_svg_without_scripts_or_outside_links() {
        let all = [HERO_LIGHT, HERO_DARK, PLAIT_BAND, LOADER_SHUTTLE]
            .into_iter()
            .chain(ILLUSTRATIONS.map(Illustration::svg));
        for svg in all {
            assert!(svg.starts_with("<svg "), "{}", &svg[..40]);
            for bad in ["<script", "onload", "href=\"http", "<foreignObject"] {
                assert!(!svg.contains(bad), "{bad}");
            }
        }
    }

    #[test]
    fn the_icons_are_the_generated_ones() {
        // icons.rs is a copy, so a regenerated set must be copied again.
        assert_eq!(
            include_str!("icons.rs"),
            include_str!("../../../assets/brand/icons/icons.rs")
        );
    }
}
