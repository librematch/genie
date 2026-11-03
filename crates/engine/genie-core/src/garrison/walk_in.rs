//! The Enter action ([`EnterAction`], `RGE_Action_Enter`): its walk to its target, its wait
//! and its update.

use crate::GenieError;
use crate::garrison::ContainerId;
use crate::garrison::EnterState;
use crate::garrison::Walls;
use crate::garrison::meeting::Meeting;
use crate::garrison::{ENTER_NEAR, HALF_TILE};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::Me;
use crate::movement::Speed;
use crate::movement::move_to::TargetMoved;
use crate::movement::move_to::{GiveUp, MoveDefaults, MoveToReads, SpriteReads, WalkTo};
use crate::movement::move_to::{ListUpdate, update_sub_actions};
#[cfg(test)]
use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait};
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
#[cfg(test)]
use crate::objects::Voice;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::Task;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::unit_ai::UnitStance;
use crate::world::Seconds;
use crate::world::index::ObjectsRef;
use crate::world::layer::Roles;
use crate::world::others::OthersRef;
use crate::world::step::Step;
use genie_diff::Diff;

/// The Enter action's type (`RGE_Action_Enter`, vtable `0x142ed4db8`), which word 9 adds.
pub const ENTER: ActionKind = ActionKind::named(3);

/// An Enter whose target died or is not allied: the update lets it go and turns to state 11
/// (`FUN_140a87320` case 11: a quiet `MoveTo` to the meeting point), which is not modelled.
const ENTER_TARGET_DIED: GenieError =
    GenieError::unsupported("an Enter whose building died or is not allied (state 11)");
/// `set_state(2)`'s wait before the Enter walks again, in seconds (`FUN_140a87320`, the constant
/// `0x3f800000` it stores).
pub(super) const WAIT_SECONDS: Seconds = Seconds::new(1.0);

/// An Enter of a unit without movement state, which is not modelled.
pub(crate) const NO_SPEED: GenieError =
    GenieError::unsupported("an Enter's MoveTo of a unit without speed");

/// An unload of a siege building or tower (class 13: its spot search `FUN_141192d10` and the
/// unit's leave), not modelled.
pub(crate) const SIEGE_UNLOAD: GenieError =
    GenieError::unsupported("an unload of a siege building (class 13)");

/// A unit's Enter action (`RGE_Action_Enter`, type 3): it walks to a building or a transport and
/// enters it.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct EnterAction {
    /// The building or transport (`+0x20`, its id `+0x30`).
    pub target: Option<ContainerId>,
    /// The action state (`+0x18`), with its walk's `MoveTo` sub-action to the target: quiet,
    /// within [`EnterAction::walk_range`], with the sprite [`EnterAction::move_shown`] gives
    /// ([`EnterAction::walk_again`]).
    pub state: EnterState,
    /// `+0x38`..`+0x40`: where it meets the target ([`meeting`]: a building's place; for a
    /// transport where the two meet, its height the target's at the constructor), which a
    /// target that moves leaves.
    ///
    /// [`meeting`]: crate::garrison::meeting::meeting_point
    pub point: Position,
    /// The task's work range (`+0x30`): a building's `MoveTo` stops this near, and the enter
    /// needs it.
    pub range: Distance,
    /// `+0x44`: state 2's wait, in seconds.
    pub timer: Seconds,
    /// `+0x68`, 1 at first: a `MoveTo` that fails (code 3) asks once more.
    pub retry: bool,
    /// `+0x6c`: the task's class (its `+0x10`), none for any.
    pub task_class: Option<ClassId>,
    /// The task's moving sprite (`+0x40`), which its `MoveTo`s take; None for none.
    pub moving: Option<SpriteId>,
}

/// What an Enter's update reads of its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnterTarget {
    /// Its state (`+0x640`).
    pub state: ObjectState,
    /// The unit's player counts its player as allied (`FUN_141663dc0`), or it is Gaia's.
    pub allied: bool,
    /// It has room for the unit (slot 0x198).
    pub room: bool,
    /// Its place.
    pub position: Position,
    /// The unit is in reach of it ([`EnterAction::in_reach`]).
    pub in_reach: bool,
    /// It is a building (type 80); else a transport.
    pub building: bool,
}

/// What a new `MoveTo` of an Enter starts by (`0x140aa2100` with 1,
/// [`crate::movement::move_to::MoveToAction::added`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkStart {
    /// The footprints' gap of the unit and its target (`FUN_14118f680`).
    pub gap: Distance,
}

/// What an Enter's update (`FUN_140a87a80`) reads as it starts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EnterReads {
    /// Its target, of the unit's own player or an ally; None for one the unit's player does not
    /// find.
    pub(crate) target: Option<EnterTarget>,
    /// What a new `MoveTo` starts by.
    pub(crate) start: WalkStart,
    /// The `MoveTo`'s give-up waits (a human player's villager), by the unit AI's stance.
    pub(crate) waits: bool,
    /// The unit AI's stance.
    pub(crate) stance: UnitStance,
    /// The sprites its `MoveTo`s' constructors take for none (`FUN_140aa0d30`,
    /// `FUN_140aa0df0`).
    pub(crate) defaults: MoveDefaults,
}

/// What [`EnterAction::walk_again`] (`set_state(4)`) leaves to its caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Walking {
    /// The unit is in reach: the caller enters (`FUN_140a867b0`), then `set_state(1)`.
    Enter,
    /// A `MoveTo` to the target started; with a point, the transport is to move there first
    /// (its slots 0x3d8 and 0x340, [`crate::world::World::move_transport`]).
    Walk(Option<Position>),
}

impl EnterAction {
    /// The factory's constructor (`FUN_140a855d0`): the target (an annex's main building, the
    /// caller's, as its add, slot 0x178), the point [`meeting`] gives at the target's height,
    /// the task's (`task`) range, class and moving sprite, `+0x68` 1; the start (slot 5,
    /// `FUN_140a86f20`) and its `set_state(4)` are [`EnterAction::walk_again`].
    ///
    /// [`meeting`]: crate::garrison::meeting::meeting_point
    #[must_use]
    pub const fn new(target: ContainerId, point: Position, task: &Task) -> Self {
        Self {
            target: Some(target),
            state: EnterState::Walk { move_to: None },
            point,
            range: task.work_range,
            timer: Seconds::ZERO,
            retry: true,
            task_class: task.object_group,
            moving: task.sprites.moving,
        }
    }

    /// `set_state(4)` (`FUN_140a87320` case 4) after the start's or the update's tests: its
    /// sub-actions go (`FUN_140a98620`; a `MoveTo`'s teardown stops the unit, the caller's), and
    /// its room test repeats theirs, which nothing between changes. A unit in reach of the
    /// target (`FUN_140a85c30`) enters it ([`Walking::Enter`]). Else a building's `MoveTo` walks
    /// to it within the task's range (`FUN_140aa0df0`); a transport's takes the point and code
    /// of [`meeting`] (`FUN_140a85f30`, [`Meeting::point`]: the action's `+0x38` and `+0x3c`)
    /// and walks to the transport within 0 (`FUN_140aa0d30` with range 0, then slot 0x68 with
    /// the transport: its state 3 goes to the transport's place; a point below 0 would make
    /// `FUN_140aa0df0` with the unit's slot 0x268, 0, alike), and code 1 asks the transport to
    /// move to the point. Both `MoveTo`s are quiet (`FUN_140a7a170` with 1) and leave a target
    /// that moves (`+0x61` 2); each starts with a new wait, takes the sprite
    /// [`EnterAction::move_shown`] gives from `sprites`, and starts as
    /// [`EnterAction::walk_start`] says.
    ///
    /// # Errors
    ///
    /// The meeting's, the sprite's and the start's.
    ///
    /// [`meeting`]: crate::garrison::meeting::meeting_point
    pub(crate) fn walk_again(
        &mut self,
        t: &EnterTarget,
        start: WalkStart,
        meeting: Meeting<'_>,
        reads: &MoveToReads<'_>,
        by_id: ObjectsRef<'_>,
    ) -> Result<Walking, GenieError> {
        self.state = EnterState::Walk { move_to: None };
        if t.in_reach {
            return Ok(Walking::Enter);
        }
        let code = if t.building {
            0
        } else {
            let (meet_at, code) = meeting.point(by_id)?;
            self.point = self.point.with_x(meet_at.x()).with_y(meet_at.y());
            code
        };
        // `FUN_140aa0df0`'s: the target's place; `FUN_140aa0d30`'s: the meeting point toward a
        // transport (a point below 0 would make the first)
        let to = WalkTo {
            id: self.target.map(ContainerId::id),
            at: t.position,
            gap: start.gap,
            state: Some(t.state),
        };
        let move_to = if t.building || self.point.x() < 0.0 || self.point.y() < 0.0 {
            reads.to_object(
                to,
                self.walk_range(t.building),
                self.moving,
                TargetMoved::Ignores,
            )?
        } else {
            reads.to_point_of(
                to,
                (self.point, self.walk_range(false).get()),
                self.moving,
                TargetMoved::Ignores,
            )?
        };
        self.state = EnterState::Walk { move_to };
        Ok(Walking::Walk((code == 1).then_some(Position::new(
            self.point.x(),
            self.point.y(),
            t.position.z(),
        ))))
    }

    /// The Enter with a `MoveTo` in `state` to its target within the task's range, as
    /// [`EnterAction::walk_again`] makes it toward a building (without a sprite), for a test.
    #[cfg(test)]
    pub(crate) const fn walking(mut self, state: MoveTo) -> Self {
        let move_to = Some(MoveToAction {
            state,
            point: Position::ORIGIN,
            target: match self.target {
                Some(t) => Some(t.id()),
                None => None,
            },
            range: self.range,
            sprite: None,
            voice: Voice::Quiet,
            on_target_moved: TargetMoved::Ignores,
            wait: MoveWait::NEW,
        });
        self.state = EnterState::Walk { move_to };
        self
    }

    /// The range its `MoveTo` stops within: the task's for a building, 0 for a transport.
    pub(super) const fn walk_range(&self, building: bool) -> Distance {
        if building { self.range } else { Distance::ZERO }
    }

    /// `set_state(2)` (`FUN_140a87320` case 2): the wait of [`WAIT_SECONDS`], and the unit shows
    /// its master's standing sprite at once (slot 0x68 with `+0x470`,
    /// [`Unit::shows_standing_sprite`]).
    ///
    /// # Errors
    ///
    /// The sprite's.
    pub(super) fn wait(&mut self, c: &mut Step<'_>, unit: &mut Unit) -> Result<(), GenieError> {
        self.state = EnterState::Wait;
        self.timer = WAIT_SECONDS;
        unit.shows_standing_sprite(
            c.w.parts.motion.layer_mut(unit.id),
            OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
            &c.w.model.graphics,
            c.w.streams.get(Stream::Sprite),
        )
    }

    /// `FUN_140a85c30`: the unit is in reach of the target: their footprints' squared gap
    /// (`FUN_14118f8c0`; `COMISS`/`JBE`: a NaN gap is not out) at most the range squared; for a
    /// range of at most 1 (`COMISS`/`JC`: a NaN range is in reach) to a target with speed (slot
    /// 0x208, above 0) the tiles about the two ([`Walls::walled_off`]) must not hold exactly
    /// two buildings.
    ///
    /// # Errors
    ///
    /// The tile test's.
    ///
    /// [`Walls::walled_off`]: crate::garrison::Walls::walled_off
    pub fn in_reach(
        range: Distance,
        gap_squared: DistanceSquared,
        target_speed: Speed,
        walls: &mut Walls<'_>,
        by_id: ObjectsRef<'_>,
    ) -> Result<bool, GenieError> {
        if range.squared() < gap_squared {
            return Ok(false);
        }
        if range <= ENTER_NEAR && Speed::ZERO < target_speed {
            return Ok(!walls.walled_off(by_id)?);
        }
        Ok(true)
    }

    /// One update (slot 6, `FUN_140a87a80`) of `unit`'s Enter, a copy its caller writes back
    /// ([`Unit::enter_part`]). It first checks its target ([`EnterAction::held_target`]). State
    /// 1 is done; state 2 waits its timer (by the world's `dt`), then walks again
    /// ([`EnterAction::again`]); state 4 walks ([`EnterAction::walk_on`]). State 13 returns 3,
    /// which deletes it as 1 does. Its code: 1 or 3 delete it.
    ///
    /// # Errors
    ///
    /// A target the update does not hold (state 11), a state not modelled, and what the walk,
    /// the enter and the transport's move do not model.
    pub(crate) fn update(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        me: Me,
        reads: &EnterReads,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        let t = Self::held_target(reads.target)?;
        match self.state {
            // state 1 returns 1, state 13 returns 3: either way the list removes the action
            // The Enter action's states the slice models: 1 done (inside), 2 waiting to try again, 4
            // walking to the target.
            // The failed Enter (`set_state(13)`), whose update returns 3.
            EnterState::Done | EnterState::Failed => return Ok(HeadCode::Deletes),
            EnterState::Wait => {
                self.timer -= c.dt.seconds();
                if self.timer <= Seconds::ZERO {
                    let _ = self.again(c, unit, actions, &t, reads, roles)?;
                }
            }
            EnterState::Walk { .. } => self.walk_on(c, unit, actions, me, &t, reads)?,
            EnterState::WalkPoint { .. } => {
                return Err(GenieError::unsupported("an Enter state not modelled"));
            }
        }
        Ok(HeadCode::Keeps)
    }

    /// State 4: a target off the action's point ([`EnterAction::target_moved`]) walks it again;
    /// then the `MoveTo` updates ([`MoveToAction::sub_update`] with the reads' give-up): codes 1
    /// and 2 enter in reach, a unit that entered in the walk again enters again
    /// ([`Unit::entered_again`]), else
    /// they wait (`set_state(2)`); code 3 (a `MoveTo` that failed) walks again once (`+0x68`),
    /// then waits; codes 4 and 5 (its target in a state above 6 let go) let the Enter's target go
    /// and turn to state 11, which is not modelled ([`ENTER_TARGET_DIED`]: the update's own test
    /// of its target, [`EnterAction::held_target`], stops on a target in state 3 first).
    ///
    /// [`MoveToAction::sub_update`]: crate::movement::move_to::MoveToAction::sub_update
    ///
    /// # Errors
    ///
    /// The walk's, the walk again's, the enter's and the sprite's.
    fn walk_on(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        me: Me,
        t: &EnterTarget,
        reads: &EnterReads,
    ) -> Result<(), GenieError> {
        let roles = &c.w.model.roles.clone();
        let walked_again = self.target_moved(t);
        let entered = walked_again && self.again(c, unit, actions, t, reads, roles)?;
        let code = self.move_to_update(c, unit, me, reads)?;
        match code.get() {
            // the walk again's enter put the unit inside, at the target's place: still in reach,
            // it enters again
            1 | 2 if entered => self.on_unit(unit, actions, |unit, actions| {
                unit.entered_again(actions, c)
            }),
            1 | 2 if t.in_reach => self.enter_at_once(actions, c, unit, roles),
            // a failed MoveTo is none a walk again made in this update
            3 if self.retry => {
                self.retry = false;
                self.again(c, unit, actions, t, reads, roles).map(|_| ())
            }
            1..=3 => self.wait(c, unit),
            4 | 5 => Err(ENTER_TARGET_DIED),
            _ => Ok(()),
        }
    }

    /// The `MoveTo`'s update ([`MoveToAction::sub_update`]) with the reads' give-up, its start
    /// showing its sprite; its code, 1 without one.
    ///
    /// [`MoveToAction::sub_update`]: crate::movement::move_to::MoveToAction::sub_update
    ///
    /// # Errors
    ///
    /// The walk's.
    fn move_to_update(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        reads: &EnterReads,
    ) -> Result<UpdateCode, GenieError> {
        let how = ListUpdate {
            gives_up: GiveUp {
                waits: reads.waits,
                stance: reads.stance,
            },
            ..ListUpdate::default()
        };
        match self.state.walk_mut() {
            Some(move_to) => update_sub_actions(move_to, c, unit, me, how),
            // a state that does not walk has no sub-action: its list's update is the empty one's
            None => update_sub_actions(&mut None, c, unit, me, how),
        }
    }

    /// The update's test of its target first (`FUN_140a87a80`): one gone, dead (state 3 or
    /// more) or of an unallied player, or without room for the unit, makes the action let it go
    /// and turn to state 11, which is not modelled.
    pub(super) fn held_target(target: Option<EnterTarget>) -> Result<EnterTarget, GenieError> {
        let Some(t) = target else {
            return Err(GenieError::unsupported(
                "an Enter whose building is gone (state 11)",
            ));
        };
        if ObjectState::Dying <= t.state || !t.allied {
            return Err(ENTER_TARGET_DIED);
        }
        if !t.room {
            return Err(GenieError::unsupported(
                "an Enter whose building has no room (state 11)",
            ));
        }
        Ok(t)
    }

    /// State 4's test of its target: off the action's point (`FUN_140a7bbf0`: any of x, y and
    /// height not equal) by more than [`HALF_TILE`] on an axis (`FUN_140a7bd10`: `COMISS`/`JA`,
    /// a NaN is not).
    pub(super) fn target_moved(&self, t: &EnterTarget) -> bool {
        let (x, y, z) = (t.position.x(), t.position.y(), t.position.z());
        let moved = |d: f32| HALF_TILE < d.abs();
        t.position != self.point
            && (moved(x - self.point.x()) || moved(y - self.point.y()) || moved(z - self.point.z()))
    }

    /// The update's `set_state(4)` ([`EnterAction::walk_again`]): its `MoveTo`'s teardown stops
    /// the unit (slot 0x5d0), then a unit in reach enters at once (`FUN_140a867b0`, then
    /// `set_state(1)`: [`EnterAction::enter_at_once`]). A transport's code 1 sends it to the
    /// point at once: it tests that it reaches within 1 of the point's tile (slot 0x3d8,
    /// `FUN_14114ffc0` with the point truncated), then moves there (slot 0x340,
    /// [`crate::world::World::move_transport`], the unit lent to the world). A transport that
    /// cannot reach it makes `set_state(13)`, which is not modelled ([`UNREACHED`]). True when
    /// the unit entered.
    ///
    /// # Errors
    ///
    /// The stop's, the walk again's, the enter's and the transport's.
    pub(super) fn again(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        t: &EnterTarget,
        reads: &EnterReads,
        roles: &Roles,
    ) -> Result<bool, GenieError> {
        if self.state.move_to().is_some()
            && let Some(m) = c.w.parts.mover_mut(unit.id)
        {
            m.stop()?;
        }
        let walking = {
            let w = &mut *c.w;
            let (tp, target) = OthersRef::of(&w.players, &w.index, c.player)?
                .find(self.target)
                .ok_or(GenieError::unreachable(
                    "an Enter's held target gone in its update",
                ))?;
            let master =
                target.master(MasterTables::new(&w.model.masters, &w.parts.own_master), tp)?;
            let meeting = Meeting {
                spatial: &mut w.spatial,
                zones: &mut w.zones,
                target,
                master,
                room: t.room,
                unit,
                mover: w.parts.mover_of(unit.id),
                target_actions: w.parts.actions_of(target.id),
                target_unread: w.parts.unread.get(target.id),
                target_mover: w.parts.mover_of(target.id),
            };
            let sprites = SpriteReads {
                defaults: reads.defaults,
                graphics: &w.model.graphics,
            };
            let moves = MoveToReads::with_sprites(sprites, (meeting.unit, meeting.mover));
            self.walk_again(
                t,
                reads.start,
                meeting,
                &moves,
                ObjectsRef::new(
                    &w.players,
                    &w.index,
                    &w.model.masters,
                    &w.parts,
                    &w.model.roles,
                    &w.model.tuning,
                ),
            )?
        };
        match walking {
            Walking::Enter => {
                self.enter_at_once(actions, c, unit, roles)?;
                return Ok(true);
            }
            Walking::Walk(Some(at)) => {
                let target = self.target;
                self.on_unit(unit, actions, |unit, actions| {
                    c.lend((&mut *unit, &mut *actions), |w| {
                        w.move_transport(target, at)
                    })
                })?;
            }
            Walking::Walk(None) => {}
        }
        Ok(false)
    }

    /// The enter at once (`FUN_140a867b0` into the target, then `set_state(1)`:
    /// [`Unit::enter_in_update`]) on the unit's Enter: this copy written back first and read back
    /// after ([`EnterAction::on_unit`]).
    ///
    /// # Errors
    ///
    /// The enter's.
    fn enter_at_once(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        unit: &mut Unit,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let target = self.target;
        self.on_unit(unit, actions, |unit, actions| {
            unit.enter_in_update(actions, c, target, roles)
        })
    }

    /// Runs `f` on `unit` and its action layer `actions` with this copy of its Enter written back
    /// to the layer (a routine that reads or changes the unit's Enter, or lends the unit to the
    /// world), then takes the Enter back as `f` left it.
    ///
    /// # Errors
    ///
    /// `f`'s, or a unit whose Enter `f` took.
    fn on_unit<R>(
        &mut self,
        unit: &mut Unit,
        actions: &mut Action,
        f: impl FnOnce(&mut Unit, &mut Action) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        actions.list.set_enter(Some(*self));
        let answer = f(unit, actions);
        *self = *actions
            .list
            .enter()
            .ok_or(GenieError::unreachable("an Enter gone in its own update"))?;
        answer
    }
}
