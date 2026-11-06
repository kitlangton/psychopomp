//! Paths in the plane, measured by arc length so motion along them keeps a
//! steady speed however a curve bunches its parameter.
use super::{Vec2, inverse_lerp};

/// A cubic Bézier: leaves `start` toward `control_a` and arrives at `end`
/// from the direction of `control_b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub start: Vec2,
    pub control_a: Vec2,
    pub control_b: Vec2,
    pub end: Vec2,
}

impl CubicBezier {
    pub fn point(&self, t: f32) -> Vec2 {
        let u = 1.0 - t;
        self.start * (u * u * u)
            + self.control_a * (3.0 * u * u * t)
            + self.control_b * (3.0 * u * t * t)
            + self.end * (t * t * t)
    }

    /// The curve as `segments` straight pieces through evenly spaced parameters.
    pub fn flatten(&self, segments: usize) -> Polyline {
        let segments = segments.max(1);
        Polyline::new(
            (0..=segments)
                .map(|i| self.point(i as f32 / segments as f32))
                .collect(),
        )
    }

    /// A straight segment as a cubic, its handles at thirds.
    pub fn line(start: Vec2, end: Vec2) -> Self {
        Self {
            start,
            control_a: start.lerp(end, 1.0 / 3.0),
            control_b: start.lerp(end, 2.0 / 3.0),
            end,
        }
    }

    /// A quadratic Bézier raised exactly to a cubic.
    pub fn quadratic(start: Vec2, control: Vec2, end: Vec2) -> Self {
        Self {
            start,
            control_a: start.lerp(control, 2.0 / 3.0),
            control_b: end.lerp(control, 2.0 / 3.0),
            end,
        }
    }

    /// Every control point at `point`: a curve of zero length.
    pub fn point_at(point: Vec2) -> Self {
        Self {
            start: point,
            control_a: point,
            control_b: point,
            end: point,
        }
    }

    /// The same curve traced from `end` back to `start`.
    pub fn reversed(&self) -> Self {
        Self {
            start: self.end,
            control_a: self.control_b,
            control_b: self.control_a,
            end: self.start,
        }
    }

    /// The two halves either side of parameter `t` (de Casteljau).
    pub fn split(&self, t: f32) -> (Self, Self) {
        let ab = self.start.lerp(self.control_a, t);
        let bc = self.control_a.lerp(self.control_b, t);
        let cd = self.control_b.lerp(self.end, t);
        let abc = ab.lerp(bc, t);
        let bcd = bc.lerp(cd, t);
        let mid = abc.lerp(bcd, t);
        (
            Self {
                start: self.start,
                control_a: ab,
                control_b: abc,
                end: mid,
            },
            Self {
                start: mid,
                control_a: bcd,
                control_b: cd,
                end: self.end,
            },
        )
    }

    /// Each control point moved `t` of the way toward `other`'s.
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            start: self.start.lerp(other.start, t),
            control_a: self.control_a.lerp(other.control_a, t),
            control_b: self.control_b.lerp(other.control_b, t),
            end: self.end.lerp(other.end, t),
        }
    }

    /// Length of the control polygon: never shorter than the curve.
    pub fn hull_length(&self) -> f32 {
        self.start.distance(self.control_a)
            + self.control_a.distance(self.control_b)
            + self.control_b.distance(self.end)
    }

    /// The parameters that cut the curve into `pieces` of equal arc length.
    pub fn equal_length_cuts(&self, pieces: usize) -> Vec<f32> {
        const SAMPLES: usize = 32;
        let table = self.flatten(SAMPLES);
        let length = table.length();
        (1..pieces.max(1))
            .map(|k| {
                let target = length * k as f32 / pieces as f32;
                let lengths = table.lengths();
                let index = lengths.partition_point(|&at| at < target).clamp(1, SAMPLES);
                let along = inverse_lerp(lengths[index - 1], lengths[index], target);
                (index as f32 - 1.0 + along) / SAMPLES as f32
            })
            .collect()
    }

    /// The curve cut into `pieces` of about equal arc length.
    pub fn divide(&self, pieces: usize) -> Vec<Self> {
        let mut out = Vec::with_capacity(pieces.max(1));
        let mut rest = *self;
        let mut done = 0.0;
        for t in self.equal_length_cuts(pieces) {
            let (head, tail) = rest.split(inverse_lerp(done, 1.0, t));
            out.push(head);
            rest = tail;
            done = t;
        }
        out.push(rest);
        out
    }
}

/// Points joined by straight segments, with the running length at each point.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline {
    points: Vec<Vec2>,
    lengths: Vec<f32>,
}

impl Polyline {
    pub fn new(points: Vec<Vec2>) -> Self {
        let mut total = 0.0;
        let lengths = points
            .iter()
            .enumerate()
            .map(|(index, point)| {
                if index > 0 {
                    total += point.distance(points[index - 1]);
                }
                total
            })
            .collect();
        Self { points, lengths }
    }

    pub fn points(&self) -> &[Vec2] {
        &self.points
    }

    /// Running length at each point, starting at zero.
    pub fn lengths(&self) -> &[f32] {
        &self.lengths
    }

    pub fn length(&self) -> f32 {
        self.lengths.last().copied().unwrap_or(0.0)
    }

    /// The point `distance` along the path, held at its ends.
    pub fn at_length(&self, distance: f32) -> Vec2 {
        match self.points.len() {
            0 => Vec2::ZERO,
            1 => self.points[0],
            count => {
                let distance = distance.clamp(0.0, self.length());
                let index = self
                    .lengths
                    .partition_point(|&length| length < distance)
                    .clamp(1, count - 1);
                let (before, after) = (self.lengths[index - 1], self.lengths[index]);
                self.points[index - 1]
                    .lerp(self.points[index], inverse_lerp(before, after, distance))
            }
        }
    }

    /// The point at `fraction` of the path's length.
    pub fn at(&self, fraction: f32) -> Vec2 {
        self.at_length(fraction * self.length())
    }

    /// The stretch between two fractions of the length, with exact end points.
    /// An empty stretch is a single point.
    pub fn slice(&self, from: f32, to: f32) -> Self {
        let length = self.length();
        let start = from.clamp(0.0, 1.0) * length;
        let end = to.clamp(0.0, 1.0) * length;
        if end <= start {
            return Self::new(vec![self.at_length(start)]);
        }
        let inner = self
            .points
            .iter()
            .zip(&self.lengths)
            .filter(|&(_, &at)| at > start && at < end)
            .map(|(point, _)| *point);
        Self::new(
            std::iter::once(self.at_length(start))
                .chain(inner)
                .chain(std::iter::once(self.at_length(end)))
                .collect(),
        )
    }

    pub fn reversed(&self) -> Self {
        Self::new(self.points.iter().rev().copied().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::vec2;

    fn corner() -> Polyline {
        Polyline::new(vec![vec2(0.0, 0.0), vec2(100.0, 0.0), vec2(100.0, 100.0)])
    }

    #[test]
    fn cubics_split_reverse_and_divide_by_arc_length() {
        let curve = CubicBezier {
            start: vec2(0.0, 0.0),
            control_a: vec2(0.0, 100.0),
            control_b: vec2(20.0, 100.0),
            end: vec2(200.0, 0.0),
        };
        let (head, tail) = curve.split(0.3);
        assert_eq!((head.start, tail.end), (curve.start, curve.end));
        assert!(head.end.distance(curve.point(0.3)) < 1e-4);
        assert!(tail.point(0.5).distance(curve.point(0.65)) < 1e-3);
        assert!(curve.reversed().point(0.2).distance(curve.point(0.8)) < 1e-4);

        let pieces = curve.divide(4);
        assert_eq!(pieces.len(), 4);
        assert_eq!((pieces[0].start, pieces[3].end), (curve.start, curve.end));
        for pair in pieces.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        let lengths: Vec<f32> = pieces.iter().map(|p| p.flatten(32).length()).collect();
        let mean = lengths.iter().sum::<f32>() / 4.0;
        assert!(
            lengths.iter().all(|l| (l - mean).abs() < mean * 0.02),
            "{lengths:?}"
        );

        let line = CubicBezier::line(vec2(0.0, 0.0), vec2(90.0, 0.0));
        assert!((line.hull_length() - 90.0).abs() < 1e-4);
        assert_eq!(line.lerp(&curve, 0.0), line);
        assert_eq!(line.lerp(&curve, 1.0), curve);
        let quad = CubicBezier::quadratic(vec2(0.0, 0.0), vec2(50.0, 100.0), vec2(100.0, 0.0));
        assert!(quad.point(0.5).distance(vec2(50.0, 50.0)) < 1e-4);
    }

    #[test]
    fn polylines_are_measured_and_sampled_by_arc_length() {
        let path = corner();
        assert_eq!(path.length(), 200.0);
        assert_eq!(path.lengths(), &[0.0, 100.0, 200.0]);
        assert_eq!(path.at(0.25), vec2(50.0, 0.0));
        assert_eq!(path.at(0.75), vec2(100.0, 50.0));
        assert_eq!(
            (path.at(-1.0), path.at(2.0)),
            (vec2(0.0, 0.0), vec2(100.0, 100.0))
        );
        assert_eq!(path.reversed().at(0.25), vec2(100.0, 50.0));
    }

    #[test]
    fn slices_keep_exact_ends_and_inner_corners() {
        let slice = corner().slice(0.25, 0.75);
        assert_eq!(
            slice.points(),
            &[vec2(50.0, 0.0), vec2(100.0, 0.0), vec2(100.0, 50.0)]
        );
        assert_eq!(slice.length(), 100.0);
        assert_eq!(corner().slice(0.6, 0.6).points().len(), 1);
    }

    #[test]
    fn arc_length_sampling_ignores_how_the_parameter_bunches() {
        // A straight curve whose handles crowd its start: the parameter midpoint
        // is near the start, but half the length is the true midpoint.
        let curve = CubicBezier {
            start: vec2(0.0, 0.0),
            control_a: vec2(1.0, 0.0),
            control_b: vec2(2.0, 0.0),
            end: vec2(100.0, 0.0),
        };
        assert_eq!(
            (curve.point(0.0), curve.point(1.0)),
            (curve.start, curve.end)
        );
        assert!(curve.point(0.5).x < 20.0);
        let path = curve.flatten(64);
        assert!((path.length() - 100.0).abs() < 1e-3);
        assert!((path.at(0.5).x - 50.0).abs() < 1e-3);
    }
}
