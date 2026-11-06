//! Typefaces and custom font declarations (`Face`, `FontFacePlan`).
//!
//! `CommitMono` is bundled and deterministic everywhere; named presets select
//! distinct typographic voices (humanist, geometric, editorial serif, system
//! UI sans), and `ScenePlan.fonts` loads custom `.ttf`/`.otf` files per film.

use std::path::PathBuf;

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Face {
    /// Bundled CommitMono.
    #[default]
    Mono,
    /// Helvetica Neue Regular, for clean neo-grotesque copy.
    Sans,
    /// Helvetica Neue Bold, for neo-grotesque display type.
    SansBold,
    /// Didot, a high-contrast display serif.
    Serif,
    /// Didot Italic.
    SerifItalic,
    /// Helvetica Neue Light.
    Light,
    /// Helvetica Neue Condensed Black: compressed poster headlines.
    Shout,
    /// Platform UI sans (`.SF NS` / `System Font` / `Inter`).
    SystemSans,
    /// Geometric display sans (`Futura`).
    Geometric,
    /// Warm humanist sans (`Avenir Next`).
    Humanist,
    /// Classic book/editorial serif (`Baskerville` / `Georgia`).
    Editorial,
}

impl Face {
    pub fn is_mono(&self) -> bool {
        *self == Self::Mono
    }
}

/// Custom font file loaded into a `ScenePlan`'s font database before shaping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontFacePlan {
    /// CSS/OpenType family name used by `TypePlan::family`.
    pub family: String,
    /// Path to `.ttf`, `.otf`, or `.ttc` file (relative to the plan file).
    pub path: PathBuf,
}

impl FontFacePlan {
    pub fn new(family: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            family: family.into(),
            path: path.into(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.family.trim().is_empty() {
            bail!("font family name must not be empty");
        }
        ensure!(
            !self.path.as_os_str().is_empty(),
            "font '{}' path must not be empty",
            self.family
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Face, FontFacePlan};

    #[test]
    fn faces_serialize_by_kebab_name_and_default_to_mono() {
        assert_eq!(
            serde_json::to_value(Face::SerifItalic).unwrap(),
            "serif-italic"
        );
        assert_eq!(
            serde_json::from_value::<Face>("shout".into()).unwrap(),
            Face::Shout
        );
        assert_eq!(serde_json::to_value(Face::Geometric).unwrap(), "geometric");
        assert!(Face::default().is_mono());
        assert!(
            FontFacePlan::new("Clash Display", "fonts/ClashDisplay.ttf")
                .validate()
                .is_ok()
        );
    }
}
