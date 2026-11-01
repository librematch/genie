//! A member sent at the target of its group's order (FD-036): the branch of `FUN_14102bbd0` for a
//! member within reach, with the gate rule that picks the part of a gate it strikes
//! (`FUN_14100dbe0`), the unit's speed (slot 0x208, `FUN_141202c70`) and its weapon reach (slot
//! 0x1e8, `FUN_141086330`).

use crate::GenieError;
use crate::combat::Charge;
use crate::combat::damage::SPECIAL_CHARGE_GAP;
use crate::groups::GroupId;
use crate::groups::WorldObject;
use crate::groups::attack::AttackOrder;
use crate::groups::group_list::member_gone;
use crate::groups::stats::{COMBAT_KINDS, weapon_range};
use crate::math::conversions::cvttss2si_64;
use crate::math::fmath::at_most_or_unordered;
use crate::math::image::group::EPSILON;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::class;
use crate::objects::masters::master_type;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::unit::Unit;
use crate::orders::Work;
use crate::players::PlayerId;
use crate::world::Millis;
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// The retarget timer of a unit sent at a target is the milliseconds it needs to get there at
/// its speed, drawn around (the immediate `2000`, as a 32-bit product).
const TRAVEL_MS: i32 = 2000;
/// The span of a unit that does not move ([`Travel::Any`]): the immediate `1000`.
const STANDING_SPAN: Millis = Millis::new(1000);
/// A combat unit of this type (`'F'`) gets the charge's speed.
const CHARGER: MasterKind = master_type::COMBAT;

/// What a unit sent at a target draws its retarget timer for ([`World::set_travel_timer`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::groups) enum Travel {
    /// Only a speed above [`EPSILON`] draws (a computer player's units and the special ones).
    Fast,
    /// Any unit draws: one without speed around [`STANDING_SPAN`].
    Any,
}

impl World {
    /// A member within reach of the target when the group is ordered on it (`FUN_14102bbd0`'s
    /// branch): it stops (slot 0x560, [`crate::movement::Mover::stand`]), takes the part of the
    /// target ([`World::gate_part`]) and the work command on it at the part's place (slot
    /// 0x128, [`World::give_work`]: carry test 1, `+0xa14` the order's retargeting byte, its task
    /// filter), stops again in an action state 6 or 7 (slot 0x2b0), and draws its retarget timer
    /// ([`World::set_travel_timer`]): any unit of a human owner draws, a computer's only the ones
    /// on the special list ([`crate::groups::UnitGroup::special_units`]) that move.
    ///
    /// # Errors
    ///
    /// A member, a part or a master the world does not hold, and what the stop, the work
    /// command and the speed do not model.
    pub(in crate::groups) fn send_in_reach(
        &mut self,
        group: GroupId,
        id: ObjectId,
        target: ObjectId,
        (order, computer): (AttackOrder, bool),
    ) -> Result<(), GenieError> {
        let part = self.gate_part(id, target)?;
        self.stand_unit(id)?;
        let at = self
            .world_object(part)
            .ok_or_else(|| member_gone(part))?
            .position();
        self.give_work(
            id,
            &Work {
                target: Some(part),
                point: at,
                carry_test: true,
                filter: order.filter,
                notified: order.disable_retargeting,
            },
        )?;
        let state = match self.object(id) {
            Some((p, u)) => self.action_list(p, u)?.1,
            None => ActionState::NO_ACTION,
        };
        if matches!(state.get(), 6 | 7) {
            self.stand_unit(id)?;
        }
        if !computer {
            return self.set_travel_timer(id, part, Travel::Any);
        }
        if self.groups.held(group)?.special_units.contains(&id) {
            self.set_travel_timer(id, part, Travel::Fast)?;
        }
        Ok(())
    }

    /// Slot 0x560 of object `id`: its mover stands ([`crate::movement::Mover::stand`]); nothing
    /// for an object the world does not hold or one without a mover.
    ///
    /// # Errors
    ///
    /// What the stop does not model.
    pub(super) fn stand_unit(&mut self, id: ObjectId) -> Result<(), GenieError> {
        if let Some((_, u)) = self.object_mut(id)
            && let Some(mover) = u.body.mover_mut()
        {
            mover.stand()?;
        }
        Ok(())
    }

    /// A unit sent at `part` draws its retarget timer from the time to get there: the footprint
    /// gap (`FUN_14118f680`) over its speed (slot 0x208), in whole seconds (`CVTTSS2SI`, 64
    /// bits) times [`TRAVEL_MS`], as the span of its unit AI's randomized timer
    /// ([`crate::unit_ai::UnitAi::set_randomized_retarget_timer`]). By `rule` a unit that does
    /// not move draws nothing ([`Travel::Fast`]) or around [`STANDING_SPAN`] ([`Travel::Any`]).
    ///
    /// # Errors
    ///
    /// A unit or a part the world does not hold, and what the speed does not model.
    pub(in crate::groups) fn set_travel_timer(
        &mut self,
        id: ObjectId,
        part: ObjectId,
        rule: Travel,
    ) -> Result<(), GenieError> {
        let (unit, part) = self
            .world_object(id)
            .zip(self.world_object(part))
            .ok_or_else(|| member_gone(id))?;
        let WorldObject::Unit(player, u) = unit else {
            return Ok(());
        };
        let speed = self.unit_speed(player, u)?;
        let span = match rule {
            Travel::Fast if at_most_or_unordered(speed, EPSILON) => return Ok(()),
            Travel::Any if speed == 0.0 => STANDING_SPAN,
            _ => {
                let gap = self.footprint_gap(unit, part)?;
                Millis::new(
                    (cvttss2si_64((gap / speed).get()) as i32).wrapping_mul(TRAVEL_MS) as u32,
                )
            }
        };
        let slot = self.live_index(player, id);
        let rng = self.streams.get(Stream::Ai);
        if let Some(ai) = slot
            .and_then(|i| self.players.live_mut(player, i).ok())
            .and_then(|u| self.parts.unit_ai.get_mut(u.id))
        {
            ai.set_randomized_retarget_timer(span, rng);
        }
        Ok(())
    }

    /// `FUN_14100dbe0` for unit `id` and target `target`: the object the unit strikes. A gate
    /// (a building of class 39) out of the unit's reach (slot 0x1e8: [`World::in_weapon_range`]
    /// false) is struck at the nearest of itself and its annexes by the footprint gap squared
    /// (`FUN_14118f8c0`), the gate first; any other target stands for itself. The id.
    ///
    /// # Errors
    ///
    /// A unit or a target the world does not hold, and a master it lacks.
    pub(in crate::groups) fn gate_part(
        &self,
        id: ObjectId,
        target: ObjectId,
    ) -> Result<ObjectId, GenieError> {
        let (unit, t) = self
            .world_object(id)
            .zip(self.world_object(target))
            .ok_or_else(|| member_gone(id))?;
        let master = self.master(t.player(), t.master_id())?;
        if master.kind != BUILDING
            || master.class != class::GATE
            || self.in_weapon_range(unit, t)?
        {
            return Ok(t.id());
        }
        let mut part = t;
        let mut nearest = self.footprint_gap_squared(unit, t)?;
        for annex in self.objects().annexes(t.id()).ids().take(ANNEXES) {
            let Some(a) = self.world_object(annex) else {
                continue;
            };
            let gap = self.footprint_gap_squared(unit, a)?;
            if gap < nearest {
                part = a;
                nearest = gap;
            }
        }
        Ok(part.id())
    }

    /// Slot 0x1e8 of `unit` on `target` (`FUN_141086330`): a combat object whose footprint gap
    /// squared to the target is at most its weapon range squared and at least its least range
    /// squared (each `COMISS`: a NaN is not in range); any other object's 0.
    ///
    /// # Errors
    ///
    /// A master the world lacks.
    pub(super) fn in_weapon_range(
        &self,
        unit: WorldObject<'_>,
        target: WorldObject<'_>,
    ) -> Result<bool, GenieError> {
        let master = self.master(unit.player(), unit.master_id())?;
        if !COMBAT_KINDS.contains(&master.kind) {
            return Ok(false);
        }
        let gap = self.footprint_gap_squared(unit, target)?;
        let range = weapon_range(master);
        Ok(gap <= range.squared() && master.min_range.squared() <= gap)
    }

    /// Slot 0x208 of a unit of player `player` (`FUN_141202c70`): its top speed, which the
    /// mover holds (its powerups, `+0xa2c` and `+0xb38`, and the count at `+0xa08` that gives 0
    /// are not held). A combat unit of type 70 whose master has a charge (`max_charge` above
    /// 0) runs faster within reach of its target while its charge is ready, which only a master
    /// of special attack 3 can have (`FUN_1412161b0`).
    ///
    /// # Errors
    ///
    /// A master the world lacks and a charge of special attack 3.
    pub(super) fn unit_speed(&self, player: PlayerId, unit: &Unit) -> Result<f32, GenieError> {
        let Some(mover) = unit.body.mover() else {
            return Ok(0.0);
        };
        let speed = mover.stats.speed;
        if speed == Speed::ZERO {
            return Ok(speed.get());
        }
        let master = unit.master(self.master_tables(), player)?;
        if master.kind != CHARGER
            || master.max_charge <= Charge::ZERO
            || master.special_attack != SpecialAttack::CHARGE
        {
            return Ok(speed.get());
        }
        Err(SPECIAL_CHARGE_GAP)
    }
}

/// A building holds at most this many annexes (`+0xbc0`..`+0xbd8`).
const ANNEXES: usize = 4;
