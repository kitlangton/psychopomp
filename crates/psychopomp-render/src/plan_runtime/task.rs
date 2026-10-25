use crate::render::{BubblePose, ContentPose, HeadlessRenderer, TaskContentFrame, TaskVisualFrame};
use anyhow::{Context, Result, bail};
use psychopomp::{
    motion::MotionState,
    plan::ContinuousChannelPlan,
    task::{TaskRecipePlan, TaskState},
};

pub(super) struct PreparedTask {
    actor_id: String,
    recipe: TaskRecipePlan,
    states: Vec<TaskState>,
    widths: Vec<f32>,
}

impl PreparedTask {
    pub(super) fn from_recipe(
        actor_id: String,
        recipe: TaskRecipePlan,
        renderer: &mut HeadlessRenderer,
    ) -> Self {
        let states = recipe.states();
        let widths = states
            .iter()
            .map(|state| match state {
                TaskState::Succeeded(Some(result)) => renderer.measure_task_result_width(result),
                _ => 128.,
            })
            .collect();
        Self {
            actor_id,
            recipe,
            states,
            widths,
        }
    }

    pub(super) fn channels(&self) -> Vec<ContinuousChannelPlan> {
        // Effect Institute's Pixi config.ts and NodeController.ts deliberately
        // use different clocks for geometry, opacity, pop, and deblur. Do not
        // replace these with one state-progress spring plus another easing.
        let mut channels = vec![
            self.channel("x", 0.45, 0., |_| self.recipe.center[0]),
            self.channel("y", 0.45, 0., |_| self.recipe.center[1]),
            self.channel("width", 0.35, 0.35, |state| {
                self.widths[self.states.iter().position(|s| s == state).unwrap()]
            }),
            self.channel("height", 0.2, 0.5, height),
            self.channel("scale", 3. / 18., 0., size_scale),
            self.channel("opacity", 3. / 10., 0., visible),
            // The source enables activity immediately. A short continuity ramp
            // replaces that discrete switch, not a content-dependent delay.
            self.channel("activity", 0.06, 0., |state| {
                if matches!(state, TaskState::Running) {
                    1.
                } else {
                    0.
                }
            }),
        ];
        for (index, state) in self.states.iter().enumerate() {
            let presence = |current: &TaskState| if current == state { 1. } else { 0. };
            channels.push(self.channel(format!("state.{index}"), 3. / 12., 0., presence));
            if matches!(state, TaskState::Hidden | TaskState::Running) {
                continue;
            }
            let result = matches!(state, TaskState::Succeeded(_));
            let hidden_scale = if result { 0.5 } else { 0.7 };
            let hidden_blur = if result { 10. } else { 6. };
            let hidden_rotation = match state {
                TaskState::Idle => -45_f32.to_radians(),
                TaskState::Failed(_) | TaskState::Death(_) => 12_f32.to_radians(),
                _ => 0.,
            };
            // Retained identities use the same hidden pose on either side of a
            // reversal, rather than copying the source's mount/reset side effects.
            channels.extend([
                self.channel(format!("content.{index}.opacity"), 3. / 18., 0., presence),
                self.channel(
                    format!("content.{index}.scale"),
                    if result { 0.25 } else { 3. / 18. },
                    if result { 0.4 } else { 0. },
                    |current| hidden_scale + (1. - hidden_scale) * presence(current),
                ),
                self.channel(
                    format!("content.{index}.blur"),
                    if result { 0.15 } else { 3. / 18. },
                    0.,
                    |current| hidden_blur * (1. - presence(current)),
                ),
                self.channel(
                    format!("content.{index}.rotation"),
                    3. / 18.,
                    0.,
                    |current| hidden_rotation * (1. - presence(current)),
                ),
            ]);
            if matches!(state, TaskState::Failed(_) | TaskState::Death(_)) {
                channels.extend([
                    self.channel(format!("bubble.{index}.opacity"), 3. / 14., 0., presence),
                    self.channel(format!("bubble.{index}.scale"), 3. / 14., 0., |current| {
                        0.6 + 0.4 * presence(current)
                    }),
                    self.channel(format!("bubble.{index}.blur"), 3. / 16., 0., |current| {
                        6. * (1. - presence(current))
                    }),
                    self.channel(format!("bubble.{index}.y"), 0.25, 0.5, |current| {
                        32. * (1. - presence(current))
                    }),
                ]);
            }
        }
        channels
    }

    fn channel(
        &self,
        property: impl Into<String>,
        duration: f32,
        bounce: f32,
        value: impl Fn(&TaskState) -> f32,
    ) -> ContinuousChannelPlan {
        psychopomp::plan::destination_channel(
            &self.actor_id,
            property,
            value(&self.recipe.initial),
            psychopomp::plan::effective_snapshots(&self.recipe.events, |e| e.at_nanos)
                .map(|e| (e.at_nanos, value(&e.state))),
            |_, _| psychopomp::plan::SpringPlan::motion(duration, bounce),
        )
    }

    pub(super) fn running_property(&self) -> Option<String> {
        self.states
            .iter()
            .any(|state| matches!(state, TaskState::Running))
            .then(|| format!("{}.activity", self.actor_id))
    }

    pub(super) fn render(
        &self,
        pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        time: f64,
        sample: impl Fn(&str, &str) -> Option<MotionState>,
    ) -> Result<()> {
        let value = |property: &str| {
            sample(&self.actor_id, property)
                .map(|state| state.position)
                .with_context(|| format!("missing task channel '{}.{property}'", self.actor_id))
        };
        let states = self
            .states
            .iter()
            .enumerate()
            .map(|(index, state)| Ok((state, value(&format!("state.{index}"))?.clamp(0., 1.))))
            .collect::<Result<Vec<_>>>()?;
        let mut contents = Vec::new();
        for (index, state) in self.states.iter().enumerate() {
            if matches!(state, TaskState::Hidden | TaskState::Running) {
                continue;
            }
            let content_value = |property| value(&format!("content.{index}.{property}"));
            let bubble = if matches!(state, TaskState::Failed(_) | TaskState::Death(_)) {
                let bubble_value = |property| value(&format!("bubble.{index}.{property}"));
                Some(BubblePose {
                    content: ContentPose {
                        opacity: bubble_value("opacity")?.clamp(0., 1.),
                        scale: bubble_value("scale")?.max(0.1),
                        blur: bubble_value("blur")?.max(0.),
                        rotation: 0.,
                    },
                    y: bubble_value("y")?,
                })
            } else {
                None
            };
            contents.push(TaskContentFrame {
                state,
                pose: ContentPose {
                    opacity: content_value("opacity")?.clamp(0., 1.),
                    scale: content_value("scale")?.clamp(0.1, 1.2),
                    blur: content_value("blur")?.max(0.),
                    rotation: content_value("rotation")?,
                },
                bubble,
            });
        }
        let frame = TaskVisualFrame {
            id: &self.actor_id,
            name: &self.recipe.name,
            center: [value("x")?, value("y")?],
            size: [value("width")?.max(1.), value("height")?.max(1.)],
            scale: value("scale")?.max(0.),
            opacity: value("opacity")?.clamp(0., 1.),
            states: &states,
            contents: &contents,
            activity: value("activity")?,
            time,
        };
        if !time.is_finite() {
            bail!("task sample time must be finite");
        }
        renderer.composite_task_visual(pixels, frame);
        Ok(())
    }
}

fn height(state: &TaskState) -> f32 {
    if matches!(state, TaskState::Running) {
        128. * 0.4
    } else {
        128.
    }
}
fn size_scale(state: &TaskState) -> f32 {
    match state {
        TaskState::Running => 0.95,
        TaskState::Death(_) => 0.94,
        _ => 1.,
    }
}
fn visible(state: &TaskState) -> f32 {
    if matches!(state, TaskState::Hidden) {
        0.
    } else {
        1.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psychopomp::plan::ActorPlan;
    use psychopomp::plan::TrackEventPlan;
    use psychopomp::task::TaskEventPlan;
    use psychopomp::{
        plan::{PresentationStepPlan, ScenePlan},
        playback::PlaybackCommand,
        timeline::PropertyId,
    };

    fn task() -> PreparedTask {
        let recipe = TaskRecipePlan {
            name: "job".into(),
            center: [960., 510.],
            initial: TaskState::Idle,
            events: vec![
                TaskEventPlan {
                    at_nanos: 1_000_000_000,
                    state: TaskState::Running,
                },
                TaskEventPlan {
                    at_nanos: 3_000_000_000,
                    state: TaskState::Succeeded(Some("42".into())),
                },
                TaskEventPlan {
                    at_nanos: 5_000_000_000,
                    state: TaskState::Failed("Error".into()),
                },
            ],
        };
        PreparedTask {
            actor_id: "job".into(),
            states: recipe.states(),
            widths: vec![128., 128., 220., 128.],
            recipe,
        }
    }

    #[test]
    fn task_uses_effect_institute_operation_specific_timings() {
        let channels = task().channels();
        for (property, duration, bounce) in [
            ("height", 0.2, 0.5),
            ("scale", 3. / 18., 0.),
            ("content.0.opacity", 3. / 18., 0.),
            ("content.2.scale", 0.25, 0.4),
            ("content.2.blur", 0.15, 0.),
            ("bubble.3.opacity", 3. / 14., 0.),
            ("bubble.3.blur", 3. / 16., 0.),
            ("bubble.3.y", 0.25, 0.5),
        ] {
            let channel = channels
                .iter()
                .find(|channel| channel.property == property)
                .unwrap_or_else(|| panic!("missing independent {property} track"));
            let TrackEventPlan::Spring {
                response_seconds,
                damping_ratio,
                ..
            } = channel.events[0]
            else {
                panic!("expected spring")
            };
            assert!(
                (response_seconds - duration * 1.2).abs() < 1e-6,
                "{property}: got response {response_seconds}, expected {}",
                duration * 1.2
            );
            assert!(
                (damping_ratio - (1. - bounce)).abs() < 1e-6,
                "{property}: damping {damping_ratio}"
            );
        }
    }

    fn prepared() -> super::super::CompiledPlan {
        let task = task();
        let mut plan = ScenePlan::new("task-timing", 8_000_000_000);
        plan.actors.push(ActorPlan {
            id: "job".into(),
            recipe: psychopomp::task::TASK_RECIPE.into(),
            data: serde_json::to_value(&task.recipe).unwrap(),
        });
        plan.continuous_channels = task.channels();
        plan.presentation_steps = [0, 1, 3, 5]
            .into_iter()
            .enumerate()
            .map(|(i, at)| PresentationStepPlan {
                id: format!("step-{i}"),
                title: format!("Step {i}"),
                start_nanos: at * 1_000_000_000,
                hold_nanos: if i == 0 {
                    0
                } else {
                    at * 1_000_000_000 + 1_500_000_000
                },
            })
            .collect();
        plan.validate().unwrap();
        super::super::CompiledPlan::compile(plan, std::path::Path::new(".")).unwrap()
    }

    #[test]
    fn generated_channels_match_the_gpu_free_preflight_manifest() {
        let task = task();
        let expected = task.recipe.channel_properties();
        let actual = task
            .channels()
            .into_iter()
            .map(|c| c.property)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn actual_task_tracks_match_samples_from_effect_institutes_motion_version() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/effect-task-timing.json"))
                .unwrap();
        let prepared = prepared();
        for (property, profile, start, from, to) in [
            ("height", "height", 1., 128., 51.2),
            ("width", "width", 3., 128., 220.),
            ("scale", "icon", 1., 1., 0.95),
            ("content.0.opacity", "icon", 1., 1., 0.),
            ("content.0.scale", "icon", 1., 1., 0.7),
            ("content.0.blur", "icon", 1., 0., 6.),
            ("content.2.opacity", "icon", 3., 0., 1.),
            ("content.2.scale", "resultScale", 3., 0.5, 1.),
            ("content.2.blur", "resultBlur", 3., 10., 0.),
            ("bubble.3.opacity", "bubbleOpacity", 5., 0., 1.),
            ("bubble.3.scale", "bubbleOpacity", 5., 0.6, 1.),
            ("bubble.3.blur", "bubbleBlur", 5., 6., 0.),
            ("bubble.3.y", "bubbleY", 5., 32., 0.),
            ("state.1", "color", 1., 0., 1.),
        ] {
            for (time, progress) in fixture["times"]
                .as_array()
                .unwrap()
                .iter()
                .zip(fixture["profiles"][profile]["values"].as_array().unwrap())
            {
                let time = time.as_f64().unwrap();
                let expected = from + (to - from) * progress.as_f64().unwrap();
                let actual = prepared
                    .timeline
                    .sample_at(&PropertyId::new(format!("job.{property}")), start + time)
                    .unwrap()
                    .position;
                assert!(
                    (f64::from(actual) - expected).abs() < 0.002,
                    "{property} at {time}: {actual} != Motion {expected}"
                );
            }
        }
        // Actual visible properties—not just matching configuration strings.
        let value = |property: &str, time| {
            prepared
                .timeline
                .sample_at(&PropertyId::new(format!("job.{property}")), time)
                .unwrap()
                .position
        };
        assert!(value("height", 1.1) < 51.2); // compression already passed its target
        assert!(value("content.2.scale", 3.2) > 1.04); // result pops past natural size
        assert!(value("content.2.blur", 3.15) < 0.34); // crisp before scale has settled
        assert!(value("bubble.3.y", 5.15) < 0.); // bubble rise has its own follow-through
        assert!(value("activity", 1.05) > 0.9); // no former 58%-of-content wait
    }

    #[test]
    fn source_timed_content_reverses_and_keeps_unrelated_channels_running() {
        use std::time::Duration;
        let prepared = prepared();
        let mut playback = prepared.playback(false).unwrap();
        playback.command(PlaybackCommand::Next, Duration::ZERO);
        for (time, command) in [
            (0.03, PlaybackCommand::Last),
            (0.065, PlaybackCommand::First),
            (0.09, PlaybackCommand::Next),
            (0.15, PlaybackCommand::Next),
        ] {
            let previous = playback.timeline();
            playback.command(command, Duration::from_secs_f64(time));
            for channel in &prepared.plan.continuous_channels {
                let property = PropertyId::new(&channel.id);
                assert_eq!(
                    previous.sample_at(&property, time),
                    playback.timeline().sample_at(&property, time),
                    "{} interrupted at {time}",
                    channel.id
                );
            }
        }
        let before = playback.timeline();
        playback.command(PlaybackCommand::Last, Duration::from_millis(175));
        // The idle icon stays absent across Running -> Success -> Failure. Its
        // independent fade/scale/blur/rotation may finish, but cannot restart.
        for suffix in ["opacity", "scale", "blur", "rotation"] {
            let property = PropertyId::new(format!("job.content.0.{suffix}"));
            for time in [0.175, 0.2, 0.3, 0.8] {
                assert_eq!(
                    before.sample_at(&property, time),
                    playback.timeline().sample_at(&property, time)
                );
            }
        }
    }
}
