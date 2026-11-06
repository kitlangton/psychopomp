//! Shared pieces for every beat: how a beat starts on its kit and mono labels
//! in explicit colours. Entrances come from [`psychopomp::kit::moves`].
use psychopomp::{
    author::PlanBuilder,
    caption::{CaptionAlign, CaptionSpanPlan},
    face::Face,
    kit::StyleKit,
    narration::Spoken,
    paint::parse_hex_rgb,
    stage::StageElement,
    tone::Tone,
    typography::TypePlan,
};
use psychopomp_media::Audio;

/// Lead before the voice and hold after it. The joined narration had 0.6 s
/// between segments; a hard cut with these leaves about 0.45 s.
pub const LEAD: u64 = 150_000_000;
pub const TAIL: u64 = 300_000_000;

/// A beat on `kit`: the plan, the placed voice, and the beat's length.
pub fn begin<'a>(
    kit: &StyleKit,
    id: &str,
    audio: &'a Audio,
    tail: u64,
) -> (PlanBuilder, Spoken<'a>, u64) {
    let duration = LEAD + audio.duration() + tail;
    let mut scene = kit.begin(id, duration);
    let spoken = audio.place(&mut scene, LEAD);
    (scene, spoken, duration)
}

pub fn rgb(hex: &str) -> [u8; 3] {
    parse_hex_rgb(hex).expect("valid hex colour")
}

/// A mono label in explicit colours; `at` is its left edge and middle.
pub fn mono(id: &str, at: [f32; 2], size: f32, spans: &[(&str, &str)]) -> StageElement {
    mono_aligned(id, at, size, spans, CaptionAlign::Left)
}

pub fn mono_aligned(
    id: &str,
    at: [f32; 2],
    size: f32,
    spans: &[(&str, &str)],
    align: CaptionAlign,
) -> StageElement {
    StageElement::Label {
        id: id.into(),
        at: [at[0], at[1], 0.0],
        size,
        align,
        spans: spans
            .iter()
            .map(|(text, hex)| CaptionSpanPlan {
                text: (*text).into(),
                tone: Tone::default(),
                color: Some(rgb(hex)),
            })
            .collect(),
        face: Face::Mono,
    }
}

/// Kinetic type whose left edge sits at `at`.
pub fn left(id: &str, at: [f32; 2], spec: TypePlan) -> StageElement {
    StageElement::kinetic_type(id, [at[0], at[1], 0.0], spec.align(CaptionAlign::Left))
}

/// Kinetic type centred on `at`.
pub fn centred(id: &str, at: [f32; 2], spec: TypePlan) -> StageElement {
    StageElement::kinetic_type(id, [at[0], at[1], 0.0], spec)
}
