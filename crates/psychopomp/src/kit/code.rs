//! Highlighted code on the Stage. A [`CodeBlock`] lays source out as one
//! left-aligned label per line, every token coloured by a [`SyntaxTheme`],
//! with optional line numbers, a panel, inserted/deleted bars, and a focus
//! bar. Each line is a top-level Stage element (`{id}-l{n}`), so the camera
//! can frame a line or a run of lines by id:
//!
//! ```ignore
//! let code = CodeBlock::new("entry", Lang::TypeScript, SOURCE).at([180.0, 220.0]);
//! let stage = kit.stage(code.elements());
//! code.reveal(&mut s, sc, at, millis(40));
//! code.focus(&mut s, sc, 3..5, at + seconds(1.2), 0.5);
//! s.camera().frame(sc, &code.ids(3..5), 160.0, at + seconds(1.2), Move::Glide(0.9))?;
//! ```
//!
//! Focus dims the other lines rather than hiding them, so the viewer keeps
//! the shape of the file while the eye goes to the line the voice names.
use std::ops::Range;

use crate::{
    author::PlanBuilder,
    caption::{CaptionAlign, CaptionSpanPlan},
    face::Face,
    math::easing::Ease,
    paint::Paint,
    stage::{StageActor, StageElement},
    syntax::{Lang, SyntaxTheme, lex},
    tone::Tone,
};

use super::scale;

/// Line pitch as a multiple of the type size.
pub const LINE_HEIGHT: f32 = 1.55;
/// Advance of one CommitMono character as a multiple of the type size.
pub const ADVANCE: f32 = 0.6;
/// How bright an unfocused line stays.
pub const DIM: f32 = 0.28;
/// How far a line rises as it reveals, as a fraction of its size.
const RISE: f32 = 0.3;

/// A whole-line mark drawn behind a line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineMark {
    Inserted,
    Deleted,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeBlock {
    pub id: String,
    pub lang: Lang,
    pub lines: Vec<String>,
    pub theme: SyntaxTheme,
    /// Type size in pixels.
    pub size: f32,
    /// Left edge of the code text and the centre of the first line.
    pub origin: [f32; 2],
    /// The first line's number, or `None` for no gutter.
    pub numbers: Option<usize>,
    /// Padding of the panel behind the code, or `None` for code set straight
    /// on the frame (full bleed).
    pub panel: Option<f32>,
    pub marks: Vec<(usize, LineMark)>,
}

impl CodeBlock {
    /// `source` in `lang` on the night theme at the film's code size. Tabs
    /// become four spaces; a trailing newline is dropped.
    pub fn new(id: &str, lang: Lang, source: &str) -> Self {
        Self {
            id: id.into(),
            lang,
            lines: source
                .trim_end_matches('\n')
                .replace('\t', "    ")
                .split('\n')
                .map(Into::into)
                .collect(),
            theme: SyntaxTheme::night(),
            size: scale::CODE,
            origin: [160.0, 200.0],
            numbers: None,
            panel: None,
            marks: Vec::new(),
        }
    }

    /// Put the code's left edge and first line's centre at `origin`.
    pub fn at(mut self, origin: [f32; 2]) -> Self {
        self.origin = origin;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn theme(mut self, theme: SyntaxTheme) -> Self {
        self.theme = theme;
        self
    }

    /// Number the lines from `first`.
    pub fn numbered(mut self, first: usize) -> Self {
        self.numbers = Some(first);
        self
    }

    /// Set the code on a panel of the theme's background, `padding` pixels
    /// around the text.
    pub fn panel(mut self, padding: f32) -> Self {
        self.panel = Some(padding);
        self
    }

    /// Mark `lines` (indices from 0) as inserted or deleted.
    pub fn mark(mut self, lines: Range<usize>, mark: LineMark) -> Self {
        self.marks.extend(lines.map(|line| (line, mark)));
        self
    }

    /// The Stage id of line `index` (from 0).
    pub fn line(&self, index: usize) -> String {
        format!("{}-l{index}", self.id)
    }

    /// The Stage ids of `lines`, for the camera.
    pub fn ids(&self, lines: Range<usize>) -> Vec<String> {
        lines.map(|index| self.line(index)).collect()
    }

    pub fn pitch(&self) -> f32 {
        self.size * LINE_HEIGHT
    }

    /// The vertical centre of line `index`.
    pub fn line_y(&self, index: usize) -> f32 {
        self.origin[1] + index as f32 * self.pitch()
    }

    /// The left edge and vertical centre of line `index`'s text.
    pub fn line_at(&self, index: usize) -> [f32; 2] {
        [self.origin[0], self.line_y(index)]
    }

    /// The widest line's width, from the monospace advance.
    pub fn text_width(&self) -> f32 {
        let widest = self
            .lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        widest as f32 * self.size * ADVANCE
    }

    /// Left, top, width, and height of everything the block draws, gutter
    /// and panel included.
    pub fn bounds(&self) -> [f32; 4] {
        let pad = self.panel.unwrap_or(0.0);
        let gutter = self.gutter_width();
        let left = self.origin[0] - gutter - pad;
        let top = self.origin[1] - self.pitch() * 0.5 - pad;
        let width = gutter + self.text_width() + pad * 2.0;
        let height = self.pitch() * self.lines.len() as f32 + pad * 2.0;
        [left, top, width, height]
    }

    fn gutter_width(&self) -> f32 {
        self.numbers.map_or(0.0, |first| {
            let digits = (first + self.lines.len()).to_string().len();
            (digits as f32 + 2.0) * self.size * ADVANCE
        })
    }

    /// The bars span the block's width, and the focus bar starts on line 0.
    fn bar(&self, id: &str, index: usize, rows: usize, color: [u8; 3]) -> StageElement {
        let [left, _, width, _] = self.bounds();
        let height = self.pitch() * rows as f32;
        let y = self.line_y(index) - self.pitch() * 0.5 + height * 0.5;
        StageElement::rect(id, [left + width * 0.5, y, 0.0], [width, height])
            .paint(solid(color))
            .stroke(None)
    }

    /// Every element, back to front: panel, mark bars, focus bar (hidden
    /// until [`Self::focus`]), line numbers, then the lines.
    pub fn elements(&self) -> Vec<StageElement> {
        let theme = self.theme;
        let mut out = Vec::new();
        if self.panel.is_some() {
            let [left, top, width, height] = self.bounds();
            out.push(
                StageElement::rect(
                    &format!("{}-panel", self.id),
                    [left + width * 0.5, top + height * 0.5, 0.0],
                    [width, height],
                )
                .paint(solid(theme.background))
                .stroke(None)
                .corner(self.size * 0.4),
            );
        }
        for &(index, mark) in &self.marks {
            let color = match mark {
                LineMark::Inserted => theme.inserted,
                LineMark::Deleted => theme.deleted,
            };
            out.push(self.bar(&format!("{}-mark{index}", self.id), index, 1, color));
        }
        out.push(self.bar(&format!("{}-focus", self.id), 0, 1, theme.focus));
        if let Some(first) = self.numbers {
            let right = self.origin[0] - self.size * ADVANCE * 1.5;
            for index in 0..self.lines.len() {
                let sign = self.marks.iter().find(|(line, _)| *line == index);
                let (text, color) = match sign {
                    Some((_, LineMark::Inserted)) => ("+".to_string(), theme.add),
                    Some((_, LineMark::Deleted)) => ("-".to_string(), theme.remove),
                    None => ((first + index).to_string(), theme.gutter),
                };
                out.push(
                    label(
                        &format!("{}-n{index}", self.id),
                        [right, self.line_y(index)],
                        self.size,
                        vec![span(&text, color)],
                    )
                    .align(CaptionAlign::Right),
                );
            }
        }
        for (index, tokens) in lex(self.lang, &self.lines.join("\n"))
            .into_iter()
            .enumerate()
        {
            let spans = tokens
                .iter()
                .map(|token| span(&token.text, theme.color(token.role)))
                .collect::<Vec<_>>();
            if spans.is_empty() {
                continue;
            }
            out.push(label(
                &self.line(index),
                self.line_at(index),
                self.size,
                spans,
            ));
        }
        out
    }

    /// Ids of every line that has text, with its number if it has one.
    fn drawn(&self, index: usize) -> Vec<String> {
        let mut ids = Vec::new();
        if !self.lines[index].trim().is_empty() {
            ids.push(self.line(index));
        }
        if self.numbers.is_some() {
            ids.push(format!("{}-n{index}", self.id));
        }
        ids
    }

    /// Hide every line and number, and the focus bar, before anything writes
    /// them. [`Self::reveal`] and [`Self::focus`] bring them in.
    pub fn hide(&self, s: &mut StageActor, sc: &mut PlanBuilder) {
        for index in 0..self.lines.len() {
            for id in self.drawn(index) {
                s.channel(sc, &format!("{id}.opacity"), 0.0);
            }
        }
        s.channel(sc, &format!("{}-focus.opacity", self.id), 0.0);
    }

    /// Show the block at rest from the first frame, focus bar hidden.
    pub fn show(&self, s: &mut StageActor, sc: &mut PlanBuilder) {
        s.channel(sc, &format!("{}-focus.opacity", self.id), 0.0);
    }

    /// Lines rise into place one after another, `stagger` apart, top first.
    pub fn reveal(&self, s: &mut StageActor, sc: &mut PlanBuilder, at: u64, stagger: u64) {
        self.reveal_lines(s, sc, 0..self.lines.len(), at, stagger);
    }

    /// Reveal only `lines`, `stagger` apart.
    pub fn reveal_lines(
        &self,
        s: &mut StageActor,
        sc: &mut PlanBuilder,
        lines: Range<usize>,
        at: u64,
        stagger: u64,
    ) {
        s.channel(sc, &format!("{}-focus.opacity", self.id), 0.0);
        let start = lines.start;
        for index in lines {
            let when = at + (index - start) as u64 * stagger;
            for id in self.drawn(index) {
                let opacity = s.channel(sc, &format!("{id}.opacity"), 0.0);
                sc.ease(&opacity, when, 1.0, 0.35, Ease::ARRIVE);
                let y = s.channel(sc, &format!("{id}.y"), self.size * RISE);
                sc.ease(&y, when, 0.0, 0.55, Ease::ARRIVE);
            }
        }
    }

    /// Bring `lines` to full strength over a focus bar and dim the rest to
    /// [`DIM`], over `seconds`. Call again to move the focus; [`Self::clear`]
    /// lifts it.
    pub fn focus(
        &self,
        s: &mut StageActor,
        sc: &mut PlanBuilder,
        lines: Range<usize>,
        at: u64,
        seconds: f32,
    ) {
        for index in 0..self.lines.len() {
            let level = if lines.contains(&index) { 1.0 } else { DIM };
            for id in self.drawn(index) {
                s.ease(
                    sc,
                    &format!("{id}.opacity"),
                    at,
                    level,
                    seconds,
                    Ease::TRAVEL,
                );
            }
        }
        let bar = format!("{}-focus", self.id);
        let rows = lines.len().max(1);
        let offset = (lines.start as f32 + (rows as f32 - 1.0) * 0.5) * self.pitch();
        let height = (rows as f32 - 1.0) * self.pitch();
        let opacity = s.channel(sc, &format!("{bar}.opacity"), 0.0);
        sc.ease(&opacity, at, 1.0, seconds, Ease::TRAVEL);
        s.ease(sc, &format!("{bar}.y"), at, offset, seconds, Ease::TRAVEL);
        s.ease(
            sc,
            &format!("{bar}.height"),
            at,
            height,
            seconds,
            Ease::TRAVEL,
        );
    }

    /// Every line back to full strength and the focus bar out.
    pub fn clear(&self, s: &mut StageActor, sc: &mut PlanBuilder, at: u64, seconds: f32) {
        for index in 0..self.lines.len() {
            for id in self.drawn(index) {
                s.ease(sc, &format!("{id}.opacity"), at, 1.0, seconds, Ease::TRAVEL);
            }
        }
        s.ease(
            sc,
            &format!("{}-focus.opacity", self.id),
            at,
            0.0,
            seconds,
            Ease::TRAVEL,
        );
    }

    /// Fade the whole block out (an exit: shorter than the entrance).
    pub fn leave(&self, s: &mut StageActor, sc: &mut PlanBuilder, at: u64, seconds: f32) {
        let mut ids = (0..self.lines.len())
            .flat_map(|index| self.drawn(index))
            .collect::<Vec<_>>();
        ids.push(format!("{}-focus", self.id));
        ids.extend(
            self.marks
                .iter()
                .map(|(index, _)| format!("{}-mark{index}", self.id)),
        );
        if self.panel.is_some() {
            ids.push(format!("{}-panel", self.id));
        }
        for id in ids {
            s.ease(sc, &format!("{id}.opacity"), at, 0.0, seconds, Ease::LEAVE);
        }
    }
}

fn solid(color: [u8; 3]) -> Paint {
    Paint::Solid {
        rgba: [color[0], color[1], color[2], 255],
    }
}

fn span(text: &str, color: [u8; 3]) -> CaptionSpanPlan {
    CaptionSpanPlan {
        text: text.into(),
        tone: Tone::default(),
        color: Some(color),
    }
}

/// A left-aligned mono label: `at` is its left edge and vertical centre.
fn label(id: &str, at: [f32; 2], size: f32, spans: Vec<CaptionSpanPlan>) -> StageElement {
    StageElement::Label {
        id: id.into(),
        at: [at[0], at[1], 0.0],
        size,
        align: CaptionAlign::Left,
        spans,
        face: Face::Mono,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{author::SECOND, kit::explainer, syntax::Role};

    const SOURCE: &str =
        "export const github = {\n  slug: \"github\",\n\n  auth: oauth(\"app\"),\n}\n";

    fn block() -> CodeBlock {
        CodeBlock::new("entry", Lang::TypeScript, SOURCE)
            .at([200.0, 300.0])
            .numbered(12)
            .panel(32.0)
            .mark(3..4, LineMark::Inserted)
    }

    fn spans_of<'a>(elements: &'a [StageElement], id: &str) -> &'a [CaptionSpanPlan] {
        elements
            .iter()
            .find_map(|element| match element {
                StageElement::Label { id: own, spans, .. } if own == id => Some(spans.as_slice()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no label {id}"))
    }

    #[test]
    fn every_line_is_a_left_aligned_label_in_theme_colours() {
        let code = block();
        let elements = code.elements();
        let theme = SyntaxTheme::night();
        let line = spans_of(&elements, "entry-l0");
        assert_eq!(line[0].text, "export const ");
        assert_eq!(line[0].color, Some(theme.color(Role::Keyword)));
        let strings = spans_of(&elements, "entry-l1");
        assert!(
            strings
                .iter()
                .any(|s| s.text.contains("\"github\"") && s.color == Some(theme.string))
        );
        // A blank line draws nothing, so it can't be framed or faded.
        assert!(elements.iter().all(|element| element.id() != "entry-l2"));
        let StageElement::Label { at, align, .. } = elements
            .iter()
            .find(|element| element.id() == "entry-l3")
            .unwrap()
        else {
            panic!("label");
        };
        assert_eq!(*align, CaptionAlign::Left);
        assert_eq!(*at, [200.0, 300.0 + 3.0 * code.pitch(), 0.0]);
        // The inserted line's gutter shows a plus in the add colour.
        let sign = spans_of(&elements, "entry-n3");
        assert_eq!(
            (sign[0].text.as_str(), sign[0].color),
            ("+", Some(theme.add))
        );
        assert_eq!(spans_of(&elements, "entry-n0")[0].text, "12");
    }

    #[test]
    fn the_panel_and_bars_wrap_the_text() {
        let code = block();
        let [left, top, width, height] = code.bounds();
        assert!(left < 200.0 - code.size);
        assert!(top < 300.0 - code.pitch() * 0.5);
        assert!(left + width > 200.0 + code.text_width());
        assert!((height - (code.pitch() * 5.0 + 64.0)).abs() < 1e-3);
        let ids = code
            .elements()
            .iter()
            .map(|element| element.id().to_string())
            .collect::<Vec<_>>();
        let order = |id: &str| ids.iter().position(|own| own == id).unwrap();
        assert!(order("entry-panel") < order("entry-mark3"));
        assert!(order("entry-mark3") < order("entry-focus"));
        assert!(order("entry-focus") < order("entry-l0"));
    }

    #[test]
    fn focus_dims_the_rest_and_moves_the_bar_over_the_lines() {
        let code = block();
        let kit = explainer::kit();
        let plan = kit.stage(code.elements());
        plan.validate().unwrap();
        let mut scene = kit.begin("code", 4 * SECOND);
        let mut s = StageActor::declare(&mut scene, "stage", &plan).unwrap();
        code.hide(&mut s, &mut scene);
        code.reveal(&mut s, &mut scene, 0, SECOND / 20);
        code.focus(&mut s, &mut scene, 3..5, 2 * SECOND, 0.5);
        let value = |scene: &PlanBuilder, property: &str| {
            scene
                .sample(s.actor(), property, 3 * SECOND)
                .unwrap()
                .position
        };
        assert_eq!(value(&scene, "entry-l3.opacity"), 1.0);
        assert_eq!(value(&scene, "entry-l0.opacity"), DIM);
        assert_eq!(value(&scene, "entry-focus.opacity"), 1.0);
        assert!((value(&scene, "entry-focus.y") - 3.5 * code.pitch()).abs() < 1e-3);
        assert!((value(&scene, "entry-focus.height") - code.pitch()).abs() < 1e-3);
        scene.finish().unwrap();
    }

    #[test]
    fn the_camera_can_frame_a_line() {
        let code = block();
        let kit = explainer::kit();
        let plan = kit.stage(code.elements());
        let mut scene = kit.begin("code", 4 * SECOND);
        let s = StageActor::declare(&mut scene, "stage", &plan).unwrap();
        let ids = code.ids(3..4);
        let targets = ids.iter().map(String::as_str).collect::<Vec<_>>();
        let pose = s.camera().framing(&scene, &targets, 200.0, 0).unwrap();
        // Framing one line pushes in: the camera zooms past rest.
        assert!(pose.zoom > 1.0 || pose.position.z > 0.0, "{pose:?}");
    }
}
