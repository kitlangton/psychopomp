//! Paint tokens shared by native presentation and file delivery. No geometry,
//! clocks, recorded pixels, or semantic state changes live in a theme.
use super::{TextSprite, fonts::ThemeFont};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Ref, RefCell},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Original,
    Evergreen,
    TokyoNight,
    Black,
    /// The OpenCode TUI's dark tokens (packages/tui theme `opencode`).
    #[serde(rename = "opencode")]
    OpenCode,
    /// The OpenCode blog's "clear neutral" diagrams: quiet frames and wires,
    /// warm ivory signals, and desaturated semantic inks used only for change.
    Neutral,
    /// A palette, status inks, card shadow, and optional face loaded from a
    /// theme file (`--theme path/to/theme.json`).
    #[serde(skip)]
    Custom(&'static CustomTheme),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub background: [u8; 3],
    pub surface: [u8; 3],
    pub raised: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub accent: [u8; 3],
    pub keyword: [u8; 3],
    pub types: [u8; 3],
    pub string: [u8; 3],
}

impl Theme {
    pub const ALL: [Self; 6] = [
        Self::Original,
        Self::Evergreen,
        Self::TokyoNight,
        Self::Black,
        Self::OpenCode,
        Self::Neutral,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Custom(custom) => &custom.name,
            Self::Original => "Original",
            Self::Evergreen => "Evergreen",
            Self::TokyoNight => "Tokyo Night",
            Self::Black => "Pure Black",
            Self::OpenCode => "OpenCode",
            Self::Neutral => "Clear Neutral",
        }
    }
    /// The next built-in theme. A theme file is not in the cycle; leaving it
    /// starts from the first (or, in reverse, the last) built-in.
    pub fn cycle(self, reverse: bool) -> Self {
        let Some(index) = Self::ALL.iter().position(|t| *t == self) else {
            return if reverse {
                Self::ALL[Self::ALL.len() - 1]
            } else {
                Self::ALL[0]
            };
        };
        Self::ALL[(index + if reverse { Self::ALL.len() - 1 } else { 1 }) % Self::ALL.len()]
    }
    /// A built-in theme by name, or a theme file by a path ending in `.json`.
    /// A theme file's font becomes this run's monospace face.
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        if value.ends_with(".json") {
            let custom: &'static CustomTheme =
                Box::leak(Box::new(CustomTheme::load(Path::new(value))?));
            if let Some(font) = &custom.font {
                super::fonts::use_theme_font(font)?;
            }
            return Ok(Self::Custom(custom));
        }
        match value {
            "original" => Ok(Self::Original),
            "evergreen" => Ok(Self::Evergreen),
            "tokyo-night" => Ok(Self::TokyoNight),
            "black" => Ok(Self::Black),
            "opencode" => Ok(Self::OpenCode),
            "neutral" => Ok(Self::Neutral),
            _ => anyhow::bail!(
                "unknown theme '{value}'; use original, evergreen, tokyo-night, black, opencode, neutral, or a theme file ending in .json"
            ),
        }
    }
    pub fn palette(self) -> Palette {
        match self {
            Self::Custom(custom) => custom.palette,
            Self::Original => Palette {
                background: [1, 2, 4],
                surface: [13, 18, 27],
                raised: [31, 36, 47],
                text: [235, 233, 227],
                muted: [143, 150, 165],
                accent: [216, 168, 120],
                keyword: [196, 181, 253],
                types: [125, 211, 252],
                string: [190, 242, 100],
            },
            Self::Evergreen => Palette {
                background: [13, 23, 20],
                surface: [22, 36, 30],
                raised: [33, 49, 41],
                text: [224, 233, 219],
                muted: [151, 173, 154],
                accent: [164, 196, 144],
                keyword: [214, 181, 140],
                types: [136, 193, 177],
                string: [180, 205, 144],
            },
            Self::TokyoNight => Palette {
                background: [26, 27, 38],
                surface: [31, 35, 53],
                raised: [41, 46, 66],
                text: [192, 202, 245],
                muted: [137, 149, 190],
                accent: [122, 162, 247],
                keyword: [187, 154, 247],
                types: [125, 207, 255],
                string: [158, 206, 106],
            },
            Self::Black => Palette {
                background: [0, 0, 0],
                surface: [10, 10, 10],
                raised: [23, 23, 23],
                text: [238, 238, 238],
                muted: [156, 156, 156],
                accent: [224, 174, 115],
                keyword: [207, 181, 237],
                types: [151, 207, 223],
                string: [179, 212, 151],
            },
            Self::OpenCode => Palette {
                background: [10, 10, 10],
                surface: [20, 20, 20],
                raised: [30, 30, 30],
                text: [238, 238, 238],
                muted: [128, 128, 128],
                accent: [250, 178, 131],
                keyword: [157, 124, 216],
                types: [229, 192, 123],
                string: [127, 216, 143],
            },
            Self::Neutral => Palette {
                background: [8, 8, 7],
                surface: [14, 14, 13],
                raised: [34, 34, 33],
                text: [226, 223, 217],
                muted: [133, 133, 133],
                accent: [224, 179, 90],
                keyword: [176, 160, 204],
                types: [214, 192, 146],
                string: [168, 186, 150],
            },
        }
    }
    /// A semantic tone's color. Status tones are identical in every built-in
    /// theme except Neutral, whose desaturated inks are reserved for change;
    /// a theme file names its own.
    pub fn tone(self, tone: psychopomp::tone::Tone) -> [u8; 3] {
        use psychopomp::tone::Tone;
        let palette = self.palette();
        if let Self::Custom(custom) = self {
            let tones = custom.tones;
            return match tone {
                Tone::Plain => palette.text,
                Tone::Request => tones.request,
                Tone::Success => tones.success,
                Tone::Error => tones.error,
                Tone::Warning => tones.warning,
                Tone::Muted => palette.muted,
                Tone::Accent => palette.accent,
            };
        }
        if self == Self::Neutral {
            return match tone {
                Tone::Plain => palette.text,
                Tone::Request => [236, 233, 228],
                Tone::Success => [165, 173, 147],
                Tone::Error => [237, 129, 126],
                Tone::Warning => [224, 179, 90],
                Tone::Muted => palette.muted,
                Tone::Accent => palette.accent,
            };
        }
        match tone {
            Tone::Plain => palette.text,
            Tone::Request => [92, 156, 245],
            Tone::Success => [127, 216, 143],
            Tone::Error => [224, 108, 117],
            Tone::Warning => [229, 192, 123],
            Tone::Muted => palette.muted,
            Tone::Accent => palette.accent,
        }
    }
    /// A card shadow's opacity on this theme's page. A theme file may soften
    /// shadows, which read as smudges on a light page.
    pub fn shadow(self, opacity: f32) -> f32 {
        match self {
            Self::Custom(custom) => opacity * custom.shadow,
            _ => opacity,
        }
    }
    pub fn background(self, original: [u8; 3]) -> [u8; 3] {
        if self == Self::Original {
            original
        } else {
            self.palette().background
        }
    }
    pub fn surface(self, original: [u8; 3]) -> [u8; 3] {
        if self == Self::Original {
            original
        } else {
            self.palette().surface
        }
    }
    /// Compatibility bridge for existing authored RGB typography. Known syntax
    /// colors retain their roles; neutral ink and the showroom accent use tokens.
    /// Other literal colors (including semantic red/green) are deliberately kept.
    pub fn ink(self, rgb: [u8; 3]) -> [u8; 3] {
        if self == Self::Original {
            return rgb;
        }
        let p = self.palette();
        match rgb {
            [196, 181, 253] => p.keyword,
            [125, 211, 252] => p.types,
            [190, 242, 100] => p.string,
            [110, 231, 183] | [216, 168, 120] | [214, 164, 112] => p.accent,
            _ => {
                let lo = *rgb.iter().min().unwrap();
                let hi = *rgb.iter().max().unwrap();
                if hi - lo <= 48 {
                    if hi >= 200 { p.text } else { p.muted }
                } else {
                    rgb
                }
            }
        }
    }
    pub(super) fn sprite(self, sprite: &mut TextSprite) {
        if self == Self::Original {
            return;
        }
        for p in sprite.pixels.as_chunks_mut::<4>().0 {
            if p[3] > 0 {
                let rgb = self.ink([p[0], p[1], p[2]]);
                p[..3].copy_from_slice(&rgb);
            }
        }
    }
}

/// A theme loaded from a JSON file; see `assets/themes/light.json`.
#[derive(Debug)]
pub struct CustomTheme {
    name: String,
    palette: Palette,
    tones: StatusTones,
    shadow: f32,
    font: Option<ThemeFont>,
}

#[derive(Clone, Copy, Debug)]
struct StatusTones {
    request: [u8; 3],
    success: [u8; 3],
    error: [u8; 3],
    warning: [u8; 3],
}

/// A loaded file is one theme: compared by identity, as only one is loaded per run.
impl PartialEq for CustomTheme {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for CustomTheme {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    name: Option<String>,
    background: Rgb,
    surface: Rgb,
    raised: Rgb,
    text: Rgb,
    muted: Rgb,
    accent: Rgb,
    keyword: Rgb,
    types: Rgb,
    string: Rgb,
    tones: ToneFile,
    shadow: Option<f32>,
    font: Option<FontFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToneFile {
    request: Rgb,
    success: Rgb,
    error: Rgb,
    warning: Rgb,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FontFile {
    family: String,
    #[serde(default)]
    files: Vec<PathBuf>,
}

/// An sRGB color written `#RRGGBB`.
#[derive(Clone, Copy)]
struct Rgb([u8; 3]);

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        parse_rgb(&value).map(Rgb).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "expected a color like \"#1A2B3C\", got \"{value}\""
            ))
        })
    }
}

fn parse_rgb(value: &str) -> Option<[u8; 3]> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

impl CustomTheme {
    /// Read and validate a theme file. Font files resolve beside it.
    pub fn load(path: &Path) -> Result<Self> {
        let bytes =
            std::fs::read(path).with_context(|| format!("read theme {}", path.display()))?;
        Self::from_json(&bytes, path).with_context(|| format!("invalid theme {}", path.display()))
    }

    fn from_json(bytes: &[u8], path: &Path) -> Result<Self> {
        let file: ThemeFile = serde_json::from_slice(bytes)?;
        let shadow = file.shadow.unwrap_or(1.0);
        ensure!(
            (0.0..=1.0).contains(&shadow),
            "shadow scales the default card shadow and must be from 0 to 1, got {shadow}"
        );
        let name = match file.name {
            Some(name) => name,
            None => path.file_stem().map_or_else(
                || "Custom".to_owned(),
                |stem| stem.to_string_lossy().into_owned(),
            ),
        };
        let font = file
            .font
            .map(|font| {
                ThemeFont::load(
                    font.family,
                    &font.files,
                    path.parent().unwrap_or(Path::new(".")),
                )
            })
            .transpose()?;
        Ok(Self {
            name,
            palette: Palette {
                background: file.background.0,
                surface: file.surface.0,
                raised: file.raised.0,
                text: file.text.0,
                muted: file.muted.0,
                accent: file.accent.0,
                keyword: file.keyword.0,
                types: file.types.0,
                string: file.string.0,
            },
            tones: StatusTones {
                request: file.tones.request.0,
                success: file.tones.success.0,
                error: file.tones.error.0,
                warning: file.tones.warning.0,
            },
            shadow,
            font,
        })
    }
}

/// Prepared glyphs recolored for the last theme they were drawn with.
pub(super) struct ThemedCache<T>(RefCell<Option<(Theme, T)>>);

impl<T> Default for ThemedCache<T> {
    fn default() -> Self {
        Self(RefCell::new(None))
    }
}

impl<T> ThemedCache<T> {
    /// The value for `theme`, rebuilt by `make` only when the theme changed.
    pub(super) fn get(&self, theme: Theme, make: impl FnOnce() -> T) -> Ref<'_, T> {
        if self
            .0
            .borrow()
            .as_ref()
            .is_none_or(|(cached, _)| *cached != theme)
        {
            *self.0.borrow_mut() = Some((theme, make()));
        }
        Ref::map(self.0.borrow(), |cached| &cached.as_ref().unwrap().1)
    }
}

/// Byte colour from `from` at 0 to `to` at 1, `t` clamped to that range.
pub(super) fn mix<const N: usize>(from: [u8; N], to: [u8; N], t: f32) -> [u8; N] {
    let t = t.clamp(0.0, 1.0);
    std::array::from_fn(|i| {
        psychopomp::math::lerp(f32::from(from[i]), f32::from(to[i]), t).round() as u8
    })
}

pub(super) fn linear(rgb: [u8; 3]) -> [f32; 3] {
    rgb.map(srgb_to_linear)
}

pub(super) fn srgb_to_linear(byte: u8) -> f32 {
    let c = f32::from(byte) / 255.;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn themes_cycle_round_trip_and_keep_semantic_literals() {
        for theme in Theme::ALL {
            assert_eq!(theme.cycle(false).cycle(true), theme);
            assert_eq!(
                serde_json::from_str::<Theme>(&serde_json::to_string(&theme).unwrap()).unwrap(),
                theme
            );
            assert_eq!(theme.ink([239, 68, 68]), [239, 68, 68]);
        }
        assert_eq!(Theme::Black.palette().background, [0; 3]);
        assert_eq!(Theme::parse("opencode").unwrap(), Theme::OpenCode);
        assert_eq!(Theme::OpenCode.palette().accent, [250, 178, 131]);
        assert!(Theme::parse("missing").is_err());
        assert_eq!(Theme::parse("neutral").unwrap(), Theme::Neutral);
        assert_eq!(
            Theme::OpenCode.tone(psychopomp::tone::Tone::Success),
            [127, 216, 143],
            "existing themes keep their status inks"
        );
    }

    const LIGHT: &str = r##"{
        "name": "Paper",
        "background": "#F9FAFA", "surface": "#FFFFFF", "raised": "#DADDE3",
        "text": "#242525", "muted": "#72767E", "accent": "#418CFF",
        "keyword": "#6667CD", "types": "#2E76E0", "string": "#308864",
        "tones": { "request": "#2E76E0", "success": "#308864", "error": "#C64044", "warning": "#B3821F" },
        "shadow": 0.3
    }"##;

    fn custom(json: &str) -> Result<CustomTheme> {
        CustomTheme::from_json(json.as_bytes(), Path::new("themes/paper.json"))
    }

    #[test]
    fn a_theme_file_paints_its_palette_tones_and_shadow() {
        use psychopomp::tone::Tone;
        let theme = Theme::Custom(Box::leak(Box::new(custom(LIGHT).unwrap())));
        assert_eq!(theme.name(), "Paper");
        assert_eq!(theme.palette().background, [0xF9, 0xFA, 0xFA]);
        assert_eq!(theme.background([1, 2, 3]), [0xF9, 0xFA, 0xFA]);
        assert_eq!(theme.tone(Tone::Error), [0xC6, 0x40, 0x44]);
        assert_eq!(theme.tone(Tone::Accent), [0x41, 0x8C, 0xFF]);
        assert_eq!(theme.tone(Tone::Plain), [0x24, 0x25, 0x25]);
        assert_eq!(theme.shadow(0.5), 0.5 * 0.3);
        // Authored light-on-dark ink takes the theme's text color.
        assert_eq!(theme.ink([235, 233, 227]), [0x24, 0x25, 0x25]);
        assert_eq!(theme.cycle(false), Theme::Original);
        assert_eq!(theme.cycle(true), Theme::Neutral);
        for builtin in Theme::ALL {
            assert_eq!(builtin.shadow(0.55), 0.55, "built-in shadows are unchanged");
            assert_ne!(builtin, theme);
        }
    }

    #[test]
    fn a_theme_file_defaults_its_name_and_shadow() {
        let json = LIGHT
            .replace(r#""name": "Paper","#, "")
            .replace(",\n        \"shadow\": 0.3", "");
        let theme = custom(&json).unwrap();
        assert_eq!(theme.name, "paper", "the file's stem");
        assert_eq!(theme.shadow, 1.0, "the default card shadow");
    }

    #[test]
    fn invalid_theme_files_say_what_is_wrong() {
        let error = |json: &str| format!("{:#}", custom(json).unwrap_err());
        assert!(
            error(&LIGHT.replace("#418CFF", "blue"))
                .contains(r##"expected a color like "#1A2B3C", got "blue""##)
        );
        assert!(error(&LIGHT.replace("#418CFF", "#418CF")).contains("got \"#418CF\""));
        assert!(
            error(&LIGHT.replace("\"raised\"", "\"border\"")).contains("unknown field `border`")
        );
        assert!(
            error(&LIGHT.replace(r##", "warning": "#B3821F""##, ""))
                .contains("missing field `warning`")
        );
        assert!(error(&LIGHT.replace("0.3", "1.5")).contains("must be from 0 to 1, got 1.5"));
        assert!(
            error(&LIGHT.replace("0.3", r#"0.3, "font": { "family": "No Such Face" }"#))
                .contains("font family 'No Such Face' is not installed")
        );
        let missing = Theme::parse("missing/theme.json").unwrap_err();
        assert!(format!("{missing:#}").contains("read theme missing/theme.json"));
    }
}
