//! The type scale for films, in pixels on a 1080-line frame, and the reading
//! rules that go with it. A frame has one boss at least three times the size
//! of the next thing; nothing is set below [`LABEL`], and `plan check` warns
//! on text under [`FLOOR`] of the frame's short side.
//!
//! Sources: Claude-Video-Skills `motion-film` (display 150-200, headline
//! 96-130, subline 36-52, hold 0.4 s + 0.2 s a word) and HyperFrames
//! `review.md` (`text_below_floor` under 2.6% of the short side).

/// One word, a number, or a verdict that fills the frame.
pub const HERO: f32 = 220.0;
/// A display line: the beat's claim.
pub const DISPLAY: f32 = 160.0;
/// A headline over evidence.
pub const HEADLINE: f32 = 112.0;
/// A subline, a sentence of support.
pub const BODY: f32 = 52.0;
/// Code on film: about 12 lines fit a 1080 frame at this size.
pub const CODE: f32 = 42.0;
/// Labels, axis names, file paths: the smallest text a film sets.
pub const LABEL: f32 = 36.0;

/// The legibility floor as a fraction of the frame's short side (28 px at
/// 1080): text below it can't be read on a phone.
pub const FLOOR: f32 = 0.026;

/// The most code lines a frame shows at once at [`CODE`] size.
pub const MAX_CODE_LINES: usize = 12;

/// The smallest legible text size on a frame whose short side is `short_side`.
pub fn floor(short_side: f32) -> f32 {
    short_side * FLOOR
}

/// A scale token on a canvas whose short side is `short_side` pixels, so a
/// 4K or portrait film keeps the same proportions.
pub fn fit(size: f32, short_side: f32) -> f32 {
    size * short_side / 1080.0
}

/// How long a line must stay on screen to be read: 0.4 s plus 0.2 s a word.
pub fn read_hold(text: &str) -> f32 {
    0.4 + 0.2 * text.split_whitespace().count() as f32
}

// The display sizes are at least three times the body size.
const _: () = assert!(HERO >= 3.0 * BODY && DISPLAY >= 3.0 * BODY);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_sit_above_the_floor() {
        assert!(LABEL > floor(1080.0));
        assert_eq!(floor(1080.0).round(), 28.0);
        assert_eq!(fit(HEADLINE, 2160.0), 2.0 * HEADLINE);
    }

    #[test]
    fn a_hold_grows_with_the_words() {
        assert!((read_hold("one") - 0.6).abs() < 1e-6);
        assert!((read_hold("the catalog owns vendor facts") - 1.4).abs() < 1e-6);
    }

    #[test]
    fn twelve_code_lines_fit_a_1080_frame() {
        let line = CODE * super::super::code::LINE_HEIGHT;
        assert!(line * MAX_CODE_LINES as f32 <= 1080.0 * 0.8);
    }
}
