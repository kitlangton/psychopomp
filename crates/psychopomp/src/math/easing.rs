//! Easing curves over `t` in 0..1, named like pmndrs `math/time`. The free
//! functions do not clamp, so a caller can deliberately extrapolate; [`Ease`]
//! names a curve that Scene Plans can carry.
use serde::{Deserialize, Serialize};

use super::smoothstep;

/// A gentle ease-out: a ring that keeps growing as it fades.
pub fn quad_out(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Fast start, gentle landing: most entrances and travel.
pub fn cubic_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

/// Gentle start and landing with a quick middle: camera moves and zooms.
pub fn cubic_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Minimum-jerk travel: position, velocity, and acceleration meet a resting
/// hold continuously at both ends, without cubic-in-out's mid-flight jerk.
pub fn smootherstep(t: f32) -> f32 {
    t * t * t * (t * (6.0 * t - 15.0) + 10.0)
}

/// The crossing time of a minimum-jerk mover. Bisection remains well behaved
/// near the zero-slope ends, where Newton iteration is poorly conditioned.
pub fn smootherstep_inverse(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    if p == 0.0 || p == 1.0 {
        return p;
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if smootherstep(mid) < p {
            low = mid;
        } else {
            high = mid;
        }
    }
    (low + high) * 0.5
}

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing curve at `t`.
pub fn cubic_bezier(t: f32, [x1, y1, x2, y2]: [f32; 4]) -> f32 {
    let t = t.clamp(0.0, 1.0);
    bezier_axis(bezier_parameter(t, x1, x2), y1, y2)
}

/// One axis of the timing curve through (0, 0), the handles, and (1, 1).
fn bezier_axis(u: f32, a: f32, b: f32) -> f32 {
    let v = 1.0 - u;
    3.0 * v * v * u * a + 3.0 * v * u * u * b + u * u * u
}

fn bezier_axis_slope(u: f32, a: f32, b: f32) -> f32 {
    let v = 1.0 - u;
    3.0 * v * v * a + 6.0 * v * u * (b - a) + 3.0 * u * u * (1.0 - b)
}

/// The curve parameter whose x is `x`: Newton steps, with bisection when the
/// slope is too flat to trust.
fn bezier_parameter(x: f32, x1: f32, x2: f32) -> f32 {
    let mut u = x;
    for _ in 0..8 {
        let error = bezier_axis(u, x1, x2) - x;
        let slope = bezier_axis_slope(u, x1, x2);
        if error.abs() < 1e-6 || slope.abs() < 1e-6 {
            break;
        }
        u = (u - error / slope).clamp(0.0, 1.0);
    }
    if (bezier_axis(u, x1, x2) - x).abs() < 1e-4 {
        return u;
    }
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..40 {
        u = (low + high) * 0.5;
        if bezier_axis(u, x1, x2) < x {
            low = u;
        } else {
            high = u;
        }
    }
    u
}

/// Launches fast and slows to `final_speed` times the average speed as it
/// arrives, so it lands with momentum: 0 is a quadratic ease-out, 1 is linear.
pub fn decelerate(t: f32, final_speed: f32) -> f32 {
    (2.0 - final_speed) * t + (final_speed - 1.0) * t * t
}

/// A named easing curve. `slope` is its derivative, so an eased move can hand
/// its velocity to a later spring without a jump.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Ease {
    Linear,
    Smoothstep,
    Smootherstep,
    CubicOut,
    CubicInOut,
    /// Arrives at this fraction of the average speed, from 0 to 2.
    Decelerate(f32),
    /// A CSS timing curve: the two handles' x (0..1) and y.
    CubicBezier([f32; 4]),
}

impl Ease {
    /// Wire, leader, bar, and axis draw-on (`cubic-bezier(0.45, 0, 0.2, 1)`):
    /// neither jumps off the start nor parks at the end.
    pub const DRAW: Self = Self::CubicBezier([0.45, 0.0, 0.2, 1.0]);

    /// Minimum-jerk travel between resting holds (`smootherstep`): velocity and
    /// acceleration meet the holds continuously at both ends.
    pub const GLIDE: Self = Self::Smootherstep;

    /// Eases by role, so a film never runs one curve on everything. An
    /// arrival lands on a long tail (expo-out, `cubic-bezier(0.16, 1, 0.3, 1)`).
    pub const ARRIVE: Self = Self::CubicBezier([0.16, 1.0, 0.3, 1.0]);
    /// A camera move or a colour flood eases both ways (power2.inOut).
    pub const TRAVEL: Self = Self::CubicBezier([0.45, 0.0, 0.55, 1.0]);
    /// An exit or a zoom through accelerates away (power3.in).
    pub const LEAVE: Self = Self::CubicBezier([0.32, 0.0, 0.67, 0.0]);

    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Smoothstep => smoothstep(t),
            Self::Smootherstep => smootherstep(t),
            Self::CubicOut => cubic_out(t),
            Self::CubicInOut => cubic_in_out(t),
            Self::Decelerate(final_speed) => decelerate(t, final_speed),
            Self::CubicBezier(handles) => cubic_bezier(t, handles),
        }
    }

    /// The derivative of [`Ease::sample`] with respect to `t`.
    pub fn slope(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => 1.0,
            Self::Smoothstep => 6.0 * t * (1.0 - t),
            Self::Smootherstep => 30.0 * t * t * (1.0 - t).powi(2),
            Self::CubicOut => 3.0 * (1.0 - t).powi(2),
            Self::CubicInOut if t < 0.5 => 12.0 * t * t,
            Self::CubicInOut => 3.0 * (2.0 - 2.0 * t).powi(2),
            Self::Decelerate(final_speed) => (2.0 - final_speed) + 2.0 * (final_speed - 1.0) * t,
            Self::CubicBezier([x1, y1, x2, y2]) => {
                let u = bezier_parameter(t, x1, x2);
                bezier_axis_slope(u, y1, y2) / bezier_axis_slope(u, x1, x2).max(1e-6)
            }
        }
    }

    /// Whether the curve rises from 0 to 1 without reversing.
    pub fn is_valid(self) -> bool {
        match self {
            Self::Decelerate(final_speed) => (0.0..=2.0).contains(&final_speed),
            Self::CubicBezier([x1, y1, x2, y2]) => {
                (0.0..=1.0).contains(&x1)
                    && (0.0..=1.0).contains(&x2)
                    && y1.is_finite()
                    && y2.is_finite()
            }
            _ => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURVES: [Ease; 7] = [
        Ease::Linear,
        Ease::Smoothstep,
        Ease::Smootherstep,
        Ease::CubicOut,
        Ease::CubicInOut,
        Ease::Decelerate(0.4),
        Ease::CubicBezier([0.45, 0.0, 0.2, 1.0]),
    ];

    #[test]
    fn easings_run_from_zero_to_one() {
        for ease in [quad_out, cubic_out, cubic_in_out] {
            assert_eq!((ease(0.0), ease(1.0)), (0.0, 1.0));
        }
        assert!(cubic_out(0.5) > 0.5, "ease-out is ahead of linear");
        assert_eq!(cubic_in_out(0.5), 0.5);
        assert!(cubic_in_out(0.25) < 0.25 && cubic_in_out(0.75) > 0.75);
        for curve in CURVES {
            assert_eq!(
                (curve.sample(0.0), curve.sample(1.0)),
                (0.0, 1.0),
                "{curve:?}"
            );
        }
    }

    #[test]
    fn slopes_are_the_derivatives() {
        for curve in CURVES {
            for step in 1..20 {
                let t = step as f32 / 20.0;
                let h = 1e-3;
                let numeric = (curve.sample(t + h) - curve.sample(t - h)) / (2.0 * h);
                assert!((numeric - curve.slope(t)).abs() < 2e-2, "{curve:?} at {t}");
            }
        }
    }

    #[test]
    fn cubic_bezier_matches_css_and_inverse_undoes_cubic_in_out() {
        // CSS `ease-in-out` is cubic-bezier(.42, 0, .58, 1): symmetric about the middle.
        let css = [0.42, 0.0, 0.58, 1.0];
        assert!((cubic_bezier(0.5, css) - 0.5).abs() < 1e-5);
        assert!((cubic_bezier(0.25, css) + cubic_bezier(0.75, css) - 1.0).abs() < 1e-5);
        assert_eq!(
            cubic_bezier(0.3, [0.0, 0.0, 1.0, 1.0]),
            0.3,
            "straight handles are linear"
        );
        // The wire-draw curve starts gently and finishes early.
        let draw = [0.45, 0.0, 0.2, 1.0];
        assert!(cubic_bezier(0.1, draw) < 0.05 && cubic_bezier(0.8, draw) > 0.95);
    }

    #[test]
    fn decelerate_launches_fast_and_lands_with_momentum() {
        let curve = Ease::Decelerate(0.4);
        assert!((curve.slope(0.0) - 1.6).abs() < 1e-6 && (curve.slope(1.0) - 0.4).abs() < 1e-6);
        assert_eq!(decelerate(0.5, 1.0), 0.5, "a final speed of 1 is linear");
        assert!(curve.is_valid() && !Ease::Decelerate(2.5).is_valid());
        let json = serde_json::to_string(&[Ease::CubicOut, Ease::Decelerate(0.4)]).unwrap();
        assert_eq!(json, r#"["cubic-out",{"decelerate":0.4}]"#);
    }

    #[test]
    fn minimum_jerk_travel_joins_holds_and_preserves_crossing_times() {
        let curve = Ease::Smootherstep;
        assert_eq!(curve.slope(0.0), 0.0);
        assert_eq!(curve.slope(1.0), 0.0);
        let h = 1e-4;
        assert!((curve.slope(h) / h).abs() < 0.004);
        assert!((curve.slope(1.0 - h) / h).abs() < 0.004);
        for p in [0.0, 0.0001, 0.01, 0.25, 0.5, 0.75, 0.99, 0.9999, 1.0] {
            assert!((curve.sample(smootherstep_inverse(p)) - p).abs() < 2e-6);
        }
        assert!((curve.slope(0.5 - h) - curve.slope(0.5 + h)).abs() < 1e-5);
    }
}
