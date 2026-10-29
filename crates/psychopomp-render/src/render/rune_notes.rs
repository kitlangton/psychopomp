//! State-free Rune renderer for Seer communication primitives (`facts`, `stats`,
//! `compare`, `steps`, `size`, `md_code` with `title="…"` and `{3,5-6}` marked
//! lines, and `> [!note]` callouts) layered over `rune_kit::Markdown` (prose,
//! syntax-coloured code, GFM tables, and native `mermaid` diagrams).

use std::ops::RangeInclusive;

use rune::{Color, El, Family, TextStyle, col, el, row, spacer, text};
use rune_kit::{Kit, Markdown};

const CAPS: TextStyle = TextStyle {
    features: "'tnum' 1",
    ..TextStyle::new(11.0, 560.0).lh(18.0).tracking(0.5)
};
const CELL: TextStyle = TextStyle {
    features: "'tnum' 1",
    ..TextStyle::new(13.5, 430.0).lh(20.0).tracking(-0.1)
};
const CODE: TextStyle = TextStyle {
    family: Family::Mono,
    features: "'tnum' 1",
    ..TextStyle::new(12.2, 430.0).lh(19.0)
};

#[derive(Clone, Debug, PartialEq)]
enum Piece {
    Md(String),
    Facts(Vec<(String, String)>),
    Compare(Vec<(String, String)>),
    Steps(Vec<StepRow>),
    Size(SizeBar),
    CodeFence {
        lang: String,
        title: Option<String>,
        marks: Vec<RangeInclusive<usize>>,
        lines: Vec<String>,
    },
    Callout {
        label: Option<String>,
        body: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct StepRow {
    status: String,
    title: String,
    note: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct SizeBar {
    parts: Vec<(String, i64, i64)>,
    files: Option<String>,
}

pub(crate) fn render_note(key_prefix: &str, src: &str, k: &Kit) -> El {
    let pieces = cut(src);
    let mut root = col()
        .key(key_prefix)
        .w_full()
        .gap(12.0)
        .motion(rune::Spring::new(0.34, 0.0));
    for (idx, piece) in pieces.into_iter().enumerate() {
        let key = format!("{key_prefix}/p{idx}");
        let child = match piece {
            Piece::Md(md) => Markdown::new(&md).build(k).key(&key),
            Piece::Facts(rows) => draw_facts(&key, &rows, k),
            Piece::Compare(sides) => draw_compare(&key, &sides, k),
            Piece::Steps(steps) => draw_steps(&key, &steps, k),
            Piece::Size(size) => draw_size(&key, &size, k),
            Piece::CodeFence {
                lang: _,
                title,
                marks,
                lines,
            } => draw_code(&key, title.as_deref(), &marks, &lines, k),
            Piece::Callout { label, body } => draw_callout(&key, label.as_deref(), &body, k),
        }
        .motion(rune::Spring::new(0.32, 0.0))
        .transition(rune::Transition::opacity());
        root = root.child(child);
    }
    root
}

fn cut(src: &str) -> Vec<Piece> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    let mut md: Vec<&str> = Vec::new();
    let flush_md = |md: &mut Vec<&str>, out: &mut Vec<Piece>| {
        let joined = md.join("\n");
        if !joined.trim().is_empty() {
            out.push(Piece::Md(joined.trim_matches('\n').to_owned()));
        }
        md.clear();
    };

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        // `> [!note]` or `> [!Label]` callouts
        if indent < 4 && trimmed.starts_with('>') {
            let after_gt = trimmed[1..].trim_start();
            if let Some(rest) = after_gt.strip_prefix("[!")
                && let Some(close) = rest.find(']')
            {
                let raw_label = rest[..close].trim();
                let label = (!raw_label.eq_ignore_ascii_case("note") && !raw_label.is_empty())
                    .then(|| raw_label.to_owned());
                let mut body_lines = Vec::new();
                let inline_tail = rest[close + 1..].trim();
                if !inline_tail.is_empty() {
                    body_lines.push(inline_tail.to_owned());
                }
                i += 1;
                while i < lines.len() {
                    let next = lines[i].trim_start();
                    if let Some(stripped) = next.strip_prefix('>') {
                        body_lines.push(stripped.strip_prefix(' ').unwrap_or(stripped).to_owned());
                        i += 1;
                    } else {
                        break;
                    }
                }
                flush_md(&mut md, &mut out);
                out.push(Piece::Callout {
                    label,
                    body: body_lines.join("\n"),
                });
                continue;
            }
        }

        if indent < 4 && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
            let ch = trimmed.chars().next().unwrap();
            let ticks = trimmed.len() - trimmed.trim_start_matches(ch).len();
            let info_raw = trimmed[ticks..].trim();
            let tag = info_raw
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            let end = (i + 1..lines.len()).find(|&j| {
                let u = lines[j].trim_start();
                u.starts_with(&ch.to_string().repeat(ticks))
                    && u.trim_start_matches(ch).trim().is_empty()
            });
            if let Some(end) = end {
                let body = &lines[i + 1..end];
                let special = match tag.as_str() {
                    "facts" | "kv" | "stats" | "stat" => Some(Piece::Facts(parse_pairs(body))),
                    "compare" | "before-after" => Some(Piece::Compare(parse_pairs(body))),
                    "steps" | "status" => Some(Piece::Steps(parse_steps(body))),
                    "size" => Some(Piece::Size(parse_size(body))),
                    _ if info_raw.contains("title=") || info_raw.contains('{') => {
                        let (title, marks) = parse_code_meta(info_raw);
                        Some(Piece::CodeFence {
                            lang: tag,
                            title,
                            marks,
                            lines: body.iter().map(|s| (*s).to_owned()).collect(),
                        })
                    }
                    _ => None,
                };
                if let Some(piece) = special {
                    flush_md(&mut md, &mut out);
                    out.push(piece);
                    i = end + 1;
                    continue;
                }
            }
        }

        md.push(line);
        i += 1;
    }
    flush_md(&mut md, &mut out);
    out
}

fn parse_pairs(lines: &[&str]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for &raw in lines {
        let line = raw.trim().trim_start_matches("- ").trim();
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            out.push((k.trim().to_owned(), v.trim().to_owned()));
        } else if let Some(last) = out.last_mut() {
            last.1.push(' ');
            last.1.push_str(line);
        }
    }
    out
}

fn parse_steps(lines: &[&str]) -> Vec<StepRow> {
    let mut out = Vec::new();
    for &raw in lines {
        let mut s = raw.trim().trim_start_matches("- ").trim();
        if s.is_empty() {
            continue;
        }
        let status = if let Some(rest) = s.strip_prefix('{')
            && let Some(end) = rest.find('}')
        {
            s = rest[end + 1..].trim();
            rest[..end].trim().to_owned()
        } else {
            "proposed".to_owned()
        };
        let (title, note) = match s.split_once(" · ") {
            Some((t, n)) => (t.trim().to_owned(), Some(n.trim().to_owned())),
            None => (s.to_owned(), None),
        };
        out.push(StepRow {
            status,
            title,
            note,
        });
    }
    out
}

fn parse_size(lines: &[&str]) -> SizeBar {
    let joined = lines.join(" ");
    let mut parts = Vec::new();
    let mut files = None;
    for chunk in joined.split(" · ") {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        if chunk.ends_with("files") || chunk.ends_with("file") {
            files = Some(chunk.to_owned());
            continue;
        }
        let mut tokens = chunk.split_whitespace();
        let Some(name) = tokens.next() else {
            continue;
        };
        let mut adds = 0_i64;
        let mut dels = 0_i64;
        for tok in tokens {
            if let Some(a) = tok.strip_prefix('+') {
                adds = a.replace(',', "").parse().unwrap_or(0);
            } else if let Some(d) = tok.strip_prefix('−').or_else(|| tok.strip_prefix('-')) {
                dels = d.replace(',', "").parse().unwrap_or(0);
            }
        }
        parts.push((name.to_owned(), adds, dels));
    }
    SizeBar { parts, files }
}

fn parse_code_meta(info: &str) -> (Option<String>, Vec<RangeInclusive<usize>>) {
    let title = info.find("title=\"").and_then(|start| {
        let rest = &info[start + 7..];
        let end = rest.find('"')?;
        Some(rest[..end].to_owned())
    });
    let mut marks = Vec::new();
    if let Some(open) = info.rfind('{')
        && let Some(close) = info[open..].find('}')
    {
        for spec in info[open + 1..open + close].split(',') {
            let spec = spec.trim();
            if let Some((a, b)) = spec.split_once('-') {
                if let (Ok(lo), Ok(hi)) = (a.trim().parse::<usize>(), b.trim().parse::<usize>()) {
                    marks.push(lo..=hi);
                }
            } else if let Ok(line) = spec.parse::<usize>() {
                marks.push(line..=line);
            }
        }
    }
    (title, marks)
}

fn status_color(status: &str, k: &Kit) -> Color {
    let lower = status.to_ascii_lowercase();
    let c = &k.c;
    if matches!(
        lower.as_str(),
        "built" | "done" | "shipped" | "merged" | "verified" | "green" | "good"
    ) {
        c.success.fg
    } else if matches!(
        lower.as_str(),
        "risk" | "blocked" | "failed" | "red" | "bad" | "error"
    ) {
        c.danger.fg
    } else if matches!(
        lower.as_str(),
        "proposal" | "proposed" | "warn" | "warning" | "hold"
    ) {
        c.warning.fg
    } else if matches!(lower.as_str(), "working" | "on a branch" | "in flight") {
        c.info.fg
    } else {
        c.text_muted
    }
}

fn split_leading_status(val: &str) -> (Option<&str>, &str) {
    let s = val.trim();
    if let Some(rest) = s.strip_prefix('{')
        && let Some(end) = rest.find('}')
    {
        return (Some(rest[..end].trim()), rest[end + 1..].trim());
    }
    (None, s)
}

fn fnv1a(s: &str) -> u32 {
    let mut h = 0x811c_9dc5_u32;
    for &b in s.as_bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn draw_facts(key: &str, rows: &[(String, String)], k: &Kit) -> El {
    let c = &k.c;
    let mut list = col()
        .key(key)
        .w_full()
        .radius(8.0)
        .fill(c.layer_02)
        .border(1.0, c.border_weak)
        .px(14.0)
        .py(6.0);
    for (idx, (label, val)) in rows.iter().enumerate() {
        if idx > 0 {
            list = list.child(el().w_full().h(1.0).fill(c.border_weak));
        }
        let val_key = format!("v-{:08x}", fnv1a(val));
        let (st, rest) = split_leading_status(val);
        let mut right = row()
            .key(val_key)
            .gap(6.0)
            .center_y()
            .ride_trailing()
            .motion(rune::Spring::new(0.28, 0.0))
            .transition(rune::Transition::blur_replace());
        if let Some(st) = st {
            let tone = status_color(st, k);
            right = right
                .child(el().w(7.0).h(7.0).radius(3.5).fill(tone))
                .child(text(st).font(TextStyle::new(12.5, 580.0)).color(tone));
        }
        if !rest.is_empty() {
            right = right.child(text(rest).font(CELL).color(c.text_base));
        }
        list = list.child(
            row()
                .key(format!("fact-{idx}"))
                .w_full()
                .py(7.0)
                .center_y()
                .gap(16.0)
                .child(text(label.to_uppercase()).font(CAPS).color(c.text_muted))
                .child(spacer())
                .child(right),
        );
    }
    list
}

fn draw_compare(key: &str, sides: &[(String, String)], k: &Kit) -> El {
    let c = &k.c;
    let mut grid = row().key(key).w_full().gap(10.0);
    for (idx, (label, body)) in sides.iter().enumerate() {
        let accent = if idx == 0 { c.danger.fg } else { c.success.fg };
        let pane = col()
            .key(format!("side-{idx}"))
            .grow(1.0)
            .shrink()
            .padding(12.0)
            .gap(6.0)
            .radius(8.0)
            .fill(c.layer_02)
            .border(1.0, c.border_weak)
            .child(
                row()
                    .gap(6.0)
                    .center_y()
                    .child(el().w(6.0).h(6.0).radius(3.0).fill(accent))
                    .child(text(label.to_uppercase()).font(CAPS).color(accent)),
            )
            .child(text(body).wrap().font(CELL).color(c.text_base));
        grid = grid.child(pane);
    }
    grid
}

fn draw_steps(key: &str, steps: &[StepRow], k: &Kit) -> El {
    let c = &k.c;
    let built = steps
        .iter()
        .filter(|s| {
            matches!(
                s.status.to_ascii_lowercase().as_str(),
                "built" | "done" | "shipped" | "merged"
            )
        })
        .count();
    let mut wrap = col()
        .key(key)
        .w_full()
        .padding(12.0)
        .gap(8.0)
        .radius(8.0)
        .fill(c.layer_02)
        .border(1.0, c.border_weak)
        .child(
            text(format!("{built} of {} built", steps.len()))
                .font(CAPS)
                .color(c.text_muted),
        );
    for (idx, step) in steps.iter().enumerate() {
        let tone = status_color(&step.status, k);
        let done = matches!(
            step.status.to_ascii_lowercase().as_str(),
            "built" | "done" | "shipped" | "merged"
        );
        let dot = if done {
            el().w(9.0).h(9.0).radius(4.5).fill(tone)
        } else {
            el().w(9.0).h(9.0).radius(4.5).border(1.5, tone)
        };
        let mut r = row()
            .key(format!("step-{idx}"))
            .w_full()
            .gap(9.0)
            .center_y()
            .child(dot)
            .child(
                text(&step.title)
                    .font(TextStyle::new(13.5, 540.0))
                    .color(c.text_base),
            );
        if let Some(note) = &step.note {
            r = r
                .child(spacer())
                .child(text(note).font(k.ty.ui).color(c.text_muted));
        }
        wrap = wrap.child(r);
    }
    wrap
}

fn draw_size(key: &str, size: &SizeBar, k: &Kit) -> El {
    let c = &k.c;
    let totals: Vec<i64> = size
        .parts
        .iter()
        .map(|(_, a, d)| (*a + *d).max(1))
        .collect();
    let grand: i64 = totals.iter().sum::<i64>().max(1);
    let kind_color = |name: &str| match name.to_ascii_lowercase().as_str() {
        "app" | "core" | "render" => c.info.fg,
        "tests" | "test" => c.success.fg,
        "docs" | "doc" => c.warning.fg,
        _ => c.text_muted,
    };

    let mut blocks = row().key("blocks").w_full().gap(3.0);
    let mut allocated = 0usize;
    for (idx, ((name, _, _), &part)) in size.parts.iter().zip(&totals).enumerate() {
        let count = if idx + 1 == size.parts.len() {
            10usize.saturating_sub(allocated).max(1)
        } else {
            ((part as f64 / grand as f64) * 10.0).round() as usize
        }
        .max(1);
        allocated += count;
        let fill = kind_color(name);
        for b in 0..count.min(10) {
            blocks = blocks.child(
                el().key(format!("b-{idx}-{b}"))
                    .grow(1.0)
                    .h(8.0)
                    .radius(2.5)
                    .fill(fill),
            );
        }
    }

    let mut legend = row().key("legend").w_full().gap(12.0).center_y();
    for (name, adds, dels) in &size.parts {
        let fill = kind_color(name);
        legend = legend.child(
            row()
                .gap(5.0)
                .center_y()
                .child(el().w(7.0).h(7.0).radius(2.0).fill(fill))
                .child(
                    text(format!("{name} +{adds} −{dels}"))
                        .font(TextStyle::new(12.0, 480.0))
                        .color(c.text_base),
                ),
        );
    }
    if let Some(files) = &size.files {
        legend = legend
            .child(spacer())
            .child(text(files).font(CAPS).color(c.text_muted));
    }

    col()
        .key(key)
        .w_full()
        .padding(12.0)
        .gap(8.0)
        .radius(8.0)
        .fill(c.layer_02)
        .border(1.0, c.border_weak)
        .child(blocks)
        .child(legend)
}

fn draw_code(
    key: &str,
    title: Option<&str>,
    marks: &[RangeInclusive<usize>],
    lines: &[String],
    k: &Kit,
) -> El {
    let c = &k.c;
    let mut card = col()
        .key(key)
        .w_full()
        .radius(8.0)
        .fill(c.bg_deep)
        .border(1.0, c.border_weak);
    if let Some(title) = title {
        card = card
            .child(
                row()
                    .w_full()
                    .px(12.0)
                    .py(7.0)
                    .fill(c.layer_02)
                    .child(text(title).font(k.ty.ui).color(c.text_muted)),
            )
            .child(el().w_full().h(1.0).fill(c.border_weak));
    }
    let mut body = col().w_full().py(6.0);
    for (idx, raw) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let marked = marks.iter().any(|r| r.contains(&line_no));
        let added = raw.starts_with('+');
        let removed = raw.starts_with('-');
        let (bar, bg, ink) = if added {
            (c.success.fg, c.success.fg.alpha(0.12), c.success.fg)
        } else if removed {
            (c.danger.fg, c.danger.fg.alpha(0.12), c.danger.fg)
        } else if marked {
            (c.info.fg, c.info.fg.alpha(0.12), c.text_base)
        } else {
            (Color::TRANSPARENT, Color::TRANSPARENT, c.text_base)
        };
        body = body.child(
            row()
                .key(format!("line-{line_no}"))
                .w_full()
                .px(10.0)
                .py(1.5)
                .gap(8.0)
                .center_y()
                .fill(bg)
                .child(el().w(2.5).h(15.0).radius(1.0).fill(bar))
                .child(text(format!("{line_no:>2}")).font(CODE).color(c.text_muted))
                .child(text(raw).font(CODE).color(ink)),
        );
    }
    card.child(body)
}

fn draw_callout(key: &str, label: Option<&str>, body: &str, k: &Kit) -> El {
    let c = &k.c;
    let mut inner = col().grow(1.0).shrink().gap(4.0);
    if let Some(label) = label {
        inner = inner.child(text(label.to_uppercase()).font(CAPS).color(c.info.fg));
    }
    inner = inner.child(Markdown::new(body).build(k));
    row()
        .key(key)
        .w_full()
        .padding(10.0)
        .gap(10.0)
        .radius(6.0)
        .fill(c.info.fg.alpha(0.08))
        .child(el().w(3.0).h_full().radius(1.5).fill(c.info.fg))
        .child(inner)
}
