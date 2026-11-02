//! General motion-graphics `Paint` (solid sRGB/hex, linear & radial gradients,
//! or semantic `Tone`) and per-scene `PalettePlan` color systems (including
//! light paper, Swiss poster, daylight glass, and custom brand palettes).

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::tone::Tone;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Paint {
    Tone {
        tone: Tone,
        #[serde(default = "default_alpha", skip_serializing_if = "is_unit_alpha")]
        alpha: f32,
    },
    Solid {
        rgba: [u8; 4],
    },
    #[serde(rename_all = "camelCase")]
    Linear {
        angle_deg: f32,
        from: [u8; 4],
        to: [u8; 4],
    },
    #[serde(rename_all = "camelCase")]
    Radial {
        #[serde(default = "default_center")]
        center: [f32; 2],
        #[serde(default = "default_radius")]
        radius: f32,
        inner: [u8; 4],
        outer: [u8; 4],
    },
}

const fn default_alpha() -> f32 {
    1.0
}
fn is_unit_alpha(v: &f32) -> bool {
    (*v - 1.0).abs() <= f32::EPSILON
}
const fn default_center() -> [f32; 2] {
    [0.5, 0.5]
}
const fn default_radius() -> f32 {
    0.75
}

impl Default for Paint {
    fn default() -> Self {
        Self::tone(Tone::Plain)
    }
}

impl Paint {
    pub const fn tone(tone: Tone) -> Self {
        Self::Tone { tone, alpha: 1.0 }
    }

    pub const fn tone_alpha(tone: Tone, alpha: f32) -> Self {
        Self::Tone { tone, alpha }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Solid {
            rgba: [r, g, b, 255],
        }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::Solid { rgba: [r, g, b, a] }
    }

    /// Parse `#RGB`, `#RRGGBB`, or `#RRGGBBAA` (leading `#` optional). Panics on invalid hex.
    pub fn hex(hex: &str) -> Self {
        Self::Solid {
            rgba: parse_hex_rgba(hex)
                .unwrap_or_else(|err| panic!("invalid hex color '{hex}': {err}")),
        }
    }

    pub const fn linear(angle_deg: f32, from: [u8; 4], to: [u8; 4]) -> Self {
        Self::Linear {
            angle_deg,
            from,
            to,
        }
    }

    pub fn linear_hex(angle_deg: f32, from: &str, to: &str) -> Self {
        Self::Linear {
            angle_deg,
            from: parse_hex_rgba(from).unwrap_or_else(|e| panic!("invalid hex '{from}': {e}")),
            to: parse_hex_rgba(to).unwrap_or_else(|e| panic!("invalid hex '{to}': {e}")),
        }
    }

    pub const fn radial(inner: [u8; 4], outer: [u8; 4]) -> Self {
        Self::Radial {
            center: [0.5, 0.5],
            radius: 0.75,
            inner,
            outer,
        }
    }

    pub fn radial_hex(center: [f32; 2], radius: f32, inner: &str, outer: &str) -> Self {
        Self::Radial {
            center,
            radius,
            inner: parse_hex_rgba(inner).unwrap_or_else(|e| panic!("invalid hex '{inner}': {e}")),
            outer: parse_hex_rgba(outer).unwrap_or_else(|e| panic!("invalid hex '{outer}': {e}")),
        }
    }

    /// Representative primary sRGB `[r, g, b]` for flat surfaces or fallback ink.
    pub fn primary_rgb(self, resolve_tone: impl FnOnce(Tone) -> [u8; 3]) -> [u8; 3] {
        match self {
            Self::Tone { tone, .. } => resolve_tone(tone),
            Self::Solid { rgba: [r, g, b, _] }
            | Self::Linear {
                from: [r, g, b, _], ..
            }
            | Self::Radial {
                inner: [r, g, b, _],
                ..
            } => [r, g, b],
        }
    }

    pub fn validate(&self) -> Result<()> {
        match *self {
            Self::Tone { alpha, .. } => {
                ensure!(
                    alpha.is_finite() && (0.0..=1.0).contains(&alpha),
                    "paint alpha must be in 0..=1"
                );
            }
            Self::Solid { .. } => {}
            Self::Linear { angle_deg, .. } => {
                ensure!(
                    angle_deg.is_finite(),
                    "linear gradient angle must be finite"
                );
            }
            Self::Radial { center, radius, .. } => {
                ensure!(
                    center[0].is_finite()
                        && center[1].is_finite()
                        && radius.is_finite()
                        && radius > 0.0,
                    "radial gradient center must be finite and radius positive"
                );
            }
        }
        Ok(())
    }
}

pub fn parse_hex_rgba(input: &str) -> Result<[u8; 4]> {
    let s = input.trim().strip_prefix('#').unwrap_or(input.trim());
    let byte = |slice: &str| -> Result<u8> {
        u8::from_str_radix(slice, 16).map_err(|_| anyhow::anyhow!("bad hex digits '{slice}'"))
    };
    match s.len() {
        3 => {
            let r = byte(&s[0..1])? * 17;
            let g = byte(&s[1..2])? * 17;
            let b = byte(&s[2..3])? * 17;
            Ok([r, g, b, 255])
        }
        6 => Ok([byte(&s[0..2])?, byte(&s[2..4])?, byte(&s[4..6])?, 255]),
        8 => Ok([
            byte(&s[0..2])?,
            byte(&s[2..4])?,
            byte(&s[4..6])?,
            byte(&s[6..8])?,
        ]),
        _ => bail!("expected 3, 6, or 8 hex digits"),
    }
}

pub fn parse_hex_rgb(input: &str) -> Result<[u8; 3]> {
    let [r, g, b, _] = parse_hex_rgba(input)?;
    Ok([r, g, b])
}

/// Authored color system for a `ScenePlan`, decoupling motion graphics from the
/// built-in dark code-editor themes.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PalettePlan {
    pub background: [u8; 3],
    pub surface: [u8; 3],
    pub raised: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub accent: [u8; 3],
    pub request: [u8; 3],
    pub success: [u8; 3],
    pub warning: [u8; 3],
    pub error: [u8; 3],
}

impl PalettePlan {
    /// Warm archival cream paper with rich carbon ink, vermilion, and Prussian blue.
    pub const fn paper() -> Self {
        Self {
            background: [244, 239, 230], // #F4EFE6
            surface: [235, 228, 215],    // #EBE4D7
            raised: [222, 213, 197],     // #DED5C5
            text: [22, 21, 20],          // #161514
            muted: [110, 105, 97],       // #6E6961
            accent: [217, 56, 30],       // #D9381E vermilion
            request: [29, 78, 137],      // #1D4E89 Prussian blue
            success: [46, 111, 64],      // #2E6F40 forest green
            warning: [189, 123, 21],     // #BD7B15 ochre
            error: [196, 43, 35],        // #C42B23 crimson
        }
    }

    /// High-contrast Swiss International poster: chalk ground, jet ink, signal orange-red.
    pub const fn swiss_poster() -> Self {
        Self {
            background: [247, 245, 240], // #F7F5F0
            surface: [236, 233, 225],    // #ECE9E1
            raised: [216, 212, 202],     // #D8D4CA
            text: [17, 17, 16],          // #111110
            muted: [102, 100, 95],       // #66645F
            accent: [255, 59, 29],       // #FF3B1D international orange
            request: [0, 71, 171],       // #0047AB cobalt
            success: [18, 128, 72],      // #128048
            warning: [224, 142, 0],      // #E08E00
            error: [217, 30, 24],        // #D91E18
        }
    }

    /// Bright Apple-style daylight glass & studio white.
    pub const fn light_glass() -> Self {
        Self {
            background: [246, 247, 250], // #F6F7FA
            surface: [255, 255, 255],    // #FFFFFF
            raised: [232, 235, 242],     // #E8EBF2
            text: [17, 19, 24],          // #111318
            muted: [108, 115, 128],      // #6C7380
            accent: [0, 102, 204],       // #0066CC
            request: [52, 120, 246],     // #3478F6
            success: [40, 167, 69],      // #28A745
            warning: [217, 130, 15],     // #D9820F
            error: [224, 52, 68],        // #E03444
        }
    }

    /// Deep obsidian studio palette with warm gold accent.
    pub const fn obsidian() -> Self {
        Self {
            background: [8, 8, 7],
            surface: [16, 16, 15],
            raised: [34, 34, 32],
            text: [232, 229, 223],
            muted: [136, 134, 129],
            accent: [228, 181, 92],
            request: [110, 168, 245],
            success: [142, 196, 148],
            warning: [230, 176, 88],
            error: [238, 118, 114],
        }
    }

    pub const fn tone(&self, tone: Tone) -> [u8; 3] {
        match tone {
            Tone::Plain => self.text,
            Tone::Muted => self.muted,
            Tone::Accent => self.accent,
            Tone::Request => self.request,
            Tone::Success => self.success,
            Tone::Warning => self.warning,
            Tone::Error => self.error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_validates_gradients() {
        assert_eq!(Paint::hex("#F4EFE6"), Paint::rgb(244, 239, 230));
        assert_eq!(Paint::hex("f08"), Paint::rgb(255, 0, 136));
        assert_eq!(Paint::hex("#11223380"), Paint::rgba(0x11, 0x22, 0x33, 0x80));

        let g = Paint::linear_hex(135.0, "#FF3B1D", "#0047AB");
        g.validate().unwrap();
        let json = serde_json::to_string(&g).unwrap();
        let round: Paint = serde_json::from_str(&json).unwrap();
        assert_eq!(round, g);

        assert!(
            Paint::radial([255, 255, 255, 255], [0, 0, 0, 255])
                .validate()
                .is_ok()
        );
        assert!(
            Paint::Radial {
                center: [0.5, 0.5],
                radius: 0.0,
                inner: [0; 4],
                outer: [0; 4],
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn paper_palette_maps_semantic_tones_to_high_contrast_inks() {
        let p = PalettePlan::paper();
        assert_eq!(p.tone(Tone::Plain), [22, 21, 20]);
        assert_eq!(p.tone(Tone::Accent), [217, 56, 30]);
    }
}
