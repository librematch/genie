//! The notifies' processing: their queue, their acceptance and their buffer.

use crate::map::Point;
use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::players::PlayerKind;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::StopOrder;
use crate::unit_ai::world_view::UnitView;
use crate::unit_ai::{BREAK, DEAD, GO_ON, Notify, PRODUCED, Surroundings, UnitAi};
use crate::world::Millis;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::layer::generated::UnitAiTuning;
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};
use std::mem::take;

/// What a unit AI's call from outside its full update reads of the build layer
/// ([`UnitAi::clear_notifies_at`], [`UnitAi::take_order_now`]; its orders' run passes it on): the
/// roles, the slots and the unit AI's tuning values (FD-046-D45, D49).
#[derive(Clone, Copy, Debug)]
pub struct AiLayer<'model> {
    /// The build's roles.
    pub(crate) roles: &'model Roles,
    /// The build's slots.
    pub(crate) slots: Slots,
    /// The unit AI's tuning values.
    pub(crate) unit_ai_tuning: &'model UnitAiTuning,
}

/// A unit's notifies to itself (its slot 0x328, [`UnitAi::post`]), borrowed beside its body: its
/// unit AI (None for a unit without one), its id and its state.
#[derive(Debug)]
pub struct Inbox<'ai> {
    /// The unit's AI.
    pub(crate) ai: Option<&'ai mut UnitAi>,
    /// The unit's id: the notify's caller and its recipient.
    pub(crate) id: ObjectId,
    /// The unit's state, which the AI's acceptance reads ([`UnitAi::accept`]).
    pub(crate) state: ObjectState,
}

impl<'ai> Inbox<'ai> {
    /// The inbox of the unit `id` in `state` with its AI `ai`, borrowed beside its body.
    pub const fn of(ai: &'ai mut Option<UnitAi>, id: ObjectId, state: ObjectState) -> Self {
        Self {
            ai: ai.as_mut(),
            id,
            state,
        }
    }

    /// Posts the notify `kind` with `params` to the unit itself ([`Notify::own`]); a unit without
    /// an AI takes none.
    ///
    /// # Errors
    ///
    /// The post's.
    pub fn post(&mut self, kind: NotifyKind, params: [i32; 3]) -> GenieResult<()> {
        match self.ai.as_deref_mut() {
            Some(ai) => ai.post(Notify::own(self.id, kind, params), self.state),
            None => Ok(()),
        }
    }
}

impl UnitAi {
    /// The notify processing of the full update: while notifies are queued and the last handler
    /// said go on, each runs through slot 0x238 ([`UnitAi::handle`]); a pass that went through
    /// them all ends with `FUN_140c5dc10`; 3 and 0xd break off. Returns the last handler's
    /// result (2 without notifies). Notifies posted during the pass (`FUN_140c55270`'s buffer)
    /// do not occur in the slice.
    pub(super) fn process_notifies(
        &mut self,
        dt: Millis,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai_rng: &mut Well512,
        side: &mut UnitView<'_, '_>,
    ) -> GenieResult<u32> {
        let mut result = GO_ON;
        while !self.notifies.is_empty() && result == GO_ON {
            self.processing = true;
            let mut index = 0;
            while index < self.notifies.len() {
                let notify = *self.notifies.checked(index)?;
                result = self.handle(notify, dt, at, player_kind, ai_rng, side)?;
                if result == DEAD {
                    // "I'm dead, purging notifyQueue and returning FALSE.": the full update ends
                    self.processing = false;
                    self.flush_buffered();
                    self.end_notifies(dt, at, side, ai_rng)?;
                    return Ok(DEAD);
                }
                if result == BREAK || result == PRODUCED {
                    break;
                }
                index += 1;
            }
            if result == GO_ON {
                self.end_notifies(dt, at, side, ai_rng)?;
            }
            self.processing = false;
            self.flush_buffered();
        }
        Ok(result)
    }

    /// `FUN_140c5dc10`: a completion of the current action (with no order) stops it
    /// (`FUN_140c5e1e0`, slot 0xb0 with 1, [`UnitAi::completes`]); then the notifies go and
    /// `+0x15c` takes `time`.
    pub(crate) fn end_notifies(
        &mut self,
        time: Millis,
        at: &Surroundings,
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
    ) -> GenieResult<()> {
        if self.completes() {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, rng)?;
        }
        self.last_update_time = time;
        self.notifies.clear();
        Ok(())
    }

    /// A notify of the current action's completion is queued while the AI has no order
    /// ([`UnitAi::end_notifies`] then stops the action).
    fn completes(&self) -> bool {
        self.notifies.iter().any(|notify| {
            notify.kind == NotifyKind::ACTION_COMPLETED
                && self.current_order.is_none()
                && AiAction::field(self.current_action) == notify.params[0]
        })
    }

    /// `FUN_140c5dc10` from outside the full update for a unit at `place` (its place and its
    /// master's class, which a stop ending an order reads): a completion stops the action (slot
    /// 0xb0 with 1), whose stopAction (slot 0x3c0) is a call on the unit's `side` at once; `rng`
    /// is the AI stream; `layer` is what the stop reads of the build layer.
    ///
    /// # Errors
    ///
    /// What the stop does not model.
    pub(crate) fn clear_notifies_at(
        &mut self,
        time: Millis,
        place: (Point, Option<ClassId>),
        side: &mut UnitView<'_, '_>,
        rng: &mut Well512,
        layer: AiLayer<'_>,
    ) -> GenieResult<()> {
        let at = Surroundings {
            position: place.0.with_height(0.0),
            class: place.1,
            roles: layer.roles.clone(),
            slots: layer.slots,
            unit_ai_tuning: *layer.unit_ai_tuning,
            ..Surroundings::default()
        };
        self.end_notifies(time, &at, side, rng)
    }

    /// `FUN_140c5dc10` from outside the full update (a MOVE, an ownership change: `time` is the
    /// world time), where a completion it would stop on is not modelled.
    ///
    /// # Errors
    ///
    /// A completion that would stop the action.
    pub fn clear_notifies(&mut self, time: Millis) -> GenieResult<()> {
        if self.completes() {
            return Err(GenieError::unsupported(
                "a completion stopping the action outside the full update",
            ));
        }
        self.last_update_time = time;
        self.notifies.clear();
        Ok(())
    }

    /// The unit's notify (its slot 0x328, `FUN_141086a10`) to its unit AI: a unit in state 2 (or
    /// state 0 with a damage notify, which slot 0x38 refuses there) passes it to the AI's slot
    /// 0x38 ([`UnitAi::accept`]). A notify the AI does not take (any but 699) goes to the
    /// owner's slot 0x2e8 (`FUN_14162dab0`, the human player's alerts), which the slice does not
    /// hold.
    pub fn post(&mut self, notify: Notify, unit_state: ObjectState) -> GenieResult<()> {
        let _ = self.accept(notify, unit_state);
        Ok(())
    }

    /// The unit AI's slot 0x38 (`FUN_140c556d0`, every type's): outside state 2 it takes nothing
    /// (0). A notify 699 is taken only from one of its attackers (a binary search of `+0xc0`),
    /// an action's invalidation only of the current action, and a movement completion
    /// (`0x1fa` with `0x262`) while the AI has an action other than a move (`0x262`, `0x268`,
    /// `0x2bf`, `0x2c0`) is ignored, answering 1 without joining the queue; any other joins the
    /// queue (`FUN_140c42470`, whose 0 on a failed allocation cannot happen) and answers 1.
    pub fn accept(&mut self, notify: Notify, unit_state: ObjectState) -> Answer {
        if unit_state != ObjectState::Alive {
            return Answer::NotTaken;
        }
        let action = self.current_action;
        let taken = match notify.kind {
            // "That unit is not attacking me, so I'm ignoring it."
            NotifyKind::NO_LONGER_ON_ME => self.attackers.contains(notify.caller),
            NotifyKind::ACTION_INVALIDATED => notify.params[0] == AiAction::field(action),
            NotifyKind::ACTION_COMPLETED => {
                // ignored (and handled) for a non-movement action
                // the game's list also holds 0x268, which no unit AI holds ([`AiAction`])
                if notify.params[0] == AiAction::Move.value()
                    && !matches!(
                        action,
                        Some(AiAction::Move | AiAction::MoveAside | AiAction::GroupAside)
                    )
                {
                    return Answer::Taken;
                }
                true
            }
            _ => true,
        };
        if taken {
            self.append(notify);
            Answer::Taken
        } else {
            Answer::NotTaken
        }
    }

    /// `FUN_140c42470`: a notify joins the queue; a full queue doubles its capacity, but while
    /// it is processed the notify goes to the buffer instead.
    pub(super) fn append(&mut self, notify: Notify) {
        if self.notify_capacity <= self.notifies.len() {
            if self.processing {
                self.buffered.push(notify);
                return;
            }
            self.notify_capacity *= 2;
        }
        self.notifies.push(notify);
    }

    /// `FUN_140c55270`: the buffered notifies join the queue, in order.
    pub(super) fn flush_buffered(&mut self) {
        for notify in take(&mut self.buffered) {
            self.append(notify);
        }
    }
}
