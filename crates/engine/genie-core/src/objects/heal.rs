//! An object's heal by an amount (slot 0x110): a combat object's (`FUN_141213b10`), at a cost for
//! the ships' classes, and a building's (`FUN_1411d33e0`), always at a cost, both paid by the
//! object's owner.

use crate::GenieResult;
use crate::combat::damage::TransformTest;
use crate::combat::damage::{heal_hp, most_hp};
use crate::construction::repair::RepairOutcome;
use crate::construction::repair::{BUILDING_REPAIR_COST, UNIT_REPAIR_COST};
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::masters::Master;
use crate::objects::masters::MasterTables;
use crate::objects::masters::player_masters;
use crate::players::PlayerId;
use crate::production::CostCount;
use crate::production::Payment;
use crate::production::PopulationTest;
use crate::production::pay_with;
use crate::world::NO_ATTRIBUTE;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::slots::PlayerSlotsExt;

/// The hit points a heal at a cost makes of `amount` (`FUN_141213b10`, `FUN_1411d33e0`): the
/// master's hit points times `amount` over its first train location's time (`+0x898`'s first
/// entry's `+2`, read live; 0 without one, a division by 0.0 as the game's).
fn costed_heal(master: &Master, amount: HitPoints) -> HitPoints {
    let train_time = master
        .creatable_part()
        .train_locations
        .first()
        .map_or(WholeSeconds::ZERO, |train_location| train_location.time);
    HitPoints::new(
        HitPoints::of_master(master.hit_points).get() * amount.get()
            / Seconds::of_master(train_time).get(),
    )
}

impl World {
    /// Slot 0x110 of a combat object (`FUN_141213b10`, types 70 and 80) at `list_index` of
    /// `player` by `amount`. A master of a class whose heal costs
    /// ([`ClassRoles::HEALED_AT_COST`]: the ships' 2, 20, 21, 22, 53) heals at a cost first: the
    /// hit points of [`costed_heal`], at most what its hit points with its powerup's room (slot
    /// 0xc0) lack of its most (slot 200, [`most_hp`]; `COMISS`, `JBE`: a NaN takes no less), then
    /// its master's hit points less its own; its owner's attribute 270 ([`UNIT_REPAIR_COST`])
    /// times them over its most pays that share of its master's costs ([`pay_with`] with
    /// `param_4` 1), and an owner who cannot pay heals nothing ([`RepairOutcome::Unpaid`], 2).
    /// Then it heals (`FUN_141193b30`, [`heal_hp`]): its slot 0xb8 runs its hit point
    /// transforms' test at once ([`World::transform_by_hit_points`]); full is 1
    /// ([`RepairOutcome::Full`]), else 0.
    ///
    /// # Errors
    ///
    /// An object the world does not hold live, a master its tables lack, an attribute its owner
    /// does not have, and the transforms'.
    pub(crate) fn heal(
        &mut self,
        player: PlayerId,
        list_index: ListIndex,
        amount: HitPoints,
    ) -> GenieResult<RepairOutcome> {
        let unit = self.players.live(player, list_index)?;
        let id = unit.id;
        let master = unit.master(
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            player,
        )?;
        let mut amount = amount;
        if self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::HEALED_AT_COST)
        {
            let combat = self.parts.combat_of(id);
            let most = most_hp(combat.hp_stinger, master.hit_points, combat.hp_powerup);
            let mut heal = costed_heal(master, amount);
            if most < combat.hp_powerup.room() + unit.life.hp + heal {
                heal = HitPoints::of_master(master.hit_points) - unit.life.hp;
            }
            let owner = self.players.player_mut(player)?;
            let rate = owner.attributes.get(UNIT_REPAIR_COST).ok_or(NO_ATTRIBUTE)?;
            let share = CostCount::new(rate.get() * heal.get() / most.get());
            if pay_with(
                player_masters(&self.model.masters, player)?,
                master.id,
                share,
                PopulationTest::Skipped,
                owner,
                &self.model.roles,
                self.model.slots,
            )? == Payment::Unpaid
            {
                return Ok(RepairOutcome::Unpaid);
            }
            amount = heal;
        }
        let master_hp = master.hit_points;
        let unit = self.players.live_mut(player, list_index)?;
        let combat = self.parts.combat_mut(id)?;
        let healed = heal_hp(
            &mut unit.life.hp,
            &mut combat.hp_powerup,
            master_hp,
            combat.hp_stinger,
            amount,
        );
        if healed.test == TransformTest::Runs {
            self.transform_by_hit_points(player, list_index)?;
        }
        Ok(RepairOutcome::from(healed.heal))
    }

    /// Slot 0x110 of a building (`FUN_1411d33e0`) at `list_index` of `player` by `amount`, always
    /// at a cost: the hit points of [`costed_heal`], at most what its hit points lack of its
    /// master's (`COMISS`, `JBE`: a NaN takes no less); its owner's attribute 271
    /// ([`BUILDING_REPAIR_COST`]) times them over its master's hit points pays that share of its
    /// master's costs, the population's entry tested ([`pay_with`] with `param_4` 0). Paid, the
    /// building heals by them as a combat object does ([`World::heal`]); else it heals nothing
    /// ([`RepairOutcome::Unpaid`], 2).
    ///
    /// # Errors
    ///
    /// An object the world does not hold live, a master its tables lack, an attribute its owner
    /// does not have, and the heal's.
    pub(crate) fn heal_building(
        &mut self,
        player: PlayerId,
        list_index: ListIndex,
        amount: HitPoints,
    ) -> GenieResult<RepairOutcome> {
        let building = self.players.live(player, list_index)?;
        let master = building.master(
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            player,
        )?;
        let most = HitPoints::of_master(master.hit_points);
        let mut heal = costed_heal(master, amount);
        if most < building.life.hp + heal {
            heal = most - building.life.hp;
        }
        let owner = self.players.player_mut(player)?;
        let rate = owner
            .attributes
            .get(BUILDING_REPAIR_COST)
            .ok_or(NO_ATTRIBUTE)?;
        let share = CostCount::new(rate.get() * heal.get() / most.get());
        if pay_with(
            player_masters(&self.model.masters, player)?,
            master.id,
            share,
            PopulationTest::Tested,
            owner,
            &self.model.roles,
            self.model.slots,
        )? == Payment::Unpaid
        {
            return Ok(RepairOutcome::Unpaid);
        }
        self.heal(player, list_index, heal)
    }
}
