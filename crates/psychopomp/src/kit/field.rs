//! Colour fields: flat full-bleed colour as a beat's ground, so a film
//! changes room instead of sitting on one dark canvas. A [`field`] is a
//! full-canvas backdrop that cross-fades with [`flood`]; a [`slab`] is a big
//! rectangle of colour in the world that the camera can travel past, and
//! [`wipe`] sweeps one across the frame.
use crate::{
    author::PlanBuilder,
    math::easing::Ease,
    paint::Paint,
    stage::{StageActor, StageElement},
};

/// A full-canvas field of `paint`, behind every element and still under
/// camera moves. Later fields paint over earlier ones.
pub fn field(id: &str, paint: Paint) -> StageElement {
    StageElement::backdrop(id, paint)
}

/// A rectangle of colour in the world: `rect` is left, top, width, height.
pub fn slab(id: &str, rect: [f32; 4], paint: Paint) -> StageElement {
    let [left, top, width, height] = rect;
    StageElement::rect(
        id,
        [left + width * 0.5, top + height * 0.5, 0.0],
        [width, height],
    )
    .paint(paint)
    .stroke(None)
}

/// Hide `fields` before anything writes them; the first field in a film
/// usually stays visible.
pub fn hide(s: &mut StageActor, sc: &mut PlanBuilder, fields: &[&str]) {
    for id in fields {
        s.channel(sc, &format!("{id}.opacity"), 0.0);
    }
}

/// Flood the frame with field `to` over `seconds`, easing both ways.
pub fn flood(s: &mut StageActor, sc: &mut PlanBuilder, to: &str, at: u64, seconds: f32) {
    let opacity = s.channel(sc, &format!("{to}.opacity"), 0.0);
    sc.ease(&opacity, at, 1.0, seconds, Ease::TRAVEL);
}

/// Take field `id` away over `seconds`.
pub fn drain(s: &mut StageActor, sc: &mut PlanBuilder, id: &str, at: u64, seconds: f32) {
    s.ease(sc, &format!("{id}.opacity"), at, 0.0, seconds, Ease::TRAVEL);
}

/// Sweep slab `id` in from `offset` pixels away (negative from the left),
/// landing on its authored place.
pub fn wipe(
    s: &mut StageActor,
    sc: &mut PlanBuilder,
    id: &str,
    offset: f32,
    at: u64,
    seconds: f32,
) {
    let x = s.channel(sc, &format!("{id}.x"), offset);
    sc.ease(&x, at, 0.0, seconds, Ease::TRAVEL);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{author::SECOND, kit::explainer};

    #[test]
    fn fields_flood_and_slabs_wipe() {
        let kit = explainer::kit();
        let plan = kit.stage([
            field("red", Paint::hex("#E8401C")),
            field("blue", Paint::hex("#1F3FD1")),
            slab("band", [0.0, 700.0, 1920.0, 380.0], Paint::hex("#F2EDE4")),
        ]);
        plan.validate().unwrap();
        let mut scene = kit.begin("fields", 3 * SECOND);
        let mut s = StageActor::declare(&mut scene, "stage", &plan).unwrap();
        hide(&mut s, &mut scene, &["blue"]);
        flood(&mut s, &mut scene, "blue", SECOND, 0.5);
        wipe(&mut s, &mut scene, "band", -1920.0, 0, 0.6);
        let at = |property: &str, nanos| scene.sample(s.actor(), property, nanos).unwrap().position;
        assert_eq!(at("blue.opacity", 0), 0.0);
        assert_eq!(at("blue.opacity", 2 * SECOND), 1.0);
        assert_eq!(at("band.x", 0), -1920.0);
        assert_eq!(at("band.x", SECOND), 0.0);
        scene.finish().unwrap();
    }
}
