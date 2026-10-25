use serde::Serialize;

use crate::math::easing::Ease;
use crate::plan::{
    ActorPlan, ContinuousChannelPlan, CuePlan, MediaPlan, PresentationStepPlan, ScalarPlan,
    ScenePlan, SemanticTargetPlan, SpringPlan, StateChannelPlan, StateEventPlan,
    TargetComponentPlan, TargetScalarPlan, TrackEventPlan,
};

pub struct PlanBuilder {
    plan: ScenePlan,
}

/// One second on the plan clock, in nanoseconds.
pub const SECOND: u64 = 1_000_000_000;

/// One millisecond on the plan clock, in nanoseconds.
pub const MILLISECOND: u64 = 1_000_000;

/// `seconds` on the plan clock, rounded to the nearest nanosecond.
pub fn seconds(seconds: f64) -> u64 {
    (seconds * 1e9).round() as u64
}

/// `millis` whole milliseconds on the plan clock.
pub const fn millis(millis: u64) -> u64 {
    millis * MILLISECOND
}

/// Canonical `120 ms` stagger gap between rows or cards (`TECHNIQUES.md`).
pub const STAGGER: u64 = millis(120);

/// Canonical `65 ms` lead between a rigid panel's body and its content.
pub const CONTENT_LAG: u64 = millis(65);

/// Standard leader and accent-bar draw-on duration (`0.42 s`).
pub const DRAW_SECONDS: f32 = 0.42;

/// Readable clamps for plan times, such as a beat keyed to a phrase that must
/// still wait for what causes it: `f("injected").not_before(contact)`.
pub trait PlanTime {
    /// This time, or `earliest` if that is later.
    fn not_before(self, earliest: u64) -> u64;
}

impl PlanTime for u64 {
    fn not_before(self, earliest: u64) -> u64 {
        self.max(earliest)
    }
}

/// Start one beat per item, `gap` apart from `start` (rows ripple about 120 ms
/// apart). `beat` receives each item and its start, and returns when that
/// beat ends; `stagger` returns the latest end, or `start` with no items.
pub fn stagger<T>(
    items: impl IntoIterator<Item = T>,
    start: u64,
    gap: u64,
    mut beat: impl FnMut(T, u64) -> u64,
) -> u64 {
    items
        .into_iter()
        .zip(0..)
        .map(|(item, index)| beat(item, start + gap * index))
        .fold(start, u64::max)
}

/// `count` times spread evenly from `from` to `to`, both included (one time
/// is `from`), in whole nanoseconds.
pub fn spread(count: u64, from: u64, to: u64) -> impl Iterator<Item = u64> {
    let span = u128::from(to.saturating_sub(from));
    (0..count).map(move |index| {
        let step = span * u128::from(index) / u128::from(count.saturating_sub(1).max(1));
        from + step as u64
    })
}

/// Named springs. Kit's palette (the `kit-motion` skill) comes first; its
/// numbers mean what they mean in knobs and SwiftUI's
/// `.spring(duration:bounce:)` (ω = 2π / duration, ζ = 1 − bounce), so a
/// preset feels the same in a reel as in an app. Everything rests on bounce 0;
/// only [`SpringPlan::MORPH_OPEN`], [`SpringPlan::MORPH_CLOSE`] and the rare
/// [`SpringPlan::PLAYFUL`] overshoot. The older role names after them keep
/// their explainer-motion durations ([`SpringPlan::visual`]'s 1.2× response)
/// and, since this palette, no bounce.
impl SpringPlan {
    /// Zoom, scrub, anything tied to a pointer: 0.1 s.
    pub const INSTANT: Self = Self::palette(0.1, 0.0);
    /// Small controls, menus, exits: 0.25 s.
    pub const SNAPPY: Self = Self::palette(0.25, 0.0);
    /// The default for a state change: 0.3 s.
    pub const DEFAULT: Self = Self::palette(0.3, 0.0);
    /// Layout moves and list reflow: 0.4 s.
    pub const CALM: Self = Self::palette(0.4, 0.0);
    /// Drawers and panels: 0.45 s.
    pub const SMOOTH: Self = Self::palette(0.45, 0.0);
    /// A chip opening into its panel: 0.42 s, bounce 0.16.
    pub const MORPH_OPEN: Self = Self::palette(0.42, 0.16);
    /// A panel closing back into its chip: 0.38 s, bounce 0.08.
    pub const MORPH_CLOSE: Self = Self::palette(0.38, 0.08);
    /// Rare delight only: 0.3 s, bounce 0.3.
    pub const PLAYFUL: Self = Self::palette(0.3, 0.3);
    /// Reveals and wipes: 0.85 s.
    pub const SLOW: Self = Self::palette(0.85, 0.0);
    /// The quickest explainer camera move: 1.0 s. [`SpringPlan::CAMERA`] is
    /// the long end of the camera range.
    pub const CAMERA_QUICK: Self = Self::palette(1.0, 0.0);
    /// A dragged element's lead follower: ω 34, ζ 0.86.
    pub const FOLLOW_LEAD: Self = Self::angular(34.0, 0.86);
    /// What trails the lead follower: ω 25, ζ 1.
    pub const FOLLOW_TRAIL: Self = Self::angular(25.0, 1.0);

    /// A rigid panel settling into place: 0.6 s, no bounce.
    pub const PANEL: Self = Self::feel(0.6, 0.0);
    /// Ink following its panel, or an overlay fading in: 0.36 s, no bounce.
    pub const CONTENT: Self = Self::feel(0.36, 0.0);
    /// Spatial entrance and persistent layout room opening (caption/text rise,
    /// tree fold/scroll, code line motion, chat message slot): 0.45 s, no bounce.
    pub const ENTER: Self = Self::feel(0.45, 0.0);
    /// Clean exit fade in place, faster and simpler than entry: 0.24 s, no bounce.
    pub const EXIT: Self = Self::feel(0.24, 0.0);
    /// Point-to-point spatial translation or anchor weight glide: 0.6 s, no bounce.
    pub const MOVE: Self = Self::feel(0.6, 0.0);
    /// A quick state change, focus shift, or status cross-fade: 0.3 s, no bounce.
    pub const SNAP: Self = Self::feel(0.3, 0.0);
    /// A small floating UI appearing in place (hover card, reaction pill,
    /// delta chip): 0.32 s, no bounce.
    pub const POP: Self = Self::feel(0.32, 0.0);
    /// A camera move with weight and a natural tail: 1.6 s, critically damped.
    pub const CAMERA: Self = Self::feel(1.6, 0.0);
    /// A hero landing: 0.85 s, no bounce.
    pub const LIVELY: Self = Self::feel(0.85, 0.0);

    /// Override settling thresholds, as when a dimensionless weight needs
    /// tighter bounds than a pixel channel.
    pub const fn with_thresholds(self, position_threshold: f32, velocity_threshold: f32) -> Self {
        Self {
            position_threshold,
            velocity_threshold,
            ..self
        }
    }

    /// A palette spring: knobs' and SwiftUI's duration and bounce.
    const fn palette(duration: f32, bounce: f32) -> Self {
        Self {
            response_seconds: duration,
            damping_ratio: 1. - bounce,
            position_threshold: 0.001,
            velocity_threshold: 0.001,
        }
    }

    /// A spring by angular frequency (rad/s) and damping ratio.
    const fn angular(omega: f32, damping_ratio: f32) -> Self {
        Self {
            response_seconds: std::f32::consts::TAU / omega,
            damping_ratio,
            position_threshold: 0.001,
            velocity_threshold: 0.001,
        }
    }

    /// [`SpringPlan::visual`] in a constant: the same arithmetic, so a named
    /// feel and its literal duration and bounce emit identical plans.
    const fn feel(duration: f32, bounce: f32) -> Self {
        Self {
            response_seconds: duration * 1.2,
            damping_ratio: 1. - bounce,
            position_threshold: 0.001,
            velocity_threshold: 0.001,
        }
    }
}

/// Whole milliseconds in nanoseconds: an f32 duration such as 0.8 is not exact
/// in nanoseconds, so eases and helpers that chain them round alike.
pub(crate) fn whole_millis(seconds: f32) -> u64 {
    (f64::from(seconds) * 1000.0).round() as u64 * 1_000_000
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ActorHandle {
    id: String,
}

impl ActorHandle {
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ContinuousHandle {
    id: String,
}

impl ContinuousHandle {
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StateHandle {
    id: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SemanticTargetHandle {
    id: String,
}

impl SemanticTargetHandle {
    pub fn x(&self) -> ScalarPlan {
        self.component(TargetComponentPlan::X)
    }

    pub fn width(&self) -> ScalarPlan {
        self.component(TargetComponentPlan::Width)
    }

    pub fn center_x(&self) -> ScalarPlan {
        self.component(TargetComponentPlan::CenterX)
    }

    pub fn line_y(&self) -> ScalarPlan {
        self.component(TargetComponentPlan::LineY)
    }

    pub fn offset(&self, component: TargetComponentPlan, offset: f32) -> ScalarPlan {
        ScalarPlan::Target(TargetScalarPlan {
            target_id: self.id.clone(),
            component,
            offset,
        })
    }

    fn component(&self, component: TargetComponentPlan) -> ScalarPlan {
        self.offset(component, 0.0)
    }
}

impl PlanBuilder {
    pub fn new(id: impl Into<String>, duration_nanos: u64) -> Self {
        Self {
            plan: ScenePlan::new(id, duration_nanos),
        }
    }

    pub fn actor(
        &mut self,
        id: impl Into<String>,
        recipe: impl Into<String>,
        data: impl Serialize,
    ) -> Result<ActorHandle, serde_json::Error> {
        let id = id.into();
        self.plan.actors.push(ActorPlan {
            id: id.clone(),
            recipe: recipe.into(),
            data: serde_json::to_value(data)?,
        });
        Ok(ActorHandle { id })
    }

    /// Replace a declared actor's recipe data, for handles whose recipe grows
    /// as the scene is authored (such as a Rolling Number's later values).
    pub fn replace_actor_data(
        &mut self,
        actor: &ActorHandle,
        data: impl Serialize,
    ) -> Result<(), serde_json::Error> {
        let data = serde_json::to_value(data)?;
        self.plan
            .actors
            .iter_mut()
            .find(|candidate| candidate.id == actor.id)
            .expect("actor handle belongs to this plan builder")
            .data = data;
        Ok(())
    }

    /// Deserialize a declared actor's current recipe data from the builder.
    pub fn actor_data<T: serde::de::DeserializeOwned>(
        &self,
        actor: &ActorHandle,
    ) -> Result<T, serde_json::Error> {
        let data = self
            .plan
            .actors
            .iter()
            .find(|candidate| candidate.id == actor.id)
            .expect("actor handle belongs to this plan builder")
            .data
            .clone();
        serde_json::from_value(data)
    }

    /// Latest literal target written to `actor.property` (or its initial value
    /// if no events have been written yet). Returns `None` if the channel has
    /// not been declared.
    pub fn latest_literal(&self, actor: &ActorHandle, property: &str) -> Option<f32> {
        let id = format!("{}.{}", actor.id, property);
        let channel = self
            .plan
            .continuous_channels
            .iter()
            .find(|candidate| candidate.id == id)?;
        let scalar = match channel.events.last() {
            Some(
                TrackEventPlan::Set { value: s, .. }
                | TrackEventPlan::Spring { target: s, .. }
                | TrackEventPlan::Ease { target: s, .. },
            ) => s,
            None => &channel.initial,
        };
        match scalar {
            ScalarPlan::Literal(value) => Some(*value),
            ScalarPlan::Target(_) => None,
        }
    }

    /// The `actor.property` channel: the existing one, or a new one starting at
    /// `initial`. An already declared channel keeps its initial value.
    pub fn channel(
        &mut self,
        actor: &ActorHandle,
        property: &str,
        initial: impl Into<ScalarPlan>,
    ) -> ContinuousHandle {
        let id = format!("{}.{}", actor.id, property);
        if self
            .plan
            .continuous_channels
            .iter()
            .any(|channel| channel.id == id)
        {
            return ContinuousHandle { id };
        }
        self.continuous(actor, property, initial)
    }

    pub fn continuous(
        &mut self,
        actor: &ActorHandle,
        property: impl Into<String>,
        initial: impl Into<ScalarPlan>,
    ) -> ContinuousHandle {
        let property = property.into();
        let id = format!("{}.{}", actor.id, property);
        self.plan.continuous_channels.push(ContinuousChannelPlan {
            id: id.clone(),
            actor_id: actor.id.clone(),
            property,
            initial: initial.into(),
            events: Vec::new(),
        });
        ContinuousHandle { id }
    }

    /// One destination per already declared Presentation Step. The first value
    /// is the initial pose; only changed later destinations emit spring events.
    /// Ordinary `spring` remains an explicit write and never suppresses events.
    pub fn step_track(
        &mut self,
        actor: &ActorHandle,
        property: impl Into<String>,
        values: &[f32],
        motion: crate::plan::SpringPlan,
    ) -> ContinuousHandle {
        let property = property.into();
        assert!(
            !values.is_empty(),
            "a destination track needs an initial value"
        );
        assert_eq!(
            values.len(),
            self.plan.presentation_steps.len(),
            "one destination per step: {}.{property}",
            actor.id
        );
        let channel = crate::plan::destination_channel(
            &actor.id,
            property,
            values[0],
            self.plan
                .presentation_steps
                .iter()
                .zip(values)
                .skip(1)
                .map(|(step, value)| (step.start_nanos, *value)),
            |_, _| motion,
        );
        let handle = ContinuousHandle {
            id: channel.id.clone(),
        };
        self.plan.continuous_channels.push(channel);
        handle
    }

    pub fn duration_nanos(&self) -> u64 {
        self.plan.duration_nanos
    }

    /// Every continuous channel's ID, in declaration order: the index a
    /// validation diagnostic's `continuousChannels[n]` path names.
    pub fn channel_ids(&self) -> Vec<String> {
        self.plan
            .continuous_channels
            .iter()
            .map(|channel| channel.id.clone())
            .collect()
    }

    pub fn set(&mut self, channel: &ContinuousHandle, at_nanos: u64, value: f32) {
        self.set_to(channel, at_nanos, value.into());
    }

    pub fn set_to(&mut self, channel: &ContinuousHandle, at_nanos: u64, value: ScalarPlan) {
        self.continuous_channel_mut(channel)
            .events
            .push(TrackEventPlan::Set { at_nanos, value });
    }

    pub fn spring(
        &mut self,
        channel: &ContinuousHandle,
        at_nanos: u64,
        target: f32,
        visual_duration: f32,
        bounce: f32,
    ) {
        self.spring_to(channel, at_nanos, target.into(), visual_duration, bounce);
    }

    pub fn spring_to(
        &mut self,
        channel: &ContinuousHandle,
        at_nanos: u64,
        target: ScalarPlan,
        visual_duration: f32,
        bounce: f32,
    ) {
        let event =
            crate::plan::SpringPlan::visual(visual_duration, bounce).event(at_nanos, target);
        self.continuous_channel_mut(channel).events.push(event);
    }

    /// Spring with an explicit profile, as when a dimensionless weight needs
    /// tighter settling thresholds than a pixel channel.
    pub fn spring_with(
        &mut self,
        channel: &ContinuousHandle,
        at_nanos: u64,
        target: f32,
        spring: crate::plan::SpringPlan,
    ) {
        let event = spring.event(at_nanos, target);
        self.continuous_channel_mut(channel).events.push(event);
    }

    /// Move `channel` from its current value to `target` along `curve` over
    /// `seconds` (rounded to whole milliseconds, so f32 durations stay exact).
    pub fn ease(
        &mut self,
        channel: &ContinuousHandle,
        at_nanos: u64,
        target: f32,
        seconds: f32,
        curve: Ease,
    ) {
        let duration_nanos = whole_millis(seconds);
        self.continuous_channel_mut(channel)
            .events
            .push(TrackEventPlan::Ease {
                at_nanos,
                target: target.into(),
                duration_nanos,
                curve,
            });
    }

    pub fn semantic_target(
        &mut self,
        id: impl Into<String>,
        actor: &ActorHandle,
        selector: impl Serialize,
    ) -> Result<SemanticTargetHandle, serde_json::Error> {
        let id = id.into();
        self.plan.semantic_targets.push(SemanticTargetPlan {
            id: id.clone(),
            actor_id: actor.id.clone(),
            selector: serde_json::to_value(selector)?,
        });
        Ok(SemanticTargetHandle { id })
    }

    pub fn state(
        &mut self,
        actor: &ActorHandle,
        name: impl Into<String>,
        initial: impl Serialize,
    ) -> Result<StateHandle, serde_json::Error> {
        let name = name.into();
        let id = format!("{}.{}", actor.id, name);
        self.plan.state_channels.push(StateChannelPlan {
            id: id.clone(),
            actor_id: actor.id.clone(),
            state: name,
            initial: serde_json::to_value(initial)?,
            events: Vec::new(),
        });
        Ok(StateHandle { id })
    }

    pub fn change(
        &mut self,
        channel: &StateHandle,
        at_nanos: u64,
        value: impl Serialize,
    ) -> Result<(), serde_json::Error> {
        let value = serde_json::to_value(value)?;
        self.state_channel_mut(channel)
            .events
            .push(StateEventPlan { at_nanos, value });
        Ok(())
    }

    pub fn cue(&mut self, id: impl Into<String>, start_nanos: u64, end_nanos: u64) {
        self.plan.cues.push(CuePlan {
            id: id.into(),
            start_nanos,
            end_nanos,
        });
    }

    pub fn media(&mut self, media: MediaPlan) {
        self.plan.media.push(media);
    }

    /// Declare a presentation stop without changing the automatic video timing.
    pub fn presentation_step(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        start_nanos: u64,
        hold_nanos: u64,
    ) {
        self.plan.presentation_steps.push(PresentationStepPlan {
            id: id.into(),
            title: title.into(),
            start_nanos,
            hold_nanos,
        });
    }

    /// The motion state of `actor.property` at `at_nanos` under the events
    /// written so far, or `None` when the channel is undeclared or refers to
    /// Semantic Targets. Helpers read the pose they move from this way; an
    /// event written later cannot change a value already read.
    pub fn sample(
        &self,
        actor: &ActorHandle,
        property: &str,
        at_nanos: u64,
    ) -> Option<crate::motion::MotionState> {
        let id = format!("{}.{}", actor.id, property);
        let channel = self
            .plan
            .continuous_channels
            .iter()
            .find(|channel| channel.id == id)?;
        let key = crate::timeline::PropertyId::new(&id);
        let timeline = crate::plan::compile_channels(
            [(channel, key.clone())],
            self.plan.duration_nanos,
            |scalar| match scalar {
                ScalarPlan::Literal(value) => Ok(*value),
                ScalarPlan::Target(_) => anyhow::bail!("a semantic scalar has no authored value"),
            },
        )
        .ok()?;
        timeline.sample_at(&key, at_nanos as f64 / 1e9)
    }

    /// Where `actor.property` is headed at `at_nanos`: the target of its
    /// latest event at or before then (equal times in written order), else
    /// its initial value. `None` when undeclared or semantic.
    pub fn destination(&self, actor: &ActorHandle, property: &str, at_nanos: u64) -> Option<f32> {
        let id = format!("{}.{}", actor.id, property);
        let channel = self
            .plan
            .continuous_channels
            .iter()
            .find(|channel| channel.id == id)?;
        let latest = channel
            .events
            .iter()
            .enumerate()
            .filter(|(_, event)| event.at_nanos() <= at_nanos)
            .max_by_key(|(index, event)| (event.at_nanos(), *index))
            .map_or(&channel.initial, |(_, event)| event.scalar());
        match latest {
            ScalarPlan::Literal(value) => Some(*value),
            ScalarPlan::Target(_) => None,
        }
    }

    /// One Presentation Step per title, `beat` apart from time zero, with IDs
    /// `{prefix}-0`, `{prefix}-1`, …: each enters at its beat and holds
    /// `settle` later, once its motion rests; the first is a still at zero.
    /// Returns each step's entry time.
    pub fn steps<T: Into<String>>(
        &mut self,
        prefix: &str,
        titles: impl IntoIterator<Item = T>,
        beat: u64,
        settle: u64,
    ) -> Vec<u64> {
        (0..)
            .zip(titles)
            .map(|(index, title)| {
                let at = index * beat;
                let hold = if index == 0 { 0 } else { at + settle };
                self.presentation_step(format!("{prefix}-{index}"), title, at, hold);
                at
            })
            .collect()
    }

    /// A Cue for every Presentation Step declared so far, named like the step
    /// and spanning `beat` from its entry, so one step can be rendered alone.
    pub fn cue_steps(&mut self, beat: u64) {
        let steps = self
            .plan
            .presentation_steps
            .iter()
            .map(|step| (step.id.clone(), step.start_nanos))
            .collect::<Vec<_>>();
        for (id, start) in steps {
            self.cue(id, start, start + beat);
        }
    }

    /// Order every continuous channel's events by time, keeping the source
    /// order of events at the same instant. Beats authored independently may
    /// write one channel out of order (a jolt inside a kick's rebound, say);
    /// the timeline samples sorted events the same way.
    pub fn sort_events(&mut self) {
        for channel in &mut self.plan.continuous_channels {
            channel.events.sort_by_key(|event| event.at_nanos());
        }
    }

    /// Drop continuous events that start after the scene ends; they can
    /// never be sampled. A segment cut from a longer gesture (a bounce that
    /// would settle after the cut) keeps everything before its end.
    pub fn drop_events_after_end(&mut self) {
        let end = self.plan.duration_nanos;
        for channel in &mut self.plan.continuous_channels {
            channel.events.retain(|event| event.at_nanos() <= end);
        }
    }

    pub fn finish(mut self) -> Result<ScenePlan, crate::plan::PlanValidationError> {
        for channel in &mut self.plan.continuous_channels {
            channel.events.sort_by_key(TrackEventPlan::at_nanos);
        }
        for channel in &mut self.plan.state_channels {
            channel.events.sort_by_key(|event| event.at_nanos);
        }
        self.plan.validate()?;
        Ok(self.plan)
    }

    fn continuous_channel_mut(&mut self, handle: &ContinuousHandle) -> &mut ContinuousChannelPlan {
        self.plan
            .continuous_channels
            .iter_mut()
            .find(|channel| channel.id == handle.id)
            .expect("continuous handle belongs to this plan builder")
    }

    fn state_channel_mut(&mut self, handle: &StateHandle) -> &mut StateChannelPlan {
        self.plan
            .state_channels
            .iter_mut()
            .find(|channel| channel.id == handle.id)
            .expect("state handle belongs to this plan builder")
    }
}

/// Declare a recipe's canonical ID, strict continuous channel table,
/// typed sampled channel struct (with renderer rest defaults), and typed
/// [`ContinuousHandle`] accessors on its authoring actor handle from one
/// single source of truth.
#[macro_export]
macro_rules! recipe_channels {
    (
        $(#[$recipe_meta:meta])*
        recipe: $recipe_const:ident = $recipe_str:literal,
        plan: $plan_ty:ty,
        actor: $actor_ty:ty,
        $(#[$sampled_meta:meta])*
        sampled: $sampled_vis:vis struct $sampled_ty:ident {
            $(
                $(#[$field_meta:meta])*
                $field:ident : $prop:literal, default = $default:expr, initial = $initial:expr
            ),+ $(,)?
        }
    ) => {
        $(#[$recipe_meta])*
        pub const $recipe_const: &str = $recipe_str;

        $(#[$sampled_meta])*
        #[derive(Clone, Copy, Debug, PartialEq)]
        $sampled_vis struct $sampled_ty {
            $(
                $(#[$field_meta])*
                pub $field: f32,
            )+
        }

        impl $sampled_ty {
            /// Rest values sampled when a channel is undeclared in the plan.
            pub const REST: Self = Self {
                $( $field: $default, )+
            };

            /// Sample every channel of this recipe from `sample(property, default)`.
            #[inline]
            pub fn sample(mut sample: impl FnMut(&str, f32) -> f32) -> Self {
                Self {
                    $( $field: sample($prop, $default), )+
                }
            }
        }

        impl $plan_ty {
            /// Canonical continuous channel property names accepted by this recipe.
            pub const CHANNELS: &'static [&'static str] = &[ $( $prop ),+ ];

            /// True when `property` is a valid continuous channel on this recipe.
            #[inline]
            pub fn accepts(property: &str) -> bool {
                matches!(property, $( $prop )|+ )
            }
        }

        impl $actor_ty {
            $(
                $(#[$field_meta])*
                #[inline]
                pub fn $field(&mut self, scene: &mut $crate::author::PlanBuilder) -> $crate::author::ContinuousHandle {
                    self.channel(scene, $prop, $initial)
                }
            )+
        }
    };
}

pub use recipe_channels;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::PlanBuilder;

    #[test]
    fn step_destinations_use_authored_times_and_do_not_restart_constants() {
        let mut p = PlanBuilder::new("steps", 3_000_000_000);
        for (index, at) in [0, 800_000_000, 2_000_000_000].into_iter().enumerate() {
            p.presentation_step(format!("s{index}"), "step", at, at);
        }
        let actor = p
            .actor("a", "title-card", json!({"title":"Example"}))
            .unwrap();
        p.step_track(
            &actor,
            "opacity",
            &[0., 0., 1.],
            crate::plan::SpringPlan::visual(0.16, 0.),
        );
        let plan = p.finish().unwrap();
        assert_eq!(plan.continuous_channels[0].events.len(), 1);
        assert_eq!(
            plan.continuous_channels[0].events[0].at_nanos(),
            2_000_000_000
        );
    }

    #[test]
    fn authored_values_and_destinations_read_the_events_written_so_far() {
        let mut scene = PlanBuilder::new("reads", 4_000_000_000);
        let actor = scene.actor("a", "title-card", json!({})).unwrap();
        assert_eq!(scene.sample(&actor, "x", 0), None, "undeclared");
        let x = scene.channel(&actor, "x", 10.0);
        scene.spring(&x, 1_000_000_000, 50.0, 0.8, 0.0);
        scene.set(&x, 3_000_000_000, -5.0);
        assert_eq!(
            scene.sample(&actor, "x", 500_000_000).unwrap().position,
            10.0
        );
        let moving = scene.sample(&actor, "x", 1_300_000_000).unwrap();
        assert!(moving.position > 10.0 && moving.position < 50.0 && moving.velocity > 0.0);
        assert_eq!(
            scene.sample(&actor, "x", 3_500_000_000).unwrap().position,
            -5.0
        );
        // Where it is headed, not where it is.
        assert_eq!(scene.destination(&actor, "x", 500_000_000), Some(10.0));
        assert_eq!(scene.destination(&actor, "x", 1_300_000_000), Some(50.0));
        assert_eq!(scene.destination(&actor, "x", 3_000_000_000), Some(-5.0));
    }

    #[test]
    fn channel_declares_once_in_first_use_order() {
        let mut scene = PlanBuilder::new("channels", 1_000_000_000);
        let actor = scene.actor("a", "title-card", json!({})).unwrap();
        let y = scene.channel(&actor, "y", 10.0);
        scene.channel(&actor, "opacity", 0.0);
        assert_eq!(scene.channel(&actor, "y", 99.0), y);
        let plan = scene.finish().unwrap();
        let ids = plan
            .continuous_channels
            .iter()
            .map(|channel| match channel.initial {
                crate::plan::ScalarPlan::Literal(value) => (channel.id.as_str(), value),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![("a.y", 10.0), ("a.opacity", 0.0)],
            "a repeated channel keeps its first initial value"
        );
    }

    #[test]
    fn handles_construct_valid_deterministic_channels() {
        let mut scene = PlanBuilder::new("demo", 2_000_000_000);
        let title = scene
            .actor("title", "title-card", json!({ "title": "Hello" }))
            .unwrap();
        let opacity = scene.continuous(&title, "opacity", 0.0);
        let subtitle = scene.state(&title, "subtitle", "First").unwrap();
        let target = scene
            .semantic_target("title-text", &title, json!({ "rangeId": "title" }))
            .unwrap();
        let x = scene.continuous(&title, "x", target.center_x());
        scene.spring(&opacity, 0, 1.0, 0.4, 0.0);
        scene.spring_to(
            &x,
            0,
            target.offset(crate::plan::TargetComponentPlan::X, 20.0),
            0.4,
            0.0,
        );
        scene.change(&subtitle, 1_000_000_000, "Second").unwrap();
        scene.cue("change", 1_000_000_000, 2_000_000_000);
        scene.presentation_step("change", "Change the title", 1_000_000_000, 2_000_000_000);
        let plan = scene.finish().unwrap();

        assert_eq!(opacity.id(), "title.opacity");
        assert_eq!(plan.state_channels[0].id, "title.subtitle");
        assert_eq!(plan.continuous_channels[0].actor_id, title.id());
        assert_eq!(plan.semantic_targets[0].id, "title-text");
        assert_eq!(plan.cues[0].id, "change");
        assert_eq!(plan.presentation_steps[0].title, "Change the title");
    }
}

#[cfg(test)]
mod timing_tests {
    use super::*;

    #[test]
    fn stagger_starts_beats_a_gap_apart_and_returns_the_latest_end() {
        let mut starts = Vec::new();
        let end = stagger(["a", "b", "c"], SECOND, millis(120), |item, at| {
            starts.push((item, at));
            at + if item == "b" { 2 * SECOND } else { SECOND }
        });
        assert_eq!(
            starts,
            [
                ("a", SECOND),
                ("b", SECOND + millis(120)),
                ("c", SECOND + millis(240))
            ]
        );
        assert_eq!(end, 3 * SECOND + millis(120), "b ends last");
        assert_eq!(stagger(Vec::<u8>::new(), SECOND, 1, |_, at| at), SECOND);
    }

    #[test]
    fn spread_includes_both_ends() {
        assert_eq!(
            spread(3, 0, SECOND).collect::<Vec<_>>(),
            [0, SECOND / 2, SECOND]
        );
        assert_eq!(spread(1, 7, 99).collect::<Vec<_>>(), [7]);
        assert_eq!(spread(0, 7, 99).count(), 0);
        assert_eq!(spread(4, 10, 10).collect::<Vec<_>>(), [10; 4]);
        assert_eq!(
            spread(3, 0, 10).last(),
            Some(10),
            "whole nanoseconds, exact end"
        );
    }

    #[test]
    fn times_read_as_clamps_and_milliseconds() {
        assert_eq!(millis(420), seconds(0.42));
        assert_eq!(5.not_before(9), 9);
        assert_eq!(12.not_before(9), 12);
    }

    #[test]
    fn named_feels_match_their_literal_springs() {
        for (feel, duration, bounce) in [
            (SpringPlan::PANEL, 0.6, 0.0),
            (SpringPlan::CONTENT, 0.36, 0.0),
            (SpringPlan::ENTER, 0.45, 0.0),
            (SpringPlan::EXIT, 0.24, 0.0),
            (SpringPlan::MOVE, 0.6, 0.0),
            (SpringPlan::SNAP, 0.3, 0.0),
            (SpringPlan::POP, 0.32, 0.0),
            (SpringPlan::CAMERA, 1.6, 0.0),
            (SpringPlan::LIVELY, 0.85, 0.0),
        ] {
            assert_eq!(feel, SpringPlan::visual(duration, bounce));
        }
        // Kit's palette, in knobs' terms: ω = 2π / duration, ζ = 1 − bounce.
        for (feel, duration, bounce) in [
            (SpringPlan::INSTANT, 0.1, 0.0),
            (SpringPlan::SNAPPY, 0.25, 0.0),
            (SpringPlan::DEFAULT, 0.3, 0.0),
            (SpringPlan::CALM, 0.4, 0.0),
            (SpringPlan::SMOOTH, 0.45, 0.0),
            (SpringPlan::MORPH_OPEN, 0.42, 0.16),
            (SpringPlan::MORPH_CLOSE, 0.38, 0.08),
            (SpringPlan::PLAYFUL, 0.3, 0.3),
            (SpringPlan::SLOW, 0.85, 0.0),
            (SpringPlan::CAMERA_QUICK, 1.0, 0.0),
        ] {
            assert_eq!(feel.response_seconds, duration);
            assert_eq!(feel.damping_ratio, 1.0 - bounce);
        }
        let lead = SpringPlan::FOLLOW_LEAD;
        assert!((std::f32::consts::TAU / lead.response_seconds - 34.0).abs() < 1e-4);
        assert_eq!(lead.damping_ratio, 0.86);
        let tight = SpringPlan::MOVE.with_thresholds(1e-5, 1e-5);
        assert_eq!(tight.position_threshold, 1e-5);
        assert_eq!(tight.velocity_threshold, 1e-5);
        assert_eq!(tight.response_seconds, SpringPlan::MOVE.response_seconds);
    }
}

#[cfg(test)]
mod step_tests {
    use super::*;

    #[test]
    fn a_step_deck_holds_each_beat_once_it_settles() {
        let mut scene = PlanBuilder::new("deck", 9 * SECOND);
        let entries = scene.steps("step", ["one", "two", "three"], 3 * SECOND, 2 * SECOND);
        assert_eq!(entries, [0, 3 * SECOND, 6 * SECOND]);
        scene.cue_steps(3 * SECOND);
        let plan = scene.finish().unwrap();
        let holds = plan
            .presentation_steps
            .iter()
            .map(|step| (step.id.as_str(), step.start_nanos, step.hold_nanos))
            .collect::<Vec<_>>();
        assert_eq!(
            holds,
            [
                ("step-0", 0, 0),
                ("step-1", 3 * SECOND, 5 * SECOND),
                ("step-2", 6 * SECOND, 8 * SECOND)
            ]
        );
        assert_eq!(plan.cues[2].id, "step-2");
        assert_eq!(plan.cues[2].end_nanos, 9 * SECOND);
    }
}
