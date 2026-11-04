//! readyToAttack (`FUN_140a7f9d0`): the ready state, the follow, the range and sight tests
//! and the facing at its end.

use crate::GenieResult;
use crate::combat::MissileCount;
use crate::combat::attack::Aim;
use crate::combat::attack::AttackState;
use crate::combat::attack::AttackerFlags;
use crate::combat::attack::{
    Attack, AttackReads, Fight, Ground, HALF, NO_RANGE, NONE, Readiness, TARGET_GONE, face_target,
    move_reads,
};
use crate::map::Distance;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::{Placed, Spatial};
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::math::rng::Well512;
use crate::movement::MoveStart;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::footprint_gap;
use crate::movement::gate_gap;
use crate::objects::ObjectId;
use crate::objects::layers::MotionMut;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::MasterKind;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitStance;
use crate::world::ATTACKER_UNMOVING;
use crate::world::Seconds;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpriteTuning;

/// Whether readyToAttack (`FUN_140a7f9d0`'s third argument) tests the ready-at-once case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtOnce {
    /// It does (0).
    Tested,
    /// It does not (1, `set_state(7)`'s range clamp).
    Skipped,
}

impl Attack {
    /// `set_state(6)` (case 6): with a target or a point (`+0x38` not −1, `UCOMISS`: a NaN one
    /// is), the range test ([`Attack::ready_to_attack`]); in reach and ready, the swing
    /// (`set_state` 7). Not ready, a unit whose slot 0x5d8 answers 1 (a master whose move
    /// algorithm has bit 0) clears its `+0x958`, the count that picks the second attack sprite,
    /// which the slice does not hold ([`Attack::set_swing`]); readyToAttack never answers 2, so
    /// the `set_state(13)` after it is not reached. Without either a unit's own Attack posts
    /// 0x1fa with 600 (the action completed) and ends (`set_state` 1, [`Attack::finish`]).
    pub(super) fn ready(
        &mut self,
        unit: &mut Unit,
        ground: &mut Ground<'_>,
        fight: &Fight,
        attack_reads: &AttackReads,
    ) -> GenieResult<()> {
        let sprite_tuning = ground.by_id.tuning().sprite;
        self.enter(AttackState::Ready, ground.motion.reborrow())?;
        if self.target.is_none() && self.point.x() == NONE {
            if self.voice.is_loud() {
                ground.post(unit, NotifyKind::ACTION_COMPLETED, [600, 0, 0])?;
            }
            return self.finish(
                unit,
                ground.motion.reborrow(),
                ground.sprite_rng,
                sprite_tuning,
            );
        }
        match self.ready_to_attack(unit, ground, fight, attack_reads, AtOnce::Tested)? {
            Readiness::Ready => self.set_swing(unit, ground, fight, attack_reads),
            Readiness::Waiting => Ok(()),
            Readiness::Away(gap) => self.follow(unit, ground, attack_reads, fight, gap),
        }
    }

    /// readyToAttack's answer to a target out of reach of a unit that may follow it (its notify
    /// 0x1fd posted): `set_state(4)`, the `MoveTo` after it with the range of slot 0x238 (the
    /// master's weapon range).
    pub(super) fn follow(
        &mut self,
        unit: &mut Unit,
        ground: &mut Ground<'_>,
        attack_reads: &AttackReads,
        fight: &Fight,
        gap: Distance,
    ) -> GenieResult<()> {
        let move_to_tuning = &ground.by_id.tuning().move_to;
        let sprite_tuning = ground.by_id.tuning().sprite;
        self.enter(
            AttackState::Move { move_to: None },
            ground.motion.reborrow(),
        )?;
        let reads = move_reads((unit, ground.motion.mover()), attack_reads, ground.graphics)?;
        if self.moving(
            fight.own.source.weapon_range,
            fight.victim.position,
            gap,
            &reads,
            move_to_tuning,
        )? == MoveStart::Arrived
        {
            self.arrived_sprite(
                unit,
                ground.motion.reborrow(),
                ground.sprite_rng,
                sprite_tuning,
            )?;
        }
        Ok(())
    }

    /// readyToAttack (`FUN_140a7f9d0`) on an object; `at_once` is its third argument (1 from
    /// `set_state(7)`'s range clamp: [`AtOnce::Skipped`]). A target of a player (not Gaia) out
    /// of the unit's player's sight (its slot 0x1e0 neither 0xf nor 0x80 with its master's fog
    /// flag) fails the attack (`set_state` 13) unless the unit AI's stance (`+0x1a9`) is 0, the
    /// top speed above 0 (`COMISS`/`JBE`) and canMoveToAttack 1, which waits
    /// ([`Attack::wait_at_seen`]: `set_state` 5, a `MoveTo` to where it was seen). A unit that moves (a master of type 30 and
    /// up with a speed, `FUN_140fdd080`; the slice reads the unit's top speed, above 0 just when
    /// the master's is) whose target stands on the attack's point (`FUN_140a7bbf0`) or within
    /// half a tile of it on each axis (`FUN_140a7bd10`: a NaN difference is not within) is ready
    /// at once; so is a unit that does not move with such a target in its range ([`in_range`]),
    /// or with a volley under way (`FUN_1412161a0`). Else the reach decides, by the target's gap
    /// (`FUN_14118f680`): below the least range (`COMISS`/`JA`) a unit's own Attack posts 0x1fe
    /// (−1, −1, −1); a unit with a top speed (`UCOMISS`: not 0) whose unit AI may move to attack
    /// (slot 0x1d0) waits, any other fails (`set_state` 13). Beyond the reach (the range, half a
    /// tile more without one) a unit's own Attack posts 0x1fd; stance 1's leash fails a target
    /// too far from the unit AI's `state_position` ([`Attack::off_leash`]); a unit that may move follows the target
    /// (`set_state` 4, [`Readiness::Away`]), any other fails. Within it the target's place becomes
    /// the point and the unit faces it ([`Attack::face`]). A gate's reach (class 39) is to the
    /// nearest of it and its first two parts (its slot 0x680 through `FUN_1415417a0`, `MINSS`:
    /// [`crate::movement::gate_gap`]).
    pub(super) fn ready_to_attack(
        &mut self,
        unit: &mut Unit,
        ground_view: &mut Ground<'_>,
        fight: &Fight,
        attack_reads: &AttackReads,
        at_once: AtOnce,
    ) -> GenieResult<Readiness> {
        let move_to_tuning = &ground_view.by_id.tuning().move_to;
        let sprite_tuning = ground_view.by_id.tuning().sprite;
        let roles = ground_view.by_id.roles();
        let attack_tuning = &ground_view.by_id.tuning().attack;
        let (target, placed) = self.held_target(ground_view.map)?;
        let speed = ground_view
            .motion
            .mover()
            .ok_or(ATTACKER_UNMOVING)?
            .stats
            .speed;
        if target_unseen(ground_view.map, fight, &placed)? {
            if attack_reads.stance == UnitStance::Aggressive
                && Speed::ZERO < speed
                && attack_reads.flags.contains(AttackerFlags::CAN_MOVE)
            {
                self.wait_at_seen(
                    unit,
                    ground_view.motion.reborrow(),
                    fight,
                    attack_reads,
                    ground_view.graphics,
                    move_to_tuning,
                )?;
                return Ok(Readiness::Waiting);
            }
            return self.fail(
                unit,
                ground_view.motion.reborrow(),
                ground_view.sprite_rng,
                sprite_tuning,
            );
        }
        let at = unit.place.position();
        let ground = ground_view
            .motion
            .mover()
            .map(|mover| mover.stats.radius.ground())
            .ok_or(ATTACKER_UNMOVING)?;
        let gap = footprint_gap(at, ground, placed.position, placed.radius);
        let near = if self.range <= Distance::new(NO_RANGE) {
            gap + Distance::new(attack_tuning.short_range_gap)
        } else {
            gap
        };
        let mobile = MasterKind::MOVING <= fight.own.kind && Speed::ZERO < speed;
        if at_once == AtOnce::Tested
            && self.ready_at_once(
                placed.position,
                near,
                mobile || attack_reads.next_volley != MissileCount::ZERO,
            )
        {
            return self.face(
                (at, ground_view.motion.mover_mut()),
                (
                    placed.position,
                    ground_view.by_id.combat_of(unit.id).attack_timer,
                ),
                fight,
                attack_reads,
                roles,
            );
        }
        if gap < self.min_range {
            if self.voice.is_loud() {
                ground_view.post(unit, NotifyKind::TOO_CLOSE, [-1; 3])?;
            }
            if speed != Speed::ZERO && attack_reads.flags.contains(AttackerFlags::CAN_MOVE) {
                return Ok(Readiness::Waiting);
            }
            return self.fail(
                unit,
                ground_view.motion.reborrow(),
                ground_view.sprite_rng,
                sprite_tuning,
            );
        }
        // a gate's reach is to the nearest of it and its first two parts (slot 0x680, MINSS)
        let gap = gate_gap(
            ground_view.map,
            (at, ground),
            (target, &placed),
            gap,
            |distance| distance,
            ground_view.by_id,
            roles,
        );
        if self.reach() < gap {
            if self.voice.is_loud() {
                ground_view.post(unit, NotifyKind::OUT_OF_REACH, [0; 3])?;
            }
            if (attack_reads.stance == UnitStance::Defensive
                && self.off_leash(placed.position, attack_reads.state_position, attack_tuning))
                || speed == Speed::ZERO
                || !attack_reads.flags.contains(AttackerFlags::CAN_MOVE)
            {
                return self.fail(
                    unit,
                    ground_view.motion.reborrow(),
                    ground_view.sprite_rng,
                    sprite_tuning,
                );
            }
            return Ok(Readiness::Away(gap));
        }
        self.point = placed.position;
        self.face(
            (at, ground_view.motion.mover_mut()),
            (
                placed.position,
                ground_view.by_id.combat_of(unit.id).attack_timer,
            ),
            fight,
            attack_reads,
            roles,
        )
    }

    /// The target and its place on `map`.
    ///
    /// # Errors
    ///
    /// [`TARGET_GONE`] for none or a target the map does not hold.
    pub(super) fn held_target(&self, map: &Spatial) -> GenieResult<(ObjectId, Placed)> {
        let target = self.target.ok_or(TARGET_GONE)?;
        let placed = map.get_known(target)?.ok_or(TARGET_GONE)?;
        Ok((target.id(), *placed))
    }

    /// readyToAttack's ready-at-once test: the target stands on the attack's point
    /// (`FUN_140a7bbf0`), or within half a tile of it on each axis (`FUN_140a7bd10`: a NaN
    /// difference is not within) and either `moves_or_volleys` (a unit that moves, or a volley
    /// under way, `FUN_1412161a0`) or the target's gap `near` is in range ([`in_range`]).
    pub(super) fn ready_at_once(
        &self,
        to: Position,
        near: Distance,
        moves_or_volleys: bool,
    ) -> bool {
        let moved = to != self.point;
        let position = self.point;
        let far = [
            to.x() - position.x(),
            to.y() - position.y(),
            to.z() - position.z(),
        ]
        .iter()
        .any(|offset| below_or_unordered(HALF, offset.abs()));
        !moved || (!far && (moves_or_volleys || in_range(near, self.range, self.min_range)))
    }

    /// readyToAttack's `set_state(13)` (as [`Attack::finish_in`]); it answers 0.
    pub(super) fn fail(
        &mut self,
        unit: &mut Unit,
        motion: MotionMut<'_>,
        sprite_rng: &mut Well512,
        sprite_tuning: SpriteTuning,
    ) -> GenieResult<Readiness> {
        self.finish_in(AttackState::Failed, unit, motion, sprite_rng, sprite_tuning)?;
        Ok(Readiness::Waiting)
    }

    /// readyToAttack's ready end (`0x140a801f2`): the unit at `at` with its mover, its attack
    /// timer `attack_timer`, faces its target at `to`
    /// (`FUN_141162db0`, [`face_target`]; masters 1704 and 1706 skip it); one not facing it yet
    /// waits; one facing it swings once its attack timer ran out (slot 0x100, `FUN_141082d40`:
    /// `COMISS` with 0 then `SETBE`, so a NaN timer has run out).
    pub(super) fn face(
        &self,
        (at, mover): (Position, Option<&mut Mover>),
        (to, attack_timer): (Position, Seconds),
        fight: &Fight,
        attack_reads: &AttackReads,
        roles: &Roles,
    ) -> GenieResult<Readiness> {
        let mover = mover.ok_or(ATTACKER_UNMOVING)?;
        if !roles
            .of_master(fight.own.master_id)
            .contains(MasterRoles::UNTURNED_ATTACKER)
            && face_target(mover, at, to, attack_reads.world_time)? == Aim::Turning
        {
            return Ok(Readiness::Waiting);
        }
        Ok(if at_most_or_unordered(attack_timer.get(), 0.0) {
            Readiness::Ready
        } else {
            Readiness::Waiting
        })
    }
}

/// readyToAttack's ready-at-once range test of a unit that does not move (`0x140a7fd92`): the
/// gap `near` (half a tile less for a range up to 0.1) is not beyond the range (`COMISS`/`JA`), and
/// the least range is at most 0.1 or at most the gap (`COMISS`/`JBE` each: NaN passes).
pub(super) fn in_range(near: Distance, range: Distance, min_range: Distance) -> bool {
    use crate::math::fmath::at_most_or_unordered;
    at_most_or_unordered(near.get(), range.get())
        && (at_most_or_unordered(min_range.get(), NO_RANGE)
            || at_most_or_unordered(min_range.get(), near.get()))
}

/// readyToAttack's sight test: a target of a player (not Gaia) whose tile the unit's player does
/// not see (its slot 0x1e0 neither 0xf) nor has explored while the target's master shows in the
/// fog (0x80 with its fog flag).
///
/// # Errors
///
/// A tile the map's sight does not hold.
pub(super) fn target_unseen(world: &Spatial, fight: &Fight, placed: &Placed) -> GenieResult<bool> {
    let (tx, ty) = (placed.position.x() as i32, placed.position.y() as i32);
    Ok(placed.player != PlayerId::GAIA
        && !world.sees_tile(fight.own.player, Tile::new(tx, ty))
        && !(fight.victim.fog_visibility != FogVisibility::INVISIBLE
            && world.explored(fight.own.player, Tile::new(tx, ty))?))
}
