//! Diagram pieces for any kit: a node is a filled box with a title, an edge
//! is a path that docks on two nodes and draws on with
//! [`StageActor::connect`], and a packet travels an edge with
//! [`StageActor::send`]. Edges and packets take a [`Tone`], so the kit's
//! palette decides their colour; node fills and titles take explicit paint.
//!
//! ```ignore
//! let mut elements = diagram::node("console", [560.0, 540.0], [420.0, 180.0], fill, title);
//! elements.push(diagram::edge("ask", "console", "catalog", Tone::Accent));
//! elements.push(diagram::packet("ask-packet", "ask", Tone::Accent));
//! s.connect(sc, "ask", at, 0.6);
//! s.send(sc, "ask-packet", at + seconds(0.7), 0.9);
//! ```
//!
//! Edges draw matte by default, a quiet grey with a hint of their tone, so
//! a dark diagram stays calm. On a colour field, [`ink`] them: they draw
//! flat in their tone, like a printed line.
use crate::{
    author::PlanBuilder,
    paint::Paint,
    stage::{Arrow, StageActor, StageElement, Waypoint},
    tone::Tone,
    typography::TypePlan,
};

/// Edge stroke width, heavy enough to read at phone size.
pub const EDGE_WIDTH: f32 = 4.0;
/// Corner radius of an elbowed edge.
pub const EDGE_CORNER: f32 = 28.0;

/// A box filled with `fill`, its title centred on it (`{id}-title`).
pub fn node(
    id: &str,
    at: [f32; 2],
    size: [f32; 2],
    fill: Paint,
    title: TypePlan,
) -> Vec<StageElement> {
    vec![
        StageElement::rect(id, [at[0], at[1], 0.0], size)
            .paint(fill)
            .stroke(None)
            .corner(size[1].min(size[0]) * 0.12),
        StageElement::kinetic_type(&format!("{id}-title"), [at[0], at[1], 0.0], title),
    ]
}

/// An outlined box: a hairline frame in `tone` with no fill.
pub fn frame(id: &str, at: [f32; 2], size: [f32; 2], tone: Tone, width: f32) -> StageElement {
    StageElement::rect(id, [at[0], at[1], 0.0], size)
        .stroke(Some(tone))
        .stroke_width(width)
        .corner(size[1].min(size[0]) * 0.12)
}

/// A straight arrow from node `from` to node `to`, docked on their outlines.
pub fn edge(id: &str, from: &str, to: &str, tone: Tone) -> StageElement {
    route(id, from, &[], to, tone)
}

/// An arrow from `from` through world points `via` to `to`, its corners rounded.
pub fn route(id: &str, from: &str, via: &[[f32; 2]], to: &str, tone: Tone) -> StageElement {
    let through = std::iter::once(Waypoint::Element(from.into()))
        .chain(via.iter().map(|p| Waypoint::Point([p[0], p[1], 0.0])))
        .chain(std::iter::once(Waypoint::Element(to.into())))
        .collect();
    StageElement::path(id, through)
        .tone(tone)
        .stroke_width(EDGE_WIDTH)
        .corner(EDGE_CORNER)
        .arrow(Arrow::End)
}

/// Draw `edges` flat in their tone from the start, instead of matte.
pub fn ink(s: &mut StageActor, sc: &mut PlanBuilder, edges: &[&str]) {
    for edge in edges {
        s.channel(sc, &format!("{edge}.tint"), 1.0);
    }
}

/// A packet that travels `edge` from its first node.
pub fn packet(id: &str, edge: &str, tone: Tone) -> StageElement {
    StageElement::packet(id, edge).tone(tone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        author::{SECOND, seconds},
        kit::explainer,
        stage::StageActor,
    };

    #[test]
    fn a_diagram_validates_draws_on_and_carries_a_packet() {
        let kit = explainer::kit();
        let title = kit.headline("console", 40.0, Paint::hex("#FFFFFF"));
        let mut elements = node(
            "a",
            [400.0, 540.0],
            [360.0, 160.0],
            Paint::hex("#222"),
            title.clone(),
        );
        elements.extend(node(
            "b",
            [1500.0, 300.0],
            [360.0, 160.0],
            Paint::hex("#222"),
            title,
        ));
        elements.push(route(
            "ask",
            "a",
            &[[960.0, 540.0], [960.0, 300.0]],
            "b",
            Tone::Accent,
        ));
        elements.push(packet("ask-packet", "ask", Tone::Accent));
        let plan = kit.stage(elements);
        plan.validate().unwrap();
        let StageElement::Path { through, arrow, .. } = plan.element("ask").unwrap() else {
            panic!("path");
        };
        assert_eq!(through.len(), 4);
        assert_eq!(*arrow, Arrow::End);

        let mut scene = kit.begin("diagram", 4 * SECOND);
        let mut s = StageActor::declare(&mut scene, "stage", &plan).unwrap();
        let drawn = s.connect(&mut scene, "ask", 0, 0.6);
        s.send(&mut scene, "ask-packet", drawn + seconds(0.2), 0.9);
        ink(&mut s, &mut scene, &["ask"]);
        let sample = |property, at| scene.sample(s.actor(), property, at).unwrap().position;
        assert_eq!(sample("ask.draw", 0), 0.0);
        assert_eq!(sample("ask.draw", SECOND), 1.0);
        assert_eq!(sample("ask.tint", 0), 1.0);
        scene.finish().unwrap();
    }
}
