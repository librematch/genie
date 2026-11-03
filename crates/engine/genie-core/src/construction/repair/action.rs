//! The Repair action's start, update and states ([`RepairAction`]): the walk, the face, the
//! wait, the work, a new target, a move and the halt.

use crate::GenieError;
use crate::combat::attack::face_target;
use crate::combat::damage::ArmorPair;
use crate::construction::repair::RepairOutcome;
use crate::construction::repair::RepairState;
use crate::construction::repair::repair_target;
use crate::construction::repair::{ONE, RepairAction, RepairReads, RepairTask, Repairee};
use crate::construction::{BuilderCount, RepairTargetId};
use crate::map::Distance;
use crate::map::Position;
use crate::math::fmath::below_or_unordered;
use crate::math::rng::Stream;
use crate::movement::Mover;
use crate::movement::move_to::{GiveUp, MoveToAction, MoveToReads, Sprites, TargetMoved, WalkTo};
use crate::movement::move_to::{ListUpdate, update_sub_actions};
use crate::movement::{Me, footprint_gap};
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
use crate::objects::Voice;
use crate::objects::WorkRate;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::{OwnerMasters, WorkValue};
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::pathing::hull_path::{NO_RESTRICTION, NO_TERRAIN, quick_reach};
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::NotifyKind;
use crate::world::Seconds;
use crate::world::index::ObjectsRef;
use crate::world::layer::Roles;
use crate::world::layer::generated::MoveToTuning;
use crate::world::layer::generated::RepairTuning;
use crate::world::lets::release;
use crate::world::step::Step;
use std::mem::replace;

mod enter;

/// The work of one update (`0x140adaa20` case 6): the work `rate` times the task's
/// `work_value` times the world's `dt`; with more than one repairer (`repairers`, `COMISS` with
/// `JBE`) it is scaled by `((n − 1) × 0.5 + 1) / n`.
#[must_use]
pub fn work_amount(
    rate: WorkRate,
    work_value: WorkValue,
    dt: Seconds,
    repairers: Option<BuilderCount>,
    repair_tuning: &RepairTuning,
) -> HitPoints {
    let mut amount = rate * work_value * dt;
    if let Some(n) = repairers {
        let n = n.as_f32();
        if ONE < n {
            amount *= ((n - ONE) * repair_tuning.extra_repairer + ONE) / n;
        }
    }
    HitPoints::new(amount)
}

/// The repairer's slot 0x328: notify `kind` with the repair's action value to its unit AI
/// (the world's).
fn notify(c: &mut Step<'_>, repairer: &Unit, kind: NotifyKind) -> Result<(), GenieError> {
    repairer.post_own(
        &mut c.w.parts.unit_ai,
        kind,
        [AiAction::Repair.value(), 0, 0],
    )
}

/// The repairer shows its master's standing sprite ([`Unit::show_standing`]).
fn standing(c: &mut Step<'_>, repairer: &mut Unit) -> Result<(), GenieError> {
    repairer.show_standing(
        c.w.parts.motion.layer_mut(repairer.id),
        OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
        &c.w.model.graphics,
        c.w.streams.get(Stream::Sprite),
        c.w.model.tuning.sprite,
    )
}

/// Slot 0x68 of the repairer: sprite `id` shows (the master's standing one for none, `standing`),
/// its node in the unit's movement state.
fn show(
    c: &mut Step<'_>,
    repairer: &mut Unit,
    id: Option<SpriteId>,
    standing: Option<SpriteId>,
) -> Result<(), GenieError> {
    let mut sprites = Sprites {
        sprite: &mut repairer.sprite,
        rng: c.w.streams.get(Stream::Sprite),
        graphics: &c.w.model.graphics,
    };
    sprites.shown(id.or(standing))?.show(
        c.w.parts.move_stats_mut(repairer.id),
        c.w.model.tuning.sprite,
    );
    Ok(())
}

impl RepairAction {
    /// A Repair from the work command (the factory `FUN_140aca4b0` case 0x6a, `FUN_140ad7fa0`):
    /// its target `to`, whose place is its point, the task, not counted, not quiet; the start
    /// (slot 5, `FUN_140ad9030` with its force flag) then turns to state 4, whose `set_state`
    /// makes the `MoveTo` to the target within `range` ([`RepairAction::walk`] with what it
    /// reads of the repairer, `move_reads`).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn start(
        to: WalkTo,
        task: RepairTask,
        range: Distance,
        move_reads: &MoveToReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<Self, GenieError> {
        let mut repair = Self {
            target: to.id.map(RepairTargetId::new),
            state: RepairState::Move { move_to: None },
            point: to.at,
            timer: Seconds::ZERO,
            task,
            range,
            counted: false,
            voice: Voice::Loud,
        };
        repair.state = RepairState::Move {
            move_to: repair.walk(move_reads, Some(to), move_to_tuning)?,
        };
        Ok(repair)
    }

    /// The `MoveTo` of state 4 (`0x140ad9c60` case 4: `FUN_140aa0df0` with the action's range and
    /// the task's moving sprite `+0x40`, then quiet) to the target `to`, or without one
    /// ([`MoveToReads::to_nothing`]), as the sub-action list adds it.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn walk(
        &self,
        move_reads: &MoveToReads<'_>,
        to: Option<WalkTo>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<Option<MoveToAction>, GenieError> {
        to.map_or_else(
            || move_reads.to_nothing(self.range, self.task.moving, move_to_tuning),
            |to| {
                move_reads.to_object(
                    to,
                    self.range,
                    self.task.moving,
                    TargetMoved::Follows,
                    move_to_tuning,
                )
            },
        )
    }

    /// The quiet `MoveTo` of state 11 (`0x140ad9c60` case 0xb: `FUN_140aa0d30`, quiet) to
    /// `point` within 0 with the task's moving sprite, as the sub-action list adds it.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn to_point(
        &self,
        point: Position,
        move_reads: &MoveToReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<Option<MoveToAction>, GenieError> {
        move_reads.to_point(point, Distance::ZERO, self.task.moving, move_to_tuning)
    }

    /// One update (slot 6, `0x140adaa20`) of `repairer` (`me`) through its step, with what it
    /// reads as it starts (`reads`). A target the world no longer holds ends it (slot 0x68 with
    /// none, `set_state(1)`); so does one no longer in state 2, one below `RepairTuning::least_hp` hit
    /// points (`COMISS` with `JA`: a NaN is repaired), or one neither its player's own, an
    /// ally's nor Gaia's. Then by its state: 1 is done (its 1: the action list deletes it); 4
    /// walks ([`RepairAction::walking`]); 6 turns the unit toward the target, then works (7); 7
    /// repairs ([`RepairAction::work`]); 10 waits a second, then works or waits again; 11 walks
    /// to its point, then ends; 13 posts notify 0x1f9, halts and returns 3 (deleted too); any
    /// other does nothing.
    ///
    /// # Errors
    ///
    /// A ship's meeting point, and what the walk and the repair do not model.
    pub(crate) fn update(
        &mut self,
        c: &mut Step<'_>,
        repairer: &mut Unit,
        me: Me,
        reads: &RepairReads,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        if self.target.is_some() && reads.target.is_none() {
            // the target is gone: FUN_140ad9050 finds no object to leave the repairers of
            self.counted = false;
            self.target = None;
            self.set_state(c, repairer, RepairState::Done, reads, roles)?;
            return Ok(HeadCode::Keeps);
        }
        if let Some(t) = reads.target
            && (t.state != ObjectState::Alive
                || c.w.model.tuning.repair.least_hp > t.hp.get()
                || (!t.allied && t.owner != PlayerId::GAIA))
        {
            self.drop_target(c, repairer, reads)?;
            self.set_state(c, repairer, RepairState::Done, reads, roles)?;
            return Ok(HeadCode::Keeps);
        }
        match self.state {
            RepairState::Done => return Ok(HeadCode::Deletes),
            RepairState::Move { .. } => self.walking(c, repairer, me, reads, roles)?,
            RepairState::Face => self.facing(c, repairer, reads, roles)?,
            RepairState::Work => self.work(c, repairer, reads, roles)?,
            RepairState::Wait => self.waiting(c, repairer, reads, roles)?,
            RepairState::WalkPoint { .. } => {
                self.walking_to_point(c, repairer, me, reads, roles)?;
            }
            RepairState::Failed => {
                notify(c, repairer, NotifyKind::ACTION_FAILED)?;
                self.set_state(c, repairer, RepairState::Halted, reads, roles)?;
                return Ok(HeadCode::Deletes);
            }
            // states 2 and 3 (and the bytes without a state, which no step sets): the update
            // returns 0
            RepairState::Halted | RepairState::Idle => {}
        }
        Ok(HeadCode::Keeps)
    }

    /// State 6 (`0x140adaa20` case 5): the unit turns toward the target ([`face_target`] with its
    /// sprites' facets), then works (7).
    ///
    /// [`face_target`]: crate::combat::attack::face_target
    pub(super) fn facing(
        &mut self,
        c: &mut Step<'_>,
        repairer: &mut Unit,
        reads: &RepairReads,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let t = reads.target.ok_or(GenieError::unreachable(
            "a Repair facing its target without one",
        ))?;
        let at = repairer.place.position();
        let mover =
            c.w.parts
                .mover_mut(repairer.id)
                .ok_or(GenieError::unreachable(
                    "a repairer without movement state (an action object always has one)",
                ))?;
        if face_target(mover, at, t.position, c.w.world_time)? {
            self.set_state(c, repairer, RepairState::Work, reads, roles)?;
        }
        Ok(())
    }

    /// State 10 (`0x140adaa20` case 9): the timer counts down by the update's seconds; once it
    /// is out (`COMISS` 0 with `JC`: a NaN is out) a unit within reach faces the target (6), any
    /// other waits a second more.
    pub(super) fn waiting(
        &mut self,
        c: &mut Step<'_>,
        repairer: &mut Unit,
        reads: &RepairReads,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        self.timer -= c.dt.seconds();
        if below_or_unordered(0.0, self.timer.get()) {
            return Ok(());
        }
        if Self::reaches(repairer, reads, &c.w.model.tuning.repair) {
            self.set_state(c, repairer, RepairState::Face, reads, roles)?;
        } else {
            self.timer = Seconds::new(ONE);
        }
        Ok(())
    }

    /// State 11 (`0x140adaa20` case 10): the walk to the Repair's point; at it (1, 2), failed
    /// (3), gone (the list empty, 1) or its target let go (5) the Repair ends; a target let go
    /// with 4 lets the Repair's go too (slot 0x68 with none, [`RepairAction::drop_target`]) and
    /// walks to the point again (`set_state(11)`).
    pub(super) fn walking_to_point(
        &mut self,
        c: &mut Step<'_>,
        repairer: &mut Unit,
        me: Me,
        reads: &RepairReads,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        match self.step_walk(c, repairer, me, reads)?.get() {
            0 => Ok(()),
            4 => {
                self.drop_target(c, repairer, reads)?;
                self.set_state(
                    c,
                    repairer,
                    RepairState::WalkPoint { move_to: None },
                    reads,
                    roles,
                )
            }
            _ => self.set_state(c, repairer, RepairState::Done, reads, roles),
        }
    }

    /// State 4 (`0x140adaa20` case 3): the quiet `MoveTo` toward the target walks
    /// ([`RepairAction::step_walk`]); once it is at it, waits or failed (or gone: 1, 2, 3), the
    /// unit within reach of the target ([`RepairAction::reaches`]) faces it (6), one with a
    /// quick path to it within `RepairTuning::reach` (slot 0x3e0, `canQuickPath`) waits (10), any other
    /// fails (13). Its target let go (4, 5) lets the Repair's go (slot 0x68 with none,
    /// [`RepairAction::drop_target`]) and ends the Repair (`set_state(1)`).
    pub(super) fn walking(
        &mut self,
        c: &mut Step<'_>,
        repairer: &mut Unit,
        me: Me,
        reads: &RepairReads,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        match self.step_walk(c, repairer, me, reads)?.get() {
            0 => return Ok(()),
            4 | 5 => {
                self.drop_target(c, repairer, reads)?;
                return self.set_state(c, repairer, RepairState::Done, reads, roles);
            }
            _ => {}
        }
        if Self::reaches(repairer, reads, &c.w.model.tuning.repair) {
            return self.set_state(c, repairer, RepairState::Face, reads, roles);
        }
        let next = if self.can_path(c, repairer, reads)? {
            RepairState::Wait
        } else {
            RepairState::Failed
        };
        self.set_state(c, repairer, next, reads, roles)
    }

    /// Slot 0x3e0 (`canQuickPath`) of the repairer to its target within `RepairTuning::reach`: the old
    /// short pather's quick path ([`crate::pathing::hull_path::quick_reach`]) with its master's
    /// x radius and its terrain restriction's mask.
    fn can_path(
        &self,
        c: &mut Step<'_>,
        repairer: &Unit,
        reads: &RepairReads,
    ) -> Result<bool, GenieError> {
        let by_id = ObjectsRef::new(
            &c.w.players,
            &c.w.index,
            &c.w.model.masters,
            &c.w.parts,
            &c.w.model.roles,
            &c.w.model.tuning,
        );
        let spatial = &mut c.w.spatial;
        let terrain = spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
        let r = reads.restriction.ok_or(NO_RESTRICTION)?;
        let mask = terrain.restriction_mask(r);
        quick_reach(
            (spatial, by_id),
            repairer.id,
            self.target.map(RepairTargetId::id),
            mask,
            Distance::new(reads.radius.x()),
            Distance::new(c.w.model.tuning.repair.reach),
        )
    }

    /// The quiet `MoveTo`'s update and its code (`FUN_140a992b0`, [`MoveToAction::sub_update`]),
    /// the villager test and the stance deciding a give-up: 0 while it walks or waits, 1 once it
    /// is at its goal (or gone), 3 once it failed, 5 once its target was let go (a target in a
    /// state above 6, [`MoveToAction::check_held`]). Its start shows its sprite.
    pub(super) fn step_walk(
        &mut self,
        c: &mut Step<'_>,
        repairer: &mut Unit,
        me: Me,
        reads: &RepairReads,
    ) -> Result<UpdateCode, GenieError> {
        let how = ListUpdate {
            gives_up: GiveUp {
                waits: reads.waits,
                stance: reads.stance,
            },
            ..ListUpdate::default()
        };
        match self.state.walk_mut() {
            Some(move_to) => update_sub_actions(move_to, c, repairer, me, how),
            // a state that does not walk has no sub-action: its list's update is the empty one's
            None => update_sub_actions(&mut None, c, repairer, me, how),
        }
    }

    /// The reach test of states 4 and 10: the footprints' gap (`FUN_14118f680`) less
    /// `RepairTuning::moving_reach` for a moving target or a scenario's game mode at most `RepairTuning::reach`
    /// (`COMISS REACH, gap` with `JC`/`JNC`: a NaN gap is out of reach).
    pub(super) fn reaches(
        repairer: &Unit,
        reads: &RepairReads,
        repair_tuning: &RepairTuning,
    ) -> bool {
        let Some(t) = reads.target else {
            return false;
        };
        let gap = Self::gap(repairer, reads, t, repair_tuning);
        !below_or_unordered(repair_tuning.reach, gap.get())
    }

    /// The footprints' gap less the moving target's or the scenario's margin.
    pub(super) fn gap(
        repairer: &Unit,
        reads: &RepairReads,
        t: Repairee,
        repair_tuning: &RepairTuning,
    ) -> Distance {
        let margin = if reads.scenario || t.moves {
            repair_tuning.moving_reach
        } else {
            0.0
        };
        let at = repairer.place.position();
        footprint_gap(at, reads.radius, t.position, t.radius) - Distance::new(margin)
    }

    /// State 7 (`0x140adaa20` case 6): a target that left the action's point (`UCOMISS` on x
    /// and y: a NaN has) within reach (`COMISS gap, REACH` with `JBE`: a NaN is) gives the point
    /// its place; out of reach the unit walks again (4). Then the work of the update
    /// ([`work_amount`]) goes to the target's repair ([`crate::construction::repair::repair_target`]):
    /// done (1) a target of a type above 69 has the armor its charge's hits took back
    /// (`FUN_1412066a0`: `+0x9e4` zeroed) and the action ends (`set_state(1)`); the player short
    /// of resources (2) posts notify 0x1fb (the game's event 0x6b to its interface,
    /// `FUN_140c898a0`, is not held) and ends it too.
    pub(super) fn work(
        &mut self,
        c: &mut Step<'_>,
        repairer: &mut Unit,
        reads: &RepairReads,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let t = reads.target.ok_or(GenieError::unreachable(
            "a Repair working without its target",
        ))?;
        let moved = t.position.x() != self.point.x() || t.position.y() != self.point.y();
        if moved {
            let gap = Self::gap(repairer, reads, t, &c.w.model.tuning.repair);
            if c.w.model.tuning.repair.reach < gap.get() {
                return self.set_state(
                    c,
                    repairer,
                    RepairState::Move { move_to: None },
                    reads,
                    roles,
                );
            }
            self.point = t.position;
        }
        let repairers = reads.counted_in.map(|(_, n)| n);
        let amount = work_amount(
            reads.rate,
            self.task.work_value,
            c.dt.seconds(),
            repairers,
            &c.w.model.tuning.repair,
        );
        let (answer, kind) = repair_target(c, self.target, amount, roles)?;
        match answer {
            RepairOutcome::Full => {
                if MasterKind::COMBAT <= kind
                    && let Some(id) = c.w.object(self.target).map(|(_, u)| u.id)
                {
                    c.w.parts.combat_mut(id)?.armor_removed = ArmorPair::ZERO;
                }
                self.set_state(c, repairer, RepairState::Done, reads, roles)
            }
            RepairOutcome::Unpaid => {
                notify(c, repairer, NotifyKind::ACTION_INVALIDATED)?;
                self.set_state(c, repairer, RepairState::Done, reads, roles)
            }
            RepairOutcome::Healed => Ok(()),
        }
    }

    /// The repairers of the object `FUN_140ad9050` counts them in (`+0x9e8`) change by `by` at
    /// once; nothing without one, or once the world no longer holds it.
    ///
    /// # Errors
    ///
    /// None (an unreachable error where a table lost the part it took).
    fn count(c: &mut Step<'_>, reads: &RepairReads, by: i8) -> Result<(), GenieError> {
        if let Some((id, _)) = reads.counted_in
            && c.w.object(id).is_some()
        {
            let n = c.w.parts.repairers_mut(id)?;
            *n = n.changed_by(by);
        }
        Ok(())
    }

    /// Slot 0x68 with none (`FUN_140ada470`): a counted action leaves its target's repairers
    /// (`FUN_140ad9050`'s object), then the base lets the target go (`FUN_140a7a230`: the
    /// target's slot 0x180, [`crate::world::lets::release`]).
    pub(super) fn drop_target(
        &mut self,
        c: &mut Step<'_>,
        repairer: &mut Unit,
        reads: &RepairReads,
    ) -> Result<(), GenieError> {
        if self.counted {
            Self::count(c, reads, -1)?;
            self.counted = false;
        }
        let target = self.target.take();
        if target.is_some() {
            let by = repairer.releaser(c)?;
            release(c, repairer, target, by)?;
        }
        Ok(())
    }

    /// `set_state` (`0x140ad9c60`): a counted action leaves its target's repairers, the
    /// sub-actions go (a walking `MoveTo`'s teardown stops the unit), then by the new state: 1
    /// lets the target go (slot 0x68 with none) and posts notify 0x1fa unless quiet; 1, 2 and 3
    /// (with notify 0x202) give the unit its standing sprite; 4 takes the target's place and a
    /// `MoveTo` to the target; 6 takes its place and shows the task's moving sprite; 7 shows its
    /// proceeding sprite (its sound, slot 0x4c0, is not held) and counts the action among the
    /// target's repairers; slot 0x68 shows the master's standing sprite for none. 10 waits a
    /// second with the standing sprite (a ship target's move to the unit, [`SHIP_REPAIR`], is
    /// not modelled); 11 walks to the point (a quiet `MoveTo`).
    pub(super) fn set_state(
        &mut self,
        c: &mut Step<'_>,
        repairer: &mut Unit,
        state: RepairState,
        reads: &RepairReads,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        if self.counted {
            Self::count(c, reads, -1)?;
            self.counted = false;
        }
        if self.state.take_move_to().is_some()
            && let Some(m) = c.w.parts.mover_mut(repairer.id)
        {
            m.stop()?;
        }
        self.state = state.bare();
        self.enter(c, repairer, reads, roles)
    }

    /// `set_state(4)` from the work command's new target `to` (slot 0x48, the caller's
    /// prelude): its place is the point, the `MoveTo` goes (a walking one stops the unit) and a
    /// new one to the target starts ([`RepairAction::walk`] with `move_reads`).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks, and the unit's stop that the motion step does not model.
    pub fn walk_again(
        &mut self,
        to: WalkTo,
        mover: Option<&mut Mover>,
        move_reads: &MoveToReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<(), GenieError> {
        let walk = self.walk(move_reads, Some(to), move_to_tuning)?;
        if self.state.take_move_to().is_some()
            && let Some(m) = mover
        {
            m.stop()?;
        }
        self.point = to.at;
        self.state = RepairState::Move { move_to: walk };
        Ok(())
    }

    /// Slot 8 (`FUN_140ad9700`), a move to `point` (its target argument unread): the point is
    /// the action's, slot 0x68 with none (a counted repairer leaves the repairers, the target
    /// goes), then `set_state(11)` (the `MoveTo` goes, a walking one stopping the unit; a quiet
    /// one to the point starts, [`RepairAction::to_point`] with `move_reads`). Returns the target
    /// let go and whether it counted.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks, and the unit's stop that the motion step does not model.
    pub fn take_point(
        &mut self,
        point: Position,
        mover: Option<&mut Mover>,
        move_reads: &MoveToReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<(Option<ObjectId>, bool), GenieError> {
        let walk = self.to_point(point, move_reads, move_to_tuning)?;
        self.point = point;
        let counted = replace(&mut self.counted, false);
        let old = self.target.take();
        if self.state.take_move_to().is_some()
            && let Some(m) = mover
        {
            m.stop()?;
        }
        self.state = RepairState::WalkPoint { move_to: walk };
        Ok((old.map(RepairTargetId::id), counted))
    }

    /// Slot 7 (`FUN_140adaa00`) as the unit's stop calls it: `set_state(2)`. Returns whether it
    /// counted among its target's repairers before (the caller's to leave them) and the target
    /// it still holds.
    ///
    /// # Errors
    ///
    /// The unit's stop that the motion step does not model.
    pub fn halt(&mut self, mover: Option<&mut Mover>) -> Result<bool, GenieError> {
        let counted = replace(&mut self.counted, false);
        if self.state.take_move_to().is_some()
            && let Some(m) = mover
        {
            m.stop()?;
        }
        self.state = RepairState::Halted;
        Ok(counted)
    }
}
