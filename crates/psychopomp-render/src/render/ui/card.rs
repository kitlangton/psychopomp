use anyhow::{Result, bail};
use psychopomp::{
    footage::Treatment,
    math::{Vec2, shapes::polygon_distance},
};

use super::{
    super::bands::for_each_row_band, super::blend_pixel, super::theme::mix, Bounds,
    rounded_rect_distance,
};

const CAMERA_DISTANCE: f32 = 1_800.0;
const BYTES_PER_PIXEL: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UiColor([u8; 4]);

impl UiColor {
    pub const fn srgb8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self([red, green, blue, alpha])
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Fill {
    Solid(UiColor),
    Linear {
        from: [f32; 2],
        to: [f32; 2],
        start: UiColor,
        end: UiColor,
    },
    Radial {
        center: [f32; 2],
        radius: f32,
        inner: UiColor,
        outer: UiColor,
    },
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfaceStyle {
    pub fill: Fill,
    pub corner_radius: f32,
    border: Option<(f32, UiColor, f32)>,
}

impl SurfaceStyle {
    pub const fn new(fill: Fill, corner_radius: f32) -> Self {
        Self {
            fill,
            corner_radius,
            border: None,
        }
    }

    pub const fn border(mut self, width: f32, color: UiColor, opacity: f32) -> Self {
        self.border = Some((width, color, opacity));
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CardProjection {
    pub scale: f32,
    pub rotation_z: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub surface_blur: f32,
    pub near_edge_blur: f32,
}

impl Default for CardProjection {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotation_z: 0.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            surface_blur: 0.0,
            near_edge_blur: 0.0,
        }
    }
}

impl CardProjection {
    /// Where a card-local point (relative to the card center) lands, relative
    /// to the card's destination center: the compositor's forward mapping.
    pub(crate) fn project(self, point: [f32; 2]) -> [f32; 2] {
        CardTransform::new(self, [0.0; 2]).project(point)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CardStyle {
    pub material: Fill,
    pub corner_radius: f32,
    pub border_width: f32,
    pub border_color: UiColor,
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_opacity: f32,
}

impl CardStyle {
    pub const fn standard() -> Self {
        Self {
            material: Fill::Linear {
                from: [0.0, 0.0],
                to: [0.0, 760.0],
                start: UiColor::srgb8(14, 14, 17, 255),
                end: UiColor::srgb8(7, 7, 9, 255),
            },
            corner_radius: 28.0,
            border_width: 1.5,
            border_color: UiColor::srgb8(104, 104, 112, 220),
            shadow_offset: [0.0, 16.0],
            shadow_blur: 28.0,
            shadow_opacity: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CardFrame {
    pub bounds: Bounds,
    pub style: CardStyle,
    pub projection: CardProjection,
    pub opacity: f32,
}

/// The outline a card is cut to, about its center. Every card is
/// `Rounded` by its style's corner radius; footage may also be a circle or a
/// polygon (card-local pixels about the center), and its border and shadow
/// follow that outline.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CardShape<'a> {
    Rounded,
    Circle,
    Polygon(&'a [Vec2]),
}

impl CardShape<'_> {
    /// Signed distance (negative inside) from a card-local point.
    fn distance(self, local: [f32; 2], size: [f32; 2], radius: f32) -> f32 {
        match self {
            Self::Rounded => rounded_rect_distance(local, size, radius),
            Self::Circle => Vec2::from(local).length() - size[0].min(size[1]) * 0.5,
            Self::Polygon(points) => polygon_distance(Vec2::from(local), points),
        }
    }
}

/// A color treatment applied to source pixels before they meet the card's
/// material: the treatment and its tint color (sRGB, 0..1).
#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceTreatment {
    pub treatment: Treatment,
    pub tint: [f32; 3],
}

impl SourceTreatment {
    fn apply(self, color: [u8; 4]) -> [u8; 4] {
        let rgb = self.treatment.apply(
            std::array::from_fn(|i| f32::from(color[i]) / 255.0),
            self.tint,
        );
        [
            (rgb[0] * 255.0).round() as u8,
            (rgb[1] * 255.0).round() as u8,
            (rgb[2] * 255.0).round() as u8,
            color[3],
        ]
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Clip {
    pub bounds: Bounds,
    pub corner_radius: f32,
}

impl Clip {
    pub fn rounded(bounds: Bounds, corner_radius: f32) -> Self {
        Self {
            bounds,
            corner_radius,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ContentFit {
    Contain,
    /// The `source` window (fractional source pixels) stretched over the
    /// `content` rectangle (card-local, from the top-left); the rest of the
    /// card shows its material.
    Region {
        source: Bounds,
        content: Bounds,
    },
}

#[derive(Clone, Copy)]
pub(crate) struct RgbaSource<'a> {
    pixels: &'a [u8],
    size: [u32; 2],
    bytes_per_row: usize,
    origin: [u32; 2],
}

impl<'a> RgbaSource<'a> {
    pub fn packed(pixels: &'a [u8], size: [u32; 2]) -> Result<Self> {
        Self::strided_region(pixels, size, rgba_row_bytes(size[0])?, [0, 0], size)
    }

    pub fn strided_region(
        pixels: &'a [u8],
        surface_size: [u32; 2],
        bytes_per_row: usize,
        origin: [u32; 2],
        size: [u32; 2],
    ) -> Result<Self> {
        if size[0] == 0 || size[1] == 0 {
            bail!("RGBA source dimensions must be non-zero");
        }
        let region_end = [
            origin[0].checked_add(size[0]),
            origin[1].checked_add(size[1]),
        ];
        if region_end[0].is_none_or(|end| end > surface_size[0])
            || region_end[1].is_none_or(|end| end > surface_size[1])
        {
            bail!("RGBA source region exceeds its surface");
        }
        let minimum_stride = rgba_row_bytes(surface_size[0])?;
        if bytes_per_row < minimum_stride {
            bail!("RGBA source stride is smaller than its surface width");
        }
        let Some(required) = bytes_per_row.checked_mul(surface_size[1] as usize) else {
            bail!("RGBA source dimensions exceed addressable memory");
        };
        if pixels.len() < required {
            bail!(
                "RGBA source requires {required} bytes, received {}",
                pixels.len()
            );
        }
        Ok(Self {
            pixels,
            size,
            bytes_per_row,
            origin,
        })
    }

    fn sample(self, x: f32, y: f32) -> [u8; 4] {
        let x = x.clamp(0.0, self.size[0] as f32 - 1.0);
        let y = y.clamp(0.0, self.size[1] as f32 - 1.0);
        let left = x.floor() as i32;
        let top = y.floor() as i32;
        // Constant 2x2 neighborhoods have exactly the same filtered RGBA at every
        // fractional position. Flat card faces and transparent padding need no
        // premultiply/interpolate/unpremultiply work; edges keep the original filter.
        if x.is_finite()
            && y.is_finite()
            && left + 1 < self.size[0] as i32
            && top + 1 < self.size[1] as i32
        {
            let index = (self.origin[1] as usize + top as usize) * self.bytes_per_row
                + (self.origin[0] as usize + left as usize) * BYTES_PER_PIXEL;
            let below = index + self.bytes_per_row;
            let pixel = &self.pixels[index..index + 4];
            if pixel == &self.pixels[index + 4..index + 8]
                && pixel == &self.pixels[below..below + 4]
                && pixel == &self.pixels[below + 4..below + 8]
            {
                return if pixel[3] == 0 {
                    [0; 4]
                } else {
                    [pixel[0], pixel[1], pixel[2], pixel[3]]
                };
            }
        }
        let fraction_x = x - left as f32;
        let fraction_y = y - top as f32;
        let samples = [
            (left, top, (1.0 - fraction_x) * (1.0 - fraction_y)),
            (left + 1, top, fraction_x * (1.0 - fraction_y)),
            (left, top + 1, (1.0 - fraction_x) * fraction_y),
            (left + 1, top + 1, fraction_x * fraction_y),
        ];
        let mut alpha = 0.0;
        let mut premultiplied = [0.0; 3];
        for (sample_x, sample_y, weight) in samples {
            if !(0..self.size[0] as i32).contains(&sample_x)
                || !(0..self.size[1] as i32).contains(&sample_y)
            {
                continue;
            }
            let source_x = self.origin[0] as usize + sample_x as usize;
            let source_y = self.origin[1] as usize + sample_y as usize;
            let index = source_y * self.bytes_per_row + source_x * BYTES_PER_PIXEL;
            let sample_alpha = f32::from(self.pixels[index + 3]) / 255.0 * weight;
            alpha += sample_alpha;
            for (channel, sum) in premultiplied.iter_mut().enumerate() {
                *sum += f32::from(self.pixels[index + channel]) / 255.0 * sample_alpha;
            }
        }
        if alpha <= 0.0 {
            return [0; 4];
        }
        [
            (premultiplied[0] / alpha * 255.0).round() as u8,
            (premultiplied[1] / alpha * 255.0).round() as u8,
            (premultiplied[2] / alpha * 255.0).round() as u8,
            (alpha * 255.0).round() as u8,
        ]
    }
}

pub(crate) struct UiCanvas<'a> {
    pixels: &'a mut [u8],
    size: [u32; 2],
    clips: Vec<Clip>,
}

impl<'a> UiCanvas<'a> {
    pub(crate) fn new(pixels: &'a mut [u8], size: [u32; 2]) -> Self {
        Self {
            pixels,
            size,
            clips: Vec::new(),
        }
    }

    pub fn bounds(&self) -> Bounds {
        Bounds {
            origin: [0.0, 0.0],
            size: [self.size[0] as f32, self.size[1] as f32],
        }
    }

    pub fn clipped<R>(
        &mut self,
        clip: Clip,
        draw: impl FnOnce(&mut Self) -> Result<R>,
    ) -> Result<R> {
        self.clips.push(clip);
        let result = draw(self);
        self.clips.pop();
        result
    }

    pub fn fill(&mut self, bounds: Bounds, corner_radius: f32, fill: Fill, opacity: f32) {
        let min_x = bounds.origin[0].floor().max(0.0) as i32;
        let max_x = bounds.right().ceil().min(self.size[0] as f32) as i32;
        let min_y = bounds.origin[1].floor().max(0.0) as i32;
        let max_y = (bounds.bottom().ceil().min(self.size[1] as f32) as i32).max(0);
        let clips = self.clips.as_slice();
        let opacity = opacity.clamp(0.0, 1.0);
        let span = max_x - min_x;
        for_each_row_band(self.pixels, self.size, min_y, max_y - 1, span, |y, row| {
            for x in min_x..max_x {
                let point = [x as f32 + 0.5, y as f32 + 0.5];
                let coverage = rounded_coverage(point, bounds, corner_radius)
                    * clip_coverage(clips, point)
                    * opacity;
                if coverage <= 0.0 {
                    continue;
                }
                let color = fill.sample(point, bounds);
                let index = x as usize * BYTES_PER_PIXEL;
                blend_pixel(&mut row[index..index + 4], color.0, coverage);
            }
        });
    }

    pub fn surface(&mut self, bounds: Bounds, style: SurfaceStyle, opacity: f32) {
        self.fill(bounds, style.corner_radius, style.fill, opacity);
        if let Some((width, color, border_opacity)) = style.border {
            self.stroke(
                bounds,
                style.corner_radius,
                width,
                color,
                opacity * border_opacity,
            );
        }
    }

    pub fn stroke(
        &mut self,
        bounds: Bounds,
        corner_radius: f32,
        width: f32,
        color: UiColor,
        opacity: f32,
    ) {
        self.stroke_fill(bounds, corner_radius, width, Fill::Solid(color), opacity);
    }

    pub fn stroke_fill(
        &mut self,
        bounds: Bounds,
        corner_radius: f32,
        width: f32,
        fill: Fill,
        opacity: f32,
    ) {
        if width <= 0.0 {
            return;
        }
        let outer = bounds;
        let inner = bounds.inset(super::Edges::all(width));
        let inner_radius = (corner_radius - width).max(0.0);
        let min_x = outer.origin[0].floor().max(0.0) as i32;
        let max_x = outer.right().ceil().min(self.size[0] as f32) as i32;
        let min_y = outer.origin[1].floor().max(0.0) as i32;
        let max_y = (outer.bottom().ceil().min(self.size[1] as f32) as i32).max(0);
        let hollow_x = (
            (inner.origin[0] + inner_radius + 1.0).ceil() as i32,
            (inner.right() - inner_radius - 1.0).floor() as i32,
        );
        let hollow_y = (
            inner.origin[1] + inner_radius + 1.0,
            inner.bottom() - inner_radius - 1.0,
        );
        let clips = self.clips.as_slice();
        let opacity = opacity.clamp(0.0, 1.0);
        let span = max_x - min_x;
        for_each_row_band(self.pixels, self.size, min_y, max_y - 1, span, |y, row| {
            let py = y as f32 + 0.5;
            let skip_interior = hollow_x.0 < hollow_x.1 && py >= hollow_y.0 && py <= hollow_y.1;
            let mut x = min_x;
            while x < max_x {
                if skip_interior && x >= hollow_x.0 && x < hollow_x.1 {
                    x = hollow_x.1;
                    continue;
                }
                let point = [x as f32 + 0.5, py];
                let coverage = (rounded_coverage(point, outer, corner_radius)
                    - rounded_coverage(point, inner, inner_radius))
                .clamp(0.0, 1.0)
                    * clip_coverage(clips, point)
                    * opacity;
                if coverage > 0.0 {
                    let color = fill.sample(point, outer);
                    let index = x as usize * BYTES_PER_PIXEL;
                    blend_pixel(&mut row[index..index + 4], color.0, coverage);
                }
                x += 1;
            }
        });
    }

    /// A round-capped stroke through `points`. Segment coverage is unioned
    /// before blending, so joints do not darken where a curve is subdivided.
    pub fn polyline(&mut self, points: &[[f32; 2]], width: f32, color: UiColor, opacity: f32) {
        if points.len() < 2 || opacity <= 0. {
            return;
        }
        let pad = width * 0.5 + 1.;
        let min = std::array::from_fn::<_, 2, _>(|axis| {
            (points.iter().map(|p| p[axis]).fold(f32::INFINITY, f32::min) - pad)
                .floor()
                .max(0.) as u32
        });
        let max = std::array::from_fn::<_, 2, _>(|axis| {
            (points
                .iter()
                .map(|p| p[axis])
                .fold(f32::NEG_INFINITY, f32::max)
                + pad)
                .ceil()
                .max(0.)
                .min(self.size[axis] as f32) as u32
        });
        if min[0] >= max[0] || min[1] >= max[1] {
            return;
        }
        let stride = (max[0] - min[0]) as usize;
        let mut mask = vec![0_f32; stride * (max[1] - min[1]) as usize];
        for segment in points.windows(2) {
            let [a, b] = [segment[0], segment[1]];
            let d = [b[0] - a[0], b[1] - a[1]];
            let length = d[0] * d[0] + d[1] * d[1];
            let lo = std::array::from_fn::<_, 2, _>(|i| {
                (a[i].min(b[i]) - pad).floor().max(min[i] as f32) as u32
            });
            let hi = std::array::from_fn::<_, 2, _>(|i| {
                (a[i].max(b[i]) + pad).ceil().max(0.).min(max[i] as f32) as u32
            });
            for y in lo[1]..hi[1] {
                for x in lo[0]..hi[0] {
                    let p = [x as f32 + 0.5 - a[0], y as f32 + 0.5 - a[1]];
                    let t = if length > 0. {
                        ((p[0] * d[0] + p[1] * d[1]) / length).clamp(0., 1.)
                    } else {
                        0.
                    };
                    let distance = (p[0] - d[0] * t).hypot(p[1] - d[1] * t);
                    let coverage = (width * 0.5 + 0.5 - distance).clamp(0., 1.);
                    let i = (y - min[1]) as usize * stride + (x - min[0]) as usize;
                    mask[i] = mask[i].max(coverage);
                }
            }
        }
        for y in min[1]..max[1] {
            for x in min[0]..max[0] {
                let point = [x as f32 + 0.5, y as f32 + 0.5];
                let coverage = mask[(y - min[1]) as usize * stride + (x - min[0]) as usize]
                    * self.clip_coverage(point);
                if coverage > 0. {
                    let i = (y as usize * self.size[0] as usize + x as usize) * BYTES_PER_PIXEL;
                    blend_pixel(&mut self.pixels[i..i + 4], color.0, coverage * opacity);
                }
            }
        }
    }

    pub fn rgba(&mut self, bounds: Bounds, source: RgbaSource<'_>, fit: ContentFit, opacity: f32) {
        if let ContentFit::Region { .. } = fit {
            let min_x = bounds.origin[0].floor().max(0.0) as i32;
            let max_x = bounds.right().ceil().min(self.size[0] as f32) as i32;
            let min_y = bounds.origin[1].floor().max(0.0) as i32;
            let max_y = (bounds.bottom().ceil().min(self.size[1] as f32) as i32).max(0);
            let clips = self.clips.as_slice();
            let opacity = opacity.clamp(0.0, 1.0);
            let span = max_x - min_x;
            for_each_row_band(self.pixels, self.size, min_y, max_y - 1, span, |y, row| {
                for x in min_x..max_x {
                    let point = [x as f32 + 0.5, y as f32 + 0.5];
                    let local = [point[0] - bounds.center()[0], point[1] - bounds.center()[1]];
                    let Some([sx, sy]) = source_coordinates(local, bounds.size, source.size, fit)
                    else {
                        continue;
                    };
                    let coverage = clip_coverage(clips, point) * opacity;
                    if coverage <= 0.0 {
                        continue;
                    }
                    let index = x as usize * BYTES_PER_PIXEL;
                    blend_pixel(&mut row[index..index + 4], source.sample(sx, sy), coverage);
                }
            });
            return;
        }
        let source_size = [source.size[0] as f32, source.size[1] as f32];
        let scale = match fit {
            ContentFit::Contain => {
                let value = (bounds.size[0] / source_size[0]).min(bounds.size[1] / source_size[1]);
                [value, value]
            }
            ContentFit::Region { .. } => unreachable!("regions return above"),
        };
        let display_size = [source_size[0] * scale[0], source_size[1] * scale[1]];
        let display = Bounds::from_center(bounds.center(), display_size);
        let min_x = bounds.origin[0].floor().max(0.0) as i32;
        let max_x = bounds.right().ceil().min(self.size[0] as f32) as i32;
        let min_y = bounds.origin[1].floor().max(0.0) as i32;
        let max_y = (bounds.bottom().ceil().min(self.size[1] as f32) as i32).max(0);
        let clips = self.clips.as_slice();
        let opacity = opacity.clamp(0.0, 1.0);
        let span = max_x - min_x;
        for_each_row_band(self.pixels, self.size, min_y, max_y - 1, span, |y, row| {
            for x in min_x..max_x {
                let point = [x as f32 + 0.5, y as f32 + 0.5];
                if point[0] < display.origin[0]
                    || point[1] < display.origin[1]
                    || point[0] >= display.right()
                    || point[1] >= display.bottom()
                {
                    continue;
                }
                let coverage = clip_coverage(clips, point) * opacity;
                if coverage <= 0.0 {
                    continue;
                }
                let source_x =
                    (point[0] - display.origin[0]) / display.size[0] * source_size[0] - 0.5;
                let source_y =
                    (point[1] - display.origin[1]) / display.size[1] * source_size[1] - 0.5;
                let color = source.sample(source_x, source_y);
                let index = x as usize * BYTES_PER_PIXEL;
                blend_pixel(&mut row[index..index + 4], color, coverage);
            }
        });
    }

    fn clip_coverage(&self, point: [f32; 2]) -> f32 {
        clip_coverage(&self.clips, point)
    }
}

fn clip_coverage(clips: &[Clip], point: [f32; 2]) -> f32 {
    clips.iter().fold(1.0, |coverage, clip| {
        coverage * rounded_coverage(point, clip.bounds, clip.corner_radius)
    })
}

impl Fill {
    fn sample(self, point: [f32; 2], bounds: Bounds) -> UiColor {
        match self {
            Self::Solid(color) => color,
            Self::Linear {
                from,
                to,
                start,
                end,
            } => {
                let direction = [to[0] - from[0], to[1] - from[1]];
                let length_squared = direction[0] * direction[0] + direction[1] * direction[1];
                let offset = [point[0] - from[0], point[1] - from[1]];
                let progress = if length_squared <= f32::EPSILON {
                    0.0
                } else {
                    ((offset[0] * direction[0] + offset[1] * direction[1]) / length_squared)
                        .clamp(0.0, 1.0)
                };
                UiColor(mix(start.0, end.0, progress))
            }
            Self::Radial {
                center,
                radius,
                inner,
                outer,
            } => {
                let center = [bounds.origin[0] + center[0], bounds.origin[1] + center[1]];
                let distance = (point[0] - center[0]).hypot(point[1] - center[1]);
                UiColor(mix(
                    inner.0,
                    outer.0,
                    (distance / radius.max(0.001)).clamp(0.0, 1.0),
                ))
            }
        }
    }
}

pub(crate) struct CardUi<'a> {
    content: UiCanvas<'a>,
    overlay: UiCanvas<'a>,
}

impl CardUi<'_> {
    pub fn content<R>(&mut self, draw: impl FnOnce(&mut UiCanvas<'_>) -> Result<R>) -> Result<R> {
        draw(&mut self.content)
    }

    pub fn overlay<R>(&mut self, draw: impl FnOnce(&mut UiCanvas<'_>) -> Result<R>) -> Result<R> {
        draw(&mut self.overlay)
    }
}

pub(crate) struct FrameUi<'a> {
    canvas: UiCanvas<'a>,
}

impl<'a> FrameUi<'a> {
    pub fn new(pixels: &'a mut [u8], size: [u32; 2]) -> Result<Self> {
        let expected = rgba_byte_len(size)?;
        if pixels.len() != expected {
            bail!(
                "UI frame requires {expected} bytes, received {}",
                pixels.len()
            );
        }
        Ok(Self {
            canvas: UiCanvas::new(pixels, size),
        })
    }

    pub fn paint<R>(&mut self, draw: impl FnOnce(&mut UiCanvas<'_>) -> Result<R>) -> Result<R> {
        draw(&mut self.canvas)
    }

    pub fn card_source(
        &mut self,
        frame: CardFrame,
        source: RgbaSource<'_>,
        fit: ContentFit,
    ) -> Result<()> {
        validate_card(frame)?;
        composite_card_source(
            self.canvas.pixels,
            self.canvas.size,
            source,
            fit,
            frame,
            CardShape::Rounded,
            None,
        );
        Ok(())
    }

    /// `card_source` cut to `shape`, its source pixels color-treated.
    pub fn card_source_shaped(
        &mut self,
        frame: CardFrame,
        shape: CardShape<'_>,
        treatment: Option<SourceTreatment>,
        source: RgbaSource<'_>,
        fit: ContentFit,
    ) -> Result<()> {
        validate_card(frame)?;
        composite_card_source(
            self.canvas.pixels,
            self.canvas.size,
            source,
            fit,
            frame,
            shape,
            treatment,
        );
        Ok(())
    }

    /// Composite a card-local `source` layer over `region` of an already
    /// drawn card, through the same projection and rounded clip, without
    /// another shell or shadow.
    pub fn card_layer(
        &mut self,
        frame: CardFrame,
        source: RgbaSource<'_>,
        region: Bounds,
    ) -> Result<()> {
        validate_card(frame)?;
        composite_card_region(self.canvas.pixels, self.canvas.size, source, region, frame);
        Ok(())
    }

    pub fn card_painted(&mut self, frame: CardFrame, content: &[u8], overlay: &[u8]) -> Result<()> {
        validate_card(frame)?;
        let required = rgba_byte_len(card_local_size(frame))?;
        if content.len() != required || overlay.len() != required {
            bail!("painted card layers do not match the card size");
        }
        composite_card_layers(
            self.canvas.pixels,
            self.canvas.size,
            frame,
            content,
            overlay,
        );
        Ok(())
    }
}

fn composite_card_layers(
    destination: &mut [u8],
    destination_size: [u32; 2],
    frame: CardFrame,
    content: &[u8],
    overlay: &[u8],
) {
    let local_size = card_local_size(frame);
    composite_card_layer(
        destination,
        destination_size,
        content,
        local_size,
        frame,
        true,
    );
    composite_card_layer(
        destination,
        destination_size,
        overlay,
        local_size,
        frame,
        false,
    );
}

fn card_local_size(frame: CardFrame) -> [u32; 2] {
    [
        frame.bounds.size[0].ceil().max(1.0) as u32,
        frame.bounds.size[1].ceil().max(1.0) as u32,
    ]
}

pub(crate) fn paint_card_layers<R>(
    frame: CardFrame,
    content: &mut Vec<u8>,
    overlay: &mut Vec<u8>,
    draw: impl FnOnce(&mut CardUi<'_>) -> Result<R>,
) -> Result<R> {
    validate_card(frame)?;
    let local_size = card_local_size(frame);
    let required = rgba_byte_len(local_size)?;
    content.resize(required, 0);
    content.fill(0);
    overlay.resize(required, 0);
    overlay.fill(0);
    let mut card = CardUi {
        content: UiCanvas::new(content, local_size),
        overlay: UiCanvas::new(overlay, local_size),
    };
    card.content.fill(
        card.content.bounds(),
        frame.style.corner_radius,
        frame.style.material,
        1.0,
    );
    draw(&mut card)
}

fn rgba_row_bytes(width: u32) -> Result<usize> {
    let Some(bytes) = (width as usize).checked_mul(BYTES_PER_PIXEL) else {
        bail!("RGBA row width exceeds addressable memory");
    };
    Ok(bytes)
}

fn rgba_byte_len(size: [u32; 2]) -> Result<usize> {
    let Some(bytes) = rgba_row_bytes(size[0])?.checked_mul(size[1] as usize) else {
        bail!("RGBA dimensions exceed addressable memory");
    };
    Ok(bytes)
}

fn validate_card(frame: CardFrame) -> Result<()> {
    let values = [
        frame.bounds.origin[0],
        frame.bounds.origin[1],
        frame.bounds.size[0],
        frame.bounds.size[1],
        frame.projection.scale,
        frame.projection.rotation_z,
        frame.projection.tilt_x,
        frame.projection.tilt_y,
        frame.projection.surface_blur,
        frame.projection.near_edge_blur,
        frame.opacity,
    ];
    if values.into_iter().any(|value| !value.is_finite()) {
        bail!("card values must be finite");
    }
    if frame.bounds.size[0] <= 0.0 || frame.bounds.size[1] <= 0.0 {
        bail!("card dimensions must be positive");
    }
    if frame.projection.scale <= 0.0 {
        bail!("card scale must be positive");
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct CardTransform {
    forward: [[f32; 3]; 3],
    inverse: [[f32; 3]; 3],
    depth: [f32; 2],
    max_near_depth: f32,
}

impl CardTransform {
    fn new(projection: CardProjection, half_size: [f32; 2]) -> Self {
        let (sine_x, cosine_x) = projection.tilt_x.sin_cos();
        let (sine_y, cosine_y) = projection.tilt_y.sin_cos();
        let (sine_z, cosine_z) = projection.rotation_z.sin_cos();
        let a = cosine_z * cosine_y;
        let b = cosine_z * sine_y * sine_x - sine_z * cosine_x;
        let c = sine_z * cosine_y;
        let d = sine_z * sine_y * sine_x + cosine_z * cosine_x;
        let depth = [-sine_y, cosine_y * sine_x];
        let forward = [
            [projection.scale * a, projection.scale * b, 0.0],
            [projection.scale * c, projection.scale * d, 0.0],
            [
                -depth[0] / CAMERA_DISTANCE,
                -depth[1] / CAMERA_DISTANCE,
                1.0,
            ],
        ];
        let inverse = invert_matrix_3x3(forward);
        let max_near_depth = card_corners(half_size)
            .into_iter()
            .map(|[x, y]| depth[0] * x + depth[1] * y)
            .fold(0.0_f32, f32::max);
        Self {
            forward,
            inverse,
            depth,
            max_near_depth,
        }
    }

    fn project(self, point: [f32; 2]) -> [f32; 2] {
        let projected = multiply_matrix_point(self.forward, point);
        [projected[0] / projected[2], projected[1] / projected[2]]
    }

    fn unproject(self, point: [f32; 2]) -> [f32; 2] {
        let local = multiply_matrix_point(self.inverse, point);
        [local[0] / local[2], local[1] / local[2]]
    }
}

fn composite_card_layer(
    destination: &mut [u8],
    destination_size: [u32; 2],
    source: &[u8],
    source_size: [u32; 2],
    frame: CardFrame,
    shell: bool,
) {
    let center = frame.bounds.center();
    let half_size = [frame.bounds.size[0] * 0.5, frame.bounds.size[1] * 0.5];
    let transform = CardTransform::new(frame.projection, half_size);
    let projected = card_corners(half_size).map(|corner| transform.project(corner));
    let padding = if shell {
        frame.style.shadow_blur * 3.0
            + frame.style.shadow_offset[0]
                .abs()
                .max(frame.style.shadow_offset[1].abs())
    } else {
        2.0
    };
    let min_x = projected
        .iter()
        .map(|point| center[0] + point[0])
        .fold(f32::INFINITY, f32::min)
        - padding;
    let max_x = projected
        .iter()
        .map(|point| center[0] + point[0])
        .fold(f32::NEG_INFINITY, f32::max)
        + padding;
    let min_y = projected
        .iter()
        .map(|point| center[1] + point[1])
        .fold(f32::INFINITY, f32::min)
        - padding;
    let max_y = projected
        .iter()
        .map(|point| center[1] + point[1])
        .fold(f32::NEG_INFINITY, f32::max)
        + padding;
    let min_x = min_x.floor().max(0.0) as i32;
    let max_x = max_x.ceil().min(destination_size[0] as f32 - 1.0) as i32;
    let min_y = min_y.floor().max(0.0) as i32;
    let max_y = max_y.ceil().min(destination_size[1] as f32 - 1.0) as i32;
    for_each_row_band(
        destination,
        destination_size,
        min_y,
        max_y,
        max_x - min_x + 1,
        |target_y, row| {
            for target_x in min_x..=max_x {
                let point = [
                    target_x as f32 + 0.5 - center[0],
                    target_y as f32 + 0.5 - center[1],
                ];
                let local = transform.unproject(point);
                let distance =
                    rounded_rect_distance(local, frame.bounds.size, frame.style.corner_radius);
                let target = target_x as usize * BYTES_PER_PIXEL;
                if shell && distance > 0.0 {
                    composite_card_shadow(row, target, transform, point, frame, CardShape::Rounded);
                    continue;
                }
                let outside = if shell {
                    distance > 0.0
                } else {
                    local[0].abs() > half_size[0] || local[1].abs() > half_size[1]
                };
                if outside {
                    continue;
                }
                let source_x =
                    (local[0] / frame.bounds.size[0] + 0.5) * source_size[0] as f32 - 0.5;
                let source_y =
                    (local[1] / frame.bounds.size[1] + 0.5) * source_size[1] as f32 - 0.5;
                let depth = transform.depth[0] * local[0] + transform.depth[1] * local[1];
                let proximity = if transform.max_near_depth > 0.001 {
                    (depth / transform.max_near_depth).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let blur = frame.projection.surface_blur.max(0.0)
                    + frame.projection.near_edge_blur.max(0.0) * proximity;
                let color = sample_layer_blurred(source, source_size, source_x, source_y, blur);
                if color[3] > 0 {
                    let coverage = if shell {
                        (-distance).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    blend_pixel(
                        &mut row[target..target + 4],
                        color,
                        coverage * frame.opacity,
                    );
                }
                if shell && frame.style.border_width > 0.0 && -distance <= frame.style.border_width
                {
                    blend_pixel(
                        &mut row[target..target + 4],
                        frame.style.border_color.0,
                        frame.opacity,
                    );
                }
            }
        },
    );
}

fn composite_card_source(
    destination: &mut [u8],
    destination_size: [u32; 2],
    source: RgbaSource<'_>,
    fit: ContentFit,
    frame: CardFrame,
    shape: CardShape<'_>,
    treatment: Option<SourceTreatment>,
) {
    let center = frame.bounds.center();
    let half_size = [frame.bounds.size[0] * 0.5, frame.bounds.size[1] * 0.5];
    let transform = CardTransform::new(frame.projection, half_size);
    let projected = card_corners(half_size).map(|corner| transform.project(corner));
    let padding = frame.style.shadow_blur * 3.0
        + frame.style.shadow_offset[0]
            .abs()
            .max(frame.style.shadow_offset[1].abs());
    let min_x = projected
        .iter()
        .map(|point| center[0] + point[0])
        .fold(f32::INFINITY, f32::min)
        - padding;
    let max_x = projected
        .iter()
        .map(|point| center[0] + point[0])
        .fold(f32::NEG_INFINITY, f32::max)
        + padding;
    let min_y = projected
        .iter()
        .map(|point| center[1] + point[1])
        .fold(f32::INFINITY, f32::min)
        - padding;
    let max_y = projected
        .iter()
        .map(|point| center[1] + point[1])
        .fold(f32::NEG_INFINITY, f32::max)
        + padding;
    let local_bounds = Bounds {
        origin: [0.0, 0.0],
        size: frame.bounds.size,
    };
    let min_x = min_x.floor().max(0.0) as i32;
    let max_x = max_x.ceil().min(destination_size[0] as f32 - 1.0) as i32;
    let min_y = min_y.floor().max(0.0) as i32;
    let max_y = max_y.ceil().min(destination_size[1] as f32 - 1.0) as i32;
    for_each_row_band(
        destination,
        destination_size,
        min_y,
        max_y,
        max_x - min_x + 1,
        |target_y, row| {
            for target_x in min_x..=max_x {
                let point = [
                    target_x as f32 + 0.5 - center[0],
                    target_y as f32 + 0.5 - center[1],
                ];
                let local = transform.unproject(point);
                let distance = shape.distance(local, frame.bounds.size, frame.style.corner_radius);
                let target = target_x as usize * BYTES_PER_PIXEL;
                if distance > 0.0 {
                    composite_card_shadow(row, target, transform, point, frame, shape);
                    continue;
                }
                let depth = transform.depth[0] * local[0] + transform.depth[1] * local[1];
                let proximity = if transform.max_near_depth > 0.001 {
                    (depth / transform.max_near_depth).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let blur = frame.projection.surface_blur.max(0.0)
                    + frame.projection.near_edge_blur.max(0.0) * proximity;
                let source_color = source_coordinates(local, frame.bounds.size, source.size, fit)
                    .map(|[source_x, source_y]| {
                        let color = sample_source_blurred(source, source_x, source_y, blur);
                        treatment.map_or(color, |treatment| treatment.apply(color))
                    });
                let color = match source_color {
                    Some(color) if color[3] == 255 => color,
                    source_color => {
                        let material_point = [local[0] + half_size[0], local[1] + half_size[1]];
                        let material = frame.style.material.sample(material_point, local_bounds).0;
                        source_color.map_or(material, |color| over_pixel(material, color))
                    }
                };
                blend_pixel(
                    &mut row[target..target + 4],
                    color,
                    (-distance).clamp(0.0, 1.0) * frame.opacity,
                );
                if frame.style.border_width > 0.0 && -distance <= frame.style.border_width {
                    blend_pixel(
                        &mut row[target..target + 4],
                        frame.style.border_color.0,
                        frame.opacity,
                    );
                }
            }
        },
    );
}

fn composite_card_region(
    destination: &mut [u8],
    destination_size: [u32; 2],
    source: RgbaSource<'_>,
    region: Bounds,
    frame: CardFrame,
) {
    let center = frame.bounds.center();
    let half_size = [frame.bounds.size[0] * 0.5, frame.bounds.size[1] * 0.5];
    let transform = CardTransform::new(frame.projection, half_size);
    let local_region = region.translate([-half_size[0], -half_size[1]]);
    let corners = [
        [local_region.origin[0], local_region.origin[1]],
        [local_region.right(), local_region.origin[1]],
        [local_region.right(), local_region.bottom()],
        [local_region.origin[0], local_region.bottom()],
    ]
    .map(|corner| transform.project(corner));
    let min_x = corners
        .iter()
        .map(|p| center[0] + p[0])
        .fold(f32::INFINITY, f32::min)
        - 2.0;
    let max_x = corners
        .iter()
        .map(|p| center[0] + p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        + 2.0;
    let min_y = corners
        .iter()
        .map(|p| center[1] + p[1])
        .fold(f32::INFINITY, f32::min)
        - 2.0;
    let max_y = corners
        .iter()
        .map(|p| center[1] + p[1])
        .fold(f32::NEG_INFINITY, f32::max)
        + 2.0;
    for target_y in
        min_y.floor().max(0.0) as i32..=max_y.ceil().min(destination_size[1] as f32 - 1.0) as i32
    {
        for target_x in min_x.floor().max(0.0) as i32
            ..=max_x.ceil().min(destination_size[0] as f32 - 1.0) as i32
        {
            let point = [
                target_x as f32 + 0.5 - center[0],
                target_y as f32 + 0.5 - center[1],
            ];
            let local = transform.unproject(point);
            let inside = [
                local[0] - local_region.origin[0],
                local[1] - local_region.origin[1],
            ];
            if inside[0] < 0.0
                || inside[1] < 0.0
                || inside[0] > region.size[0]
                || inside[1] > region.size[1]
            {
                continue;
            }
            let distance =
                rounded_rect_distance(local, frame.bounds.size, frame.style.corner_radius);
            let coverage = (-distance).clamp(0.0, 1.0) * frame.opacity;
            if coverage <= 0.0 {
                continue;
            }
            let depth = transform.depth[0] * local[0] + transform.depth[1] * local[1];
            let proximity = if transform.max_near_depth > 0.001 {
                (depth / transform.max_near_depth).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let blur = frame.projection.surface_blur.max(0.0)
                + frame.projection.near_edge_blur.max(0.0) * proximity;
            let color = sample_source_blurred(
                source,
                inside[0] / region.size[0] * source.size[0] as f32 - 0.5,
                inside[1] / region.size[1] * source.size[1] as f32 - 0.5,
                blur,
            );
            let target = (target_y as usize * destination_size[0] as usize + target_x as usize)
                * BYTES_PER_PIXEL;
            blend_pixel(&mut destination[target..target + 4], color, coverage);
        }
    }
}

fn composite_card_shadow(
    destination: &mut [u8],
    target: usize,
    transform: CardTransform,
    point: [f32; 2],
    frame: CardFrame,
    shape: CardShape<'_>,
) {
    let shadow_local = transform.unproject([
        point[0] - frame.style.shadow_offset[0],
        point[1] - frame.style.shadow_offset[1],
    ]);
    let shadow_distance =
        shape.distance(shadow_local, frame.bounds.size, frame.style.corner_radius);
    if shadow_distance >= frame.style.shadow_blur * 3.0 {
        return;
    }
    let shadow = (-shadow_distance.max(0.0).powi(2)
        / (2.0 * frame.style.shadow_blur.max(0.001).powi(2)))
    .exp()
        * frame.style.shadow_opacity;
    blend_pixel(
        &mut destination[target..target + 4],
        [0, 0, 0, 255],
        shadow * frame.opacity,
    );
}

fn source_coordinates(
    local: [f32; 2],
    card_size: [f32; 2],
    source_size: [u32; 2],
    fit: ContentFit,
) -> Option<[f32; 2]> {
    if let ContentFit::Region { source, content } = fit {
        let inside = [
            local[0] + card_size[0] * 0.5 - content.origin[0],
            local[1] + card_size[1] * 0.5 - content.origin[1],
        ];
        if inside[0] < 0.0
            || inside[1] < 0.0
            || inside[0] > content.size[0]
            || inside[1] > content.size[1]
        {
            return None;
        }
        return Some([
            source.origin[0] + inside[0] / content.size[0] * source.size[0] - 0.5,
            source.origin[1] + inside[1] / content.size[1] * source.size[1] - 0.5,
        ]);
    }
    let source_size = [source_size[0] as f32, source_size[1] as f32];
    let scale = match fit {
        ContentFit::Contain => {
            let value = (card_size[0] / source_size[0]).min(card_size[1] / source_size[1]);
            [value, value]
        }
        ContentFit::Region { .. } => unreachable!("regions return above"),
    };
    let display_size = [source_size[0] * scale[0], source_size[1] * scale[1]];
    if local[0].abs() > display_size[0] * 0.5 || local[1].abs() > display_size[1] * 0.5 {
        return None;
    }
    Some([
        (local[0] / display_size[0] + 0.5) * source_size[0] - 0.5,
        (local[1] / display_size[1] + 0.5) * source_size[1] - 0.5,
    ])
}

fn sample_source_blurred(source: RgbaSource<'_>, x: f32, y: f32, blur: f32) -> [u8; 4] {
    if blur <= 0.2 {
        return source.sample(x, y);
    }
    let radius = blur * 0.72;
    let axis = [-radius, 0.0, radius];
    let weights = [1.0, 2.0, 1.0];
    let mut alpha = 0.0;
    let mut premultiplied = [0.0; 3];
    for (offset_y, weight_y) in axis.into_iter().zip(weights) {
        for (offset_x, weight_x) in axis.into_iter().zip(weights) {
            let sample = source.sample(x + offset_x, y + offset_y);
            if sample[3] == 0 {
                continue;
            }
            let weight = weight_x * weight_y / 16.0;
            let sample_alpha = f32::from(sample[3]) / 255.0 * weight;
            alpha += sample_alpha;
            for (channel, sum) in premultiplied.iter_mut().enumerate() {
                *sum += f32::from(sample[channel]) / 255.0 * sample_alpha;
            }
        }
    }
    if alpha <= 0.0 {
        return [0; 4];
    }
    [
        (premultiplied[0] / alpha * 255.0).round() as u8,
        (premultiplied[1] / alpha * 255.0).round() as u8,
        (premultiplied[2] / alpha * 255.0).round() as u8,
        (alpha * 255.0).round() as u8,
    ]
}

fn over_pixel(background: [u8; 4], foreground: [u8; 4]) -> [u8; 4] {
    let mut output = background;
    blend_pixel(&mut output, foreground, 1.0);
    output
}

fn sample_layer_blurred(pixels: &[u8], size: [u32; 2], x: f32, y: f32, blur: f32) -> [u8; 4] {
    let source = RgbaSource {
        pixels,
        size,
        bytes_per_row: size[0] as usize * BYTES_PER_PIXEL,
        origin: [0, 0],
    };
    sample_source_blurred(source, x, y, blur)
}

fn rounded_coverage(point: [f32; 2], bounds: Bounds, radius: f32) -> f32 {
    let local = [point[0] - bounds.center()[0], point[1] - bounds.center()[1]];
    (0.75 - rounded_rect_distance(local, bounds.size, radius)).clamp(0.0, 1.0)
}

fn card_corners([half_width, half_height]: [f32; 2]) -> [[f32; 2]; 4] {
    [
        [-half_width, -half_height],
        [half_width, -half_height],
        [half_width, half_height],
        [-half_width, half_height],
    ]
}

fn multiply_matrix_point(matrix: [[f32; 3]; 3], point: [f32; 2]) -> [f32; 3] {
    let vector = [point[0], point[1], 1.0];
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

fn invert_matrix_3x3(matrix: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let [a, b, c] = matrix;
    let determinant = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0]);
    let inverse_determinant = determinant.recip();
    [
        [
            (b[1] * c[2] - b[2] * c[1]) * inverse_determinant,
            (a[2] * c[1] - a[1] * c[2]) * inverse_determinant,
            (a[1] * b[2] - a[2] * b[1]) * inverse_determinant,
        ],
        [
            (b[2] * c[0] - b[0] * c[2]) * inverse_determinant,
            (a[0] * c[2] - a[2] * c[0]) * inverse_determinant,
            (a[2] * b[0] - a[0] * b[2]) * inverse_determinant,
        ],
        [
            (b[0] * c[1] - b[1] * c[0]) * inverse_determinant,
            (a[1] * c[0] - a[0] * c[1]) * inverse_determinant,
            (a[0] * b[1] - a[1] * b[0]) * inverse_determinant,
        ],
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        Bounds, CardFrame, CardProjection, CardStyle, CardTransform, CardUi, Clip, ContentFit,
        Fill, FrameUi, RgbaSource, SurfaceStyle, UiCanvas, UiColor, paint_card_layers,
    };
    use anyhow::Result;

    #[test]
    fn fills_with_an_infinite_bottom_paint_nothing() {
        let size = [4_u32, 6];
        let mut pixels = vec![0_u8; 4 * 6 * 4];
        let mut canvas = UiCanvas::new(&mut pixels, size);
        let solid = Fill::Solid(UiColor::srgb8(255, 255, 255, 255));
        for bounds in [
            Bounds {
                origin: [0.0, f32::NEG_INFINITY],
                size: [4.0, 0.0],
            },
            Bounds {
                origin: [0.0, -10.0],
                size: [4.0, f32::NEG_INFINITY],
            },
        ] {
            canvas.fill(bounds, 0.0, solid, 1.0);
            canvas.stroke_fill(bounds, 0.0, 1.0, solid, 1.0);
        }
        assert!(pixels.iter().all(|&byte| byte == 0));
    }

    fn card(
        ui: &mut FrameUi<'_>,
        frame: CardFrame,
        draw: impl FnOnce(&mut CardUi<'_>) -> Result<()>,
    ) -> Result<()> {
        let (mut content, mut overlay) = (Vec::new(), Vec::new());
        paint_card_layers(frame, &mut content, &mut overlay, draw)?;
        ui.card_painted(frame, &content, &overlay)
    }

    #[test]
    fn source_sampling_matches_filtered_reference_in_flat_and_mixed_regions() {
        // Preserve the original premultiplied bilinear calculation as the oracle,
        // including transparent RGB, edge clamping and strided region addressing.
        fn reference(source: RgbaSource<'_>, x: f32, y: f32) -> [u8; 4] {
            let x = x.clamp(0., source.size[0] as f32 - 1.);
            let y = y.clamp(0., source.size[1] as f32 - 1.);
            let left = x.floor() as i32;
            let top = y.floor() as i32;
            let fx = x - left as f32;
            let fy = y - top as f32;
            let mut alpha = 0.;
            let mut rgb = [0.; 3];
            for (x, y, weight) in [
                (left, top, (1. - fx) * (1. - fy)),
                (left + 1, top, fx * (1. - fy)),
                (left, top + 1, (1. - fx) * fy),
                (left + 1, top + 1, fx * fy),
            ] {
                if !(0..source.size[0] as i32).contains(&x)
                    || !(0..source.size[1] as i32).contains(&y)
                {
                    continue;
                }
                let index = (source.origin[1] as usize + y as usize) * source.bytes_per_row
                    + (source.origin[0] as usize + x as usize) * 4;
                let a = source.pixels[index + 3] as f32 / 255. * weight;
                alpha += a;
                for (channel, sum) in rgb.iter_mut().enumerate() {
                    *sum += source.pixels[index + channel] as f32 / 255. * a;
                }
            }
            if alpha <= 0. {
                return [0; 4];
            }
            [
                (rgb[0] / alpha * 255.).round() as u8,
                (rgb[1] / alpha * 255.).round() as u8,
                (rgb[2] / alpha * 255.).round() as u8,
                (alpha * 255.).round() as u8,
            ]
        }
        fn blurred(source: RgbaSource<'_>, x: f32, y: f32, blur: f32) -> [u8; 4] {
            if blur <= 0.2 {
                return reference(source, x, y);
            }
            let radius = blur * 0.72;
            let mut alpha = 0.;
            let mut rgb = [0.; 3];
            for (oy, wy) in [-radius, 0., radius].into_iter().zip([1., 2., 1.]) {
                for (ox, wx) in [-radius, 0., radius].into_iter().zip([1., 2., 1.]) {
                    let sample = reference(source, x + ox, y + oy);
                    let a = sample[3] as f32 / 255. * (wx * wy / 16.);
                    alpha += a;
                    for (channel, sum) in rgb.iter_mut().enumerate() {
                        *sum += sample[channel] as f32 / 255. * a;
                    }
                }
            }
            if alpha <= 0. {
                return [0; 4];
            }
            [
                (rgb[0] / alpha * 255.).round() as u8,
                (rgb[1] / alpha * 255.).round() as u8,
                (rgb[2] / alpha * 255.).round() as u8,
                (alpha * 255.).round() as u8,
            ]
        }
        let positions = [
            -2., 0., 0.00001, 0.1, 0.49999, 0.5, 0.71317, 0.99999, 1., 1.9, 2., 3.,
        ];
        for byte in 0..=255u8 {
            for alpha in [0, 1, 17, 127, 254, 255] {
                let pixels = [byte, 255 - byte, byte.wrapping_mul(7), alpha].repeat(9);
                let source = RgbaSource::packed(&pixels, [3, 3]).unwrap();
                for x in positions {
                    for y in positions {
                        assert_eq!(
                            source.sample(x, y),
                            reference(source, x, y),
                            "flat {byte}/{alpha} at {x},{y}"
                        );
                    }
                }
                if byte % 17 == 0 {
                    for blur in [0., 0.2, 0.20001, 0.5, 4., 12., 16.] {
                        assert_eq!(
                            super::sample_source_blurred(source, 0.37, 0.9, blur),
                            blurred(source, 0.37, 0.9, blur)
                        );
                    }
                }
            }
        }
        let mut seed = 0x471ac31u32;
        for _ in 0..80 {
            let mut pixels = vec![0; 7 * 5 * 4];
            for byte in &mut pixels {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *byte = (seed >> 24) as u8;
            }
            let source =
                RgbaSource::strided_region(&pixels, [6, 5], 7 * 4, [2, 1], [3, 3]).unwrap();
            for x in positions {
                for y in positions {
                    assert_eq!(
                        source.sample(x, y),
                        reference(source, x, y),
                        "strided {x},{y}"
                    );
                    for blur in [0.2, 0.20001, 0.5, 4., 12.] {
                        assert_eq!(
                            super::sample_source_blurred(source, x, y, blur),
                            blurred(source, x, y, blur),
                            "blur {blur} at {x},{y}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn perspective_transform_round_trips_points() {
        let transform = CardTransform::new(
            CardProjection {
                scale: 0.91,
                rotation_z: -0.08,
                tilt_x: 0.14,
                tilt_y: -0.12,
                surface_blur: 0.0,
                near_edge_blur: 0.0,
            },
            [700.0, 400.0],
        );
        let point = [340.0, -170.0];
        let restored = transform.unproject(transform.project(point));

        assert!((restored[0] - point[0]).abs() < 0.001);
        assert!((restored[1] - point[1]).abs() < 0.001);
    }

    #[test]
    fn nested_clips_constrain_composed_rgba_content() {
        let mut output = vec![0; 8 * 8 * 4];
        let source = [255, 255, 255, 255];
        let source = RgbaSource::packed(&source, [1, 1]).unwrap();
        let mut frame = FrameUi::new(&mut output, [8, 8]).unwrap();
        frame
            .paint(|ui| {
                ui.clipped(
                    Clip::rounded(Bounds::from_center([4.0, 4.0], [4.0, 4.0]), 0.0),
                    |ui| {
                        ui.rgba(ui.bounds(), source, ContentFit::Contain, 1.0);
                        Ok(())
                    },
                )
            })
            .unwrap();

        assert_eq!(&output[(4 * 8 + 4) * 4..(4 * 8 + 4) * 4 + 4], &[255; 4]);
        assert_eq!(&output[0..4], &[0; 4]);
    }

    #[test]
    fn surfaces_compose_fill_and_border_as_one_operation() {
        let mut output = vec![0; 8 * 8 * 4];
        let mut canvas = UiCanvas::new(&mut output, [8, 8]);
        canvas.surface(
            Bounds {
                origin: [1.0, 1.0],
                size: [6.0, 6.0],
            },
            SurfaceStyle::new(Fill::Solid(UiColor::srgb8(220, 20, 20, 255)), 0.0).border(
                1.0,
                UiColor::srgb8(20, 220, 20, 255),
                1.0,
            ),
            1.0,
        );

        assert!(
            output
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[0] > pixel[1])
        );
        assert!(
            output
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[1] > pixel[0])
        );
    }

    #[test]
    fn gradient_strokes_support_subtle_top_rim_lighting() {
        let mut output = vec![0; 10 * 10 * 4];
        let mut canvas = UiCanvas::new(&mut output, [10, 10]);
        canvas.stroke_fill(
            Bounds {
                origin: [1.0, 1.0],
                size: [8.0, 8.0],
            },
            0.0,
            1.0,
            Fill::Linear {
                from: [0.0, 1.0],
                to: [0.0, 9.0],
                start: UiColor::srgb8(255, 255, 255, 80),
                end: UiColor::srgb8(255, 255, 255, 8),
            },
            1.0,
        );

        let top_alpha = output[(10 + 5) * 4 + 3];
        let bottom_alpha = output[(8 * 10 + 5) * 4 + 3];
        assert!(top_alpha > bottom_alpha);
    }

    #[test]
    fn projected_card_composes_material_content_border_and_shadow() {
        let mut output = vec![0; 32 * 24 * 4];
        let source = [180, 90, 30, 255];
        let source = RgbaSource::packed(&source, [1, 1]).unwrap();
        let mut frame = FrameUi::new(&mut output, [32, 24]).unwrap();
        card(
            &mut frame,
            CardFrame {
                bounds: Bounds::from_center([16.0, 11.0], [16.0, 10.0]),
                style: CardStyle::standard(),
                projection: CardProjection {
                    scale: 0.9,
                    rotation_z: -0.08,
                    tilt_x: -0.12,
                    tilt_y: 0.16,
                    surface_blur: 0.0,
                    near_edge_blur: 1.5,
                },
                opacity: 1.0,
            },
            |card| {
                card.content(|ui| {
                    ui.rgba(ui.bounds(), source, ContentFit::Contain, 1.0);
                    Ok(())
                })
            },
        )
        .unwrap();

        assert!(output.as_chunks::<4>().0.iter().any(|pixel| pixel[0] > 80));
        assert!(output.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
        let _ = UiColor::srgb8(1, 2, 3, 4);
    }

    #[test]
    fn borrowed_card_source_samples_a_strided_region() {
        let mut output = vec![0; 16 * 16 * 4];
        let mut surface = vec![0; 4 * 2 * 4];
        for y in 0..2 {
            for x in 0..4 {
                let index = (y * 4 + x) * 4;
                surface[index..index + 4].copy_from_slice(if x < 2 {
                    &[220, 20, 20, 255]
                } else {
                    &[20, 220, 20, 255]
                });
            }
        }
        let source = RgbaSource::strided_region(&surface, [4, 2], 16, [2, 0], [2, 2]).unwrap();
        let mut frame = FrameUi::new(&mut output, [16, 16]).unwrap();
        frame
            .card_source(
                CardFrame {
                    bounds: Bounds::from_center([8.0, 8.0], [8.0, 8.0]),
                    style: CardStyle {
                        material: super::Fill::Solid(UiColor::srgb8(0, 0, 0, 0)),
                        corner_radius: 0.0,
                        border_width: 0.0,
                        border_color: UiColor::srgb8(0, 0, 0, 0),
                        shadow_offset: [0.0, 0.0],
                        shadow_blur: 0.0,
                        shadow_opacity: 0.0,
                    },
                    projection: CardProjection::default(),
                    opacity: 1.0,
                },
                source,
                ContentFit::Contain,
            )
            .unwrap();

        assert_eq!(
            &output[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4],
            &[20, 220, 20, 255]
        );
    }

    #[test]
    fn zero_width_border_never_paints_transparent_card_bounds() {
        for borrowed in [false, true] {
            for width in [7., 7.25, 8.] {
                let mut output = vec![0; 16 * 16 * 4];
                let style = CardStyle {
                    material: Fill::Solid(UiColor::srgb8(0, 0, 0, 0)),
                    corner_radius: 0.,
                    border_width: 0.,
                    border_color: UiColor::srgb8(255, 0, 0, 255),
                    shadow_offset: [0.; 2],
                    shadow_blur: 0.,
                    shadow_opacity: 0.,
                };
                let frame = CardFrame {
                    bounds: Bounds::from_center([8., 8.], [width, 7.]),
                    style,
                    projection: CardProjection::default(),
                    opacity: 1.,
                };
                let mut ui = FrameUi::new(&mut output, [16, 16]).unwrap();
                if borrowed {
                    ui.card_source(
                        frame,
                        RgbaSource::packed(&[0; 4], [1, 1]).unwrap(),
                        ContentFit::Contain,
                    )
                    .unwrap();
                } else {
                    card(&mut ui, frame, |_| Ok(())).unwrap();
                }
                assert!(
                    output.iter().all(|v| *v == 0),
                    "zero-width border painted a pixel: borrowed={borrowed}, width={width}"
                );
            }
        }
    }

    #[test]
    fn rgba_sources_reject_overflowing_regions_and_dimensions() {
        assert!(RgbaSource::packed(&[], [u32::MAX, u32::MAX]).is_err());
        assert!(
            RgbaSource::strided_region(&[], [u32::MAX, 1], usize::MAX, [u32::MAX, 0], [1, 1])
                .is_err()
        );
    }
}
