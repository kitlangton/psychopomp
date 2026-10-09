//! The flat editor key: every input to the flat editor pixels (the shapes
//! pass, the title, and the code and annotations painted before the card
//! projection), compared exactly so an unchanged sample can reuse the last
//! flat frame.
//!
//! Floats compare by bits, so `-0.0` and `0.0` (or two NaNs) never collide;
//! a spurious miss only costs a rebuild. Every frame struct is destructured
//! without `..`, so a new field fails to compile until the key covers it.
//! Plan data behind references (code lines, hover plans) is cloned and
//! compared with `Eq`, which those types derive and which keeps floats out.
use psychopomp::{code::CodeLine, ide::HoverPlan};

use super::{
    EditorFrame, InlineRevealFrame, LineMarkFrame, PointerFrame, RenderSpec, Theme, TokenHighlight,
    ide::{CaretFrame, DiagnosticFrame, EditorAnnotations, HoverFrame, InlayFrame, SelectionFrame},
};

#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct FlatEditorKey {
    words: Vec<u32>,
    lines: Vec<CodeLine>,
    hovers: Vec<HoverPlan>,
}

#[derive(Default)]
struct Words(Vec<u32>);

impl Words {
    fn f(&mut self, value: f32) {
        self.0.push(value.to_bits());
    }

    fn u(&mut self, value: usize) {
        let value = value as u64;
        self.0.extend([value as u32, (value >> 32) as u32]);
    }

    fn s(&mut self, value: &str) {
        self.u(value.len());
        self.0.extend(value.bytes().map(u32::from));
    }
}

impl FlatEditorKey {
    pub(super) fn new(frame: &EditorFrame<'_>, theme: Theme, spec: &RenderSpec) -> Self {
        let EditorFrame {
            panel_offset_x,
            panel_offset_y,
            panel_opacity,
            line_marks,
            panel_rotation,
            panel_tilt_x,
            panel_tilt_y,
            panel_scale,
            panel_near_blur,
            focus_intensity,
            focus_line_y,
            focus_height,
            token_highlight,
            pointer,
            inline_reveals,
            lines,
            annotations,
        } = frame;
        let RenderSpec {
            width,
            height,
            file_name,
        } = spec;
        let mut w = Words::default();
        w.u(theme as usize);
        w.u(*width as usize);
        w.u(*height as usize);
        w.s(file_name);
        for value in [
            panel_offset_x,
            panel_offset_y,
            panel_opacity,
            panel_rotation,
            panel_tilt_x,
            panel_tilt_y,
            panel_scale,
            panel_near_blur,
            focus_intensity,
            focus_line_y,
            focus_height,
        ] {
            w.f(*value);
        }
        let TokenHighlight {
            x,
            y,
            width,
            opacity,
        } = token_highlight;
        let PointerFrame {
            x: pointer_x,
            y: pointer_y,
            opacity: pointer_opacity,
            rotation,
            scale,
            blur,
        } = pointer;
        for value in [
            x,
            y,
            width,
            opacity,
            pointer_x,
            pointer_y,
            pointer_opacity,
            rotation,
            scale,
            blur,
        ] {
            w.f(*value);
        }
        w.u(line_marks.len());
        for LineMarkFrame {
            line_id,
            mark,
            presence,
            row_height,
        } in *line_marks
        {
            w.s(line_id);
            w.u(*mark as usize);
            w.f(*presence);
            w.f(*row_height);
        }
        w.u(inline_reveals.len());
        for InlineRevealFrame {
            line_id,
            start_span,
            end_span,
            progress,
        } in *inline_reveals
        {
            w.s(line_id);
            w.u(*start_span);
            w.u(*end_span);
            w.f(*progress);
        }
        w.u(lines.len());
        let mut owned_lines = Vec::with_capacity(lines.len());
        for psychopomp::code::PlacedLine {
            line,
            x,
            y,
            opacity,
            blur,
        } in *lines
        {
            owned_lines.push((*line).clone());
            for value in [x, y, opacity, blur] {
                w.f(*value);
            }
        }
        let EditorAnnotations {
            inlays,
            diagnostics,
            selections,
            carets,
            hovers,
        } = annotations;
        w.u(inlays.len());
        for InlayFrame {
            line_id,
            start_span,
            end_span,
        } in *inlays
        {
            w.s(line_id);
            w.u(*start_span);
            w.u(*end_span);
        }
        w.u(diagnostics.len());
        for DiagnosticFrame {
            x,
            width,
            line_y,
            severity,
            gutter,
            draw,
            wave,
            opacity,
        } in *diagnostics
        {
            w.u(*severity as usize);
            w.u(usize::from(*gutter));
            for value in [x, width, line_y, draw, wave, opacity] {
                w.f(*value);
            }
        }
        w.u(selections.len());
        for SelectionFrame {
            x,
            width,
            line_y,
            opacity,
        } in *selections
        {
            for value in [x, width, line_y, opacity] {
                w.f(*value);
            }
        }
        w.u(carets.len());
        for CaretFrame { x, line_y, opacity } in *carets {
            for value in [x, line_y, opacity] {
                w.f(*value);
            }
        }
        w.u(hovers.len());
        let mut owned_hovers = Vec::with_capacity(hovers.len());
        for HoverFrame {
            plan,
            x,
            width,
            line_y,
            presence,
            opacity,
        } in *hovers
        {
            owned_hovers.push((*plan).clone());
            for value in [x, width, line_y, presence, opacity] {
                w.f(*value);
            }
        }
        Self {
            words: w.0,
            lines: owned_lines,
            hovers: owned_hovers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EditorFrame, FlatEditorKey, InlineRevealFrame, LineMarkFrame, PointerFrame, RenderSpec,
        Theme, TokenHighlight,
    };
    use crate::render::ide::{CaretFrame, SelectionFrame};

    #[test]
    fn flat_editor_key_misses_when_any_flat_input_changes() {
        let spec = RenderSpec {
            width: 64,
            height: 48,
            file_name: "a.ts".into(),
        };
        let marks = [LineMarkFrame {
            line_id: "l1",
            mark: psychopomp::editor::LineMarkPlan::Added,
            presence: 0.5,
            row_height: 44.,
        }];
        let reveals = [InlineRevealFrame {
            line_id: "l1",
            start_span: 0,
            end_span: 1,
            progress: 0.25,
        }];
        let selections = [SelectionFrame {
            x: 1.,
            width: 2.,
            line_y: 3.,
            opacity: 0.5,
        }];
        let carets = [CaretFrame {
            x: 1.,
            line_y: 0.,
            opacity: 1.,
        }];
        let span = |text: &str| psychopomp::code::StyledSpan {
            text: text.into(),
            style: psychopomp::code::SyntaxStyle::Plain,
        };
        let line = psychopomp::code::CodeLine::new("l1", vec![span("a")]);
        let edited = psychopomp::code::CodeLine::new("l1", vec![span("b")]);
        let placed = |line| {
            [psychopomp::code::PlacedLine {
                line,
                x: 0.,
                y: 0.,
                opacity: 1.,
                blur: 0.,
            }]
        };
        let (lines, edited_lines) = (placed(&line), placed(&edited));
        let base = || EditorFrame {
            panel_offset_x: 0.,
            panel_offset_y: 0.,
            panel_opacity: 1.,
            line_marks: &[],
            panel_rotation: 0.,
            panel_tilt_x: 0.,
            panel_tilt_y: 0.,
            panel_scale: 1.,
            panel_near_blur: 0.,
            focus_intensity: 0.,
            focus_line_y: 0.,
            focus_height: 44.,
            token_highlight: TokenHighlight {
                x: 0.,
                y: 0.,
                width: 0.,
                opacity: 0.,
            },
            pointer: PointerFrame {
                x: 0.,
                y: 0.,
                opacity: 0.,
                rotation: 0.,
                scale: 1.,
                blur: 0.,
            },
            inline_reveals: &[],
            lines: &lines,
            annotations: Default::default(),
        };
        let key = |frame: &EditorFrame<'_>| FlatEditorKey::new(frame, Theme::default(), &spec);
        let reference = key(&base());
        assert_eq!(reference, key(&base()));
        for index in 0..12 {
            let mut f = base();
            match index {
                0 => f.focus_intensity = f32::from_bits(1),
                1 => f.focus_line_y = -0.,
                2 => f.focus_height = 44.000_004,
                3 => f.token_highlight.width = 1.,
                4 => f.pointer.blur = 0.1,
                5 => f.line_marks = &marks,
                6 => f.inline_reveals = &reveals,
                7 => f.lines = &[],
                8 => f.lines = &edited_lines,
                9 => f.annotations.selections = &selections,
                10 => f.annotations.carets = &carets,
                _ => f.panel_offset_y = 1.,
            }
            assert_ne!(reference, key(&f), "change {index} kept the key");
        }
        assert_ne!(
            reference,
            FlatEditorKey::new(&base(), Theme::Neutral, &spec)
        );
        let renamed = RenderSpec {
            file_name: "b.ts".into(),
            ..spec.clone()
        };
        assert_ne!(
            reference,
            FlatEditorKey::new(&base(), Theme::default(), &renamed)
        );
    }
}
