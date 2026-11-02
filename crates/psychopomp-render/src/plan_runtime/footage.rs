//! Prepared footage. Video Cards, images, and footage overlays are one recipe
//! here: each decodes into a [`FootagePlan`] (a Video Card is a framed
//! rectangle whose trim is its placement's source range; an image, a bare or
//! framed one whose height follows its pixels), so they share one store, one
//! playhead mapping, and one compositor. Stage footage elements share the
//! store and playhead mapping and draw on the GPU.
//!
//! Preflight checks payloads, channels, and placements without reading a
//! file. Preparation opens every source once: stills decode in memory, and
//! videos and image sequences are probed and decoded into the shared disk
//! cache at about twice the widest they are shown.
use std::{cell::RefCell, collections::HashMap, path::Path};

use anyhow::{Context, Result, bail};
use psychopomp::{
    anchor::AnchorPlan,
    footage::{self, Clip, Fit, FootagePlan, Mask, Treatment},
    image::ImagePlan,
    math::Vec2,
    plan::{ActorPlan, ContinuousChannelPlan, MediaKindPlan, MediaPlan, ScalarPlan},
    stage::{StageElement, StagePlan},
    video::{VIDEO_CHANNELS, VideoPlan},
};

use super::preflight::{decode, strict_channels};
use crate::{
    footage::{FootageStore, SourceId, StageFootageFrame},
    render::{FootageLayer, FootagePose, HeadlessRenderer, Theme},
    video::Decode,
};

/// Which payload an overlay came from. Video Cards draw beneath images and
/// footage, as they always have.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Recipe {
    /// A Video Card decodes at its authored size and rate.
    Video {
        size: [u32; 2],
        fps: u32,
    },
    /// An image's height follows its pixels.
    Image,
    Footage,
}

pub(super) struct FootageInput {
    id: String,
    recipe: Recipe,
    plan: FootagePlan,
    media: MediaPlan,
    /// The trim in source nanoseconds; `None` plays the whole source.
    trim: Option<(u64, u64)>,
    /// The widest the clip is shown, in pixels: stills halve toward twice
    /// it and videos decode at about twice it.
    shown: f32,
}

/// A Stage footage element's clip, checked against the plan's media.
pub(super) struct StageFootageInput {
    element: String,
    clip: Clip,
    size: [f32; 2],
    fit: Fit,
    media: MediaPlan,
    shown: f32,
}

/// Every footage input of a plan: overlays in draw order, and the Stage's.
#[derive(Default)]
pub(super) struct FootageInputs {
    overlays: Vec<FootageInput>,
    stage: Vec<StageFootageInput>,
}

fn placement<'a>(
    kind: &str,
    actor: &str,
    id: &str,
    media: &'a [MediaPlan],
    accepts: impl Fn(&MediaKindPlan) -> bool,
    expected: &str,
) -> Result<&'a MediaPlan> {
    let placement = media
        .iter()
        .find(|media| media.id == id)
        .with_context(|| format!("{kind} '{actor}' references unknown media '{id}'"))?;
    if !accepts(&placement.kind) {
        bail!("{kind} '{actor}' must reference {expected} media");
    }
    Ok(placement)
}

/// The largest (`most`) or smallest literal `actor.property` ever takes:
/// its initial value and every event target.
fn extreme(
    channels: &[ContinuousChannelPlan],
    actor: &str,
    property: &str,
    default: f32,
    most: bool,
) -> f32 {
    let values = channels
        .iter()
        .filter(|channel| channel.actor_id == actor && channel.property == property)
        .flat_map(|channel| {
            std::iter::once(&channel.initial).chain(channel.events.iter().map(|e| e.scalar()))
        })
        .filter_map(|scalar| match scalar {
            ScalarPlan::Literal(value) => Some(*value),
            ScalarPlan::Target(_) => None,
        });
    values.fold(default, |a, b| if most { a.max(b) } else { a.min(b) })
}

/// How wide footage of box width `width` is ever shown: its largest scale
/// over its tightest focus.
fn shown(channels: &[ContinuousChannelPlan], actor: &str, prefix: &str, width: f32) -> f32 {
    let scale = extreme(channels, actor, &format!("{prefix}scale"), 1.0, true).clamp(0.05, 8.0);
    let focus =
        extreme(channels, actor, &format!("{prefix}focus-size"), 1.0, false).clamp(0.05, 1.0);
    width * scale / focus
}

impl FootageInputs {
    pub(super) fn video(
        &mut self,
        actor: &ActorPlan,
        media: &[MediaPlan],
        channels: &[ContinuousChannelPlan],
    ) -> Result<()> {
        let video = decode(actor, "video", VideoPlan::validate)?;
        strict_channels(&actor.id, channels, "video", |property| {
            VIDEO_CHANNELS.contains(&property)
        })?;
        let placement = placement(
            "video actor",
            &actor.id,
            &video.media_id,
            media,
            |kind| matches!(kind, MediaKindPlan::Video),
            "video",
        )?;
        let [width, _] = video.card_size();
        let height = video.width * video.size[1] as f32 / video.size[0] as f32;
        let plan = FootagePlan {
            fit: Fit::Fill,
            mask: Mask::Rect { radius: 18.0 },
            framed: true,
            title: video.title.clone(),
            ..FootagePlan::new(Clip::new(&video.media_id), video.center, [width, height])
        };
        self.overlays.push(FootageInput {
            id: actor.id.clone(),
            recipe: Recipe::Video {
                size: video.size,
                fps: video.fps,
            },
            plan,
            trim: Some((placement.source_start_nanos, placement.source_end_nanos)),
            media: placement.clone(),
            shown: video.width,
        });
        Ok(())
    }

    pub(super) fn image(
        &mut self,
        actor: &ActorPlan,
        media: &[MediaPlan],
        channels: &[ContinuousChannelPlan],
    ) -> Result<()> {
        let image = decode(actor, "image", ImagePlan::validate)?;
        strict_channels(&actor.id, channels, "image", |property| {
            image.accepts(property)
        })?;
        let placement = placement(
            "image actor",
            &actor.id,
            &image.media_id,
            media,
            |kind| matches!(kind, MediaKindPlan::Image),
            "image",
        )?;
        let plan = FootagePlan {
            fit: Fit::Fill,
            mask: Mask::Rect {
                radius: if image.framed { 18.0 } else { image.radius },
            },
            framed: image.framed,
            title: image.title.clone(),
            anchors: image.anchors.clone(),
            // The height follows the decoded image.
            ..FootagePlan::new(Clip::new(&image.media_id), image.center, [image.width, 8.0])
        };
        self.overlays.push(FootageInput {
            id: actor.id.clone(),
            recipe: Recipe::Image,
            plan,
            media: placement.clone(),
            trim: None,
            shown: image.width,
        });
        Ok(())
    }

    pub(super) fn footage(
        &mut self,
        actor: &ActorPlan,
        media: &[MediaPlan],
        channels: &[ContinuousChannelPlan],
    ) -> Result<()> {
        let plan = decode(actor, "footage", FootagePlan::validate)?;
        strict_channels(&actor.id, channels, "footage", |property| {
            plan.accepts(property)
        })?;
        let placement = placement(
            "footage actor",
            &actor.id,
            &plan.clip.media,
            media,
            |kind| matches!(kind, MediaKindPlan::Video | MediaKindPlan::Image),
            "video or image",
        )?;
        self.overlays.push(FootageInput {
            id: actor.id.clone(),
            recipe: Recipe::Footage,
            trim: plan.clip.trim_nanos(),
            media: placement.clone(),
            shown: shown(channels, &actor.id, "", plan.size[0]),
            plan,
        });
        Ok(())
    }

    /// The Stage root's footage elements: each must play a planned video or
    /// image.
    pub(super) fn stage(
        &mut self,
        stage: &str,
        recipe: &StagePlan,
        media: &[MediaPlan],
        channels: &[ContinuousChannelPlan],
    ) -> Result<()> {
        for element in recipe.all_elements() {
            let StageElement::Footage {
                id,
                size,
                clip,
                fit,
                ..
            } = element
            else {
                continue;
            };
            let placement = placement(
                "stage footage",
                id,
                &clip.media,
                media,
                |kind| matches!(kind, MediaKindPlan::Video | MediaKindPlan::Image),
                "video or image",
            )?;
            // Camera push-ins magnify the Stage, so leave some headroom.
            let shown = shown(channels, stage, &format!("{id}."), size[0]) * 1.5;
            self.stage.push(StageFootageInput {
                element: id.clone(),
                clip: clip.clone(),
                size: *size,
                fit: *fit,
                media: placement.clone(),
                shown,
            });
        }
        Ok(())
    }

    /// Media this footage plays, so preflight can count it consumed.
    pub(super) fn media_ids(&self) -> impl Iterator<Item = &str> {
        self.overlays
            .iter()
            .map(|input| input.media.id.as_str())
            .chain(self.stage.iter().map(|input| input.media.id.as_str()))
    }

    /// Overlays that pin to anchors.
    pub(super) fn anchored(&self) -> impl Iterator<Item = (&str, &[AnchorPlan])> {
        self.overlays
            .iter()
            .filter(|input| !matches!(input.recipe, Recipe::Video { .. }))
            .map(|input| (input.id.as_str(), input.plan.anchors.as_slice()))
    }

    /// Open every source once, in draw order: Video Cards first, then images
    /// and footage as declared.
    pub(super) fn open(self, base: &Path) -> Result<Footage> {
        let mut store = FootageStore::new();
        let mut overlays = self.overlays;
        overlays.sort_by_key(|input| !matches!(input.recipe, Recipe::Video { .. }));
        let overlays = overlays
            .into_iter()
            .map(|input| input.open(base, &mut store))
            .collect::<Result<Vec<_>>>()?;
        let stage = self
            .stage
            .into_iter()
            .map(|input| input.open(base, &mut store))
            .collect::<Result<Vec<_>>>()?;
        Ok(Footage {
            store: RefCell::new(store),
            overlays,
            stage,
            layers: RefCell::new(HashMap::new()),
        })
    }
}

/// What to open: `clip` from `media`, about `shown` pixels wide in a box of
/// `size` cut by `fit`; a Video Card passes its own decode size and rate.
struct Request<'a> {
    clip: &'a Clip,
    media: &'a MediaPlan,
    fit: Fit,
    size: [f32; 2],
    shown: f32,
    fixed: Option<([u32; 2], u32)>,
}

fn open_source(base: &Path, store: &mut FootageStore, request: Request<'_>) -> Result<SourceId> {
    let Request {
        clip,
        media,
        fit,
        size,
        shown,
        fixed,
    } = request;
    let path = super::resolve_media_path(base, media);
    if matches!(media.kind, MediaKindPlan::Image) {
        return store.still(&path, shown);
    }
    let sequence = footage::is_sequence(&path).then(|| clip.fps.unwrap_or(24));
    let (size, fps, decoder, range) = match fixed {
        Some((size, fps)) => (size, fps, None, None),
        None => {
            let probe = footage::probe(&path, sequence)?;
            let fps = clip
                .fps
                .or(sequence)
                .unwrap_or_else(|| (probe.fps.round() as u32).clamp(1, 60));
            let width = clip
                .resolution
                .unwrap_or_else(|| decode_width(probe.size, fit, size, shown));
            // The playhead never leaves the trim, so decode only it.
            (
                scaled(probe.size, width),
                fps,
                probe.vpx_alpha,
                clip.trim_nanos(),
            )
        }
    };
    store.video(Decode {
        source: path,
        size,
        fps,
        sequence,
        decoder,
        range,
    })
}

/// Decoded widths, so tiles of nearby sizes share a decode.
const LADDER: [u32; 10] = [160, 240, 320, 480, 640, 960, 1280, 1920, 2560, 3840];

/// The decode width that shows `source` in a box of `size` at up to `shown`
/// pixels wide with at most 2:1 minification: the ladder rung at or above
/// twice that, never past the source's own width.
fn decode_width(source: [u32; 2], fit: Fit, size: [f32; 2], shown: f32) -> u32 {
    let source_size = [source[0] as f32, source[1] as f32];
    let (window, _) = fit.frame(source_size, size);
    let needed = 2.0 * shown * source_size[0] / window[2].max(1.0);
    LADDER
        .into_iter()
        .find(|rung| *rung as f32 >= needed)
        .unwrap_or(3840)
        .min(source[0])
}

/// `source` scaled to `width`, keeping its aspect, in even pixels.
fn scaled(source: [u32; 2], width: u32) -> [u32; 2] {
    let width = (width.max(2) / 2) * 2;
    let height = (width as f64 * f64::from(source[1]) / f64::from(source[0].max(1))).round() as u32;
    [width, (height.max(2) / 2) * 2]
}

impl FootageInput {
    fn open(self, base: &Path, store: &mut FootageStore) -> Result<PreparedFootage> {
        let fixed = match self.recipe {
            Recipe::Video { size, fps } => Some((size, fps)),
            _ => None,
        };
        let request = Request {
            clip: &self.plan.clip,
            media: &self.media,
            fit: self.plan.fit,
            size: self.plan.size,
            shown: self.shown,
            fixed,
        };
        let source = open_source(base, store, request)
            .with_context(|| format!("open footage '{}'", self.id))?;
        let mut plan = self.plan;
        if self.recipe == Recipe::Image {
            let pixels = store.size(source);
            plan.size[1] = plan.size[0] * pixels[1] as f32 / pixels[0].max(1) as f32;
        }
        let trim = self.trim.unwrap_or((0, store.end_nanos(source)));
        Ok(PreparedFootage {
            id: self.id,
            // Video Cards and images keep their exact direct pixels.
            layered: self.recipe == Recipe::Footage,
            plan,
            media: self.media,
            trim,
            source,
        })
    }
}

impl StageFootageInput {
    fn open(self, base: &Path, store: &mut FootageStore) -> Result<StageFootage> {
        let request = Request {
            clip: &self.clip,
            media: &self.media,
            fit: self.fit,
            size: self.size,
            shown: self.shown,
            fixed: None,
        };
        let source = open_source(base, store, request)
            .with_context(|| format!("open stage footage '{}'", self.element))?;
        let trim = self
            .clip
            .trim_nanos()
            .unwrap_or((0, store.end_nanos(source)));
        Ok(StageFootage {
            element: self.element,
            clip: self.clip,
            media: self.media,
            trim,
            source,
        })
    }
}

/// One prepared footage overlay.
pub(super) struct PreparedFootage {
    id: String,
    /// Drawn through a cached layer, redrawn only when its look changes.
    layered: bool,
    plan: FootagePlan,
    media: MediaPlan,
    trim: (u64, u64),
    source: SourceId,
}

/// One prepared Stage footage element.
struct StageFootage {
    element: String,
    clip: Clip,
    media: MediaPlan,
    trim: (u64, u64),
    source: SourceId,
}

/// Plan footage: the shared store, the overlays in draw order, the Stage's
/// elements, and each layered overlay's last drawing.
pub(super) struct Footage {
    store: RefCell<FootageStore>,
    overlays: Vec<PreparedFootage>,
    stage: Vec<StageFootage>,
    layers: RefCell<HashMap<String, (LayerKey, Option<FootageLayer>)>>,
}

/// Everything an overlay's pixels depend on: its pose, its frame, and the
/// theme. Equal keys are equal layers.
#[derive(Clone, Copy, PartialEq)]
struct LayerKey {
    pose: [u32; 15],
    frame: u64,
    theme: Theme,
}

impl LayerKey {
    fn new(pose: &FootagePose, frame: u64, theme: Theme) -> Self {
        let values = [
            pose.center[0],
            pose.center[1],
            pose.scale,
            pose.opacity,
            pose.rotation,
            pose.tilt[0],
            pose.tilt[1],
            pose.blur,
            pose.defocus,
            pose.focus.0[0],
            pose.focus.0[1],
            pose.focus.1,
            pose.treatment.saturation,
            pose.treatment.tint,
            pose.treatment.dim,
        ];
        Self {
            pose: values.map(f32::to_bits),
            frame,
            theme,
        }
    }
}

/// The source frame a clip shows at plan time `time`: the written `time`
/// channel's playhead, or its natural one.
fn frame_index(
    store: &FootageStore,
    source: SourceId,
    clip: &Clip,
    media: &MediaPlan,
    trim: (u64, u64),
    time: f64,
    playhead: Option<f32>,
) -> u64 {
    let playhead = match playhead {
        Some(seconds) => (f64::from(seconds) * 1e9).round() as i64,
        None => clip.natural_nanos(media, (time * 1_000_000_000.0).round() as u64),
    };
    store.frame_index(source, clip.source_nanos(trim, playhead))
}

impl Footage {
    pub(super) fn overlays(&self) -> &[PreparedFootage] {
        &self.overlays
    }

    /// The decoded size of each Stage footage element's frames.
    pub(super) fn stage_sizes(&self) -> HashMap<String, [u32; 2]> {
        let store = self.store.borrow();
        self.stage
            .iter()
            .map(|footage| (footage.element.clone(), store.size(footage.source)))
            .collect()
    }

    /// The identity of the frame `overlay` shows: footage changes pixels
    /// without any channel moving.
    pub(super) fn identity(
        &self,
        overlay: &PreparedFootage,
        time: f64,
        playhead: Option<f32>,
    ) -> u64 {
        let store = self.store.borrow();
        let index = overlay.frame_index(&store, time, playhead);
        store.identity(overlay.source, index)
    }

    /// Draw every overlay, in order: `placed` gives each one's center (its
    /// literal center or pinned anchor; `None` when it cannot be placed) and
    /// written playhead. Layered overlays blend in one pass per run, between
    /// the direct draws of Video Cards and images.
    pub(super) fn render(
        &self,
        pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        time: f64,
        placed: impl Fn(&PreparedFootage) -> Option<(Vec2, Option<f32>)>,
        sample: impl Fn(&str, &str, f32) -> f32,
    ) -> Result<()> {
        let mut run = Vec::new();
        for overlay in &self.overlays {
            let Some((center, playhead)) = placed(overlay) else {
                continue;
            };
            let pose = overlay.pose(center, |property, default| {
                sample(&overlay.id, property, default)
            });
            if pose.opacity <= 0.001 {
                continue;
            }
            let (identity, frame, size) = {
                let mut store = self.store.borrow_mut();
                let index = overlay.frame_index(&store, time, playhead);
                (
                    store.identity(overlay.source, index),
                    store.frame(overlay.source, index)?,
                    store.size(overlay.source),
                )
            };
            if !overlay.layered {
                self.blend(&mut run, pixels, renderer);
                renderer.composite_footage(pixels, &overlay.plan, &frame, size, pose)?;
                continue;
            }
            // The compositor samples sharply until blur passes 0.2 px, so a
            // spring settling below that draws the same pixels.
            let pose = if pose.blur + pose.defocus <= 0.2 {
                FootagePose {
                    blur: 0.0,
                    defocus: 0.0,
                    ..pose
                }
            } else {
                pose
            };
            let key = LayerKey::new(&pose, identity, renderer.theme());
            let mut layers = self.layers.borrow_mut();
            if layers
                .get(&overlay.id)
                .is_none_or(|(cached, _)| *cached != key)
            {
                let layer = renderer.footage_layer(&overlay.plan, &frame, size, pose)?;
                layers.insert(overlay.id.clone(), (key, layer));
            }
            run.push(overlay.id.as_str());
        }
        self.blend(&mut run, pixels, renderer);
        Ok(())
    }

    /// Blend the run of layered overlays drawn so far, and empty it.
    fn blend(&self, run: &mut Vec<&str>, pixels: &mut [u8], renderer: &HeadlessRenderer) {
        let layers = self.layers.borrow();
        let drawn = run
            .drain(..)
            .filter_map(|id| layers.get(id)?.1.as_ref())
            .collect::<Vec<_>>();
        FootageLayer::blend_all(&drawn, pixels, renderer.size());
    }

    /// The frames the Stage's visible footage shows at `time`. `value` reads
    /// a Stage channel (`<element>.<property>`).
    pub(super) fn stage_frames(
        &self,
        time: f64,
        value: impl Fn(&str, f32) -> f32,
    ) -> Result<Vec<StageFootageFrame>> {
        let mut store = self.store.borrow_mut();
        let mut frames = Vec::new();
        for footage in &self.stage {
            if value(&format!("{}.opacity", footage.element), 1.0) <= 0.001 {
                continue;
            }
            let written = value(&format!("{}.time", footage.element), f32::NAN);
            let playhead = (!written.is_nan()).then_some(written);
            let index = frame_index(
                &store,
                footage.source,
                &footage.clip,
                &footage.media,
                footage.trim,
                time,
                playhead,
            );
            frames.push(StageFootageFrame {
                element: footage.element.clone(),
                identity: store.identity(footage.source, index),
                pixels: store.frame(footage.source, index)?,
                size: store.size(footage.source),
            });
        }
        Ok(frames)
    }

    /// Store reads, hits, evictions, and peak memory, for measurement.
    pub(super) fn stats(&self) -> crate::footage::Stats {
        self.store.borrow().stats()
    }
}

impl PreparedFootage {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn anchors(&self) -> &[AnchorPlan] {
        &self.plan.anchors
    }

    /// The literal center, used while the overlay has no anchors.
    pub(super) fn center(&self) -> Vec2 {
        Vec2::from(self.plan.center)
    }

    /// The overlay's pose at `center` from its channels.
    fn pose(&self, center: Vec2, value: impl Fn(&str, f32) -> f32) -> FootagePose {
        let rotation = value("rotation", 0.0);
        FootagePose {
            center: [center.x + value("x", 0.0), center.y + value("y", 0.0)],
            scale: value("scale", 1.0),
            opacity: value("opacity", 1.0).clamp(0.0, 1.0),
            rotation: if self.plan.rotation == 0.0 {
                rotation
            } else {
                self.plan.rotation + rotation
            },
            tilt: [value("tilt-x", 0.0), value("tilt-y", 0.0)],
            blur: value("blur", 0.0).max(0.0),
            defocus: value("defocus", 0.0),
            focus: (
                [value("focus-x", 0.5), value("focus-y", 0.5)],
                value("focus-size", 1.0),
            ),
            treatment: Treatment {
                saturation: value("saturation", 1.0),
                tint: value("tint", 0.0),
                dim: value("dim", 0.0),
            },
        }
    }

    fn frame_index(&self, store: &FootageStore, time: f64, playhead: Option<f32>) -> u64 {
        frame_index(
            store,
            self.source,
            &self.plan.clip,
            &self.media,
            self.trim,
            time,
            playhead,
        )
    }
}

#[cfg(test)]
mod tests;
