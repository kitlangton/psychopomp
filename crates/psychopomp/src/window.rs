//! The floating window shared by the text-surface overlays (Terminal, Chat
//! Thread, and Changed Files): a rounded panel whose body settles into place
//! like a Stage card while its content follows about 65 ms later. Every pose
//! is an ordinary Continuous Channel, so windows retarget in native playback.
use crate::{
    author::{ActorHandle, PlanBuilder},
    math::easing::Ease,
    plan::SpringPlan,
};

/// Channels every window takes: body `opacity`, `x`/`y` offsets, `scale`,
/// and `content` (the presence of everything inside, 0 to 1).
pub const WINDOW_PROPERTIES: [&str; 5] = ["opacity", "x", "y", "scale", "content"];

/// Height of a window's title bar.
pub const TITLE_BAR: f32 = 44.0;

pub fn is_window_property(property: &str) -> bool {
    WINDOW_PROPERTIES.contains(&property)
}

/// Settle in: the body rises 12 px into place from a 0.96 scale with no
/// bounce and fades in, and its content follows 65 ms later. A window with a
/// `show` starts hidden. Returns when the entrance reads as finished.
pub(crate) fn settle_in(scene: &mut PlanBuilder, actor: &ActorHandle, at_nanos: u64) -> u64 {
    let scale = scene.channel(actor, "scale", crate::stage::ENTER_SCALE);
    scene.set(&scale, at_nanos, crate::stage::ENTER_SCALE);
    scene.spring_with(&scale, at_nanos, 1.0, SpringPlan::SMOOTH);
    let y = scene.channel(actor, "y", 12.0);
    scene.set(&y, at_nanos, 12.0);
    scene.spring_with(&y, at_nanos, 0.0, SpringPlan::SMOOTH);
    let opacity = scene.channel(actor, "opacity", 0.0);
    scene.set(&opacity, at_nanos, 0.0);
    scene.ease(&opacity, at_nanos, 1.0, 0.2, Ease::Smootherstep);
    let content = scene.channel(actor, "content", 0.0);
    scene.set(&content, at_nanos, 0.0);
    scene.spring(&content, at_nanos + 65_000_000, 1.0, 0.36, 0.0);
    at_nanos + 450_000_000
}

/// Fade the content, then the body, in place.
pub(crate) fn dismiss(scene: &mut PlanBuilder, actor: &ActorHandle, at_nanos: u64) {
    let content = scene.channel(actor, "content", 1.0);
    scene.spring(&content, at_nanos, 0.0, 0.25, 0.0);
    let opacity = scene.channel(actor, "opacity", 1.0);
    scene.ease(
        &opacity,
        at_nanos + 60_000_000,
        0.0,
        0.3,
        Ease::Smootherstep,
    );
}

/// IDs that name sub-channels (`line.<id>.reveal`) must not contain dots or
/// whitespace, so a channel splits back into its ID and property.
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Split `<prefix>.<id>.<property>` into its ID and property.
pub(crate) fn split_channel<'a>(property: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    property
        .strip_prefix(prefix)?
        .strip_prefix('.')?
        .split_once('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_split_into_an_id_and_a_property() {
        assert_eq!(
            split_channel("line.l3.reveal", "line"),
            Some(("l3", "reveal"))
        );
        assert_eq!(split_channel("line.l3", "line"), None);
        assert_eq!(split_channel("lines.l3.reveal", "line"), None);
        assert!(valid_id("build-1_a") && !valid_id("a.b") && !valid_id("") && !valid_id("a b"));
    }

    #[test]
    fn settling_in_declares_a_hidden_body_and_a_trailing_content_spring() {
        let mut scene = PlanBuilder::new("window", 2_000_000_000);
        let actor = scene.actor("w", "terminal", serde_json::json!({})).unwrap();
        let done = settle_in(&mut scene, &actor, 100_000_000);
        assert_eq!(done, 550_000_000);
        let plan = scene.finish().unwrap();
        let channel = |property: &str| {
            plan.continuous_channels
                .iter()
                .find(|c| c.property == property)
                .unwrap()
        };
        assert!(
            matches!(channel("opacity").initial, crate::plan::ScalarPlan::Literal(v) if v == 0.0)
        );
        assert_eq!(channel("content").events.len(), 2);
        assert!(WINDOW_PROPERTIES.iter().all(|p| is_window_property(p)));
    }
}
