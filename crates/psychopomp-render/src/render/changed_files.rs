//! Changed Files pixels: the card and optional title bar, a header with the
//! file count and the totals as Rolling Numbers, and fixed rows of status
//! badges, paths (directory muted, name plain), right-aligned `+N −M`
//! columns, and five-block diffstats whose blocks pop in as the row lands.
use anyhow::Result;
use psychopomp::{
    caption::{CaptionAlign, CaptionSpanPlan},
    changed_files::{
        Block, ChangedFilePlan, ChangedFilesPlan, PADDING, RowChannel, diffstat, grouped,
        row_property, split_path,
    },
    math::{remap_clamp, smoothstep},
    rolling::{CompiledRoll, RollingNumberPlan},
    tone::Tone,
};

use super::{
    HeadlessRenderer, VerticalMask,
    theme::mix,
    ui::{Bounds, card::UiCanvas},
    window::{ShellCache, WindowChrome, WindowPose, clipped, mono_spec, solid},
};

/// A landing row rises from this far below.
const RISE: f32 = 12.0;
const BAR_ALPHA: f32 = 0.12;

/// Column positions, measured once from the font: x offsets from the card's
/// left edge.
pub(crate) struct ChangedFilesLayout {
    column: f32,
    badge: f32,
    path: f32,
    /// Path columns available before the counts.
    path_columns: usize,
    added_right: f32,
    removed_right: f32,
    squares: f32,
    square: f32,
    square_gap: f32,
    names: Vec<[String; 3]>,
    /// The file count, added, and removed totals.
    totals: Vec<(RollingNumberPlan, CompiledRoll)>,
    shell: ShellCache,
}

impl ChangedFilesLayout {
    /// Whether any total is settling at `seconds`.
    pub(crate) fn moving(&self, seconds: f64) -> bool {
        self.totals.iter().any(|(_, roll)| roll.moving(seconds))
    }
}

/// `text` cut to `columns` characters with a trailing ellipsis.
fn clip_end(text: &str, columns: usize) -> String {
    if text.chars().count() <= columns {
        return text.to_owned();
    }
    let mut cut = text
        .chars()
        .take(columns.saturating_sub(1))
        .collect::<String>();
    cut.push('…');
    cut
}

/// A path that fits `columns`: its directory loses leading segments to an
/// ellipsis first, then the name is cut.
fn fit_path(dir: &str, name: &str, columns: usize) -> (String, String) {
    let name_chars = name.chars().count();
    if dir.chars().count() + name_chars <= columns {
        return (dir.to_owned(), name.to_owned());
    }
    if name_chars + 2 > columns {
        return (String::new(), clip_end(name, columns));
    }
    let room = columns - name_chars;
    let mut kept = String::new();
    for segment in dir.trim_end_matches('/').rsplit('/') {
        let candidate = format!("{segment}/{kept}");
        if candidate.chars().count() + 2 > room {
            break;
        }
        kept = candidate;
    }
    (format!("…/{kept}"), name.to_owned())
}

fn count_label(sign: &str, count: u32) -> String {
    format!("{sign}{}", grouped(count))
}

impl HeadlessRenderer {
    pub(crate) fn prepare_changed_files(&mut self, plan: &ChangedFilesPlan) -> ChangedFilesLayout {
        let size = plan.size;
        let column = self.mono_column(size);
        let badge = (size * 1.08).round();
        let path = PADDING + badge + (size * 0.75).round();
        let square = (size * 0.46).round();
        let square_gap = (size * 0.16).round();
        let right = plan.width - PADDING;
        let squares = right - 5.0 * square - 4.0 * square_gap;
        let (initial, rolls) = plan.totals_schedule();
        let totals = std::iter::once(initial)
            .chain(rolls.iter().copied())
            .collect::<Vec<_>>();
        let widest =
            |label: fn(&ChangedFilePlan) -> u32,
             total: fn(&psychopomp::changed_files::ChangedTotalsPlan) -> u32| {
                plan.files
                    .iter()
                    .map(|file| count_label("+", label(file)).chars().count())
                    .chain(
                        totals
                            .iter()
                            .map(|t| count_label("+", total(t)).chars().count()),
                    )
                    .max()
                    .unwrap_or(2) as f32
            };
        let removed_right = squares - (size * 0.9).round();
        let removed_width = widest(|f| f.removed, |t| t.removed) * column;
        let added_right = removed_right - removed_width - (size * 0.55).round();
        let added_width = widest(|f| f.added, |t| t.added) * column;
        let path_columns = ((added_right - added_width - (size * 0.9) - path) / column)
            .floor()
            .max(4.0) as usize;
        let center = plan.header_center();
        let roll = |renderer: &mut Self, number: RollingNumberPlan, values: Vec<(u64, String)>| {
            let number = values
                .into_iter()
                .fold(number, |number, (at, value)| number.roll(at, value));
            let compiled = renderer.compile_rolling_number(&number);
            (number, compiled)
        };
        let label_width = "Files changed".chars().count() as f32 * column;
        let files = RollingNumberPlan::new(
            [plan.origin[0] + PADDING + label_width + size * 0.95, center],
            (size * 0.82).round(),
            initial.files.to_string(),
        )
        .chip();
        let added = RollingNumberPlan::new(
            [plan.origin[0] + added_right, center],
            size,
            grouped(initial.added),
        )
        .aligned(CaptionAlign::Right)
        .tone(Tone::Success)
        .prefix(vec![CaptionSpanPlan::new("+", Tone::Success)]);
        let removed = RollingNumberPlan::new(
            [plan.origin[0] + removed_right, center],
            size,
            grouped(initial.removed),
        )
        .aligned(CaptionAlign::Right)
        .tone(Tone::Error)
        .prefix(vec![CaptionSpanPlan::new("−", Tone::Error)]);
        let totals = vec![
            roll(
                self,
                files,
                rolls
                    .iter()
                    .map(|t| (t.at_nanos, t.files.to_string()))
                    .collect(),
            ),
            roll(
                self,
                added,
                rolls
                    .iter()
                    .map(|t| (t.at_nanos, grouped(t.added)))
                    .collect(),
            ),
            roll(
                self,
                removed,
                rolls
                    .iter()
                    .map(|t| (t.at_nanos, grouped(t.removed)))
                    .collect(),
            ),
        ];
        ChangedFilesLayout {
            column,
            badge,
            path,
            path_columns,
            added_right,
            removed_right,
            squares,
            square,
            square_gap,
            names: plan
                .files
                .iter()
                .map(|file| {
                    [RowChannel::Reveal, RowChannel::Highlight, RowChannel::Dim]
                        .map(|channel| row_property(&file.id, channel))
                })
                .collect(),
            totals,
            shell: ShellCache::default(),
        }
    }

    pub(crate) fn composite_changed_files(
        &mut self,
        pixels: &mut [u8],
        plan: &ChangedFilesPlan,
        layout: &ChangedFilesLayout,
        seconds: f64,
        sample: impl Fn(&str, f32) -> f32,
    ) -> Result<()> {
        let pose = WindowPose::sample(&sample);
        if !pose.visible() {
            return Ok(());
        }
        let palette = self.theme.palette();
        let size = plan.card_size();
        let fill = mix(palette.background, palette.surface, 0.85);
        self.composite_window(
            pixels,
            Bounds {
                origin: plan.origin,
                size,
            },
            pose,
            WindowChrome {
                bar: plan.title.is_some(),
                title: plan.title.as_deref(),
                fill,
                corner_radius: 14.0,
            },
            &layout.shell,
        )?;
        let ink = pose.ink();
        if ink <= 0.001 {
            return Ok(());
        }
        let canvas = [self.spec.width, self.spec.height];
        let [left, header] = pose.place([plan.origin[0], plan.header_center()]);
        let row = plan.row_height();
        let reveals = layout
            .names
            .iter()
            .map(|names| sample(&names[0], 1.0).clamp(0.0, 1.0))
            .collect::<Vec<_>>();

        // Header: the label, the rolling totals, and the aggregate diffstat.
        self.mono_spans(
            pixels,
            [("Files changed", palette.text)],
            plan.size,
            [left + PADDING, header],
            ink,
            usize::MAX,
            None,
        );
        let offset = [
            pose.place(plan.origin)[0] - plan.origin[0],
            header - plan.header_center(),
        ];
        for (number, roll) in &layout.totals {
            self.composite_rolling_number_at(
                pixels,
                number,
                roll,
                seconds,
                number.origin,
                |property, default| match property {
                    "opacity" => ink,
                    "x" => offset[0],
                    "y" => offset[1],
                    _ => default,
                },
            );
        }
        let sum = plan.sum();
        let progress = reveals.iter().sum::<f32>() / reveals.len() as f32;
        self.diffstat_blocks(
            pixels,
            layout,
            [left + layout.squares, header],
            diffstat(sum.added, sum.removed),
            |index| ink * smoothstep((progress * 5.0 - index as f32).clamp(0.0, 1.0)),
        );
        let rows_top = header + plan.header_height() * 0.5;
        {
            let mut ui = UiCanvas::new(pixels, canvas);
            ui.fill(
                Bounds {
                    origin: [left + 1.0, rows_top - 1.0],
                    size: [size[0] - 2.0, 1.0],
                },
                0.0,
                solid(palette.raised),
                ink,
            );
        }

        // Rows through the list's window.
        let scroll = sample("scroll", 0.0);
        let window = [rows_top, rows_top + plan.visible_rows() as f32 * row];
        let mask = VerticalMask {
            top: window[0] + 1.0,
            bottom: window[1],
            fade: if plan.max_rows.is_some() {
                row * 0.45
            } else {
                0.0
            },
        };
        let clip = Bounds {
            origin: [left, window[0] + 1.0],
            size: [size[0], window[1] - window[0] - 1.0],
        };
        for (index, file) in plan.files.iter().enumerate() {
            let reveal = reveals[index];
            let top = rows_top + (index as f32 - scroll) * row;
            if reveal <= 0.001 || top + row <= window[0] || top >= window[1] {
                continue;
            }
            let [_, highlight, dim] = &layout.names[index];
            let highlight = sample(highlight, 0.0).clamp(0.0, 1.0);
            let dim = sample(dim, 0.0).clamp(0.0, 1.0);
            let presence = smoothstep(remap_clamp(reveal, [0.0, 0.85], [0.0, 1.0]));
            let alpha = ink * presence * (1.0 - 0.62 * dim);
            let center = top + row * 0.5 + (1.0 - presence) * RISE;
            if index + 1 < plan.files.len() || plan.max_rows.is_some() {
                clipped(pixels, canvas, clip, 0.0, |ui| {
                    ui.fill(
                        Bounds {
                            origin: [left + PADDING, top + row - 0.5],
                            size: [size[0] - PADDING * 2.0, 1.0],
                        },
                        0.0,
                        solid(palette.raised),
                        ink * presence * 0.55,
                    );
                });
            }
            if highlight > 0.001 {
                let accent = palette.accent;
                let bar = Bounds {
                    origin: [left + PADDING * 0.5, top + 3.0],
                    size: [size[0] - PADDING, row - 6.0],
                };
                clipped(pixels, canvas, clip, 0.0, |ui| {
                    ui.fill(
                        bar,
                        8.0,
                        solid(accent),
                        ink * presence * highlight * BAR_ALPHA,
                    );
                    ui.fill(
                        Bounds {
                            origin: bar.origin,
                            size: [3.0, bar.size[1]],
                        },
                        1.5,
                        solid(accent),
                        ink * presence * highlight,
                    );
                });
            }
            // Status badge.
            let tone = self.theme.tone(file.status.tone());
            let badge = Bounds::from_center(
                [left + PADDING + layout.badge * 0.5, center],
                [layout.badge, layout.badge],
            );
            clipped(pixels, canvas, clip, 0.0, |ui| {
                ui.fill(badge, 5.0, solid(tone), alpha * 0.16);
                ui.stroke(
                    badge,
                    5.0,
                    1.2,
                    super::ui::card::UiColor::srgb8(tone[0], tone[1], tone[2], 255),
                    alpha * 0.55,
                );
            });
            let letter = file.status.letter();
            let width = self
                .plain_text_sprite(letter, mono_spec(plan.size * 0.72, tone))
                .advance;
            self.mono_spans(
                pixels,
                [(letter, tone)],
                plan.size * 0.72,
                [badge.center()[0] - width * 0.5, center],
                alpha,
                usize::MAX,
                Some(mask),
            );
            // Path: a rename shows where it came from.
            let (dir, name) = split_path(&file.path);
            let mut columns = layout.path_columns;
            let mut x = left + layout.path;
            if let Some(from) = &file.from {
                let (_, from_name) = split_path(from);
                let from_label = format!("{from_name} → ");
                let used = from_label.chars().count();
                if used + 6 < columns {
                    x = self.mono_spans(
                        pixels,
                        [(from_label.as_str(), palette.muted)],
                        plan.size,
                        [x, center],
                        alpha * 0.85,
                        usize::MAX,
                        Some(mask),
                    );
                    columns -= used;
                }
            }
            let (dir, name) = fit_path(dir, name, columns);
            let name_color = if file.status == psychopomp::changed_files::FileStatus::Deleted {
                mix(palette.text, palette.muted, 0.45)
            } else {
                palette.text
            };
            self.mono_spans(
                pixels,
                [(dir.as_str(), palette.muted), (name.as_str(), name_color)],
                plan.size,
                [x, center],
                alpha,
                usize::MAX,
                Some(mask),
            );
            // Counts, right-aligned in their columns.
            for (count, sign, right, status) in [
                (file.added, "+", layout.added_right, Tone::Success),
                (file.removed, "−", layout.removed_right, Tone::Error),
            ] {
                if count == 0 {
                    continue;
                }
                let label = count_label(sign, count);
                let color = self.theme.tone(status);
                let width = label.chars().count() as f32 * layout.column;
                self.mono_spans(
                    pixels,
                    [(label.as_str(), color)],
                    plan.size,
                    [left + right - width, center],
                    alpha,
                    usize::MAX,
                    Some(mask),
                );
            }
            // Blocks pop in left to right as the row lands.
            self.diffstat_blocks(
                pixels,
                layout,
                [left + layout.squares, center],
                diffstat(file.added, file.removed),
                |k| {
                    alpha
                        * smoothstep(remap_clamp(
                            reveal,
                            [0.3 + 0.1 * k as f32, 0.6 + 0.1 * k as f32],
                            [0.0, 1.0],
                        ))
                },
            );
        }
        Ok(())
    }

    fn diffstat_blocks(
        &mut self,
        pixels: &mut [u8],
        layout: &ChangedFilesLayout,
        [x, y]: [f32; 2],
        blocks: [Block; 5],
        alpha: impl Fn(usize) -> f32,
    ) {
        let palette = self.theme.palette();
        let added = self.theme.tone(Tone::Success);
        let removed = self.theme.tone(Tone::Error);
        let neutral = mix(palette.raised, palette.muted, 0.25);
        let mut ui = UiCanvas::new(pixels, [self.spec.width, self.spec.height]);
        for (index, block) in blocks.into_iter().enumerate() {
            let a = alpha(index);
            if a <= 0.001 {
                continue;
            }
            let color = match block {
                Block::Added => added,
                Block::Removed => removed,
                Block::Neutral => neutral,
            };
            let pop = 0.9 + 0.1 * a.min(1.0);
            ui.fill(
                Bounds::from_center(
                    [
                        x + layout.square * 0.5
                            + index as f32 * (layout.square + layout.square_gap),
                        y,
                    ],
                    [layout.square * pop, layout.square * pop],
                ),
                2.0,
                solid(color),
                a,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{clip_end, fit_path};

    #[test]
    fn long_paths_lose_leading_directories_first() {
        assert_eq!(
            fit_path("src/session/", "a.ts", 40),
            ("src/session/".into(), "a.ts".into())
        );
        assert_eq!(
            fit_path("packages/opencode/src/session/", "compaction.ts", 26),
            ("…/session/".into(), "compaction.ts".into())
        );
        assert_eq!(fit_path("a/", "very-long-name.ts", 8).1, "very-lo…");
        assert_eq!(clip_end("abc", 3), "abc");
    }
}

#[cfg(test)]
mod gpu_tests {
    use psychopomp::changed_files::{ChangedFilePlan, ChangedFilesPlan, FileStatus};

    use crate::render::{HeadlessRenderer, RenderSpec};

    #[test]
    #[ignore = "requires a headless GPU; revealing a row never moves another and hidden cards leave no ink"]
    fn rows_reveal_in_their_own_slots() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "changed-files-proof".into(),
        }))
        .unwrap();
        let plan = ChangedFilesPlan::new(
            [300.0, 200.0],
            1300.0,
            vec![
                ChangedFilePlan::new("a", "src/a.ts", FileStatus::Added, 12, 0),
                ChangedFilePlan::new("b", "src/b.ts", FileStatus::Modified, 4, 2),
            ],
        );
        let layout = renderer.prepare_changed_files(&plan);
        let background = renderer.render_title_card("", None, 0.0);
        let mut draw = |opacity: f32, second: f32| {
            let mut pixels = background.clone();
            renderer
                .composite_changed_files(&mut pixels, &plan, &layout, 0.0, |property, d| {
                    match property {
                        "opacity" => opacity,
                        "row.b.reveal" => second,
                        _ => d,
                    }
                })
                .unwrap();
            pixels
        };
        assert!(draw(0.0, 1.0) == background, "hidden cards leave no ink");
        let rows = |pixels: &[u8]| {
            let top = plan.rows_top() as usize + 2;
            let bottom = (plan.rows_top() + plan.row_height()) as usize - 2;
            pixels[top * 1920 * 4..bottom * 1920 * 4].to_vec()
        };
        let hidden = draw(1.0, 0.0);
        let shown = draw(1.0, 1.0);
        assert!(hidden != shown);
        assert_eq!(rows(&hidden), rows(&shown), "the first row holds still");
    }
}
