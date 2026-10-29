//! First-class kinetic typography (`TypePlan`, `SplitUnit`) for display titles,
//! editorial layouts, masked baseline word/line rises, character staggers, and
//! tracking sweeps.

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{caption::CaptionAlign, face::Face, paint::Paint, tone::Tone};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SplitUnit {
    #[default]
    Block,
    Line,
    Word,
    Char,
}

impl SplitUnit {
    pub fn is_default(&self) -> bool {
        *self == Self::Block
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeUnit {
    pub index: usize,
    pub line: usize,
    pub leading_spaces: usize,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TypePlan {
    pub text: String,
    #[serde(default, skip_serializing_if = "Face::is_mono")]
    pub face: Face,
    /// Optional explicit font-family name (such as a custom `ScenePlan.fonts` entry
    /// or installed display family), overriding `face`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    pub size: f32,
    #[serde(default = "default_weight", skip_serializing_if = "is_default_weight")]
    pub weight: u16,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    /// Base tracking in `em` (`-0.08..=0.6`, e.g. `-0.04` for tight display type).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tracking: f32,
    /// Line-height ratio (`0.75..=3.0`, e.g. `0.92` for stacked poster type).
    #[serde(default = "default_line_height", skip_serializing_if = "is_default_lh")]
    pub line_height: f32,
    #[serde(default, skip_serializing_if = "CaptionAlign::is_default")]
    pub align: CaptionAlign,
    #[serde(default)]
    pub paint: Paint,
    #[serde(default, skip_serializing_if = "SplitUnit::is_default")]
    pub split: SplitUnit,
    /// Clip each line to its stationary vertical line box so rising words/chars
    /// emerge from an invisible baseline slit without fading.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub masked: bool,
    /// Optional soft background halo behind glyphs (off by default so display
    /// typography stays razor-sharp on light, gradient, or photographic plates).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub backing: bool,
}

const fn default_weight() -> u16 {
    400
}
fn is_default_weight(w: &u16) -> bool {
    *w == 400
}
const fn default_line_height() -> f32 {
    1.15
}
fn is_default_lh(v: &f32) -> bool {
    (*v - 1.15).abs() <= 1e-5
}
fn is_zero(v: &f32) -> bool {
    v.abs() <= f32::EPSILON
}

pub const TYPE_BASE_DEFAULTS: [(&str, f32); 8] = [
    ("opacity", 1.0),
    ("x", 0.0),
    ("y", 0.0),
    ("z", 0.0),
    ("scale", 1.0),
    ("rotation", 0.0),
    ("blur", 0.0),
    ("tracking", 0.0),
];

pub const TYPE_UNIT_DEFAULTS: [(&str, f32); 6] = [
    ("opacity", 1.0),
    ("x", 0.0),
    ("y", 0.0),
    ("scale", 1.0),
    ("rotation", 0.0),
    ("blur", 0.0),
];

impl TypePlan {
    pub fn new(text: impl Into<String>, face: Face, size: f32) -> Self {
        Self {
            text: text.into(),
            face,
            family: None,
            size,
            weight: 400,
            italic: false,
            tracking: 0.0,
            line_height: 1.15,
            align: CaptionAlign::Left,
            paint: Paint::tone(Tone::Plain),
            split: SplitUnit::Block,
            masked: false,
            backing: false,
        }
    }

    pub fn family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }

    pub const fn weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    pub const fn bold(mut self) -> Self {
        self.weight = 700;
        self
    }

    pub const fn black(mut self) -> Self {
        self.weight = 900;
        self
    }

    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub const fn tracking(mut self, em: f32) -> Self {
        self.tracking = em;
        self
    }

    pub const fn line_height(mut self, ratio: f32) -> Self {
        self.line_height = ratio;
        self
    }

    pub const fn align(mut self, align: CaptionAlign) -> Self {
        self.align = align;
        self
    }

    pub const fn paint(mut self, paint: Paint) -> Self {
        self.paint = paint;
        self
    }

    pub const fn split(mut self, split: SplitUnit) -> Self {
        self.split = split;
        self
    }

    pub const fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    pub const fn backing(mut self, backing: bool) -> Self {
        self.backing = backing;
        self
    }

    /// Deterministic decomposition of `text` into animatable `TypeUnit`s (`u0..u{N-1}`).
    pub fn units(&self) -> Vec<TypeUnit> {
        let mut units = Vec::new();
        match self.split {
            SplitUnit::Block => {
                for (line_idx, line) in self.text.split('\n').enumerate() {
                    if !line.is_empty() {
                        let index = units.len();
                        units.push(TypeUnit {
                            index,
                            line: line_idx,
                            leading_spaces: 0,
                            text: line.to_owned(),
                        });
                    }
                }
            }
            SplitUnit::Line => {
                for (line_idx, line) in self.text.split('\n').enumerate() {
                    if !line.is_empty() {
                        let index = units.len();
                        units.push(TypeUnit {
                            index,
                            line: line_idx,
                            leading_spaces: 0,
                            text: line.to_owned(),
                        });
                    }
                }
            }
            SplitUnit::Word => {
                for (line_idx, line) in self.text.split('\n').enumerate() {
                    let mut chars = line.chars().peekable();
                    while chars.peek().is_some() {
                        let mut spaces = 0_usize;
                        while chars.next_if(|c| c.is_whitespace()).is_some() {
                            spaces += 1;
                        }
                        let mut word = String::new();
                        while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
                            word.push(c);
                        }
                        if !word.is_empty() {
                            let index = units.len();
                            units.push(TypeUnit {
                                index,
                                line: line_idx,
                                leading_spaces: if units.last().is_some_and(|u| u.line == line_idx)
                                {
                                    spaces.max(1)
                                } else {
                                    spaces
                                },
                                text: word,
                            });
                        }
                    }
                }
            }
            SplitUnit::Char => {
                for (line_idx, line) in self.text.split('\n').enumerate() {
                    let mut spaces = 0_usize;
                    for ch in line.chars() {
                        if ch.is_whitespace() {
                            spaces += 1;
                            continue;
                        }
                        let index = units.len();
                        units.push(TypeUnit {
                            index,
                            line: line_idx,
                            leading_spaces: spaces,
                            text: ch.to_string(),
                        });
                        spaces = 0;
                    }
                }
            }
        }
        units
    }

    pub fn unit_count(&self) -> usize {
        match self.split {
            SplitUnit::Block => 1,
            _ => self.units().len(),
        }
    }

    pub fn property_default(&self, property: &str) -> Option<f32> {
        if let Some(&(_, v)) = TYPE_BASE_DEFAULTS.iter().find(|(k, _)| *k == property) {
            return Some(v);
        }
        let rest = property.strip_prefix('u')?;
        let (idx_str, sub) = rest.split_once('.')?;
        let idx: usize = idx_str.parse().ok()?;
        if idx >= self.unit_count() {
            return None;
        }
        TYPE_UNIT_DEFAULTS
            .iter()
            .find(|(k, _)| *k == sub)
            .map(|&(_, v)| v)
    }

    pub fn accepts_property(&self, property: &str) -> bool {
        self.property_default(property).is_some()
    }

    pub fn validate(&self, id: &str) -> Result<()> {
        if self.text.trim().is_empty() {
            bail!("type '{id}' text must not be empty");
        }
        ensure!(
            self.size.is_finite() && (8.0..=720.0).contains(&self.size),
            "type '{id}' size {} must be in 8..=720",
            self.size
        );
        ensure!(
            (100..=1000).contains(&self.weight),
            "type '{id}' weight {} must be in 100..=1000",
            self.weight
        );
        ensure!(
            self.tracking.is_finite() && (-0.25..=1.5).contains(&self.tracking),
            "type '{id}' tracking {} must be in -0.25..=1.5 em",
            self.tracking
        );
        ensure!(
            self.line_height.is_finite() && (0.6..=4.0).contains(&self.line_height),
            "type '{id}' lineHeight {} must be in 0.6..=4.0",
            self.line_height
        );
        if let Some(fam) = &self.family {
            ensure!(
                !fam.trim().is_empty(),
                "type '{id}' family must not be empty"
            );
        }
        ensure!(
            self.unit_count() <= 256,
            "type '{id}' splits into {} units (limit 256)",
            self.unit_count()
        );
        self.paint.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words_and_chars_and_resolves_unit_defaults() {
        let spec = TypePlan::new("MOTION\nWITHOUT BOXES", Face::SansBold, 120.0)
            .tracking(-0.04)
            .line_height(0.92)
            .split(SplitUnit::Word)
            .masked(true);
        spec.validate("hero").unwrap();
        let units = spec.units();
        assert_eq!(units.len(), 3);
        assert_eq!((units[0].line, units[0].text.as_str()), (0, "MOTION"));
        assert_eq!((units[1].line, units[1].text.as_str()), (1, "WITHOUT"));
        assert_eq!(
            (
                units[2].line,
                units[2].leading_spaces,
                units[2].text.as_str()
            ),
            (1, 1, "BOXES")
        );
        assert_eq!(spec.property_default("tracking"), Some(0.0));
        assert_eq!(spec.property_default("u0.y"), Some(0.0));
        assert_eq!(spec.property_default("u2.opacity"), Some(1.0));
        assert_eq!(spec.property_default("u3.y"), None);
    }
}
