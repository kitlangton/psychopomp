//! Output frame geometry, frame rate, and shutter angle (`CanvasPlan`).
//!
//! Omitting `canvas` in a `ScenePlan` defaults to `1920x1080 @ 60 fps` with a
//! `180°` shutter, preserving byte-identical JSON and pixels for existing plans
//! while unlocking `1080x1920` portrait, `1080x1080` square, `3840x2160` 4K,
//! and `24/30/60 fps` cadences.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const DEFAULT_WIDTH: u32 = 1920;
pub const DEFAULT_HEIGHT: u32 = 1080;
pub const DEFAULT_FPS: u32 = 60;
pub const DEFAULT_SHUTTER_ANGLE: f32 = 180.0;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanvasPlan {
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[serde(default = "default_shutter_angle")]
    pub shutter_angle: f32,
}

const fn default_width() -> u32 {
    DEFAULT_WIDTH
}
const fn default_height() -> u32 {
    DEFAULT_HEIGHT
}
const fn default_fps() -> u32 {
    DEFAULT_FPS
}
const fn default_shutter_angle() -> f32 {
    DEFAULT_SHUTTER_ANGLE
}

impl Default for CanvasPlan {
    fn default() -> Self {
        Self::landscape_1080p()
    }
}

impl CanvasPlan {
    pub const fn landscape_1080p() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: DEFAULT_FPS,
            shutter_angle: DEFAULT_SHUTTER_ANGLE,
        }
    }

    pub const fn portrait_1080p() -> Self {
        Self {
            width: 1080,
            height: 1920,
            fps: DEFAULT_FPS,
            shutter_angle: DEFAULT_SHUTTER_ANGLE,
        }
    }

    pub const fn square_1080p() -> Self {
        Self {
            width: 1080,
            height: 1080,
            fps: DEFAULT_FPS,
            shutter_angle: DEFAULT_SHUTTER_ANGLE,
        }
    }

    pub const fn uhd_4k() -> Self {
        Self {
            width: 3840,
            height: 2160,
            fps: DEFAULT_FPS,
            shutter_angle: DEFAULT_SHUTTER_ANGLE,
        }
    }

    pub const fn custom(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            fps: DEFAULT_FPS,
            shutter_angle: DEFAULT_SHUTTER_ANGLE,
        }
    }

    pub const fn fps(mut self, fps: u32) -> Self {
        self.fps = fps;
        self
    }

    pub const fn shutter_angle(mut self, degrees: f32) -> Self {
        self.shutter_angle = degrees;
        self
    }

    pub fn is_default(&self) -> bool {
        self.width == DEFAULT_WIDTH
            && self.height == DEFAULT_HEIGHT
            && self.fps == DEFAULT_FPS
            && (self.shutter_angle - DEFAULT_SHUTTER_ANGLE).abs() <= f32::EPSILON
    }

    pub const fn size(self) -> [u32; 2] {
        [self.width, self.height]
    }

    pub const fn center(self) -> [f32; 2] {
        [self.width as f32 * 0.5, self.height as f32 * 0.5]
    }

    pub const fn frame_bytes(self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    pub fn shutter_seconds(self) -> f64 {
        f64::from(self.shutter_angle.clamp(0.0, 360.0)) / 360.0 / f64::from(self.fps.max(1))
    }

    pub fn validate(&self) -> Result<()> {
        if !(64..=7680).contains(&self.width) || !self.width.is_multiple_of(2) {
            bail!(
                "canvas width {} must be an even integer in 64..=7680",
                self.width
            );
        }
        if !(64..=7680).contains(&self.height) || !self.height.is_multiple_of(2) {
            bail!(
                "canvas height {} must be an even integer in 64..=7680",
                self.height
            );
        }
        if !(1..=240).contains(&self.fps) {
            bail!("canvas fps {} must be in 1..=240", self.fps);
        }
        if !self.shutter_angle.is_finite() || !(0.0..=360.0).contains(&self.shutter_angle) {
            bail!(
                "canvas shutterAngle {} must be finite in 0..=360",
                self.shutter_angle
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_validate_and_default_round_trips() {
        assert!(CanvasPlan::default().is_default());
        for preset in [
            CanvasPlan::landscape_1080p(),
            CanvasPlan::portrait_1080p(),
            CanvasPlan::square_1080p(),
            CanvasPlan::uhd_4k().fps(24).shutter_angle(172.8),
        ] {
            preset.validate().unwrap();
            let json = serde_json::to_string(&preset).unwrap();
            let back: CanvasPlan = serde_json::from_str(&json).unwrap();
            assert_eq!(back, preset);
        }
        assert!(CanvasPlan::custom(1081, 1920).validate().is_err());
        assert!(CanvasPlan::landscape_1080p().fps(0).validate().is_err());
    }
}
