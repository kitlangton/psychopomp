//! Groups showroom: transform rigs and matte masks. A jointed arm swings as
//! three nested groups, its label turning with the hand; a card skews and
//! stretches as one group; headline words rise from behind invisible slot
//! edges; footage plays inside display type; and gradients fill a polygon
//! star and an SVG path, sliding in from behind their own edges.
use anyhow::Result;
use psychopomp::{
    author::{PlanBuilder, SECOND, seconds},
    footage,
    math::easing::Ease,
    plan::SpringPlan,
    stage::{StageActor, StagePlan},
};

fn gradient(from: [u8; 4], to: [u8; 4]) -> serde_json::Value {
    serde_json::json!({ "kind": "linear", "angleDeg": 90.0, "from": from, "to": to })
}

fn stage() -> Result<StagePlan> {
    let bar = |id: &str, at: [f32; 2], size: [f32; 2]| {
        serde_json::json!({ "kind": "shape", "id": id, "at": [at[0], at[1], 0],
            "shape": { "rect": size }, "corner": size[0] * 0.5, "stroke": null,
            "fill": gradient([255, 196, 92, 255], [242, 92, 84, 255]) })
    };
    let joint = |id: &str, at: [f32; 2]| {
        serde_json::json!({ "kind": "shape", "id": id, "at": [at[0], at[1], 0],
            "shape": { "circle": 15 }, "fill": "raised", "width": 2 })
    };
    let star = (0..10)
        .map(|k| {
            let angle = k as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
            let radius = if k % 2 == 0 { 130.0 } else { 56.0 };
            [radius * angle.cos(), radius * angle.sin()]
        })
        .collect::<Vec<_>>();
    Ok(serde_json::from_value(serde_json::json!({
        "post": { "bloom": 0.2, "grain": 0.012, "vignette": 0.25, "backdrop": 0.12 },
        "elements": [
            { "kind": "group", "id": "rig", "at": [470, 560, 0], "children": [
                { "kind": "shape", "id": "rig-base", "at": [0, 26, 0], "shape": { "rect": [260, 36] },
                  "corner": 14, "fill": "raised" },
                { "kind": "group", "id": "arm", "at": [0, 0, 0], "children": [
                    bar("arm-bar", [0.0, -110.0], [38.0, 250.0]),
                    { "kind": "group", "id": "hand", "at": [0, -220, 0], "children": [
                        bar("hand-bar", [0.0, -80.0], [30.0, 190.0]),
                        joint("hand-tip", [0.0, -165.0]),
                        { "kind": "label", "id": "hand-label", "at": [0, -215, 0], "size": 24,
                          "spans": [{ "text": "nested", "tone": "muted" }] }
                    ] },
                    joint("elbow", [0.0, -220.0])
                ] },
                joint("shoulder", [0.0, 0.0])
            ] },
            { "kind": "group", "id": "slab", "at": [470, 860, 0], "pivot": [0, 70], "children": [
                { "kind": "card", "id": "slab-card", "at": [0, 0, 0], "size": [360, 128],
                  "title": "skew + stretch", "status": [{ "text": "one group, warped" }] }
            ] },
            { "kind": "group", "id": "slot-a", "at": [1340, 200, 0],
              "clip": { "kind": "rect", "size": [1000, 120] }, "children": [
                { "kind": "type", "id": "line-a", "at": [0, 0, 0],
                  "spec": { "text": "MOTION WITHOUT", "face": "sans-bold", "size": 96, "tracking": -0.01, "align": "center" } }
            ] },
            { "kind": "group", "id": "slot-b", "at": [1340, 318, 0],
              "clip": { "kind": "rect", "size": [1000, 120] }, "children": [
                { "kind": "type", "id": "line-b", "at": [0, 0, 0],
                  "spec": { "text": "BOXES", "face": "sans-bold", "size": 96, "tracking": -0.01, "align": "center",
                            "paint": gradient([255, 196, 92, 255], [242, 92, 84, 255]) } }
            ] },
            { "kind": "group", "id": "matte", "at": [1340, 590, 0],
              "clip": { "kind": "type", "text": "DEPTH", "face": "sans-bold", "size": 280, "weight": 900, "tracking": 0.02 },
              "children": [
                { "kind": "footage", "id": "fractal", "at": [0, 0, 0], "size": [1000, 563],
                  "clip": { "media": "fractal-media", "repeat": "loop" } }
            ] },
            { "kind": "group", "id": "star", "at": [1140, 900, 0],
              "clip": { "kind": "polygon", "points": star }, "children": [
                { "kind": "shape", "id": "star-fill", "at": [0, 0, 0], "shape": { "rect": [280, 280] },
                  "stroke": null, "fill": gradient([120, 200, 255, 255], [150, 92, 255, 255]) }
            ] },
            { "kind": "group", "id": "blob", "at": [1560, 900, 0],
              "clip": { "kind": "path",
                        "d": "M 0 -40 C 40 -150 190 -90 130 10 C 100 60 40 100 0 130 C -40 100 -100 60 -130 10 C -190 -90 -40 -150 0 -40 Z" },
              "children": [
                { "kind": "shape", "id": "blob-fill", "at": [0, 0, 0], "shape": { "rect": [320, 300] },
                  "stroke": null, "fill": gradient([255, 120, 180, 255], [255, 196, 92, 255]) }
            ] }
        ]
    }))?)
}

fn main() -> Result<()> {
    let plan = stage()?;
    let mut scene = PlanBuilder::new("groups", 8 * SECOND);
    let media = footage::media(
        "fractal-media",
        "../assets/footage/gradient.mp4",
        0,
        scene.duration_nanos(),
    );
    scene.media(media);
    let mut s = StageActor::declare(&mut scene, "stage", &plan)?;
    let sc = &mut scene;

    // The rig hinges up from its base; the arm and hand swing against each
    // other, so the hand's label rides two composed turns.
    s.hinge_in(
        sc,
        "rig",
        seconds(0.1),
        [0.0, 60.0],
        -12.0,
        SpringPlan::PANEL,
    );
    for (id, at, swings) in [
        ("arm", 0.4, [-0.55, 0.42, -0.2, 0.0]),
        ("hand", 0.55, [0.9, -0.7, 0.45, 0.0]),
    ] {
        s.channel(sc, &format!("{id}.rotation"), 0.0);
        for (k, to) in swings.into_iter().enumerate() {
            let when = seconds(at + 1.5 * k as f64);
            s.ease(
                sc,
                &format!("{id}.rotation"),
                when,
                to,
                1.3,
                Ease::Smootherstep,
            );
        }
    }

    // The card skews and stretches as one group about its bottom edge.
    s.channel(sc, "slab.opacity", 0.0);
    s.ease(sc, "slab.opacity", seconds(0.3), 1.0, 0.3, Ease::GLIDE);
    s.channel(sc, "slab.skew", 0.0);
    s.channel(sc, "slab.scale-x", 1.0);
    for (k, (skew, stretch)) in [(0.32, 1.2), (-0.32, 0.9), (0.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        let when = seconds(1.0 + 1.8 * k as f64);
        s.ease(sc, "slab.skew", when, skew, 1.4, Ease::Smootherstep);
        s.ease(sc, "slab.scale-x", when, stretch, 1.4, Ease::Smootherstep);
    }

    // Headline lines rise from behind their slots' invisible bottom edges,
    // at full opacity: the edge does the reveal.
    for (id, at) in [("line-a", 0.5), ("line-b", 0.62)] {
        let y = s.channel(sc, &format!("{id}.y"), 130.0);
        sc.spring_with(&y, seconds(at), 0.0, SpringPlan::ENTER);
    }

    // Footage plays inside the display type, drifting under the glyphs.
    s.channel(sc, "matte.opacity", 0.0);
    s.ease(sc, "matte.opacity", seconds(1.2), 1.0, 0.3, Ease::GLIDE);
    let scale = s.channel(sc, "matte.scale", 0.92);
    sc.spring_with(&scale, seconds(1.2), 1.0, SpringPlan::PANEL);
    s.channel(sc, "fractal.x", -120.0);
    s.ease(
        sc,
        "fractal.x",
        seconds(1.2),
        120.0,
        6.5,
        Ease::Smootherstep,
    );

    // Gradients slide up into the star and in from the side of the path,
    // from behind the mattes' own edges; the star then turns as a whole.
    let fill = s.channel(sc, "star-fill.y", 280.0);
    sc.spring_with(&fill, seconds(2.0), 0.0, SpringPlan::MOVE);
    let fill = s.channel(sc, "blob-fill.x", -330.0);
    sc.spring_with(&fill, seconds(2.3), 0.0, SpringPlan::MOVE);
    s.channel(sc, "star.rotation", 0.0);
    s.ease(
        sc,
        "star.rotation",
        seconds(3.6),
        std::f32::consts::TAU / 5.0,
        1.6,
        Ease::Smootherstep,
    );
    s.channel(sc, "blob.scale", 1.0);
    s.ease(
        sc,
        "blob.scale",
        seconds(4.4),
        1.12,
        0.6,
        Ease::Smootherstep,
    );
    s.ease(sc, "blob.scale", seconds(5.0), 1.0, 0.8, Ease::Smootherstep);

    scene.finish()?.write_or_print(Some(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/groups.json".into()),
    ))?;
    Ok(())
}
