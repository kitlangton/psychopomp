//! Chat Threads: a conversation in a window, in a Slack-like or an
//! iMessage-like style. Messages have stable IDs and authors with avatars;
//! a typing indicator holds the next message's slot with animated dots and
//! grows into the message when it is said; reactions pop in beneath it.
//!
//! The thread is anchored to its composer: every message's room is a pure
//! function of its `typing`, `reveal`, and reaction channels, and rooms
//! stack upward from the bottom, so a new message pushes every older one up
//! by exactly the room it opens. Wrapped text heights come from the renderer
//! as [`ChatMetrics`]; this module never measures glyphs.
use std::collections::HashSet;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    author::{ActorHandle, ContinuousHandle, PlanBuilder, seconds},
    math::{easing::Ease, lerp},
    tone::Tone,
    window,
};

pub const CHAT_RECIPE: &str = "chat";

const DEFAULT_TEXT_SIZE: f32 = 22.0;
const MAX_MESSAGES: usize = 400;

/// Typing grows its slot; saying grows it into the message without bouncing
/// the thread history above it.
const TYPING_SECONDS: f32 = 0.32;
const SAY_SECONDS: f32 = 0.5;
const SAY_BOUNCE: f32 = 0.0;
const REACT_SECONDS: f32 = 0.36;
const REACT_BOUNCE: f32 = 0.0;
const HIGHLIGHT_IN_SECONDS: f32 = 0.18;
const HIGHLIGHT_OUT_SECONDS: f32 = 0.4;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChatStyle {
    /// Avatars, names, and timestamps over unboxed text, as in Slack.
    #[default]
    Slack,
    /// Rounded bubbles, yours on the right, as in iMessage.
    Bubbles,
}

impl ChatStyle {
    pub fn is_default(&self) -> bool {
        *self == Self::Slack
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatPlan {
    /// Top-left corner of the window.
    pub origin: [f32; 2],
    /// Window `[width, height]`.
    pub size: [f32; 2],
    #[serde(default, skip_serializing_if = "ChatStyle::is_default")]
    pub style: ChatStyle,
    #[serde(
        default = "default_text_size",
        skip_serializing_if = "is_default_text_size"
    )]
    pub text_size: f32,
    /// The header: a channel (`# opencode`) or a conversation's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Placeholder in the composer; omitted, none is drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composer: Option<String>,
    /// In the bubbles style, this person's messages sit on the right.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub me: Option<String>,
    pub people: Vec<ChatPersonPlan>,
    /// Every message that may appear, oldest first. A message without
    /// `reveal` or `typing` channels is shown from time zero.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<ChatMessagePlan>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatPersonPlan {
    pub id: String,
    pub name: String,
    /// The avatar's color.
    #[serde(default, skip_serializing_if = "Tone::is_default")]
    pub tone: Tone,
    /// Avatar text; omitted, the first letters of the name's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    /// A small label after the name, such as `APP`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatMessagePlan {
    pub id: String,
    pub author: String,
    /// The text; empty while only a typing indicator holds the slot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<ChatSpanPlan>,
    /// A timestamp label, such as `10:42 AM`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<ChatReactionPlan>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatSpanPlan {
    pub text: String,
    #[serde(default, skip_serializing_if = "Tone::is_default")]
    pub tone: Tone,
    /// Monospaced on a code chip.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub code: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatReactionPlan {
    pub id: String,
    /// An emoji or a short label.
    pub label: String,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub count: u32,
}

fn default_text_size() -> f32 {
    DEFAULT_TEXT_SIZE
}

fn is_default_text_size(size: &f32) -> bool {
    *size == DEFAULT_TEXT_SIZE
}

fn one() -> u32 {
    1
}

fn is_one(count: &u32) -> bool {
    *count == 1
}

impl ChatSpanPlan {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
            code: false,
        }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self::new(text, Tone::Plain)
    }

    pub fn code(text: impl Into<String>) -> Self {
        Self {
            code: true,
            ..Self::plain(text)
        }
    }
}

impl ChatPersonPlan {
    pub fn new(id: impl Into<String>, name: impl Into<String>, tone: Tone) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            tone,
            initials: None,
            badge: None,
        }
    }

    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// The avatar's letters: `initials`, or the first letter of up to two words.
    pub fn avatar_text(&self) -> String {
        self.initials.clone().unwrap_or_else(|| {
            self.name
                .split_whitespace()
                .filter_map(|word| word.chars().next())
                .take(2)
                .flat_map(char::to_uppercase)
                .collect()
        })
    }
}

impl ChatMessagePlan {
    pub fn chars(&self) -> usize {
        self.spans
            .iter()
            .map(|span| span.text.chars().count())
            .sum()
    }
}

/// A per-message channel: `message.<id>.<name>`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatChannel {
    /// The typing indicator's presence, 0 to 1.
    Typing,
    /// Seconds since typing began: the dots' clock (-1 before).
    Wait,
    /// The message's presence, 0 to 1: grows its slot from the indicator.
    Reveal,
    /// Fraction of the text's characters shown, as a reply streams in.
    Typed,
    /// A wash behind the message.
    Highlight,
}

impl ChatChannel {
    pub const ALL: [Self; 5] = [
        Self::Typing,
        Self::Wait,
        Self::Reveal,
        Self::Typed,
        Self::Highlight,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Typing => "typing",
            Self::Wait => "wait",
            Self::Reveal => "reveal",
            Self::Typed => "typed",
            Self::Highlight => "highlight",
        }
    }

    pub fn parse(property: &str) -> Option<(&str, Self)> {
        let (id, name) = window::split_channel(property, "message")?;
        Self::ALL
            .into_iter()
            .find(|channel| channel.name() == name)
            .map(|channel| (id, channel))
    }
}

pub fn message_property(id: &str, channel: ChatChannel) -> String {
    format!("message.{id}.{}", channel.name())
}

/// A reaction's presence channel: `reaction.<message>.<reaction>`.
pub fn reaction_property(message: &str, reaction: &str) -> String {
    format!("reaction.{message}.{reaction}")
}

pub fn parse_reaction(property: &str) -> Option<(&str, &str)> {
    window::split_channel(property, "reaction")
}

/// Pixel geometry of one style at one text size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatGeometry {
    pub header: f32,
    pub composer: f32,
    pub padding: f32,
    pub avatar: f32,
    /// Left edge of message text, from the content's left.
    pub indent: f32,
    pub name_row: f32,
    pub line: f32,
    /// Bubble inset `[x, y]`.
    pub bubble_pad: [f32; 2],
    pub reactions: f32,
    /// Space above a message that starts a group, and above one that continues it.
    pub gaps: [f32; 2],
    /// Width the text wraps at.
    pub wrap: f32,
}

impl ChatPlan {
    pub fn new(origin: [f32; 2], size: [f32; 2], people: Vec<ChatPersonPlan>) -> Self {
        Self {
            origin,
            size,
            style: ChatStyle::Slack,
            text_size: DEFAULT_TEXT_SIZE,
            title: None,
            subtitle: None,
            composer: None,
            me: None,
            people,
            messages: Vec::new(),
        }
    }

    pub fn style(mut self, style: ChatStyle) -> Self {
        self.style = style;
        self
    }

    pub fn titled(mut self, title: impl Into<String>, subtitle: Option<&str>) -> Self {
        self.title = Some(title.into());
        self.subtitle = subtitle.map(str::to_owned);
        self
    }

    pub fn composer(mut self, placeholder: impl Into<String>) -> Self {
        self.composer = Some(placeholder.into());
        self
    }

    pub fn me(mut self, person: impl Into<String>) -> Self {
        self.me = Some(person.into());
        self
    }

    pub fn person(&self, id: &str) -> Option<&ChatPersonPlan> {
        self.people.iter().find(|person| person.id == id)
    }

    pub fn find(&self, id: &str) -> Option<usize> {
        self.messages.iter().position(|message| message.id == id)
    }

    /// Whether message `index` is yours (bubbles on the right).
    pub fn mine(&self, index: usize) -> bool {
        self.style == ChatStyle::Bubbles
            && self.me.as_deref() == Some(self.messages[index].author.as_str())
    }

    /// Whether message `index` starts a run by its author, so it shows the
    /// author's avatar and name.
    pub fn group_start(&self, index: usize) -> bool {
        index == 0 || self.messages[index - 1].author != self.messages[index].author
    }

    pub fn geometry(&self) -> ChatGeometry {
        let em = self.text_size;
        let header = if self.title.is_some() {
            (em * if self.subtitle.is_some() { 3.6 } else { 2.9 }).round()
        } else {
            0.0
        };
        let composer = if self.composer.is_some() {
            (em * 3.4).round()
        } else {
            (em * 0.6).round()
        };
        let padding = (em * 1.1).round();
        let content = self.size[0] - padding * 2.0;
        match self.style {
            ChatStyle::Slack => {
                let avatar = (em * 1.85).round();
                let indent = avatar + (em * 0.65).round();
                ChatGeometry {
                    header,
                    composer,
                    padding,
                    avatar,
                    indent,
                    name_row: (em * 1.45).round(),
                    line: (em * 1.42).round(),
                    bubble_pad: [(em * 0.6).round(), (em * 0.3).round()],
                    reactions: (em * 1.85).round(),
                    gaps: [(em * 0.95).round(), (em * 0.3).round()],
                    wrap: (content - indent).max(em * 4.0),
                }
            }
            ChatStyle::Bubbles => {
                let avatar = (em * 1.5).round();
                let indent = avatar + (em * 0.45).round();
                let bubble_pad = [(em * 0.72).round(), (em * 0.45).round()];
                ChatGeometry {
                    header,
                    composer,
                    padding,
                    avatar,
                    indent,
                    name_row: (em * 1.15).round(),
                    line: (em * 1.32).round(),
                    bubble_pad,
                    reactions: (em * 1.75).round(),
                    gaps: [(em * 0.8).round(), (em * 0.2).round()],
                    wrap: ((content - indent) * 0.7 - bubble_pad[0] * 2.0).max(em * 4.0),
                }
            }
        }
    }

    /// The height of a typing indicator's body: one line in a pill or bubble.
    pub fn typing_body(&self) -> f32 {
        let geometry = self.geometry();
        geometry.line + geometry.bubble_pad[1] * 2.0
    }

    /// The content area's `[top, bottom]` in canvas pixels.
    pub fn content_span(&self) -> [f32; 2] {
        let geometry = self.geometry();
        [
            self.origin[1] + geometry.header,
            self.origin[1] + self.size[1] - geometry.composer,
        ]
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.origin.iter().chain(&self.size).all(|v| v.is_finite()),
            "chat origin and size must be finite"
        );
        ensure!(
            (320.0..=3800.0).contains(&self.size[0]) && (200.0..=2160.0).contains(&self.size[1]),
            "chat size must be between [320, 200] and [3800, 2160]"
        );
        ensure!(
            (10.0..=48.0).contains(&self.text_size),
            "chat textSize must be between 10 and 48"
        );
        ensure!(!self.people.is_empty(), "a chat needs at least one person");
        ensure!(
            self.messages.len() <= MAX_MESSAGES,
            "chats are limited to {MAX_MESSAGES} messages"
        );
        let mut people = HashSet::new();
        for person in &self.people {
            ensure!(
                window::valid_id(&person.id),
                "chat person id '{}' must be letters, digits, '-', or '_'",
                person.id
            );
            ensure!(
                people.insert(person.id.as_str()),
                "chat person '{}' is repeated",
                person.id
            );
            ensure!(
                !person.name.trim().is_empty(),
                "chat person '{}' needs a name",
                person.id
            );
        }
        if let Some(me) = &self.me {
            ensure!(
                people.contains(me.as_str()),
                "chat `me` names unknown person '{me}'"
            );
        }
        let mut messages = HashSet::new();
        for message in &self.messages {
            ensure!(
                window::valid_id(&message.id),
                "chat message id '{}' must be letters, digits, '-', or '_'",
                message.id
            );
            ensure!(
                messages.insert(message.id.as_str()),
                "chat message '{}' is repeated",
                message.id
            );
            ensure!(
                people.contains(message.author.as_str()),
                "chat message '{}' is by unknown person '{}'",
                message.id,
                message.author
            );
            ensure!(
                message.chars() <= 2000,
                "chat message '{}' is longer than 2000 characters",
                message.id
            );
            let mut reactions = HashSet::new();
            for reaction in &message.reactions {
                ensure!(
                    window::valid_id(&reaction.id) && reactions.insert(reaction.id.as_str()),
                    "chat message '{}' reaction id '{}' must be unique letters, digits, '-', or '_'",
                    message.id,
                    reaction.id
                );
                ensure!(
                    !reaction.label.is_empty() && reaction.label.chars().count() <= 12,
                    "chat reaction '{}' needs a label of at most 12 characters",
                    reaction.id
                );
            }
        }
        Ok(())
    }

    /// Each message's block given its measured metrics and sampled pose,
    /// stacked up from the bottom of the content area (`bottom`, in pixels).
    pub fn layout(
        &self,
        metrics: &[ChatMetrics],
        poses: &[ChatPose],
        bottom: f32,
    ) -> Vec<ChatBlock> {
        assert_eq!(metrics.len(), self.messages.len(), "one metric per message");
        assert_eq!(poses.len(), self.messages.len(), "one pose per message");
        let geometry = self.geometry();
        let rooms = (0..self.messages.len())
            .map(|index| self.room(index, &geometry, &metrics[index], &poses[index]))
            .collect::<Vec<_>>();
        let mut top = bottom;
        let mut blocks = rooms
            .iter()
            .enumerate()
            .rev()
            .map(|(message, &(gap, room))| {
                top -= gap + room;
                ChatBlock {
                    message,
                    top: top + gap,
                    room,
                }
            })
            .collect::<Vec<_>>();
        blocks.reverse();
        blocks
    }

    /// The `(gap above, room)` a message takes.
    fn room(
        &self,
        index: usize,
        geometry: &ChatGeometry,
        metrics: &ChatMetrics,
        pose: &ChatPose,
    ) -> (f32, f32) {
        let typing = pose.typing.clamp(0.0, 1.0);
        let reveal = pose.reveal.max(0.0);
        let presence = typing.max(reveal.min(1.0));
        if presence <= 0.0 {
            return (0.0, 0.0);
        }
        let gap = geometry.gaps[usize::from(!self.group_start(index))] * presence;
        let waiting = typing * (metrics.header + self.typing_body());
        let said =
            metrics.header + metrics.body + metrics.reactions * pose.reactions.clamp(0.0, 1.0);
        // A spring may overshoot the message's room slightly; it never goes negative.
        (gap, lerp(waiting, said, reveal).max(0.0))
    }
}

/// One message's measured heights in pixels: its avatar/name row (zero when
/// it continues a group), its text, and its reaction row.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChatMetrics {
    pub header: f32,
    pub body: f32,
    pub reactions: f32,
}

/// One message's sampled channels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChatPose {
    pub typing: f32,
    pub reveal: f32,
    /// The presence of its reaction row: its most present reaction.
    pub reactions: f32,
}

/// Where one message's block sits this sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChatBlock {
    pub message: usize,
    /// Canvas y of the block's top (below its gap).
    pub top: f32,
    pub room: f32,
}

/// Authoring handle for one chat thread. Messages are appended as the scene
/// is authored, so every call grows the recipe and writes its channels.
#[derive(Clone, Debug)]
pub struct ChatActor {
    actor: ActorHandle,
    plan: ChatPlan,
    /// Typing slots waiting for their text: (message, author, typing began).
    waiting: Vec<(String, String, u64)>,
    time: Option<String>,
}

impl ChatActor {
    /// Messages already in `plan` are shown from time zero.
    pub fn declare(scene: &mut PlanBuilder, id: impl Into<String>, plan: ChatPlan) -> Result<Self> {
        plan.validate()?;
        let actor = scene.actor(id, CHAT_RECIPE, &plan)?;
        Ok(Self {
            actor,
            plan,
            waiting: Vec::new(),
            time: None,
        })
    }

    pub fn actor(&self) -> &ActorHandle {
        &self.actor
    }

    pub fn id(&self) -> &str {
        self.actor.id()
    }

    pub fn plan(&self) -> &ChatPlan {
        &self.plan
    }

    pub fn channel(
        &mut self,
        scene: &mut PlanBuilder,
        property: &str,
        initial: f32,
    ) -> ContinuousHandle {
        scene.channel(&self.actor, property, initial)
    }

    fn message_channel(
        &mut self,
        scene: &mut PlanBuilder,
        id: &str,
        channel: ChatChannel,
        initial: f32,
    ) -> ContinuousHandle {
        scene.channel(&self.actor, &message_property(id, channel), initial)
    }

    pub fn show(&mut self, scene: &mut PlanBuilder, at_nanos: u64) -> u64 {
        window::settle_in(scene, &self.actor, at_nanos)
    }

    pub fn hide(&mut self, scene: &mut PlanBuilder, at_nanos: u64) {
        window::dismiss(scene, &self.actor, at_nanos);
    }

    /// Stamp later messages with `label` (such as `10:42 AM`).
    pub fn stamp(&mut self, label: impl Into<String>) {
        self.time = Some(label.into());
    }

    fn commit(&mut self, scene: &mut PlanBuilder) -> Result<()> {
        self.plan.validate()?;
        scene.replace_actor_data(&self.actor, &self.plan)?;
        Ok(())
    }

    fn append(&mut self, scene: &mut PlanBuilder, author: &str) -> Result<String> {
        ensure!(
            self.plan.person(author).is_some(),
            "chat person '{author}' is not declared"
        );
        let id = format!("m{}", self.plan.messages.len());
        self.plan.messages.push(ChatMessagePlan {
            id: id.clone(),
            author: author.to_owned(),
            spans: Vec::new(),
            time: self.time.clone(),
            reactions: Vec::new(),
        });
        self.commit(scene)?;
        // Both start hidden, so the message has no slot until one opens it.
        self.message_channel(scene, &id, ChatChannel::Typing, 0.0);
        self.message_channel(scene, &id, ChatChannel::Reveal, 0.0);
        Ok(id)
    }

    /// Show `author` typing at `at_nanos`: a dots indicator opens the next
    /// message's slot until [`Self::say`] grows it into the message.
    pub fn typing(
        &mut self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        author: &str,
    ) -> Result<String> {
        ensure!(
            !self.waiting.iter().any(|(_, typist, _)| typist == author),
            "chat person '{author}' is already typing"
        );
        let id = self.append(scene, author)?;
        let typing = self.message_channel(scene, &id, ChatChannel::Typing, 0.0);
        scene.spring(&typing, at_nanos, 1.0, TYPING_SECONDS, 0.0);
        let wait = self.message_channel(scene, &id, ChatChannel::Wait, -1.0);
        scene.set(&wait, at_nanos, 0.0);
        let rest = (scene.duration_nanos().saturating_sub(at_nanos) / 1_000_000) as f32 / 1000.0;
        if rest > 0.0 {
            scene.ease(&wait, at_nanos, rest, rest, Ease::Linear);
        }
        self.waiting.push((id.clone(), author.to_owned(), at_nanos));
        Ok(id)
    }

    /// Say `spans` as `author` at `at_nanos`, growing their typing slot into
    /// the message, or opening a new one. Returns the message ID.
    pub fn say(
        &mut self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        author: &str,
        spans: Vec<ChatSpanPlan>,
    ) -> Result<String> {
        let waiting = self
            .waiting
            .iter()
            .position(|(_, typist, _)| typist == author);
        let id = match waiting {
            Some(index) => {
                let (id, _, began) = self.waiting.remove(index);
                // Stop the dots' clock once the message has replaced them.
                let wait = self.message_channel(scene, &id, ChatChannel::Wait, -1.0);
                let settled = at_nanos + seconds(f64::from(SAY_SECONDS));
                scene.set(&wait, settled, (settled.saturating_sub(began)) as f32 / 1e9);
                id
            }
            None => self.append(scene, author)?,
        };
        let index = self.plan.find(&id).expect("appended above");
        self.plan.messages[index].spans = spans;
        self.commit(scene)?;
        let reveal = self.message_channel(scene, &id, ChatChannel::Reveal, 0.0);
        scene.spring(&reveal, at_nanos, 1.0, SAY_SECONDS, SAY_BOUNCE);
        Ok(id)
    }

    /// Say one plain line of text.
    pub fn say_text(
        &mut self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        author: &str,
        text: &str,
    ) -> Result<String> {
        self.say(scene, at_nanos, author, vec![ChatSpanPlan::plain(text)])
    }

    /// Say `spans` with the text streaming in at `chars_per_second`, as a
    /// bot's reply does. Returns the message ID and when the text finishes.
    pub fn stream(
        &mut self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        author: &str,
        spans: Vec<ChatSpanPlan>,
        chars_per_second: f32,
    ) -> Result<(String, u64)> {
        let chars = spans
            .iter()
            .map(|span| span.text.chars().count())
            .sum::<usize>();
        let id = self.say(scene, at_nanos, author, spans)?;
        let typed = self.message_channel(scene, &id, ChatChannel::Typed, 0.0);
        let start = at_nanos + crate::author::CONTENT_LAG;
        let done = crate::caption::type_steps(scene, &typed, start, chars.max(1), chars_per_second);
        Ok((id, done))
    }

    /// Pop a reaction under `message` at `at_nanos`. Returns its ID.
    pub fn react(
        &mut self,
        scene: &mut PlanBuilder,
        at_nanos: u64,
        message: &str,
        label: &str,
        count: u32,
    ) -> Result<String> {
        let index = self
            .plan
            .find(message)
            .with_context(|| format!("chat message '{message}' is not declared"))?;
        let id = format!("r{}", self.plan.messages[index].reactions.len());
        self.plan.messages[index].reactions.push(ChatReactionPlan {
            id: id.clone(),
            label: label.to_owned(),
            count: count.max(1),
        });
        self.commit(scene)?;
        let channel = scene.channel(&self.actor, &reaction_property(message, &id), 0.0);
        scene.spring(&channel, at_nanos, 1.0, REACT_SECONDS, REACT_BOUNCE);
        Ok(id)
    }

    /// Light a wash behind `message` at `at_nanos` and let it go `for_seconds` later.
    pub fn highlight(
        &mut self,
        scene: &mut PlanBuilder,
        message: &str,
        at_nanos: u64,
        for_seconds: f32,
    ) -> Result<()> {
        ensure!(
            self.plan.find(message).is_some(),
            "chat message '{message}' is not declared"
        );
        let channel = self.message_channel(scene, message, ChatChannel::Highlight, 0.0);
        scene.spring(&channel, at_nanos, 1.0, HIGHLIGHT_IN_SECONDS, 0.0);
        let off = at_nanos + seconds(f64::from(for_seconds.max(0.0)));
        scene.spring(&channel, off, 0.0, HIGHLIGHT_OUT_SECONDS, 0.0);
        Ok(())
    }
}

/// Window channels plus the per-message and per-reaction channels.
pub fn accepts_property(property: &str) -> bool {
    window::is_window_property(property)
}

/// Bounce of a typing dot `index` (0..3) at `seconds` since typing began:
/// `(lift, ink)`, each 0 to 1. The dots rise in turn, a wave every 1.2 s.
pub fn typing_dot(seconds: f32, index: usize) -> (f32, f32) {
    if seconds < 0.0 {
        return (0.0, 0.4);
    }
    let phase = (seconds / 1.2 - index as f32 * 0.14).rem_euclid(1.0);
    // One smooth bump over the first half of the cycle, rest for the second.
    let bump = if phase < 0.5 {
        (std::f32::consts::PI * phase / 0.5).sin().powi(2)
    } else {
        0.0
    };
    (bump, 0.4 + 0.6 * bump)
}

#[cfg(test)]
mod tests;
