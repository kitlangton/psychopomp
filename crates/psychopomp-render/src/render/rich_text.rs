//! Bounded native Markdown typography, shaped as rich runs before rasterization.
//! Font metrics and wrapping are theme-independent; colored sprites are cached
//! per current theme, not re-laid out every animation frame.
use super::*;
use cosmic_text::Style;
use psychopomp::component_prototype::RichTextPlan;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::cell::RefCell;

#[derive(Clone, Default, Debug)]
struct Run {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    link: bool,
    strike: bool,
}
#[derive(Default, Debug)]
struct Block {
    runs: Vec<Run>,
    heading: usize,
    indent: usize,
    marker: Option<String>,
    quote: bool,
    code: bool,
    rule: bool,
}

fn blocks(markdown: &str) -> Result<Vec<Block>> {
    let mut blocks = Vec::new();
    let mut block = Block::default();
    let mut style = Run::default();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut quote = 0;
    let mut emphasis = [0_usize; 4];
    let flush = |blocks: &mut Vec<Block>, block: &mut Block| {
        if !block.runs.is_empty() || block.rule {
            blocks.push(std::mem::take(block));
        }
    };
    for event in Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Start(Tag::Paragraph) => {
                block.indent = lists.len();
                block.quote = quote > 0;
            }
            Event::Start(Tag::Heading { level, .. }) => {
                block.heading = level as usize;
                block.quote = quote > 0;
            }
            Event::Start(Tag::Strong) => emphasis[0] += 1,
            Event::Start(Tag::Emphasis) => emphasis[1] += 1,
            Event::Start(Tag::Strikethrough) => emphasis[2] += 1,
            Event::Start(Tag::Link { .. }) => emphasis[3] += 1,
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut blocks, &mut block);
                quote += 1;
            }
            Event::Start(Tag::List(start)) => {
                flush(&mut blocks, &mut block);
                lists.push(start);
            }
            Event::Start(Tag::Item) => {
                flush(&mut blocks, &mut block);
                block.indent = lists.len();
                block.quote = quote > 0;
                block.marker = Some(match lists.last_mut() {
                    Some(Some(n)) => {
                        let text = format!("{n}.");
                        *n += 1;
                        text
                    }
                    _ => "•".into(),
                });
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut blocks, &mut block);
                block.code = true;
            }
            Event::Text(text) => block.runs.push(Run {
                text: text.into_string(),
                code: block.code,
                ..style.clone()
            }),
            Event::Code(text) => block.runs.push(Run {
                text: text.into_string(),
                code: true,
                ..style.clone()
            }),
            Event::SoftBreak => block.runs.push(Run {
                text: " ".into(),
                ..style.clone()
            }),
            Event::HardBreak => block.runs.push(Run {
                text: "\n".into(),
                ..style.clone()
            }),
            Event::Rule => {
                flush(&mut blocks, &mut block);
                block.rule = true;
                flush(&mut blocks, &mut block);
            }
            Event::End(TagEnd::Strong) => emphasis[0] -= 1,
            Event::End(TagEnd::Emphasis) => emphasis[1] -= 1,
            Event::End(TagEnd::Strikethrough) => emphasis[2] -= 1,
            Event::End(TagEnd::Link) => emphasis[3] -= 1,
            Event::End(TagEnd::List(_)) => {
                flush(&mut blocks, &mut block);
                lists.pop();
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush(&mut blocks, &mut block);
                quote -= 1;
            }
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::CodeBlock,
            ) => flush(&mut blocks, &mut block),
            Event::Html(_) | Event::InlineHtml(_) | Event::Start(Tag::Image { .. }) => {
                bail!("rich text does not embed HTML or images; use separate visual actors")
            }
            _ => bail!("unsupported Markdown construct in rich text"),
        }
        style.bold = emphasis[0] > 0;
        style.italic = emphasis[1] > 0;
        style.strike = emphasis[2] > 0;
        style.link = emphasis[3] > 0;
    }
    flush(&mut blocks, &mut block);
    Ok(blocks)
}

pub(crate) struct RichTextSource {
    pub(crate) plan: RichTextPlan,
    blocks: Vec<ParsedBlock>,
}

struct ParsedBlock {
    kind: Block,
    size: f32,
    line_height: f32,
    x: f32,
    width: f32,
}

pub(crate) fn parse(plan: RichTextPlan) -> Result<RichTextSource> {
    anyhow::ensure!(
        plan.fade_blur.is_finite() && (0.0..=12.).contains(&plan.fade_blur),
        "rich text fade blur must be finite and in [0,12]"
    );
    if let Some([top, bottom, fade]) = plan.vertical_mask {
        anyhow::ensure!(
            (VerticalMask { top, bottom, fade }).is_valid(),
            "invalid rich text vertical mask"
        );
    }
    if plan.origin.iter().any(|n| !n.is_finite())
        || !plan.width.is_finite()
        || !(80.0..=1600.).contains(&plan.width)
        || !plan.font_size.is_finite()
        || !(12.0..=120.).contains(&plan.font_size)
        || plan.markdown.len() > 32_000
        || plan.markdown.trim().is_empty()
    {
        bail!(
            "rich text requires finite origin, width 80..1600, font size 12..120, and 1..32000 Markdown bytes"
        );
    }
    let parsed = blocks(&plan.markdown)?;
    anyhow::ensure!(parsed.len() <= 64, "rich text supports at most 64 blocks");
    let parsed = parsed
        .into_iter()
        .map(|block| {
            let size = plan.font_size
                * match block.heading {
                    1 => 1.8,
                    2 => 1.5,
                    3 => 1.25,
                    4.. => 1.1,
                    _ => 1.,
                };
            let line_height = (size * 1.4).ceil();
            let x = block.indent as f32 * size * 1.1
                + if block.quote { size * 0.75 } else { 0. }
                + if block.code { 16. } else { 0. };
            let width = plan.width - x - if block.code { 16. } else { 0. };
            anyhow::ensure!(
                width >= size * 2.,
                "rich text nesting leaves too little room for text"
            );
            Ok(ParsedBlock {
                kind: block,
                size,
                line_height,
                x,
                width,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(RichTextSource {
        plan,
        blocks: parsed,
    })
}

struct ShapedBlock {
    buffer: RefCell<Buffer>,
    x: f32,
    y: f32,
    height: u32,
    advance: f32,
    kind: Block,
    marker: Option<TextSprite>,
}
pub(crate) struct RichTextGlyphs {
    blocks: Vec<ShapedBlock>,
    width: u32,
    cached: theme::ThemedCache<Vec<TextSprite>>,
    mask: Option<VerticalMask>,
    fade_blur: f32,
}

impl HeadlessRenderer {
    pub(crate) fn prepare_rich_text_source(
        &mut self,
        source: RichTextSource,
    ) -> Result<RichTextGlyphs> {
        let RichTextSource { plan, blocks } = source;
        let mut shaped = Vec::new();
        let mut y = 0.;
        for ParsedBlock {
            kind: block,
            size,
            line_height,
            x,
            width,
        } in blocks
        {
            let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(size, line_height));
            buffer.set_size(Some(width), None);
            buffer.set_wrap(Wrap::WordOrGlyph);
            let base = Attrs::new().family(fonts::SANS);
            let spans = block
                .runs
                .iter()
                .map(|r| {
                    let attrs = base
                        .clone()
                        .family(if r.code { fonts::mono() } else { fonts::SANS })
                        .weight(if r.bold || block.heading > 0 {
                            Weight::BOLD
                        } else {
                            Weight::NORMAL
                        })
                        .style(if r.italic {
                            Style::Italic
                        } else {
                            Style::Normal
                        })
                        .metadata(
                            usize::from(r.code)
                                | usize::from(r.link) << 1
                                | usize::from(r.strike) << 2,
                        );
                    (r.text.as_str(), attrs)
                })
                .collect::<Vec<_>>();
            buffer.set_rich_text(spans, &base, Shaping::Advanced, None);
            buffer.shape_until_scroll(&mut self.font_system, false);
            let height = if block.rule {
                24
            } else {
                buffer
                    .layout_runs()
                    .map(|r| r.line_top + r.line_height)
                    .fold(0., f32::max)
                    .ceil() as u32
            };
            let marker = block.marker.as_ref().map(|text| {
                make_sprite(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    vec![(text.as_str(), base.clone())],
                    base.clone(),
                    Metrics::new(size, line_height),
                    (size * 1.1).ceil() as u32,
                    line_height as u32,
                )
            });
            let advance = if block.code || block.rule {
                plan.width
            } else {
                x + buffer.layout_runs().map(|r| r.line_w).fold(0., f32::max)
            };
            shaped.push(ShapedBlock {
                buffer: RefCell::new(buffer),
                x,
                y,
                height,
                advance,
                kind: block,
                marker,
            });
            y += height as f32 + size * 0.45;
        }
        anyhow::ensure!(
            y <= 1800.,
            "rich text exceeds 1800 pixels; split it into separate actors or slides"
        );
        Ok(RichTextGlyphs {
            blocks: shaped,
            width: plan.width.ceil() as u32,
            cached: Default::default(),
            mask: plan
                .vertical_mask
                .map(|[top, bottom, fade]| VerticalMask { top, bottom, fade }),
            fade_blur: plan.fade_blur,
        })
    }

    pub(crate) fn composite_rich_text(
        &mut self,
        pixels: &mut [u8],
        glyphs: &RichTextGlyphs,
        origin: [f32; 2],
        sample: impl Fn(&str, f32) -> f32,
    ) {
        let sprites = glyphs.cached.get(self.theme, || {
            let p = self.theme.palette();
            glyphs
                .blocks
                .iter()
                .map(|b| {
                    let mut buffer = b.buffer.borrow_mut();
                    let mut pixels = vec![0; (glyphs.width * b.height.max(1) * 4) as usize];
                    let mut canvas =
                        ui::card::UiCanvas::new(&mut pixels, [glyphs.width, b.height.max(1)]);
                    let color =
                        |rgb: [u8; 3]| ui::card::UiColor::srgb8(rgb[0], rgb[1], rgb[2], 255);
                    if b.kind.code {
                        canvas.fill(
                            ui::Bounds {
                                origin: [0., 0.],
                                size: [glyphs.width as f32, b.height as f32],
                            },
                            6.,
                            ui::card::Fill::Solid(color(p.surface)),
                            1.,
                        );
                    }
                    if b.kind.quote {
                        canvas.fill(
                            ui::Bounds {
                                origin: [0., 0.],
                                size: [2., b.height as f32],
                            },
                            0.,
                            ui::card::Fill::Solid(color(p.accent)),
                            0.7,
                        );
                    }
                    if b.kind.rule {
                        canvas.fill(
                            ui::Bounds {
                                origin: [0., 12.],
                                size: [glyphs.width as f32, 1.],
                            },
                            0.,
                            ui::card::Fill::Solid(color(p.muted)),
                            0.4,
                        );
                    }
                    for run in buffer.layout_runs() {
                        for g in run.glyphs {
                            if g.metadata & 1 != 0 && !b.kind.code {
                                canvas.fill(
                                    ui::Bounds {
                                        origin: [b.x + g.x, run.line_top + 3.],
                                        size: [g.w, run.line_height - 6.],
                                    },
                                    0.,
                                    ui::card::Fill::Solid(color(p.raised)),
                                    1.,
                                );
                            }
                            if g.metadata & 6 != 0 {
                                canvas.fill(
                                    ui::Bounds {
                                        origin: [
                                            b.x + g.x,
                                            run.line_y
                                                + if g.metadata & 4 != 0 {
                                                    -run.line_height * 0.22
                                                } else {
                                                    3.
                                                },
                                        ],
                                        size: [g.w, 1.2],
                                    },
                                    0.,
                                    ui::card::Fill::Solid(color(if g.metadata & 2 != 0 {
                                        p.accent
                                    } else {
                                        p.text
                                    })),
                                    0.7,
                                );
                            }
                        }
                    }
                    // Metadata keeps styles through shaping and wrapping, then colors
                    // glyphs before coverage blending rather than tinting final pixels.
                    buffer.draw(
                        &mut self.font_system,
                        &mut self.swash_cache,
                        Color::rgb(p.text[0], p.text[1], p.text[2]),
                        |x, y, w, h, c| {
                            paint_rect(
                                &mut pixels,
                                glyphs.width,
                                b.height,
                                x + b.x as i32,
                                y,
                                w,
                                h,
                                [c.r(), c.g(), c.b(), c.a()],
                            );
                        },
                    );
                    if let Some(marker) = &b.marker {
                        let mut marker = marker.clone();
                        self.theme.sprite(&mut marker);
                        composite_text(
                            &mut pixels,
                            [glyphs.width, b.height],
                            TextDraw::new(&marker, [b.x - marker.width as f32, 0.]),
                        );
                    }
                    TextSprite {
                        width: glyphs.width,
                        height: b.height.max(1),
                        advance: b.advance,
                        pixels,
                    }
                })
                .collect()
        });
        let opacity = sample("opacity", 1.).clamp(0., 1.);
        let reveal = sample("reveal", 1.).clamp(0., 1.);
        for (index, (block, sprite)) in glyphs.blocks.iter().zip(sprites.iter()).enumerate() {
            let alpha = opacity * sample(&format!("block.{index}.opacity"), 1.).clamp(0., 1.);
            let y = sample(&format!("block.{index}.y"), block.y);
            composite_text(
                pixels,
                [self.spec.width, self.spec.height],
                TextDraw {
                    clip_width: if reveal >= 1. {
                        sprite.width as f32
                    } else {
                        sprite.advance * reveal
                    },
                    filter: TextFilter::Blur(
                        sample("blur", (1. - alpha) * glyphs.fade_blur).max(0.),
                    ),
                    opacity: alpha,
                    mask: glyphs.mask,
                    ..TextDraw::new(sprite, [origin[0], origin[1] + y])
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires headless GPU; ordinary paragraph fades must use optically sharp glyphs"]
    fn paragraph_fades_are_sharp_by_default() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "paragraph-fade-proof".into(),
        }))
        .unwrap();
        let plan:RichTextPlan=serde_json::from_value(serde_json::json!({"origin":[220.25,330.5],"width":1000,"fontSize":34,"markdown":"A **sharp** paragraph with `inline code`."})).unwrap();
        let glyphs = renderer
            .prepare_rich_text_source(parse(plan.clone()).unwrap())
            .unwrap();
        for opacity in [0.15, 0.4, 0.75, 1., 0.] {
            let mut actual = renderer.render_title_card("", None, 0.);
            let mut expected = actual.clone();
            renderer.composite_rich_text(&mut actual, &glyphs, plan.origin, |name, default| {
                if name == "opacity" { opacity } else { default }
            });
            renderer.composite_rich_text(&mut expected, &glyphs, plan.origin, |name, default| {
                match name {
                    "opacity" => opacity,
                    "blur" => 0.,
                    _ => default,
                }
            });
            assert!(
                actual == expected,
                "paragraph at opacity {opacity} must not inherit title blur"
            );
        }
    }
    #[test]
    fn markdown_keeps_nested_styles_and_block_structure() {
        let b = blocks(
            "# Heading\n\n**bold and *italic*** with `code`\n\n3. one\n4. two\n\n> a quote\n\n---",
        )
        .unwrap();
        assert_eq!(b[0].heading, 1);
        assert!(b[1].runs.iter().any(|r| r.bold && r.italic));
        assert!(b[1].runs.iter().any(|r| r.code && r.text == "code"));
        assert_eq!(b[2].marker.as_deref(), Some("3."));
        assert_eq!(b[3].marker.as_deref(), Some("4."));
        assert!(b[4].quote);
        assert!(b[5].rule);
        assert!(blocks("<script>x</script>").is_err());
        assert!(blocks("![image](https://example.com/x.png)").is_err());
        let nested = blocks("**outer __inner__ retained** plain").unwrap();
        assert!(
            nested[0]
                .runs
                .iter()
                .filter(|r| r.text.contains("retained"))
                .all(|r| r.bold)
        );
        assert!(!nested[0].runs.last().unwrap().bold);
        let list = blocks("- outer\n  - **nested**\n- back").unwrap();
        assert_eq!(
            list.iter().map(|b| b.indent).collect::<Vec<_>>(),
            vec![1, 2, 1]
        );
    }
}
