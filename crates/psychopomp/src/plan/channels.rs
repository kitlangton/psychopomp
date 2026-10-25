//! Shared numeric lowering. Recipes retain scalar resolution, layout and motion
//! policy; raw authored events are never coalesced or suppressed here.
use anyhow::Result;

use super::{ContinuousChannelPlan, ScalarPlan, TrackEventPlan};
use crate::timeline::{PropertyId, SpringProfile, TimedEvent, Timeline};

/// The exact parameters serialized in a spring event. Retaining the authored
/// response avoids recovering it (with rounding) from angular frequency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringPlan {
    pub response_seconds: f32,
    pub damping_ratio: f32,
    pub position_threshold: f32,
    pub velocity_threshold: f32,
}

impl SpringPlan {
    /// A spring in knobs' and SwiftUI's terms, like Kit's palette:
    /// ω = 2π / duration, ζ = 1 − bounce.
    pub fn visual(duration: f32, bounce: f32) -> Self {
        assert!(
            duration.is_finite() && duration > 0.,
            "visual duration must be positive"
        );
        assert!((0.0..1.0).contains(&bounce), "bounce must be in [0, 1)");
        Self {
            response_seconds: duration,
            damping_ratio: 1. - bounce,
            position_threshold: 0.001,
            velocity_threshold: 0.001,
        }
    }

    /// A spring in Motion's terms, for ports of Motion code: its
    /// `visualDuration` is 1.2× shorter than the response.
    pub fn motion(visual_duration: f32, bounce: f32) -> Self {
        Self {
            response_seconds: visual_duration * 1.2,
            ..Self::visual(visual_duration, bounce)
        }
    }

    pub fn profile(self) -> SpringProfile {
        SpringProfile::new(
            self.response_seconds,
            self.damping_ratio,
            self.position_threshold,
            self.velocity_threshold,
        )
    }

    pub fn event(self, at_nanos: u64, target: impl Into<ScalarPlan>) -> TrackEventPlan {
        TrackEventPlan::Spring {
            at_nanos,
            target: target.into(),
            response_seconds: self.response_seconds,
            damping_ratio: self.damping_ratio,
            position_threshold: self.position_threshold,
            velocity_threshold: self.velocity_threshold,
        }
    }
}

impl TrackEventPlan {
    pub fn spring_plan(&self) -> Option<SpringPlan> {
        match *self {
            Self::Set { .. } | Self::Ease { .. } => None,
            Self::Spring {
                response_seconds,
                damping_ratio,
                position_threshold,
                velocity_threshold,
                ..
            } => Some(SpringPlan {
                response_seconds,
                damping_ratio,
                position_threshold,
                velocity_threshold,
            }),
        }
    }

    pub fn lower(
        &self,
        property: PropertyId,
        mut resolve: impl FnMut(&ScalarPlan) -> Result<f32>,
    ) -> Result<TimedEvent> {
        let at = self.at_nanos() as f64 / 1e9;
        Ok(match self {
            Self::Set { value, .. } => TimedEvent::set(at, property, resolve(value)?),
            Self::Spring { target, .. } => TimedEvent::spring(
                at,
                property,
                resolve(target)?,
                self.spring_plan().expect("spring event").profile(),
            ),
            Self::Ease {
                target,
                duration_nanos,
                curve,
                ..
            } => TimedEvent::ease(
                at,
                property,
                resolve(target)?,
                (*duration_nanos as f64 / 1e9) as f32,
                *curve,
            ),
        })
    }
}

/// Compile selected channels under caller-chosen IDs. In particular, the code
/// inspector uses local property names while delivery uses full channel IDs.
pub fn compile_channels<'a>(
    channels: impl IntoIterator<Item = (&'a ContinuousChannelPlan, PropertyId)>,
    duration_nanos: u64,
    mut resolve: impl FnMut(&ScalarPlan) -> Result<f32>,
) -> Result<Timeline> {
    let mut initial = Vec::new();
    let mut events = Vec::new();
    for (channel, property) in channels {
        initial.push((property.clone(), resolve(&channel.initial)?));
        for event in &channel.events {
            events.push(event.lower(property.clone(), &mut resolve)?);
        }
    }
    Timeline::compile_events(initial, events, duration_nanos as f64 / 1e9)
}

/// Ordered snapshots describe destinations, not observable intermediate writes.
/// Identity discovery and numeric lowering must see the same effective sequence.
pub fn effective_snapshots<T>(snapshots: &[T], at: impl Fn(&T) -> u64) -> impl Iterator<Item = &T> {
    snapshots
        .iter()
        .enumerate()
        .filter_map(move |(index, snapshot)| {
            snapshots
                .get(index + 1)
                .is_none_or(|next| at(next) != at(snapshot))
                .then_some(snapshot)
        })
}

/// Lower already computed, ordered snapshot destinations. The motion selector
/// runs only for changed destinations, allowing operation-specific profiles.
/// This opt-in policy must not be applied to raw authored Set/Spring events.
pub fn destination_channel(
    actor_id: &str,
    property: impl Into<String>,
    initial: f32,
    destinations: impl IntoIterator<Item = (u64, f32)>,
    mut motion: impl FnMut(f32, f32) -> SpringPlan,
) -> ContinuousChannelPlan {
    let property = property.into();
    let mut destinations = destinations.into_iter().peekable();
    let mut current = initial;
    let mut events = Vec::new();
    while let Some((at, target)) = destinations.next() {
        if destinations.peek().is_some_and(|(next, _)| *next == at) || target == current {
            continue;
        }
        events.push(motion(current, target).event(at, target));
        current = target;
    }
    ContinuousChannelPlan {
        id: format!("{actor_id}.{property}"),
        actor_id: actor_id.into(),
        property,
        initial: initial.into(),
        events,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::MotionState;

    #[test]
    fn lowering_distinguishes_adjacent_nanoseconds_and_raw_equal_time_order() {
        let literal = |s: &ScalarPlan| match s {
            ScalarPlan::Literal(v) => Ok(*v),
            _ => anyhow::bail!("literal"),
        };
        let mut c = destination_channel("a", "x", 0., [], |_, _| SpringPlan::visual(0.4, 0.));
        let id = PropertyId::new(&c.id);
        c.events = vec![
            TrackEventPlan::Set {
                at_nanos: 100_000_000_000,
                value: 7_f32.into(),
            },
            TrackEventPlan::Set {
                at_nanos: 100_000_000_001,
                value: 11_f32.into(),
            },
        ];
        let t = compile_channels([(&c, id.clone())], 101_000_000_000, literal).unwrap();
        assert_eq!(t.sample_at(&id, 100.), Some(MotionState::at(7.)));
        assert_eq!(
            t.sample_at(&id, 100_000_000_001_f64 / 1e9),
            Some(MotionState::at(11.))
        );
        c.events = vec![
            TrackEventPlan::Set {
                at_nanos: 0,
                value: 7_f32.into(),
            },
            SpringPlan::visual(0.4, 0.).event(0, 10.),
        ];
        let forward = compile_channels([(&c, id.clone())], 1_000_000_000, literal).unwrap();
        c.events.reverse();
        let reverse = compile_channels([(&c, id.clone())], 1_000_000_000, literal).unwrap();
        assert_ne!(forward.sample_at(&id, 0.1), reverse.sample_at(&id, 0.1));
    }

    #[test]
    fn lowering_preserves_raw_equal_time_order_and_nanosecond_precision() {
        let mut channel =
            destination_channel("actor", "x", 0., [], |_, _| SpringPlan::visual(0.4, 0.));
        channel.events = vec![
            TrackEventPlan::Set {
                at_nanos: 100_000_000_000,
                value: 7_f32.into(),
            },
            SpringPlan::visual(0.4, 0.).event(100_000_000_001, 10.),
        ];
        let literal = |v: &ScalarPlan| match v {
            ScalarPlan::Literal(v) => Ok(*v),
            _ => anyhow::bail!("literal only"),
        };
        let local = PropertyId::new("x");
        let timeline =
            compile_channels([(&channel, local.clone())], 101_000_000_000, literal).unwrap();
        assert_eq!(timeline.sample_at(&local, 100.), Some(MotionState::at(7.)));
        assert_eq!(
            timeline.sample_at(&local, 99.999),
            Some(MotionState::at(0.))
        );
        assert!(timeline.sample_at(&local, 100.1).unwrap().velocity > 0.);
        assert_eq!(
            timeline.sample_at(&local, 100.1),
            timeline.sample_at(&local, 100.1)
        );
        assert!(
            timeline
                .sample_at(&PropertyId::new("actor.x"), 100.)
                .is_none()
        );
    }

    #[test]
    fn destinations_coalesce_before_omitting_unchanged_targets() {
        let channel = destination_channel(
            "a",
            "x",
            0.,
            [(1, 1.), (1, 0.), (2, 2.), (3, 2.), (4, 0.)],
            |from, to| SpringPlan::visual(if to < from { 0.2 } else { 0.4 }, 0.),
        );
        assert_eq!(channel.events.len(), 2);
        assert_eq!(channel.events[0].at_nanos(), 2);
        assert_eq!(channel.events[1].at_nanos(), 4);
        assert_eq!(
            channel.events[1].spring_plan().unwrap().response_seconds,
            0.2
        );
        let source = [(1, "phantom"), (1, "final"), (2, "later")];
        assert_eq!(
            effective_snapshots(&source, |e| e.0)
                .map(|e| e.1)
                .collect::<Vec<_>>(),
            ["final", "later"]
        );
    }
}
