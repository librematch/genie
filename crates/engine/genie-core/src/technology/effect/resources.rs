//! The player's resources and attributes: the modify and multiply slots, the store, the
//! setters and the side effects of allied vision and the farms' food.

use crate::math::fmath::below_or_unordered;
use crate::objects::ObjectId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::technology::ResourceChange;
use crate::technology::effect::attribute_modifier::EffectHook;
use crate::world::AllySharing;
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

impl World {
    /// Slot 0x1b0 (`FUN_141620c70`) of player `player_id` on resource `attribute` with `amount`:
    /// every live combat unit's and building's volleys lose the shares of the units inside, the
    /// attribute changes (`FUN_141661160`: `change` sets it, slot 0xf0
    /// [`World::set_player_attribute`]; or adds to it, slot 0xf8 with 0 and 1,
    /// [`World::add_player_attribute`]; the local player's statistics are not simulation), and
    /// the volleys take the shares again. A change of resource 0x20 would not run in the screen's
    /// mode 4 (a playback's is not) or with the game's `+0x668` set (not held: taken unset).
    pub(super) fn modify_resource(
        &mut self,
        player_id: PlayerId,
        attribute: AttributeId,
        amount: Amount,
        change: ResourceChange,
    ) -> GenieResult<()> {
        self.all_garrison_volleys(player_id, EffectHook::Before)?;
        match change {
            ResourceChange::Set => self.set_player_attribute(player_id, attribute, amount)?,
            ResourceChange::Add => self.add_player_attribute(player_id, attribute, amount)?,
        }
        self.all_garrison_volleys(player_id, EffectHook::After)
    }

    /// Slot 0x1b8 (`FUN_141620e30`) of player `player_id` on resource `attribute` by `factor`: as
    /// [`World::modify_resource`], the attribute set to its value times `factor` (`FUN_1416614c0`).
    pub(super) fn multiply_resource(
        &mut self,
        player_id: PlayerId,
        attribute: AttributeId,
        factor: f32,
    ) -> GenieResult<()> {
        self.all_garrison_volleys(player_id, EffectHook::Before)?;
        let value = self.attribute_of(player_id, attribute) * factor;
        self.set_player_attribute(player_id, attribute, value)?;
        self.all_garrison_volleys(player_id, EffectHook::After)
    }

    /// `FUN_141217ac0` ([`EffectHook::Before`]) or `FUN_14120b910` on each of player `player_id`'s
    /// live combat units and buildings ([`World::garrison_volleys`]).
    pub(super) fn all_garrison_volleys(
        &mut self,
        player_id: PlayerId,
        hook: EffectHook,
    ) -> GenieResult<()> {
        let ids: Vec<ObjectId> = self
            .players
            .player(player_id)?
            .objects
            .iter()
            .map(|unit| unit.id)
            .collect();
        for id in ids {
            let kind = self
                .object(id)
                .map(|(_, unit)| unit.life.master_id)
                .and_then(|master_id| player_master(&self.model.masters, player_id, master_id))
                .map(|master| master.kind);
            if matches!(kind, Some(MasterKind::COMBAT | MasterKind::BUILDING)) {
                self.garrison_volleys(player_id, id, hook)?;
            }
        }
        Ok(())
    }

    /// Slot 0xf0 (`FUN_14162d610`) of player `player`: attribute `attribute` takes `amount`
    /// (the store, [`Player::store_attribute`]), with its side effects: 0x32 turns the allied
    /// vision on first ([`World::allied_vision_on`]); 0xc4 changes nothing for a player above 0
    /// and spreads to the others at most 0 (`COMISS`, `JC`: not a NaN;
    /// [`AttributeRoles::SPREAD_ATTRIBUTE`]), each other player's attribute written straight
    /// with no store ([`Player::set_attribute`]); 0xb7 rebuilds the masks after; 0x21 above 0
    /// calls the scripts' `EffectFunction<amount>` (the XS runtime is not modelled).
    ///
    /// [`Player::store_attribute`]: crate::world::Player::store_attribute
    /// [`Player::set_attribute`]: crate::world::Player::set_attribute
    pub(crate) fn set_player_attribute(
        &mut self,
        player: PlayerId,
        attribute: AttributeId,
        amount: Amount,
    ) -> GenieResult<()> {
        {
            let held = self.model.roles.of_attribute(attribute);
            if held.contains(AttributeRoles::ALLIED_VISION) {
                self.allied_vision_on(player)?;
            } else if held.contains(AttributeRoles::SPREAD_ATTRIBUTE) {
                if Amount::ZERO
                    < self.attribute_of(player, self.model.slots.attribute.spread_attribute)
                {
                    return Ok(());
                }
                // the message to the players is the interface's
                for player_id in PlayerId::each(self.players.len()).skip(1) {
                    if player_id != player
                        && self.attribute_of(player_id, self.model.slots.attribute.spread_attribute)
                            <= Amount::ZERO
                    {
                        // straight into the attribute, no store (`0x14162d834`)
                        self.players
                            .player_mut(player_id)?
                            .set_attribute(self.model.slots.attribute.spread_attribute, amount)?;
                    }
                }
            } else if held.contains(AttributeRoles::EFFECT_FUNCTION) && (Amount::ZERO < amount) {
                return Err(GenieError::unsupported(
                    "an effect's script function (attribute 0x21, EffectFunction in Effects.xs)",
                ));
            }
        }
        self.players
            .player_mut(player)?
            .store_attribute(attribute, amount, &self.model.roles);
        if self
            .model
            .roles
            .of_attribute(attribute)
            .contains(AttributeRoles::SHARED_VISION)
        {
            // the world's slot 0x78; the message is the interface's
            self.rebuild_vision()?;
        }
        Ok(())
    }

    /// Attribute 0x32's side effect: player `player_id`'s allied vision (`+0x2cc`) on, the masks
    /// rebuilt (the world's slot 0x78, `FUN_1415e92b0`) and its tile list refilled
    /// ([`World::refill_tiles`], `FUN_141285cb0`).
    pub(crate) fn allied_vision_on(&mut self, player_id: PlayerId) -> GenieResult<()> {
        self.players
            .player_mut(player_id)?
            .ally_sharing
            .set(AllySharing::VISION, true);
        self.rebuild_vision()?;
        self.refill_tiles(player_id);
        Ok(())
    }

    /// `FUN_141615270` after player `owner`'s change of the farms' food (from `before`, by
    /// `amount`): with attribute 31 at least 0 (`COMISS`, `JC`: not a NaN), each object the tracker
    /// files under the farm masters (`DAT_142f925d0`'s keys: the build's `farm`, `rice_farm`,
    /// `pasture` and `pasture_mangrove`, 50, 1187, 1897 and 1893; their done objects, in that
    /// order, [`World::tracked_key`]) from type 40 on with an action (its Make) has the food still
    /// to make (`+0x44`) grow: with `held` its food (`+0x310`) and `food = held + still to make`,
    /// it takes `(food / before)^exponent × amount + food − held` (`powf`, here libm's).
    pub(super) fn farm_food(
        &mut self,
        owner: PlayerId,
        before: f32,
        amount: f32,
    ) -> GenieResult<()> {
        let exponent = self
            .attribute_of(owner, self.model.slots.attribute.farm_exponent)
            .get();
        if below_or_unordered(exponent, 0.0) {
            return Ok(());
        }
        let mut ids = Vec::new();
        let master_slots = self.model.slots.master;
        for farm in [
            master_slots.farm,
            master_slots.rice_farm,
            master_slots.pasture,
            master_slots.pasture_mangrove,
        ] {
            ids.extend(self.tracked_key(owner, farm)?);
        }
        for id in ids {
            let Some((player_id, unit)) = self.object(id) else {
                continue;
            };
            if unit.master(self.master_tables(), player_id)?.kind < MasterKind::ACTION {
                continue;
            }
            let held = unit.life.amount;
            let Some(make) = self.parts.actions_mut(id)?.list.make_mut() else {
                continue;
            };
            let food = (held + make.remaining).get();
            let grown = libm::powf(food / before, exponent);
            make.remaining = Amount::new((grown * amount + food) - held.get());
        }
        Ok(())
    }
}
