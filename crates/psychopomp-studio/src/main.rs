//! `psychopomp-studio`: Rune preview shell and component playground for
//! Psychopomp (`App::playground`, `⌘⇧K` `rune-knobs`, `R` deterministic beat
//! replay, live `960×540` plan/reel scrubber, and `scripts/rune-hot` live
//! patching).

mod stories;

use std::path::PathBuf;
use std::time::Duration;

use psychopomp_render::{PreviewSession, Theme};
use rune::{
    App, AppSpec, Cx, El, Family, Key, KeyEvent, Pixels, TextStyle, Tx, col, el, row, spacer, text,
};
use rune_kit::{Badge, Kit};
use rune_knobs::{Group, Knob, Knobs, Preset, Style};

use stories::{ConnectStory, build_playground, connect_card_el};

pub(crate) const CARD_WIDTH: Knob = Knob::new("Card width", 360.0, 680.0, 470.0)
    .step(4.0)
    .about("Surface card width in points");
pub(crate) const CARD_PAD: Knob = Knob::new("Card padding", 10.0, 32.0, 18.0)
    .step(1.0)
    .about("Inner card padding in points");
pub(crate) const CARD_RADIUS: Knob = Knob::new("Card radius", 4.0, 24.0, 12.0)
    .step(1.0)
    .about("Surface corner radius in points");
pub(crate) const SPRING_SECS: Knob = Knob::new("Spring duration", 0.14, 0.80, 0.36)
    .step(0.02)
    .about("Layout spring duration in seconds");
pub(crate) const SPRING_BOUNCE: Knob = Knob::new("Spring bounce", 0.0, 0.35, 0.0)
    .step(0.02)
    .about("Spring overshoot bounce amount");
pub(crate) const THEME_LOOK: Knob = Knob::choice("Surface theme", &["OpenCode", "Neutral"], 0)
    .about("Token palette for Rune surfaces")
    .hints(&["@opencode/ui dark tokens", "Core neutral tokens"]);
pub(crate) const SHUTTER_MODE: Knob =
    Knob::choice("Viewport shutter", &["Draft 1x", "Shutter 24x"], 0)
        .about("Single sample or 24x shutter")
        .hints(&[
            "Instant single-sample scrub",
            "Linear 24-sample GPU shutter",
        ]);

const SURFACE_GROUP: Group = Group::new(
    "Surface",
    "surface-card",
    &[CARD_WIDTH, CARD_PAD, CARD_RADIUS, THEME_LOOK],
)
.under("Studio")
.presets(&[
    Preset {
        name: "Compact",
        values: &[("Card width", 420.0), ("Card padding", 14.0)],
    },
    Preset {
        name: "Generous",
        values: &[
            ("Card width", 540.0),
            ("Card padding", 24.0),
            ("Card radius", 16.0),
        ],
    },
]);

const MOTION_GROUP: Group = Group::new("Motion", "surface-card", &[SPRING_SECS, SPRING_BOUNCE])
    .under("Studio")
    .presets(&[
        Preset {
            name: "Crisp",
            values: &[("Spring duration", 0.28), ("Spring bounce", 0.0)],
        },
        Preset {
            name: "Lively",
            values: &[("Spring duration", 0.42), ("Spring bounce", 0.14)],
        },
    ]);

const VIEWPORT_GROUP: Group = Group::new("Viewport", "viewport", &[SHUTTER_MODE]).under("Studio");

pub(crate) const STUDIO_GROUPS: &[Group] = &[SURFACE_GROUP, MOTION_GROUP, VIEWPORT_GROUP];
pub(crate) const STORY_GROUPS: &[Group] = &[SURFACE_GROUP, MOTION_GROUP];

pub(crate) fn knob_style(dark: bool) -> Style {
    if dark { Style::dark() } else { Style::light() }
}

pub(crate) fn kit_for_knobs(knobs: &Knobs) -> Kit {
    if knobs.get(&THEME_LOOK) >= 0.5 {
        Kit::neutral(true)
    } else {
        rune_theme_opencode::kit(true).clone()
    }
}

pub(crate) struct StudioState {
    knobs: Knobs,
    plan_path: Option<PathBuf>,
    session: Option<PreviewSession>,
    pixels: Pixels,
    playhead: f64,
    duration: f64,
    playing: bool,
    last_tick: Option<f64>,
    rendered_key: Option<(u64, bool)>,
    demo: ConnectStory,
    remembered_frame: bool,
}

fn parse_plan_arg() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for (idx, arg) in args.iter().enumerate() {
        if arg == "--plan" {
            return args.get(idx + 1).map(PathBuf::from);
        }
    }
    None
}

pub(crate) fn studio_init() -> StudioState {
    let plan_path = parse_plan_arg();
    let session = plan_path
        .as_deref()
        .and_then(|p| PreviewSession::open(p, Theme::Neutral).ok());
    let duration = session
        .as_ref()
        .map_or(2.4, PreviewSession::duration_seconds);
    let mut state = StudioState {
        knobs: Knobs::default(),
        plan_path,
        session,
        pixels: Pixels::new(),
        playhead: 0.0,
        duration,
        playing: false,
        last_tick: None,
        rendered_key: None,
        demo: ConnectStory::default(),
        remembered_frame: false,
    };
    state.refresh_viewport();
    state
}

impl StudioState {
    fn refresh_viewport(&mut self) {
        let shutter = self.knobs.get(&SHUTTER_MODE) >= 0.5 && !self.playing;
        let quant = (self.playhead * 1000.0).round() as u64;
        if self.rendered_key == Some((quant, shutter)) {
            return;
        }
        if let Some(session) = &mut self.session
            && let Ok(rgba) = session.render_preview_960x540(self.playhead, shutter)
        {
            self.pixels.set(960, 540, rgba);
            self.rendered_key = Some((quant, shutter));
        }
        self.demo.tab = if self.playhead < 0.6 {
            0
        } else if self.playhead < 1.5 {
            1
        } else {
            2
        };
        self.demo.show_go = self.playhead < 1.1;
        self.demo.renamed = self.playhead >= 1.1;
    }

    fn seek(&mut self, t: f64) {
        self.playhead = t.clamp(0.0, self.duration.max(0.1));
        self.refresh_viewport();
    }
}

fn studio_poll(s: &mut StudioState, now: f64, tx: &mut Tx) -> bool {
    if !s.remembered_frame {
        s.remembered_frame = rune_mac::remember_frame("Psychopomp Studio");
    }
    if !s.playing {
        s.last_tick = None;
        return false;
    }
    tx.wake_after(Duration::from_millis(16));
    let dt = s
        .last_tick
        .map_or(1.0 / 60.0, |prev| (now - prev).clamp(0.0, 0.1));
    s.last_tick = Some(now);
    let next = s.playhead + dt;
    if next >= s.duration {
        s.playing = false;
        s.seek(s.duration);
    } else {
        s.seek(next);
    }
    true
}

fn studio_key(s: &mut StudioState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if rune_knobs::key(&mut s.knobs, ev) {
        tx.motion(s.knobs.slide());
        s.refresh_viewport();
        return true;
    }
    match &ev.key {
        Key::Text(ch) if ch == " " && !ev.mods.cmd && !ev.mods.ctrl => {
            if s.playhead >= s.duration {
                s.seek(0.0);
            }
            s.playing = !s.playing;
            tx.wake_after(Duration::ZERO);
            true
        }
        Key::Text(ch) if ch.eq_ignore_ascii_case("r") && !ev.mods.cmd && !ev.mods.ctrl => {
            s.seek(0.0);
            s.playing = true;
            tx.wake_after(Duration::ZERO);
            true
        }
        Key::Text(ch) if ch.eq_ignore_ascii_case("s") && !ev.mods.cmd && !ev.mods.ctrl => {
            let cur = s.knobs.get(&SHUTTER_MODE);
            s.knobs
                .set(&SHUTTER_MODE, if cur < 0.5 { 1.0 } else { 0.0 });
            s.refresh_viewport();
            true
        }
        Key::Left => {
            s.playing = false;
            s.seek(s.playhead - 1.0 / 60.0);
            true
        }
        Key::Right => {
            s.playing = false;
            s.seek(s.playhead + 1.0 / 60.0);
            true
        }
        _ => false,
    }
}

fn studio_page(s: &StudioState, _cx: &mut Cx<StudioState>) -> El {
    let k = kit_for_knobs(&s.knobs);
    let c = &k.c;
    let title = s
        .session
        .as_ref()
        .map(|ses| ses.title().to_owned())
        .or_else(|| s.plan_path.as_ref().map(|p| p.display().to_string()))
        .unwrap_or_else(|| "live rune stage · pass --plan <file.json>".into());

    let top_bar = row()
        .key("studio-header")
        .w_full()
        .h(44.0)
        .px(16.0)
        .gap(10.0)
        .center_y()
        .fill(c.layer_01)
        .child(
            text("Psychopomp Studio")
                .font(TextStyle::new(14.0, 620.0))
                .color(c.text_base),
        )
        .child(Badge::new(&title).build(&k))
        .child(spacer())
        .child(Badge::new("⌘⇧D playground").build(&k))
        .child(Badge::new("⌘⇧K knobs").build(&k))
        .child(Badge::new("R replay").build(&k));

    let viewport_content = if s.session.is_some() {
        el().key("viewport")
            .size(768.0, 432.0)
            .radius(10.0)
            .clip_rounded(10.0)
            .border(1.0, c.border_base)
            .pixels(&s.pixels)
    } else {
        connect_card_el(
            &s.knobs,
            s.demo.tab,
            s.demo.show_go,
            s.demo.renamed,
            &s.demo.search,
        )
    };

    let mut cue_strip = row().key("cue-strip").w_full().gap(6.0).center_y();
    let cues: Vec<(String, f64)> = s
        .session
        .as_ref()
        .map(|ses| ses.cues().to_vec())
        .unwrap_or_else(|| {
            vec![
                ("intro".into(), 0.0),
                ("tokens".into(), 0.6),
                ("merge-go".into(), 1.1),
                ("web-search".into(), 1.6),
            ]
        });
    for (id, at) in cues.into_iter().take(8) {
        let active = (s.playhead - at).abs() < 0.25;
        cue_strip = cue_strip.child(
            row()
                .key(format!("cue-{id}"))
                .h(24.0)
                .px(9.0)
                .radius(5.0)
                .center_y()
                .fill(if active {
                    c.info.fg.alpha(0.18)
                } else {
                    c.layer_02
                })
                .border(1.0, if active { c.info.fg } else { c.border_weak })
                .child(
                    text(format!("{id} · {at:.1}s"))
                        .font(TextStyle {
                            family: Family::Mono,
                            ..TextStyle::new(11.0, 500.0)
                        })
                        .color(if active { c.info.fg } else { c.text_muted }),
                )
                .on_press(move |st: &mut StudioState| {
                    st.playing = false;
                    st.seek(at);
                }),
        );
    }

    let frac = (s.playhead / s.duration.max(0.001)).clamp(0.0, 1.0) as f32;
    let transport = col()
        .key("studio-transport")
        .w_full()
        .px(16.0)
        .py(10.0)
        .gap(8.0)
        .fill(c.layer_01)
        .child(
            row()
                .w_full()
                .gap(10.0)
                .center_y()
                .child(
                    row()
                        .key("play-btn")
                        .h(26.0)
                        .px(12.0)
                        .radius(5.0)
                        .center()
                        .fill(c.layer_02)
                        .border(1.0, c.border_base)
                        .child(
                            text(if s.playing { "Pause" } else { "Play" })
                                .font(TextStyle::new(12.0, 560.0))
                                .color(c.text_base),
                        )
                        .on_press_tx(|st: &mut StudioState, tx: &mut Tx| {
                            st.playing = !st.playing;
                            tx.wake_after(Duration::ZERO);
                        }),
                )
                .child(
                    row()
                        .key("replay-btn")
                        .h(26.0)
                        .px(10.0)
                        .radius(5.0)
                        .center()
                        .fill(c.layer_02)
                        .child(
                            text("R · Replay")
                                .font(TextStyle::new(12.0, 520.0))
                                .color(c.text_muted),
                        )
                        .on_press_tx(|st: &mut StudioState, tx: &mut Tx| {
                            st.seek(0.0);
                            st.playing = true;
                            tx.wake_after(Duration::ZERO);
                        }),
                )
                .child(
                    el().key("scrubber")
                        .grow(1.0)
                        .h(6.0)
                        .radius(3.0)
                        .fill(c.layer_02)
                        .child(el().w_frac(frac).h(6.0).radius(3.0).fill(c.info.fg)),
                )
                .child(
                    text(format!("{:05.2}s / {:05.2}s", s.playhead, s.duration))
                        .font(TextStyle {
                            family: Family::Mono,
                            ..TextStyle::new(12.0, 500.0)
                        })
                        .color(c.text_muted),
                ),
        )
        .child(cue_strip);

    col()
        .key("studio-root")
        .grow(1.0)
        .h_full()
        .fill(c.bg_deep)
        .child(top_bar)
        .child(el().w_full().h(1.0).fill(c.border_weak))
        .child(
            col()
                .key("viewport-stage")
                .w_full()
                .grow(1.0)
                .shrink()
                .center()
                .child(viewport_content),
        )
        .child(el().w_full().h(1.0).fill(c.border_weak))
        .child(transport)
}

pub(crate) fn studio_view(s: &StudioState, cx: &mut Cx<StudioState>) -> El {
    let page = studio_page(s, cx);
    rune_knobs::side(
        page,
        &s.knobs,
        STUDIO_GROUPS,
        knob_style(cx.system_dark()),
        |st: &mut StudioState| &mut st.knobs,
    )
}

type StudioScene = (&'static str, fn(&mut StudioState));

static SCENES: &[StudioScene] = &[
    ("knobs-open", |s| {
        s.knobs.show();
        s.seek(1.5);
    }),
    ("merged", |s| s.seek(1.3)),
];

fn main() {
    let spec = AppSpec::<StudioState> {
        scenes: SCENES,
        poll: Some(studio_poll),
        on_key: Some(studio_key),
        ..AppSpec::new(
            "Psychopomp Studio",
            (1120.0, 700.0),
            studio_init,
            studio_view,
        )
    };
    App::new(spec).playground(build_playground()).run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Mods, Runtime};

    #[test]
    fn every_knob_has_concise_about_and_choice_hints() {
        for group in STUDIO_GROUPS {
            for knob in group.knobs {
                assert!(
                    !knob.about.trim().is_empty(),
                    "knob '{}' needs .about(...)",
                    knob.name
                );
                let words = knob.about.split_whitespace().count();
                assert!(
                    (1..=8).contains(&words),
                    "knob '{}' about '{}' must be 1..=8 words (got {words})",
                    knob.name,
                    knob.about
                );
                if !knob.labels.is_empty() {
                    assert_eq!(
                        knob.hints.len(),
                        knob.labels.len(),
                        "choice knob '{}' needs one .hints(...) entry per choice",
                        knob.name
                    );
                }
            }
        }
    }

    #[test]
    fn studio_shell_knobs_and_replay_step_deterministically_headless() {
        let mut rt = Runtime::new(
            studio_init(),
            studio_view,
            (1120.0, 700.0),
            Env::new(true, 2.0),
        );
        rt.frame(0.0);
        assert!(rt.presented("studio-header").is_some());
        assert!(rt.presented("surface-card").is_some());

        // Toggle ⌘⇧K knobs panel headlessly (`Key::Command('k')` + `mods.shift`).
        rt.update(|s, tx| {
            let taken = studio_key(
                s,
                &KeyEvent {
                    key: Key::Command('k'),
                    mods: Mods {
                        cmd: true,
                        shift: true,
                        ..Mods::default()
                    },
                },
                tx,
            );
            assert!(taken);
        });
        rt.frame(1.0 / 60.0);
        assert!(rt.state.knobs.open);

        // Press R to replay deterministically from 0.0s.
        rt.update(|s, tx| {
            let taken = studio_key(
                s,
                &KeyEvent {
                    key: Key::Text("r".into()),
                    mods: Mods::default(),
                },
                tx,
            );
            assert!(taken);
        });
        assert!(rt.state.playing);
        assert_eq!(rt.state.playhead, 0.0);
    }
}
