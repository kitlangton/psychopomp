//! The font database. CommitMono is compiled in, so every machine shapes the
//! same text with the same weights; installed fonts only supply glyphs it lacks
//! (chess pieces, CJK, emoji). See `assets/fonts/OFL.txt`. A theme file may
//! name another face for monospace text; it is chosen once per run, so theme
//! changes never change glyph metrics.
use std::{path::Path, sync::OnceLock};

use anyhow::{Context, Result, ensure};
use cosmic_text::{Attrs, Family, FontSystem, Stretch, Style, Weight, fontdb};
use psychopomp::face::Face;

/// The bundled monospace family used for code and labels.
const MONO: Family<'static> = Family::Name("CommitMono");
/// The installed sans family used for prose and headers.
pub(crate) const SANS: Family<'static> = Family::Name("Helvetica Neue");
/// The installed display serif.
const SERIF: Family<'static> = Family::Name("Didot");

/// A face a theme file supplies in place of CommitMono: a family name and the
/// font files that provide it, or none when the family is installed.
#[derive(Debug)]
pub(crate) struct ThemeFont {
    family: String,
    data: Vec<Vec<u8>>,
}

impl ThemeFont {
    /// Read `files` (relative to `base`) and check that they, or the installed
    /// fonts when there are none, provide `family`.
    pub(crate) fn load(family: String, files: &[impl AsRef<Path>], base: &Path) -> Result<Self> {
        ensure!(!family.trim().is_empty(), "font family must not be empty");
        let data = files
            .iter()
            .map(|file| {
                let path = base.join(file);
                std::fs::read(&path).with_context(|| format!("read font {}", path.display()))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut db = fontdb::Database::new();
        if data.is_empty() {
            db.load_system_fonts();
        }
        for font in &data {
            db.load_font_data(font.clone());
        }
        let provided = |name: &str| {
            db.faces()
                .any(|face| face.families.iter().any(|(f, _)| f == name))
        };
        if !provided(&family) {
            let mut families = db
                .faces()
                .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
                .collect::<Vec<_>>();
            families.sort();
            families.dedup();
            anyhow::bail!(if data.is_empty() {
                format!("font family '{family}' is not installed; list its font files in `files`")
            } else {
                format!(
                    "font family '{family}' is not in its font files, which provide: {}",
                    families.join(", ")
                )
            });
        }
        Ok(Self { family, data })
    }
}

static THEME_FONT: OnceLock<&'static ThemeFont> = OnceLock::new();

/// Use `font` for monospace text in every font system made after this call.
pub(crate) fn use_theme_font(font: &'static ThemeFont) -> Result<()> {
    let current = *THEME_FONT.get_or_init(|| font);
    ensure!(
        std::ptr::eq(current, font),
        "a run uses one theme font; '{}' is already selected",
        current.family
    );
    Ok(())
}

/// The family for monospace text: CommitMono unless a theme file named another.
pub(crate) fn mono() -> Family<'static> {
    THEME_FONT
        .get()
        .map_or(MONO, |font| Family::Name(&font.family))
}
/// Attributes that select `face`.
pub(crate) fn attrs(face: Face) -> Attrs<'static> {
    let attrs = Attrs::new();
    match face {
        Face::Mono => attrs.family(mono()),
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

/// A font system whose CommitMono faces are exactly the bundled ones, plus the
/// theme font's faces when a theme file chose one.
pub(crate) fn font_system() -> FontSystem {
    font_system_with(THEME_FONT.get().copied())
}

fn font_system_with(theme_font: Option<&ThemeFont>) -> FontSystem {
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
    for font in theme_font.iter().flat_map(|font| &font.data) {
        db.load_font_data(font.clone());
    }
    FontSystem::new_with_locale_and_db("en-US".to_owned(), db)
}

#[cfg(test)]
mod tests {
    use cosmic_text::fontdb::{Query, Source, Stretch, Style, Weight};
    use std::path::Path;

    use super::ThemeFont;

    const BUNDLED: &str = "../../assets/fonts";

    #[test]
    fn a_theme_font_is_loaded_beside_commit_mono() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(BUNDLED);
        // Any face works; the bundled one keeps the test independent of the machine.
        let font =
            ThemeFont::load("CommitMono".into(), &["CommitMono-700-Regular.otf"], &base).unwrap();
        let fonts = super::font_system_with(Some(&font));
        let faces = fonts
            .db()
            .faces()
            .filter(|face| face.families.iter().any(|(name, _)| name == "CommitMono"))
            .count();
        assert_eq!(faces, 5, "the four bundled faces and the theme's");
    }

    #[test]
    fn a_theme_font_must_provide_its_family() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join(BUNDLED);
        let error =
            ThemeFont::load("Inter".into(), &["CommitMono-400-Regular.otf"], &base).unwrap_err();
        assert_eq!(
            error.to_string(),
            "font family 'Inter' is not in its font files, which provide: CommitMono"
        );
        let error = ThemeFont::load("Inter".into(), &["missing.ttf"], &base).unwrap_err();
        assert!(error.to_string().starts_with("read font "));
        assert!(ThemeFont::load(" ".into(), &[] as &[&str], &base).is_err());
        assert_eq!(super::mono(), super::MONO, "no theme file chose a font");
    }

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
}
