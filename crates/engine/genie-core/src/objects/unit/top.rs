//! What a unit's actions hold: their taker, its top action, target and gait.

use crate::GenieError;
use crate::math::fmath::BVector;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::actions::Action;
use crate::objects::masters::ActionType;
use crate::objects::masters::Master;
use crate::objects::masters::action_type;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::unit::{TAKEN_OUT, Unit};

use crate::world::checkpoint::TopAction;

impl Unit {
    /// Slot 0x520: where it last moved to, as the game's vector: a moving object's `+0x824`
    /// (`FUN_14106d530`, [`crate::movement::Mover::moved_at`]), any other's place
    /// (`FUN_141196c10`).
    #[must_use]
    pub(crate) fn moved_vector(&self) -> BVector {
        self.body
            .mover()
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

    /// The target of its hunt, which it attacks or gathers from: the first of its actions that
    /// decides, the Hunt, a Gather with a target, the Attack and a missile with a target (a Hunt
    /// or an Attack without one decides none).
    pub(crate) fn attack_target(&self) -> Option<ObjectId> {
        self.actions
            .list
            .hunt()
            .map(|h| h.target)
            .or_else(|| self.actions.list.gather().and_then(|g| g.target).map(Some))
            .or_else(|| self.actions.list.attack().map(|a| a.target))
            .or_else(|| self.actions.list.missile().and_then(|a| a.target).map(Some))
            .flatten()
    }

    /// `why` with what this object was doing in its context: its id and master, its top action
    /// (type and state) and its unit AI's (`ai`) order, action and target.
    #[must_use]
    pub fn in_context(&self, ai: Option<&crate::unit_ai::UnitAi>, why: GenieError) -> GenieError {
        let mut why = why
            .with("object", self.id)
            .with("master", self.life.master_id);
        if let Some(a) = self.top_action() {
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
    pub fn me(&self) -> crate::movement::Me {
        crate::movement::Me {
            id: self.id,
            action_state: self
                .top_action()
                .map_or(ActionState::NO_ACTION, |a| a.state),
            group: self.group.id,
            human_order: self.last_human_order,
        }
    }

    /// What the walk reads of it when it meets it ([`crate::map::spatial::Gait`]).
    #[must_use]
    pub fn gait(&self) -> Option<crate::map::spatial::Gait> {
        let m = self.body.mover()?;
        Some(crate::map::spatial::Gait {
            moving: m
                .velocity
                .as_ref()
                .is_some_and(|v| 0.0 < crate::math::fmath::length(v.velocity)),
            waits: m.waiting != 0
                && !self
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

    /// Its slot 0x190 (`FUN_14106d090`, then `FUN_140a78450`): the object its top action targets
    /// (the action's `+0x30` when not −1); none without a top action.
    ///
    /// # Errors
    ///
    /// A top action the slice does not model.
    pub fn top_action_object(&self) -> Result<Option<ObjectId>, GenieError> {
        if self.actions.unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "slot 0x190 of an action the slice does not model",
            ));
        }
        Ok(self.top_target())
    }

    /// Its top action's slots 0x190 and 0x288 (`FUN_14106d090`, `FUN_141082dc0`): the action has
    /// neither a target nor a point (`+0x30`, `+0x38` −1), or there is none. An Artifact holds
    /// them while it walks a move ([`crate::ownership::artifact::Artifact::idle`]); a `MoveTo` always holds
    /// its point.
    ///
    /// # Errors
    ///
    /// A top action the slice does not model.
    pub fn top_action_idle(&self) -> Result<bool, GenieError> {
        if self.actions.unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "slots 0x190 and 0x288 of an action the slice does not model",
            ));
        }
        match self.top_action_kind() {
            None => Ok(true),
            Some(crate::ownership::artifact::ARTIFACT_TYPE) => Ok(self
                .actions
                .list
                .artifact()
                .is_some_and(crate::ownership::artifact::Artifact::idle)),
            // a Make has no target, but its point (`+0x38`) is the base constructor's 0.0
            Some(crate::movement::move_to::MOVE_TO | crate::gathering::make::MAKE) => Ok(false),
            Some(_) => Err(GenieError::unsupported(
                "slots 0x190 and 0x288 of an action the slice does not model",
            )),
        }
    }

    /// The type of its top action, if it has one.
    #[must_use]
    pub fn top_action_kind(&self) -> Option<ActionType> {
        self.top_action().map(|a| a.kind)
    }

    /// Its top action (the list's head) as the slice models it, type and state.
    #[must_use]
    pub fn top_action(&self) -> Option<TopAction> {
        self.actions.list.head().map(|a| TopAction {
            kind: a.kind(),
            state: a.state(),
        })
    }

    /// The sub-action of its top action as the slice models it, type and state: the `MoveTo`
    /// (type 1) an action walks with, a Hunt's Attack (9) and an Artifact's Enter (3). None for
    /// an action without one, or without an action.
    #[must_use]
    pub fn top_sub_action(&self) -> Option<TopAction> {
        use crate::movement::move_to::{MOVE_TO, MoveTo};
        let walking = |m: Option<MoveTo>| {
            m.map(|m| TopAction {
                kind: MOVE_TO,
                state: ActionState::new(m.number()),
            })
        };
        match self.actions.list.head()? {
            Action::Artifact(a) => walking(a.move_to.map(|m| m.state)).or_else(|| {
                a.enter.map(|e| TopAction {
                    kind: crate::garrison::ENTER,
                    state: ActionState::new(e.state.get()),
                })
            }),
            Action::Hunt(h) => h
                .attack
                .map(|a| TopAction {
                    kind: crate::combat::attack::ATTACK,
                    state: ActionState::new(a.state.get()),
                })
                .or_else(|| walking(h.move_to.map(|m| m.state))),
            Action::Build(b) => walking(b.move_to.map(|m| m.state)),
            Action::Repair(r) => walking(r.move_to.map(|m| m.state)),
            Action::Fly(f) => walking(f.move_to.map(|m| m.state)),
            Action::Gather(g) => walking(g.move_to.map(|m| m.state)),
            Action::Attack(a) => walking(a.move_to.map(|m| m.state)),
            Action::Explore(e) => walking(e.move_to.map(|m| m.state)),
            Action::Enter(e) => walking(e.move_to.map(|m| m.state)),
            Action::Transport(t) => walking(t.move_to.map(|m| m.state)),
            Action::MoveTo(_) | Action::Missile(_) | Action::Make(_) | Action::WaitSolidify(_) => {
                None
            }
        }
    }

    /// The target of its top action ([`Unit::top_action`], [`Action::target`]); None without an
    /// action the slice holds or without a target.
    #[must_use]
    pub fn top_target(&self) -> Option<ObjectId> {
        self.actions.list.head().and_then(Action::target)
    }

    /// The second target of its top action ([`Action::second_target`]); None without an action
    /// the slice holds or without a second target.
    #[must_use]
    pub fn top_second_target(&self) -> Option<ObjectId> {
        self.actions.list.head().and_then(Action::second_target)
    }

    /// Slot 0x2d0 with 0 (`FUN_14106d170`): whether its top action may be interrupted (the group
    /// test first; the slice holds no groups). None may; type `0x7e` never; by state, 0, 1, 3,
    /// 0xd and 0xe may, 2 unless an Attack (9); in any other state only an Artifact (`0x6b`)
    /// without a target or a point (`+0x30`, `+0x38` = −1). A trade (`0x6f`) in state 4 is not
    /// modelled, nor is an action the slice does not hold.
    pub(crate) fn interruptible(&self) -> Result<bool, GenieError> {
        if self.actions.unmodelled_action.is_some() {
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
                .actions
                .list
                .artifact()
                .is_some_and(|a| a.goal.is_none_or(|g| g.x() == -1.0)),
            _ => false,
        })
    }
}
