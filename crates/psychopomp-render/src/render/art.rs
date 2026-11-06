//! The Stage's `Art` element read from SVG into vector paths once, before any
//! frame: every visible path's subpaths as cubics in art units centered on
//! the document, with its own fill, fill rule, stroke, and stroke width.
use std::borrow::Cow;

use anyhow::{Context, Result, ensure};
use psychopomp::{
    art::{ArtMorph, ArtPath, Artwork, FillRule, Subpath, SvgArt},
    math::{Vec2, curve::CubicBezier, vec2},
};
use resvg::usvg::{self, tiny_skia_path::PathSegment};

/// Read `svg` into vector paths. Text and images are ignored; gradient
/// paints become the mean of their stops (an element `fill` override paints
/// a true gradient across the whole art).
pub(crate) fn parse_art(svg: &SvgArt) -> Result<Artwork> {
    let tree =
        usvg::Tree::from_str(svg.svg(), &usvg::Options::default()).context("SVG does not parse")?;
    let size = tree.size();
    let center = vec2(size.width(), size.height()) * 0.5;
    let mut art = Artwork::default();
    collect(tree.root(), 1.0, center, &mut art);
    ensure!(
        art.paths.iter().any(|path| !path.subpaths.is_empty()),
        "SVG draws no paths"
    );
    Ok(art)
}

fn collect(group: &usvg::Group, opacity: f32, center: Vec2, art: &mut Artwork) {
    let opacity = opacity * group.opacity().get();
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => collect(group, opacity, center, art),
            usvg::Node::Path(path) if path.is_visible() => {
                if let Some(path) = art_path(path, opacity, center) {
                    art.paths.push(path);
                }
            }
            _ => {}
        }
    }
}

fn art_path(path: &usvg::Path, opacity: f32, center: Vec2) -> Option<ArtPath> {
    let transform = path.abs_transform();
    let map = |point: usvg::tiny_skia_path::Point| {
        let mut point = point;
        transform.map_point(&mut point);
        vec2(point.x, point.y) - center
    };
    let mut subpaths = Vec::new();
    let mut curves: Vec<CubicBezier> = Vec::new();
    let (mut start, mut at) = (Vec2::ZERO, Vec2::ZERO);
    let finish = |curves: &mut Vec<CubicBezier>, closed: bool, subpaths: &mut Vec<Subpath>| {
        if !curves.is_empty() {
            subpaths.push(Subpath {
                curves: std::mem::take(curves),
                closed,
            });
        }
    };
    for segment in path.data().segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                finish(&mut curves, false, &mut subpaths);
                start = map(p);
                at = start;
            }
            PathSegment::LineTo(p) => {
                let end = map(p);
                curves.push(CubicBezier::line(at, end));
                at = end;
            }
            PathSegment::QuadTo(c, p) => {
                let end = map(p);
                curves.push(CubicBezier::quadratic(at, map(c), end));
                at = end;
            }
            PathSegment::CubicTo(a, b, p) => {
                let end = map(p);
                curves.push(CubicBezier {
                    start: at,
                    control_a: map(a),
                    control_b: map(b),
                    end,
                });
                at = end;
            }
            PathSegment::Close => {
                if at.distance(start) > 1e-4 {
                    curves.push(CubicBezier::line(at, start));
                }
                finish(&mut curves, true, &mut subpaths);
                at = start;
            }
        }
    }
    finish(&mut curves, false, &mut subpaths);
    if subpaths.is_empty() {
        return None;
    }
    let (sx, sy) = transform.get_scale();
    let fill = path
        .fill()
        .map(|fill| color(fill.paint(), fill.opacity().get() * opacity));
    let rule = match path.fill().map(usvg::Fill::rule) {
        Some(usvg::FillRule::EvenOdd) => FillRule::EvenOdd,
        _ => FillRule::NonZero,
    };
    let stroke = path
        .stroke()
        .map(|stroke| color(stroke.paint(), stroke.opacity().get() * opacity));
    let width = path
        .stroke()
        .map_or(0.0, |stroke| stroke.width().get() * (sx * sy).abs().sqrt());
    Some(ArtPath {
        subpaths,
        fill,
        rule,
        stroke,
        width,
    })
}

/// Straight sRGB `0..=1` with alpha.
fn color(paint: &usvg::Paint, opacity: f32) -> [f32; 4] {
    let rgb = |c: usvg::Color| {
        [
            f32::from(c.red) / 255.0,
            f32::from(c.green) / 255.0,
            f32::from(c.blue) / 255.0,
        ]
    };
    let mean = |stops: &[usvg::Stop]| -> [f32; 4] {
        if stops.is_empty() {
            return [0.5, 0.5, 0.5, opacity];
        }
        let mut sum = [0.0; 4];
        for stop in stops {
            let [r, g, b] = rgb(stop.color());
            for (total, v) in sum.iter_mut().zip([r, g, b, stop.opacity().get()]) {
                *total += v;
            }
        }
        let n = stops.len() as f32;
        [sum[0] / n, sum[1] / n, sum[2] / n, sum[3] / n * opacity]
    };
    match paint {
        usvg::Paint::Color(c) => {
            let [r, g, b] = rgb(*c);
            [r, g, b, opacity]
        }
        usvg::Paint::LinearGradient(g) => mean(g.stops()),
        usvg::Paint::RadialGradient(g) => mean(g.stops()),
        usvg::Paint::Pattern(_) => [0.5, 0.5, 0.5, opacity],
    }
}

/// An `Art` element's artwork and morph targets, matched pairwise once.
pub(crate) struct PreparedArt {
    shapes: Vec<Artwork>,
    morphs: Vec<ArtMorph>,
}

impl PreparedArt {
    /// `stroked` strokes every path (an element-level stroke override), so
    /// paths the SVG leaves unstroked still draw on.
    pub(crate) fn new(art: &SvgArt, morphs: &[SvgArt], stroked: bool) -> Result<Self> {
        let mut shapes = std::iter::once(art)
            .chain(morphs)
            .enumerate()
            .map(|(index, svg)| {
                parse_art(svg).with_context(|| match index {
                    0 => "art".to_owned(),
                    n => format!("morph target {n}"),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if stroked {
            for path in shapes.iter_mut().flat_map(|art| &mut art.paths) {
                path.stroke.get_or_insert([1.0; 4]);
            }
        }
        let morphs = shapes
            .windows(2)
            .map(|pair| ArtMorph::new(&pair[0], &pair[1]))
            .collect();
        Ok(Self { shapes, morphs })
    }

    /// The artwork at `morph`: target `n` at whole `n`, matched blends between.
    pub(crate) fn at(&self, morph: f32) -> Cow<'_, Artwork> {
        let last = self.shapes.len() - 1;
        let morph = morph.clamp(0.0, last as f32);
        let index = (morph.floor() as usize).min(last);
        let blend = morph - index as f32;
        if blend <= 1e-4 || index == last {
            Cow::Borrowed(&self.shapes[index])
        } else if blend >= 1.0 - 1e-4 {
            Cow::Borrowed(&self.shapes[index + 1])
        } else {
            Cow::Owned(self.morphs[index].at(blend))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_paths_become_centered_cubic_subpaths_with_their_own_paint() {
        let art = parse_art(&SvgArt::new(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100" width="200" height="100">
                <path d="M0 0 H100 V100 Z M120 20 Q160 0 180 40" fill="#ff0000" stroke="#0000ff" stroke-width="4"/>
                <g opacity="0.5"><circle cx="150" cy="50" r="20" fill="#00ff00" fill-rule="evenodd"/></g>
            </svg>"##,
        ))
        .unwrap();
        assert_eq!(art.paths.len(), 2);
        let first = &art.paths[0];
        assert_eq!(first.subpaths.len(), 2);
        assert!(first.subpaths[0].closed && !first.subpaths[1].closed);
        assert_eq!(first.subpaths[0].curves.len(), 3);
        assert_eq!(first.subpaths[0].curves[0].start, vec2(-100.0, -50.0));
        assert_eq!(first.fill, Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(first.stroke, Some([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(first.width, 4.0);
        let circle = &art.paths[1];
        assert_eq!(circle.rule, FillRule::EvenOdd);
        assert_eq!(circle.fill.unwrap()[3], 0.5);
        assert!(circle.stroke.is_none());
        let area = circle.subpaths[0].signed_area().abs();
        assert!((area - std::f32::consts::PI * 400.0).abs() < 20.0, "{area}");
    }

    #[test]
    fn svgs_that_draw_nothing_are_rejected() {
        assert!(parse_art(&SvgArt::new("<svg")).is_err());
        assert!(
            parse_art(&SvgArt::new(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"></svg>"#
            ))
            .is_err()
        );
    }

    #[test]
    fn prepared_art_lands_on_each_target_and_blends_between() {
        let square = |size: f32| {
            SvgArt::new(format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {size} {size}"><rect width="{size}" height="{size}"/></svg>"#
            ))
        };
        let art = PreparedArt::new(&square(100.0), &[square(200.0)], true).unwrap();
        assert!(art.at(0.0).paths[0].stroke.is_some());
        let area = |m: f32| art.at(m).paths[0].subpaths[0].signed_area().abs();
        assert!((area(0.0) - 10_000.0).abs() < 1.0);
        assert!((area(1.0) - 40_000.0).abs() < 1.0);
        assert!((area(7.0) - 40_000.0).abs() < 1.0);
        assert!((area(0.5) - 22_500.0).abs() < 1.0);
    }
}
