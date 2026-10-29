use anyhow::{Result, ensure};
use psychopomp::{
    author::{PlanBuilder, seconds},
    confetti::{ConfettiActor, ConfettiPlan},
    effects::combustion,
    face::Face,
    sfx,
    stage::{BoltEnd, StageActor, StageElement, StagePlan, StagePost},
    tone::Tone,
};
use psychopomp_media::{Media, Voice};
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let media = Media::open(&root)?;
    // Kit's available instant clone; the professional clone is awaiting retraining.
    let kit = Voice::eleven("dNdjwQil3LjvIangzxuD")
        .v4()
        .stability(0.15)
        .similarity(0.65);
    let low = media.say(
        "whisper",
        &kit,
        "[extremely quiet intimate ASMR whisper, disappointed] This week's a three.",
    )?;
    let loud = media.say("reveal", &kit,
        "[sudden explosive screaming, ecstatic, completely unhinged, fast] JUST FUCKING KIDDING! TEN! TEN! TEN! [triumphant yelling] A HUNDRED WEEKS! TEN OUT OF TEN! AGAIN!")?;
    let boom = media.sfx("impact", "A gigantic cinematic bass drop impact, explosive distorted sub bass punch, metallic crunch and short electric crackling tail. Immediate attack. No speech.", seconds(1.5))?;
    let horn = media.sfx("horn", "An absurd celebratory stadium airhorn triple blast with crowd cheering, punchy distorted rave horn, immediate attack, wild victory sound. No words.", seconds(2.0))?;
    media.finish()?;
    let reveal = seconds(0.3) + low.duration() + seconds(0.32);
    let duration = reveal + loud.duration() + seconds(0.9);
    ensure!(
        duration <= seconds(15.0),
        "narration exceeds 15 seconds: {}",
        duration as f64 / 1e9
    );
    let mut sc = PlanBuilder::new("weekly-ten", duration);
    low.place(&mut sc, seconds(0.3));
    let voice = loud.place(&mut sc, reveal);
    let reveal = voice.at("just");
    let ten = voice.at("ten");
    let hundred = voice.at("hundred");
    let again = voice.at("again");
    let mut elements = vec![
        StageElement::label(
            "title",
            [960.0, 150.0, 0.0],
            54.0,
            &[("Kit's Weekly Update", Tone::Plain)],
        )
        .face(Face::SansBold),
        StageElement::label(
            "three",
            [960.0, 520.0, 80.0],
            310.0,
            &[("3/10", Tone::Muted)],
        )
        .face(Face::Light),
        StageElement::label(
            "jk",
            [960.0, 510.0, 120.0],
            145.0,
            &[("JUST FUCKING", Tone::Plain)],
        )
        .face(Face::Shout),
        StageElement::label(
            "kidding",
            [960.0, 690.0, 120.0],
            170.0,
            &[("KIDDING!", Tone::Warning)],
        )
        .face(Face::Shout),
        StageElement::orb("core", [960.0, 540.0, -100.0], 230.0)
            .points(1100)
            .tone(Tone::Warning),
        StageElement::label(
            "ten",
            [960.0, 500.0, 150.0],
            310.0,
            &[("10/10", Tone::Warning)],
        )
        .face(Face::Shout),
        StageElement::label(
            "again",
            [960.0, 790.0, 150.0],
            155.0,
            &[("AGAIN.", Tone::Plain)],
        )
        .face(Face::Shout),
        StageElement::label(
            "streak",
            [960.0, 950.0, 80.0],
            42.0,
            &[("100 WEEKS. ALL TENS.", Tone::Success)],
        )
        .face(Face::SansBold),
    ];
    for (i, x) in [100.0, 1820.0].into_iter().enumerate() {
        elements.push(StageElement::Bolt {
            id: format!("bolt{i}"),
            from: BoltEnd::Point([x, 100.0, 0.0]),
            to: BoltEnd::Element("core".into()),
            strikes: 3,
            branching: 0.8,
            tone: Tone::Warning,
        });
    }
    for i in 0..100 {
        elements.push(
            StageElement::label(
                &format!("week{i}"),
                [
                    300.0 + (i % 10) as f32 * 146.0,
                    300.0 + (i / 10) as f32 * 52.0,
                    -220.0,
                ],
                34.0,
                &[("10", Tone::Success)],
            )
            .face(Face::Shout),
        );
    }
    let mut s = StageActor::declare(
        &mut sc,
        "stage",
        &StagePlan {
            post: StagePost::RESTRAINED,
            elements,
        },
    )?;
    for id in ["jk", "kidding", "ten", "again", "streak", "core"] {
        s.channel(&mut sc, &format!("{id}.opacity"), 0.0);
    }
    for i in 0..100 {
        s.channel(&mut sc, &format!("week{i}.opacity"), 0.0);
    }
    s.channel(&mut sc, "post.bloom", 0.2);
    s.to(&mut sc, "three.scale", 0, 0.9, 2.0);
    s.to(&mut sc, "three.opacity", reveal, 0.0, 0.08);
    for id in ["jk", "kidding"] {
        s.set(&mut sc, &format!("{id}.opacity"), reveal, 1.0);
        s.channel(&mut sc, &format!("{id}.scale"), 1.6);
        s.bounce(&mut sc, &format!("{id}.scale"), reveal, 1.0, 0.28, 0.2);
        s.to(&mut sc, &format!("{id}.opacity"), ten, 0.0, 0.06);
    }
    s.jolt(&mut sc, reveal, [1.0, 0.2], 0.85);
    sfx::GLITCH.play(&mut sc, "scratch", reveal, -9.0);
    s.set(&mut sc, "core.opacity", ten, 1.0);
    s.clock_for(&mut sc, "core.burst", ten, combustion::DURATION);
    s.to(&mut sc, "core.opacity", ten + seconds(0.4), 0.0, 0.35);
    s.set(&mut sc, "ten.opacity", ten, 1.0);
    s.channel(&mut sc, "ten.scale", 1.9);
    s.bounce(&mut sc, "ten.scale", ten, 1.0, 0.35, 0.2);
    boom.play(&mut sc, ten, -5.0);
    horn.play(&mut sc, ten + seconds(0.15), -15.0);
    for (i, at) in voice.at_every("ten").into_iter().enumerate() {
        s.jolt(
            &mut sc,
            at,
            [if i % 2 == 0 { 1.0 } else { -1.0 }, 0.3],
            0.75,
        );
        s.hit(&mut sc, "post.chroma", at, 0.25, 0.0);
        s.hit(&mut sc, "post.zoom", at, 0.12, 0.0);
        s.zap(&mut sc, &format!("bolt{}", i % 2), at);
        sfx::IMPACT.play(&mut sc, format!("ten-hit{i}"), at, -10.0);
    }
    for i in 0..100 {
        let at = hundred + seconds(i as f64 * 0.008);
        s.fade_in(&mut sc, &format!("week{i}"), at, 0.22, 0.12);
    }
    s.fade_in(&mut sc, "streak", hundred, 1.0, 0.2);
    s.set(&mut sc, "again.opacity", again, 1.0);
    s.channel(&mut sc, "again.scale", 1.8);
    s.bounce(&mut sc, "again.scale", again, 1.0, 0.3, 0.2);
    boom.play(&mut sc, again, -9.0);
    for (i, x) in [200.0, 1720.0].into_iter().enumerate() {
        let mut confetti = ConfettiActor::declare(
            &mut sc,
            format!("party{i}"),
            &ConfettiPlan::new([x, 1050.0])
                .count(180)
                .seed(i as u32)
                .speed(1700.0),
        )?;
        confetti.burst(&mut sc, ten);
        confetti.burst(&mut sc, again);
    }
    let mut plan = sc.finish()?;
    for clip in &mut plan.media {
        clip.path = root.join(&clip.path).canonicalize()?;
    }
    fs::create_dir_all("output/weekly-ten")?;
    fs::write(
        "output/weekly-ten/plan.json",
        serde_json::to_string_pretty(&plan)?,
    )?;
    eprintln!(
        "weekly update: {:.2}s; ten {:.2}, hundred {:.2}, again {:.2}",
        duration as f64 / 1e9,
        ten as f64 / 1e9,
        hundred as f64 / 1e9,
        again as f64 / 1e9
    );
    Ok(())
}
