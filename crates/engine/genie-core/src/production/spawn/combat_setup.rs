//! The combat setup (`FUN_14121df60`): what a new combat object (a unit or a building) takes from
//! its master and its player, and what its player's attributes count of it.

use crate::GenieResult;
use crate::combat::combat_update::FAITH_TOP;
use crate::combat::{Charge, Firepower};
use crate::gathering::DecayTimer;
use crate::math::rng::Stream;
use crate::objects::lifecycle::death_parts::count_class;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::cost_entries;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::technology::Attributes;
use crate::world::Millis;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::layer::ClassRoles;
use crate::world::slots::PlayerSlotsExt;

/// What the combat setup writes on its object besides the zeroes and −1s of a new object's
/// defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CombatSetup {
    /// `+0xa00`: its master's volley fire (`+0x8c0`).
    pub(crate) firepower: Firepower,
    /// `+0x9e0`: its master's `max_charge` (`+0x918`). The test of the object's type against 70
    /// reads the 70 the routine wrote just before, so every object takes it.
    pub(crate) charge: Charge,
    /// `+0x99c` ([`DecayTimer::of_rate`]): its player's salvage decay rate (attribute 65) times
    /// 1000.
    pub(crate) decay_timer: DecayTimer,
    /// `+0xa54` ([`powerup_update_after`]): the world time plus the "setup" draw mod 100.
    pub(crate) powerup_update: WorldTime,
    /// `+0x308` and `+0x310`: the faith ([`World::setup_faith`]), if any.
    pub(crate) faith: Option<(AttributeId, Amount)>,
}

impl World {
    /// `FUN_14121df60` on player `player_id`'s new object of master `made`, after its static
    /// setup's storage: its class counters ([`count_class`], `FUN_141226060`), its faith, its
    /// decay timer and the "setup" draw (Object stream, line 0x1c1). A building (type 80) with a
    /// train location (an entry of `+0x898` other than −1) adds its paid cost to attribute 247
    /// (the player's slot 0xf8); any other object to 98, 164 and 246 (246 while the telemetry's
    /// match runs: `FUN_140982ff0()`'s `+0x921`, which every game's start sets, `FUN_140f1fb20`
    /// with `FUN_140980ac0`, and only its teardown `FUN_140c85fb0` clears, `FUN_140980de0`). A
    /// villager (class 4) adds one to attribute 248. What it writes on the object.
    ///
    /// Held elsewhere: the unit AI (`FUN_141215a90`, [`crate::unit_ai::UnitAi::constructed`]),
    /// the capture flag `+0x924`, the town-bell records (`+0x9ac`..`+0x9d8`, all −1:
    /// [`crate::orders::town_bell::TownBell`]'s default), the powerups `FUN_1412066b0` clears
    /// (0 on a new object, the speed powerup `+0xa2c` its infantry's: 0). Slot 277's caches of
    /// the master's tasks (`TRIBE_Combat_Object::slot277`, `0x141206d50`) are derived where read:
    /// the aura map `+0xa58` and its list `+0xaa8` by the powerup update
    /// ([`crate::combat::aura`]), `+0xb74` by [`crate::unit_ai::NewObject::of`]. Not held: the
    /// hit point transforms `+0xb20` and the sounds `+0xac4`..`+0xacc`.
    ///
    /// # Errors
    ///
    /// A player the world does not hold, without attribute 65 or one of the counted
    /// attributes, or whose masters the slice does not have.
    pub(crate) fn combat_setup(
        &mut self,
        player_id: PlayerId,
        made: &Master,
    ) -> GenieResult<CombatSetup> {
        count_class(
            &mut self.players.player_mut(player_id)?.attributes,
            made,
            1.0,
            &self.model.roles,
        );
        let faith = self.setup_faith(player_id, made)?;
        let slots = self.model.slots.attribute;
        let decay_rate =
            Attributes::get(self.players.player(player_id)?, slots.salvage_decay_rate)?;
        let powerup_draw = self.streams.get_mut(Stream::Object).rand15();
        let paid: i32 = cost_entries(&made.creatable_part().cost)
            .filter(|&(_, _, paid)| paid)
            .map(|(_, amount, _)| i32::from(amount.get()))
            .sum();
        let paid = Amount::new(paid as f32);
        let owner = self.players.player_mut(player_id)?;
        if made.kind == MasterKind::BUILDING {
            if !made.creatable_part().trained_at.is_empty() {
                owner.add_attribute(slots.building_cost, paid)?;
            }
        } else {
            owner.add_attribute(slots.total_value, paid)?;
            owner.add_attribute(slots.units_value, paid)?;
            owner.add_attribute(slots.made_units_cost, paid)?;
        }
        if self
            .model
            .roles
            .of_class(made.class)
            .contains(ClassRoles::VILLAGER)
        {
            owner.add_attribute(slots.made_villagers, Amount::new(1.0))?;
        }
        Ok(CombatSetup {
            firepower: made.creatable_part().volley_fire,
            charge: made.creatable_part().max_charge,
            decay_timer: DecayTimer::of_rate(decay_rate),
            powerup_update: powerup_update_after(self.world_time, powerup_draw),
            faith,
        })
    }

    /// The combat setup's faith (`FUN_14121df60`): an object of master `made` (player
    /// `player_id`'s) of a faithful class (18 or 53), or one whose master has a conversion task
    /// (`FUN_1415c60b0` with 0x68, [`World::has_task`]), holds the attribute kind
    /// [`AttributeSlots::faith`] (`+0x308`, the immediate 0x22) at [`FAITH_TOP`] (`+0x310`);
    /// none for any other.
    ///
    /// # Errors
    ///
    /// A player whose masters the slice does not have.
    ///
    /// [`AttributeSlots::faith`]: crate::world::layer::AttributeSlots::faith
    pub(crate) fn setup_faith(
        &self,
        player_id: PlayerId,
        made: &Master,
    ) -> GenieResult<Option<(AttributeId, Amount)>> {
        let faithful = self
            .model
            .roles
            .of_class(made.class)
            .contains(ClassRoles::FAITHFUL)
            || self.has_task(player_id, made.id, ActionKind::CONVERT)?;
        Ok(faithful.then_some((self.model.slots.attribute.faith, FAITH_TOP)))
    }
}

/// The combat setup's powerup update offset (`FUN_14121df60`, `+0xa54`): the world time `now`
/// plus the "setup" draw `draw` (Object stream, line 0x1c1) mod 100, in milliseconds.
pub(crate) const fn powerup_update_after(now: WorldTime, draw: u32) -> WorldTime {
    now.after(Millis::new(draw % 100))
}
