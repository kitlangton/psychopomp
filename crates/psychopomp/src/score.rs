//! Composable choreography (`Beat`, `Span`, `CueTime`, and value Actor handles).
//!
//! ## Core Principle
//!
//! **The [`PlanBuilder`] is the only mutable thing; actor handles are values.**
//!
//! A [`Beat`] is a pure schedule transformer over `&mut PlanBuilder`:
//!
//! ```text
//! ⟦Beat⟧ : Time (start_nanos: u64) -> (Span, Writes into PlanBuilder)
//! ```
//!
//! Actor handles ([`Stage`], [`Caption`], [`Callout`], [`RollingNumber`],
//! [`Tree`], [`Plot`], [`Lanes`], [`Sequence`], [`Video`]) are immutable
//! `Clone` values whose methods take `&self` and return `impl Beat`. Because
//! every actor's beats play against the same `&mut PlanBuilder`, any number of
//! actors and sounds compose inside one `all![...]` or `.then(...)` expression
//! without per-actor contexts or escape hatches.

use std::{cell::RefCell, path::PathBuf, rc::Rc};

use anyhow::Result;
use serde_json::Value;

use crate::{
    author::{ActorHandle, ContinuousHandle, PlanBuilder, whole_millis},
    bars::{BarsActor, BarsPlan, SortOrder},
    callout::{CalloutActor, CalloutPlan},
    caption::{self, CaptionActor, CaptionPlan, CaptionSpanPlan},
    changed_files::{ChangedFilesActor, ChangedFilesPlan},
    chat::{ChatActor, ChatPlan, ChatSpanPlan},
    checklist::{ChecklistActor, ChecklistPlan, Outcome},
    chrome,
    confetti::{ConfettiActor, ConfettiPlan},
    effects::spinner::Mark,
    ide::{
        CursorActor, CursorPlan, DiagnosticActor, DiagnosticPlan, HoverActor, HoverPlan, InlayHint,
    },
    image::{ImageActor, ImagePlan},
    lanes::{LanesActor, LanesPlan},
    lens::{LensActor, LensPlan},
    lower_third::{LowerThirdActor, LowerThirdPlan},
    math::easing::Ease,
    meter::{MeterActor, MeterPlan},
    plan::{MediaKindPlan, MediaPlan, MediaRolePlan, ScenePlan, SpringPlan, TrackEventPlan},
    plot::{PlotActor, PlotPlan},
    rolling::{RollingNumberActor, RollingNumberPlan},
    sequence::{SequenceActor, SequencePlan},
    stage::{
        Camera as StageCamera, CameraRig, Move, OrbEntrance, PORT_POP_SECONDS, StageActor,
        StagePlan, packet, reply_after,
    },
    subtitles::{SubtitlesActor, SubtitlesPlan},
    terminal::{TerminalActor, TerminalPlan},
    text::{TextActor, TextPlan},
    tone::Tone,
    tree::{TreeActor, TreeModel, TreePlan},
    video::{VideoActor, VideoPlan},
};

pub use crate::author::{CONTENT_LAG, DRAW_SECONDS, MILLISECOND, STAGGER, millis, spread};
pub use crate::rune_surface::ScoreRuneSurface as RuneSurface;

/// Calibrated spring motion profiles from `explainer-motion/TECHNIQUES.md`
/// (re-exposing [`SpringPlan`] constants).
pub struct Feel;

impl Feel {
    /// A rigid panel settling into place: 0.6 s, bounce 0.12.
    pub const PANEL: SpringPlan = SpringPlan::PANEL;
    /// Ink following its panel, or an overlay fading in: 0.36 s, no bounce.
    pub const CONTENT: SpringPlan = SpringPlan::CONTENT;
    /// Spatial entrance and persistent layout room opening: 0.45 s, no bounce.
    pub const ENTER: SpringPlan = SpringPlan::ENTER;
    /// Clean exit fade in place, faster than entry: 0.24 s, no bounce.
    pub const EXIT: SpringPlan = SpringPlan::EXIT;
    /// Point-to-point spatial translation or anchor weight glide: 0.6 s, no bounce.
    pub const MOVE: SpringPlan = SpringPlan::MOVE;
    /// A quick state change, focus shift, or status cross-fade: 0.3 s, no bounce.
    pub const SNAP: SpringPlan = SpringPlan::SNAP;
    /// A small floating UI pop with restrained overshoot: 0.32 s, bounce 0.18.
    pub const POP: SpringPlan = SpringPlan::POP;
    /// A camera move with weight and a natural tail: 1.6 s, critically damped.
    pub const CAMERA: SpringPlan = SpringPlan::CAMERA;
    /// A hero landing with a little overshoot: 0.85 s, bounce 0.2.
    pub const LIVELY: SpringPlan = SpringPlan::LIVELY;
}

/// The choreographic interval `[start, end]` occupied by a [`Beat`] on the
/// plan clock, in nanoseconds (`start <= end`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

impl Span {
    /// Construct a span `[start, end]`, clamping `end` to be at least `start`.
    pub const fn new(start: u64, end: u64) -> Self {
        let end = if end < start { start } else { end };
        Self { start, end }
    }

    /// Zero-duration span `[at, at]` for an impulse beat.
    pub const fn impulse(at: u64) -> Self {
        Self { start: at, end: at }
    }

    /// Choreographic duration `end - start` in nanoseconds.
    pub const fn duration(self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    /// `delta` nanoseconds after this span's `start`.
    pub const fn after_start(self, delta: u64) -> u64 {
        self.start + delta
    }

    /// `delta` nanoseconds before this span's `end` (clamped at zero).
    pub const fn before_end(self, delta: u64) -> u64 {
        self.end.saturating_sub(delta)
    }
}

/// Any plan-clock anchor that can cue a [`Beat`]: a raw `u64` nanosecond
/// timestamp (such as `Spoken::at("phrase")`) or the completion of a prior
/// [`Span`].
pub trait CueTime: Copy {
    /// Resolved nanosecond timestamp on the plan clock.
    fn cue_nanos(self) -> u64;

    /// `delta` nanoseconds after this cue time.
    fn after(self, delta: u64) -> u64 {
        self.cue_nanos() + delta
    }

    /// `lead` nanoseconds before this cue time, saturating at zero.
    fn early(self, lead: u64) -> u64 {
        self.cue_nanos().saturating_sub(lead)
    }

    /// This cue time, or `earliest` if `earliest` is later (`max`).
    ///
    /// Expresses causal guards such as `f("injected").not_before(contact.after(millis(340)))`.
    fn not_before(self, earliest: impl CueTime) -> u64 {
        self.cue_nanos().max(earliest.cue_nanos())
    }

    /// Earliest launch of a reply to a packet that arrives at this time
    /// ([`crate::stage::reply_after`]: `340 ms` gather + `80 ms` reaction).
    fn reply(self) -> u64 {
        reply_after(self.cue_nanos())
    }
}

impl CueTime for u64 {
    fn cue_nanos(self) -> u64 {
        self
    }
}

impl CueTime for Span {
    fn cue_nanos(self) -> u64 {
        self.end
    }
}

/// A composable unit of choreography that emits writes into a [`PlanBuilder`].
pub trait Beat: Sized {
    /// Emit this beat's writes into `scene` starting at `at` nanoseconds, and
    /// return its occupied choreographic [`Span`].
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span;

    /// Sequential composition: play `self` at `at`, then play `next` at
    /// `self.end`.
    ///
    /// Obeys associativity (`a.then(b).then(c) == a.then(b.then(c))`) and
    /// identity (`empty().then(a) == a == a.then(empty())`).
    fn then<B: Beat>(self, next: B) -> Then<Self, B> {
        Then {
            first: self,
            second: next,
        }
    }

    /// Play `self`, wait `gap` nanoseconds after `self.end`, then play `next`.
    ///
    /// Equivalent to `self.then(next.after(gap))` and `self.then(hold(gap)).then(next)`.
    fn then_after<B: Beat>(self, gap: u64, next: B) -> Then<Self, After<B>> {
        self.then(next.after(gap))
    }

    /// Parallel composition: start both `self` and `other` at `at`; complete
    /// when both have completed (`max(self.end, other.end)`).
    ///
    /// Obeys associativity, identity (`empty()`), and delay distributivity
    /// (`a.also(b).after(d) == a.after(d).also(b.after(d))`).
    fn also<B: Beat>(self, other: B) -> Also<Self, B> {
        Also {
            left: self,
            right: other,
        }
    }

    /// Accompany `self` with a subordinate beat `subordinate` that starts at
    /// `self.start`, while keeping `self`'s choreographic [`Span`].
    fn with<B: Beat>(self, subordinate: B) -> With<Self, B> {
        With {
            primary: self,
            subordinate,
        }
    }

    /// Trigger a subordinate `reaction` when `self` finishes (`self.end`),
    /// while preserving `self`'s [`Span`].
    fn on_end<B: Beat>(self, reaction: B) -> OnEnd<Self, B> {
        OnEnd {
            primary: self,
            reaction,
        }
    }

    /// Delay this beat by `delay` nanoseconds relative to the cue time.
    ///
    /// The returned span is `[at, inner.end]` where `inner` starts at `at + delay`.
    fn after(self, delay: u64) -> After<Self> {
        After { delay, beat: self }
    }

    /// Start this beat `lead` nanoseconds *before* the cue time (saturating at 0).
    fn early(self, lead: u64) -> Early<Self> {
        Early { lead, beat: self }
    }

    /// Extend this beat's sequential span by `hold_nanos` of stillness after it finishes.
    fn hold_for(self, hold_nanos: u64) -> Then<Self, Hold> {
        self.then(hold(hold_nanos))
    }
}

impl<F> Beat for F
where
    F: FnOnce(&mut PlanBuilder, u64) -> Span,
{
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        self(scene, at)
    }
}

/// The identity beat: writes nothing and occupies `[at, at]` (`duration == 0`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Empty;

/// Identity beat that emits no writes and advances time by `0 ns`.
pub const fn empty() -> Empty {
    Empty
}

impl Beat for Empty {
    fn play(self, _scene: &mut PlanBuilder, at: u64) -> Span {
        Span::impulse(at)
    }
}

/// A pure wait of `duration_nanos` that emits no writes and occupies
/// `[at, at + duration_nanos]`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hold {
    pub duration_nanos: u64,
}

/// Wait `duration_nanos` on the timeline without writing any events.
pub const fn hold(duration_nanos: u64) -> Hold {
    Hold { duration_nanos }
}

impl Beat for Hold {
    fn play(self, _scene: &mut PlanBuilder, at: u64) -> Span {
        Span::new(at, at + self.duration_nanos)
    }
}

/// A zero-duration beat (`Span::impulse(at)`) constructed from a closure.
pub struct Impulse<F> {
    f: F,
}

/// Construct an impulse beat that writes at `at` and advances the cursor by `0 ns`.
pub fn impulse<F: FnOnce(&mut PlanBuilder, u64)>(f: F) -> Impulse<F> {
    Impulse { f }
}

impl<F: FnOnce(&mut PlanBuilder, u64)> Beat for Impulse<F> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        (self.f)(scene, at);
        Span::impulse(at)
    }
}

/// A span-bearing beat constructed from a closure returning its completion time `end`.
pub struct Action<F> {
    f: F,
}

/// Construct a beat from an action that starts at `at` and returns its `end` timestamp.
pub fn action<F: FnOnce(&mut PlanBuilder, u64) -> u64>(f: F) -> Action<F> {
    Action { f }
}

impl<F: FnOnce(&mut PlanBuilder, u64) -> u64> Beat for Action<F> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        let end = (self.f)(scene, at);
        Span::new(at, end)
    }
}

/// Sequential composition of two beats (`first.then(second)`).
#[derive(Clone, Copy, Debug)]
pub struct Then<A, B> {
    pub first: A,
    pub second: B,
}

impl<A: Beat, B: Beat> Beat for Then<A, B> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        let first_span = self.first.play(scene, at);
        let second_span = self.second.play(scene, first_span.end);
        Span::new(at, second_span.end)
    }
}

/// Parallel composition of two beats (`left.also(right)`).
#[derive(Clone, Copy, Debug)]
pub struct Also<A, B> {
    pub left: A,
    pub right: B,
}

impl<A: Beat, B: Beat> Beat for Also<A, B> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        let left_span = self.left.play(scene, at);
        let right_span = self.right.play(scene, at);
        Span::new(at, left_span.end.max(right_span.end))
    }
}

/// Primary beat accompanied by a subordinate beat at `start` (`primary.with(subordinate)`).
#[derive(Clone, Copy, Debug)]
pub struct With<A, B> {
    pub primary: A,
    pub subordinate: B,
}

impl<A: Beat, B: Beat> Beat for With<A, B> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        let span = self.primary.play(scene, at);
        let _ = self.subordinate.play(scene, at);
        span
    }
}

/// Primary beat that triggers `reaction` at `primary.end`, keeping `primary`'s span.
#[derive(Clone, Copy, Debug)]
pub struct OnEnd<A, B> {
    pub primary: A,
    pub reaction: B,
}

impl<A: Beat, B: Beat> Beat for OnEnd<A, B> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        let span = self.primary.play(scene, at);
        let _ = self.reaction.play(scene, span.end);
        span
    }
}

/// A beat delayed by `delay` nanoseconds (`beat.after(delay)`).
#[derive(Clone, Copy, Debug)]
pub struct After<B> {
    pub delay: u64,
    pub beat: B,
}

impl<B: Beat> Beat for After<B> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        let inner = self.beat.play(scene, at + self.delay);
        Span::new(at, inner.end)
    }
}

/// A beat shifted earlier by `lead` nanoseconds (`beat.early(lead)`).
#[derive(Clone, Copy, Debug)]
pub struct Early<B> {
    pub lead: u64,
    pub beat: B,
}

impl<B: Beat> Beat for Early<B> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        let shifted = at.saturating_sub(self.lead);
        self.beat.play(scene, shifted)
    }
}

/// Sequential composition of a homogeneous collection of beats.
pub struct Seq<I> {
    beats: I,
}

/// Play an iterator of beats one after another.
pub fn seq<I>(beats: I) -> Seq<I> {
    Seq { beats }
}

impl<B: Beat, I: IntoIterator<Item = B>> Beat for Seq<I> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        let mut cursor = at;
        for beat in self.beats {
            cursor = beat.play(scene, cursor).end;
        }
        Span::new(at, cursor)
    }
}

/// Parallel composition of a homogeneous collection of beats.
pub struct All<I> {
    beats: I,
}

/// Start an iterator of beats at the same timestamp; finish when the last finishes.
pub fn all<I>(beats: I) -> All<I> {
    All { beats }
}

impl<B: Beat, I: IntoIterator<Item = B>> Beat for All<I> {
    fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
        let mut end = at;
        for beat in self.beats {
            end = end.max(beat.play(scene, at).end);
        }
        Span::new(at, end)
    }
}

/// Staggered parallel composition: item `i` starts at `at + i * gap`.
pub struct Stagger<I, F> {
    gap: u64,
    items: I,
    make_beat: F,
}

/// Start one beat per item, spaced `gap` nanoseconds apart from `at`.
///
/// Satisfies the law:
/// `stagger(gap, [x0, x1, ...], f) == all(items.enumerate().map(|(i, x)| f(x).after(i * gap)))`.
pub fn stagger<T, I, B, F>(gap: u64, items: I, make_beat: F) -> Stagger<I, F>
where
    I: IntoIterator<Item = T>,
    F: FnMut(T) -> B,
{
    Stagger {
        gap,
        items,
        make_beat,
    }
}

impl<T, I, B, F> Beat for Stagger<I, F>
where
    I: IntoIterator<Item = T>,
    B: Beat,
    F: FnMut(T) -> B,
{
    fn play(mut self, scene: &mut PlanBuilder, at: u64) -> Span {
        let mut latest_end = at;
        for (index, item) in self.items.into_iter().enumerate() {
            let item_start = at + self.gap * index as u64;
            let span = (self.make_beat)(item).play(scene, item_start);
            latest_end = latest_end.max(span.end);
        }
        Span::new(at, latest_end)
    }
}

/// Run one beat per item in parallel at the same cue time (`stagger(0, items, make_beat)`).
pub fn each<T, I, B, F>(items: I, make_beat: F) -> Stagger<I, F>
where
    I: IntoIterator<Item = T>,
    F: FnMut(T) -> B,
{
    stagger(0, items, make_beat)
}

/// Indexed variant of [`stagger`] when the beat depends on its `0..` index.
pub struct StaggerIndexed<I, F> {
    gap: u64,
    items: I,
    make_beat: F,
}

/// Start one beat per `(index, item)`, spaced `gap` nanoseconds apart from `at`.
pub fn stagger_indexed<T, I, B, F>(gap: u64, items: I, make_beat: F) -> StaggerIndexed<I, F>
where
    I: IntoIterator<Item = T>,
    F: FnMut(usize, T) -> B,
{
    StaggerIndexed {
        gap,
        items,
        make_beat,
    }
}

impl<T, I, B, F> Beat for StaggerIndexed<I, F>
where
    I: IntoIterator<Item = T>,
    B: Beat,
    F: FnMut(usize, T) -> B,
{
    fn play(mut self, scene: &mut PlanBuilder, at: u64) -> Span {
        let mut latest_end = at;
        for (index, item) in self.items.into_iter().enumerate() {
            let item_start = at + self.gap * index as u64;
            let span = (self.make_beat)(index, item).play(scene, item_start);
            latest_end = latest_end.max(span.end);
        }
        Span::new(at, latest_end)
    }
}

/// Compose a heterogeneous list of beats in sequence using [`Beat::then`].
#[macro_export]
macro_rules! chain {
    () => {
        $crate::score::empty()
    };
    ($first:expr $(,)?) => {
        $first
    };
    ($first:expr, $($rest:expr),+ $(,)?) => {
        $crate::score::Beat::then($first, $crate::chain!($($rest),+))
    };
}

/// Compose a heterogeneous list of beats in parallel using [`Beat::also`].
#[macro_export]
macro_rules! all {
    () => {
        $crate::score::empty()
    };
    ($first:expr $(,)?) => {
        $first
    };
    ($first:expr, $($rest:expr),+ $(,)?) => {
        $crate::score::Beat::also($first, $crate::all!($($rest),+))
    };
}

/// Play one or more beats in parallel anchored at `time` on `scene`, returning
/// their combined [`Span`].
///
/// Supports both `at!(scene, time => b1, b2, ...)` (cue-sheet style) and
/// `at!(scene, time, b1, b2, ...)`.
#[macro_export]
macro_rules! at {
    ($scene:expr, $time:expr => $($beat:expr),+ $(,)?) => {
        $scene.at($time, $crate::all![$($beat),+])
    };
    ($scene:expr, $time:expr, $($beat:expr),+ $(,)?) => {
        $scene.at($time, $crate::all![$($beat),+])
    };
}

pub use {all as parallel, at, chain};

/// Schedule a layer audio clip at the cue time (impulse).
pub fn sound(
    id: impl Into<String>,
    path: impl Into<PathBuf>,
    duration_nanos: u64,
    gain_db: f32,
) -> impl Beat {
    let id = id.into();
    let path = path.into();
    impulse(move |scene: &mut PlanBuilder, at| {
        scene.media(MediaPlan {
            id,
            path,
            kind: MediaKindPlan::Audio,
            role: MediaRolePlan::Layer,
            source_start_nanos: 0,
            source_end_nanos: duration_nanos,
            timeline_start_nanos: at,
            timeline_end_nanos: at + duration_nanos,
            gain_db,
        });
    })
}

/// A score runner bound to `&mut PlanBuilder`.
pub struct Score<'a> {
    scene: &'a mut PlanBuilder,
}

impl<'a> Score<'a> {
    pub fn new(scene: &'a mut PlanBuilder) -> Self {
        Self { scene }
    }

    pub fn scene(&mut self) -> &mut PlanBuilder {
        self.scene
    }

    /// Play `beat` anchored at `time` on the plan clock and return its [`Span`].
    pub fn at(&mut self, time: impl CueTime, beat: impl Beat) -> Span {
        beat.play(self.scene, time.cue_nanos())
    }

    /// Play `beat` at `time` and record a named [`crate::plan::CuePlan`] for its span.
    pub fn cue(&mut self, id: impl Into<String>, time: impl CueTime, beat: impl Beat) -> Span {
        let span = self.at(time, beat);
        self.scene.cue(id, span.start, span.end);
        span
    }
}

impl PlanBuilder {
    /// Open a composable [`Score`] over this plan builder.
    pub fn score(&mut self) -> Score<'_> {
        Score::new(self)
    }

    /// Play `beat` anchored at `time` on this plan builder and return its [`Span`].
    pub fn at(&mut self, time: impl CueTime, beat: impl Beat) -> Span {
        beat.play(self, time.cue_nanos())
    }

    /// Play `beat` at `time` and record a named [`crate::plan::CuePlan`] for its span.
    pub fn play_cue(&mut self, id: impl Into<String>, time: impl CueTime, beat: impl Beat) -> Span {
        let span = self.at(time, beat);
        self.cue(id, span.start, span.end);
        span
    }
}

// ===========================================================================
// Value Actor Handles (methods take `&self` and return `impl Beat + '_`)
// ===========================================================================

/// Value handle for a Stage actor whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Stage {
    inner: StageActor,
}

impl From<StageActor> for Stage {
    fn from(inner: StageActor) -> Self {
        Self { inner }
    }
}

impl StageActor {
    /// Return a value [`Stage`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Stage {
        Stage {
            inner: self.clone(),
        }
    }
}

impl Stage {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &StagePlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: StageActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn plan(&self) -> &StagePlan {
        self.inner.plan()
    }

    /// Declare `property` with `initial` value if not already declared (impulse).
    pub fn channel(&self, property: impl Into<String>, initial: f32) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, _at| {
            scene.channel(self.inner.actor(), &property, initial);
        })
    }

    /// Declare multiple `(property, initial)` channels in order (impulse).
    pub fn channels<'a, I, S>(&'a self, pairs: I) -> impl Beat + 'a
    where
        I: IntoIterator<Item = (S, f32)> + 'a,
        S: AsRef<str>,
    {
        impulse(move |scene, _at| {
            for (property, initial) in pairs {
                scene.channel(self.inner.actor(), property.as_ref(), initial);
            }
        })
    }

    /// Jump `property` to `value` at the cue time (impulse).
    pub fn set(&self, property: impl Into<String>, value: f32) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner.clone().set(scene, &property, at, value);
        })
    }

    /// Critically-damped spring on `property` to `target` over `seconds` (impulse).
    pub fn to(&self, property: impl Into<String>, target: f32, seconds: f32) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner.clone().to(scene, &property, at, target, seconds);
        })
    }

    /// Spring `property` to `target` with `seconds` and `bounce` (impulse).
    pub fn bounce(
        &self,
        property: impl Into<String>,
        target: f32,
        seconds: f32,
        bounce: f32,
    ) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .bounce(scene, &property, at, target, seconds, bounce);
        })
    }

    /// Spring `property` to `target` with a named [`SpringPlan`] feel (impulse).
    pub fn spring(
        &self,
        property: impl Into<String>,
        target: f32,
        feel: SpringPlan,
    ) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            let ch = scene.channel(self.inner.actor(), &property, 0.0);
            scene.spring_with(&ch, at, target, feel);
        })
    }

    /// Ease `property` to `target` over `seconds` along `curve` (impulse).
    pub fn ease(
        &self,
        property: impl Into<String>,
        target: f32,
        seconds: f32,
        curve: Ease,
    ) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .ease(scene, &property, at, target, seconds, curve);
        })
    }

    /// Start an elapsed-seconds clock on `property` running to the end of the scene (impulse).
    pub fn clock(&self, property: impl Into<String>) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner.clone().clock(scene, &property, at);
        })
    }

    /// Start a fixed-lifetime elapsed-seconds clock on `property` for `seconds` (impulse).
    pub fn clock_for(&self, property: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner.clone().clock_for(scene, &property, at, seconds);
        })
    }

    /// Instant-attack flash to `peak` followed by a cubic-out decay to `rest` (impulse).
    pub fn hit(&self, property: impl Into<String>, peak: f32, rest: f32) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner.clone().hit(scene, &property, at, peak, rest);
        })
    }

    /// Two-frame shove on `[x, y]` that springs back past rest (impulse).
    pub fn kick<'a>(&'a self, channels: [&'a str; 2], offset: [f32; 2]) -> impl Beat + 'a {
        impulse(move |scene, at| {
            self.inner.clone().kick(scene, channels, at, offset);
        })
    }

    /// Camera impact jolt along `direction` with `strength` (impulse).
    pub fn jolt(&self, direction: [f32; 2], strength: f32) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().jolt(scene, at, direction, strength);
        })
    }

    /// Settle `card` into place; spans the `500 ms` until the panel is ready to wire.
    pub fn settle_in(&self, card: impl Into<String>) -> impl Beat + '_ {
        let card = card.into();
        action(move |scene, at| self.inner.clone().settle_in(scene, &card, at))
    }

    /// Type `label` in at `chars_per_second`; spans until the last character appears.
    pub fn type_in(&self, label: impl Into<String>, chars_per_second: f32) -> impl Beat + '_ {
        let label = label.into();
        action(move |scene, at| {
            self.inner
                .clone()
                .type_in(scene, &label, at, chars_per_second)
        })
    }

    /// Draw `beam` over `seconds` (after the port pop); spans until contact at the far end.
    pub fn connect(&self, beam: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let beam = beam.into();
        action(move |scene, at| self.inner.clone().connect(scene, &beam, at, seconds))
    }

    /// Back-timed connection that reaches its far port at the cue time `at`.
    pub fn connect_contacting(&self, beam: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let beam = beam.into();
        let lead = whole_millis(PORT_POP_SECONDS) + whole_millis(seconds);
        action(move |scene, at| {
            self.inner
                .clone()
                .connect(scene, &beam, at.saturating_sub(lead), seconds)
        })
    }

    /// Launch `packet` at the cue time and fly for `seconds`; spans `[at, arrival]`.
    pub fn send(&self, packet: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let packet = packet.into();
        action(move |scene, at| self.inner.clone().send(scene, &packet, at, seconds))
    }

    /// Back-timed packet launch that *arrives* at the cue time `at` after flying
    /// for `seconds` (launching `seconds` earlier and gathering before that).
    pub fn send_arriving(&self, packet_id: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let packet_id = packet_id.into();
        let flight = whole_millis(seconds);
        move |scene: &mut PlanBuilder, arrival: u64| {
            let launch = arrival.saturating_sub(flight);
            let landed = self.inner.clone().send(scene, &packet_id, launch, seconds);
            let _ = packet::GATHER;
            Span::new(launch, landed)
        }
    }

    /// Pluck `beam` like a struck cable (impulse).
    pub fn twang(&self, beam: impl Into<String>) -> impl Beat + '_ {
        let beam = beam.into();
        impulse(move |scene, at| {
            self.inner.clone().twang(scene, &beam, at);
        })
    }

    /// Flash a card's ink or pulse an orb on arrival (impulse).
    pub fn land(&self, element: impl Into<String>) -> impl Beat + '_ {
        let element = element.into();
        impulse(move |scene, at| {
            self.inner.clone().land(scene, &element, at);
        })
    }

    /// Three-frame horizontal glitch burst on `card` (`27 ms` steps, then 0).
    /// Spans until the card returns to rest (`3 * 27 ms = 81 ms`).
    pub fn glitch(&self, card: impl Into<String>, seeds: [f32; 3]) -> impl Beat + '_ {
        let card = card.into();
        action(move |scene, at| self.inner.clone().glitch(scene, &card, at, seeds))
    }

    /// Return a value [`Camera`] handle whose shot methods produce composable [`Beat`]s.
    pub fn camera(&self) -> Camera {
        Camera {
            inner: self.inner.camera(),
        }
    }

    /// Glide `property` to `target` in `seconds` on a minimum-jerk (smootherstep) curve (impulse).
    pub fn glide(&self, property: impl Into<String>, target: f32, seconds: f32) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .glide(scene, &property, at, target, seconds);
        })
    }

    /// Fade `element` in to `opacity` on a `seconds` spring, starting hidden (impulse).
    pub fn fade_in(
        &self,
        element: impl Into<String>,
        opacity: f32,
        seconds: f32,
    ) -> impl Beat + '_ {
        let element = element.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .fade_in(scene, &element, at, opacity, seconds);
        })
    }

    /// Fade `element` out to `0.0` on a `seconds` spring from its resting opacity (impulse).
    pub fn fade_out(&self, element: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let element = element.into();
        impulse(move |scene, at| {
            self.inner.clone().fade_out(scene, &element, at, seconds);
        })
    }

    /// Calibrated orb entrance (scale, deblur, turn, fade in) (impulse).
    pub fn orb_in(&self, orb: impl Into<String>, entrance: OrbEntrance) -> impl Beat + '_ {
        let orb = orb.into();
        impulse(move |scene, at| {
            self.inner.clone().orb_in(scene, &orb, at, entrance);
        })
    }

    /// Send `packet` along its multi-leg route, landing on each stop; spans until final arrival.
    pub fn relay(&self, packet: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let packet = packet.into();
        action(move |scene, at| {
            self.inner
                .clone()
                .relay(scene, &packet, at, seconds)
                .last()
                .copied()
                .unwrap_or(at)
        })
    }

    /// Morph `form` to shape `index` over `seconds`; spans `seconds`.
    pub fn morph(&self, form: impl Into<String>, index: usize, seconds: f32) -> impl Beat + '_ {
        let form = form.into();
        action(move |scene, at| self.inner.clone().morph(scene, &form, at, index, seconds))
    }

    /// Strike lightning along `bolt`; spans until the first return stroke connects.
    pub fn zap(&self, bolt: impl Into<String>) -> impl Beat + '_ {
        let bolt = bolt.into();
        action(move |scene, at| self.inner.clone().zap(scene, &bolt, at))
    }

    /// Ease `element`'s electric charge to `intensity` over `seconds` (impulse).
    pub fn charge(
        &self,
        element: impl Into<String>,
        intensity: f32,
        seconds: f32,
    ) -> impl Beat + '_ {
        let element = element.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .charge(scene, &element, at, intensity, seconds);
        })
    }

    /// Keep an electric arc humming along `bolt` at `intensity` (impulse).
    pub fn hum(&self, bolt: impl Into<String>, intensity: f32, seconds: f32) -> impl Beat + '_ {
        let bolt = bolt.into();
        impulse(move |scene, at| {
            self.inner.clone().hum(scene, &bolt, at, intensity, seconds);
        })
    }

    /// Burn `card` away to ash; spans until the card is gone.
    pub fn dissolve(&self, card: impl Into<String>) -> impl Beat + '_ {
        let card = card.into();
        action(move |scene, at| self.inner.clone().dissolve(scene, &card, at))
    }

    /// Materialize `card` out of ash over `seconds`; spans `seconds`.
    pub fn materialize(&self, card: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let card = card.into();
        action(move |scene, at| self.inner.clone().materialize(scene, &card, at, seconds))
    }

    /// Sweep a scan line down `card` over `seconds`; spans `seconds`.
    pub fn scan(&self, card: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let card = card.into();
        action(move |scene, at| self.inner.clone().scan(scene, &card, at, seconds))
    }

    /// Raise `shield` over `seconds` (impulse).
    pub fn raise(&self, shield: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let shield = shield.into();
        impulse(move |scene, at| {
            self.inner.clone().raise(scene, &shield, at, seconds);
        })
    }

    /// Lower `shield` over `seconds` (impulse).
    pub fn lower(&self, shield: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let shield = shield.into();
        impulse(move |scene, at| {
            self.inner.clone().lower(scene, &shield, at, seconds);
        })
    }

    /// Run `post.rewind` tape interference with `chroma` hit; spans `1.4 s`.
    pub fn rewind(&self, chroma: f32) -> impl Beat + '_ {
        action(move |scene, at| self.inner.clone().rewind(scene, at, chroma))
    }

    /// Play `orb`'s burst backwards over `seconds`; spans until whole.
    pub fn unburst(&self, orb: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let orb = orb.into();
        action(move |scene, at| self.inner.clone().unburst(scene, &orb, at, seconds))
    }

    /// Knock `card` away from `source` when its burst pressure wave passes; spans until arrival.
    pub fn shock_kick(
        &self,
        source: impl Into<String>,
        card: impl Into<String>,
        push: f32,
        falloff: Option<f32>,
    ) -> impl Beat + '_ {
        let source = source.into();
        let card = card.into();
        action(move |scene, at| {
            self.inner
                .clone()
                .shock_kick(scene, &source, at, &card, push, falloff)
        })
    }

    /// Resolve `card`'s status spinner (started at `started`) into its mark; spans until drawn.
    pub fn resolve_spinner(&self, card: impl Into<String>, started: u64) -> impl Beat + '_ {
        let card = card.into();
        action(move |scene, at| {
            self.inner
                .clone()
                .resolve_spinner(scene, &card, started, at)
        })
    }

    /// Two-ring halo entrance (`inner` then `outer` 60 ms later) (impulse).
    pub fn halo<'a>(&'a self, rings: [(&'a str, f32); 2], seconds: f32) -> impl Beat + 'a {
        impulse(move |scene, at| {
            self.inner.clone().halo(scene, rings, at, seconds);
        })
    }

    /// Release a two-ring halo in reverse (impulse).
    pub fn halo_out<'a>(&'a self, rings: [&'a str; 2], seconds: f32) -> impl Beat + 'a {
        impulse(move |scene, at| {
            self.inner.clone().halo_out(scene, rings, at, seconds);
        })
    }

    /// Sweep `ring` as a timer to `sweep` over `seconds`; spans `seconds`.
    pub fn ring_timer(&self, ring: impl Into<String>, seconds: f32, sweep: f32) -> impl Beat + '_ {
        let ring = ring.into();
        action(move |scene, at| {
            self.inner
                .clone()
                .ring_timer(scene, &ring, at, seconds, sweep)
        })
    }

    /// Step `cards` back to `amount` of dimness on `seconds` springs (impulse).
    pub fn dim<'a, I, S>(&'a self, cards: I, amount: f32, seconds: f32) -> impl Beat + 'a
    where
        I: IntoIterator<Item = S> + 'a,
        S: AsRef<str>,
    {
        impulse(move |scene, at| {
            self.inner.clone().dim(scene, cards, at, amount, seconds);
        })
    }

    /// Swap two labels `[from, to]` separated by `gap`; spans until `to` starts.
    pub fn swap_labels<'a>(&'a self, pair: [&'a str; 2], gap: u64) -> impl Beat + 'a {
        action(move |scene, at| self.inner.clone().swap_labels(scene, pair, at, gap))
    }

    /// Cross-fade `card`'s status directly from `from` to `to` over `seconds`; spans `seconds`.
    pub fn swap_status(
        &self,
        card: impl Into<String>,
        pair: [usize; 2],
        seconds: f32,
    ) -> impl Beat + '_ {
        let card = card.into();
        action(move |scene, at| {
            self.inner
                .clone()
                .swap_status(scene, &card, at, pair, seconds)
        })
    }

    /// Unplug `beam` over `seconds`; spans until its port resolves away.
    pub fn disconnect(&self, beam: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let beam = beam.into();
        action(move |scene, at| self.inner.clone().disconnect(scene, &beam, at, seconds))
    }
}

/// Value handle for the Stage [`CameraRig`] whose shot methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Camera {
    inner: CameraRig,
}

impl From<CameraRig> for Camera {
    fn from(inner: CameraRig) -> Self {
        Self { inner }
    }
}

impl CameraRig {
    /// Return a value [`Camera`] handle whose shot methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Camera {
        Camera {
            inner: self.clone(),
        }
    }
}

impl Camera {
    pub fn rig(&self) -> &CameraRig {
        &self.inner
    }

    pub fn pose(&self, scene: &PlanBuilder, at_nanos: u64) -> StageCamera {
        self.inner.pose(scene, at_nanos)
    }

    pub fn establish(&self, back: f32, seconds: f32) -> impl Beat + '_ {
        action(move |scene, at| self.inner.establish(scene, at, back, seconds))
    }

    pub fn frame<'a>(
        &'a self,
        targets: &'a [&'a str],
        padding: f32,
        motion: Move,
    ) -> impl Beat + 'a {
        action(move |scene, at| {
            self.inner
                .frame(scene, targets, padding, at, motion)
                .unwrap_or_else(|err| panic!("camera frame: {err:#}"))
        })
    }

    pub fn push_in(&self, distance: f32, motion: Move) -> impl Beat + '_ {
        action(move |scene, at| self.inner.push_in(scene, at, distance, motion))
    }

    pub fn pull_back(&self, distance: f32, motion: Move) -> impl Beat + '_ {
        action(move |scene, at| self.inner.pull_back(scene, at, distance, motion))
    }

    pub fn drift(&self, toward: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let toward = toward.into();
        action(move |scene, at| {
            self.inner
                .drift(scene, &toward, at, seconds)
                .unwrap_or_else(|err| panic!("camera drift: {err:#}"))
        })
    }

    pub fn whip<'a>(
        &'a self,
        targets: &'a [&'a str],
        padding: f32,
        seconds: f32,
    ) -> impl Beat + 'a {
        action(move |scene, at| {
            self.inner
                .whip(scene, targets, padding, at, seconds)
                .unwrap_or_else(|err| panic!("camera whip: {err:#}"))
        })
    }

    pub fn orbit(
        &self,
        around: impl Into<String>,
        angles: [f32; 2],
        motion: Move,
    ) -> impl Beat + '_ {
        let around = around.into();
        action(move |scene, at| {
            self.inner
                .orbit(scene, &around, at, angles, motion)
                .unwrap_or_else(|err| panic!("camera orbit: {err:#}"))
        })
    }

    pub fn dolly_zoom(
        &self,
        subject: impl Into<String>,
        dolly: f32,
        motion: Move,
    ) -> impl Beat + '_ {
        let subject = subject.into();
        action(move |scene, at| {
            self.inner
                .dolly_zoom(scene, &subject, at, dolly, motion)
                .unwrap_or_else(|err| panic!("camera dolly_zoom: {err:#}"))
        })
    }

    pub fn roll(&self, radians: f32, motion: Move) -> impl Beat + '_ {
        action(move |scene, at| self.inner.roll(scene, at, radians, motion))
    }

    pub fn focus_on(&self, element: impl Into<String>, motion: Move) -> impl Beat + '_ {
        let element = element.into();
        action(move |scene, at| {
            self.inner
                .focus_on(scene, &element, at, motion)
                .unwrap_or_else(|err| panic!("camera focus_on: {err:#}"))
        })
    }

    pub fn aperture(&self, dof: f32, motion: Move) -> impl Beat + '_ {
        action(move |scene, at| self.inner.aperture(scene, at, dof, motion))
    }

    pub fn follow(&self, target: impl Into<String>, motion: Move) -> impl Beat + '_ {
        let target = target.into();
        action(move |scene, at| {
            self.inner
                .follow(scene, &target, at, motion)
                .unwrap_or_else(|err| panic!("camera follow: {err:#}"))
        })
    }

    pub fn release(&self, motion: Move) -> impl Beat + '_ {
        action(move |scene, at| {
            self.inner
                .release(scene, at, motion)
                .unwrap_or_else(|err| panic!("camera release: {err:#}"))
        })
    }

    pub fn handheld(&self, amount: f32, seconds: f32) -> impl Beat + '_ {
        action(move |scene, at| self.inner.handheld(scene, at, amount, seconds))
    }
}

/// Value handle for a Caption actor whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Caption {
    inner: CaptionActor,
}

impl From<CaptionActor> for Caption {
    fn from(inner: CaptionActor) -> Self {
        Self { inner }
    }
}

impl CaptionActor {
    /// Return a value [`Caption`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Caption {
        Caption {
            inner: self.clone(),
        }
    }
}

impl Caption {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &CaptionPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: CaptionActor::declare(scene, id, plan)?,
        })
    }

    /// Declare the standard top-left explainer header (`#number  title`).
    pub fn header(scene: &mut PlanBuilder, label: &str, title: &str) -> Result<Self> {
        Ok(Self {
            inner: chrome::header(scene, label, title)?,
        })
    }

    /// Declare a top-right status chip (`● text`).
    pub fn chip(scene: &mut PlanBuilder, id: &str, dot: Tone, text: &str) -> Result<Self> {
        Ok(Self {
            inner: chrome::chip(scene, id, dot, text)?,
        })
    }

    /// Declare a bottom-left explainer footer from `(text, tone)` pairs
    /// (matching [`crate::stage::StageElement::label`]).
    pub fn footer(scene: &mut PlanBuilder, id: &str, spans: &[(&str, Tone)]) -> Result<Self> {
        let spans = spans
            .iter()
            .map(|&(text, tone)| CaptionSpanPlan::new(text, tone))
            .collect();
        Ok(Self {
            inner: chrome::footer(scene, id, spans)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    /// Fade and rise in (impulse).
    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            caption::show(scene, self.inner.actor(), at);
        })
    }

    /// Fade out in place (impulse).
    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            caption::hide(scene, self.inner.actor(), at);
        })
    }

    /// Glide to `anchor` (impulse).
    pub fn move_to(&self, anchor: impl Into<String>) -> impl Beat + '_ {
        let anchor = anchor.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .move_to(scene, &anchor, at)
                .unwrap_or_else(|err| panic!("caption '{}': {err:#}", self.inner.id()));
        })
    }

    /// Type the caption in at `chars_per_second`, holding the caret for
    /// `caret_hold_seconds` afterward. Spans until typing finishes.
    pub fn type_in(&self, chars_per_second: f32, caret_hold_seconds: f32) -> impl Beat + '_ {
        action(move |scene, at| {
            self.inner
                .type_in_at(scene, at, chars_per_second, caret_hold_seconds)
        })
    }
}

/// Value handle for a Callout actor whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Callout {
    inner: CalloutActor,
}

impl From<CalloutActor> for Callout {
    fn from(inner: CalloutActor) -> Self {
        Self { inner }
    }
}

impl CalloutActor {
    /// Return a value [`Callout`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Callout {
        Callout {
            inner: self.clone(),
        }
    }
}

impl Callout {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &CalloutPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: CalloutActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    /// Draw the leader out from the anchor mark and reveal the label; spans
    /// until the leader reaches the label (`420 ms`).
    pub fn show(&self) -> impl Beat + '_ {
        action(move |scene, at| self.inner.clone().show(scene, at))
    }

    /// Fade the label and retract the leader (impulse).
    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }

    /// Glide the callout to `anchor` on a critically damped spring (impulse).
    pub fn move_to(&self, anchor: impl Into<String>) -> impl Beat + '_ {
        let anchor = anchor.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .move_to(scene, &anchor, at)
                .unwrap_or_else(|err| panic!("callout '{}': {err:#}", self.inner.id()));
        })
    }

    /// Flare the anchor ring and leader, then decay (impulse).
    pub fn emphasize(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().emphasize(scene, at);
        })
    }
}

/// Value handle for a Rolling Number actor whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct RollingNumber {
    inner: RollingNumberActor,
}

impl From<RollingNumberActor> for RollingNumber {
    fn from(inner: RollingNumberActor) -> Self {
        Self { inner }
    }
}

impl RollingNumberActor {
    /// Return a value [`RollingNumber`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> RollingNumber {
        RollingNumber {
            inner: self.clone(),
        }
    }
}

impl RollingNumber {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: RollingNumberPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: RollingNumberActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    /// Fade and rise in (impulse).
    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            caption::show(scene, self.inner.actor(), at);
        })
    }

    /// Fade out in place (impulse).
    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            caption::hide(scene, self.inner.actor(), at);
        })
    }

    /// Roll to `value` at the cue time; spans the roll's `duration_nanos`.
    pub fn roll(&self, value: impl Into<String>) -> impl Beat + '_ {
        let value = value.into();
        action(move |scene, at| {
            self.inner
                .roll_at(scene, at, value)
                .unwrap_or_else(|err| panic!("rolling number '{}': {err:#}", self.inner.id()))
        })
    }

    /// Glide to `anchor` (impulse).
    pub fn move_to(&self, anchor: impl Into<String>) -> impl Beat + '_ {
        let anchor = anchor.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .move_to(scene, &anchor, at)
                .unwrap_or_else(|err| panic!("rolling number '{}': {err:#}", self.inner.id()));
        })
    }
}

/// Value handle for a Tree actor whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Tree {
    inner: TreeActor,
}

impl From<TreeActor> for Tree {
    fn from(inner: TreeActor) -> Self {
        Self { inner }
    }
}

impl TreeActor {
    /// Return a value [`Tree`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Tree {
        Tree {
            inner: self.clone(),
        }
    }
}

impl Tree {
    pub fn declare(scene: &mut PlanBuilder, id: impl Into<String>, plan: TreePlan) -> Result<Self> {
        Ok(Self {
            inner: TreeActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn model(&self) -> &TreeModel {
        self.inner.model()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            caption::show(scene, self.inner.actor(), at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            caption::hide(scene, self.inner.actor(), at);
        })
    }

    pub fn open(&self, path: impl Into<String>) -> impl Beat + '_ {
        let path = path.into();
        impulse(move |scene, at| {
            self.inner
                .fold_at(scene, &path, at, true)
                .unwrap_or_else(|err| panic!("tree '{}': {err:#}", self.inner.id()));
        })
    }

    pub fn close(&self, path: impl Into<String>) -> impl Beat + '_ {
        let path = path.into();
        impulse(move |scene, at| {
            self.inner
                .fold_at(scene, &path, at, false)
                .unwrap_or_else(|err| panic!("tree '{}': {err:#}", self.inner.id()));
        })
    }

    pub fn reveal(&self, path: impl Into<String>) -> impl Beat + '_ {
        let path = path.into();
        impulse(move |scene, at| {
            self.inner
                .reveal_at(scene, &path, at)
                .unwrap_or_else(|err| panic!("tree '{}': {err:#}", self.inner.id()));
        })
    }

    pub fn scroll_to(&self, row: f32) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.scroll_to_at(scene, row, at);
        })
    }

    pub fn highlight(&self, path: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let path = path.into();
        impulse(move |scene, at| {
            self.inner
                .highlight_at(scene, &path, at, seconds)
                .unwrap_or_else(|err| panic!("tree '{}': {err:#}", self.inner.id()));
        })
    }

    pub fn set(&self, path: impl Into<String>, value: impl Into<Value>) -> impl Beat + '_ {
        let path = path.into();
        let value = value.into();
        impulse(move |scene, at| {
            self.inner
                .set_at(scene, &path, value, at)
                .unwrap_or_else(|err| panic!("tree '{}': {err:#}", self.inner.id()));
        })
    }
}

/// Value handle for a Plot actor whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Plot {
    inner: PlotActor,
}

impl From<PlotActor> for Plot {
    fn from(inner: PlotActor) -> Self {
        Self { inner }
    }
}

impl PlotActor {
    /// Return a value [`Plot`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Plot {
        Plot {
            inner: self.clone(),
        }
    }
}

impl Plot {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &PlotPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: PlotActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self, axes_seconds: f32) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().show(scene, at, axes_seconds);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }

    pub fn draw(&self, series: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let series = series.into();
        action(move |scene, at| self.inner.clone().draw(scene, &series, at, seconds))
    }

    pub fn fade(&self, series: impl Into<String>, opacity: f32) -> impl Beat + '_ {
        let series = series.into();
        impulse(move |scene, at| {
            self.inner.clone().fade(scene, &series, at, opacity);
        })
    }

    pub fn ride(&self, series: impl Into<String>, range: [f32; 2], seconds: f32) -> impl Beat + '_ {
        let series = series.into();
        action(move |scene, at| self.inner.clone().ride(scene, &series, range, at, seconds))
    }

    pub fn stop_ride(&self, series: impl Into<String>) -> impl Beat + '_ {
        let series = series.into();
        impulse(move |scene, at| {
            self.inner.clone().stop_ride(scene, &series, at);
        })
    }

    pub fn velocity(&self, series: impl Into<String>, shown: f32) -> impl Beat + '_ {
        let series = series.into();
        impulse(move |scene, at| {
            self.inner.clone().velocity(scene, &series, at, shown);
        })
    }

    pub fn mark(&self, mark_id: impl Into<String>) -> impl Beat + '_ {
        let mark_id = mark_id.into();
        impulse(move |scene, at| {
            self.inner.clone().mark(scene, &mark_id, at);
        })
    }
}

/// Value handle for a Lanes actor whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Lanes {
    inner: LanesActor,
}

impl From<LanesActor> for Lanes {
    fn from(inner: LanesActor) -> Self {
        Self { inner }
    }
}

impl LanesActor {
    /// Return a value [`Lanes`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Lanes {
        Lanes {
            inner: self.clone(),
        }
    }
}

impl Lanes {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &LanesPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: LanesActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self, seconds: f32) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().show(scene, at, seconds);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }

    pub fn scrub(&self, range: [f32; 2], seconds: f32) -> impl Beat + '_ {
        action(move |scene, at| self.inner.clone().scrub(scene, range, at, seconds))
    }

    pub fn emphasize(&self, lane: impl Into<String>, emphasis: f32) -> impl Beat + '_ {
        let lane = lane.into();
        impulse(move |scene, at| {
            self.inner.clone().emphasize(scene, &lane, at, emphasis);
        })
    }
}

/// Value handle for a Sequence Diagram actor whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Sequence {
    inner: SequenceActor,
}

impl From<SequenceActor> for Sequence {
    fn from(inner: SequenceActor) -> Self {
        Self { inner }
    }
}

impl SequenceActor {
    /// Return a value [`Sequence`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Sequence {
        Sequence {
            inner: self.clone(),
        }
    }
}

impl Sequence {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &SequencePlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: SequenceActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().show(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }

    pub fn to(&self, property: impl Into<String>, target: f32, seconds: f32) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner.clone().to(scene, &property, at, target, seconds);
        })
    }

    pub fn animate(
        &self,
        property: impl Into<String>,
        initial: f32,
        target: f32,
        seconds: f32,
    ) -> impl Beat + '_ {
        let property = property.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .animate(scene, &property, initial, at, target, seconds);
        })
    }

    pub fn reveal(&self, row: impl Into<String>) -> impl Beat + '_ {
        let row = row.into();
        impulse(move |scene, at| {
            self.inner.clone().reveal(scene, &row, at);
        })
    }

    pub fn fade(&self, row: impl Into<String>, opacity: f32) -> impl Beat + '_ {
        let row = row.into();
        impulse(move |scene, at| {
            self.inner.clone().fade(scene, &row, at, opacity);
        })
    }

    pub fn strike(&self, row: impl Into<String>) -> impl Beat + '_ {
        let row = row.into();
        impulse(move |scene, at| {
            self.inner.clone().strike(scene, &row, at);
        })
    }

    pub fn participant(
        &self,
        participant: impl Into<String>,
        property: impl Into<String>,
        initial: f32,
        target: f32,
    ) -> impl Beat + '_ {
        let participant = participant.into();
        let property = property.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .participant(scene, &participant, &property, initial, at, target);
        })
    }
}

/// Value handle for a Video Card actor whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Video {
    inner: VideoActor,
}

impl From<VideoActor> for Video {
    fn from(inner: VideoActor) -> Self {
        Self { inner }
    }
}

impl VideoActor {
    /// Return a value [`Video`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Video {
        Video {
            inner: self.clone(),
        }
    }
}

impl Video {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: VideoPlan,
        media: MediaPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: VideoActor::declare(scene, id, plan, media)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn plan(&self) -> &VideoPlan {
        self.inner.plan()
    }

    pub fn fly_in(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().fly_in(scene, at);
        })
    }

    pub fn focus(&self, region: [f32; 4], seconds: f32) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().focus(scene, at, region, seconds);
        })
    }

    pub fn unfocus(&self, seconds: f32) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().unfocus(scene, at, seconds);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }
}

/// Value handle for a [`TerminalActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Terminal {
    inner: Rc<RefCell<TerminalActor>>,
}

impl From<TerminalActor> for Terminal {
    fn from(inner: TerminalActor) -> Self {
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }
}

impl TerminalActor {
    /// Return a value [`Terminal`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Terminal {
        Terminal::from(self.clone())
    }
}

impl Terminal {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: TerminalPlan,
    ) -> Result<Self> {
        Ok(Self::from(TerminalActor::declare(scene, id, plan)?))
    }

    pub fn actor(&self) -> ActorHandle {
        self.inner.borrow().actor().clone()
    }

    pub fn id(&self) -> String {
        self.inner.borrow().id().to_owned()
    }

    pub fn show(&self) -> impl Beat + '_ {
        action(move |scene, at| self.inner.borrow_mut().show(scene, at))
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.borrow_mut().hide(scene, at);
        })
    }

    pub fn prompt(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .prompt(scene, at)
                .unwrap_or_else(|err| panic!("terminal prompt: {err:#}"));
        })
    }

    pub fn idle(&self, until_nanos: u64) -> impl Beat + '_ {
        impulse(move |scene, _at| {
            self.inner.borrow_mut().idle(scene, until_nanos);
        })
    }

    pub fn type_command(&self, text: impl Into<String>) -> impl Beat + '_ {
        let text = text.into();
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .type_command(scene, at, &text)
                .unwrap_or_else(|err| panic!("terminal type_command: {err:#}"))
        })
    }

    pub fn type_at(&self, text: impl Into<String>, chars_per_second: f32) -> impl Beat + '_ {
        let text = text.into();
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .type_at(scene, at, &text, chars_per_second)
                .unwrap_or_else(|err| panic!("terminal type_at: {err:#}"))
        })
    }

    pub fn print(
        &self,
        lines: impl IntoIterator<Item = Vec<CaptionSpanPlan>> + 'static,
    ) -> impl Beat + '_ {
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .print(scene, at, lines)
                .unwrap_or_else(|err| panic!("terminal print: {err:#}"))
        })
    }

    pub fn print_text(&self, text: impl Into<String>, tone: Tone) -> impl Beat + '_ {
        let text = text.into();
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .print_text(scene, at, &text, tone)
                .unwrap_or_else(|err| panic!("terminal print_text: {err:#}"))
        })
    }

    pub fn stream(&self, spans: Vec<CaptionSpanPlan>, chars_per_second: f32) -> impl Beat + '_ {
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .stream(scene, at, spans, chars_per_second)
                .unwrap_or_else(|err| panic!("terminal stream: {err:#}"))
        })
    }

    pub fn spin(&self, spans: Vec<CaptionSpanPlan>) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .spin(scene, at, spans)
                .unwrap_or_else(|err| panic!("terminal spin: {err:#}"));
        })
    }

    pub fn resolve(
        &self,
        task: impl Into<String>,
        mark: Mark,
        done: Vec<CaptionSpanPlan>,
    ) -> impl Beat + '_ {
        let task = task.into();
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .resolve(scene, &task, at, mark, done)
                .unwrap_or_else(|err| panic!("terminal resolve: {err:#}"))
        })
    }

    pub fn highlight(&self, line: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let line = line.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .highlight(scene, &line, at, seconds)
                .unwrap_or_else(|err| panic!("terminal highlight: {err:#}"));
        })
    }
}

/// Value handle for a [`ChatActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Chat {
    inner: Rc<RefCell<ChatActor>>,
}

impl From<ChatActor> for Chat {
    fn from(inner: ChatActor) -> Self {
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }
}

impl ChatActor {
    /// Return a value [`Chat`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Chat {
        Chat::from(self.clone())
    }
}

impl Chat {
    pub fn declare(scene: &mut PlanBuilder, id: impl Into<String>, plan: ChatPlan) -> Result<Self> {
        Ok(Self::from(ChatActor::declare(scene, id, plan)?))
    }

    pub fn actor(&self) -> ActorHandle {
        self.inner.borrow().actor().clone()
    }

    pub fn id(&self) -> String {
        self.inner.borrow().id().to_owned()
    }

    pub fn show(&self) -> impl Beat + '_ {
        action(move |scene, at| self.inner.borrow_mut().show(scene, at))
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.borrow_mut().hide(scene, at);
        })
    }

    pub fn stamp(&self, label: impl Into<String>) -> impl Beat + '_ {
        let label = label.into();
        impulse(move |_scene, _at| {
            self.inner.borrow_mut().stamp(label);
        })
    }

    pub fn typing(&self, author: impl Into<String>) -> impl Beat + '_ {
        let author = author.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .typing(scene, at, &author)
                .unwrap_or_else(|err| panic!("chat typing: {err:#}"));
        })
    }

    pub fn say(&self, author: impl Into<String>, spans: Vec<ChatSpanPlan>) -> impl Beat + '_ {
        let author = author.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .say(scene, at, &author, spans)
                .unwrap_or_else(|err| panic!("chat say: {err:#}"));
        })
    }

    pub fn say_text(&self, author: impl Into<String>, text: impl Into<String>) -> impl Beat + '_ {
        let author = author.into();
        let text = text.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .say_text(scene, at, &author, &text)
                .unwrap_or_else(|err| panic!("chat say_text: {err:#}"));
        })
    }

    pub fn stream(
        &self,
        author: impl Into<String>,
        spans: Vec<ChatSpanPlan>,
        chars_per_second: f32,
    ) -> impl Beat + '_ {
        let author = author.into();
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .stream(scene, at, &author, spans, chars_per_second)
                .unwrap_or_else(|err| panic!("chat stream: {err:#}"))
                .1
        })
    }

    pub fn react(
        &self,
        message: impl Into<String>,
        label: impl Into<String>,
        count: u32,
    ) -> impl Beat + '_ {
        let message = message.into();
        let label = label.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .react(scene, at, &message, &label, count)
                .unwrap_or_else(|err| panic!("chat react: {err:#}"));
        })
    }

    pub fn highlight(&self, message: impl Into<String>, for_seconds: f32) -> impl Beat + '_ {
        let message = message.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .highlight(scene, &message, at, for_seconds)
                .unwrap_or_else(|err| panic!("chat highlight: {err:#}"));
        })
    }
}

/// Value handle for a [`ChangedFilesActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct ChangedFiles {
    inner: Rc<RefCell<ChangedFilesActor>>,
}

impl From<ChangedFilesActor> for ChangedFiles {
    fn from(inner: ChangedFilesActor) -> Self {
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }
}

impl ChangedFilesActor {
    /// Return a value [`ChangedFiles`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> ChangedFiles {
        ChangedFiles::from(self.clone())
    }
}

impl ChangedFiles {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: ChangedFilesPlan,
    ) -> Result<Self> {
        Ok(Self::from(ChangedFilesActor::declare(scene, id, plan)?))
    }

    pub fn actor(&self) -> ActorHandle {
        self.inner.borrow().actor().clone()
    }

    pub fn id(&self) -> String {
        self.inner.borrow().id().to_owned()
    }

    pub fn show(&self) -> impl Beat + '_ {
        action(move |scene, at| self.inner.borrow_mut().show(scene, at))
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.borrow_mut().hide(scene, at);
        })
    }

    pub fn reveal_row(&self, id: impl Into<String>) -> impl Beat + '_ {
        let id = id.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .reveal_row(scene, &id, at)
                .unwrap_or_else(|err| panic!("changed_files reveal_row: {err:#}"));
        })
    }

    pub fn reveal(&self, stagger_seconds: f32) -> impl Beat + '_ {
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .reveal(scene, at, stagger_seconds)
                .unwrap_or_else(|err| panic!("changed_files reveal: {err:#}"))
        })
    }

    pub fn highlight(&self, id: impl Into<String>, for_seconds: f32) -> impl Beat + '_ {
        let id = id.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .highlight(scene, &id, at, for_seconds)
                .unwrap_or_else(|err| panic!("changed_files highlight: {err:#}"));
        })
    }

    pub fn focus(&self, id: impl Into<String>) -> impl Beat + '_ {
        let id = id.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .focus(scene, &id, at)
                .unwrap_or_else(|err| panic!("changed_files focus: {err:#}"));
        })
    }

    pub fn unfocus(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.borrow_mut().unfocus(scene, at);
        })
    }
}

/// Value handle for a [`LowerThirdActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct LowerThird {
    inner: LowerThirdActor,
}

impl From<LowerThirdActor> for LowerThird {
    fn from(inner: LowerThirdActor) -> Self {
        Self { inner }
    }
}

impl LowerThirdActor {
    /// Return a value [`LowerThird`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> LowerThird {
        LowerThird {
            inner: self.clone(),
        }
    }
}

impl LowerThird {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &LowerThirdPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: LowerThirdActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self) -> impl Beat + '_ {
        action(move |scene, at| self.inner.clone().show(scene, at))
    }

    pub fn hide(&self) -> impl Beat + '_ {
        action(move |scene, at| self.inner.clone().hide(scene, at))
    }
}

/// Value handle for a [`ChecklistActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Checklist {
    inner: Rc<RefCell<ChecklistActor>>,
}

impl From<ChecklistActor> for Checklist {
    fn from(inner: ChecklistActor) -> Self {
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }
}

impl ChecklistActor {
    /// Return a value [`Checklist`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Checklist {
        Checklist::from(self.clone())
    }
}

impl Checklist {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &ChecklistPlan,
    ) -> Result<Self> {
        Ok(Self::from(ChecklistActor::declare(scene, id, plan)?))
    }

    pub fn actor(&self) -> ActorHandle {
        self.inner.borrow().actor().clone()
    }

    pub fn id(&self) -> String {
        self.inner.borrow().id().to_owned()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.borrow_mut().show(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.borrow_mut().hide(scene, at);
        })
    }

    pub fn reveal(&self) -> impl Beat + '_ {
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .reveal(scene, at)
                .unwrap_or_else(|err| panic!("checklist reveal: {err:#}"))
        })
    }

    pub fn reveal_item(&self, item: impl Into<String>) -> impl Beat + '_ {
        let item = item.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .reveal_item(scene, &item, at)
                .unwrap_or_else(|err| panic!("checklist reveal_item: {err:#}"));
        })
    }

    pub fn start(&self, item: impl Into<String>) -> impl Beat + '_ {
        let item = item.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .start(scene, &item, at)
                .unwrap_or_else(|err| panic!("checklist start: {err:#}"));
        })
    }

    pub fn resolve(&self, item: impl Into<String>, outcome: impl Into<Outcome>) -> impl Beat + '_ {
        let item = item.into();
        let outcome = outcome.into();
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .resolve(scene, &item, at, outcome)
                .unwrap_or_else(|err| panic!("checklist resolve: {err:#}"))
        })
    }
}

/// Value handle for a [`MeterActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Meter {
    inner: MeterActor,
}

impl From<MeterActor> for Meter {
    fn from(inner: MeterActor) -> Self {
        Self { inner }
    }
}

impl MeterActor {
    /// Return a value [`Meter`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Meter {
        Meter {
            inner: self.clone(),
        }
    }
}

impl Meter {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &MeterPlan,
        initial: f32,
    ) -> Result<Self> {
        Ok(Self {
            inner: MeterActor::declare(scene, id, plan, initial)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().show(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }

    pub fn set(&self, value: f32) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().set(scene, at, value);
        })
    }

    pub fn sweep(&self, value: f32, seconds: f32) -> impl Beat + '_ {
        action(move |scene, at| self.inner.clone().sweep(scene, at, value, seconds))
    }
}

/// Value handle for a [`BarsActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Bars {
    inner: Rc<RefCell<BarsActor>>,
}

impl From<BarsActor> for Bars {
    fn from(inner: BarsActor) -> Self {
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }
}

impl BarsActor {
    /// Return a value [`Bars`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Bars {
        Bars::from(self.clone())
    }
}

impl Bars {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &BarsPlan,
    ) -> Result<Self> {
        Ok(Self::from(BarsActor::declare(scene, id, plan)?))
    }

    pub fn actor(&self) -> ActorHandle {
        self.inner.borrow().actor().clone()
    }

    pub fn id(&self) -> String {
        self.inner.borrow().id().to_owned()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.borrow_mut().show(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.borrow_mut().hide(scene, at);
        })
    }

    pub fn reveal_rows(&self) -> impl Beat + '_ {
        action(move |scene, at| self.inner.borrow_mut().reveal_rows(scene, at))
    }

    pub fn set(
        &self,
        row: impl Into<String>,
        series: impl Into<String>,
        value: f32,
    ) -> impl Beat + '_ {
        let row = row.into();
        let series = series.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .set(scene, &row, &series, at, value)
                .unwrap_or_else(|err| panic!("bars set: {err:#}"));
        })
    }

    pub fn grow<'a>(
        &'a self,
        series: impl Into<String>,
        values: &'a [(&'a str, f32)],
    ) -> impl Beat + 'a {
        let series = series.into();
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .grow(scene, at, &series, values)
                .unwrap_or_else(|err| panic!("bars grow: {err:#}"))
        })
    }

    pub fn sort(&self, series: impl Into<String>, order: SortOrder) -> impl Beat + '_ {
        let series = series.into();
        impulse(move |scene, at| {
            self.inner
                .borrow_mut()
                .sort(scene, at, &series, order)
                .unwrap_or_else(|err| panic!("bars sort: {err:#}"));
        })
    }

    pub fn reveal_deltas(&self) -> impl Beat + '_ {
        action(move |scene, at| {
            self.inner
                .borrow_mut()
                .reveal_deltas(scene, at)
                .unwrap_or_else(|err| panic!("bars reveal_deltas: {err:#}"))
        })
    }
}

/// Value handle for a [`SubtitlesActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Subtitles {
    inner: SubtitlesActor,
}

impl From<SubtitlesActor> for Subtitles {
    fn from(inner: SubtitlesActor) -> Self {
        Self { inner }
    }
}

impl SubtitlesActor {
    /// Return a value [`Subtitles`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Subtitles {
        Subtitles {
            inner: self.clone(),
        }
    }
}

impl Subtitles {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &SubtitlesPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: SubtitlesActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().show(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }
}

/// Value handle for a [`ConfettiActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Confetti {
    inner: ConfettiActor,
}

impl From<ConfettiActor> for Confetti {
    fn from(inner: ConfettiActor) -> Self {
        Self { inner }
    }
}

impl ConfettiActor {
    /// Return a value [`Confetti`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Confetti {
        Confetti {
            inner: self.clone(),
        }
    }
}

impl Confetti {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &ConfettiPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: ConfettiActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn burst(&self) -> impl Beat + '_ {
        action(move |scene, at| self.inner.clone().burst(scene, at))
    }
}

/// Value handle for a [`TextActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Text {
    inner: TextActor,
}

impl From<TextActor> for Text {
    fn from(inner: TextActor) -> Self {
        Self { inner }
    }
}

impl TextActor {
    /// Return a value [`Text`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Text {
        Text {
            inner: self.clone(),
        }
    }
}

impl Text {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &TextPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: TextActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().show(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }

    pub fn swap<'a>(&'a self, next: &'a Self) -> impl Beat + 'a {
        impulse(move |scene, at| {
            let mut n = next.inner.clone();
            self.inner.clone().swap(scene, &mut n, at);
        })
    }

    pub fn move_to(&self, anchor: impl Into<String>) -> impl Beat + '_ {
        let anchor = anchor.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .move_to(scene, &anchor, at)
                .unwrap_or_else(|err| panic!("text '{}': {err:#}", self.inner.id()));
        })
    }
}

/// Value handle for an [`ImageActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Image {
    inner: ImageActor,
}

impl From<ImageActor> for Image {
    fn from(inner: ImageActor) -> Self {
        Self { inner }
    }
}

impl ImageActor {
    /// Return a value [`Image`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Image {
        Image {
            inner: self.clone(),
        }
    }
}

impl Image {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &ImagePlan,
        media: MediaPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: ImageActor::declare(scene, id, plan, media)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn fly_in(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().fly_in(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }

    pub fn move_to(&self, anchor: impl Into<String>) -> impl Beat + '_ {
        let anchor = anchor.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .move_to(scene, &anchor, at)
                .unwrap_or_else(|err| panic!("image '{}': {err:#}", self.inner.id()));
        })
    }
}

/// Value handle for a [`LensActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Lens {
    inner: LensActor,
}

impl From<LensActor> for Lens {
    fn from(inner: LensActor) -> Self {
        Self { inner }
    }
}

impl LensActor {
    /// Return a value [`Lens`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Lens {
        Lens {
            inner: self.clone(),
        }
    }
}

impl Lens {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &LensPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: LensActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().show(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().hide(scene, at);
        })
    }

    pub fn move_to(&self, anchor: impl Into<String>) -> impl Beat + '_ {
        let anchor = anchor.into();
        impulse(move |scene, at| {
            self.inner
                .clone()
                .move_to(scene, &anchor, at)
                .unwrap_or_else(|err| panic!("lens '{}': {err:#}", self.inner.id()));
        })
    }

    pub fn slide(&self, offset: [f32; 2]) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().slide(scene, offset, at);
        })
    }

    pub fn magnify(&self, magnification: f32) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().magnify(scene, magnification, at);
        })
    }

    pub fn resize(&self, size: [f32; 2]) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().resize(scene, size, at);
        })
    }

    pub fn focus(&self, offset: [f32; 2]) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clone().focus(scene, offset, at);
        })
    }
}

/// Value handle for a [`DiagnosticActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    inner: DiagnosticActor,
}

impl From<DiagnosticActor> for Diagnostic {
    fn from(inner: DiagnosticActor) -> Self {
        Self { inner }
    }
}

impl DiagnosticActor {
    /// Return a value [`Diagnostic`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Diagnostic {
        Diagnostic {
            inner: self.clone(),
        }
    }
}

impl Diagnostic {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &DiagnosticPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: DiagnosticActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self) -> impl Beat + '_ {
        action(move |scene, at| self.inner.show(scene, at))
    }

    pub fn clear(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.clear(scene, at);
        })
    }
}

/// Value handle for a [`HoverActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Hover {
    inner: HoverActor,
}

impl From<HoverActor> for Hover {
    fn from(inner: HoverActor) -> Self {
        Self { inner }
    }
}

impl HoverActor {
    /// Return a value [`Hover`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Hover {
        Hover {
            inner: self.clone(),
        }
    }
}

impl Hover {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &HoverPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: HoverActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.show(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.hide(scene, at);
        })
    }
}

/// Value handle for a [`CursorActor`] whose choreography methods return [`Beat`]s.
#[derive(Clone, Debug)]
pub struct Cursor {
    inner: CursorActor,
}

impl From<CursorActor> for Cursor {
    fn from(inner: CursorActor) -> Self {
        Self { inner }
    }
}

impl CursorActor {
    /// Return a value [`Cursor`] handle whose methods produce composable [`Beat`]s.
    pub fn beats(&self) -> Cursor {
        Cursor {
            inner: self.clone(),
        }
    }
}

impl Cursor {
    pub fn declare(
        scene: &mut PlanBuilder,
        id: impl Into<String>,
        plan: &CursorPlan,
    ) -> Result<Self> {
        Ok(Self {
            inner: CursorActor::declare(scene, id, plan)?,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        self.inner.actor()
    }

    pub fn id(&self) -> &str {
        self.inner.id()
    }

    pub fn show(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.show(scene, at);
        })
    }

    pub fn hide(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.inner.hide(scene, at);
        })
    }

    pub fn move_to(&self, anchor: impl Into<String>, head: f32) -> impl Beat + '_ {
        let anchor = anchor.into();
        impulse(move |scene, at| {
            self.inner
                .move_to(scene, &anchor, head, at)
                .unwrap_or_else(|err| panic!("cursor '{}': {err:#}", self.inner.id()));
        })
    }

    pub fn select(&self, anchor: impl Into<String>, seconds: f32) -> impl Beat + '_ {
        let anchor = anchor.into();
        action(move |scene, at| {
            self.inner
                .select(scene, &anchor, at, seconds)
                .unwrap_or_else(|err| panic!("cursor '{}': {err:#}", self.inner.id()))
        })
    }
}

impl InlayHint {
    pub fn show_beat(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.show(scene, at);
        })
    }

    pub fn hide_beat(&self) -> impl Beat + '_ {
        impulse(move |scene, at| {
            self.hide(scene, at);
        })
    }
}

/// Low-level channel [`Beat`] helpers for custom properties on any actor.
pub fn spring_channel(
    channel: &ContinuousHandle,
    target: f32,
    visual_duration: f32,
    bounce: f32,
) -> impl Beat + '_ {
    impulse(move |scene, at| {
        scene.spring(channel, at, target, visual_duration, bounce);
    })
}

/// A suspicious same-timestamp multi-write on a continuous channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteConflict {
    pub channel_id: String,
    pub at_nanos: u64,
    pub count: usize,
}

/// Inspect a [`ScenePlan`] for accidental same-timestamp write collisions on any
/// continuous channel.
///
/// The single intentional same-timestamp compound idiom in Psychopomp is a
/// single `Set` immediately followed by a single `Spring` or `Ease` at the same
/// `at_nanos` (as in `settle_in`, `hit`, `clock_for`). Any other same-timestamp
/// group (such as two competing `Spring` writes from parallel branches, or a
/// `Set` after a `Spring` at the same timestamp) is returned as a [`WriteConflict`].
pub fn find_write_conflicts(plan: &ScenePlan) -> Vec<WriteConflict> {
    let mut conflicts = Vec::new();
    for channel in &plan.continuous_channels {
        let mut i = 0;
        while i < channel.events.len() {
            let at = channel.events[i].at_nanos();
            let mut j = i + 1;
            while j < channel.events.len() && channel.events[j].at_nanos() == at {
                j += 1;
            }
            let group = &channel.events[i..j];
            if group.len() > 1 {
                let valid_set_then_move = group.len() == 2
                    && matches!(group[0], TrackEventPlan::Set { .. })
                    && matches!(
                        group[1],
                        TrackEventPlan::Spring { .. } | TrackEventPlan::Ease { .. }
                    );
                if !valid_set_then_move {
                    conflicts.push(WriteConflict {
                        channel_id: channel.id.clone(),
                        at_nanos: at,
                        count: group.len(),
                    });
                }
            }
            i = j;
        }
    }
    conflicts
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{
        author::SECOND,
        callout::{CalloutAnchorPlan, CalloutSide},
        caption::CaptionAlign,
        plan::compile_channels,
        sfx,
        subtitles::SubtitleWordPlan,
        timeline::{PropertyId, Timeline},
    };

    fn run_test_beat(beat: impl Beat, at: u64) -> (Span, serde_json::Value, Timeline) {
        let mut scene = PlanBuilder::new("law-test", 20 * SECOND);
        let actor = scene
            .actor("a", "title-card", json!({"title": "T"}))
            .unwrap();
        scene.continuous(&actor, "x", 0.0);
        scene.continuous(&actor, "y", 0.0);
        scene.continuous(&actor, "z", 0.0);
        let span = scene.at(at, beat);
        let plan = scene.finish().unwrap();
        let timeline = compile_channels(
            plan.continuous_channels
                .iter()
                .map(|c| (c, PropertyId::new(&c.id))),
            plan.duration_nanos,
            |s| match s {
                crate::plan::ScalarPlan::Literal(v) => Ok(*v),
                _ => unreachable!(),
            },
        )
        .unwrap();
        let json = serde_json::to_value(&plan).unwrap();
        (span, json, timeline)
    }

    fn handle(prop: &str) -> ContinuousHandle {
        let mut dummy = PlanBuilder::new("d", SECOND);
        let a = dummy
            .actor("a", "title-card", json!({"title": "T"}))
            .unwrap();
        dummy.continuous(&a, prop, 0.0)
    }

    fn spring_x(target: f32, dur: u64) -> impl Beat + Clone {
        let x = handle("x");
        move |scene: &mut PlanBuilder, at: u64| {
            scene.spring(&x, at, target, 0.4, 0.1);
            Span::new(at, at + dur)
        }
    }

    fn spring_y(target: f32, dur: u64) -> impl Beat + Clone {
        let y = handle("y");
        move |scene: &mut PlanBuilder, at: u64| {
            scene.spring(&y, at, target, 0.5, 0.0);
            Span::new(at, at + dur)
        }
    }

    fn ease_z(target: f32, seconds: f32) -> impl Beat + Clone {
        let z = handle("z");
        let dur = whole_millis(seconds);
        move |scene: &mut PlanBuilder, at: u64| {
            scene.ease(&z, at, target, seconds, Ease::Smootherstep);
            Span::new(at, at + dur)
        }
    }

    fn impulse_x(target: f32) -> impl Beat + Clone {
        let x = handle("x");
        move |scene: &mut PlanBuilder, at: u64| {
            scene.spring(&x, at, target, 0.35, 0.0);
            Span::impulse(at)
        }
    }

    #[test]
    fn law_1_sequence_is_associative_and_has_empty_identity() {
        let a = spring_x(10.0, millis(300));
        let b = spring_y(25.0, millis(450));
        let c = ease_z(1.0, 0.6);

        let (span_l, plan_l, _) =
            run_test_beat(a.clone().then(b.clone()).then(c.clone()), millis(100));
        let (span_r, plan_r, _) = run_test_beat(a.clone().then(b.then(c)), millis(100));
        assert_eq!(span_l, span_r);
        assert_eq!(plan_l, plan_r);

        let (span_id_l, plan_id_l, _) = run_test_beat(empty().then(a.clone()), millis(200));
        let (span_id_r, plan_id_r, _) = run_test_beat(a.clone().then(empty()), millis(200));
        let (span_base, plan_base, _) = run_test_beat(a, millis(200));
        assert_eq!(span_id_l, span_base);
        assert_eq!(span_id_r, span_base);
        assert_eq!(plan_id_l, plan_base);
        assert_eq!(plan_id_r, plan_base);
    }

    #[test]
    fn law_2_hold_fusion_and_delay_isomorphism_are_exact_in_nanoseconds() {
        let a = spring_x(50.0, millis(400));
        let d1 = 100_000_000;
        let d2 = 200_000_000;

        let (s1, p1, _) = run_test_beat(hold(d1).then(hold(d2)).then(a.clone()), 0);
        let (s2, p2, _) = run_test_beat(hold(d1 + d2).then(a.clone()), 0);
        let (s3, p3, _) = run_test_beat(a.clone().after(d1).after(d2), 0);
        let (s4, p4, _) = run_test_beat(a.after(d1 + d2), 0);

        assert_eq!(s1, s2);
        assert_eq!(s2, s3);
        assert_eq!(s3, s4);
        assert_eq!(p1, p2);
        assert_eq!(p2, p3);
        assert_eq!(p3, p4);
    }

    #[test]
    fn law_3_delay_shifts_into_sequence_head() {
        let a = spring_x(12.0, millis(250));
        let b = spring_y(-8.0, millis(600));
        let d = millis(350);

        let (s1, p1, _) = run_test_beat(a.clone().then(b.clone()).after(d), millis(50));
        let (s2, p2, _) = run_test_beat(a.after(d).then(b), millis(50));
        assert_eq!(s1, s2);
        assert_eq!(p1, p2);
    }

    #[test]
    fn law_4_parallel_is_associative_and_distributes_delay() {
        let a = spring_x(10.0, millis(300));
        let b = spring_y(20.0, millis(700));
        let c = ease_z(0.5, 0.4);
        let d = millis(180);

        let (s_l, p_l, _) = run_test_beat(a.clone().also(b.clone()).also(c.clone()), millis(50));
        let (s_r, p_r, _) = run_test_beat(a.clone().also(b.clone().also(c)), millis(50));
        assert_eq!(s_l, s_r);
        assert_eq!(p_l, p_r);

        let (sd_1, pd_1, _) = run_test_beat(a.clone().also(b.clone()).after(d), millis(100));
        let (sd_2, pd_2, _) = run_test_beat(a.after(d).also(b.after(d)), millis(100));
        assert_eq!(sd_1, sd_2);
        assert_eq!(pd_1, pd_2);
    }

    #[test]
    fn law_5_impulse_absorption_and_accompaniment_laws() {
        let p = impulse_x(42.0);
        let a = spring_y(100.0, millis(500));

        let (s_seq, plan_seq, _) = run_test_beat(p.clone().then(a.clone()), millis(120));
        let (s_par, plan_par, _) = run_test_beat(p.clone().also(a.clone()), millis(120));
        assert_eq!(s_seq, s_par);
        assert_eq!(plan_seq, plan_par);

        let long_bg = ease_z(1.0, 2.0);
        let (s_with, _, _) = run_test_beat(a.clone().with(long_bg), millis(100));
        assert_eq!(s_with, Span::new(millis(100), millis(600)));

        let (s_end, plan_end, _) = run_test_beat(a.clone().on_end(p.clone()), millis(100));
        let (s_then, plan_then, _) = run_test_beat(a.then(p), millis(100));
        assert_eq!(s_end, s_then);
        assert_eq!(plan_end, plan_then);
    }

    #[test]
    fn law_6_stagger_equals_delayed_parallel_expansion() {
        let gap = millis(120);
        let targets = [10.0_f32, 30.0, 60.0, 90.0];

        let (s_stag, p_stag, _) =
            run_test_beat(stagger(gap, targets, |t| spring_x(t, millis(200))), SECOND);
        let (s_manual, p_manual, _) = run_test_beat(
            all![
                spring_x(10.0, millis(200)).after(0),
                spring_x(30.0, millis(200)).after(gap),
                spring_x(60.0, millis(200)).after(2 * gap),
                spring_x(90.0, millis(200)).after(3 * gap),
            ],
            SECOND,
        );
        assert_eq!(s_stag, s_manual);
        assert_eq!(p_stag, p_manual);
    }

    #[test]
    fn law_7_retargeted_parallel_springs_commute_across_distinct_timestamps() {
        let w0 = impulse_x(100.0);
        let w1 = impulse_x(-40.0).after(millis(180));
        let w2 = impulse_x(25.0).after(millis(420));

        let (_, _, tl_forward) =
            run_test_beat(all![w0.clone(), w1.clone(), w2.clone()], millis(100));
        let (_, _, tl_reverse) = run_test_beat(all![w2, w0, w1], millis(100));

        let prop = PropertyId::new("a.x");
        for step in 0..=200 {
            let t = step as f64 * 0.01;
            let s_f = tl_forward.sample_at(&prop, t).unwrap();
            let s_r = tl_reverse.sample_at(&prop, t).unwrap();
            assert_eq!(
                [s_f.position.to_bits(), s_f.velocity.to_bits()],
                [s_r.position.to_bits(), s_r.velocity.to_bits()],
                "mismatch at t={t:.2}s"
            );
        }
        let at_retarget = tl_forward.sample_at(&prop, 0.28).unwrap();
        assert!(at_retarget.velocity.abs() > 10.0);
    }

    #[test]
    fn cross_actor_beat_composes_stage_caption_rolling_number_callout_and_sound() {
        let stage_plan: StagePlan = serde_json::from_value(json!({
            "elements": [
                { "kind": "card", "id": "client", "at": [560, 540, 0], "size": [300, 110], "title": "client" },
                { "kind": "orb", "id": "server", "at": [1360, 540, 0], "radius": 140 },
                { "kind": "beam", "id": "link", "from": "client", "to": "server" },
                { "kind": "packet", "id": "hello", "beam": "link", "label": "GET /hello" }
            ]
        }))
        .unwrap();

        let mut scene = PlanBuilder::new("cross-actor", 6 * SECOND);
        let stage = Stage::declare(&mut scene, "stage", &stage_plan).unwrap();
        let version = RollingNumber::declare(
            &mut scene,
            "version",
            RollingNumberPlan::new([960.0, 160.0], 56.0, "rc.112")
                .aligned(CaptionAlign::Center)
                .tone(Tone::Accent),
        )
        .unwrap();
        let note = Callout::declare(
            &mut scene,
            "note",
            &CalloutPlan::new(
                CalloutAnchorPlan::Stage {
                    id: "client".into(),
                    element: "client".into(),
                    edge: CalloutSide::Top,
                    side: None,
                },
                vec![CaptionSpanPlan::new("upgraded", Tone::Success)],
            )
            .anchor(CalloutAnchorPlan::Stage {
                id: "server".into(),
                element: "server".into(),
                edge: CalloutSide::Top,
                side: Some(CalloutSide::TopLeft),
            }),
        )
        .unwrap();
        let footer = Caption::declare(
            &mut scene,
            "footer",
            &CaptionPlan::line(
                [140.0, 1004.0],
                28.0,
                vec![CaptionSpanPlan::new(
                    "all actors share one score",
                    Tone::Plain,
                )],
            ),
        )
        .unwrap();

        // One single beat tree orchestrating Stage, RollingNumber, Callout, Caption, and SFX:
        let span = scene.at(
            0,
            stage
                .settle_in("client")
                .with(version.show())
                .with(note.show())
                .then(stage.connect("link", 0.6))
                .then(stage.send("hello", 0.8).with(sound(
                    "send",
                    "sfx/send.wav",
                    millis(150),
                    -10.0,
                )))
                .then(all![
                    stage.land("server"),
                    stage.jolt([1.0, 0.0], 0.6),
                    version.roll("rc.117"),
                    note.move_to("server").then(note.emphasize()),
                    footer.type_in(50.0, 0.6),
                ]),
        );

        assert!(span.end > 2 * SECOND);
        let plan = scene.finish().unwrap();
        assert_eq!(plan.actors.len(), 4);
        assert_eq!(plan.media.len(), 1);
        assert_eq!(plan.media[0].timeline_start_nanos, millis(1400));
        let rolled: RollingNumberPlan =
            serde_json::from_value(plan.actors[1].data.clone()).unwrap();
        assert_eq!(rolled.rolls.len(), 1);
        assert_eq!(rolled.rolls[0].value, "rc.117");
        assert_eq!(rolled.rolls[0].at_nanos, millis(2200));
    }

    #[test]
    fn cross_actor_camera_stage_terminal_subtitles_and_sfx_beat() {
        let stage_plan: StagePlan = serde_json::from_value(json!({
            "elements": [
                { "kind": "card", "id": "client", "at": [560, 540, 0], "size": [300, 110], "title": "client" },
                { "kind": "orb", "id": "server", "at": [1360, 540, 0], "radius": 140 },
                { "kind": "beam", "id": "link", "from": "client", "to": "server" },
                { "kind": "packet", "id": "hello", "beam": "link", "label": "GET /hello" }
            ]
        }))
        .unwrap();

        let mut scene = PlanBuilder::new("camera-terminal-subtitles", 8 * SECOND);
        let stage = Stage::declare(&mut scene, "stage", &stage_plan).unwrap();
        let cam = stage.camera();
        let term = Terminal::declare(
            &mut scene,
            "term",
            TerminalPlan::new([120.0, 680.0], 720.0, 6).titled("zsh"),
        )
        .unwrap();
        let mut sub_plan = SubtitlesPlan::new([960.0, 980.0], 900.0);
        sub_plan.words.push(SubtitleWordPlan {
            text: "Connected.".into(),
            start_nanos: millis(400),
            end_nanos: millis(1200),
        });
        let subs = Subtitles::declare(&mut scene, "subs", &sub_plan).unwrap();

        let span = scene.at(
            0,
            all![
                cam.establish(160.0, 1.4),
                stage
                    .settle_in("client")
                    .with(term.show())
                    .then(stage.connect("link", 0.6))
                    .on_end(sfx::TICK.beat("wired", -18.0))
                    .then(
                        stage
                            .send("hello", 0.8)
                            .with(cam.follow("hello", Move::Spring(0.5)))
                            .with(term.type_command("curl -s http://server/hello")),
                    )
                    .then(all![
                        stage.land("server"),
                        cam.release(Move::Spring(0.8)),
                        term.print_text("200 OK", Tone::Success),
                        sfx::CONFIRM.beat("ok", -12.0),
                    ])
                    .then_after(SECOND, subs.hide()),
            ],
        );

        assert!(span.end > 3 * SECOND);
        let plan = scene.finish().unwrap();
        assert_eq!(plan.actors.len(), 3);
        assert_eq!(plan.media.len(), 2);
    }

    #[derive(Clone, Debug)]
    enum Expr {
        Empty,
        Hold(u64),
        Leaf { channel: u8, target: f32, span: u64 },
        After(u64, Box<Expr>),
        Seq(Box<Expr>, Box<Expr>),
        Par(Box<Expr>, Box<Expr>),
    }

    impl Expr {
        fn generate(seed: &mut u64, depth: u32) -> Self {
            let mut next = || {
                *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                *seed
            };
            if depth == 0 {
                return match next() % 3 {
                    0 => Self::Empty,
                    1 => Self::Hold((next() % 5 + 1) * millis(40)),
                    _ => Self::Leaf {
                        channel: (next() % 3) as u8,
                        target: ((next() % 200) as f32) - 50.0,
                        span: (next() % 4) * millis(50),
                    },
                };
            }
            match next() % 5 {
                0 => Self::Empty,
                1 => Self::Hold((next() % 5 + 1) * millis(30)),
                2 => Self::After(
                    (next() % 4) * millis(45),
                    Box::new(Self::generate(seed, depth - 1)),
                ),
                3 => Self::Seq(
                    Box::new(Self::generate(seed, depth - 1)),
                    Box::new(Self::generate(seed, depth - 1)),
                ),
                _ => Self::Par(
                    Box::new(Self::generate(seed, depth - 1)),
                    Box::new(Self::generate(seed, depth - 1)),
                ),
            }
        }

        fn rewrite(&self) -> Self {
            match self {
                Self::Empty | Self::Hold(_) | Self::Leaf { .. } => self.clone(),
                Self::After(0, inner) => inner.rewrite(),
                Self::After(d1, inner) => match inner.as_ref() {
                    Self::After(d2, sub) => Self::After(d1 + d2, Box::new(sub.rewrite())),
                    Self::Par(a, b) => Self::Par(
                        Box::new(Self::After(*d1, a.clone()).rewrite()),
                        Box::new(Self::After(*d1, b.clone()).rewrite()),
                    ),
                    Self::Seq(a, b) => Self::Seq(
                        Box::new(Self::After(*d1, a.clone()).rewrite()),
                        Box::new(b.rewrite()),
                    ),
                    _ => Self::Seq(Box::new(Self::Hold(*d1)), Box::new(inner.rewrite())),
                },
                Self::Seq(a, b) => {
                    let a = a.rewrite();
                    let b = b.rewrite();
                    match (a, b) {
                        (Self::Empty, b) => b,
                        (a, Self::Empty) => a,
                        (Self::Hold(d1), Self::Hold(d2)) => Self::Hold(d1 + d2),
                        (Self::Seq(x, y), z) => {
                            Self::Seq(x, Box::new(Self::Seq(y, Box::new(z)).rewrite()))
                        }
                        (a, b) => Self::Seq(Box::new(a), Box::new(b)),
                    }
                }
                Self::Par(a, b) => {
                    let a = a.rewrite();
                    let b = b.rewrite();
                    match (a, b) {
                        (Self::Empty, b) => b,
                        (a, Self::Empty) => a,
                        (Self::Par(x, y), z) => {
                            Self::Par(x, Box::new(Self::Par(y, Box::new(z)).rewrite()))
                        }
                        (a, b) => Self::Par(Box::new(a), Box::new(b)),
                    }
                }
            }
        }
    }

    impl Beat for &Expr {
        fn play(self, scene: &mut PlanBuilder, at: u64) -> Span {
            match self {
                Expr::Empty => empty().play(scene, at),
                Expr::Hold(d) => hold(*d).play(scene, at),
                Expr::Leaf {
                    channel,
                    target,
                    span,
                } => {
                    let prop = match channel {
                        0 => "x",
                        1 => "y",
                        _ => "z",
                    };
                    let ch = handle(prop);
                    scene.spring(&ch, at, *target, 0.35, 0.0);
                    Span::new(at, at + *span)
                }
                Expr::After(d, inner) => inner.as_ref().after(*d).play(scene, at),
                Expr::Seq(a, b) => a.as_ref().then(b.as_ref()).play(scene, at),
                Expr::Par(a, b) => a.as_ref().also(b.as_ref()).play(scene, at),
            }
        }
    }

    #[test]
    fn property_200_random_beat_trees_preserve_span_events_and_sampled_trajectories_under_law_rewrites()
     {
        let mut seed = 0x5EED_CAFE_BAAD_F00Du64;
        for case in 0..200 {
            let tree = Expr::generate(&mut seed, 4);
            let rewritten = tree.rewrite();
            let start = (case as u64 % 5) * millis(50);

            let (span_1, plan_1, tl_1) = run_test_beat(&tree, start);
            let (span_2, plan_2, tl_2) = run_test_beat(&rewritten, start);

            assert_eq!(span_1, span_2, "span mismatch on random tree #{case}");
            assert_eq!(
                plan_1["continuousChannels"], plan_2["continuousChannels"],
                "compiled channels mismatch on random tree #{case}"
            );

            for prop in ["a.x", "a.y", "a.z"] {
                let id = PropertyId::new(prop);
                for sample in [0.0, 0.15, 0.33, 0.75, 1.25] {
                    assert_eq!(
                        tl_1.sample_at(&id, sample),
                        tl_2.sample_at(&id, sample),
                        "sampled trajectory mismatch on tree #{case} at {sample}"
                    );
                }
            }
        }
    }

    #[test]
    fn conflict_detector_allows_set_then_spring_and_flags_competing_parallel_springs() {
        let mut scene = PlanBuilder::new("clean", 2 * SECOND);
        let a = scene
            .actor("a", "title-card", json!({"title": "T"}))
            .unwrap();
        let x = scene.continuous(&a, "x", 0.0);
        scene.set(&x, SECOND, 16.0);
        scene.spring(&x, SECOND, 0.0, 0.55, 0.16);
        let clean = scene.finish().unwrap();
        assert!(find_write_conflicts(&clean).is_empty());

        let (_, dirty_json, _) = run_test_beat(spring_y(10.0, 0).also(spring_y(20.0, 0)), SECOND);
        let dirty: ScenePlan = serde_json::from_value(dirty_json).unwrap();
        assert_eq!(
            find_write_conflicts(&dirty),
            vec![WriteConflict {
                channel_id: "a.y".into(),
                at_nanos: SECOND,
                count: 2,
            }]
        );
    }
}
