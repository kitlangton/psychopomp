pub mod pr_53821;
pub mod pr_53822;
pub mod pr_53823;

pub use pr_53821::build_53821;
pub use pr_53822::build_53822;
pub use pr_53823::build_53823;

use anyhow::{Context, Result};
use psychopomp::{
    author::{PlanBuilder, seconds},
    caption::CaptionSpanPlan,
    chrome::{chip, footer},
    editor::diff::{Diff, DiffEditor},
    narration::{Narration, Spoken},
    plan::ScenePlan,
    tone::Tone,
};

pub struct Pr {
    pub number: &'static str,
    pub title: &'static str,
    pub slug: &'static str,
}

pub fn span(text: &str, tone: Tone) -> CaptionSpanPlan {
    CaptionSpanPlan::new(text, tone)
}

pub fn header(scene: &mut PlanBuilder, pr: &Pr, type_at: Option<u64>) -> Result<()> {
    let mut caption = psychopomp::chrome::header(scene, pr.number, pr.title)?;
    if let Some(at) = type_at {
        caption.type_in(scene, at, 55.0, 0.6);
    }
    Ok(())
}

pub fn code_with(
    pr: &Pr,
    narration: &Narration,
    (diff, steps, note): (Diff, Vec<&'static str>, &'static str),
    entrance: bool,
    annotate: impl FnOnce(&mut PlanBuilder, &DiffEditor, &Spoken<'_>) -> Result<()>,
) -> Result<ScenePlan> {
    let reading =
        narration.reading(seconds(0.9), [(&format!("{}-code", pr.slug), seconds(1.6))])?;
    let mut scene = PlanBuilder::new(format!("{}-code", pr.slug), reading.duration());
    let [spoken] = reading.place(&mut scene);
    header(&mut scene, pr, None)?;
    let mut change = chip(&mut scene, "chip-change", Tone::Accent, "the change")?;
    change.show(&mut scene, seconds(0.2));
    let times = steps
        .iter()
        .map(|phrase| spoken.at(phrase))
        .collect::<Vec<_>>();
    let editor = diff.declare(&mut scene, &times, seconds(0.9), entrance)?;
    let mut caption = footer(&mut scene, "footer", vec![span(note, Tone::Muted)])?;
    caption.show(&mut scene, seconds(0.6));
    annotate(&mut scene, &editor, &spoken)?;
    scene.finish().with_context(|| format!("{}-code", pr.slug))
}
