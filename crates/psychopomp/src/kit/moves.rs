//! Entrances, exits, and dims for any Stage element, each with its own
//! character, so a beat picks the move that suits its idea instead of
//! fading everything up the same way:
//!
//! | Move | Reads as |
//! |---|---|
//! | [`arrive`] | the next item in a list: rises a short way |
//! | [`slide`] | something that comes from somewhere: travels in from the side |
//! | [`stamp`] | a verdict: lands with weight from larger |
//! | [`pop`] | a node or chip: grows from smaller |
//! | [`show`] | a fact already there: appears in place |
//! | [`out`] | an exit, shorter than any entrance |
//! | [`dim`] | look elsewhere: drops to a level |
//!
//! Entrances write the element's start state at time 0, so the element is
//! hidden until the move begins. [`on`] lands a key move a few frames ahead
//! of the word it belongs to.
use crate::{
    author::PlanBuilder, math::easing::Ease, narration::Spoken, plan::SpringPlan, stage::StageActor,
};

/// How far ahead of its word a key move lands: the eye leads the ear.
pub const LEAD_WORD: u64 = 80_000_000;

/// When a key move should land for `phrase`: [`LEAD_WORD`] ahead of it.
pub fn on(spoken: &Spoken, phrase: &str) -> u64 {
    spoken.at(phrase).saturating_sub(LEAD_WORD)
}

/// Hide `ids` from the first frame.
pub fn hide(s: &mut StageActor, sc: &mut PlanBuilder, ids: &[&str]) {
    for id in ids {
        s.channel(sc, &format!("{id}.opacity"), 0.0);
    }
}

/// Rise a short way into place: for text that reads as a list.
pub fn arrive(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: u64) {
    let opacity = s.channel(sc, &format!("{id}.opacity"), 0.0);
    sc.ease(&opacity, at, 1.0, 0.3, Ease::ARRIVE);
    let y = s.channel(sc, &format!("{id}.y"), 26.0);
    sc.ease(&y, at, 0.0, 0.5, Ease::ARRIVE);
}

/// Slide in from `dx` pixels to the side: for things that come from somewhere.
pub fn slide(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: u64, dx: f32) {
    let opacity = s.channel(sc, &format!("{id}.opacity"), 0.0);
    sc.ease(&opacity, at, 1.0, 0.25, Ease::ARRIVE);
    let x = s.channel(sc, &format!("{id}.x"), dx);
    sc.ease(&x, at, 0.0, 0.6, Ease::ARRIVE);
}

/// Land with weight from a little larger: for stamps and verdicts.
pub fn stamp(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: u64) {
    let opacity = s.channel(sc, &format!("{id}.opacity"), 0.0);
    sc.ease(&opacity, at, 1.0, 0.12, Ease::ARRIVE);
    let scale = s.channel(sc, &format!("{id}.scale"), 1.35);
    sc.spring_with(&scale, at, 1.0, SpringPlan::ENTER);
}

/// Grow from smaller: for nodes and chips.
pub fn pop(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: u64) {
    let opacity = s.channel(sc, &format!("{id}.opacity"), 0.0);
    sc.ease(&opacity, at, 1.0, 0.2, Ease::ARRIVE);
    let scale = s.channel(sc, &format!("{id}.scale"), 0.82);
    sc.spring_with(&scale, at, 1.0, SpringPlan::ENTER);
}

/// Appear in place, no travel.
pub fn show(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: u64) {
    let opacity = s.channel(sc, &format!("{id}.opacity"), 0.0);
    sc.ease(&opacity, at, 1.0, 0.3, Ease::ARRIVE);
}

/// Leave, shorter than any entrance.
pub fn out(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: u64) {
    s.ease(sc, &format!("{id}.opacity"), at, 0.0, 0.2, Ease::LEAVE);
}

/// Dim to `level` so the eye goes elsewhere.
pub fn dim(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: u64, level: f32) {
    s.ease(sc, &format!("{id}.opacity"), at, level, 0.35, Ease::TRAVEL);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        author::SECOND,
        kit::{explainer, field},
        paint::Paint,
    };

    #[test]
    fn entrances_start_hidden_and_land_in_place() {
        let kit = explainer::kit();
        let plan = kit.stage(
            ["a", "b", "c", "d"]
                .map(|id| field::slab(id, [100.0, 100.0, 200.0, 200.0], Paint::hex("#E8401C"))),
        );
        let mut scene = kit.begin("moves", 3 * SECOND);
        let mut s = StageActor::declare(&mut scene, "stage", &plan).unwrap();
        arrive(&mut s, &mut scene, "a", SECOND);
        slide(&mut s, &mut scene, "b", SECOND, -80.0);
        stamp(&mut s, &mut scene, "c", SECOND);
        pop(&mut s, &mut scene, "d", SECOND);
        out(&mut s, &mut scene, "a", 2 * SECOND);
        let at = |property: &str, time| scene.sample(s.actor(), property, time).unwrap().position;
        for id in ["a", "b", "c", "d"] {
            assert_eq!(at(&format!("{id}.opacity"), 0), 0.0, "{id} starts hidden");
        }
        assert_eq!(at("a.y", 0), 26.0);
        assert_eq!(at("b.x", 0), -80.0);
        assert!(at("c.scale", 0) > 1.0, "a stamp lands from larger");
        assert!(at("d.scale", 0) < 1.0, "a pop grows from smaller");
        let settled = 2 * SECOND - 1;
        assert_eq!(at("a.y", settled), 0.0);
        assert_eq!(at("b.x", settled), 0.0);
        assert!((at("c.scale", settled) - 1.0).abs() < 0.01);
        assert_eq!(at("a.opacity", settled), 1.0);
        assert_eq!(at("a.opacity", 3 * SECOND - 1), 0.0, "out leaves");
    }
}
