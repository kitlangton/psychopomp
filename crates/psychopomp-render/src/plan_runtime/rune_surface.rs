//! Preflight, motion-gate auditing, and GPU shutter compositing for
//! `RuneSurfacePlan` (`recipe: "rune-surface"`).
//!
//! `PreparedRuneSurface` holds only pure `Send + Sync` plan data (`uid`, `id`,
//! `plan`) so `PreparedPlan` moves freely to presentation worker threads; the
//! active thread caches its live `RuneTimeline<SurfaceState>` in thread-local LRU.

use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, bail};
use psychopomp::{
    plan::{ActorPlan, ContinuousChannelPlan},
    rune_surface::{RUNE_SURFACE_RECIPE, RuneSurfacePlan},
};
use rune::motion_gate::Allow;

use super::preflight::{decode, strict_channels};
use crate::render::{
    HeadlessRenderer,
    rune_surface::{
        SurfacePose, build_surface_timeline, sync_surface_pose, with_cached_surface_timeline,
    },
};

static NEXT_SURFACE_UID: AtomicU64 = AtomicU64::new(1);

/// How long after a discrete Rune commit its springs/transitions may still move.
const COMMIT_SETTLE_SECS: f64 = 1.25;

pub(crate) struct PreparedRuneSurface {
    uid: u64,
    id: String,
    plan: RuneSurfacePlan,
}

impl PreparedRuneSurface {
    pub(super) fn new(actor: &ActorPlan, channels: &[ContinuousChannelPlan]) -> Result<Self> {
        let plan: RuneSurfacePlan = decode(actor, RUNE_SURFACE_RECIPE, RuneSurfacePlan::validate)?;
        strict_channels(
            &actor.id,
            channels,
            RUNE_SURFACE_RECIPE,
            RuneSurfacePlan::accepts,
        )?;
        Ok(Self {
            uid: NEXT_SURFACE_UID.fetch_add(1, Ordering::Relaxed),
            id: actor.id.clone(),
            plan,
        })
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    #[allow(dead_code)]
    pub(crate) fn plan(&self) -> &RuneSurfacePlan {
        &self.plan
    }

    pub(crate) fn native(&self) -> bool {
        self.plan.commits.is_empty()
    }

    /// True when a scheduled Rune commit may still be animating at `time`.
    pub(crate) fn moving(&self, time: f64) -> bool {
        self.plan.commits.iter().any(|c| {
            let ct = c.at_nanos as f64 * 1.0e-9;
            time >= ct - 1.0e-4 && time <= ct + COMMIT_SETTLE_SECS
        })
    }

    /// Composite a multi-sample GPU `Rgba16Float` shutter exposure of this Rune
    /// surface directly over `base_pixels` in linear light.
    pub(crate) fn composite_exposure(
        &self,
        base_pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        exposure: &[(f64, f32)],
        sample: impl Fn(&str, &str, f64, f32) -> f32,
    ) -> Result<()> {
        let any_visible = exposure
            .iter()
            .any(|&(t, w)| w > 0.0 && sample(&self.id, "opacity", t, 1.0) > 0.001);
        if !any_visible {
            return Ok(());
        }
        with_cached_surface_timeline(self.uid, &self.plan, |tl| {
            renderer.composite_rune_with_pose(base_pixels, tl, exposure, |t| SurfacePose {
                x: sample(&self.id, "x", t, 0.0),
                y: sample(&self.id, "y", t, 0.0),
                scale: sample(&self.id, "scale", t, 1.0).max(0.01),
                opacity: sample(&self.id, "opacity", t, 1.0).clamp(0.0, 1.0),
            })
        })
    }

    /// Single-instant composite (used by single-sample paths).
    pub(crate) fn render_single(
        &self,
        base_pixels: &mut [u8],
        renderer: &mut HeadlessRenderer,
        time: f64,
        sample: impl Fn(&str, &str, f32) -> f32,
    ) -> Result<()> {
        self.composite_exposure(base_pixels, renderer, &[(time, 1.0)], |a, p, _t, def| {
            sample(a, p, def)
        })
    }

    /// Presented element rect `[x, y, w, h]` in canvas pixels at `time`.
    #[allow(dead_code)]
    pub(crate) fn presented_rect(
        &self,
        time: f64,
        key: &str,
        sample: impl Fn(&str, &str, f32) -> f32,
    ) -> Option<[f32; 4]> {
        let pose = SurfacePose {
            x: sample(&self.id, "x", 0.0),
            y: sample(&self.id, "y", 0.0),
            scale: sample(&self.id, "scale", 1.0).max(0.01),
            opacity: sample(&self.id, "opacity", 1.0).clamp(0.0, 1.0),
        };
        with_cached_surface_timeline(self.uid, &self.plan, |tl| {
            sync_surface_pose(tl, time, pose);
            tl.presented_px(time, key).map(|(rect, _)| rect)
        })
    }

    /// Audit this surface's discrete commits with `rune::motion_gate` at `120 Hz`.
    pub(crate) fn audit_motion(&self, duration_secs: f64) -> Result<()> {
        let tl = build_surface_timeline(&self.plan);
        let mut leaked_rules: Vec<Allow> = vec![
            (
                "fade-snap",
                "segment",
                "",
                "static_pill mode intentionally snaps without a sliding indicator",
            ),
            (
                "color-snap",
                "segment",
                "",
                "static_pill mode intentionally snaps without a sliding indicator",
            ),
            (
                "jump",
                "df0.",
                "",
                "upstream rune_kit::DiffView virtual list remounts row slots synchronously on mode/annotation commits",
            ),
            (
                "offset-jump",
                "df0.",
                "",
                "upstream rune_kit::DiffView virtual list shifts row slots synchronously on mode/annotation commits",
            ),
            (
                "pop-in",
                "df0.",
                "",
                "upstream rune_kit::DiffView virtual list mounts split/unified cells without transitions",
            ),
            (
                "pop-out",
                "df0.",
                "",
                "upstream rune_kit::DiffView virtual list unmounts split/unified cells without transitions",
            ),
        ];
        for rule in &self.plan.allow_motion {
            let r: &'static str = Box::leak(rule[0].clone().into_boxed_str());
            let k: &'static str = Box::leak(rule[1].clone().into_boxed_str());
            let inp: &'static str = Box::leak(rule[2].clone().into_boxed_str());
            let why: &'static str = Box::leak(rule[3].clone().into_boxed_str());
            leaked_rules.push((r, k, inp, why));
        }
        let findings = tl.motion_audit(duration_secs, 120.0, &leaked_rules);
        if !findings.is_empty() {
            let formatted = findings
                .iter()
                .map(|f| format!("  - {f}"))
                .collect::<Vec<_>>()
                .join("\n");
            bail!(
                "rune-surface '{}' failed motion_gate ({} finding(s)):\n{formatted}",
                self.id,
                findings.len()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use psychopomp::{
        author::{PlanBuilder, seconds},
        rune_surface::{KitTone, RuneAction, RuneSurfaceActor, RuneSurfacePlan, kit},
    };

    use crate::plan_runtime::preflight;

    #[test]
    fn motion_gate_passes_clean_kit_and_note_surfaces() {
        let mut scene = PlanBuilder::new("motion-gate-ok", seconds(3.0));
        let dialog = kit::dialog(
            "dialog",
            420.0,
            vec![
                kit::segmented("seg", &["Cost", "Tokens", "Search"], 0, Some(250.0)),
                kit::col(
                    "rows",
                    4.0,
                    vec![
                        kit::list_row("go", "OpenCode Go", "API key only", KitTone::Danger, false),
                        kit::list_row(
                            "console",
                            "OpenCode Console",
                            "sign in or API key",
                            KitTone::Base,
                            true,
                        ),
                    ],
                ),
            ],
        );
        let mut ui =
            RuneSurfaceActor::declare(&mut scene, "ui", RuneSurfacePlan::kit(dialog)).unwrap();
        ui.commit(
            &mut scene,
            seconds(0.8),
            "select tokens",
            RuneAction::SelectSegment {
                key: "seg".into(),
                index: 1,
            },
        )
        .unwrap();
        ui.commit(
            &mut scene,
            seconds(1.6),
            "drop go",
            RuneAction::Batch {
                actions: vec![
                    RuneAction::SetVisible {
                        key: "go".into(),
                        visible: false,
                    },
                    RuneAction::SetRow {
                        key: "console".into(),
                        title: Some("OpenCode".into()),
                        hint: Some("sign in or API key · Go included".into()),
                        tone: None,
                        hint_tone: Some(KitTone::Success),
                        selected: Some(true),
                    },
                ],
            },
        )
        .unwrap();
        let plan = scene.finish().unwrap();
        let preflight = preflight::Plan::new(plan).unwrap();
        for surface in &preflight.rune_surfaces {
            surface.audit_motion(3.0).unwrap();
        }
    }
}
