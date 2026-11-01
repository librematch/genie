//! The `MoveTo`s the Attack's `set_state` makes (`0x140a820c0`, cases 4, 5 and 11) and the update
//! of its sub-action list (`FUN_140a992b0`).

use crate::GenieError;
use crate::combat::attack::AttackState;
use crate::combat::attack::{ATTACK_WITHOUT_TARGET, Attack, AttackReads};
use crate::map::Distance;
use crate::map::Position;
use crate::movement::Me;
use crate::movement::Speed;
use crate::movement::move_to::{GiveUp, MoveTo, MoveToAction, MoveToReads, SpriteReads, WalkTo};
use crate::movement::move_to::{ListUpdate, update_sub_actions};
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
use crate::objects::unit::Unit;
use crate::world::Seconds;
use crate::world::step::Step;

/// A sub-action list's code that a quiet `MoveTo` does not give: 4 is its target let go when it
/// is not quiet.
pub(super) const QUIET_CODE: GenieError =
    GenieError::unreachable("an Attack's quiet MoveTo's code that only a loud one gives");

/// What a new `MoveTo` of `unit` reads as `set_state` makes it: the sprites' defaults of its
/// master (`r`), the sprites' table `graphics`, and the unit's state and top speed now.
///
/// # Errors
///
/// A unit that does not move.
pub(super) fn move_reads<'g>(
    unit: &Unit,
    r: &AttackReads,
    graphics: &'g crate::objects::Graphics,
) -> Result<MoveToReads<'g>, GenieError> {
    let _ = speed_of(unit)?;
    let sprites = SpriteReads {
        defaults: r.move_defaults,
        graphics,
    };
    Ok(MoveToReads::with_sprites(sprites, unit))
}

/// The unit's top speed (slot 0x208), which a `MoveTo`'s start reads.
///
/// # Errors
///
/// A unit that does not move.
pub(crate) fn speed_of(unit: &Unit) -> Result<Speed, GenieError> {
    Ok(unit
        .body
        .mover()
        .ok_or(crate::world::ATTACKER_UNMOVING)?
        .stats
        .speed)
}

impl Attack {
    /// The sprite the Attack gives its `MoveTo`s (`+0x68`): its constructor's argument, the
    /// master's running or walking one; none leaves the `MoveTo` constructor's default.
    const fn given_sprite(&self) -> Option<SpriteId> {
        match self.arms.move_sprite {
            Some(g) => Some(g.id),
            None => None,
        }
    }

    /// `set_state(4)`'s `MoveTo` to the target object (`FUN_140aa0df0` with the Attack's range and
    /// walking sprite, quiet through `FUN_140a7a170` with 1, its `+0x61` the Attack's), as the
    /// action list adds and starts it ([`MoveToReads::to_object`]): `at` is the target's place
    /// and `gap` the unit's from the target's footprint (`FUN_14118f680`). A unit within 0.01 of
    /// it is there already, state 1; else state 3, or 13 for a unit without speed. None for a
    /// unit whose state is 3 or more: the list deletes it. The timer is 0. True when it started
    /// arrived.
    ///
    /// # Errors
    ///
    /// An Attack without a target, and a sprite the table lacks.
    pub(super) fn walk_to_target(
        &mut self,
        at: Position,
        gap: Distance,
        reads: &MoveToReads<'_>,
    ) -> Result<bool, GenieError> {
        if self.target.is_none() {
            return Err(ATTACK_WITHOUT_TARGET.with("state", self.state));
        }
        let to = WalkTo {
            id: self.target,
            at,
            gap,
            state: None,
        };
        self.move_to =
            reads.to_object(to, self.range, self.given_sprite(), self.on_target_moved)?;
        self.timer = Seconds::ZERO;
        Ok(self.move_to.is_some_and(|m| m.arrived()))
    }

    /// The `MoveTo` to a point of states 5 and 11 (`FUN_140aa0d30` to where the target was seen,
    /// `+0xa4`..`+0xac`, with the Attack's walking sprite, quiet, its `+0x61` the Attack's), as
    /// the action list adds and starts it ([`MoveToReads::to_point`]): state 5 walks within the
    /// range, state 11 onto the point. The point's gap is far, so state 3, or 13 for a unit
    /// without speed. None for a unit whose state is 3 or more. The timer is 0.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub(super) fn walk_to_seen(&mut self, reads: &MoveToReads<'_>) -> Result<(), GenieError> {
        let range = if self.state == AttackState::GotoPoint {
            Distance::ZERO
        } else {
            self.range
        };
        let walk = reads.to_point(self.seen, range, self.given_sprite())?;
        self.move_to = walk.map(|m| MoveToAction {
            on_target_moved: self.on_target_moved,
            ..m
        });
        self.timer = Seconds::ZERO;
        Ok(())
    }

    /// The Attack's sub-action list's update (`FUN_140a992b0`: its first action's, the quiet
    /// `MoveTo`'s, [`MoveToAction::sub_update`]) and its code, which the state's case reads: 0
    /// while it walks or waits, 1 once it ended, 3 once it failed, 5 once its target was let go;
    /// 1 for an empty list. The start shows the Attack's walking sprite (the unit's standing one
    /// for none), the motion step gives up as `gives_up` says, and a start or a move posts the
    /// units in the unit's way to its unit AI at once.
    ///
    /// # Errors
    ///
    /// The walk's and the notifies'.
    pub(super) fn update_list(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        r: &AttackReads,
        gives_up: GiveUp,
    ) -> Result<UpdateCode, GenieError> {
        let state = self.move_to.as_ref().map(|m| m.state);
        let how = ListUpdate {
            gives_up,
            otherwise: r.standing,
            ..ListUpdate::default()
        };
        let code = update_sub_actions(&mut self.move_to, c, unit, me, how)?;
        if matches!(state, Some(MoveTo::Start | MoveTo::Moving)) {
            unit.post_in_the_way()?;
        }
        Ok(code)
    }

    /// The sub-action list's `MoveTo` goes: the list deletes it and its destructor
    /// (`0x140aa1f30`) stops the unit.
    ///
    /// # Errors
    ///
    /// A unit that does not move.
    pub(super) fn delete_move_to(&mut self, unit: &mut Unit) -> Result<(), GenieError> {
        if self.move_to.take().is_some() {
            unit.body
                .mover_mut()
                .ok_or(crate::world::ATTACKER_UNMOVING)?
                .stop()?;
        }
        Ok(())
    }
}
