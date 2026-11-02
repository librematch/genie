//! What a unit and its action layer answer: their taker, its context, its gait, whether its top
//! action may be interrupted and the target it attacks.

use crate::GenieError;
use crate::math::fmath::BVector;
use crate::movement::Mover;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::Master;
use crate::objects::masters::action_type;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::unit::{TAKEN_OUT, Unit};

use crate::world::Unread;
use crate::world::checkpoint::TopAction;

impl Unit {
    /// Slot 0x520: where it last moved to, as the game's vector: a moving object's `+0x824`
    /// (`FUN_14106d530`, [`crate::movement::Mover::moved_at`] of its `mover`), any other's place
    /// (`FUN_141196c10`).
    #[must_use]
    pub(crate) fn moved_vector(&self, mover: Option<&Mover>) -> BVector {
        mover
            .map_or_else(|| self.position(), |m| m.moved_at)
            .vector()
    }

    /// The stand-in left in a list for a unit taken out of it ([`TAKEN_OUT`]).
    #[must_use]
    pub(crate) fn taken_out() -> Self {
        Self {
            id: TAKEN_OUT,
            ..Self::default()
        }
    }

    /// Whether a building's slot 0x178 or 0x180 (`FUN_1411dba70`, `FUN_1411d6020`) lists its taker
    /// among the attacking units (`+0xce0`, [`Building::attacking_units`](crate::objects::layers::Building::attacking_units)): this object of `master` is a
    /// building (type 80) below state 2 whose master has a stack unit (`+0x986`), and the taker's
    /// player is not allied with its owner (`FUN_141663d90` with 1: `allied` false).
    pub(crate) fn lists_taker(&self, master: &Master, allied: bool) -> bool {
        master.kind == BUILDING
            && self.life.state < ObjectState::Alive
            && master.building.is_some_and(|b| b.stack_unit.is_some())
            && !allied
    }

    /// Slot 0x178 on this object of its player's live list, taken by the object `by` (`listed`:
    /// its building layer, which lists it, [`Unit::lists_taker`]). A building's (`FUN_1411dba70`) first lists the taker among its attacking
    /// units (`+0xce0`, [`Building::attacking_units`](crate::objects::layers::Building::attacking_units); once per take). Then the base take
    /// (`FUN_14119d3e0`): its worker count (`+0x644`) rises, to at most 550, and a live object flagged
    /// to sleep drops the flags (a sleeping one's wake is [`crate::world::World::take`]'s). The
    /// building's wake from semi-asleep (`+0xc98`) after it never runs here: only the building's
    /// load (`FUN_1411df160` -> `FUN_1411ddd20`) sets that flag, from the save's
    /// `temp_semi_asleep`, which no saved building of the 158041 recordings has (1473 headers
    /// read); the slice does not hold it.
    pub(crate) fn taken(
        &mut self,
        listed: Option<&mut crate::objects::layers::Building>,
        by: ObjectId,
    ) {
        if let Some(layer) = listed {
            layer.attacking_units.push(by);
        }
        if self.workers < crate::objects::WorkerCount::MOST {
            self.workers = self.workers.plus(1);
        }
        self.life.goto_sleep = false;
    }

    /// `why` with what this object was doing in its context: its id and master, its top action
    /// (type and state) and its unit AI's (`ai`) order, action and target.
    #[must_use]
    pub fn in_context(
        &self,
        actions: &crate::objects::layers::Action,
        ai: Option<&crate::unit_ai::UnitAi>,
        why: GenieError,
    ) -> GenieError {
        let mut why = why
            .with("object", self.id)
            .with("master", self.life.master_id);
        if let Some(a) = actions.top_action() {
            why = why.with("action", format_args!("{} state {}", a.kind, a.state));
        }
        if let Some(ai) = ai {
            why = why.with(
                "unit AI",
                format_args!(
                    "order {} action {} target {}",
                    crate::unit_ai::OrderKind::field(ai.current_order),
                    crate::unit_ai::ActionKind::field(ai.current_action),
                    ObjectId::field(ai.current_target)
                ),
            );
        }
        why
    }

    /// The unit as its walk reads it ([`crate::movement::Me`]): its id, its top action's state
    /// (0 without one), its group and its last human order's time.
    #[must_use]
    pub fn me(&self, actions: &crate::objects::layers::Action) -> crate::movement::Me {
        crate::movement::Me {
            id: self.id,
            action_state: actions
                .top_action()
                .map_or(ActionState::NO_ACTION, |a| a.state),
            group: self.group.id,
            human_order: self.last_human_order,
        }
    }

    /// What the walk reads of it when it meets it ([`crate::map::spatial::Gait`]), with its
    /// `mover` (none for an object that does not move).
    #[must_use]
    pub fn gait(
        &self,
        actions: &crate::objects::layers::Action,
        mover: Option<&Mover>,
    ) -> Option<crate::map::spatial::Gait> {
        let m = mover?;
        Some(crate::map::spatial::Gait {
            moving: m
                .velocity
                .as_ref()
                .is_some_and(|v| 0.0 < crate::math::fmath::length(v.velocity)),
            waits: m.waiting != 0
                && !actions
                    .top_action()
                    .is_some_and(|a| matches!(a.state.get(), 6 | 7)),
            last_move: m.last_move,
            pause: m.paths.first().map(|l| l.pause_time),
            human_order: self.last_human_order,
            path_made: m
                .paths
                .first()
                .filter(|l| !l.points.is_empty())
                .map(|l| l.created),
            group: self.group.id,
        })
    }
}

impl crate::objects::layers::Action {
    /// Slot 0x2d0 with 0 (`FUN_14106d170`): whether its top action may be interrupted (the group
    /// test first; the slice holds no groups). None may; type `0x7e` never; by state, 0, 1, 3,
    /// 0xd and 0xe may, 2 unless an Attack (9); in any other state only an Artifact (`0x6b`)
    /// without a target or a point (`+0x30`, `+0x38` = −1). A trade (`0x6f`) in state 4 is not
    /// modelled, nor is an action the slice does not hold (`unread`: the load could not read the
    /// object whole, [`Unread`]).
    pub(crate) fn interruptible(&self, unread: Option<&Unread>) -> Result<bool, GenieError> {
        if unread.is_some() {
            return Err(GenieError::unsupported(
                "slot 0x2d0 of an action the slice does not model",
            ));
        }
        let Some(TopAction { kind, state }) = self.top_action() else {
            return Ok(true);
        };
        Ok(match (kind, state.get()) {
            (action_type::ACTION_126, _) => false,
            (action_type::TRADE, 4) => {
                return Err(GenieError::unsupported("slot 0x2d0 of a trade in state 4"));
            }
            (_, 0 | 1 | 3 | 0xd | 0xe) => true,
            (_, 2) => kind != crate::combat::attack::ATTACK,
            (crate::ownership::artifact::ARTIFACT_TYPE, _) => self
                .list
                .artifact()
                .is_some_and(|a| a.goal.is_none_or(|g| g.x() == -1.0)),
            _ => false,
        })
    }

    /// The target of its hunt, which it attacks or gathers from: the first of its actions that
    /// decides, the Hunt, a Gather with a target, the Attack and a missile with a target (a Hunt
    /// or an Attack without one decides none).
    pub(crate) fn attack_target(&self) -> Option<ObjectId> {
        self.list
            .hunt()
            .map(|h| h.target)
            .or_else(|| self.list.gather().and_then(|g| g.target).map(Some))
            .or_else(|| self.list.attack().map(|a| a.target))
            .or_else(|| self.list.missile().and_then(|a| a.target).map(Some))
            .flatten()
    }
}
