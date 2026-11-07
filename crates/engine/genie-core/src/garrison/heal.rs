//! A building's heal of its units inside and its repair by the villagers inside: the end of
//! `TRIBE_Building_Object::update`'s whole seconds (`0x1411c9000`).

use crate::GenieResult;
use crate::combat::damage::ArmorPair;
use crate::combat::damage::heal_base_hp;
use crate::construction::repair::RepairOutcome;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::layers::Action;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

impl Unit {
    /// The heal of this building's units inside, from the first (`+0x2f8`, whoever's each is,
    /// [`World::live_inside`]; [`Unit::building_seconds`]): each of its own master's class 4
    /// counts as a villager; with a heal `rate` above 0 (`COMISS`, `SETA`: a NaN heals none), one
    /// below its own master's hit points (`COMISS`, `JBE`: a NaN is not) heals by the rate. Below
    /// type 70 it heals by its own slots (`FUN_141193b30`, [`heal_base_hp`]); a combat unit and a
    /// building inside heal as a combat object (`FUN_141213b10`, [`World::heal`]: for a building
    /// not its own slot 0x110), and one that this left full gets back the armor its charge's
    /// hits took (`FUN_1412066a0`: `+0x9e4` zeroed). Returns how many villagers are inside, whose
    /// repair follows ([`Unit::repair_by_garrison`]).
    ///
    /// [`World::live_inside`]: crate::world::World::live_inside
    /// [`World::heal`]: crate::world::World::heal
    ///
    /// # Errors
    ///
    /// A unit inside outside the live lists, a master its tables lack, and the heal's.
    pub(crate) fn heal_garrison(
        &self,
        step: &mut Step<'_>,
        rate: HitPoints,
        roles: &Roles,
    ) -> GenieResult<usize> {
        let heals = HitPoints::ZERO < rate;
        let mut villagers = 0;
        for &id in &self.garrison {
            let (player, list_index) = step.world.live_inside(id)?;
            let unit = step.world.players.live(player, list_index)?;
            let master = unit.master(
                MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
                player,
            )?;
            if roles.of_class(master.class).contains(ClassRoles::VILLAGER) {
                villagers += 1;
            }
            let below = unit.life.hp < HitPoints::of_master(master.hit_points);
            if heals && below {
                if master.kind < MasterKind::COMBAT {
                    let master_hp = master.hit_points;
                    let unit = step.world.players.live_mut(player, list_index)?;
                    let _ = heal_base_hp(&mut unit.life.hp, master_hp, rate);
                } else if step.world.heal(player, list_index, rate)? == RepairOutcome::Full {
                    step.world.parts.combat_mut(id)?.armor_removed = ArmorPair::ZERO;
                }
            }
        }
        Ok(villagers)
    }

    /// The repair of this building by its `villagers` inside, after their heal
    /// ([`Unit::building_seconds`]): with villagers inside, its master's `garrison_repair_rate`
    /// `rate` (`+0x9c4`) above 0 and its hit points below its master's `master_hp` (`COMISS`,
    /// `JBE`: a NaN repairs none), its own slot 0x110 by the villagers times the rate
    /// (`FUN_1411d33e0`, [`World::heal_building`]), the building lent to the world. The update
    /// drops the answer.
    ///
    /// [`World::heal_building`]: crate::world::World::heal_building
    ///
    /// # Errors
    ///
    /// The lend's and the heal's.
    pub(crate) fn repair_by_garrison(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        villagers: usize,
        rate: HitPoints,
        master_hp: MasterHitPoints,
    ) -> GenieResult<()> {
        let repairs = 0 < villagers
            && HitPoints::ZERO < rate
            && self.life.hp < HitPoints::of_master(master_hp);
        if !repairs {
            return Ok(());
        }
        // the villagers' count as the game's int converts it (`CVTDQ2PS`)
        let amount = HitPoints::new(villagers as f32 * rate.get());
        let _ = step.lend_at((&mut *self, &mut *actions), |world, player, list_index| {
            world.heal_building(player, list_index, amount)
        })?;
        Ok(())
    }
}
