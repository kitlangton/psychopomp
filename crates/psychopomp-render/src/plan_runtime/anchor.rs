//! Shared anchor resolution for every pinnable overlay (callouts, captions,
//! Rolling Numbers, text, images). Preflight checks that each anchor names
//! something the plan can place; at every sample the prepared root places it:
//! `render::stage_anchor` projects a Stage element through the same camera,
//! roll, and punch-in the Stage paints with, and `PreparedEditor::anchor`
//! carries a measured code range through line motion and the panel's card
//! projection. Sequence Diagram participants and rows are placed by their own
//! prepared overlay. Overlays only ask for points.
use anyhow::{Result, bail};
use psychopomp::{
    anchor::{self, AnchorPlan, AnchorTarget},
    math::{Vec2, vec2},
    motion::MotionState,
    plan::SemanticTargetPlan,
    stage::StageElement,
};

use super::{
    PreparedRoot, preflight::RootPlan, rune_surface::PreparedRuneSurface,
    sequence::PreparedSequence,
};
use crate::render::{stage_anchor, stage_rune_anchor};

/// What anchors may name: the root, its Semantic Targets, the plan's
/// Sequence Diagrams, and any `RuneSurface` actors.
pub(super) struct Placeable<'a> {
    pub root: &'a RootPlan,
    pub targets: &'a [SemanticTargetPlan],
    pub sequences: &'a [PreparedSequence],
    pub rune_surfaces: &'a [PreparedRuneSurface],
}

/// Every anchor of `kind` actor `owner` must name something the plan can place.
pub(super) fn validate<'a>(
    kind: &str,
    owner: &str,
    anchors: impl IntoIterator<Item = (&'a str, AnchorTarget<'a>)>,
    placeable: &Placeable<'_>,
) -> Result<()> {
    let Placeable {
        root,
        targets,
        sequences,
        rune_surfaces,
    } = placeable;
    for (id, target) in anchors {
        match (target, root) {
            (AnchorTarget::Point(_), _) => {}
            (
                AnchorTarget::Participant {
                    sequence,
                    participant,
                    ..
                },
                _,
            ) => {
                let Some(found) = sequences.iter().find(|s| s.id() == sequence) else {
                    bail!(
                        "{kind} '{owner}' anchor '{id}' references unknown sequence '{sequence}'"
                    );
                };
                if found.plan().participant_index(participant).is_none() {
                    bail!(
                        "{kind} '{owner}' anchor '{id}' references unknown participant '{participant}' of sequence '{sequence}'"
                    );
                }
            }
            (AnchorTarget::Row { sequence, row, .. }, _) => {
                let Some(found) = sequences.iter().find(|s| s.id() == sequence) else {
                    bail!(
                        "{kind} '{owner}' anchor '{id}' references unknown sequence '{sequence}'"
                    );
                };
                if found.plan().row_index(row).is_none() {
                    bail!(
                        "{kind} '{owner}' anchor '{id}' references unknown row '{row}' of sequence '{sequence}'"
                    );
                }
            }
            (AnchorTarget::Stage { element, .. }, RootPlan::Stage { recipe, .. }) => {
                if recipe
                    .element(element)
                    .and_then(StageElement::anchor)
                    .is_none()
                {
                    bail!(
                        "{kind} '{owner}' anchor '{id}' needs a positioned stage element; '{element}' is not one"
                    );
                }
            }
            (AnchorTarget::Editor { target, .. }, RootPlan::Editor { editor, .. }) => {
                if !targets
                    .iter()
                    .any(|t| t.id == target && t.actor_id == editor.actor_id())
                {
                    bail!(
                        "{kind} '{owner}' anchor '{id}' references unknown editor semantic target '{target}'"
                    );
                }
            }
            (AnchorTarget::Stage { .. }, _) => {
                bail!("{kind} '{owner}' anchor '{id}' needs a stage root")
            }
            (AnchorTarget::Editor { .. }, _) => {
                bail!("{kind} '{owner}' anchor '{id}' needs an editor root")
            }
            (AnchorTarget::Rune { actor, key, .. }, root) => {
                if key.trim().is_empty() {
                    bail!("{kind} '{owner}' anchor '{id}' needs a non-empty rune element key");
                }
                let on_overlay = rune_surfaces.iter().any(|s| s.id() == actor);
                let on_stage_card = matches!(
                    root,
                    RootPlan::Stage { recipe, .. }
                        if matches!(recipe.element(actor), Some(StageElement::RuneCard { .. }))
                );
                if !on_overlay && !on_stage_card {
                    bail!(
                        "{kind} '{owner}' anchor '{id}' references unknown rune surface or stage rune-card '{actor}'"
                    );
                }
            }
        }
    }
    Ok(())
}

/// `validate` for an overlay's shared anchor plans.
pub(super) fn validate_plans(
    kind: &str,
    owner: &str,
    anchors: &[AnchorPlan],
    placeable: &Placeable<'_>,
) -> Result<()> {
    validate(
        kind,
        owner,
        anchors.iter().map(|anchor| (anchor.id(), anchor.target())),
        placeable,
    )
}

/// Where `target` is on the delivered frame at `time`, from the prepared root
/// or Sequence Diagram: the recipe owns the layout, the overlay only asks for
/// a point.
#[expect(clippy::too_many_arguments)]
pub(super) fn resolve(
    root: &PreparedRoot,
    sequences: &[PreparedSequence],
    rune_surfaces: &[PreparedRuneSurface],
    target: AnchorTarget<'_>,
    size: [u32; 2],
    value: impl Fn(&str, &str, f32) -> f32,
    motion: impl Fn(&str, &str) -> Option<MotionState>,
    time: f64,
) -> Option<Vec2> {
    match (target, root) {
        (AnchorTarget::Point(at), _) => Some(at),
        (AnchorTarget::Participant { sequence, .. } | AnchorTarget::Row { sequence, .. }, _) => {
            sequences
                .iter()
                .find(|candidate| candidate.id() == sequence)?
                .anchor(target, value)
        }
        (AnchorTarget::Stage { element, edge }, PreparedRoot::Stage(stage)) => stage_anchor(
            stage.plan(),
            &|property, default| value(stage.id(), property, default),
            time,
            vec2(size[0] as f32, size[1] as f32),
            element,
            edge,
        ),
        (AnchorTarget::Editor { target, edge }, PreparedRoot::Editor { editor, .. }) => {
            editor.anchor(target, edge, size, motion)
        }
        (AnchorTarget::Rune { actor, key, edge }, root) => {
            if let Some(surface) = rune_surfaces.iter().find(|s| s.id() == actor) {
                let [rx, ry, rw, rh] = surface.presented_rect(time, key, &value)?;
                let bounds = psychopomp::math::shapes::Box2 {
                    min: vec2(rx, ry),
                    max: vec2(rx + rw, ry + rh),
                };
                return Some(edge.on(psychopomp::math::shapes::Shape::Box(bounds)));
            }
            if let PreparedRoot::Stage(stage) = root {
                return stage_rune_anchor(
                    stage.plan(),
                    &|property, default| value(stage.id(), property, default),
                    time,
                    vec2(size[0] as f32, size[1] as f32),
                    actor,
                    key,
                    edge,
                );
            }
            None
        }
        _ => None,
    }
}

/// An anchored overlay's origin at one sample: each weighted anchor resolved,
/// moved by its offset, and blended by `actor`'s `anchor.<id>` weights.
/// `None` when no weighted anchor can be placed.
pub(super) fn pin(
    actor: &str,
    anchors: &[AnchorPlan],
    value: impl Fn(&str, &str, f32) -> f32,
    resolve: impl Fn(AnchorTarget<'_>) -> Option<Vec2>,
) -> Option<Vec2> {
    anchor::blend(anchors.iter().enumerate().filter_map(|(index, plan)| {
        let weight = value(
            actor,
            &anchor::weight_property(plan.id()),
            anchor::initial_weight(index),
        );
        if weight.abs() < 1e-6 {
            return None;
        }
        Some((resolve(plan.target())? + plan.offset(), weight))
    }))
}

/// Whether any of `anchors` moves with the Stage camera.
pub(super) fn on_stage(anchors: &[AnchorPlan]) -> bool {
    anchors.iter().any(|anchor| anchor.target().on_stage())
}

#[cfg(test)]
mod tests {
    use psychopomp::{
        anchor::{AnchorPlan, Edge, weight_property},
        author::PlanBuilder,
        caption::{CaptionActor, CaptionPlan, CaptionSpanPlan},
        math::vec2,
        plan::ScenePlan,
        rolling::{RollingNumberActor, RollingNumberPlan},
        stage::{StageActor, StagePlan},
        text::{TextActor, TextPlan},
        tone::Tone,
    };

    use super::pin;
    use crate::plan_runtime::{preflight, validate_renderer_plan};

    fn stage_recipe() -> StagePlan {
        serde_json::from_value(serde_json::json!({
            "elements": [
                { "kind": "card", "id": "api", "at": [960, 540, 0], "size": [300, 110], "title": "api" },
                { "kind": "card", "id": "client", "at": [420, 540, 0], "size": [300, 110], "title": "client" },
                { "kind": "beam", "id": "link", "from": "client", "to": "api" }
            ]
        }))
        .unwrap()
    }

    /// A stage (or blank) plan with a caption, a Rolling Number, and a text
    /// actor pinned to `anchors`, each writing `property` once.
    fn pinned(stage: bool, anchors: &[AnchorPlan], property: &str) -> ScenePlan {
        let mut scene = PlanBuilder::new("pinned", 2_000_000_000);
        if stage {
            StageActor::declare(&mut scene, "stage", &stage_recipe()).unwrap();
        }
        let mut caption = CaptionPlan::line(
            [0.0, 0.0],
            24.0,
            vec![CaptionSpanPlan::new("pinned", Tone::Plain)],
        );
        let mut number = RollingNumberPlan::new([0.0, 0.0], 24.0, "1");
        let mut text = TextPlan::new("pinned", [0.0, 0.0]);
        for anchor in anchors {
            caption = caption.anchor(anchor.clone());
            number = number.anchor(anchor.clone());
            text = text.anchor(anchor.clone());
        }
        let mut caption = CaptionActor::declare(&mut scene, "caption", &caption).unwrap();
        let mut number = RollingNumberActor::declare(&mut scene, "number", number).unwrap();
        let mut text = TextActor::declare(&mut scene, "text", &text).unwrap();
        for channel in [
            caption.channel(&mut scene, property, 0.0),
            number.channel(&mut scene, property, 0.0),
            text.channel(&mut scene, property, 0.0),
        ] {
            scene.set(&channel, 100_000_000, 1.0);
        }
        scene.finish().unwrap()
    }

    #[test]
    fn pinned_overlays_check_anchors_against_the_root_and_weights_strictly() {
        let anchors = [
            AnchorPlan::stage("api", "api", Edge::Bottom).with_offset([0.0, 30.0]),
            AnchorPlan::point("corner", [100.0, 100.0]),
        ];
        validate_renderer_plan(&pinned(true, &anchors, "anchor.corner")).unwrap();
        validate_renderer_plan(&pinned(true, &anchors, "opacity")).unwrap();
        validate_renderer_plan(&pinned(false, &anchors[1..], "anchor.corner")).unwrap();
        for (stage, anchors, property, expected) in [
            (true, &anchors[..], "anchor.nowhere", "unknown property"),
            (true, &anchors[..], "scale", "unknown property"),
            (false, &anchors[..], "opacity", "needs a stage root"),
            (
                true,
                &[AnchorPlan::stage("beam", "link", Edge::Top)][..],
                "opacity",
                "positioned stage element",
            ),
            (
                true,
                &[AnchorPlan::editor("call", "fetch", Edge::Top)][..],
                "opacity",
                "needs an editor root",
            ),
        ] {
            let error = validate_renderer_plan(&pinned(stage, anchors, property)).unwrap_err();
            assert!(format!("{error:#}").contains(expected), "{error:#}");
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; Stage-pinned overlays land where callouts do and split merged shutter samples"]
    fn stage_pins_follow_the_camera_and_keep_shutter_samples_apart() {
        use psychopomp::{
            callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan},
            math::Vec2,
        };

        use crate::plan_runtime::{PreparedPlan, new_renderer};

        let mut renderer = pollster::block_on(new_renderer("pins")).unwrap();
        let mut build = |with_caption: bool| {
            let mut scene = PlanBuilder::new("pins", 5_000_000_000);
            let mut stage = StageActor::declare(&mut scene, "stage", &stage_recipe()).unwrap();
            stage.to(&mut scene, "camera.z", 0, 300.0, 1.4);
            stage.to(&mut scene, "camera.x", 0, 120.0, 1.4);
            CalloutActor::declare(
                &mut scene,
                "note",
                &CalloutPlan::new(
                    CalloutAnchorPlan::Stage {
                        id: "api".into(),
                        element: "api".into(),
                        edge: Edge::Bottom,
                        side: None,
                    },
                    vec![CaptionSpanPlan::new("note", Tone::Plain)],
                ),
            )
            .unwrap();
            if with_caption {
                CaptionActor::declare(
                    &mut scene,
                    "caption",
                    &CaptionPlan::line(
                        [0.0, 0.0],
                        24.0,
                        vec![CaptionSpanPlan::new("pinned", Tone::Plain)],
                    )
                    .anchor(AnchorPlan::stage("api", "api", Edge::Bottom).with_offset([0.0, 30.0])),
                )
                .unwrap();
            }
            PreparedPlan::prepare(
                scene.finish().unwrap(),
                std::path::Path::new("."),
                &mut renderer,
            )
            .unwrap()
        };
        let callouts_only = build(false);
        let prepared = build(true);
        let size = renderer.size();
        let timeline = &prepared.timeline;
        let anchors = prepared.captions[0].anchors();
        for time in [0.0, 0.4, 0.9, 4.5] {
            let pinned = prepared
                .pin("caption", anchors, Vec2::ZERO, time, timeline, size)
                .unwrap();
            let pose = prepared
                .callout_pose(&prepared.callouts[0], time, timeline, size)
                .unwrap();
            assert_eq!(pinned, pose.anchor + vec2(0.0, 30.0), "at {time}");
        }
        let moving = crate::exposure::exposure(0.5, 1.0 / 60.0, 24);
        let resting = crate::exposure::exposure(4.9, 1.0 / 60.0, 24);
        assert!(
            callouts_only
                .only_callouts_differ(&moving, "stage", size)
                .unwrap()
        );
        assert!(
            !prepared
                .only_callouts_differ(&moving, "stage", size)
                .unwrap()
        );
        assert!(
            prepared
                .only_callouts_differ(&resting, "stage", size)
                .unwrap()
        );
        assert_ne!(
            prepared.overlay_key(0.5, "stage", size).unwrap(),
            prepared.overlay_key(0.505, "stage", size).unwrap()
        );
        assert_eq!(
            prepared.overlay_key(4.9, "stage", size).unwrap(),
            prepared.overlay_key(4.905, "stage", size).unwrap()
        );
    }

    fn sequenced(anchor: AnchorPlan) -> ScenePlan {
        use psychopomp::sequence::{
            SequenceActor, SequenceParticipantPlan, SequencePlan, SequenceRowPlan,
        };
        let mut scene = PlanBuilder::new("sequenced", 3_000_000_000);
        let recipe = SequencePlan {
            origin: [300.0, 200.0],
            width: 1320.0,
            row_height: 80.0,
            slots: None,
            participants: vec![
                SequenceParticipantPlan::new("client", "client", ""),
                SequenceParticipantPlan::new("api", "api", "us-east-1"),
            ],
            rows: vec![
                SequenceRowPlan::message("request", "client", "api", "GET /user", Tone::Request),
                SequenceRowPlan::note("cold", &["api"], "cold start", Tone::Warning),
            ],
        };
        let mut sequence = SequenceActor::declare(&mut scene, "flow", &recipe).unwrap();
        sequence.animate(&mut scene, "x", 0.0, 1_000_000_000, 120.0, 0.5);
        CaptionActor::declare(
            &mut scene,
            "caption",
            &CaptionPlan::line(
                [0.0, 0.0],
                24.0,
                vec![CaptionSpanPlan::new("pinned", Tone::Plain)],
            )
            .anchor(anchor),
        )
        .unwrap();
        scene.finish().unwrap()
    }

    #[test]
    fn sequence_participants_and_rows_are_anchors() {
        for anchor in [
            AnchorPlan::participant("p", "flow", "api", Edge::Bottom),
            AnchorPlan::row("r", "flow", "request", Edge::Top),
            AnchorPlan::row("n", "flow", "cold", Edge::Right),
        ] {
            validate_renderer_plan(&sequenced(anchor)).unwrap();
        }
        for (anchor, expected) in [
            (
                AnchorPlan::participant("p", "nowhere", "api", Edge::Bottom),
                "unknown sequence 'nowhere'",
            ),
            (
                AnchorPlan::participant("p", "flow", "db", Edge::Bottom),
                "unknown participant 'db'",
            ),
            (
                AnchorPlan::row("r", "flow", "reply", Edge::Top),
                "unknown row 'reply'",
            ),
        ] {
            let error = validate_renderer_plan(&sequenced(anchor)).unwrap_err();
            assert!(format!("{error:#}").contains(expected), "{error:#}");
        }
    }

    #[test]
    #[ignore = "requires a headless GPU; anchors on Sequence Diagram headers and rows follow the diagram"]
    fn sequence_anchors_land_on_measured_headers_and_follow_the_diagram() {
        use psychopomp::{
            callout::{CalloutActor, CalloutAnchorPlan, CalloutPlan},
            math::Vec2,
        };

        use crate::plan_runtime::{PreparedPlan, new_renderer};

        let mut renderer = pollster::block_on(new_renderer("sequence-anchors")).unwrap();
        let mut plan = sequenced(
            AnchorPlan::participant("p", "flow", "api", Edge::Bottom).with_offset([0.0, 20.0]),
        );
        let mut scene = PlanBuilder::new("callout", plan.duration_nanos);
        CalloutActor::declare(
            &mut scene,
            "note",
            &CalloutPlan::new(
                CalloutAnchorPlan::Row {
                    id: "request".into(),
                    sequence: "flow".into(),
                    row: "request".into(),
                    edge: Edge::Center,
                    side: None,
                },
                vec![CaptionSpanPlan::new("note", Tone::Plain)],
            ),
        )
        .unwrap();
        plan.actors.extend(scene.finish().unwrap().actors);
        let prepared =
            PreparedPlan::prepare(plan, std::path::Path::new("."), &mut renderer).unwrap();
        let size = renderer.size();
        let timeline = &prepared.timeline;
        let sequence = prepared.sequences[0].plan();
        let anchors = prepared.captions[0].anchors();
        let at = |time: f64| {
            prepared
                .pin("caption", anchors, Vec2::ZERO, time, timeline, size)
                .unwrap()
        };
        let header = sequence.header_box(1, renderer.sequence_widths(sequence).0[1]);
        assert!(header.max.x - header.min.x >= 150.0);
        assert_eq!(at(0.5), vec2(header.center().x, header.max.y + 20.0));
        assert_eq!(at(2.9) - at(0.5), vec2(120.0, 0.0), "rides the diagram's x");
        let request = sequence.row_box(0, 0.0).unwrap().center();
        let pose = prepared
            .callout_pose(&prepared.callouts[0], 0.5, timeline, size)
            .unwrap();
        assert_eq!(pose.anchor, request);
    }

    #[test]
    fn typed_text_decodes_like_hand_built_text() {
        let typed = TextPlan::new("hello", [960.0, 780.0])
            .size(30.0)
            .color([170, 182, 200])
            .anchor(AnchorPlan::point("here", [10.0, 20.0]));
        let mut plan = ScenePlan::new("text", 1_000_000_000);
        plan.actors.push(psychopomp::plan::ActorPlan {
            id: "typed".into(),
            recipe: "text".into(),
            data: serde_json::to_value(&typed).unwrap(),
        });
        plan.actors.push(psychopomp::plan::ActorPlan {
            id: "hand".into(),
            recipe: "text".into(),
            data: serde_json::json!({"text": "hello", "center": [960, 780], "fontSize": 30, "color": [170, 182, 200]}),
        });
        let checked = preflight::Plan::new(plan).unwrap();
        let [typed_text, hand] = &checked.texts[..] else {
            panic!("two texts")
        };
        assert_eq!(typed_text.center, hand.center);
        assert_eq!(typed_text.font_size, hand.font_size);
        assert_eq!(typed_text.color, hand.color);
        assert_eq!(
            typed_text.content.sample_at(0.0).current,
            hand.content.sample_at(0.0).current
        );
        assert!(hand.anchors.is_empty());
        assert_eq!(typed_text.anchors, typed.anchors);
    }

    #[test]
    fn pins_blend_weighted_anchors_with_their_offsets() {
        let anchors = [
            AnchorPlan::point("a", [100.0, 100.0]).with_offset([0.0, 20.0]),
            AnchorPlan::stage("b", "card", Edge::Top).with_offset([0.0, -20.0]),
        ];
        let resolve = |target: psychopomp::anchor::AnchorTarget<'_>| match target {
            psychopomp::anchor::AnchorTarget::Point(at) => Some(at),
            _ => Some(vec2(300.0, 300.0)),
        };
        // Before any weight channel, the first anchor holds the overlay.
        let at_rest = pin("label", &anchors, |_, _, default| default, resolve);
        assert_eq!(at_rest, Some(vec2(100.0, 120.0)));
        let halfway = pin("label", &anchors, |_, _, _| 0.5, resolve);
        assert_eq!(halfway, Some(vec2(200.0, 200.0)));
        let moved = pin(
            "label",
            &anchors,
            |_, property, _| f32::from(property == weight_property("b")),
            resolve,
        );
        assert_eq!(moved, Some(vec2(300.0, 280.0)));
        // An anchor the root cannot place drops out of the blend.
        let unplaced = pin(
            "label",
            &anchors,
            |_, _, _| 0.5,
            |target| match target {
                psychopomp::anchor::AnchorTarget::Point(at) => Some(at),
                _ => None,
            },
        );
        assert_eq!(unplaced, Some(vec2(100.0, 120.0)));
        assert_eq!(pin("label", &anchors, |_, _, _| 0.0, resolve), None);
    }
}
