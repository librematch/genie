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
use crate::world::slots::{CheckedIndex, PlayerSlots};
use crate::{GenieError, GenieResult};

impl World {
    /// Slot 0x1b0 (`FUN_141620c70`) of player `p` on resource `attribute` with `amount`: every
    /// live combat unit's and building's volleys lose the shares of the units inside, the
    /// attribute changes (`FUN_141661160`: `change` sets it, slot 0xf0
    /// [`World::set_player_attribute`]; or adds to it, slot 0xf8 with no statistics,
    /// [`World::add_player_attribute`]; the local player's statistics are not simulation), and
    /// the volleys take the shares again. A change of resource 0x20 would not run in the screen's
    /// mode 4 (a playback's is not) or with the game's `+0x668` set (not held: taken unset).
    pub(super) fn modify_resource(
        &mut self,
        p: PlayerId,
        attribute: AttributeId,
        amount: Amount,
        change: ResourceChange,
    ) -> GenieResult<()> {
        self.all_garrison_volleys(p, EffectHook::Before)?;
        match change {
            ResourceChange::Set => self.set_player_attribute(p, attribute, amount)?,
            ResourceChange::Add => self.add_player_attribute(p, attribute, amount)?,
        }
        self.all_garrison_volleys(p, EffectHook::After)
    }

    /// Slot 0x1b8 (`FUN_141620e30`) of player `p` on resource `attribute` by `v`: as
    /// [`World::modify_resource`], the attribute set to its value times `v` (`FUN_1416614c0`).
    pub(super) fn multiply_resource(
        &mut self,
        p: PlayerId,
        attribute: AttributeId,
        v: f32,
    ) -> GenieResult<()> {
        self.all_garrison_volleys(p, EffectHook::Before)?;
        let value = self.attribute_of(p, attribute) * v;
        self.set_player_attribute(p, attribute, value)?;
        self.all_garrison_volleys(p, EffectHook::After)
    }

    /// `FUN_141217ac0` ([`EffectHook::Before`]) or `FUN_14120b910` on each of player `p`'s live
    /// combat units and buildings ([`World::garrison_volleys`]).
    pub(super) fn all_garrison_volleys(
        &mut self,
        p: PlayerId,
        hook: EffectHook,
    ) -> GenieResult<()> {
        let ids: Vec<ObjectId> = self
            .players
            .player(p)?
            .objects
            .iter()
            .map(|u| u.id)
            .collect();
        for id in ids {
            let kind = self
                .object(id)
                .map(|(_, u)| u.life.master_id)
                .and_then(|m| player_master(&self.model.masters, p, m))
                .map(|m| m.kind);
            if matches!(kind, Some(MasterKind::COMBAT | MasterKind::BUILDING)) {
                self.garrison_volleys(p, id, hook)?;
            }
        }
        Ok(())
    }

    /// `FUN_1416674b0`, the attribute's store: within the player's attributes' count it takes `v`.
    /// A computer player's resource
    /// peaks (`+0x6dc`) and attribute 262's fields (`+0x628`..`+0x630`) are not held.
    pub(super) fn store_attribute(
        &mut self,
        p: PlayerId,
        attribute: AttributeId,
        v: Amount,
    ) -> GenieResult<()> {
        let player = self.players.player_mut(p)?;
        let i = attribute.index();
        if player.attributes.len() <= i {
            return Ok(());
        }
        *player.attributes.checked_mut(i)? = v;
        Ok(())
    }

    /// Slot 0xf0 (`FUN_14162d610`) of player `p`: attribute `attribute` takes `v`
    /// ([`World::store_attribute`]), with its side effects: 0x32 turns the allied vision on first
    /// ([`World::allied_vision_on`]); 0xc4 changes nothing for a player above 0 and spreads to the
    /// others at most 0 ([`AttributeRoles::SPREAD_ATTRIBUTE`]); 0xb7 rebuilds the masks after; 0x21
    /// above 0 calls the scripts' `EffectFunction<v>` (the XS runtime is not modelled).
    pub(crate) fn set_player_attribute(
        &mut self,
        p: PlayerId,
        attribute: AttributeId,
        v: Amount,
    ) -> GenieResult<()> {
        {
            let held = self.model.roles.of_attribute(attribute);
            if held.contains(AttributeRoles::ALLIED_VISION) {
                self.allied_vision_on(p)?;
            } else if held.contains(AttributeRoles::SPREAD_ATTRIBUTE) {
                if Amount::ZERO < self.attribute_of(p, self.model.slots.attribute.spread_attribute)
                {
                    return Ok(());
                }
                // the message to the players is the interface's
                for q in PlayerId::each(self.players.len()).skip(1) {
                    if q != p
                        && self.attribute_of(q, self.model.slots.attribute.spread_attribute)
                            <= Amount::ZERO
                    {
                        self.store_attribute(q, self.model.slots.attribute.spread_attribute, v)?;
                    }
                }
            } else if held.contains(AttributeRoles::EFFECT_FUNCTION) && (Amount::ZERO < v) {
                return Err(GenieError::unsupported(
                    "an effect's script function (attribute 0x21, EffectFunction in Effects.xs)",
                ));
            }
        }
        self.store_attribute(p, attribute, v)?;
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

    /// Slot 0xf8 (`FUN_141616820`) of player `p` with no statistics (`param_4` 0): `v` adds to
    /// attribute `attribute` ([`crate::world::Player::add_attribute`]), with the side effects of
    /// 0x32 (the allied vision on first), 0xb7 (the masks rebuilt first, with the old value) and
    /// 0xc4 (nothing for a player above 0; the others at most 0 add it too).
    pub(crate) fn add_player_attribute(
        &mut self,
        p: PlayerId,
        attribute: AttributeId,
        v: Amount,
    ) -> GenieResult<()> {
        {
            let held = self.model.roles.of_attribute(attribute);
            if held.contains(AttributeRoles::ALLIED_VISION) {
                self.allied_vision_on(p)?;
            } else if held.contains(AttributeRoles::SHARED_VISION) {
                self.rebuild_vision()?;
            } else if held.contains(AttributeRoles::SPREAD_ATTRIBUTE) {
                if Amount::ZERO < self.attribute_of(p, self.model.slots.attribute.spread_attribute)
                {
                    return Ok(());
                }
                for q in PlayerId::each(self.players.len()).skip(1) {
                    let theirs = self.attribute_of(q, self.model.slots.attribute.spread_attribute);
                    if q != p && theirs <= Amount::ZERO {
                        self.store_attribute(
                            q,
                            self.model.slots.attribute.spread_attribute,
                            v + theirs,
                        )?;
                    }
                }
            } else {
                return self.players.player_mut(p)?.add_attribute(attribute, v);
            }
        }
        // FUN_14165d040's store of the sum
        let sum = v + self.attribute_of(p, attribute);
        self.store_attribute(p, attribute, sum)?;
        Ok(())
    }

    /// Attribute 0x32's side effect: player `p`'s allied vision (`+0x2cc`) on, the masks rebuilt
    /// (the world's slot 0x78, `FUN_1415e92b0`) and its tile list refilled
    /// ([`World::refill_tiles`], `FUN_141285cb0`).
    pub(super) fn allied_vision_on(&mut self, p: PlayerId) -> GenieResult<()> {
        self.players
            .player_mut(p)?
            .ally_sharing
            .set(AllySharing::VISION, true);
        self.rebuild_vision()?;
        self.refill_tiles(p);
        Ok(())
    }

    /// `FUN_141615270` after player `p`'s change of the farms' food (from `before`, by `amount`):
    /// with attribute 31 at least 0 (`COMISS`, `JC`: not a NaN), each object the tracker files
    /// under the farm masters (`DAT_142f925d0`'s keys: the build's `farm`, `rice_farm`,
    /// `pasture` and `pasture_mangrove`, 50, 1187, 1897 and 1893; their done objects, in that
    /// order, [`World::tracked_key`]) from type 40 on with an action (its Make) has the food still to make (`+0x44`) grow: with `held` its food
    /// (`+0x310`) and `f = held + still to make`, it takes `(f / before)^exponent × amount + f −
    /// held` (`powf`, here libm's).
    pub(super) fn farm_food(&mut self, p: PlayerId, before: f32, amount: f32) -> GenieResult<()> {
        let exponent = self
            .attribute_of(p, self.model.slots.attribute.farm_exponent)
            .get();
        if below_or_unordered(exponent, 0.0) {
            return Ok(());
        }
        let mut ids = Vec::new();
        let s = self.model.slots.master;
        for farm in [s.farm, s.rice_farm, s.pasture, s.pasture_mangrove] {
            ids.extend(self.tracked_key(p, farm)?);
        }
        for id in ids {
            let Some((q, u)) = self.object(id) else {
                continue;
            };
            if u.master(self.master_tables(), q)?.kind < MasterKind::ACTION {
                continue;
            }
            let held = u.life.amount;
            let Some(make) = self.parts.actions_mut(id)?.list.make_mut() else {
                continue;
            };
            let f = (held + make.remaining).get();
            let grown = libm::powf(f / before, exponent);
            make.remaining = Amount::new((grown * amount + f) - held.get());
        }
        Ok(())
    }
}
