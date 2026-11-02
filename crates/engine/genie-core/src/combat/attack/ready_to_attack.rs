//! readyToAttack (`FUN_140a7f9d0`): the ready state, the follow, the range and sight tests
//! and the facing at its end.

use crate::GenieError;
use crate::combat::attack::AttackState;
use crate::combat::attack::{
    Attack, AttackReads, Fight, Ground, HALF, MASTER_1704, MASTER_1706, NO_RANGE, NONE, Readiness,
    SHORT_RANGE_GAP, TARGET_GONE, face_target, move_reads,
};
use crate::map::Distance;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::{Placed, Spatial};
use crate::math::rng::Well512;
use crate::movement::Speed;
use crate::movement::footprint_gap;
use crate::objects::ObjectId;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::master_type::MOVING;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitStance;
use crate::world::Seconds;

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
        g: &mut Ground<'_>,
        f: &Fight,
        r: &AttackReads,
    ) -> Result<(), GenieError> {
        self.enter(AttackState::Ready, unit)?;
        if self.target.is_none() && self.point.x() == NONE {
            if !self.sub_action {
                g.post(unit, NotifyKind::ACTION_COMPLETED, [600, 0, 0])?;
            }
            return self.finish(unit, g.sprite_rng);
        }
        match self.ready_to_attack(unit, g, f, r, false)? {
            Readiness::Ready => self.set_swing(unit, g, f, r),
            Readiness::Waiting => Ok(()),
            Readiness::Away(gap) => self.follow(unit, g, r, f, gap),
        }
    }

    /// readyToAttack's answer to a target out of reach of a unit that may follow it (its notify
    /// 0x1fd posted): `set_state(4)`, the `MoveTo` after it with the range of slot 0x238 (the
    /// master's weapon range).
    pub(super) fn follow(
        &mut self,
        unit: &mut Unit,
        g: &mut Ground<'_>,
        r: &AttackReads,
        f: &Fight,
        gap: Distance,
    ) -> Result<(), GenieError> {
        self.enter(AttackState::Move, unit)?;
        let reads = move_reads(unit, r, g.graphics)?;
        if self.moving(f.own.source.weapon_range, f.victim.position, gap, &reads)? {
            self.arrived_sprite(unit, g.sprite_rng)?;
        }
        Ok(())
    }

    /// readyToAttack (`FUN_140a7f9d0`) on an object; `skip_ready` is its third argument (1 from
    /// `set_state(7)`'s range clamp: no ready-at-once test). A target of a player (not Gaia) out
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
        g: &mut Ground<'_>,
        f: &Fight,
        r: &AttackReads,
        skip_ready: bool,
    ) -> Result<Readiness, GenieError> {
        let (target, t) = self.held_target(g.map)?;
        let speed = unit
            .body
            .mover()
            .ok_or(crate::world::ATTACKER_UNMOVING)?
            .stats
            .speed;
        if target_unseen(g.map, f, &t)? {
            if r.stance == UnitStance::AGGRESSIVE && Speed::ZERO < speed && r.can_move {
                self.wait_at_seen(unit, f, r, g.graphics)?;
                return Ok(Readiness::Waiting);
            }
            return self.fail(unit, g.sprite_rng);
        }
        let at = unit.body.position();
        let ground = unit
            .body
            .mover()
            .map(|m| m.stats.radius.ground())
            .ok_or(crate::world::ATTACKER_UNMOVING)?;
        let gap = footprint_gap(at, ground, t.position, t.radius);
        let near = if self.range <= Distance::new(NO_RANGE) {
            gap + SHORT_RANGE_GAP
        } else {
            gap
        };
        let mobile = MOVING <= f.own.kind && Speed::ZERO < speed;
        if !skip_ready && self.ready_at_once(t.position, near, mobile || r.volley_under_way) {
            return self.face(
                unit,
                (t.position, g.by_id.combat_of(unit.id).attack_timer),
                f,
                r,
            );
        }
        if gap < self.min_range {
            if !self.sub_action {
                g.post(unit, NotifyKind::TOO_CLOSE, [-1; 3])?;
            }
            if speed != Speed::ZERO && r.can_move {
                return Ok(Readiness::Waiting);
            }
            return self.fail(unit, g.sprite_rng);
        }
        // a gate's reach is to the nearest of it and its first two parts (slot 0x680, MINSS)
        let gap = crate::movement::gate_gap(g.map, (at, ground), (target, &t), gap, |g| g, g.by_id);
        if self.reach() < gap {
            if !self.sub_action {
                g.post(unit, NotifyKind::OUT_OF_REACH, [0; 3])?;
            }
            if (r.stance == UnitStance::DEFENSIVE && self.off_leash(t.position, r.state_position))
                || speed == Speed::ZERO
                || !r.can_move
            {
                return self.fail(unit, g.sprite_rng);
            }
            return Ok(Readiness::Away(gap));
        }
        self.point = t.position;
        self.face(
            unit,
            (t.position, g.by_id.combat_of(unit.id).attack_timer),
            f,
            r,
        )
    }

    /// The target and its place on `map`.
    ///
    /// # Errors
    ///
    /// [`TARGET_GONE`] for none or a target the map does not hold.
    pub(super) fn held_target(&self, map: &Spatial) -> Result<(ObjectId, Placed), GenieError> {
        let target = self.target.ok_or(TARGET_GONE)?;
        let t = map.get_known(target)?.ok_or(TARGET_GONE)?;
        Ok((target, *t))
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
        let p = self.point;
        let far = [to.x() - p.x(), to.y() - p.y(), to.z() - p.z()]
            .iter()
            .any(|d| crate::math::fmath::below_or_unordered(HALF, d.abs()));
        !moved || (!far && (moves_or_volleys || in_range(near, self.range, self.min_range)))
    }

    /// readyToAttack's `set_state(13)` (as [`Attack::finish_in`]); it answers 0.
    pub(super) fn fail(
        &mut self,
        unit: &mut Unit,
        sprite_rng: &mut Well512,
    ) -> Result<Readiness, GenieError> {
        self.finish_in(AttackState::Failed, unit, sprite_rng)?;
        Ok(Readiness::Waiting)
    }

    /// readyToAttack's ready end (`0x140a801f2`): the unit, its attack timer `attack_timer`, faces its target at `to`
    /// (`FUN_141162db0`, [`face_target`]; masters 1704 and 1706 skip it); one not facing it yet
    /// waits; one facing it swings once its attack timer ran out (slot 0x100, `FUN_141082d40`:
    /// `COMISS` with 0 then `SETBE`, so a NaN timer has run out).
    pub(super) fn face(
        &self,
        unit: &mut Unit,
        (to, attack_timer): (Position, Seconds),
        f: &Fight,
        r: &AttackReads,
    ) -> Result<Readiness, GenieError> {
        let at = unit.body.position();
        let mover = unit
            .body
            .mover_mut()
            .ok_or(crate::world::ATTACKER_UNMOVING)?;
        if !matches!(f.own.master_id, MASTER_1704 | MASTER_1706)
            && !face_target(mover, at, to, r.world_time)?
        {
            return Ok(Readiness::Waiting);
        }
        Ok(
            if crate::math::fmath::at_most_or_unordered(attack_timer.get(), 0.0) {
                Readiness::Ready
            } else {
                Readiness::Waiting
            },
        )
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
pub(super) fn target_unseen(world: &Spatial, f: &Fight, t: &Placed) -> Result<bool, GenieError> {
    let (tx, ty) = (t.position.x() as i32, t.position.y() as i32);
    Ok(t.player != PlayerId::GAIA
        && !world.sees_tile(f.own.player, Tile::new(tx, ty))
        && !(f.victim.fog_visibility != FogVisibility::INVISIBLE
            && world.explored(f.own.player, Tile::new(tx, ty))?))
}
