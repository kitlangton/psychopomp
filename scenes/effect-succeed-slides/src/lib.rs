//! Presentation-sized adaptation of Effect Institute's basics/effect-succeed.
//! The two stable declaration lines are a display reflow, not a changing slot.
use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, SECOND},
    code::{StyledSpan, SyntaxStyle},
    editor::{
        self, EDITOR_RECIPE, EditorInlineRevealPlan, EditorLinePlan, EditorPartPlan,
        EditorRecipePlan, semantic_line, semantic_range,
    },
    plan::ScenePlan,
};

pub fn build_plan() -> Result<ScenePlan> {
    let mut scene = PlanBuilder::new("effect-succeed", 14 * SECOND);
    let editor = scene.actor("editor", EDITOR_RECIPE, editor_recipe())?;
    let constructor = scene.continuous(&editor, "constructor", 0.0);
    let value = scene.continuous(&editor, "value", 0.0);
    let annotation = scene.continuous(&editor, "annotation", 0.0);
    let error = scene.continuous(&editor, "error", 0.0);
    let requirements = scene.continuous(&editor, "requirements", 0.0);

    scene.presentation_step("initial", "Start with a value", 0, 0);
    for (id, title, at, channels, target) in [
        (
            "constructor",
            "Describe a successful computation",
            1,
            vec![&constructor],
            1.0,
        ),
        (
            "value",
            "Give it the string \"pismire\"",
            3,
            vec![&value],
            1.0,
        ),
        (
            "annotation",
            "It succeeds with a string",
            5,
            vec![&annotation],
            1.0,
        ),
        ("error", "It can never fail", 7, vec![&error], 1.0),
        (
            "requirements",
            "It has no requirements",
            9,
            vec![&requirements],
            1.0,
        ),
        (
            "elide",
            "Leave out the trailing nevers",
            11,
            vec![&error, &requirements],
            0.0,
        ),
    ] {
        let start = at * SECOND;
        let hold = start + 1_500_000_000;
        for channel in channels {
            // Published inline motion: Motion's 0.4-second visual duration
            // (a 0.48 s response), no bounce.
            scene.spring(channel, start, target, 0.4 * 1.2, 0.0);
        }
        scene.cue(id, start, hold);
        scene.presentation_step(id, title, start, hold);
    }
    Ok(scene.finish()?)
}

fn editor_recipe() -> EditorRecipePlan {
    use SyntaxStyle::{Keyword, Plain, Rgb, String as StringStyle, Type};

    let lines = vec![
        line(
            "import",
            vec![
                part("keyword", "import", Keyword),
                part("effect", " { Effect } ", Plain),
                part("from", "from", Keyword),
                part("module", " \"effect\"", StringStyle),
            ],
        ),
        line("blank", vec![]),
        line(
            "magicWord",
            vec![
                part("const", "const", Keyword),
                part("name", " magicWord", Plain),
                tokens(
                    "type-open",
                    &[
                        (": ", Plain),
                        ("Effect", Plain),
                        (".", Plain),
                        ("Effect", Type),
                        ("<", Plain),
                        ("string", Keyword),
                    ],
                ),
                tokens("error", &[(", ", Plain), ("never", Keyword)]),
                tokens("requirements", &[(", ", Plain), ("never", Keyword)]),
                part("type-close", ">", Plain),
                part("equals", " =", Plain),
            ],
        ),
        line(
            "value",
            vec![
                part("indent", "  ", Plain),
                tokens(
                    "constructor",
                    &[
                        ("Effect", Plain),
                        (".", Plain),
                        ("succeed", Rgb(250, 204, 130)),
                        ("(", Plain),
                    ],
                ),
                part("placeholder", "...", Plain),
                part("pismire", "\"pismire\"", StringStyle),
                part("close", ")", Plain),
            ],
        ),
    ];
    let ids = lines.iter().map(|line| line.id.clone()).collect::<Vec<_>>();
    EditorRecipePlan {
        file_name: "effect-succeed.ts".into(),
        lines,
        initial_line_ids: ids.clone(),
        final_line_ids: ids,
        snapshots: Vec::new(),
        line_height: 44.0,
        entering_offset_x: 0.0,
        focus_line_id: "magicWord".into(),
        focus_height: 88.0,
        inline_reveal: Some(reveal("value", "constructor", "constructor", false)),
        additional_inline_reveals: vec![
            reveal("value", "close", "constructor", false),
            reveal("value", "placeholder", "value", true),
            reveal("value", "pismire", "value", false),
            reveal("magicWord", "type-open", "annotation", false),
            reveal("magicWord", "type-close", "annotation", false),
            reveal("magicWord", "error", "error", false),
            reveal("magicWord", "requirements", "requirements", false),
        ],
    }
}

fn part(id: &str, text: &str, style: SyntaxStyle) -> EditorPartPlan {
    tokens(id, &[(text, style)])
}

fn tokens(id: &str, spans: &[(&str, SyntaxStyle)]) -> EditorPartPlan {
    let spans = spans
        .iter()
        .map(|(text, style)| StyledSpan::new(*text, *style));
    editor::part(id, spans.collect())
}

/// Every part is also a Semantic Target of its own name.
fn line(id: &str, parts: Vec<EditorPartPlan>) -> EditorLinePlan {
    let ranges = parts
        .iter()
        .map(|part| semantic_range(&part.id, &part.id, &part.id))
        .collect();
    semantic_line(id, parts, ranges)
}

fn reveal(line: &str, range: &str, channel: &str, reversed: bool) -> EditorInlineRevealPlan {
    EditorInlineRevealPlan {
        line_id: line.into(),
        range_id: range.into(),
        channel: Some(channel.into()),
        reversed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psychopomp::{
        editor::inspect_steps,
        plan::{ScalarPlan, compile_channels},
        timeline::PropertyId,
    };
    use serde_json::json;

    #[test]
    fn demo_preserves_lines_and_keeps_common_text_outside_changing_ranges() {
        let recipe = editor_recipe();
        assert_eq!(recipe.initial_line_ids, recipe.final_line_ids);
        let editor = recipe.compile().unwrap();
        assert_eq!(editor.sample_lines(|_, _| 1.0).len(), 4);
        for reveal in recipe
            .inline_reveal
            .iter()
            .chain(&recipe.additional_inline_reveals)
        {
            assert!(!["const", "name", "equals", "indent"].contains(&reveal.range_id.as_str()));
        }
    }

    #[test]
    fn punctuation_and_namespace_are_not_colored_as_a_function_or_type() {
        let recipe = editor_recipe();
        let constructor = &recipe.lines[3].parts[1];
        assert_eq!(constructor.id, "constructor");
        assert_eq!(
            constructor
                .spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<Vec<_>>(),
            ["Effect", ".", "succeed", "("]
        );
        assert!(matches!(constructor.spans[0].style, SyntaxStyle::Plain));
        assert!(matches!(constructor.spans[2].style, SyntaxStyle::Rgb(..)));
        assert!(matches!(constructor.spans[3].style, SyntaxStyle::Plain));
    }

    #[test]
    fn steps_match_the_published_slot_reveals_in_any_sampling_order() {
        let plan = build_plan().unwrap();
        assert_eq!(plan.presentation_steps.len(), 7);
        let channels = [
            "constructor",
            "value",
            "annotation",
            "error",
            "requirements",
        ];
        let expected = [
            [0., 0., 0., 0., 0.],
            [1., 0., 0., 0., 0.],
            [1., 1., 0., 0., 0.],
            [1., 1., 1., 0., 0.],
            [1., 1., 1., 1., 0.],
            [1., 1., 1., 1., 1.],
            [1., 1., 1., 0., 0.],
        ];
        let timeline = compile_channels(
            plan.continuous_channels
                .iter()
                .map(|channel| (channel, PropertyId::new(&channel.id))),
            plan.duration_nanos,
            |scalar| match scalar {
                ScalarPlan::Literal(value) => Ok(*value),
                _ => anyhow::bail!("expected literal scalar"),
            },
        )
        .unwrap();
        assert_eq!(plan.continuous_channels.len(), channels.len());
        for (index, channel) in plan.continuous_channels.iter().enumerate() {
            assert_eq!(channel.property, channels[index]);
            let id = PropertyId::new(&channel.id);
            for event in &channel.events {
                let spring = event.spring_plan().expect("expected spring");
                assert_eq!(spring.response_seconds, 0.4 * 1.2);
                assert_eq!(spring.damping_ratio, 1.0);
            }
            for step_index in [6, 0, 4, 1, 5, 3, 2, 6] {
                let state = timeline
                    .sample_at(
                        &id,
                        plan.presentation_steps[step_index].hold_nanos as f64 / SECOND as f64,
                    )
                    .unwrap();
                assert_eq!(state.position, expected[step_index][index]);
                assert_eq!(state.velocity, 0.0);
            }
        }
    }

    #[test]
    fn held_text_matches_the_source_slots_after_display_reflow() {
        let report = serde_json::to_value(inspect_steps(&build_plan().unwrap()).unwrap()).unwrap();
        assert_eq!(report["warnings"], json!([]));
        let steps = report["steps"].as_array().unwrap();
        let cases: [(&str, [&[&str]; 2]); 7] = [
            (
                "const magicWord =\n  ...",
                [&["const", "name", "equals"], &["indent", "placeholder"]],
            ),
            (
                "const magicWord =\n  «Effect.succeed(»...«)»",
                [&[], &["constructor", "close"]],
            ),
            (
                "const magicWord =\n  Effect.succeed(«\"pismire\"»)",
                [&[], &["placeholder", "pismire"]],
            ),
            (
                "const magicWord«: Effect.Effect<string»«>» =\n  Effect.succeed(\"pismire\")",
                [&["type-open", "type-close"], &[]],
            ),
            (
                "const magicWord: Effect.Effect<string«, never»> =\n  Effect.succeed(\"pismire\")",
                [&["error"], &[]],
            ),
            (
                "const magicWord: Effect.Effect<string, never«, never»> =\n  Effect.succeed(\"pismire\")",
                [&["requirements"], &[]],
            ),
            (
                "const magicWord: Effect.Effect<string> =\n  Effect.succeed(\"pismire\")",
                [&["error", "requirements"], &[]],
            ),
        ];
        assert_eq!(steps.len(), cases.len());
        for (index, (step, (delta, changed))) in steps.iter().zip(cases).enumerate() {
            let editors = step["editors"].as_array().unwrap();
            assert_eq!(editors.len(), 1);
            assert_eq!(editors[0]["actorId"], "editor");
            let lines = editors[0]["lines"].as_array().unwrap();
            assert_eq!(
                lines
                    .iter()
                    .map(|line| line["id"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["import", "blank", "magicWord", "value"]
            );
            let text = |field: &str| {
                lines[2..]
                    .iter()
                    .map(|line| line[field].as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            assert_eq!(text("after"), delta.replace(['«', '»'], ""));
            assert_eq!(text("delta"), delta);
            if index > 0 {
                assert_eq!(text("before"), cases[index - 1].0.replace(['«', '»'], ""));
                assert_eq!(lines[0]["changedPartIds"], json!([]));
            }
            assert!(lines.iter().all(|line| line["moved"] == false));
            for (line, parts) in lines[2..].iter().zip(changed) {
                assert_eq!(line["changedPartIds"], json!(parts));
            }
        }
        assert_eq!(
            steps[2]["editors"][0]["lines"][3]["beforeDelta"],
            "  Effect.succeed(«...»)",
        );
        assert_eq!(
            steps[6]["editors"][0]["lines"][2]["beforeDelta"],
            "const magicWord: Effect.Effect<string«, never»«, never»> =",
        );
    }
}
