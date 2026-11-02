//! The Stage camera: one sampled pose and its projection ([`Camera`]), shared
//! by the renderer, callout anchors, and Scene Programs, and the authoring rig
//! that writes camera shots as ordinary Continuous Channels ([`CameraRig`]).
//!
//! The pose is a rig. `camera.x|y` pan it across the canvas and `camera.z`
//! dollies it along its view axis. `camera.yaw|pitch` swing it around a pivot
//! on that axis at world depth `camera.pivot`, so an orbit keeps the pivot
//! still on screen while nearer and farther things parallax. `camera.zoom`
//! multiplies the focal length (magnification without a perspective change;
//! a dolly zoom trades it against `z`), and `camera.roll` turns the delivered
//! image. At the default pose the projection is the original translation-only
//! one, bit for bit.
//!
//! Cards, labels, and rings are billboards: their centers move in 3D, but they
//! keep facing the lens, so text stays legible at any angle. Orb particles,
//! embers, surface rings, and the wires between projected ends are projected
//! point by point, so orbiting shows their true depth.
use anyhow::{Context, Result, bail, ensure};

use super::{FOCAL, StageElement, StagePlan};
use crate::{
    author::{ActorHandle, PlanBuilder, whole_millis},
    math::{
        Quat, Vec2, Vec3,
        easing::Ease,
        shapes::{Box2, Shape},
        vec2, vec3,
    },
};

/// Every `camera.*` channel and the value it has when a plan never writes it.
/// `camera.track.<id>` weights default to 0 as well.
pub const CAMERA_CHANNELS: [(&str, f32); 16] = [
    ("camera.x", 0.0),
    ("camera.y", 0.0),
    ("camera.z", 0.0),
    ("camera.yaw", 0.0),
    ("camera.pitch", 0.0),
    ("camera.roll", 0.0),
    ("camera.zoom", 1.0),
    ("camera.pivot", 0.0),
    ("camera.focus", 0.0),
    ("camera.dof", 0.0),
    ("camera.handheld", 0.0),
    ("camera.shake", 0.0),
    ("camera.quake", 0.0),
    ("camera.kick-x", 0.0),
    ("camera.kick-y", 0.0),
    ("camera.punch", 0.0),
];

/// The prefix of the per-element follow weights the renderer resolves.
pub const TRACK: &str = "camera.track.";

/// Where `camera.*` rests when a plan never writes it.
pub fn camera_default(property: &str) -> f32 {
    CAMERA_CHANNELS
        .iter()
        .find(|(name, _)| *name == property)
        .map_or(0.0, |(_, value)| *value)
}

/// A sampled camera pose and the frame it projects into.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Pan in x/y; z dollies toward the scene along the view axis.
    pub position: Vec3,
    /// Radians of swing around the pivot: positive yaw moves the camera to the
    /// right (it looks back left at the pivot), positive pitch raises it to
    /// look down.
    pub yaw: f32,
    pub pitch: f32,
    /// Radians the delivered image turns, clockwise on screen. The frame crops
    /// just enough to keep its corners covered.
    pub roll: f32,
    /// Focal length as a multiple of [`FOCAL`].
    pub zoom: f32,
    /// World depth, on the view axis, of the point yaw and pitch swing around.
    pub pivot: f32,
    /// The frame size in pixels.
    pub size: Vec2,
}

impl Camera {
    /// The resting camera for a frame of `size`: pixel exact at depth zero.
    pub fn new(size: Vec2) -> Self {
        Self {
            position: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            zoom: 1.0,
            pivot: 0.0,
            size,
        }
    }

    /// The unturned camera panned and dollied to `position`.
    pub fn at(position: Vec3, size: Vec2) -> Self {
        Self {
            position,
            ..Self::new(size)
        }
    }

    /// Whether yaw or pitch turn it away from looking straight along +z.
    pub fn turned(&self) -> bool {
        self.yaw != 0.0 || self.pitch != 0.0
    }

    /// The rotation from the camera's frame (x right, y down, z away) to the
    /// world: yaw about the world's vertical, then pitch about the camera's right.
    pub fn orientation(&self) -> Quat {
        Quat::from_rotation_y(-self.yaw) * Quat::from_rotation_x(-self.pitch)
    }

    /// The direction it looks.
    pub fn forward(&self) -> Vec3 {
        self.world_dir(Vec3::Z)
    }

    /// A world direction in the camera's frame; unchanged when not turned.
    pub fn view_dir(&self, direction: Vec3) -> Vec3 {
        if self.turned() {
            self.orientation().inverse() * direction
        } else {
            direction
        }
    }

    /// A camera-frame direction in the world; unchanged when not turned.
    pub fn world_dir(&self, direction: Vec3) -> Vec3 {
        if self.turned() {
            self.orientation() * direction
        } else {
            direction
        }
    }

    /// The point yaw and pitch swing around.
    pub fn pivot_point(&self) -> Vec3 {
        (self.size * 0.5 + self.position.truncate()).extend(self.pivot)
    }

    /// Where the lens is.
    pub fn eye(&self) -> Vec3 {
        self.pivot_point() - self.forward() * (FOCAL + self.pivot - self.position.z)
    }

    /// A point's offset from the view axis and its distance along it from the
    /// lens. The unturned arithmetic is the original projection's, exactly.
    fn view(&self, point: Vec3) -> (Vec2, f32) {
        if self.turned() {
            let local = self.orientation().inverse() * (point - self.pivot_point());
            (
                local.truncate(),
                FOCAL + self.pivot - self.position.z + local.z,
            )
        } else {
            (
                point.truncate() - self.size * 0.5 - self.position.truncate(),
                FOCAL + point.z - self.position.z,
            )
        }
    }

    /// Screen position and scale of a world point, or `None` behind the lens.
    /// This is where Stage primitives are drawn; the image roll comes after
    /// ([`Self::rolled`]).
    pub fn project(&self, point: Vec3) -> Option<(Vec2, f32)> {
        let (lateral, depth) = self.view(point);
        if depth <= 1.0 {
            return None;
        }
        let scale = FOCAL * self.zoom / depth;
        Some((self.size * 0.5 + lateral * scale, scale))
    }

    /// The screen rectangle `[x, y, width, height]` of a world rectangle of
    /// `size` centered at `center` facing the camera, such as a card a reel's
    /// zoom flies into (`transitionFocus`); `None` behind the camera.
    pub fn project_rect(&self, center: Vec3, size: Vec2) -> Option<[f32; 4]> {
        let (center, scale) = self.project(center)?;
        let size = size * scale;
        let corner = center - size * 0.5;
        Some([corner.x, corner.y, size.x, size.y])
    }

    /// Distance from the lens along the view axis.
    pub fn distance(&self, point: Vec3) -> f32 {
        self.view(point).1
    }

    /// Depth for draw order and depth of field: the world z the point would
    /// have if the camera were not turned, which is exactly its z when it is not.
    /// `camera.focus` is measured on this scale.
    pub fn depth(&self, point: Vec3) -> f32 {
        if self.turned() {
            self.view(point).1 - FOCAL + self.position.z
        } else {
            point.z
        }
    }

    /// The world point at depth `depth` (see [`Self::depth`]) that projects to `screen`.
    pub fn unproject(&self, screen: Vec2, depth: f32) -> Vec3 {
        let along = FOCAL + depth - self.position.z;
        let lateral = (screen - self.size * 0.5) * (along / (FOCAL * self.zoom));
        if self.turned() {
            self.pivot_point() + self.orientation() * lateral.extend(depth - self.pivot)
        } else {
            (self.size * 0.5 + self.position.truncate() + lateral).extend(depth)
        }
    }

    /// The pan (`camera.x|y`) that puts `point` at the center of the frame,
    /// keeping this orientation, pivot, and dolly.
    pub fn aim(&self, point: Vec3) -> Vec2 {
        let forward = self.forward();
        let along = (point.z - self.pivot) / forward.z.max(1e-3);
        point.truncate() - self.size * 0.5 - forward.truncate() * along
    }

    /// Magnification that keeps the rolled image covering the frame's corners.
    pub fn cover(&self) -> f32 {
        if self.roll == 0.0 {
            return 1.0;
        }
        let roll = self.roll.abs();
        roll.cos() + self.size.max_element() / self.size.min_element().max(1.0) * roll.sin()
    }

    /// Where a projected point lands in the delivered frame after the roll.
    pub fn rolled(&self, screen: Vec2) -> Vec2 {
        if self.roll == 0.0 {
            return screen;
        }
        let center = self.size * 0.5;
        center + Vec2::from_angle(self.roll).rotate(screen - center) * self.cover()
    }

    /// The box a billboard covers in the delivered frame, or `None` when its
    /// center is behind the lens.
    pub fn screen_box(&self, footprint: Footprint) -> Option<Box2> {
        let (center, scale) = self.project(footprint.at)?;
        let center = center + footprint.shift * scale;
        let half = footprint.half * scale;
        let (mut low, mut high) = (Vec2::MAX, Vec2::MIN);
        for corner in [
            vec2(-1.0, -1.0),
            vec2(1.0, -1.0),
            vec2(-1.0, 1.0),
            vec2(1.0, 1.0),
        ] {
            let point = self.rolled(center + half * corner);
            low = low.min(point);
            high = high.max(point);
        }
        Some(Box2 {
            min: low,
            max: high,
        })
    }

    /// The pose with this orientation, pivot, zoom, and roll whose pan and
    /// dolly fit every footprint inside the frame with `padding` pixels to
    /// spare, centered, dollied as close as the tightest axis allows. `None`
    /// when nothing fits (the padding leaves no room).
    pub fn framed(self, footprints: &[Footprint], padding: f32) -> Option<Self> {
        let center = self.size * 0.5;
        let room = self.size * 0.5 - Vec2::splat(padding);
        if footprints.is_empty() || room.min_element() <= 0.0 {
            return None;
        }
        let bounds = |camera: &Camera| {
            footprints.iter().try_fold(
                Box2 {
                    min: Vec2::MAX,
                    max: Vec2::MIN,
                },
                |all, footprint| {
                    let one = camera.screen_box(*footprint)?;
                    Some(Box2 {
                        min: all.min.min(one.min),
                        max: all.max.max(one.max),
                    })
                },
            )
        };
        // Newton steps on the pan, with a numeric Jacobian: depth makes the
        // bounds' center move unevenly as the camera pans.
        let centered = |mut camera: Camera| -> Option<(Camera, Box2)> {
            for _ in 0..16 {
                let here = bounds(&camera)?;
                let error = here.center() - center;
                if error.length() < 1e-3 {
                    break;
                }
                let nudged = |delta: Vec2| {
                    let mut probe = camera;
                    probe.position += delta.extend(0.0);
                    bounds(&probe).map(|moved| moved.center() - here.center())
                };
                let (dx, dy) = (nudged(Vec2::X)?, nudged(Vec2::Y)?);
                let determinant = dx.x * dy.y - dx.y * dy.x;
                if determinant.abs() < 1e-9 {
                    break;
                }
                let step = vec2(
                    (-error.x * dy.y + error.y * dy.x) / determinant,
                    (error.x * dx.y - error.y * dx.x) / determinant,
                );
                camera.position += step.extend(0.0);
            }
            Some((camera, bounds(&camera)?))
        };
        let fits = |z: f32| {
            let mut camera = self;
            camera.position.z = z;
            let (camera, framed) = centered(camera)?;
            let half = framed.extents();
            (half.x <= room.x && half.y <= room.y).then_some(camera)
        };
        // The fit only loosens as the camera pulls back: bisect the dolly.
        let (mut far, mut near) = (self.position.z - 6.0 * FOCAL, self.position.z + 3.0 * FOCAL);
        let mut best = fits(far)?;
        for _ in 0..40 {
            let middle = (far + near) * 0.5;
            match fits(middle) {
                Some(camera) => {
                    best = camera;
                    far = middle;
                }
                None => near = middle,
            }
        }
        Some(best)
    }
}

/// What an element covers on screen: a camera-facing box `half` its size
/// around its projected `at`, offset by `shift`. Both scale with perspective.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    pub at: Vec3,
    pub half: Vec2,
    pub shift: Vec2,
}

/// CommitMono's advance, as a fraction of the font size: an estimate of a
/// label's width without shaping it.
const ADVANCE: f32 = 0.6;

/// The billboard of a positioned element at world `at` and its own `scale`.
fn footprint(element: &StageElement, at: Vec3, scale: f32) -> Option<Footprint> {
    let (half, shift) = match element {
        StageElement::Card { size, .. } => (Vec2::from(*size) * 0.5, Vec2::ZERO),
        StageElement::Orb { radius, .. } => (Vec2::splat(*radius), Vec2::ZERO),
        StageElement::Ring {
            radius, thickness, ..
        } => (Vec2::splat(radius + thickness * 0.5), Vec2::ZERO),
        StageElement::Label {
            size, spans, align, ..
        } => {
            let chars = spans
                .iter()
                .map(|span| span.text.chars().count())
                .sum::<usize>();
            let half = vec2(chars as f32 * size * ADVANCE, size * 1.2) * 0.5;
            let shift = match align {
                crate::caption::CaptionAlign::Left => half.x,
                crate::caption::CaptionAlign::Center => 0.0,
                crate::caption::CaptionAlign::Right => -half.x,
            };
            (half, vec2(shift, 0.0))
        }
        // A form turns, so it frames by its farthest point.
        StageElement::Form { shapes, .. } => (
            Vec2::splat(
                shapes
                    .iter()
                    .map(|shape| shape.radius())
                    .fold(0.0, f32::max),
            ),
            Vec2::ZERO,
        ),
        StageElement::Shape { shape, .. } => match shape.outline(Vec2::ZERO, 1.0) {
            Shape::Box(bounds) => (bounds.extents(), Vec2::ZERO),
            Shape::Circle(circle) => (Vec2::splat(circle.radius), Vec2::ZERO),
            Shape::Polygon(polygon) => {
                let bounds = polygon.bounds();
                (bounds.extents(), bounds.center())
            }
            Shape::Point(_) => (Vec2::ZERO, Vec2::ZERO),
        },
        StageElement::Icon { size, .. } => (Vec2::splat(size * 0.5), Vec2::ZERO),
        StageElement::Footage { size, .. } | StageElement::RuneCard { size, .. } => {
            (Vec2::from(*size) * 0.5, Vec2::ZERO)
        }
        StageElement::Beam { .. }
        | StageElement::Packet { .. }
        | StageElement::Path { .. }
        | StageElement::Bolt { .. }
        | StageElement::Shield { .. }
        | StageElement::Backdrop { .. } => return None,
    };
    Some(Footprint {
        at,
        half: half * scale,
        shift: shift * scale,
    })
}

/// How a camera move travels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Move {
    /// A critically damped spring that settles in about this many seconds:
    /// weight and a natural tail, and an interrupted move keeps its velocity.
    Spring(f32),
    /// A minimum-jerk glide (smootherstep) of exactly this many seconds, for
    /// precisely timed moves between resting compositions.
    Glide(f32),
    /// Any eased curve over exactly this many seconds.
    Ease(f32, Ease),
    /// An instant cut.
    Cut,
}

impl Move {
    pub fn seconds(self) -> f32 {
        match self {
            Self::Spring(seconds) | Self::Glide(seconds) | Self::Ease(seconds, _) => seconds,
            Self::Cut => 0.0,
        }
    }

    fn nanos(self) -> u64 {
        whole_millis(self.seconds())
    }
}

/// Authoring handle for the Stage camera: every shot reads the pose it starts
/// from (the channels as written so far) and writes ordinary `camera.*`
/// channels, so shots retarget, interrupt, and compose with jolts like any
/// other motion. Get one from [`super::StageActor::camera`].
#[derive(Clone, Debug)]
pub struct CameraRig {
    actor: ActorHandle,
    plan: StagePlan,
    size: Vec2,
}

impl CameraRig {
    pub(super) fn new(actor: ActorHandle, plan: StagePlan) -> Self {
        Self {
            actor,
            plan,
            size: vec2(1920.0, 1080.0),
        }
    }

    /// For a frame other than 1920×1080.
    pub fn sized(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }

    /// The authored pose at `at_nanos`: the channels written so far, before
    /// any follow, jolt, or handheld sway the renderer adds.
    pub fn pose(&self, scene: &PlanBuilder, at_nanos: u64) -> Camera {
        let value = |property: &str| self.value(scene, property, at_nanos);
        Camera {
            position: vec3(value("camera.x"), value("camera.y"), value("camera.z")),
            yaw: value("camera.yaw"),
            pitch: value("camera.pitch"),
            roll: value("camera.roll"),
            zoom: value("camera.zoom"),
            pivot: value("camera.pivot"),
            size: self.size,
        }
    }

    /// Move to `pose`: pan, dolly, orientation, zoom, pivot, and roll each
    /// travel by `motion` when they change, and any follow hands back to the
    /// pose on the same profile. Returns when the move is due to settle.
    pub fn move_to(
        &self,
        scene: &mut PlanBuilder,
        pose: &Camera,
        at_nanos: u64,
        motion: Move,
    ) -> u64 {
        for (property, target) in [
            ("camera.x", pose.position.x),
            ("camera.y", pose.position.y),
            ("camera.z", pose.position.z),
            ("camera.yaw", pose.yaw),
            ("camera.pitch", pose.pitch),
            ("camera.roll", pose.roll),
            ("camera.zoom", pose.zoom),
            ("camera.pivot", pose.pivot),
        ] {
            self.write(scene, property, at_nanos, target, motion);
        }
        self.release_tracks(scene, None, at_nanos, motion);
        at_nanos + motion.nanos()
    }

    /// The pose at `at_nanos` re-aimed and dollied so `targets` (elements,
    /// or a beam's or packet's two ends) fill the frame inside `padding`
    /// pixels. Orientation, zoom, pivot, and roll are kept; elements are
    /// framed where their own channels have put them by then.
    pub fn framing(
        &self,
        scene: &PlanBuilder,
        targets: &[&str],
        padding: f32,
        at_nanos: u64,
    ) -> Result<Camera> {
        let mut footprints = Vec::new();
        for target in targets {
            footprints.extend(self.footprints(scene, target, at_nanos)?);
        }
        self.pose(scene, at_nanos)
            .framed(&footprints, padding)
            .with_context(|| format!("{targets:?} cannot fit inside {padding} px of padding"))
    }

    /// Frame `targets` with `padding` pixels to spare: the camera pans and
    /// dollies to the composition that holds them all. Returns when it settles.
    pub fn frame(
        &self,
        scene: &mut PlanBuilder,
        targets: &[&str],
        padding: f32,
        at_nanos: u64,
        motion: Move,
    ) -> Result<u64> {
        let pose = self.framing(scene, targets, padding, at_nanos + motion.nanos())?;
        Ok(self.move_to(scene, &pose, at_nanos, motion))
    }

    /// Open on the composition: the camera starts `back` pixels farther away
    /// and dollies in onto the current pose on a critically damped spring.
    pub fn establish(
        &self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        back: f32,
        seconds: f32,
    ) -> u64 {
        let rest = self.value(scene, "camera.z", at_nanos);
        let z = scene.channel(&self.actor, "camera.z", rest - back);
        scene.set(&z, at_nanos, rest - back);
        scene.spring(&z, at_nanos, rest, seconds, 0.0);
        at_nanos + whole_millis(seconds)
    }

    /// Dolly `distance` pixels closer along the view axis.
    pub fn push_in(
        &self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        distance: f32,
        motion: Move,
    ) -> u64 {
        let z = self.value(scene, "camera.z", at_nanos);
        self.write(scene, "camera.z", at_nanos, z + distance, motion);
        at_nanos + motion.nanos()
    }

    /// Dolly `distance` pixels back to show consequences.
    pub fn pull_back(
        &self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        distance: f32,
        motion: Move,
    ) -> u64 {
        self.push_in(scene, at_nanos, -distance, motion)
    }

    /// A slow, live move toward `toward` (the actor that is speaking): a
    /// third of the way to centering it and a slight push, eased gently over
    /// `seconds` so the frame never sits dead still.
    pub fn drift(
        &self,
        scene: &mut PlanBuilder,
        toward: &str,
        at_nanos: u64,
        seconds: f32,
    ) -> Result<u64> {
        let pose = self.pose(scene, at_nanos);
        let aim = pose.aim(self.center(scene, toward, at_nanos)?);
        let lean = pose.position.truncate().lerp(aim, 0.35);
        let target = Camera {
            position: lean.extend(pose.position.z + 24.0),
            ..pose
        };
        Ok(self.move_to(scene, &target, at_nanos, Move::Ease(seconds, Ease::GLIDE)))
    }

    /// Whip to frame `targets`: a fast minimum-jerk move that the shutter
    /// streaks, with a radial `post.zoom` streak swelling through its middle.
    pub fn whip(
        &self,
        scene: &mut PlanBuilder,
        targets: &[&str],
        padding: f32,
        at_nanos: u64,
        seconds: f32,
    ) -> Result<u64> {
        let done = self.frame(scene, targets, padding, at_nanos, Move::Glide(seconds))?;
        let streak = scene.channel(&self.actor, "post.zoom", 0.0);
        let half = seconds * 0.5;
        scene.ease(&streak, at_nanos, 0.14, half, Ease::GLIDE);
        scene.ease(
            &streak,
            at_nanos + whole_millis(half),
            0.0,
            half,
            Ease::GLIDE,
        );
        Ok(done)
    }

    /// Crash-zoom onto `targets`: a fast minimum-jerk lunge that frames them
    /// inside `padding` pixels with a Dutch `roll` (radians) and a radial
    /// `post.zoom` + `post.chroma` punch through its middle.
    pub fn crash(
        &self,
        scene: &mut PlanBuilder,
        targets: &[&str],
        padding: f32,
        roll: f32,
        at_nanos: u64,
        seconds: f32,
    ) -> Result<u64> {
        let mut pose = self.framing(scene, targets, padding, at_nanos + whole_millis(seconds))?;
        pose.roll = roll;
        let done = self.move_to(scene, &pose, at_nanos, Move::Glide(seconds));
        let half = seconds * 0.5;
        let mid = at_nanos + whole_millis(half);
        for (property, peak) in [("post.zoom", 0.22), ("post.chroma", 0.38)] {
            let ch = scene.channel(&self.actor, property, 0.0);
            scene.ease(&ch, at_nanos, peak, half, Ease::GLIDE);
            scene.ease(&ch, mid, 0.0, half, Ease::GLIDE);
        }
        Ok(done)
    }

    /// Swing around `around` to `yaw` and `pitch` (radians), which becomes
    /// the pivot and the center of the frame: nearer things slide one way,
    /// farther things the other, and the subject stays put.
    pub fn orbit(
        &self,
        scene: &mut PlanBuilder,
        around: &str,
        at_nanos: u64,
        [yaw, pitch]: [f32; 2],
        motion: Move,
    ) -> Result<u64> {
        let subject = self.center(scene, around, at_nanos + motion.nanos())?;
        let mut target = Camera {
            yaw,
            pitch,
            pivot: subject.z,
            ..self.pose(scene, at_nanos + motion.nanos())
        };
        target.position = target.aim(subject).extend(target.position.z);
        Ok(self.move_to(scene, &target, at_nanos, motion))
    }

    /// Vertigo: dolly `dolly` pixels in (negative pulls back) while the focal
    /// length follows the distance, so `subject` holds its size and place and
    /// the depth around it stretches or compresses. Both channels share one
    /// curve, so the subject is exactly still throughout a glide.
    pub fn dolly_zoom(
        &self,
        scene: &mut PlanBuilder,
        subject: &str,
        at_nanos: u64,
        dolly: f32,
        motion: Move,
    ) -> Result<u64> {
        let pose = self.pose(scene, at_nanos);
        let distance = pose.distance(self.center(scene, subject, at_nanos)?);
        ensure!(
            distance - dolly > 0.1 * FOCAL,
            "a {dolly} px dolly zoom would pass through '{subject}'"
        );
        let target = Camera {
            position: pose.position + Vec3::Z * dolly,
            zoom: pose.zoom * (distance - dolly) / distance,
            ..pose
        };
        Ok(self.move_to(scene, &target, at_nanos, motion))
    }

    /// Roll the image to `radians` (clockwise on screen), a Dutch angle for
    /// unease; 0 levels it.
    pub fn roll(&self, scene: &mut PlanBuilder, at_nanos: u64, radians: f32, motion: Move) -> u64 {
        self.write(scene, "camera.roll", at_nanos, radians, motion);
        at_nanos + motion.nanos()
    }

    /// Pull focus to the plane of `element` (depth of field needs
    /// `camera.dof` above zero; see [`Self::aperture`]).
    pub fn focus_on(
        &self,
        scene: &mut PlanBuilder,
        element: &str,
        at_nanos: u64,
        motion: Move,
    ) -> Result<u64> {
        let end = at_nanos + motion.nanos();
        let depth = self
            .pose(scene, end)
            .depth(self.center(scene, element, end)?);
        self.write(scene, "camera.focus", at_nanos, depth, motion);
        Ok(end)
    }

    /// Open (larger) or close the depth of field: blur pixels per 100 px of
    /// depth from the focal plane.
    pub fn aperture(&self, scene: &mut PlanBuilder, at_nanos: u64, dof: f32, motion: Move) -> u64 {
        self.write(scene, "camera.dof", at_nanos, dof, motion);
        at_nanos + motion.nanos()
    }

    /// Follow `target`, a packet or a positioned element: the renderer pans
    /// to keep it centered at every shutter sample, so a packet in flight is
    /// tracked exactly. The camera catches it over `motion` and keeps it until
    /// the next shot (or [`Self::release`]) hands back.
    pub fn follow(
        &self,
        scene: &mut PlanBuilder,
        target: &str,
        at_nanos: u64,
        motion: Move,
    ) -> Result<u64> {
        let followable = match self.plan.element(target) {
            Some(StageElement::Packet { .. }) => true,
            Some(element) => element.anchor().is_some(),
            None => false,
        };
        ensure!(
            followable,
            "the camera follows a packet or a positioned element; '{target}' is neither"
        );
        self.release_tracks(scene, Some(target), at_nanos, motion);
        self.write(scene, &format!("{TRACK}{target}"), at_nanos, 1.0, motion);
        Ok(at_nanos + motion.nanos())
    }

    /// Stop following and hold where the followed things come to rest: an
    /// element's center, or the element a packet lands on.
    pub fn release(&self, scene: &mut PlanBuilder, at_nanos: u64, motion: Move) -> Result<u64> {
        let end = at_nanos + motion.nanos();
        let pose = self.pose(scene, end);
        let mut pan = Vec2::ZERO;
        let mut total = 0.0;
        for element in &self.plan.elements {
            let weight = self.weight(scene, element.id(), at_nanos);
            if weight <= 0.0 {
                continue;
            }
            let rest = match element {
                StageElement::Packet { beam, reverse, .. } => {
                    let Some(StageElement::Beam { from, to, .. }) = self.plan.element(beam) else {
                        bail!("packet '{}' has no beam", element.id());
                    };
                    if *reverse { from } else { to }
                }
                _ => element.id(),
            };
            pan += pose.aim(self.center(scene, rest, end)?) * weight;
            total += weight;
        }
        if total > 0.0 {
            let target = pose.position.truncate().lerp(pan / total, total.min(1.0));
            self.write(scene, "camera.x", at_nanos, target.x, motion);
            self.write(scene, "camera.y", at_nanos, target.y, motion);
        }
        self.release_tracks(scene, None, at_nanos, motion);
        Ok(end)
    }

    /// Fade a handheld operator in (`amount` about 1) or out (0) over
    /// `seconds`: a slow sway of pan and angle, for quiet beats.
    pub fn handheld(
        &self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        amount: f32,
        seconds: f32,
    ) -> u64 {
        self.write(
            scene,
            "camera.handheld",
            at_nanos,
            amount,
            Move::Ease(seconds, Ease::GLIDE),
        );
        at_nanos + whole_millis(seconds)
    }

    /// Where `element` sits in the delivered frame under `pose`, as for a
    /// Reel zoom's focus rectangle.
    pub fn screen_box(
        &self,
        scene: &PlanBuilder,
        element: &str,
        pose: &Camera,
        at_nanos: u64,
    ) -> Result<Box2> {
        self.footprints(scene, element, at_nanos)?
            .into_iter()
            .map(|footprint| pose.screen_box(footprint))
            .try_fold(
                Box2 {
                    min: Vec2::MAX,
                    max: Vec2::MIN,
                },
                |all, one| {
                    let one = one?;
                    Some(Box2 {
                        min: all.min.min(one.min),
                        max: all.max.max(one.max),
                    })
                },
            )
            .with_context(|| format!("'{element}' is behind the camera"))
    }

    fn value(&self, scene: &PlanBuilder, property: &str, at_nanos: u64) -> f32 {
        scene
            .sample(&self.actor, property, at_nanos)
            .map_or_else(|| camera_default(property), |state| state.position)
    }

    fn weight(&self, scene: &PlanBuilder, id: &str, at_nanos: u64) -> f32 {
        scene
            .sample(&self.actor, &format!("{TRACK}{id}"), at_nanos)
            .map_or(0.0, |state| state.position.max(0.0))
    }

    /// Write `target` to a channel by `motion`, unless it is already headed
    /// there: an unchanged destination keeps its trajectory.
    fn write(
        &self,
        scene: &mut PlanBuilder,
        property: &str,
        at_nanos: u64,
        target: f32,
        motion: Move,
    ) {
        let destination = scene
            .destination(&self.actor, property, at_nanos)
            .unwrap_or_else(|| camera_default(property));
        if (destination - target).abs() <= 1e-3 {
            return;
        }
        let channel = scene.channel(&self.actor, property, camera_default(property));
        match motion {
            Move::Spring(seconds) => scene.spring(&channel, at_nanos, target, seconds, 0.0),
            Move::Glide(seconds) => {
                scene.ease(&channel, at_nanos, target, seconds, Ease::Smootherstep)
            }
            Move::Ease(seconds, curve) => scene.ease(&channel, at_nanos, target, seconds, curve),
            Move::Cut => scene.set(&channel, at_nanos, target),
        }
    }

    /// Hand every follow but `keep` back to the authored pan.
    fn release_tracks(
        &self,
        scene: &mut PlanBuilder,
        keep: Option<&str>,
        at_nanos: u64,
        motion: Move,
    ) {
        for element in &self.plan.elements {
            let id = element.id();
            if Some(id) != keep
                && scene
                    .sample(&self.actor, &format!("{TRACK}{id}"), at_nanos)
                    .is_some()
            {
                self.write(scene, &format!("{TRACK}{id}"), at_nanos, 0.0, motion);
            }
        }
    }

    /// The world center of a positioned element at `at_nanos`, after its own
    /// x/y/z channels.
    fn center(&self, scene: &PlanBuilder, id: &str, at_nanos: u64) -> Result<Vec3> {
        let element = self
            .plan
            .element(id)
            .with_context(|| format!("the stage has no element '{id}'"))?;
        let anchor = element
            .anchor()
            .with_context(|| format!("'{id}' has no position of its own"))?;
        let offset = |axis: &str| {
            scene
                .sample(&self.actor, &format!("{id}.{axis}"), at_nanos)
                .map_or(0.0, |state| state.position)
        };
        Ok(Vec3::from(anchor) + vec3(offset("x"), offset("y"), offset("z")))
    }

    /// What `id` covers: its own billboard, a beam's two ends, or a packet's beam.
    fn footprints(&self, scene: &PlanBuilder, id: &str, at_nanos: u64) -> Result<Vec<Footprint>> {
        let element = self
            .plan
            .element(id)
            .with_context(|| format!("the stage has no element '{id}'"))?;
        match element {
            StageElement::Beam { from, to, .. } => {
                let mut ends = self.footprints(scene, from, at_nanos)?;
                ends.extend(self.footprints(scene, to, at_nanos)?);
                Ok(ends)
            }
            StageElement::Packet { beam, .. } => self.footprints(scene, beam, at_nanos),
            _ => {
                let scale = scene
                    .sample(&self.actor, &format!("{id}.scale"), at_nanos)
                    .map_or(1.0, |state| state.position.max(0.01));
                let at = self.center(scene, id, at_nanos)?;
                Ok(footprint(element, at, scale).into_iter().collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        author::{PlanBuilder, SECOND},
        stage::StageActor,
    };

    const SIZE: Vec2 = Vec2::new(1920.0, 1080.0);

    fn plan() -> StagePlan {
        serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "card", "id": "client", "at": [520, 540, 0], "size": [300, 120], "title": "client" },
                { "kind": "card", "id": "api", "at": [1400, 500, 0], "size": [300, 120], "title": "api" },
                { "kind": "card", "id": "archive", "at": [960, 300, 900], "size": [300, 120], "title": "archive" },
                { "kind": "orb", "id": "core", "at": [960, 760, 300], "radius": 120 },
                { "kind": "beam", "id": "link", "from": "client", "to": "api" },
                { "kind": "packet", "id": "request", "beam": "link" }
            ]
        }))
        .unwrap()
    }

    /// The projection before the camera could turn, kept to prove the
    /// default pose reproduces it bit for bit.
    fn original(position: Vec3, point: Vec3) -> Option<(Vec2, f32)> {
        let depth = FOCAL + point.z - position.z;
        if depth <= 1.0 {
            return None;
        }
        let scale = FOCAL / depth;
        let center = SIZE * 0.5;
        Some((
            center + (point.truncate() - center - position.truncate()) * scale,
            scale,
        ))
    }

    #[test]
    fn the_default_pose_reproduces_the_original_projection_exactly() {
        for (index, position) in [
            Vec3::ZERO,
            vec3(-110.0, 15.0, 60.0),
            vec3(330.3, -47.1, -160.7),
        ]
        .into_iter()
        .enumerate()
        {
            let camera = Camera::at(position, SIZE);
            for i in 0..200u32 {
                let h = |salt| crate::math::random::hash(i, salt + index as u32 * 7);
                let point = vec3(
                    h(1) * 3000.0 - 500.0,
                    h(2) * 2000.0 - 400.0,
                    h(3) * 2400.0 - 900.0,
                );
                assert_eq!(camera.project(point), original(position, point));
                assert_eq!(camera.depth(point), point.z);
                assert_eq!(camera.rolled(vec2(h(4), h(5))), vec2(h(4), h(5)));
            }
        }
    }

    #[test]
    fn an_orbit_holds_its_pivot_and_parallaxes_around_it() {
        let rest = Camera {
            pivot: 300.0,
            ..Camera::at(vec3(0.0, 220.0, 0.0), SIZE)
        };
        let pivot = rest.pivot_point();
        let (near, far) = (pivot - Vec3::Z * 400.0, pivot + Vec3::Z * 600.0);
        for (yaw, pitch) in [(0.4, 0.0), (-0.6, 0.0), (0.0, 0.3), (0.5, -0.2)] {
            let turned = Camera { yaw, pitch, ..rest };
            let (center, scale) = turned.project(pivot).unwrap();
            assert!(center.abs_diff_eq(SIZE * 0.5, 1e-3), "{center}");
            assert!((scale - rest.project(pivot).unwrap().1).abs() < 1e-5);
            // The lens stays the same distance from the pivot.
            assert!((turned.eye().distance(pivot) - (FOCAL + 300.0)).abs() < 1e-2);
            let shift = |point| turned.project(point).unwrap().0 - SIZE * 0.5;
            if yaw > 0.0 {
                assert!(
                    shift(near).x < -1.0 && shift(far).x > 1.0,
                    "near left, far right"
                );
            }
            if pitch > 0.0 {
                // Looking down from above: nearer things sit lower in frame.
                assert!(
                    shift(near).y > 1.0 && shift(far).y < -1.0,
                    "raised: near down, far up"
                );
            }
            // Unprojecting at a point's depth recovers it.
            for point in [near, far, vec3(400.0, 900.0, -100.0)] {
                let (screen, _) = turned.project(point).unwrap();
                assert!(
                    turned
                        .unproject(screen, turned.depth(point))
                        .abs_diff_eq(point, 0.05)
                );
            }
        }
        // A barely turned camera is continuous with the unturned one.
        let point = vec3(300.0, 200.0, 500.0);
        let nudged = Camera { yaw: 1e-6, ..rest }.project(point).unwrap();
        assert!(nudged.0.abs_diff_eq(rest.project(point).unwrap().0, 1e-2));
    }

    #[test]
    fn aiming_centers_a_point_under_any_orientation() {
        let point = vec3(1500.0, 260.0, 700.0);
        for yaw in [0.0, 0.3, -0.5] {
            let mut camera = Camera {
                yaw,
                pitch: 0.15,
                pivot: 100.0,
                zoom: 1.4,
                ..Camera::at(vec3(0.0, 0.0, 80.0), SIZE)
            };
            camera.position = camera.aim(point).extend(camera.position.z);
            let (center, _) = camera.project(point).unwrap();
            assert!(center.abs_diff_eq(SIZE * 0.5, 1e-2), "{yaw}: {center}");
        }
    }

    #[test]
    fn zoom_magnifies_without_changing_perspective() {
        let camera = Camera::new(SIZE);
        let zoomed = Camera {
            zoom: 2.0,
            ..camera
        };
        for point in [vec3(300.0, 200.0, 0.0), vec3(1500.0, 900.0, 800.0)] {
            let (a, sa) = camera.project(point).unwrap();
            let (b, sb) = zoomed.project(point).unwrap();
            assert!((b - SIZE * 0.5).abs_diff_eq((a - SIZE * 0.5) * 2.0, 1e-3));
            assert!((sb - sa * 2.0).abs() < 1e-6);
        }
    }

    #[test]
    fn a_roll_turns_the_frame_and_crops_just_enough_to_cover_it() {
        let camera = Camera {
            roll: 0.05,
            ..Camera::new(SIZE)
        };
        // Every delivered corner samples inside the projected frame.
        for corner in [vec2(0.0, 0.0), SIZE, vec2(SIZE.x, 0.0), vec2(0.0, SIZE.y)] {
            let center = SIZE * 0.5;
            let source = center + Vec2::from_angle(-0.05).rotate(corner - center) / camera.cover();
            assert!(source.cmpge(Vec2::splat(-1e-3)).all() && source.cmple(SIZE + 1e-3).all());
        }
        let turned = camera.rolled(vec2(1460.0, 540.0)) - SIZE * 0.5;
        assert!(
            turned.y > 20.0,
            "positive roll turns clockwise on screen: {turned}"
        );
    }

    #[test]
    fn framing_fits_every_target_inside_the_padding() {
        for (pose, targets, padding) in [
            (Camera::new(SIZE), vec!["client", "api"], 120.0),
            (Camera::new(SIZE), vec!["client"], 200.0),
            (Camera::new(SIZE), vec!["archive", "core", "link"], 80.0),
            (
                Camera {
                    yaw: 0.35,
                    pitch: 0.1,
                    pivot: 300.0,
                    roll: 0.03,
                    ..Camera::new(SIZE)
                },
                vec!["client", "core"],
                100.0,
            ),
        ] {
            let mut scene = PlanBuilder::new("framing", 4 * SECOND);
            let stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
            let rig = stage.camera();
            rig.move_to(&mut scene, &pose, 0, Move::Cut);
            let framed = rig.framing(&scene, &targets, padding, 0).unwrap();
            assert_eq!(
                (framed.yaw, framed.pitch, framed.roll),
                (pose.yaw, pose.pitch, pose.roll)
            );
            let boxes = targets
                .iter()
                .map(|id| rig.screen_box(&scene, id, &framed, 0).unwrap())
                .collect::<Vec<_>>();
            let low = boxes.iter().fold(Vec2::MAX, |low, b| low.min(b.min));
            let high = boxes.iter().fold(Vec2::MIN, |high, b| high.max(b.max));
            let inside = Vec2::splat(padding - 0.05);
            assert!(
                low.cmpge(inside).all() && high.cmple(SIZE - inside).all(),
                "{targets:?}: {low} {high}"
            );
            // Centered, and as close as the tighter axis allows.
            assert!(((low + high) * 0.5).abs_diff_eq(SIZE * 0.5, 0.05));
            let slack = (low - padding).min(SIZE - padding - high);
            assert!(
                slack.min_element() < 0.5,
                "{targets:?} is not framed tightly: {slack}"
            );
        }
    }

    #[test]
    fn framing_is_impossible_when_the_padding_leaves_no_room() {
        let mut scene = PlanBuilder::new("framing", SECOND);
        let stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        assert!(
            stage
                .camera()
                .framing(&scene, &["client"], 600.0, 0)
                .is_err()
        );
        assert!(
            stage
                .camera()
                .framing(&scene, &["nowhere"], 10.0, 0)
                .is_err()
        );
    }

    #[test]
    fn a_glide_leaves_and_meets_its_rests_with_zero_velocity() {
        let mut scene = PlanBuilder::new("glide", 4 * SECOND);
        let mut stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        stage.glide(&mut scene, "camera.x", SECOND, 200.0, 1.6);
        let actor = stage.camera().actor;
        let sample = |at: u64| scene.sample(&actor, "camera.x", at).unwrap();
        assert_eq!(
            (sample(SECOND).position, sample(SECOND).velocity),
            (0.0, 0.0)
        );
        let end = SECOND + 1_600_000_000;
        assert_eq!((sample(end).position, sample(end).velocity), (200.0, 0.0));
        let middle = sample(SECOND + 800_000_000);
        assert!((middle.position - 100.0).abs() < 1e-3);
        // Minimum jerk peaks at 1.875 times the average speed.
        assert!((middle.velocity - 1.875 * 200.0 / 1.6).abs() < 0.05);
        let near = sample(SECOND + 1_000_000).velocity;
        assert!(
            near > 0.0 && near < 0.01,
            "it leaves the rest gently: {near}"
        );
    }

    #[test]
    fn a_dolly_zoom_holds_its_subject_while_the_background_moves() {
        let mut scene = PlanBuilder::new("vertigo", 4 * SECOND);
        let stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        let rig = stage.camera();
        let framed = rig
            .frame(&mut scene, &["core"], 300.0, 0, Move::Cut)
            .unwrap();
        let done = rig
            .dolly_zoom(&mut scene, "core", framed, 500.0, Move::Glide(2.0))
            .unwrap();
        let core = vec3(960.0, 760.0, 300.0);
        let archive = vec3(960.0, 300.0, 900.0);
        let before = rig.pose(&scene, framed);
        let size = |pose: &Camera| pose.project(core).unwrap().1;
        for at in (framed..=done).step_by(100_000_000) {
            let pose = rig.pose(&scene, at);
            assert!(
                (size(&pose) - size(&before)).abs() < 1e-4,
                "the subject holds its size"
            );
            assert!(pose.project(core).unwrap().0.abs_diff_eq(SIZE * 0.5, 1e-2));
        }
        let after = rig.pose(&scene, done);
        assert!(after.position.z - before.position.z == 500.0 && after.zoom < before.zoom);
        let far = |pose: &Camera| pose.project(archive).unwrap().1 / size(pose);
        assert!(far(&after) < far(&before) * 0.9, "the far plane recedes");
        assert!(
            rig.dolly_zoom(&mut scene, "core", done, 5000.0, Move::Glide(1.0))
                .is_err(),
            "it cannot dolly through its subject"
        );
    }

    #[test]
    fn shots_write_only_what_changes_and_hand_a_follow_back() {
        let mut scene = PlanBuilder::new("shots", 8 * SECOND);
        let stage = StageActor::declare(&mut scene, "stage", &plan()).unwrap();
        let rig = stage.camera();
        let framed = rig
            .frame(&mut scene, &["client", "api"], 140.0, 0, Move::Spring(1.4))
            .unwrap();
        let followed = rig
            .follow(&mut scene, "request", framed, Move::Spring(0.6))
            .unwrap();
        assert!(
            rig.follow(&mut scene, "link", framed, Move::Spring(0.6))
                .is_err()
        );
        rig.focus_on(&mut scene, "archive", followed, Move::Spring(1.0))
            .unwrap();
        rig.orbit(
            &mut scene,
            "core",
            followed + SECOND,
            [0.4, 0.1],
            Move::Glide(2.0),
        )
        .unwrap();
        let plan = scene.finish().unwrap();
        let channel = |property: &str| {
            plan.continuous_channels
                .iter()
                .find(|c| c.property == property)
                .unwrap_or_else(|| panic!("{property}"))
        };
        for property in ["camera.roll", "camera.zoom"] {
            assert!(
                plan.continuous_channels
                    .iter()
                    .all(|c| c.property != property),
                "{property} never changed, so it is never written"
            );
        }
        let track = channel("camera.track.request");
        assert_eq!(
            track.events.len(),
            2,
            "followed, then released by the orbit"
        );
        assert_eq!(channel("camera.focus").events.len(), 1);
        assert_eq!(channel("camera.yaw").events.len(), 1);
        let stage_plan = plan.actors[0].data.clone();
        let stage_plan: StagePlan = serde_json::from_value(stage_plan).unwrap();
        for channel in &plan.continuous_channels {
            assert!(
                stage_plan.accepts(&channel.property),
                "{}",
                channel.property
            );
        }
    }
}
