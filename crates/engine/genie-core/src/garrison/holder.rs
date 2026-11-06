//! A container's own part of a unit's enter and leave (its slots 0x1c0 and 0x1c8): its list,
//! its volleys and a building's counts.

use crate::GenieResult;
use crate::combat::Firepower;
use crate::garrison::ContainerId;
use crate::garrison::GarrisonCounts;
use crate::garrison::{NO_MASTERS, attack_rate, share};
use crate::objects::ObjectId;
use crate::objects::layers::{Building, Combat};
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::parts::PartTable;
use crate::world::step::Step;

impl Unit {
    /// [`Unit::go_inside`]'s slot 0x1c0 of container `target` (`FUN_14120b790`, a building's
    /// through `FUN_1411cb850`; its owner `bp`, its reload powerup `reload`), the unit's master
    /// `me` and the container's `bm`: a unit (type 70) adds its share to the container's
    /// volleys, and its list takes the unit (`FUN_14118c890`); a building (type 80) then counts
    /// the unit in by its countdown to its raid (`+0x9a0`, [`GarrisonCounts::of_unit`]).
    pub(crate) fn join_building(
        &self,
        step: &mut Step<'_>,
        target: Option<ContainerId>,
        bp: PlayerId,
        reload: Seconds,
        me: &Master,
        bm: &Master,
    ) -> GenieResult<()> {
        let roles = &step.world.model.roles.clone();
        let masters = step
            .world
            .model
            .masters
            .get(step.player.index())
            .ok_or(NO_MASTERS)?;
        let firepower = if me.kind == MasterKind::COMBAT {
            let own = step.world.model.masters.of(bp).unwrap_or(masters);
            let rate = attack_rate(bm, own, reload, roles, &step.world.model.tuning.garrison);
            share(
                me,
                masters,
                step.world.parts.combat_of(self.id).reload_powerup,
                rate,
                roles,
            )?
        } else {
            Firepower::ZERO
        };
        if let Some((_, unit)) = step.world.object_mut(target) {
            unit.garrison.push(self.id);
            let id = unit.id;
            step.world.parts.combat_mut(id)?.firepower += firepower;
            if bm.kind == MasterKind::BUILDING {
                let countdown = step.world.parts.combat_of(self.id).raider_countdown;
                let counts = GarrisonCounts::of_unit(me, countdown, roles);
                step.world
                    .parts
                    .building_mut(id)?
                    .garrison_counts
                    .count_in(counts);
            }
        }
        Ok(())
    }

    /// The container's slot 0x1c8 (`FUN_141217910`, a building's through `FUN_1411d6ad0`; its
    /// master `bm`) for unit `id` that leaves it, with its share `share` of the volleys and what
    /// it counts `counts` ([`GarrisonCounts::of_unit`] by its countdown to its raid, `+0x9a0`,
    /// which slot 0x1a8 ends only after it): the share comes off the volleys in `combats`, back
    /// to the master's volley fire when none is left inside or they fell below it; its list lets
    /// the unit go (`FUN_141197e10`); a building (type 80) then counts the unit out of its layer
    /// in `buildings`.
    ///
    /// # Errors
    ///
    /// None (an unreachable error where a table lost the layer it took).
    pub(crate) fn let_go(
        &mut self,
        combats: &mut PartTable<Combat>,
        buildings: &mut PartTable<Building>,
        id: ObjectId,
        share: Option<Firepower>,
        counts: GarrisonCounts,
        bm: &Master,
    ) -> GenieResult<()> {
        if let Some(share) = share {
            combats.of_mut(self.id)?.firepower -= share;
        }
        if let Some(slot) = self.garrison.iter().position(|&object_id| object_id == id) {
            let _ = self.garrison.swap_remove(slot);
        }
        // COMISS volley, firepower; JBE keeps it: a NaN firepower stays
        if self.garrison.is_empty()
            || combats.of(self.id).firepower < bm.creatable_part().volley_fire
        {
            combats.of_mut(self.id)?.firepower = bm.creatable_part().volley_fire;
        }
        if bm.kind == MasterKind::BUILDING {
            buildings.of_mut(self.id)?.garrison_counts.count_out(counts);
        }
        Ok(())
    }
}
