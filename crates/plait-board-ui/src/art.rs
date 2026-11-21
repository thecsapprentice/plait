//! The brand's art, compiled in from `assets/brand/` for both UIs (the
//! desktop app and the public board). These are our own SVGs, so inlining
//! them takes no outside input. Each is decoration: callers hide it from
//! assistive tech, and say in words whatever it shows.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_art_is_inline_svg_without_scripts_or_outside_links() {
        for svg in [HERO_LIGHT, HERO_DARK, PLAIT_BAND, LOADER_SHUTTLE] {
            assert!(svg.starts_with("<svg "), "{}", &svg[..40]);
            for bad in ["<script", "onload", "href=\"http", "<foreignObject"] {
                assert!(!svg.contains(bad), "{bad}");
            }
        }
    }
}
