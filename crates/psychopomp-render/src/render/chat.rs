//! Chat Thread pixels: the window, its header and composer, avatars, names,
//! timestamps and badges, wrapped sans-serif text with code chips, bubbles
//! that grow out of typing indicators, streamed text, highlights, and
//! reaction pills. Wrapping is measured once at preparation; colored sprites
//! are cached per theme. The layout comes from `ChatPlan::layout`.
use std::collections::HashMap;

use anyhow::Result;
use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, Weight, Wrap};
use psychopomp::{
    chat::{
        ChatBlock, ChatChannel, ChatGeometry, ChatMetrics, ChatPlan, ChatPose, ChatSpanPlan,
        ChatStyle, message_property, reaction_property, typing_dot,
    },
    math::{lerp, remap_clamp, smoothstep},
    tone::Tone,
};

use super::{
    HeadlessRenderer, TextDraw, TextSprite, composite_text, fonts, make_sprite, paint_rect,
    theme::{Palette, ThemedCache, mix},
    ui::{
        Bounds,
        card::{UiCanvas, UiColor},
    },
    window::{ShellCache, WindowChrome, WindowPose, clipped, solid},
};

/// Code spans are set a little smaller than the prose around them.
const CODE_SCALE: f32 = 0.88;
/// A message rises into its slot from this far below.
const RISE: f32 = 10.0;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Label {
    Title,
    Subtitle,
    Composer,
    Name,
    Badge,
    Time,
    Initials,
    Reaction,
}

impl Label {
    /// Font size in text sizes, and weight.
    fn style(self) -> (f32, Weight) {
        match self {
            Self::Title => (1.0, Weight::BOLD),
            Self::Subtitle => (0.72, Weight::NORMAL),
            Self::Composer => (0.92, Weight::NORMAL),
            Self::Name => (1.0, Weight::BOLD),
            Self::Badge => (0.56, Weight::BOLD),
            Self::Time => (0.7, Weight::NORMAL),
            Self::Initials => (0.72, Weight::BOLD),
            Self::Reaction => (0.78, Weight::NORMAL),
        }
    }
}

/// One wrapped line of a message: its top and height in the text block, and
/// the right edge of the text after each character count.
struct BodyLine {
    top: f32,
    height: f32,
    stops: Vec<(usize, f32)>,
}

struct ShapedBody {
    width: f32,
    height: f32,
    lines: Vec<BodyLine>,
    chars: usize,
}

/// A chat's theme-independent text measurements and its per-theme sprites.
pub(crate) struct ChatGlyphs {
    bodies: Vec<ShapedBody>,
    metrics: Vec<ChatMetrics>,
    names: ChatNames,
    sprites: ThemedCache<ChatSprites>,
    shell: ShellCache,
}

struct ChatSprites {
    bodies: Vec<Option<TextSprite>>,
    labels: HashMap<(Label, String), TextSprite>,
}

/// Channel property names, formatted once.
struct ChatNames {
    messages: Vec<[String; 5]>,
    reactions: Vec<Vec<String>>,
}

/// The text as shaped: each code span is padded by a thin space on either
/// side, inside its chip. `None` marks padding.
fn segments(spans: &[ChatSpanPlan]) -> Vec<(&str, Option<&ChatSpanPlan>)> {
    let mut segments = Vec::new();
    for span in spans {
        if span.code {
            segments.push((PAD, None));
            segments.push((span.text.as_str(), Some(span)));
            segments.push((PAD, None));
        } else {
            segments.push((span.text.as_str(), Some(span)));
        }
    }
    segments
}

const PAD: &str = "\u{2009}";

fn shape(
    fonts: &mut FontSystem,
    plan: &ChatPlan,
    spans: &[ChatSpanPlan],
    color: impl Fn(&ChatSpanPlan) -> [u8; 3],
) -> Buffer {
    let geometry = plan.geometry();
    let size = plan.text_size;
    let mut buffer = Buffer::new(fonts, Metrics::new(size, geometry.line));
    buffer.set_size(Some(geometry.wrap), None);
    buffer.set_wrap(Wrap::WordOrGlyph);
    let base = Attrs::new().family(fonts::SANS);
    let rich = segments(spans)
        .into_iter()
        .map(|(text, span)| {
            let Some(span) = span else {
                return (text, base.clone().metadata(1));
            };
            let [r, g, b] = color(span);
            let attrs = base.clone().color(Color::rgb(r, g, b));
            let attrs = if span.code {
                attrs
                    .family(fonts::mono())
                    .metrics(Metrics::new(size * CODE_SCALE, geometry.line))
                    .metadata(1)
            } else {
                attrs
            };
            (text, attrs)
        })
        .collect::<Vec<_>>();
    buffer.set_rich_text(rich, &base, Shaping::Advanced, None);
    buffer.shape_until_scroll(fonts, false);
    buffer
}

fn measure(buffer: &Buffer, spans: &[ChatSpanPlan]) -> ShapedBody {
    let segments = segments(spans);
    let text = segments.iter().map(|(text, _)| *text).collect::<String>();
    // Padding characters before each shaped character, so stops count the
    // authored text that `typed` reveals.
    let mut pads = vec![0];
    for (segment, span) in &segments {
        for _ in segment.chars() {
            let last = *pads.last().expect("starts at zero");
            pads.push(last + usize::from(span.is_none()));
        }
    }
    // Shaped characters before each paragraph, counting the newlines.
    let mut starts = Vec::new();
    let mut count = 0;
    for paragraph in text.split('\n') {
        starts.push(count);
        count += paragraph.chars().count() + 1;
    }
    let mut lines = Vec::new();
    let mut width: f32 = 0.0;
    let mut height: f32 = 0.0;
    for run in buffer.layout_runs() {
        let start = starts.get(run.line_i).copied().unwrap_or(0);
        let mut stops = run
            .glyphs
            .iter()
            .map(|glyph| {
                let shaped = start + run.text[..glyph.end.min(run.text.len())].chars().count();
                let shaped = shaped.min(pads.len() - 1);
                (shaped - pads[shaped], glyph.x + glyph.w)
            })
            .collect::<Vec<_>>();
        stops.sort_by_key(|&(chars, _)| chars);
        width = width.max(run.line_w);
        height = height.max(run.line_top + run.line_height);
        lines.push(BodyLine {
            top: run.line_top,
            height: run.line_height,
            stops,
        });
    }
    let chars = spans
        .iter()
        .map(|span| span.text.chars().count())
        .sum::<usize>();
    ShapedBody {
        width: width.ceil(),
        height: if chars == 0 { 0.0 } else { height.ceil() },
        lines,
        chars,
    }
}

fn contrast_ink(fill: [u8; 3], palette: &Palette) -> [u8; 3] {
    let luminance =
        (0.2126 * f32::from(fill[0]) + 0.7152 * f32::from(fill[1]) + 0.0722 * f32::from(fill[2]))
            / 255.0;
    if luminance > 0.7 {
        mix(palette.background, [0; 3], 0.4)
    } else {
        [250, 250, 250]
    }
}

impl HeadlessRenderer {
    /// Shape every message once, without colors, for the layout's metrics.
    pub(crate) fn prepare_chat(&mut self, plan: &ChatPlan) -> ChatGlyphs {
        let geometry = plan.geometry();
        let bodies = plan
            .messages
            .iter()
            .map(|message| {
                let buffer = shape(&mut self.font_system, plan, &message.spans, |_| [0; 3]);
                measure(&buffer, &message.spans)
            })
            .collect::<Vec<_>>();
        let named = plan.style == ChatStyle::Slack || plan.people.len() > 2;
        let metrics = plan
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                let header = if plan.group_start(index) && (named && !plan.mine(index)) {
                    geometry.name_row
                } else {
                    0.0
                };
                let text = bodies[index].height;
                let body = match plan.style {
                    ChatStyle::Slack => text,
                    ChatStyle::Bubbles if text > 0.0 => text + geometry.bubble_pad[1] * 2.0,
                    ChatStyle::Bubbles => 0.0,
                };
                ChatMetrics {
                    header,
                    body,
                    reactions: if message.reactions.is_empty() {
                        0.0
                    } else {
                        geometry.reactions
                    },
                }
            })
            .collect();
        let names = ChatNames {
            messages: plan
                .messages
                .iter()
                .map(|message| {
                    ChatChannel::ALL.map(|channel| message_property(&message.id, channel))
                })
                .collect(),
            reactions: plan
                .messages
                .iter()
                .map(|message| {
                    message
                        .reactions
                        .iter()
                        .map(|reaction| reaction_property(&message.id, &reaction.id))
                        .collect()
                })
                .collect(),
        };
        ChatGlyphs {
            bodies,
            metrics,
            names,
            sprites: ThemedCache::default(),
            shell: ShellCache::default(),
        }
    }

    fn chat_sprites(&mut self, plan: &ChatPlan, glyphs: &ChatGlyphs) -> ChatSprites {
        let palette = self.theme.palette();
        let theme = self.theme;
        let bodies = plan
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                let shaped = &glyphs.bodies[index];
                if shaped.height <= 0.0 {
                    return None;
                }
                let ink = if plan.mine(index) {
                    contrast_ink(theme.tone(Tone::Request), &palette)
                } else {
                    palette.text
                };
                let mut buffer =
                    shape(
                        &mut self.font_system,
                        plan,
                        &message.spans,
                        |span| match span.tone {
                            Tone::Plain => ink,
                            tone => theme.tone(tone),
                        },
                    );
                let size = [shaped.width as u32 + 8, shaped.height as u32 + 2];
                let mut pixels = vec![0_u8; size[0] as usize * size[1] as usize * 4];
                {
                    let mut canvas = UiCanvas::new(&mut pixels, size);
                    let chip = mix(palette.raised, palette.muted, 0.12);
                    for run in buffer.layout_runs() {
                        let mut span: Option<(f32, f32)> = None;
                        let flush = |canvas: &mut UiCanvas<'_>, span: (f32, f32)| {
                            canvas.fill(
                                Bounds {
                                    origin: [span.0 + 3.0, run.line_top + 3.0],
                                    size: [span.1 - span.0, run.line_height - 6.0],
                                },
                                4.0,
                                solid(chip),
                                1.0,
                            );
                        };
                        for glyph in run.glyphs {
                            if glyph.metadata & 1 == 0 {
                                if let Some(done) = span.take() {
                                    flush(&mut canvas, done);
                                }
                                continue;
                            }
                            span = Some(match span {
                                Some((from, _)) => (from, glyph.x + glyph.w),
                                None => (glyph.x, glyph.x + glyph.w),
                            });
                        }
                        if let Some(done) = span {
                            flush(&mut canvas, done);
                        }
                    }
                }
                let [r, g, b] = ink;
                buffer.draw(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    Color::rgb(r, g, b),
                    |x, y, w, h, color| {
                        paint_rect(
                            &mut pixels,
                            size[0],
                            size[1],
                            x + 3,
                            y,
                            w,
                            h,
                            [color.r(), color.g(), color.b(), color.a()],
                        );
                    },
                );
                Some(TextSprite {
                    width: size[0],
                    height: size[1],
                    advance: shaped.width,
                    pixels,
                })
            })
            .collect();
        let mut wanted: Vec<(Label, String, [u8; 3])> = Vec::new();
        if let Some(title) = &plan.title {
            wanted.push((Label::Title, title.clone(), palette.text));
        }
        if let Some(subtitle) = &plan.subtitle {
            wanted.push((Label::Subtitle, subtitle.clone(), palette.muted));
        }
        if let Some(composer) = &plan.composer {
            wanted.push((Label::Composer, composer.clone(), palette.muted));
        }
        for person in &plan.people {
            wanted.push((Label::Name, person.name.clone(), palette.text));
            wanted.push((
                Label::Initials,
                person.avatar_text(),
                contrast_ink(avatar_fill(theme.tone(person.tone), &palette), &palette),
            ));
            if let Some(badge) = &person.badge {
                wanted.push((Label::Badge, badge.clone(), palette.muted));
            }
        }
        for message in &plan.messages {
            if let Some(time) = &message.time {
                wanted.push((Label::Time, time.clone(), palette.muted));
            }
            for reaction in &message.reactions {
                wanted.push((Label::Reaction, reaction.label.clone(), palette.text));
                wanted.push((Label::Reaction, reaction.count.to_string(), palette.text));
            }
        }
        let mut labels = HashMap::new();
        for (label, text, color) in wanted {
            if labels.contains_key(&(label, text.clone())) {
                continue;
            }
            let (scale, weight) = label.style();
            let size = plan.text_size * scale;
            let [r, g, b] = color;
            let attrs = Attrs::new()
                .family(fonts::SANS)
                .weight(weight)
                .color(Color::rgb(r, g, b));
            let mut sprite = make_sprite(
                &mut self.font_system,
                &mut self.swash_cache,
                vec![(text.as_str(), attrs.clone())],
                attrs,
                Metrics::new(size, (size * 1.4).ceil()),
                (size * text.chars().count() as f32 * 1.2 + size * 2.0).ceil() as u32,
                (size * 1.4).ceil() as u32,
            );
            crop(&mut sprite);
            labels.insert((label, text), sprite);
        }
        ChatSprites { bodies, labels }
    }

    pub(crate) fn composite_chat(
        &mut self,
        pixels: &mut [u8],
        plan: &ChatPlan,
        glyphs: &ChatGlyphs,
        sample: impl Fn(&str, f32) -> f32,
    ) -> Result<()> {
        let pose = WindowPose::sample(&sample);
        if !pose.visible() {
            return Ok(());
        }
        let palette = self.theme.palette();
        self.composite_window(
            pixels,
            Bounds {
                origin: plan.origin,
                size: plan.size,
            },
            pose,
            WindowChrome {
                bar: false,
                title: None,
                fill: palette.surface,
                corner_radius: 18.0,
            },
            &glyphs.shell,
        )?;
        let ink = pose.ink();
        if ink <= 0.001 {
            return Ok(());
        }
        let sprites = glyphs
            .sprites
            .get(self.theme, || self.chat_sprites(plan, glyphs));
        let geometry = plan.geometry();
        let origin = pose.place(plan.origin);
        let content = [
            origin[1] + geometry.header,
            origin[1] + plan.size[1] - geometry.composer,
        ];
        let frame = Frame {
            plan,
            sprites: &sprites,
            theme: self.theme,
            palette,
            geometry,
            canvas: [self.spec.width, self.spec.height],
            origin,
            left: origin[0] + geometry.padding,
            right: origin[0] + plan.size[0] - geometry.padding,
            content,
            window: Bounds {
                origin: [origin[0], content[0]],
                size: [plan.size[0], content[1] - content[0]],
            },
        };
        frame.header(pixels, ink);
        frame.composer(pixels, ink);
        // Messages, stacked up from the composer.
        let poses = glyphs
            .names
            .messages
            .iter()
            .zip(&glyphs.names.reactions)
            .map(|(names, reactions)| ChatPose {
                typing: sample(&names[0], 0.0),
                reveal: sample(&names[2], 1.0),
                reactions: reactions
                    .iter()
                    .map(|name| sample(name, 1.0).clamp(0.0, 1.0))
                    .fold(0.0, f32::max),
            })
            .collect::<Vec<_>>();
        let bottom = content[1] - geometry.padding * 0.4;
        for block in plan.layout(&glyphs.metrics, &poses, bottom) {
            if block.room <= 0.0 || block.top + block.room <= content[0] || block.top >= content[1]
            {
                continue;
            }
            let names = &glyphs.names.messages[block.message];
            let pose = poses[block.message];
            let presence = pose.typing.clamp(0.0, 1.0).max(pose.reveal.clamp(0.0, 1.0));
            let message = Message {
                index: block.message,
                block,
                pose,
                shaped: &glyphs.bodies[block.message],
                metrics: glyphs.metrics[block.message],
                alpha: ink * smoothstep(remap_clamp(presence, [0.0, 0.7], [0.0, 1.0])),
                top: block.top + (1.0 - smoothstep(presence)) * RISE,
            };
            if message.alpha <= 0.001 {
                continue;
            }
            frame.highlight(pixels, &message, sample(&names[4], 0.0).clamp(0.0, 1.0));
            frame.author(pixels, &message);
            frame.body(
                pixels,
                &message,
                sample(&names[1], -1.0),
                sample(&names[3], 1.0).clamp(0.0, 1.0),
            );
            let pops = glyphs.names.reactions[block.message]
                .iter()
                .map(|name| sample(name, 1.0))
                .collect::<Vec<_>>();
            frame.reactions(pixels, &message, &pops);
        }
        Ok(())
    }
}

/// One sample's shared context while painting a chat.
struct Frame<'a> {
    plan: &'a ChatPlan,
    sprites: &'a ChatSprites,
    theme: super::Theme,
    palette: Palette,
    geometry: ChatGeometry,
    canvas: [u32; 2],
    origin: [f32; 2],
    left: f32,
    right: f32,
    /// The messages' area, `[top, bottom]`.
    content: [f32; 2],
    window: Bounds,
}

/// One message's sampled block.
struct Message<'a> {
    index: usize,
    block: ChatBlock,
    pose: ChatPose,
    shaped: &'a ShapedBody,
    metrics: ChatMetrics,
    alpha: f32,
    /// The block's top, with its entrance rise.
    top: f32,
}

impl Frame<'_> {
    fn label(&self, kind: Label, text: &str) -> Option<&TextSprite> {
        self.sprites.labels.get(&(kind, text.to_owned()))
    }

    /// A label whose left edge is at `at[0]`, centered on `at[1]`.
    fn draw(&self, pixels: &mut [u8], sprite: &TextSprite, at: [f32; 2], alpha: f32, clip: bool) {
        composite_text(
            pixels,
            self.canvas,
            TextDraw {
                opacity: alpha,
                clip_y: clip.then_some(self.content),
                ..TextDraw::new(
                    sprite,
                    [at[0].round(), (at[1] - sprite.height as f32 * 0.5).round()],
                )
            },
        );
    }

    /// A fill clipped to the messages' area.
    fn fill(&self, pixels: &mut [u8], bounds: Bounds, radius: f32, color: [u8; 3], alpha: f32) {
        clipped(pixels, self.canvas, self.window, 0.0, |canvas| {
            canvas.fill(bounds, radius, solid(color), alpha);
        });
    }

    fn outline(&self, pixels: &mut [u8], bounds: Bounds, radius: f32, alpha: f32) {
        let [r, g, b] = self.palette.raised;
        clipped(pixels, self.canvas, self.window, 0.0, |canvas| {
            canvas.stroke(bounds, radius, 1.2, UiColor::srgb8(r, g, b, 255), alpha);
        });
    }

    /// The channel or conversation name, its subtitle, and a hairline.
    fn header(&self, pixels: &mut [u8], ink: f32) {
        let (plan, geometry) = (self.plan, self.geometry);
        if geometry.header <= 0.0 {
            return;
        }
        let middle = self.origin[1] + geometry.header * 0.5;
        let lines = [
            plan.title
                .as_deref()
                .and_then(|t| self.label(Label::Title, t)),
            plan.subtitle
                .as_deref()
                .and_then(|t| self.label(Label::Subtitle, t)),
        ];
        let offsets = if lines[1].is_some() {
            [-plan.text_size * 0.42, plan.text_size * 0.6]
        } else {
            [0.0, 0.0]
        };
        for (sprite, offset) in lines.into_iter().zip(offsets) {
            if let Some(sprite) = sprite {
                let x = match plan.style {
                    ChatStyle::Bubbles => self.origin[0] + (plan.size[0] - sprite.advance) * 0.5,
                    ChatStyle::Slack => self.left,
                };
                self.draw(pixels, sprite, [x, middle + offset], ink, false);
            }
        }
        UiCanvas::new(pixels, self.canvas).fill(
            Bounds {
                origin: [self.origin[0] + 1.0, self.content[0] - 1.0],
                size: [plan.size[0] - 2.0, 1.0],
            },
            0.0,
            solid(self.palette.raised),
            ink,
        );
    }

    /// The message field and its placeholder.
    fn composer(&self, pixels: &mut [u8], ink: f32) {
        let (plan, geometry) = (self.plan, self.geometry);
        let Some(placeholder) = plan.composer.as_deref() else {
            return;
        };
        let height = geometry.composer - geometry.padding;
        let bounds = Bounds {
            origin: [self.left, self.content[1] + geometry.padding * 0.35],
            size: [self.right - self.left, height],
        };
        let radius = match plan.style {
            ChatStyle::Bubbles => height * 0.5,
            ChatStyle::Slack => 10.0,
        };
        let [r, g, b] = self.palette.raised;
        let mut canvas = UiCanvas::new(pixels, self.canvas);
        canvas.fill(
            bounds,
            radius,
            solid(mix(self.palette.surface, self.palette.background, 0.45)),
            ink,
        );
        canvas.stroke(bounds, radius, 1.25, UiColor::srgb8(r, g, b, 255), ink);
        if let Some(sprite) = self.label(Label::Composer, placeholder) {
            let at = [bounds.origin[0] + plan.text_size * 0.8, bounds.center()[1]];
            self.draw(pixels, sprite, at, ink, false);
        }
    }

    /// Where a message's text column starts (others) or ends (yours).
    fn text_x(&self, message: &Message<'_>) -> f32 {
        if self.plan.mine(message.index) {
            self.right
        } else {
            self.left + self.geometry.indent
        }
    }

    /// A wash and accent bar behind the whole block.
    fn highlight(&self, pixels: &mut [u8], message: &Message<'_>, highlight: f32) {
        if highlight <= 0.001 {
            return;
        }
        let gap = self.geometry.gaps[1];
        let bounds = Bounds {
            origin: [self.origin[0] + 6.0, message.block.top - gap * 0.5 - 4.0],
            size: [self.plan.size[0] - 12.0, message.block.room + gap + 8.0],
        };
        let accent = self.palette.accent;
        self.fill(pixels, bounds, 8.0, accent, message.alpha * highlight * 0.1);
        let bar = Bounds {
            origin: [bounds.origin[0], bounds.origin[1] + 4.0],
            size: [3.0, bounds.size[1] - 8.0],
        };
        self.fill(pixels, bar, 1.5, accent, message.alpha * highlight);
    }

    /// The avatar and, on the first message of a run, the name row: name,
    /// badge, and (in Slack) the timestamp.
    fn author(&self, pixels: &mut [u8], message: &Message<'_>) {
        let (plan, geometry, alpha) = (self.plan, self.geometry, message.alpha);
        let index = message.index;
        let person = plan
            .person(&plan.messages[index].author)
            .expect("validated author");
        if plan.group_start(index) && !plan.mine(index) {
            let (offset, radius) = match plan.style {
                ChatStyle::Slack => (3.0, geometry.avatar * 0.22),
                ChatStyle::Bubbles => (message.metrics.header, geometry.avatar * 0.5),
            };
            let avatar = Bounds {
                origin: [self.left, message.top + offset],
                size: [geometry.avatar; 2],
            };
            let fill = avatar_fill(self.theme.tone(person.tone), &self.palette);
            self.fill(pixels, avatar, radius, fill, alpha);
            if let Some(sprite) = self.label(Label::Initials, &person.avatar_text()) {
                let center = avatar.center();
                self.draw(
                    pixels,
                    sprite,
                    [center[0] - sprite.advance * 0.5, center[1]],
                    alpha,
                    true,
                );
            }
        }
        if message.metrics.header <= 0.0 {
            return;
        }
        let middle = message.top + message.metrics.header * 0.5;
        let mut x = self.text_x(message);
        let mut quiet = 1.0;
        if plan.style == ChatStyle::Bubbles {
            x += geometry.bubble_pad[0];
            quiet = 0.7;
        }
        if let Some(sprite) = self.label(Label::Name, &person.name) {
            self.draw(pixels, sprite, [x, middle], alpha * quiet, true);
            x += sprite.advance + plan.text_size * 0.4;
        }
        if let Some(sprite) = person
            .badge
            .as_deref()
            .and_then(|b| self.label(Label::Badge, b))
        {
            let chip = Bounds {
                origin: [x, middle - plan.text_size * 0.45],
                size: [sprite.advance + plan.text_size * 0.6, plan.text_size * 0.9],
            };
            self.fill(pixels, chip, 4.0, self.palette.raised, alpha);
            let at = [chip.origin[0] + plan.text_size * 0.3, middle];
            self.draw(pixels, sprite, at, alpha, true);
            x = chip.right() + plan.text_size * 0.4;
        }
        if plan.style == ChatStyle::Slack
            && let Some(sprite) = plan.messages[index]
                .time
                .as_deref()
                .and_then(|t| self.label(Label::Time, t))
        {
            self.draw(pixels, sprite, [x, middle + 1.0], alpha, true);
        }
    }

    /// The body: a typing indicator whose dots run on `wait`, growing into
    /// the message (a bubble, in that style) whose text shows by `typed`.
    fn body(&self, pixels: &mut [u8], message: &Message<'_>, wait: f32, typed: f32) {
        let (plan, geometry, palette) = (self.plan, self.geometry, self.palette);
        let (shaped, alpha) = (message.shaped, message.alpha);
        let mine = plan.mine(message.index);
        let typing = message.pose.typing.clamp(0.0, 1.0);
        let reveal = message.pose.reveal.max(0.0);
        let text_x = self.text_x(message);
        let top = message.top + message.metrics.header;
        let dots = typing * (1.0 - smoothstep(remap_clamp(reveal, [0.0, 0.3], [0.0, 1.0])));
        let indicator = [plan.text_size * 3.3, plan.typing_body()];
        let full = match plan.style {
            ChatStyle::Slack => [shaped.width, shaped.height],
            ChatStyle::Bubbles => [
                shaped.width + geometry.bubble_pad[0] * 2.0,
                shaped.height + geometry.bubble_pad[1] * 2.0,
            ],
        };
        let grow = reveal.clamp(0.0, 1.06);
        let size = [
            lerp(indicator[0], full[0], grow).max(0.0),
            lerp(indicator[1], full[1], grow).max(0.0),
        ];
        let bubble = Bounds {
            origin: [if mine { text_x - size[0] } else { text_x }, top],
            size,
        };
        let pill = Bounds {
            origin: [text_x, top],
            size: indicator,
        };
        let mine_fill = self.theme.tone(Tone::Request);
        let other_fill = mix(palette.surface, palette.raised, 0.9);
        let surface = match plan.style {
            ChatStyle::Bubbles if shaped.height > 0.0 || dots > 0.001 => Some((
                bubble,
                if mine { mine_fill } else { other_fill },
                (geometry.line * 0.5 + geometry.bubble_pad[1]).min(size[1] * 0.5),
                typing.max(smoothstep(remap_clamp(reveal, [0.0, 0.3], [0.0, 1.0]))),
            )),
            ChatStyle::Slack if dots > 0.001 => Some((pill, other_fill, indicator[1] * 0.5, dots)),
            _ => None,
        };
        if let Some((bounds, fill, radius, presence)) = surface {
            self.fill(pixels, bounds, radius, fill, alpha * presence);
        }
        if dots > 0.001 {
            let center = match plan.style {
                ChatStyle::Slack => pill.center(),
                ChatStyle::Bubbles => bubble.center(),
            };
            let dot = plan.text_size * 0.3;
            let base = if mine {
                contrast_ink(mine_fill, &palette)
            } else {
                palette.muted
            };
            for k in 0..3 {
                let (lift, glow) = typing_dot(wait, k);
                let at = [
                    center[0] + (k as f32 - 1.0) * dot * 2.1,
                    center[1] - lift * dot * 0.8,
                ];
                let color = mix(base, palette.text, glow * 0.6);
                self.fill(
                    pixels,
                    Bounds::from_center(at, [dot, dot]),
                    dot * 0.5,
                    color,
                    alpha * dots * glow,
                );
            }
        }
        let text_alpha = alpha * smoothstep(remap_clamp(reveal, [0.22, 0.7], [0.0, 1.0]));
        let Some(sprite) = &self.sprites.bodies[message.index] else {
            return;
        };
        if text_alpha <= 0.001 {
            return;
        }
        // Text sits where the settled bubble puts it and shows only inside
        // the bubble as it grows.
        let (at, rows, columns) = match plan.style {
            ChatStyle::Slack => (
                [text_x - 3.0, top],
                self.content,
                [f32::NEG_INFINITY, f32::INFINITY],
            ),
            ChatStyle::Bubbles => {
                let settled = if mine { text_x - full[0] } else { text_x };
                (
                    [
                        settled + geometry.bubble_pad[0] - 3.0,
                        top + geometry.bubble_pad[1],
                    ],
                    [
                        self.content[0].max(bubble.origin[1]),
                        self.content[1].min(bubble.bottom()),
                    ],
                    [bubble.origin[0], bubble.right()],
                )
            }
        };
        let at = [at[0].round(), at[1].round()];
        let shown = if typed >= 1.0 {
            usize::MAX
        } else {
            ((typed * shaped.chars as f32) + 1e-3).floor() as usize
        };
        for line in &shaped.lines {
            let visible = line
                .stops
                .iter()
                .filter(|(chars, _)| *chars <= shown)
                .map(|(_, x)| *x)
                .fold(0.0_f32, f32::max);
            if visible <= 0.0 {
                continue;
            }
            let row = [at[1] + line.top, at[1] + line.top + line.height];
            let whole = line.stops.last().is_none_or(|(chars, _)| *chars <= shown);
            let end = at[0]
                + if whole {
                    sprite.width as f32
                } else {
                    visible + 3.0
                };
            let source_left = (columns[0] - at[0]).max(0.0);
            let width = end.min(columns[1]) - (at[0] + source_left);
            if width <= 0.0 {
                continue;
            }
            composite_text(
                pixels,
                self.canvas,
                TextDraw {
                    source_left,
                    clip_width: width,
                    opacity: text_alpha,
                    clip_y: Some([row[0].max(rows[0]), row[1].min(rows[1])]),
                    ..TextDraw::new(sprite, [at[0] + source_left, at[1]])
                },
            );
        }
    }

    /// Reaction pills beneath the body, each popping in on its own channel.
    fn reactions(&self, pixels: &mut [u8], message: &Message<'_>, pops: &[f32]) {
        let (plan, geometry, palette) = (self.plan, self.geometry, self.palette);
        let reactions = &plan.messages[message.index].reactions;
        if reactions.is_empty() {
            return;
        }
        let mine = plan.mine(message.index);
        let top =
            message.top + message.metrics.header + message.metrics.body + geometry.reactions * 0.16;
        let height = geometry.reactions * 0.72;
        let gap = plan.text_size * 0.3;
        let mut x = if mine {
            self.right
        } else {
            self.text_x(message)
        };
        let mut order = (0..reactions.len()).collect::<Vec<_>>();
        if mine {
            order.reverse();
        }
        for index in order {
            let (reaction, pop) = (&reactions[index], pops[index]);
            let (Some(glyph), Some(count)) = (
                self.label(Label::Reaction, &reaction.label),
                self.label(Label::Reaction, &reaction.count.to_string()),
            ) else {
                continue;
            };
            if pop <= 0.001 {
                continue;
            }
            let width = glyph.advance + gap + count.advance + plan.text_size;
            let pill = Bounds {
                origin: [if mine { x - width } else { x }, top],
                size: [width, height],
            };
            let scale = lerp(0.55, 1.0, pop.max(0.0));
            let shown = Bounds::from_center(pill.center(), [width * scale, height * scale]);
            let presence = smoothstep(pop.clamp(0.0, 1.0) / 0.4) * message.alpha;
            let radius = shown.size[1] * 0.5;
            self.fill(
                pixels,
                shown,
                radius,
                mix(palette.surface, palette.raised, 0.75),
                presence,
            );
            self.outline(pixels, shown, radius, presence);
            let text = presence * smoothstep(remap_clamp(pop, [0.5, 1.0], [0.0, 1.0]));
            let middle = pill.center()[1];
            let pen = pill.origin[0] + plan.text_size * 0.5;
            self.draw(pixels, glyph, [pen, middle], text, true);
            self.draw(
                pixels,
                count,
                [pen + glyph.advance + gap, middle],
                text,
                true,
            );
            x = if mine {
                pill.origin[0] - gap
            } else {
                pill.right() + gap
            };
        }
    }
}

/// An avatar's tile: the person's tone, a little softened.
fn avatar_fill(tone: [u8; 3], palette: &Palette) -> [u8; 3] {
    mix(tone, palette.surface, 0.12)
}

/// Crop a sprite's transparent right side to its advance.
fn crop(sprite: &mut TextSprite) {
    let width = (sprite.advance.ceil() as u32 + 2).min(sprite.width).max(1);
    if width == sprite.width {
        return;
    }
    let mut cropped = vec![0_u8; width as usize * sprite.height as usize * 4];
    for y in 0..sprite.height as usize {
        let from = y * sprite.width as usize * 4;
        let to = y * width as usize * 4;
        cropped[to..to + width as usize * 4]
            .copy_from_slice(&sprite.pixels[from..from + width as usize * 4]);
    }
    sprite.width = width;
    sprite.pixels = cropped;
}

#[cfg(test)]
mod gpu_tests {
    use psychopomp::{
        chat::{ChatMessagePlan, ChatPersonPlan, ChatPlan, ChatSpanPlan},
        tone::Tone,
    };

    use crate::render::{HeadlessRenderer, RenderSpec};

    #[test]
    #[ignore = "requires a headless GPU; a new message pushes older ones up and hidden chats leave no ink"]
    fn a_new_message_lifts_the_thread_and_sampling_is_deterministic() {
        let mut renderer = pollster::block_on(HeadlessRenderer::new(RenderSpec {
            width: 1920,
            height: 1080,
            file_name: "chat-proof".into(),
        }))
        .unwrap();
        let mut plan = ChatPlan::new(
            [460.0, 120.0],
            [1000.0, 800.0],
            vec![ChatPersonPlan::new("dax", "Dax", Tone::Accent)],
        );
        plan.messages = ["first", "second"]
            .iter()
            .enumerate()
            .map(|(i, text)| ChatMessagePlan {
                id: format!("m{i}"),
                author: "dax".into(),
                spans: vec![ChatSpanPlan::plain(*text)],
                time: None,
                reactions: Vec::new(),
            })
            .collect();
        let glyphs = renderer.prepare_chat(&plan);
        let background = renderer.render_title_card("", None, 0.0);
        let mut draw = |opacity: f32, second: f32| {
            let mut pixels = background.clone();
            renderer
                .composite_chat(&mut pixels, &plan, &glyphs, |property, d| match property {
                    "opacity" => opacity,
                    "message.m1.reveal" => second,
                    _ => d,
                })
                .unwrap();
            pixels
        };
        assert!(draw(0.0, 1.0) == background, "hidden chats leave no ink");
        let alone = draw(1.0, 0.0);
        let both = draw(1.0, 1.0);
        // The first message's ink rises when the second takes the bottom slot.
        let ink_rows = |pixels: &[u8]| {
            (0..1080)
                .filter(|y| {
                    (530..1300).any(|x| {
                        let i = (y * 1920 + x) * 4;
                        pixels[i] > 200
                    })
                })
                .collect::<Vec<_>>()
        };
        assert!(ink_rows(&both)[0] < ink_rows(&alone)[0]);
        assert!(
            draw(1.0, 0.6) == draw(1.0, 0.6),
            "sampling is deterministic"
        );
    }
}
