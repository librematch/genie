//! What the engine tells a [`tracing`] subscriber while it runs, for debugging.
//!
//! The library installs no subscriber: a binary that runs the engine does (genie-oracle's
//! `census` and `checkpoints` examples print text lines to stderr once `RUST_LOG` is set).
//! Without a subscriber, or with its filter below a span's or event's level, each costs one
//! comparison with `tracing`'s maximum level, and the state watch is not taken.
//!
//! Three targets, so a filter can pick (`RUST_LOG=genie_core::unsupported=warn` …):
//!
//! - `genie_core::update`: the span `update` (`tick`, `world_time`; DEBUG) of each world update,
//!   inside it the event `command` (`player`, `command`; DEBUG) of each recorded command, the
//!   span `player` (`player`; DEBUG) of each player's update and the span `object` (`id`,
//!   `master`; TRACE) of each object's update.
//! - `genie_core::unsupported` (WARN): where a case the engine does not model lands: an object
//!   freezes (`object`) or a whole player stops, with the update (`tick`), the player and the
//!   case: `why`, the reason of its [`GenieError`]; `at`, where it was raised; `context`, what
//!   its site attached, when it did.
//! - `genie_core::state` (DEBUG): a new object took the world's next object id (the event
//!   `new_object`: `object`, `player`, `master`, `tick`), an object's update changed its state (the event `object_state`,
//!   `from` and `to`), its top action (`action`: the type `from` and `to`, the state `from_state`
//!   and `to_state`) or its unit AI (`unit_ai`: `from_order`, `to_order`, `from_ai_action`,
//!   `to_ai_action`, `from_target`, `to_target`); −1 stands for none. Each carries `tick`,
//!   `player` and `object`.
//! - `genie_core::draws` (TRACE): each step of a random generator (the event `draw`: `stream`,
//!   the [`crate::math::rng::Stream`] as its number, 255 for a generator outside the eleven; `draw`,
//!   the generator's draw count after it; `value`, the raw step; `at`, the engine's line that
//!   asked for it). Compared with the tick oracle's `ai_draws`, it finds the update where a
//!   stream's draws part from the game's.

use crate::GenieError;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::masters::MasterId;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::{AiAction, OrderKind, UnitAi};
use crate::world::Tick;
use std::panic::Location;

/// The target of the update, player and object spans and the command events.
pub const UPDATE: &str = "genie_core::update";
/// The target of the events where an unmodelled case lands.
pub const UNSUPPORTED: &str = "genie_core::unsupported";
/// The target of the state change events.
pub const STATE: &str = "genie_core::state";
/// The target of the random generators' steps.
pub const DRAWS: &str = "genie_core::draws";

/// A random generator's step ([`DRAWS`]); `at` is the line of the engine that asked for it.
#[track_caller]
pub(crate) fn draw(stream: u8, draw: u32, value: u32) {
    tracing::trace!(
        target: DRAWS,
        stream,
        draw,
        value,
        at = %Location::caller()
    );
}

/// A new object of `master` for `player` took the world's next object id `object` in update
/// `tick` ([`STATE`]); `at` is the line of the engine that made it.
#[track_caller]
pub(crate) fn new_object(tick: Tick, player: PlayerId, object: ObjectId, master: MasterId) {
    tracing::debug!(
        target: STATE,
        tick = tick.get(),
        player = player.index(),
        object = object.get(),
        master = %master,
        at = %Location::caller(),
        "new_object"
    );
}

/// The context of `why` as one text (`key: value, …`), none without any.
fn context_of(why: &GenieError) -> Option<String> {
    let parts: Vec<String> = why
        .context()
        .map(|(key, text)| format!("{key}: {text}"))
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// An object's update meets a case the engine does not model: the object freezes.
pub(crate) fn object_freezes(tick: Tick, player: PlayerId, object: ObjectId, why: &GenieError) {
    let (reason, at) = (why.reason(), why.location());
    if let Some(context) = context_of(why) {
        tracing::warn!(
            target: UNSUPPORTED,
            tick = tick.get(),
            player = player.index(),
            object = object.get(),
            why = reason,
            at = %at,
            context,
            "an object freezes"
        );
    } else {
        tracing::warn!(
            target: UNSUPPORTED,
            tick = tick.get(),
            player = player.index(),
            object = object.get(),
            why = reason,
            at = %at,
            "an object freezes"
        );
    }
}

/// A command or a pass meets a case the engine does not model: the whole player stops.
pub(crate) fn player_stops(tick: Tick, player: PlayerId, why: &GenieError) {
    let (reason, at) = (why.reason(), why.location());
    if let Some(context) = context_of(why) {
        tracing::warn!(
            target: UNSUPPORTED,
            tick = tick.get(),
            player = player.index(),
            why = reason,
            at = %at,
            context,
            "a player stops"
        );
    } else {
        tracing::warn!(
            target: UNSUPPORTED,
            tick = tick.get(),
            player = player.index(),
            why = reason,
            at = %at,
            "a player stops"
        );
    }
}

/// A case the engine does not model after an object's update (its leave, a capture's transfer,
/// a new object's unit AI): the player's first unmodelled case, without a freeze.
pub(crate) fn player_unsupported(tick: Tick, player: PlayerId, why: &GenieError) {
    let (reason, at) = (why.reason(), why.location());
    if let Some(context) = context_of(why) {
        tracing::warn!(
            target: UNSUPPORTED,
            tick = tick.get(),
            player = player.index(),
            why = reason,
            at = %at,
            context,
            "a case not modelled"
        );
    } else {
        tracing::warn!(
            target: UNSUPPORTED,
            tick = tick.get(),
            player = player.index(),
            why = reason,
            at = %at,
            "a case not modelled"
        );
    }
}

/// What the state events compare of an object before and after its update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Watch {
    /// The object's state (`+0x640`).
    object_state: ObjectState,
    /// Its top action's type and state.
    action: Option<(i16, u8)>,
    /// Its unit AI's order, action and target.
    order: Option<[i32; 3]>,
}

impl Watch {
    /// The watch of `unit`, its action layer `actions` and its unit AI `ai`, taken only while a
    /// subscriber listens to the state events.
    pub(crate) fn of(unit: &Unit, actions: &Action, ai: Option<&UnitAi>) -> Option<Self> {
        tracing::enabled!(target: STATE, tracing::Level::DEBUG).then(|| Self {
            object_state: unit.life.state,
            action: actions
                .top_action()
                .map(|top_action| (top_action.kind.get(), top_action.state.get())),
            order: ai.map(|ai| {
                [
                    OrderKind::field(ai.current_order),
                    AiAction::field(ai.current_action),
                    ObjectId::field(ai.current_target),
                ]
            }),
        })
    }

    /// Tell the subscriber what changed from `self` to `unit` (its action layer `actions`, its
    /// unit AI `ai`) in the update `tick` of `player`.
    pub(crate) fn report(
        self,
        (unit, actions, ai): (&Unit, &Action, Option<&UnitAi>),
        tick: Tick,
        player: PlayerId,
    ) {
        let Some(now) = Self::of(unit, actions, ai) else {
            return;
        };
        let object = unit.id.get();
        if now.object_state != self.object_state {
            tracing::debug!(
                target: STATE,
                tick = tick.get(),
                player = player.index(),
                object,
                from = self.object_state.get(),
                to = now.object_state.get(),
                "object_state"
            );
        }
        if now.action != self.action {
            let field = |action: Option<(i16, u8)>| action.map_or(-1, |(kind, _)| i32::from(kind));
            let state =
                |action: Option<(i16, u8)>| action.map_or(-1, |(_, state)| i32::from(state));
            tracing::debug!(
                target: STATE,
                tick = tick.get(),
                player = player.index(),
                object,
                from = field(self.action),
                to = field(now.action),
                from_state = state(self.action),
                to_state = state(now.action),
                "action"
            );
        }
        if now.order != self.order {
            let [from_order, from_ai_action, from_target] = self.order.unwrap_or([-1; 3]);
            let [to_order, to_ai_action, to_target] = now.order.unwrap_or([-1; 3]);
            tracing::debug!(
                target: STATE,
                tick = tick.get(),
                player = player.index(),
                object,
                from_order,
                to_order,
                from_ai_action,
                to_ai_action,
                from_target,
                to_target,
                "unit_ai"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Command, UnmodelledCommand};
    #[cfg(test)]
    use crate::math::rng::{Stream, Streams, Well512};
    use crate::objects::HitPoints;
    use crate::objects::ListIndex;
    use crate::objects::lifecycle::{Class, Lifecycle};
    use crate::test_ids::object_id;
    use crate::test_ids::player_id;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::WorldTime;
    #[cfg(test)]
    use crate::world::tests::stock_masters;
    use crate::world::{Player, World};
    #[cfg(test)]
    use std::fmt::Debug;
    use std::fmt::Write;
    use std::sync::{Arc, Mutex};
    #[cfg(test)]
    use tracing::callsite::rebuild_interest_cache;
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    #[cfg(test)]
    use tracing::subscriber::with_default;
    use tracing::{Event, Metadata};

    /// A subscriber that writes down every span and event as `level target name field=value…`.
    #[derive(Clone, Default)]
    struct Recorder {
        lines: Arc<Mutex<Vec<String>>>,
    }

    /// A span's name and fields, or an event's message and fields.
    struct Line(String);

    impl Visit for Line {
        fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
            let _ = if field.name() == "message" {
                write!(self.0, " {value:?}")
            } else {
                write!(self.0, " {}={value:?}", field.name())
            };
        }
    }

    impl Recorder {
        fn push(&self, meta: &Metadata<'_>, line: &Line) {
            let head = format!("{} {}", meta.level(), meta.target());
            self.lines.lock().unwrap().push(head + &line.0);
        }

        fn lines(&self) -> Vec<String> {
            self.lines.lock().unwrap().clone()
        }
    }

    impl tracing::Subscriber for Recorder {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, span: &Attributes<'_>) -> Id {
            let mut line = Line(format!(" {}", span.metadata().name()));
            span.record(&mut line);
            self.push(span.metadata(), &line);
            Id::from_u64(1)
        }

        fn record(&self, _: &Id, _: &Record<'_>) {}

        fn record_follows_from(&self, _: &Id, _: &Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut line = Line(String::new());
            event.record(&mut line);
            self.push(event.metadata(), &line);
        }

        fn enter(&self, _: &Id) {}

        fn exit(&self, _: &Id) {}
    }

    /// The lines a subscriber hears while `run` runs. The interest cache is rebuilt inside the
    /// scope: a callsite another test registered at the same time, with no subscriber yet, may
    /// hold a stale "never".
    fn heard_with_draws(run: impl FnOnce()) -> Vec<String> {
        let recorder = Recorder::default();
        with_default(recorder.clone(), || {
            rebuild_interest_cache();
            run();
        });
        recorder.lines()
    }

    /// [`heard_with_draws`] without the random generators' steps.
    fn heard(run: impl FnOnce()) -> Vec<String> {
        let mut lines = heard_with_draws(run);
        lines.retain(|text| !text.contains(DRAWS));
        lines
    }

    fn one(objects: Vec<Unit>) -> World {
        let player = Player {
            objects: objects.into(),
            ..Player::default()
        };
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        stock_masters(&mut world);
        world
    }

    /// Each step names its stream, the draw count, the raw value and the engine's line that
    /// asked for it, through every draw helper.
    #[test]
    fn draw_tells_its_stream_value_and_the_line_that_asked() {
        let mut streams = Streams::default();
        streams.seed_all(29910);
        let ai = streams.get_mut(Stream::Ai);
        let mut copy = ai.clone();
        let (lines, line) = (
            heard_with_draws(|| {
                let _ = ai.rand15();
            }),
            line!() - 2,
        );
        let value = copy.raw();
        let draws: Vec<&String> = lines.iter().filter(|text| text.contains(DRAWS)).collect();
        assert_eq!(draws.len(), 1, "{lines:?}");
        // the path as the compiler gives it (`file!()`, backslashes on Windows)
        let head = format!(
            "TRACE {DRAWS} stream=4 draw={} value={value} at={}:{line}:",
            copy.draws,
            file!()
        );
        assert!(draws[0].starts_with(&head), "{} against {head}", draws[0]);
        // a generator outside the streams, and a helper of two steps
        let mut lone = Well512::default();
        lone.set_seed(1);
        let lines = heard_with_draws(|| {
            let _ = lone.raw64();
        });
        let lone_draws = lines
            .iter()
            .filter(|text| text.contains("stream=255"))
            .count();
        assert_eq!(lone_draws, 2, "{lines:?}");
    }

    #[test]
    fn without_subscriber_no_watch_is_taken() {
        assert_eq!(Watch::of(&Unit::default(), &Action::default(), None), None);
    }

    #[test]
    fn an_update_tells_its_span_its_players_and_its_objects() {
        let mut world = one(vec![Unit {
            id: object_id(7),
            ..Unit::default()
        }]);
        let lines = heard(|| world.tick(Millis::new(10)));
        // each step of the schedule in its span, the player's in the player's
        assert_eq!(
            lines[..10],
            [
                "DEBUG genie_core::update update tick=1 world_time=10",
                "TRACE genie_core::update step step=Commands",
                "DEBUG genie_core::update player player=0",
                "TRACE genie_core::update step step=Clock",
                "TRACE genie_core::update step step=OpenGates",
                "TRACE genie_core::update step step=StartWork",
                "TRACE genie_core::update step step=Groups",
                "TRACE genie_core::update step step=WorldPlayer",
                "TRACE genie_core::update step step=Objects",
                "TRACE genie_core::update object id=7 master=0",
            ]
        );
    }

    #[test]
    fn an_unread_object_tells_why_it_freezes() {
        let why =
            GenieError::unsupported("a default task the slice does not model").with("site", 1);
        let at = why.location();
        let mut world = one(vec![Unit {
            id: object_id(5),
            ..Unit::default()
        }]);
        world.set_unread(object_id(5), why);
        let lines = heard(|| world.tick(Millis::new(10)));
        let warned: Vec<_> = lines
            .iter()
            .filter(|text| text.starts_with("WARN"))
            .collect();
        assert_eq!(
            warned,
            [&format!(
                "WARN genie_core::unsupported an object freezes tick=1 player=0 object=5 \
                 why=\"a default task the slice does not model\" at={at} \
                 context=\"site: 1, object: 5, master: 0\""
            )]
        );
    }

    #[test]
    fn an_unmodelled_command_tells_the_command_and_why_its_player_stops() {
        let why = GenieError::unsupported("QUEUE at two buildings");
        let at = why.location();
        let mut world = one(Vec::new());
        let lines = heard(|| {
            world.tick_with(
                Millis::new(104),
                &[Command::Unmodelled(UnmodelledCommand {
                    player: player_id(0),
                    packet: Vec::new(),
                    why,
                })],
            );
        });
        assert_eq!(lines[1], "TRACE genie_core::update step step=Commands");
        assert!(lines[2].starts_with("DEBUG genie_core::update command command=Unmodelled"));
        assert!(lines.contains(&format!(
            "WARN genie_core::unsupported a player stops tick=1 player=0 \
             why=\"QUEUE at two buildings\" at={at}"
        )));
    }

    #[test]
    fn dying_object_tells_its_state_change() {
        let mut world = one(vec![Unit {
            id: object_id(5),
            life: Lifecycle {
                class: Class::Animated,
                hp: HitPoints::ZERO,
                ..Lifecycle::default()
            },
            ..Unit::default()
        }]);
        let before = world.players[0].objects[ListIndex::new(0)].life.state;
        let lines = heard(|| world.tick(Millis::new(10)));
        let after = world.players[0].objects[ListIndex::new(0)].life.state;
        assert_ne!(before, after, "{lines:#?}");
        assert!(lines.contains(&format!(
            "DEBUG genie_core::state object_state tick=1 player=0 object=5 from={before} to={after}"
        )), "{lines:#?}");
    }
}
