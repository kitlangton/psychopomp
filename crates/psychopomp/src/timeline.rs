use std::collections::HashMap;

use anyhow::{Result, bail};

use crate::{
    math::easing::Ease,
    motion::{MotionState, Spring},
};

mod retarget;
pub use retarget::{
    Retarget, RetargetMode, RetargetSchedule, ScheduleTime, ScheduledWrite, StartDelay,
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PropertyId(String);

impl PropertyId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SpringProfile {
    spring: Spring,
    position_threshold: f32,
    velocity_threshold: f32,
}

impl SpringProfile {
    pub fn new(
        response_seconds: f32,
        damping_ratio: f32,
        position_threshold: f32,
        velocity_threshold: f32,
    ) -> Self {
        assert!(
            position_threshold.is_finite() && position_threshold > 0.0,
            "position threshold must be positive"
        );
        assert!(
            velocity_threshold.is_finite() && velocity_threshold > 0.0,
            "velocity threshold must be positive"
        );
        Self {
            spring: Spring::new(response_seconds, damping_ratio),
            position_threshold,
            velocity_threshold,
        }
    }

    /// Matches knobs' and SwiftUI's `duration` and `bounce`: ω = 2π / duration,
    /// ζ = 1 − bounce.
    pub fn from_visual_duration(
        visual_duration_seconds: f32,
        bounce: f32,
        position_threshold: f32,
        velocity_threshold: f32,
    ) -> Self {
        let visual = crate::plan::SpringPlan::visual(visual_duration_seconds, bounce);
        Self::new(
            visual.response_seconds,
            visual.damping_ratio,
            position_threshold,
            velocity_threshold,
        )
    }
}

/// One write to a property; it starts from whatever the property is then.
#[derive(Clone, Debug)]
enum Animation {
    Set {
        property: PropertyId,
        value: f32,
    },
    Spring {
        property: PropertyId,
        target: f32,
        profile: SpringProfile,
    },
    /// Moves from wherever the property is to `target` along `curve` over
    /// `seconds`, independent of spring physics.
    Ease {
        property: PropertyId,
        target: f32,
        seconds: f32,
        curve: Ease,
    },
}

#[derive(Clone, Debug)]
pub struct TimedEvent {
    at: f64,
    animation: Animation,
}

impl TimedEvent {
    pub fn set(at: f64, property: PropertyId, value: f32) -> Self {
        Self::new(at, Animation::Set { property, value })
    }

    pub fn spring(at: f64, property: PropertyId, target: f32, profile: SpringProfile) -> Self {
        Self::new(
            at,
            Animation::Spring {
                property,
                target,
                profile,
            },
        )
    }

    pub fn ease(at: f64, property: PropertyId, target: f32, seconds: f32, curve: Ease) -> Self {
        Self::new(
            at,
            Animation::Ease {
                property,
                target,
                seconds,
                curve,
            },
        )
    }

    fn new(at: f64, animation: Animation) -> Self {
        assert!(
            at.is_finite() && at >= 0.0,
            "event time must be finite and non-negative"
        );
        Self { at, animation }
    }

    pub fn at(&self) -> f64 {
        self.at
    }
}

#[derive(Clone, Copy, Debug)]
enum SegmentKind {
    Set,
    Spring {
        spring: Spring,
        target: f32,
        settled_after: f64,
    },
    Ease {
        target: f32,
        seconds: f64,
        curve: Ease,
    },
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    start: f64,
    initial: MotionState,
    kind: SegmentKind,
}

impl Segment {
    fn sample(self, time: f64) -> MotionState {
        match self.kind {
            SegmentKind::Set => self.initial,
            SegmentKind::Spring {
                spring,
                target,
                settled_after,
            } => {
                let elapsed = time - self.start;
                if elapsed <= 0.0 {
                    self.initial
                } else if elapsed >= settled_after {
                    MotionState::at(target)
                } else {
                    spring.sample(self.initial, target, elapsed as f32)
                }
            }
            SegmentKind::Ease {
                target,
                seconds,
                curve,
            } => {
                let elapsed = time - self.start;
                if elapsed >= seconds {
                    return MotionState::at(target);
                }
                let progress = (elapsed.max(0.0) / seconds) as f32;
                let span = target - self.initial.position;
                MotionState {
                    position: self.initial.position + span * curve.sample(progress),
                    velocity: span * curve.slope(progress) / seconds as f32,
                }
            }
        }
    }
}

pub struct Timeline {
    tracks: HashMap<PropertyId, Vec<Segment>>,
}

impl Timeline {
    /// Compiles ordered writes authored on an explicit scene clock.
    ///
    /// Equal timestamps preserve input order, allowing imported formats to
    /// express a deterministic set followed by a spring retarget.
    pub fn compile_events(
        initial_values: impl IntoIterator<Item = (PropertyId, f32)>,
        events: impl IntoIterator<Item = TimedEvent>,
        duration: f64,
    ) -> Result<Self> {
        if !duration.is_finite() || duration < 0.0 {
            bail!("timeline duration must be finite and non-negative");
        }
        let mut timeline = Self::with_initial_values(initial_values)?;
        let mut events = events.into_iter().enumerate().collect::<Vec<_>>();
        events.sort_by(|(left_index, left), (right_index, right)| {
            left.at
                .total_cmp(&right.at)
                .then(left_index.cmp(right_index))
        });
        for (_, event) in events {
            if event.at > duration {
                bail!(
                    "timeline event at {:.3}s exceeds duration {duration:.3}s",
                    event.at
                );
            }
            timeline.compile_leaf(&event.animation, event.at)?;
        }
        Ok(timeline)
    }

    pub fn sample(&self, property: &PropertyId, time: f32) -> Option<MotionState> {
        self.sample_at(property, f64::from(time))
    }

    pub fn sample_at(&self, property: &PropertyId, time: f64) -> Option<MotionState> {
        let track = self.tracks.get(property)?;
        let time = time.max(0.0);
        let segment = track.iter().rev().find(|segment| segment.start <= time)?;
        Some(segment.sample(time))
    }

    fn compile_leaf(&mut self, animation: &Animation, start: f64) -> Result<()> {
        match animation {
            Animation::Set { property, value } => {
                if !value.is_finite() {
                    bail!("property '{}' set value must be finite", property.as_str());
                }
                self.push_segment(
                    property,
                    Segment {
                        start,
                        initial: MotionState::at(*value),
                        kind: SegmentKind::Set,
                    },
                );
            }
            Animation::Spring {
                property,
                target,
                profile,
            } => {
                if !target.is_finite() {
                    bail!(
                        "property '{}' spring target must be finite",
                        property.as_str()
                    );
                }
                let initial = self.sample_at(property, start).ok_or_else(|| {
                    anyhow::anyhow!("property '{}' has no initial value", property.as_str())
                })?;
                if !initial.position.is_finite() || !initial.velocity.is_finite() {
                    bail!(
                        "property '{}' has non-finite motion at retarget",
                        property.as_str()
                    );
                }
                self.push_segment(
                    property,
                    Segment {
                        start,
                        initial,
                        kind: SegmentKind::Spring {
                            spring: profile.spring,
                            target: *target,
                            settled_after: profile.spring.settling_time(
                                initial,
                                *target,
                                profile.position_threshold,
                                profile.velocity_threshold,
                            ),
                        },
                    },
                );
            }
            Animation::Ease {
                property,
                target,
                seconds,
                curve,
            } => {
                if !target.is_finite()
                    || !seconds.is_finite()
                    || *seconds <= 0.0
                    || !curve.is_valid()
                {
                    bail!(
                        "property '{}' ease needs a finite target, a positive duration, and a valid curve",
                        property.as_str()
                    );
                }
                let initial = self.sample_at(property, start).ok_or_else(|| {
                    anyhow::anyhow!("property '{}' has no initial value", property.as_str())
                })?;
                self.push_segment(
                    property,
                    Segment {
                        start,
                        initial,
                        kind: SegmentKind::Ease {
                            target: *target,
                            seconds: f64::from(*seconds),
                            curve: *curve,
                        },
                    },
                );
            }
        }
        Ok(())
    }

    fn with_initial_values(
        initial_values: impl IntoIterator<Item = (PropertyId, f32)>,
    ) -> Result<Self> {
        let mut tracks = HashMap::new();
        for (property, value) in initial_values {
            if !value.is_finite() {
                bail!(
                    "property '{}' initial value must be finite",
                    property.as_str()
                );
            }
            let id = property.as_str().to_owned();
            if tracks
                .insert(
                    property,
                    vec![Segment {
                        start: 0.0,
                        initial: MotionState::at(value),
                        kind: SegmentKind::Set,
                    }],
                )
                .is_some()
            {
                bail!("property '{id}' has more than one initial value");
            }
        }
        Ok(Self { tracks })
    }

    fn push_segment(&mut self, property: &PropertyId, segment: Segment) {
        let track = self.tracks.entry(property.clone()).or_default();
        let insertion = track.partition_point(|existing| existing.start <= segment.start);
        track.insert(insertion, segment);
    }
}

#[cfg(test)]
mod tests {
    use super::{PropertyId, SpringProfile, TimedEvent, Timeline};
    use crate::math::easing::Ease;

    #[test]
    fn eased_moves_follow_their_curve_and_hand_velocity_to_a_spring() {
        let x = PropertyId::new("beam.draw");
        let timeline = Timeline::compile_events(
            [(x.clone(), 0.0)],
            [
                TimedEvent::ease(1.0, x.clone(), 1.0, 0.5, Ease::Decelerate(0.4)),
                TimedEvent::spring(1.25, x.clone(), 0.2, profile()),
            ],
            4.0,
        )
        .unwrap();
        assert_eq!(timeline.sample_at(&x, 0.5).unwrap().position, 0.0);
        let launch = timeline.sample_at(&x, 1.0).unwrap();
        assert_eq!(launch.position, 0.0);
        assert!(
            (launch.velocity - 3.2).abs() < 1e-5,
            "fast launch: 1.6 x the average speed"
        );
        let middle = timeline.sample_at(&x, 1.2499).unwrap();
        let retarget = timeline.sample_at(&x, 1.25).unwrap();
        assert!((middle.position - retarget.position).abs() < 1e-3);
        assert!(
            (middle.velocity - retarget.velocity).abs() < 1e-2,
            "the spring inherits velocity"
        );
        let settled = Timeline::compile_events(
            [(x.clone(), 0.0)],
            [TimedEvent::ease(0.0, x.clone(), 1.0, 0.5, Ease::CubicOut)],
            1.0,
        )
        .unwrap();
        assert_eq!(
            settled.sample_at(&x, 0.5).unwrap(),
            crate::motion::MotionState::at(1.0)
        );
        assert!(
            Timeline::compile_events(
                [(x.clone(), 0.0)],
                [TimedEvent::ease(0.0, x, 1.0, 0.0, Ease::Linear)],
                1.0,
            )
            .is_err(),
            "an ease needs a positive duration"
        );
    }

    fn profile() -> SpringProfile {
        SpringProfile::new(0.5, 0.8, 0.02, 0.05)
    }

    fn compile(
        initial_values: impl IntoIterator<Item = (PropertyId, f32)>,
        events: impl IntoIterator<Item = TimedEvent>,
    ) -> anyhow::Result<Timeline> {
        Timeline::compile_events(initial_values, events, 10.0)
    }

    #[test]
    fn initial_values_are_validated_before_writes() {
        let initial = PropertyId::new("initial");
        let events = || {
            [
                TimedEvent::set(2.0, PropertyId::new("bad-set"), f32::NAN),
                TimedEvent::spring(0.5, PropertyId::new("missing"), 1.0, profile()),
            ]
        };
        for (values, expected) in [
            (
                vec![(initial.clone(), f32::NAN)],
                "property 'initial' initial value must be finite",
            ),
            (
                vec![(initial.clone(), 0.0), (initial, 1.0)],
                "property 'initial' has more than one initial value",
            ),
            (Vec::new(), "property 'missing' has no initial value"),
        ] {
            assert_eq!(
                compile(values, events()).err().unwrap().to_string(),
                expected
            );
        }
    }

    #[test]
    fn retargeting_is_independent_of_event_order() {
        let x = PropertyId::new("pointer.x");
        let first = TimedEvent::spring(0.0, x.clone(), 10.0, profile());
        let second = TimedEvent::spring(0.4, x.clone(), 20.0, profile());
        let forward = compile([(x.clone(), 0.0)], [first.clone(), second.clone()]).unwrap();
        let reversed = compile([(x.clone(), 0.0)], [second, first]).unwrap();

        assert!(forward.sample(&x, 0.4).unwrap().velocity.abs() > 0.0);
        assert!(forward.sample(&x, 2.0).unwrap().position > 19.9);
        for time in [0.0, 0.2, 0.4, 0.8, 2.0] {
            let forward = forward.sample(&x, time).unwrap();
            let reversed = reversed.sample(&x, time).unwrap();
            assert!((forward.position - reversed.position).abs() < 0.0001);
            assert!((forward.velocity - reversed.velocity).abs() < 0.0001);
        }
    }

    #[test]
    fn retargeted_springs_preserve_velocity() {
        let x = PropertyId::new("camera.x");
        let first = TimedEvent::spring(0.0, x.clone(), 100.0, profile());
        let uninterrupted = compile([(x.clone(), 0.0)], [first.clone()]).unwrap();
        let before_retarget = uninterrupted.sample_at(&x, 0.3).unwrap();
        let timeline = compile(
            [(x.clone(), 0.0)],
            [first, TimedEvent::spring(0.3, x.clone(), -40.0, profile())],
        )
        .unwrap();
        let at_retarget = timeline.sample_at(&x, 0.3).unwrap();

        assert_eq!(at_retarget, before_retarget);
        assert!(at_retarget.velocity.abs() > 0.001);
    }

    #[test]
    fn set_is_discontinuous_and_resets_velocity() {
        let x = PropertyId::new("panel.x");
        let timeline = compile(
            [(x.clone(), 0.0)],
            [
                TimedEvent::spring(0.0, x.clone(), 100.0, profile()),
                TimedEvent::set(0.3, x.clone(), -20.0),
            ],
        )
        .unwrap();
        let state = timeline.sample(&x, 0.3).unwrap();

        assert_eq!(state.position, -20.0);
        assert_eq!(state.velocity, 0.0);
    }

    #[test]
    fn spring_becomes_exactly_stationary_after_settling() {
        let x = PropertyId::new("panel.x");
        let timeline = compile(
            [(x.clone(), 0.0)],
            [TimedEvent::spring(0.0, x.clone(), 100.0, profile())],
        )
        .unwrap();
        let state = timeline.sample(&x, 10.0).unwrap();

        assert_eq!(state.position, 100.0);
        assert_eq!(state.velocity, 0.0);
    }

    #[test]
    fn visual_duration_matches_motion_parameterization() {
        let profile = SpringProfile::from_visual_duration(0.25, 0.0, 0.001, 0.001);
        let x = PropertyId::new("x");
        let timeline = compile(
            [(x.clone(), 0.0)],
            [TimedEvent::spring(0.0, x.clone(), 1.0, profile)],
        )
        .unwrap();
        let state = timeline.sample(&x, 0.25).unwrap();

        assert!((state.position - 0.9864).abs() < 0.0001);
        assert!(state.position < 1.0);
    }

    #[test]
    fn explicit_events_preserve_same_time_source_order() {
        let x = PropertyId::new("x");
        let profile = SpringProfile::new(0.4, 1.0, 0.001, 0.001);
        let timeline = Timeline::compile_events(
            [(x.clone(), 0.0)],
            [
                TimedEvent::set(0.5, x.clone(), 4.0),
                TimedEvent::spring(0.5, x.clone(), 10.0, profile),
            ],
            2.0,
        )
        .unwrap();

        let at = timeline.sample(&x, 0.5).unwrap();
        let moving = timeline.sample(&x, 0.6).unwrap();
        assert_eq!(at.position, 4.0);
        assert!(moving.position > 4.0);
    }

    #[test]
    fn explicit_events_sort_by_time_without_losing_equal_time_order() {
        let x = PropertyId::new("x");
        let timeline = Timeline::compile_events(
            [(x.clone(), 0.0)],
            [
                TimedEvent::set(1.0, x.clone(), 1.0),
                TimedEvent::set(0.5, x.clone(), 2.0),
                TimedEvent::set(0.5, x.clone(), 3.0),
            ],
            2.0,
        )
        .unwrap();

        assert_eq!(timeline.sample(&x, 0.5).unwrap().position, 3.0);
        assert_eq!(timeline.sample(&x, 1.0).unwrap().position, 1.0);
    }

    #[test]
    fn explicit_event_selection_retains_sub_microsecond_clock_precision() {
        let x = PropertyId::new("x");
        let timeline = Timeline::compile_events(
            [(x.clone(), 0.0)],
            [
                TimedEvent::set(100.0, x.clone(), 1.0),
                TimedEvent::set(100.000_000_001, x.clone(), 2.0),
            ],
            101.0,
        )
        .unwrap();

        assert_eq!(timeline.sample_at(&x, 100.0).unwrap().position, 1.0);
        assert_eq!(
            timeline.sample_at(&x, 100.000_000_001).unwrap().position,
            2.0
        );
    }
}
