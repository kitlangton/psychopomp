//! Header entrances lower to ordinary tracks. Only a resting hidden word opts
//! into delay; authored/live paths share the cancellation-safe numeric schedule.
use crate::render::{HeaderGlyphs, HeadlessRenderer, header_words};
use anyhow::{Context, Result, bail};
use psychopomp::{
    component_prototype::{HeaderPlan, HeaderSplit},
    plan::{ContinuousChannelPlan, ScenePlan},
    playback::StartDelay,
    timeline::{PropertyId, SpringProfile},
};
use std::{collections::HashMap, time::Duration};

pub(super) struct PreparedHeader {
    id: String,
    plan: HeaderPlan,
    glyphs: HeaderGlyphs,
}

fn validate_recipe(p: &HeaderPlan, duration: u64) -> Result<()> {
    let count = header_words(&p.text, p.split).len();
    if p.text.trim().is_empty()
        || p.text.len() > 600
        || p.text.contains(['\n', '\r'])
        || count > 20
        || count == 0
        || p.origin.iter().any(|v| !v.is_finite())
        || !p.width.is_finite()
        || !(80.0..=1600.).contains(&p.width)
        || !p.font_size.is_finite()
        || !(16.0..=120.).contains(&p.font_size)
        || !p.duration_seconds.is_finite()
        || !(0.15..=0.8).contains(&p.duration_seconds)
        || p.stagger_millis > 100
        || p.stagger_millis * count.saturating_sub(1) as u64 > 600
        || p.split == HeaderSplit::Line && p.stagger_millis != 0
    {
        bail!(
            "header requires bounded single-line text, 1..20 words, finite layout, 150..800ms motion, and a total stagger no longer than 600ms"
        );
    }
    if let Some(r) = p.reflection
        && (!r.opacity.is_finite()
            || !(0.0..=0.6).contains(&r.opacity)
            || !r.depth.is_finite()
            || !(1.0..=120.).contains(&r.depth)
            || !r.gap.is_finite()
            || !(0.0..=30.).contains(&r.gap))
    {
        bail!("invalid header reflection opacity, depth, or gap");
    }
    let mut previous = 0;
    for e in &p.events {
        if e.at_nanos == 0 || e.at_nanos < previous || e.at_nanos > duration {
            bail!("header events must be ordered, positive, and within duration");
        }
        previous = e.at_nanos;
    }
    Ok(())
}

fn channel(id: &str, p: &HeaderPlan, index: usize, duration: u64) -> Result<ContinuousChannelPlan> {
    use psychopomp::timeline::{Retarget, RetargetMode, RetargetSchedule, ScheduleTime};
    let name = format!("{id}.__header.{index}.reveal");
    let property = PropertyId::new(&name);
    let initial = f32::from(p.visible);
    let response = p.duration_seconds;
    let profile = SpringProfile::new(response, 1., 0.00001, 0.00001);
    let mut schedule = RetargetSchedule::new(vec![(property.clone(), initial)])?;
    for e in psychopomp::plan::effective_snapshots(&p.events, |e| e.at_nanos) {
        schedule
            .retarget(
                ScheduleTime::Nanos(e.at_nanos),
                [Retarget {
                    property: property.clone(),
                    target: f32::from(e.visible),
                    profile,
                    delay: Some(StartDelay {
                        from: 0.,
                        to: 1.,
                        delay: Duration::from_millis(index as u64 * p.stagger_millis),
                    }),
                    mode: RetargetMode::Animate,
                }],
                Some(duration),
            )
            .context("header stagger exceeds scene duration")?;
    }
    let events = schedule
        .writes()
        .iter()
        .map(|write| {
            psychopomp::plan::SpringPlan {
                response_seconds: response,
                damping_ratio: 1.,
                position_threshold: 0.00001,
                velocity_threshold: 0.00001,
            }
            .event(
                write.at.nanos().expect("authored integer start"),
                write.target,
            )
        })
        .collect();
    Ok(ContinuousChannelPlan {
        id: name,
        actor_id: id.into(),
        property: format!("__header.{index}.reveal"),
        initial: initial.into(),
        events,
    })
}

pub(super) fn compile_inputs(plan: &mut ScenePlan, inputs: &[(String, HeaderPlan)]) -> Result<()> {
    for (id, p) in inputs {
        validate_recipe(p, plan.duration_nanos)?;
        if plan
            .continuous_channels
            .iter()
            .any(|c| c.actor_id == *id && c.property.starts_with("__header."))
        {
            bail!("authored channels cannot use reserved __header properties");
        }
        for index in 0..header_words(&p.text, p.split).len() {
            let c = channel(id, p, index, plan.duration_nanos)?;
            super::generated::extend(plan, [c], super::generated::Owner::Header)?;
        }
    }
    Ok(())
}

impl PreparedHeader {
    pub(super) fn from_recipe(
        id: String,
        plan: HeaderPlan,
        renderer: &mut HeadlessRenderer,
    ) -> Result<Self> {
        let glyphs = renderer.prepare_header(&plan)?;
        Ok(Self { id, plan, glyphs })
    }
    pub(super) fn delays(&self, delays: &mut HashMap<String, StartDelay>) {
        for index in 0..header_words(&self.plan.text, self.plan.split).len() {
            delays.insert(
                format!("{}.__header.{index}.reveal", self.id),
                StartDelay {
                    from: 0.,
                    to: 1.,
                    delay: Duration::from_millis(index as u64 * self.plan.stagger_millis),
                },
            );
        }
    }
    pub(super) fn render(
        &self,
        pixels: &mut [u8],
        renderer: &HeadlessRenderer,
        sample: impl Fn(&str, &str, f32) -> f32,
    ) {
        renderer.composite_header(pixels, &self.plan, &self.glyphs, |p, d| {
            sample(&self.id, p, d)
        });
    }

    pub(super) fn debug_line(&self, sample: impl Fn(&str, &str, f32) -> f32) -> String {
        let words = header_words(&self.plan.text, self.plan.split)
            .into_iter()
            .enumerate()
            .map(|(i, range)| {
                let value = sample(
                    &self.id,
                    &format!("__header.{i}.reveal"),
                    f32::from(self.plan.visible),
                );
                format!(
                    "{}:{:.0}%",
                    &self.plan.text[range],
                    value.clamp(0., 1.) * 100.
                )
            })
            .collect::<Vec<_>>()
            .join("  ");
        format!(
            "{} [{}ms gap] progress: {words}",
            self.id, self.plan.stagger_millis
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_runtime::{PreparedPlan, new_renderer};
    use psychopomp::motion::MotionState;
    use psychopomp::{
        component_prototype::{HEADER, HeaderEvent},
        playback::PlaybackCommand,
    };
    use std::path::Path;

    fn recipe() -> HeaderPlan {
        HeaderPlan {
            origin: [0., 0.],
            text: "Every word keeps its place.".into(),
            font_size: 62.,
            width: 1400.,
            split: HeaderSplit::Words,
            stagger_millis: 60,
            duration_seconds: 0.4,
            reflection: None,
            visible: false,
            events: vec![HeaderEvent {
                at_nanos: 3_000_000_000,
                visible: true,
            }],
        }
    }

    #[test]
    fn authored_cancellation_preserves_integer_order_beyond_f64_integer_precision() {
        let due = 1_u64 << 54;
        for cancel in [due - 1, due] {
            let mut p = recipe();
            p.events = vec![
                HeaderEvent {
                    at_nanos: due - 60_000_000,
                    visible: true,
                },
                HeaderEvent {
                    at_nanos: cancel,
                    visible: false,
                },
            ];
            let c = channel("header", &p, 1, due + 1_000_000_000).unwrap();
            assert_eq!(c.events.len(), if cancel < due { 1 } else { 2 });
            assert_eq!(c.events.last().unwrap().at_nanos(), cancel);
            let mut scene = ScenePlan::new("large-time", due + 1_000_000_000);
            scene.actors.push(psychopomp::plan::ActorPlan {
                id: "header".into(),
                recipe: HEADER.into(),
                data: serde_json::to_value(p).unwrap(),
            });
            scene.continuous_channels.push(c);
            scene.validate().unwrap();
        }
    }

    #[test]
    fn authored_staggers_cancel_superseded_starts_and_keep_unchanged_due_times() {
        let mut p = recipe();
        p.events.push(HeaderEvent {
            at_nanos: 3_010_000_000,
            visible: true,
        });
        let c = channel("header", &p, 4, 10_000_000_000).unwrap();
        assert_eq!(c.events.len(), 1);
        assert_eq!(c.events[0].at_nanos(), 3_240_000_000);
        p.events.extend([
            HeaderEvent {
                at_nanos: 3_050_000_000,
                visible: false,
            },
            HeaderEvent {
                at_nanos: 3_070_000_000,
                visible: true,
            },
        ]);
        let c = channel("header", &p, 4, 10_000_000_000).unwrap();
        assert!(!c.events.iter().any(|e| e.at_nanos() == 3_240_000_000));
        assert_eq!(c.events.last().unwrap().at_nanos(), 3_310_000_000);
        p.text = "too\nmany lines".into();
        assert!(validate_recipe(&p, 10_000_000_000).is_err());
    }

    #[test]
    #[ignore = "requires a headless GPU; authored and native stagger onsets match and pending words disappear on reversal"]
    fn staggered_header_pixels_match_export_and_cancel_waiting_words() {
        let plan = psychopomp_component_prototypes::build_slideshow_deck()
            .unwrap()
            .slides
            .pop()
            .unwrap()
            .plan;
        let mut renderer = pollster::block_on(new_renderer("staggered-header-proof")).unwrap();
        let p = PreparedPlan::prepare(plan, Path::new("."), &mut renderer).unwrap();
        let mut playback = p.playback(false).unwrap();
        let initial = p
            .render_sample_using(&mut renderer, 0., &playback.timeline())
            .unwrap();
        playback.command(PlaybackCommand::Next, Duration::ZERO);
        let timeline = playback.timeline();
        let first = PropertyId::new("words.__header.0.reveal");
        let last = PropertyId::new("words.__header.4.reveal");
        assert!(timeline.sample_at(&first, 0.08).unwrap().position > 0.);
        assert_eq!(timeline.sample_at(&last, 0.08), Some(MotionState::at(0.)));
        for at in [0.02, 0.08, 0.19, 0.3, 0.5, 1.] {
            assert!(
                p.render_sample_using(&mut renderer, at, &timeline).unwrap()
                    == p.render_sample(&mut renderer, 3. + at).unwrap(),
                "native/export onset mismatch at {at}"
            );
        }
        let before = p
            .render_sample_using(&mut renderer, 0.08, &timeline)
            .unwrap();
        playback.command(PlaybackCommand::Previous, Duration::from_millis(80));
        let reversed = playback.timeline();
        assert!(
            before
                == p.render_sample_using(&mut renderer, 0.08, &reversed)
                    .unwrap()
        );
        for at in [0.09, 0.25, 0.5, 2.] {
            assert_eq!(reversed.sample_at(&last, at), Some(MotionState::at(0.)));
        }
        assert!(
            initial == p.render_sample_using(&mut renderer, 2., &reversed).unwrap(),
            "no reflected or delayed word residue after reversal"
        );
    }
}
