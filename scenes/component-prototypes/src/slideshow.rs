//! Native audition surface for reusable slide typography and bounded diagrams.
use super::*;

pub fn build_slideshow_deck() -> Result<DeckPlan> {
    let deck = DeckPlan {
        version: 1,
        id: "slideshow-components".into(),
        slides: vec![
            SlidePlan {
                title: "Rich text · emphasis and inline code".into(),
                plan: rich_text()?,
            },
            SlidePlan {
                title: "Lists and quotations · composable blocks".into(),
                plan: lists()?,
            },
            SlidePlan {
                title: "Header entrances · soften, reveal, rise".into(),
                plan: headers()?,
            },
            SlidePlan {
                title: "Type expressions · measured width".into(),
                plan: width_text()?,
            },
            SlidePlan {
                title: "Venn · intersection follows geometry".into(),
                plan: venn()?,
            },
            psychopomp_keyed_grid::build_plain_table_slide()?,
            SlidePlan {
                title: "Composition · existing pieces, same theme".into(),
                plan: super::composition()?,
            },
            SlidePlan {
                title: "Header variations · reflections and stagger".into(),
                plan: header_variations()?,
            },
        ],
    };
    deck.validate()?;
    Ok(deck)
}

fn markdown(
    p: &mut PlanBuilder,
    id: &str,
    markdown: &str,
    origin: [f32; 2],
    width: f32,
    font_size: f32,
) -> Result<ActorHandle> {
    Ok(p.actor(
        id,
        RICH_TEXT,
        RichTextPlan {
            origin,
            width,
            font_size,
            markdown: markdown.into(),
            vertical_mask: None,
            fade_blur: 0.,
        },
    )?)
}
fn prose_fade(p: &mut PlanBuilder, actor: &ActorHandle, property: &str, values: &[f32]) {
    p.step_track(
        actor,
        property,
        values,
        psychopomp::plan::SpringPlan::visual(0.16, 0.),
    );
}

fn rich_text() -> Result<ScenePlan> {
    let mut p = scene(
        "rich-text-showcase",
        &[
            "Establish the headline.",
            "Reveal the supporting paragraph.",
            "Add the code note without moving the paragraph.",
            "Keep only the headline.",
        ],
    );
    markdown(
        &mut p,
        "heading",
        "# Say it with a little **emphasis**.",
        [220., 180.],
        1480.,
        42.,
    )?;
    let body = markdown(
        &mut p,
        "body",
        "A paragraph can be **confident**, *quietly expressive*, or precise enough to name `Result<A, E>` without changing the subject.\n\nMix **bold and *italic***, ~~retired ideas~~, and [a useful reference](https://example.com).",
        [225., 375.],
        1280.,
        34.,
    )?;
    prose_fade(&mut p, &body, "opacity", &[0., 1., 1., 0.]);
    let code = markdown(
        &mut p,
        "code",
        "```ts\ntype Result<A, E> = Success<A> | Failure<E>\n```",
        [225., 700.],
        1300.,
        28.,
    )?;
    prose_fade(&mut p, &code, "opacity", &[0., 0., 1., 0.]);
    Ok(p.finish()?)
}
fn lists() -> Result<ScenePlan> {
    let mut p = scene(
        "lists-quotes-showcase",
        &[
            "A title and a quiet list.",
            "Reveal one item.",
            "Reveal the next item.",
            "Bring in the quote beside the list.",
        ],
    );
    markdown(
        &mut p,
        "heading",
        "# One idea at a time.",
        [220., 150.],
        1480.,
        42.,
    )?;
    let list = markdown(
        &mut p,
        "list",
        "1. **Keep identity.**\n   Retain the words the viewer is following.\n2. **Make room.**\n   Reveal only the part that changes.\n3. **Let it settle.**\n   Leave space to read the result.",
        [225., 355.],
        720.,
        32.,
    )?;
    prose_fade(&mut p, &list, "block.1.opacity", &[0., 1., 1., 1.]);
    prose_fade(&mut p, &list, "block.2.opacity", &[0., 0., 1., 1.]);
    let quote = markdown(
        &mut p,
        "quote",
        "> The words you keep should feel like **the same words**.\n\n*Stability is a visual promise.*",
        [1080., 390.],
        600.,
        35.,
    )?;
    prose_fade(&mut p, &quote, "opacity", &[0., 0., 0., 1.]);
    Ok(p.finish()?)
}
fn headers() -> Result<ScenePlan> {
    let mut p = scene(
        "header-entrances",
        &[
            "Three entrances, not three new clocks.",
            "Bring the headers on.",
            "Hold while supporting content arrives.",
            "Send them back.",
        ],
    );
    for (index, (name, title, y)) in [
        ("soften", "A soft arrival.", 220.),
        ("width", "Make an entrance.", 425.),
        ("rise", "Through the edge.", 630.),
    ]
    .into_iter()
    .enumerate()
    {
        text(
            &mut p,
            &format!("label-{index}"),
            name,
            [230., y + 20.],
            23.,
            MUTED,
        )?;
        let actor = p.actor(
            name,
            RICH_TEXT,
            RichTextPlan {
                origin: [520., y],
                width: 1140.,
                font_size: 62.,
                markdown: format!("**{title}**"),
                vertical_mask: if name == "rise" {
                    Some([y - 5., y + 100., 10.])
                } else {
                    None
                },
                fade_blur: 4.,
            },
        )?;
        track(&mut p, &actor, "opacity", &[0., 1., 1., 0.]);
        match name {
            "width" => track(&mut p, &actor, "reveal", &[0., 1., 1., 0.]),
            "rise" => track(&mut p, &actor, "y", &[y + 95., y, y, y + 95.]),
            _ => {}
        }
    }
    let note = markdown(
        &mut p,
        "note",
        "A held header stays **sharp and still** when something else changes.",
        [520., 850.],
        1140.,
        24.,
    )?;
    prose_fade(&mut p, &note, "opacity", &[0., 0., 1., 0.]);
    Ok(p.finish()?)
}
fn width_text() -> Result<ScenePlan> {
    use psychopomp::code::{StyledSpan, SyntaxStyle};
    let styled = |id: &str, runs: Vec<(&str, SyntaxStyle)>| TextPart {
        id: id.into(),
        text: runs.iter().map(|(text, _)| *text).collect(),
        color: INK,
        spans: runs
            .into_iter()
            .map(|(text, style)| StyledSpan {
                text: text.into(),
                style,
            })
            .collect(),
    };
    use SyntaxStyle::{Keyword, Plain, String as StringStyle, Type};
    let mut p = scene(
        "width-type-showcase",
        &[
            "Start with a stable expression.",
            "Reveal one union member.",
            "Make room for a second member.",
            "Remove only the last member.",
            "Return to the original type.",
        ],
    );
    markdown(
        &mut p,
        "heading",
        "# Types that make room.",
        [220., 160.],
        1480.,
        42.,
    )?;
    p.actor(
        "expression",
        WIDTH_TEXT,
        TypesetPlan {
            origin: [225., 430.],
            font: Font::Mono,
            font_size: 54.,
            parts: vec![
                styled(
                    "prefix",
                    vec![("type ", Keyword), ("Color", Type), (" = ", Plain)],
                ),
                styled("red", vec![("\"red\"", StringStyle)]),
                styled("green", vec![(" | ", Plain), ("\"green\"", StringStyle)]),
                styled("blue", vec![(" | ", Plain), ("\"blue\"", StringStyle)]),
                styled("suffix", vec![(";", Plain)]),
            ],
            visible: words(&["prefix", "red", "suffix"]),
            events: [
                words(&["prefix", "red", "green", "suffix"]),
                words(&["prefix", "red", "green", "blue", "suffix"]),
                words(&["prefix", "red", "green", "suffix"]),
                words(&["prefix", "red", "suffix"]),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, visible)| TextEvent {
                at_nanos: (i + 1) as u64 * BEAT,
                visible,
            })
            .collect(),
        },
    )?;
    markdown(
        &mut p,
        "note",
        "The prefix, first member, and semicolon keep their identity.\n\nOnly the new segment opens; retained ink does not fade.",
        [225., 650.],
        1350.,
        29.,
    )?;
    Ok(p.finish()?)
}

fn header_variations() -> Result<ScenePlan> {
    let mut p = scene(
        "header-variations",
        &[
            "Three more ways through the edge.",
            "Rise, reflect, and stagger.",
            "Hold while the supporting line arrives.",
            "Return without any delayed stragglers.",
        ],
    );
    for (id, label, text, y, split, stagger, duration, reflection) in [
        (
            "mirror",
            "reflection",
            "Through the looking glass.",
            210.,
            HeaderSplit::Line,
            0,
            0.4,
            Some(HeaderReflection {
                opacity: 0.28,
                depth: 64.,
                gap: 5.,
            }),
        ),
        (
            "words",
            "word by word",
            "Every word has its moment.",
            440.,
            HeaderSplit::Words,
            60,
            0.4,
            None,
        ),
        (
            "quick",
            "quick + reflected",
            "A little more momentum.",
            670.,
            HeaderSplit::Words,
            25,
            0.28,
            Some(HeaderReflection {
                opacity: 0.22,
                depth: 42.,
                gap: 4.,
            }),
        ),
    ] {
        super::text(
            &mut p,
            &format!("label-{id}"),
            label,
            [225., y + 25.],
            23.,
            MUTED,
        )?;
        p.actor(
            id,
            HEADER,
            HeaderPlan {
                origin: [540., y],
                text: text.into(),
                font_size: 62.,
                width: 1160.,
                split,
                stagger_millis: stagger,
                duration_seconds: duration,
                reflection,
                visible: false,
                events: vec![
                    HeaderEvent {
                        at_nanos: BEAT,
                        visible: true,
                    },
                    HeaderEvent {
                        at_nanos: 3 * BEAT,
                        visible: false,
                    },
                ],
            },
        )?;
    }
    let note = markdown(
        &mut p,
        "note",
        "One fixed edge. Every word keeps its place.",
        [540., 930.],
        1160.,
        24.,
    )?;
    prose_fade(&mut p, &note, "opacity", &[0., 0., 1., 0.]);
    Ok(p.finish()?)
}
fn venn() -> Result<ScenePlan> {
    let mut p = scene(
        "venn-showcase",
        &[
            "Two separate sets.",
            "Reveal their intersection.",
            "Place A inside B.",
            "Equal extents.",
            "A square containing boundary.",
            "Return to overlapping circles.",
        ],
    );
    markdown(
        &mut p,
        "heading",
        "# The overlap is the point.",
        [220., 145.],
        1480.,
        42.,
    )?;
    let actor = p.actor(
        "sets",
        VENN,
        VennPlan {
            center: [960., 610.],
            left: "A".into(),
            right: "B".into(),
            radius: 175.,
        },
    )?;
    for (name, values) in [
        ("left.x", vec![-230., -105., 0., 0., 0., -105.]),
        ("right.x", vec![230., 105., 0., 0., 0., 105.]),
        ("left.radius", vec![175., 175., 105., 175., 105., 175.]),
        ("right.radius", vec![175., 175., 230., 175., 230., 175.]),
        ("right.roundness", vec![1., 1., 1., 1., 0., 1.]),
    ] {
        p.step_track(
            &actor,
            name,
            &values,
            psychopomp::plan::SpringPlan::visual(0.3, 0.3),
        );
    }
    markdown(
        &mut p,
        "caption",
        "The hatching is the **actual intersection**, even while the boundaries move.",
        [390., 920.],
        1180.,
        24.,
    )?;
    Ok(p.finish()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn type_highlighting_uses_lexical_roles_not_animation_segment_colors() {
        let plan = width_text().unwrap();
        let actor = plan.actors.iter().find(|a| a.id == "expression").unwrap();
        // Require authored styled spans. Token colors must not depend on which
        // union member is entering.
        let mut actual = Vec::new();
        for part in actor.data["parts"].as_array().unwrap() {
            let spans = part["spans"].as_array().expect("authored styled spans");
            assert!(!spans.is_empty());
            for span in spans {
                for c in span["text"].as_str().unwrap().chars() {
                    actual.push((c, span["style"].as_str().unwrap().to_owned()));
                }
            }
        }
        let expected = [
            ("type ", "keyword"),
            ("Color", "type"),
            (" = ", "plain"),
            ("\"red\"", "string"),
            (" | ", "plain"),
            ("\"green\"", "string"),
            (" | ", "plain"),
            ("\"blue\"", "string"),
            (";", "plain"),
        ]
        .into_iter()
        .flat_map(|(text, role)| text.chars().map(move |c| (c, role.to_owned())))
        .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
    #[test]
    fn showcase_is_deterministic_and_uses_existing_continuous_tracks() {
        let a = build_slideshow_deck().unwrap();
        let b = build_slideshow_deck().unwrap();
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(&b).unwrap()
        );
        assert_eq!(a.slides.len(), 8);
        assert_eq!(
            serde_json::to_value(&a.slides[5]).unwrap(),
            serde_json::to_value(psychopomp_keyed_grid::build_plain_table_slide().unwrap())
                .unwrap()
        );
        for slide in a.slides {
            assert!(slide.plan.state_channels.is_empty());
            assert!(!slide.plan.presentation_steps.is_empty());
        }
    }

    #[test]
    fn prose_uses_short_unblurred_fades_while_headers_keep_their_treatment() {
        use psychopomp::plan::TrackEventPlan;
        for plan in [rich_text().unwrap(), lists().unwrap()] {
            for actor in &plan.actors {
                let p: RichTextPlan = serde_json::from_value(actor.data.clone()).unwrap();
                assert_eq!(p.fade_blur, 0.);
            }
            for channel in &plan.continuous_channels {
                for event in &channel.events {
                    if let TrackEventPlan::Spring {
                        response_seconds,
                        damping_ratio,
                        ..
                    } = event
                    {
                        assert!((response_seconds - 0.16).abs() < 0.00001);
                        assert_eq!(*damping_ratio, 1.);
                    }
                }
            }
        }
        let plan = headers().unwrap();
        let rise = plan.actors.iter().find(|a| a.id == "rise").unwrap();
        let p: RichTextPlan = serde_json::from_value(rise.data.clone()).unwrap();
        assert_eq!(p.fade_blur, 4.);
        assert_eq!(p.vertical_mask, Some([625., 730., 10.]));
        for channel in plan
            .continuous_channels
            .iter()
            .filter(|c| c.actor_id == "rise")
        {
            for event in &channel.events {
                if let TrackEventPlan::Spring {
                    response_seconds, ..
                } = event
                {
                    assert!((response_seconds - 0.4).abs() < 0.00001);
                }
            }
        }
    }
}
