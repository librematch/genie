//! A container's own part of a unit's enter and leave (its slots 0x1c0 and 0x1c8): its list,
//! its volleys and a building's counts.

use crate::GenieResult;
use crate::combat::Firepower;
use crate::garrison::ContainerId;
use crate::garrison::GarrisonCounts;
use crate::garrison::{NO_MASTERS, attack_rate, share};
use crate::objects::ObjectId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::layer::Roles;
use crate::world::parts::Parts;
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
    /// master `bm`, its parts in `parts`) for unit `id` of master `made` that leaves it, with its
    /// share `share` of the volleys: the share comes off the volleys, back to the master's volley
    /// fire when none is left inside or they fell below it; its list lets the unit go
    /// (`FUN_141197e10`); a building (type 80) then counts the unit out by its countdown to its
    /// raid (`+0x9a0`, [`GarrisonCounts::of_unit`]), which slot 0x1a8 ends only after it.
    ///
    /// # Errors
    ///
    /// None (an unreachable error where a parts' table lost the layer it took).
    pub(crate) fn let_go(
        &mut self,
        parts: &mut Parts,
        id: ObjectId,
        share: Option<Firepower>,
        made: &Master,
        bm: &Master,
        roles: &Roles,
    ) -> GenieResult<()> {
        if let Some(share) = share {
            parts.combat_mut(self.id)?.firepower -= share;
        }
        if let Some(slot) = self.garrison.iter().position(|&object_id| object_id == id) {
            let _ = self.garrison.swap_remove(slot);
        }
        // COMISS volley, firepower; JBE keeps it: a NaN firepower stays
        if self.garrison.is_empty()
            || parts.combat_of(self.id).firepower < bm.creatable_part().volley_fire
        {
            parts.combat_mut(self.id)?.firepower = bm.creatable_part().volley_fire;
        }
        if bm.kind == MasterKind::BUILDING {
            let countdown = parts.combat_of(id).raider_countdown;
            let counts = GarrisonCounts::of_unit(made, countdown, roles);
            parts
                .building_mut(self.id)?
                .garrison_counts
                .count_out(counts);
        }
        Ok(())
    }
}
