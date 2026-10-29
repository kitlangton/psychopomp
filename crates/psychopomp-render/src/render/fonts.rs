//! The font database. CommitMono is compiled in, so every machine shapes the
//! same text with the same weights; installed fonts only supply glyphs it lacks
//! (chess pieces, CJK, emoji). See `assets/fonts/OFL.txt`.
use std::sync::LazyLock;

use cosmic_text::{Attrs, Family, FontSystem, Stretch, Style, Weight, fontdb};
use psychopomp::face::Face;

/// The bundled monospace family used for code and labels.
pub(crate) const MONO: Family<'static> = Family::Name("CommitMono");
/// The installed sans family used for prose and headers.
pub(crate) const SANS: Family<'static> = Family::Name("Helvetica Neue");
/// The installed display serif.
const SERIF: Family<'static> = Family::Name("Didot");

/// Attributes that select `face`.
pub(crate) fn attrs(face: Face) -> Attrs<'static> {
    let attrs = Attrs::new();
    match face {
        Face::Mono => attrs.family(MONO),
        Face::Sans => attrs.family(SANS),
        Face::SansBold => attrs.family(SANS).weight(Weight::BOLD),
        Face::Serif => attrs.family(SERIF),
        Face::SerifItalic => attrs.family(SERIF).style(Style::Italic),
        Face::Light => attrs.family(SANS).weight(Weight::LIGHT),
        Face::Shout => attrs
            .family(SANS)
            .stretch(Stretch::Condensed)
            .weight(Weight::BLACK),
    }
}

const COMMIT_MONO: [&[u8]; 4] = [
    include_bytes!("../../../../assets/fonts/CommitMono-400-Regular.otf"),
    include_bytes!("../../../../assets/fonts/CommitMono-400-Italic.otf"),
    include_bytes!("../../../../assets/fonts/CommitMono-700-Regular.otf"),
    include_bytes!("../../../../assets/fonts/CommitMono-700-Italic.otf"),
];

static FONT_DB: LazyLock<fontdb::Database> = LazyLock::new(|| {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    // An installed CommitMono (any version or variant) must never win a match.
    let installed = db
        .faces()
        .filter(|face| {
            face.families.iter().any(|(name, _)| {
                name.replace(' ', "")
                    .to_ascii_lowercase()
                    .starts_with("commitmono")
            })
        })
        .map(|face| face.id)
        .collect::<Vec<_>>();
    for id in installed {
        db.remove_face(id);
    }
    for font in COMMIT_MONO {
        db.load_font_data(font.to_vec());
    }
    db.set_monospace_family("CommitMono");
    db
});

/// A font system whose CommitMono faces are exactly the bundled ones.
pub(crate) fn font_system() -> FontSystem {
    FontSystem::new_with_locale_and_db("en-US".to_owned(), FONT_DB.clone())
}

#[cfg(test)]
mod tests {
    use cosmic_text::fontdb::{Query, Source, Stretch, Style, Weight};

    #[test]
    fn every_commit_mono_style_resolves_to_a_bundled_face() {
        let fonts = super::font_system();
        let db = fonts.db();
        let bundled = db
            .faces()
            .filter(|face| face.families.iter().any(|(name, _)| name == "CommitMono"))
            .collect::<Vec<_>>();
        assert_eq!(bundled.len(), 4, "exactly the four bundled faces");
        assert!(
            bundled
                .iter()
                .all(|face| matches!(face.source, Source::Binary(_)))
        );
        for (weight, style) in [
            (Weight::NORMAL, Style::Normal),
            (Weight::SEMIBOLD, Style::Normal),
            (Weight::BOLD, Style::Italic),
        ] {
            let id = db
                .query(&Query {
                    families: &[super::MONO],
                    weight,
                    stretch: Stretch::Normal,
                    style,
                })
                .expect("a CommitMono match");
            let face = db.face(id).unwrap();
            assert!(matches!(face.source, Source::Binary(_)));
            assert_eq!(face.style, style);
        }
    }

    #[test]
    fn cached_font_db_clones_without_rescanning_system_fonts() {
        let first = super::font_system();
        let t0 = std::time::Instant::now();
        let second = super::font_system();
        assert_eq!(first.db().len(), second.db().len());
        assert!(
            t0.elapsed().as_millis() < 25,
            "second font_system() should clone cached FONT_DB quickly"
        );
    }
}
