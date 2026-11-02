//! Vector artwork for the Stage's `Art` element: SVG documents embedded in the
//! plan, and the geometry the renderer samples from them each frame (cubic
//! subpaths, arc-length trim windows, and matched Bézier morphs).
//!
//! The core crate parses no SVG. The renderer reads an [`SvgArt`] into an
//! [`Artwork`] once, then everything here is a pure function of the channels.
use std::path::Path;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::math::{
    Vec2,
    curve::{CubicBezier, Polyline},
    lerp,
};

/// An SVG document carried inside the plan, so a plan renders the same
/// wherever it is moved.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SvgArt(String);

impl SvgArt {
    /// SVG markup, such as `<svg viewBox="0 0 100 100"><path d="…"/></svg>`.
    pub fn new(svg: impl Into<String>) -> Self {
        Self(svg.into())
    }

    /// The SVG file at `path`, read now and embedded in the plan.
    ///
    /// Panics when the file cannot be read: a Scene Program names its own
    /// assets, so a missing one is an authoring bug.
    pub fn from_file(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(svg) => Self(svg),
            Err(error) => panic!("cannot read SVG art {}: {error}", path.display()),
        }
    }

    pub fn svg(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(self.0.contains("<svg"), "art is not an SVG document");
        Ok(())
    }
}

/// How an `Art` element's trim window covers its subpaths.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrimMode {
    /// Every subpath draws on at once, each over its own length (After
    /// Effects' "simultaneously").
    #[default]
    Each,
    /// One window over all subpaths end to end, in document order, so the
    /// art draws on like a pen.
    Sequence,
}

impl TrimMode {
    pub fn is_each(&self) -> bool {
        *self == Self::Each
    }
}

/// The visible stretch of a stroke, as fractions of its length: `start` and
/// `end` (either order), both shifted by `offset`, which wraps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trim {
    pub start: f32,
    pub end: f32,
    pub offset: f32,
}

impl Trim {
    pub const FULL: Self = Self {
        start: 0.0,
        end: 1.0,
        offset: 0.0,
    };

    /// The window as at most two fractional spans in `0..=1`: one, or two
    /// when the offset carries it past the end and it wraps to the start.
    pub fn spans(&self) -> Vec<[f32; 2]> {
        let low = self.start.min(self.end).clamp(0.0, 1.0);
        let high = self.start.max(self.end).clamp(0.0, 1.0);
        let width = high - low;
        if width <= 1e-5 {
            return Vec::new();
        }
        if width >= 1.0 - 1e-5 {
            return vec![[0.0, 1.0]];
        }
        let from = low + self.offset;
        let from = from - from.floor();
        let to = from + width;
        if to <= 1.0 {
            vec![[from, to]]
        } else {
            vec![[from, 1.0], [0.0, to - 1.0]]
        }
    }
}

/// A run of cubics, each starting where the previous ended.
#[derive(Clone, Debug, PartialEq)]
pub struct Subpath {
    pub curves: Vec<CubicBezier>,
    pub closed: bool,
}

impl Subpath {
    /// Straight pieces close enough to the curves at `scale` screen pixels
    /// per unit; a closed subpath ends on its first point.
    pub fn flatten(&self, scale: f32) -> Polyline {
        let mut points = Vec::new();
        for (index, curve) in self.curves.iter().enumerate() {
            if index == 0 {
                points.push(curve.start);
            }
            let segments = if straight(curve) {
                1
            } else {
                ((curve.hull_length() * scale.max(0.01)).sqrt() * 0.9)
                    .ceil()
                    .clamp(2.0, 40.0) as usize
            };
            points.extend((1..=segments).map(|i| curve.point(i as f32 / segments as f32)));
        }
        if self.closed
            && let (Some(first), Some(last)) = (points.first().copied(), points.last().copied())
            && first.distance(last) > 1e-4
        {
            points.push(first);
        }
        Polyline::new(points)
    }

    pub fn length(&self) -> f32 {
        self.flatten(1.0).length()
    }

    /// Shoelace area of the flattened outline: positive when it turns
    /// clockwise on screen (y down).
    pub fn signed_area(&self) -> f32 {
        let line = self.flatten(1.0);
        let points = line.points();
        points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(a, b)| a.perp_dot(*b))
            .sum::<f32>()
            * 0.5
    }

    /// The mean of the curve ends: where a vanishing partner collapses.
    pub fn centroid(&self) -> Vec2 {
        if self.curves.is_empty() {
            return Vec2::ZERO;
        }
        self.curves.iter().map(|c| c.start).sum::<Vec2>() / self.curves.len() as f32
    }

    /// The stretches of this subpath inside `trim`, each a polyline; a
    /// closed subpath's wrapped window stays one unbroken stroke.
    pub fn trimmed(&self, trim: Trim, scale: f32) -> Vec<Polyline> {
        let line = self.flatten(scale);
        slice_spans(&line, &trim.spans(), self.closed)
    }
}

fn straight(curve: &CubicBezier) -> bool {
    let chord = curve.end - curve.start;
    let length = chord.length();
    if length < 1e-4 {
        return curve.hull_length() < 1e-3;
    }
    let off = |p: Vec2| (p - curve.start).perp_dot(chord).abs() / length;
    off(curve.control_a) < 0.05 && off(curve.control_b) < 0.05
}

/// `line` cut to fractional `spans`. On a closed line a span ending at 1
/// followed by one starting at 0 joins across the seam.
fn slice_spans(line: &Polyline, spans: &[[f32; 2]], closed: bool) -> Vec<Polyline> {
    let mut pieces: Vec<Polyline> = spans
        .iter()
        .map(|&[from, to]| line.slice(from, to))
        .filter(|piece| piece.points().len() >= 2)
        .collect();
    if closed && spans.len() == 2 && pieces.len() == 2 {
        let tail = pieces.pop().unwrap_or_default();
        let head = pieces.pop().unwrap_or_default();
        let mut points = head.points().to_vec();
        points.extend(tail.points().iter().skip(1));
        pieces.push(Polyline::new(points));
    }
    pieces
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// One SVG path: its subpaths, filled and stroked by their own colors
/// (straight sRGB, `0..=1`).
#[derive(Clone, Debug, PartialEq)]
pub struct ArtPath {
    pub subpaths: Vec<Subpath>,
    pub fill: Option<[f32; 4]>,
    pub rule: FillRule,
    pub stroke: Option<[f32; 4]>,
    pub width: f32,
}

impl ArtPath {
    fn centroid(&self) -> Vec2 {
        if self.subpaths.is_empty() {
            return Vec2::ZERO;
        }
        self.subpaths.iter().map(Subpath::centroid).sum::<Vec2>() / self.subpaths.len() as f32
    }
}

/// A whole SVG document as vector paths, centered on its view box.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Artwork {
    pub paths: Vec<ArtPath>,
}

impl Artwork {
    /// Low and high corners of every curve's control points.
    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        let mut points = self
            .paths
            .iter()
            .flat_map(|path| &path.subpaths)
            .flat_map(|sub| &sub.curves)
            .flat_map(|c| [c.start, c.control_a, c.control_b, c.end])
            .peekable();
        points.peek()?;
        Some(points.fold((Vec2::MAX, Vec2::MIN), |(low, high), p| {
            (low.min(p), high.max(p))
        }))
    }

    /// The stroked stretches of every path's subpaths inside `trim`, as
    /// (path index, polyline). `Sequence` lays the subpaths end to end in
    /// document order and runs one window over the whole run.
    pub fn trimmed_strokes(
        &self,
        trim: Trim,
        mode: TrimMode,
        scale: f32,
    ) -> Vec<(usize, Polyline)> {
        let subpaths = self
            .paths
            .iter()
            .enumerate()
            .filter(|(_, path)| path.stroke.is_some())
            .flat_map(|(index, path)| path.subpaths.iter().map(move |sub| (index, sub)));
        match mode {
            TrimMode::Each => subpaths
                .flat_map(|(index, sub)| {
                    sub.trimmed(trim, scale)
                        .into_iter()
                        .map(move |line| (index, line))
                })
                .collect(),
            TrimMode::Sequence => {
                let lines = subpaths
                    .map(|(index, sub)| (index, sub.flatten(scale)))
                    .collect::<Vec<_>>();
                let total = lines.iter().map(|(_, l)| l.length()).sum::<f32>();
                if total <= 1e-4 {
                    return Vec::new();
                }
                let spans = trim.spans();
                let mut out = Vec::new();
                let mut before = 0.0;
                for (index, line) in lines {
                    let (from, to) = (before / total, (before + line.length()) / total);
                    before += line.length();
                    let local = spans
                        .iter()
                        .filter_map(|&[a, b]| {
                            let (a, b) = (a.max(from), b.min(to));
                            (b > a).then(|| {
                                let width = (to - from).max(1e-9);
                                [(a - from) / width, (b - from) / width]
                            })
                        })
                        .collect::<Vec<_>>();
                    out.extend(
                        slice_spans(&line, &local, false)
                            .into_iter()
                            .map(|piece| (index, piece)),
                    );
                }
                out
            }
        }
    }
}

/// Two artworks matched curve for curve, so any point between them is a
/// plain blend of control points (see [`ArtMorph::at`]).
///
/// Paths pair by document order, and subpaths within them by order. A
/// path or subpath with no partner grows from (or shrinks to) the centre of
/// the one it pairs with. Each matched pair is cut into the same number of
/// cubics, every original cubic getting pieces in proportion to its length,
/// so corners stay corners and equal fractions of length travel together.
/// A closed target is turned to wind the same way as its source, then its
/// start is rotated to the closest fit.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtMorph {
    pairs: Vec<PathPair>,
}

#[derive(Clone, Debug, PartialEq)]
struct PathPair {
    from: ArtPath,
    to: ArtPath,
    /// Subpaths whose target was traced backwards to match: blends past
    /// halfway trace it the target's own way again, so fills keep its winding.
    reversed: Vec<bool>,
}

impl ArtMorph {
    pub fn new(from: &Artwork, to: &Artwork) -> Self {
        let count = from.paths.len().max(to.paths.len());
        let pairs = (0..count)
            .map(|index| match (from.paths.get(index), to.paths.get(index)) {
                (Some(a), Some(b)) => match_paths(a, b),
                (Some(a), None) => match_paths(a, &vanished(a)),
                (None, Some(b)) => match_paths(&vanished(b), b),
                (None, None) => unreachable!(),
            })
            .collect();
        Self { pairs }
    }

    /// The blend `t` of the way from the source to the target.
    pub fn at(&self, t: f32) -> Artwork {
        let t = t.clamp(0.0, 1.0);
        Artwork {
            paths: self
                .pairs
                .iter()
                .map(|PathPair { from, to, reversed }| ArtPath {
                    subpaths: from
                        .subpaths
                        .iter()
                        .zip(&to.subpaths)
                        .zip(reversed)
                        .map(|((a, b), &reversed)| {
                            let curves = a
                                .curves
                                .iter()
                                .zip(&b.curves)
                                .map(|(ca, cb)| ca.lerp(cb, t));
                            Subpath {
                                curves: if reversed && t >= 0.5 {
                                    curves.rev().map(|c| c.reversed()).collect()
                                } else {
                                    curves.collect()
                                },
                                closed: if t < 0.5 { a.closed } else { b.closed },
                            }
                        })
                        .collect(),
                    fill: blend_color(from.fill, to.fill, t),
                    rule: if t < 0.5 { from.rule } else { to.rule },
                    stroke: blend_color(from.stroke, to.stroke, t),
                    width: lerp(from.width, to.width, t),
                })
                .collect(),
        }
    }
}

/// `path` collapsed to its centre with its colors faded out.
fn vanished(path: &ArtPath) -> ArtPath {
    let center = path.centroid();
    ArtPath {
        subpaths: path
            .subpaths
            .iter()
            .map(|sub| Subpath {
                curves: vec![CubicBezier::point_at(center)],
                closed: sub.closed,
            })
            .collect(),
        fill: path.fill.map(|[r, g, b, _]| [r, g, b, 0.0]),
        rule: path.rule,
        stroke: path.stroke.map(|[r, g, b, _]| [r, g, b, 0.0]),
        width: path.width,
    }
}

fn blend_color(a: Option<[f32; 4]>, b: Option<[f32; 4]>, t: f32) -> Option<[f32; 4]> {
    let clear = |c: [f32; 4]| [c[0], c[1], c[2], 0.0];
    let (a, b) = match (a, b) {
        (None, None) => return None,
        (Some(a), None) => (a, clear(a)),
        (None, Some(b)) => (clear(b), b),
        (Some(a), Some(b)) => (a, b),
    };
    Some(std::array::from_fn(|i| lerp(a[i], b[i], t)))
}

fn match_paths(a: &ArtPath, b: &ArtPath) -> PathPair {
    let count = a.subpaths.len().max(b.subpaths.len());
    let (mut from, mut to) = (a.clone(), b.clone());
    from.subpaths.clear();
    to.subpaths.clear();
    let mut reversed = Vec::with_capacity(count);
    for index in 0..count {
        let (sa, sb) = match (a.subpaths.get(index), b.subpaths.get(index)) {
            (Some(sa), Some(sb)) => (sa.clone(), sb.clone()),
            (Some(sa), None) => (sa.clone(), collapsed(sa, b.centroid())),
            (None, Some(sb)) => (collapsed(sb, a.centroid()), sb.clone()),
            (None, None) => unreachable!(),
        };
        let (sa, sb, flipped) = match_subpaths(&sa, &sb);
        from.subpaths.push(sa);
        to.subpaths.push(sb);
        reversed.push(flipped);
    }
    PathPair { from, to, reversed }
}

fn collapsed(partner: &Subpath, at: Vec2) -> Subpath {
    Subpath {
        curves: vec![CubicBezier::point_at(at)],
        closed: partner.closed,
    }
}

/// Two subpaths cut to the same number of cubics and aligned so the `k`th
/// cubic of each travels to the other, and whether the target now runs
/// backwards.
pub fn match_subpaths(a: &Subpath, b: &Subpath) -> (Subpath, Subpath, bool) {
    let count = a.curves.len().max(b.curves.len()).max(1);
    let ca = subdivide(&a.curves, count);
    let mut cb = subdivide(&b.curves, count);
    let cost = |x: &[CubicBezier], y: &[CubicBezier]| -> f32 {
        x.iter()
            .zip(y)
            .map(|(p, q)| p.start.distance_squared(q.start) + p.end.distance_squared(q.end))
            .sum()
    };
    let reversed = |curves: &[CubicBezier]| -> Vec<CubicBezier> {
        curves.iter().rev().map(CubicBezier::reversed).collect()
    };
    let mut flipped = false;
    if a.closed && b.closed {
        let (area_a, area_b) = (a.signed_area(), b.signed_area());
        if area_a * area_b < 0.0 {
            cb = reversed(&cb);
            flipped = true;
        }
        let best = (0..count)
            .min_by(|&r, &s| {
                let turn = |k: usize| {
                    let mut turned = cb.clone();
                    turned.rotate_left(k);
                    cost(&ca, &turned)
                };
                turn(r).total_cmp(&turn(s))
            })
            .unwrap_or(0);
        cb.rotate_left(best);
    } else if cost(&ca, &reversed(&cb)) < cost(&ca, &cb) {
        cb = reversed(&cb);
        flipped = true;
    }
    (
        Subpath {
            curves: ca,
            closed: a.closed,
        },
        Subpath {
            curves: cb,
            closed: b.closed,
        },
        flipped,
    )
}

/// `curves` cut into exactly `count` cubics (at least as many as there
/// are), each original getting pieces in proportion to its arc length.
pub fn subdivide(curves: &[CubicBezier], count: usize) -> Vec<CubicBezier> {
    if curves.is_empty() {
        return vec![CubicBezier::point_at(Vec2::ZERO); count];
    }
    let lengths = curves
        .iter()
        .map(|c| c.flatten(16).length())
        .collect::<Vec<_>>();
    let mut pieces = vec![1usize; curves.len()];
    for _ in curves.len()..count {
        let longest = (0..curves.len())
            .max_by(|&i, &j| {
                (lengths[i] / pieces[i] as f32)
                    .total_cmp(&(lengths[j] / pieces[j] as f32))
                    .then(j.cmp(&i))
            })
            .unwrap_or(0);
        pieces[longest] += 1;
    }
    curves
        .iter()
        .zip(pieces)
        .flat_map(|(curve, n)| curve.divide(n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::vec2;

    fn square(size: f32, clockwise: bool) -> Subpath {
        let mut corners = [
            vec2(0.0, 0.0),
            vec2(size, 0.0),
            vec2(size, size),
            vec2(0.0, size),
        ];
        if !clockwise {
            corners.reverse();
        }
        Subpath {
            curves: (0..4)
                .map(|i| CubicBezier::line(corners[i], corners[(i + 1) % 4]))
                .collect(),
            closed: true,
        }
    }

    fn close(a: Vec2, b: Vec2) -> bool {
        a.distance(b) < 1e-3
    }

    #[test]
    fn trim_spans_wrap_past_the_end_and_ignore_order() {
        let trim = |start, end, offset| Trim { start, end, offset }.spans();
        assert_eq!(trim(0.0, 1.0, 0.3), vec![[0.0, 1.0]]);
        assert_eq!(trim(0.5, 0.5, 0.0), Vec::<[f32; 2]>::new());
        assert_eq!(trim(0.75, 0.25, 0.0), vec![[0.25, 0.75]]);
        let wrapped = trim(0.0, 0.5, 0.75);
        assert_eq!(wrapped.len(), 2);
        assert!((wrapped[0][0] - 0.75).abs() < 1e-6 && wrapped[0][1] == 1.0);
        assert!(wrapped[1][0] == 0.0 && (wrapped[1][1] - 0.25).abs() < 1e-6);
        // Negative offsets wrap the same way.
        assert_eq!(trim(0.0, 0.25, -0.5), vec![[0.5, 0.75]]);
    }

    #[test]
    fn a_closed_trim_crossing_the_seam_stays_one_stroke() {
        let sq = square(100.0, true);
        let pieces = sq.trimmed(
            Trim {
                start: 0.0,
                end: 0.25,
                offset: 0.875,
            },
            1.0,
        );
        assert_eq!(pieces.len(), 1);
        let piece = &pieces[0];
        assert!((piece.length() - 100.0).abs() < 1e-2);
        assert!(close(piece.points()[0], vec2(0.0, 50.0)));
        assert!(close(*piece.points().last().unwrap(), vec2(50.0, 0.0)));
        // An open path's wrapped window is two strokes.
        let open = Subpath {
            closed: false,
            ..sq.clone()
        };
        let line = open.flatten(1.0);
        assert_eq!(
            slice_spans(&line, &[[0.875, 1.0], [0.0, 0.125]], false).len(),
            2
        );
    }

    #[test]
    fn sequence_trim_draws_subpaths_one_after_another() {
        let art = Artwork {
            paths: vec![ArtPath {
                subpaths: vec![square(100.0, true), square(100.0, true)],
                fill: None,
                rule: FillRule::NonZero,
                stroke: Some([1.0; 4]),
                width: 2.0,
            }],
        };
        let half = Trim {
            start: 0.0,
            end: 0.5,
            offset: 0.0,
        };
        let each = art.trimmed_strokes(half, TrimMode::Each, 1.0);
        assert_eq!(each.len(), 2);
        assert!(each.iter().all(|(_, l)| (l.length() - 200.0).abs() < 1e-2));
        let seq = art.trimmed_strokes(half, TrimMode::Sequence, 1.0);
        assert_eq!(seq.len(), 1);
        assert!((seq[0].1.length() - 400.0).abs() < 1e-2);
        // Unstroked paths draw no strokes.
        let mut bare = art.clone();
        bare.paths[0].stroke = None;
        assert!(
            bare.trimmed_strokes(Trim::FULL, TrimMode::Each, 1.0)
                .is_empty()
        );
    }

    #[test]
    fn subdividing_keeps_corners_and_splits_by_length() {
        let sq = square(100.0, true);
        let cut = subdivide(&sq.curves, 10);
        assert_eq!(cut.len(), 10);
        // Every original corner is still a piece's start.
        for corner in [vec2(100.0, 0.0), vec2(100.0, 100.0), vec2(0.0, 100.0)] {
            assert!(cut.iter().any(|c| close(c.start, corner)), "{corner}");
        }
        // Consecutive pieces meet.
        for pair in cut.windows(2) {
            assert!(close(pair[0].end, pair[1].start));
        }
        let lengths = cut
            .iter()
            .map(|c| c.flatten(8).length())
            .collect::<Vec<_>>();
        assert!(
            lengths.iter().all(|l| (33.0..=50.1).contains(l)),
            "{lengths:?}"
        );
    }

    #[test]
    fn morph_matches_winding_and_start_then_lands_exactly() {
        let a = square(100.0, true);
        // The same square traced the other way from another corner.
        let mut b = square(100.0, false);
        b.curves.rotate_left(2);
        let (ma, mb, flipped) = match_subpaths(&a, &b);
        assert!(flipped);
        assert_eq!(ma.curves.len(), mb.curves.len());
        for (p, q) in ma.curves.iter().zip(&mb.curves) {
            assert!(close(p.start, q.start), "{} vs {}", p.start, q.start);
        }
        // Two rings of different sizes keep their endpoints exact.
        let art = |sub: Subpath| Artwork {
            paths: vec![ArtPath {
                subpaths: vec![sub],
                fill: Some([1.0, 0.0, 0.0, 1.0]),
                rule: FillRule::NonZero,
                stroke: None,
                width: 1.0,
            }],
        };
        let big = art(square(200.0, true));
        let small = art(square(100.0, true));
        let morph = ArtMorph::new(&small, &big);
        let end = morph.at(1.0);
        assert!(
            (end.paths[0].subpaths[0].signed_area() - 40_000.0).abs() < 1.0,
            "{}",
            end.paths[0].subpaths[0].signed_area()
        );
        let mid = morph.at(0.5);
        assert!((mid.paths[0].subpaths[0].signed_area() - 22_500.0).abs() < 1.0);
        // Against an opposite winding, each half keeps its own end's winding.
        let mut ccw = big.clone();
        ccw.paths[0].subpaths[0] = square(200.0, false);
        let morph = ArtMorph::new(&small, &ccw);
        assert!(morph.at(0.25).paths[0].subpaths[0].signed_area() > 0.0);
        assert!(morph.at(0.75).paths[0].subpaths[0].signed_area() < 0.0);
    }

    #[test]
    fn unpartnered_paths_grow_from_the_center_and_fade() {
        let one = Artwork {
            paths: vec![ArtPath {
                subpaths: vec![square(100.0, true)],
                fill: Some([0.0, 0.0, 1.0, 1.0]),
                rule: FillRule::NonZero,
                stroke: None,
                width: 1.0,
            }],
        };
        let morph = ArtMorph::new(&Artwork::default(), &one);
        let start = morph.at(0.0);
        assert_eq!(start.paths[0].fill.unwrap()[3], 0.0);
        assert!(start.paths[0].subpaths[0].signed_area().abs() < 1e-3);
        assert_eq!(morph.at(1.0).paths[0].fill, one.paths[0].fill);
    }
}
