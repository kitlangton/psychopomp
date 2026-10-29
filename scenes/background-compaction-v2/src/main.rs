use std::{collections::HashMap, env, fmt::Write as _, fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use psychopomp::{
    plan::{ScalarPlan, ScenePlan, compile_channels},
    timeline::PropertyId,
};

const FPS: f64 = 60.0;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("trace") {
        let plan = args.get(1).context("usage: trace <plan.json> <out-dir> [channel-prefix…]")?;
        let out = args.get(2).context("usage: trace <plan.json> <out-dir> [channel-prefix…]")?;
        return trace(&PathBuf::from(plan), &PathBuf::from(out), &args[3..]);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = args
        .first()
        .map_or_else(|| root.join("background-compaction-v2.json"), PathBuf::from);
    psychopomp_background_compaction_v2::build_plan(&root.join("narration"))?.write_or_print(Some(&output))?;
    eprintln!("Wrote {}", output.display());
    Ok(())
}

/// Sample every literal channel of a plan once per 60 fps frame and flag
/// motion that breaks Kit's rules: visible pops (opacity or scale changing
/// most of its range in one frame), teleports, and velocity steps on
/// position or size channels while the element is visible. Writes
/// `flags.txt`, and `trace.csv` for channels matching the prefixes.
fn trace(plan_path: &PathBuf, out: &PathBuf, prefixes: &[String]) -> Result<()> {
    let plan = ScenePlan::from_json(&fs::read_to_string(plan_path)?)?;
    let frames = (plan.duration_nanos as f64 / 1e9 * FPS).floor() as usize;
    let mut series: HashMap<String, Vec<f32>> = HashMap::new();
    for channel in &plan.continuous_channels {
        let literal = |scalar: &ScalarPlan| match scalar {
            ScalarPlan::Literal(value) => Ok(*value),
            ScalarPlan::Target(_) => bail!("target"),
        };
        let Ok(timeline) = compile_channels(
            [(channel, PropertyId::new(channel.id.clone()))],
            plan.duration_nanos,
            literal,
        ) else {
            continue;
        };
        let property = PropertyId::new(channel.id.clone());
        let values = (0..frames)
            .map(|frame| {
                timeline
                    .sample_at(&property, frame as f64 / FPS)
                    .map_or(f32::NAN, |state| state.position)
            })
            .collect();
        series.insert(format!("{}/{}", channel.actor_id, channel.property), values);
    }

    // The element a channel belongs to: `actor/element.property` or `actor/property`.
    let split = |key: &str| -> (String, String) {
        match key.rsplit_once('.') {
            Some((element, property)) => (element.to_owned(), property.to_owned()),
            None => {
                let (actor, property) = key.split_once('/').unwrap_or(("", key));
                (format!("{actor}/"), property.to_owned())
            }
        }
    };
    let opacity_of = |element: &str| -> Option<&Vec<f32>> {
        series
            .get(&format!("{element}.opacity"))
            .or_else(|| series.get(&format!("{element}opacity")))
    };

    let mut flags = Vec::new();
    let mut keys: Vec<&String> = series.keys().collect();
    keys.sort();
    for key in &keys {
        let (element, property) = split(key);
        let values = &series[*key];
        let opacity = opacity_of(&element);
        let typed = series.get(&format!("{element}.typed"));
        let visible = |frame: usize| opacity.map_or(1.0, |o| o[frame]);
        for frame in 2..frames {
            let (a, b, c) = (values[frame - 2], values[frame - 1], values[frame]);
            if !(a.is_finite() && b.is_finite() && c.is_finite()) {
                continue;
            }
            let shown = visible(frame).max(visible(frame - 1));
            let at = frame as f64 / FPS;
            let mut flag = |kind: &str, detail: String| {
                flags.push(format!("{at:7.3}s  f{frame:<5} {kind:<14} {key}  {detail}"));
            };
            match property.as_str() {
                "opacity" => {
                    let hidden_text = typed.is_some_and(|typed| typed[frame] < 0.02);
                    if (c - b).abs() > 0.34 && c.max(b) > 0.34 && !hidden_text {
                        flag("pop", format!("{b:.2} → {c:.2}"));
                    }
                }
                "scale" if shown > 0.1 && (c - b).abs() > 0.04 => {
                    flag("scale-pop", format!("{b:.3} → {c:.3} at opacity {shown:.2}"));
                }
                "x" | "y" | "width" | "height" if shown > 0.1 => {
                    let (v0, v1) = (b - a, c - b);
                    if v1.abs() > 90.0 {
                        flag("teleport", format!("{v1:.1} px in one frame at opacity {shown:.2}"));
                    } else if (v1 - v0).abs() > 6.0 && (v1 - v0).abs() > 0.45 * v0.abs().max(v1.abs()) {
                        let kind = if v0.abs() < 0.5 { "launch" } else if v1.abs() < 0.5 { "stop" } else { "velocity-step" };
                        flag(kind, format!("{v0:.1} → {v1:.1} px/frame at opacity {shown:.2}"));
                    }
                }
                _ => {}
            }
        }
    }
    fs::create_dir_all(out)?;
    let mut summary = String::new();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in &flags {
        let kind = line.split_whitespace().nth(2).unwrap_or("");
        *counts.entry(kind).or_default() += 1;
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort();
    writeln!(summary, "{} channels, {frames} frames at {FPS} fps", series.len())?;
    for (kind, count) in counts {
        writeln!(summary, "{kind}: {count}")?;
    }
    fs::write(out.join("flags.txt"), format!("{summary}\n{}\n", flags.join("\n")))?;
    print!("{summary}");

    if !prefixes.is_empty() {
        let chosen: Vec<&&String> = keys
            .iter()
            .filter(|key| prefixes.iter().any(|prefix| key.contains(prefix.as_str())))
            .collect();
        let mut csv = format!("t,{}\n", chosen.iter().map(|key| key.as_str()).collect::<Vec<_>>().join(","));
        for frame in 0..frames {
            write!(csv, "{:.4}", frame as f64 / FPS)?;
            for key in &chosen {
                write!(csv, ",{:.3}", series[**key][frame])?;
            }
            csv.push('\n');
        }
        fs::write(out.join("trace.csv"), csv)?;
    }
    Ok(())
}
