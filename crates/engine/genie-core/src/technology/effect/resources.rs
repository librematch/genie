//! The player's resources and attributes: the modify and multiply slots, the store, the
//! setters and the side effects of allied vision and the farms' food.

use crate::GenieError;
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::objects::masters::{master_type, player_master};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::technology::ResourceChange;
use crate::technology::effect::OBJECT_GONE;
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

/// The attribute that turns the allied vision on (`FUN_141616820`'s and `FUN_14162d610`'s 0x32):
/// the player's `allied_LOS_Enable` (`+0x2cc`) takes 1, the world's masks are rebuilt (its slot
/// 0x78, [`World::rebuild_vision`]) and the player's map files every tile its masks reach as new
/// ([`World::refill_tiles`], `FUN_141285cb0`).
pub(super) const ALLIED_VISION: AttributeId = AttributeId::numbered(0x32);
/// The attribute that shares every player's sight (0xb7): the masks are rebuilt.
pub(super) const SHARED_VISION: AttributeId = AttributeId::numbered(0xb7);
/// The attribute of the town centres' flare reveal across the players (0xc4, Spies' "Treason"):
/// a player with it above 0 changes nothing; else each other player with it at most 0 takes the
/// amount too.
pub(super) const SPREAD_ATTRIBUTE: AttributeId = AttributeId::numbered(0xc4);
/// The attribute whose set above 0 calls the scripts' `EffectFunction<n>` (0x21, `FUN_14162d610`).
pub(super) const EFFECT_FUNCTION: AttributeId = AttributeId::numbered(0x21);
/// The attribute giving the exponent the farms' food grows by (31, `FUN_141615270`).
pub(super) const FARM_EXPONENT: AttributeId = AttributeId::numbered(31);
/// The masters whose objects' food a farm-food change grows (`DAT_142f925d0`'s keys: 50, 1187,
/// 1897, 1893), in that order.
pub(super) const FARMS: [MasterId; 4] = [
    MasterId::named(50),
    MasterId::named(1187),
    MasterId::named(1897),
    MasterId::named(1893),
];

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
    ) -> Result<(), GenieError> {
        self.all_garrison_volleys(p, false)?;
        match change {
            ResourceChange::Set => self.set_player_attribute(p, attribute, amount)?,
            ResourceChange::Add => self.add_player_attribute(p, attribute, amount)?,
        }
        self.all_garrison_volleys(p, true)
    }

    /// Slot 0x1b8 (`FUN_141620e30`) of player `p` on resource `attribute` by `v`: as
    /// [`World::modify_resource`], the attribute set to its value times `v` (`FUN_1416614c0`).
    pub(super) fn multiply_resource(
        &mut self,
        p: PlayerId,
        attribute: AttributeId,
        v: f32,
    ) -> Result<(), GenieError> {
        self.all_garrison_volleys(p, false)?;
        let value = self.attribute_of(p, attribute) * v;
        self.set_player_attribute(p, attribute, value)?;
        self.all_garrison_volleys(p, true)
    }

    /// `FUN_141217ac0` (`after` false) or `FUN_14120b910` on each of player `p`'s live combat
    /// units and buildings ([`World::garrison_volleys`]).
    pub(super) fn all_garrison_volleys(
        &mut self,
        p: PlayerId,
        after: bool,
    ) -> Result<(), GenieError> {
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
                .and_then(|m| player_master(&self.masters, p, m))
                .map(|m| m.kind);
            if matches!(kind, Some(master_type::COMBAT | master_type::BUILDING)) {
                self.garrison_volleys(p, id, after)?;
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
    ) -> Result<(), GenieError> {
        let player = self.players.player_mut(p)?;
        let i = attribute.index();
        if player.attributes.len() <= i {
            return Ok(());
        }
        *player.attributes.checked_mut(i)? = v;
        Ok(())
    }

    /// Slot 0xf0 (`FUN_14162d610`) of player `p`: attribute `attribute` takes `v`
    /// ([`World::store_attribute`]), with its side effects: 0x32 turns the allied vision on
    /// first ([`World::allied_vision_on`]); 0xc4 changes nothing for a player above 0 and spreads
    /// to the others at most 0 ([`SPREAD_ATTRIBUTE`]); 0xb7 rebuilds the masks after; 0x21 above 0
    /// calls the scripts' `EffectFunction<v>` (the XS runtime is not modelled).
    pub(crate) fn set_player_attribute(
        &mut self,
        p: PlayerId,
        attribute: AttributeId,
        v: Amount,
    ) -> Result<(), GenieError> {
        match attribute {
            ALLIED_VISION => self.allied_vision_on(p)?,
            SPREAD_ATTRIBUTE => {
                if Amount::ZERO < self.attribute_of(p, SPREAD_ATTRIBUTE) {
                    return Ok(());
                }
                // the message to the players is the interface's
                for q in PlayerId::each(self.players.len()).skip(1) {
                    if q != p && self.attribute_of(q, SPREAD_ATTRIBUTE) <= Amount::ZERO {
                        self.store_attribute(q, SPREAD_ATTRIBUTE, v)?;
                    }
                }
            }
            EFFECT_FUNCTION if Amount::ZERO < v => {
                return Err(GenieError::unsupported(
                    "an effect's script function (attribute 0x21, EffectFunction in Effects.xs)",
                ));
            }
            _ => {}
        }
        self.store_attribute(p, attribute, v)?;
        if attribute == SHARED_VISION {
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
    ) -> Result<(), GenieError> {
        match attribute {
            ALLIED_VISION => self.allied_vision_on(p)?,
            SHARED_VISION => self.rebuild_vision()?,
            SPREAD_ATTRIBUTE => {
                if Amount::ZERO < self.attribute_of(p, SPREAD_ATTRIBUTE) {
                    return Ok(());
                }
                for q in PlayerId::each(self.players.len()).skip(1) {
                    let theirs = self.attribute_of(q, SPREAD_ATTRIBUTE);
                    if q != p && theirs <= Amount::ZERO {
                        self.store_attribute(q, SPREAD_ATTRIBUTE, v + theirs)?;
                    }
                }
            }
            _ => {
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
    pub(super) fn allied_vision_on(&mut self, p: PlayerId) -> Result<(), GenieError> {
        self.players.player_mut(p)?.allied_vision = true;
        self.rebuild_vision()?;
        self.refill_tiles(p);
        Ok(())
    }

    /// `FUN_141615270` after player `p`'s change of the farms' food (from `before`, by `amount`):
    /// with attribute 31 at least 0 (`COMISS`, `JC`: not a NaN), each object the tracker files
    /// under the farm masters ([`FARMS`], their done objects, in that order,
    /// [`World::tracked_key`]) from type 40 on with an action (its Make) has the food still to make (`+0x44`) grow: with `held` its food
    /// (`+0x310`) and `f = held + still to make`, it takes `(f / before)^exponent × amount + f −
    /// held` (`powf`, here libm's).
    pub(super) fn farm_food(
        &mut self,
        p: PlayerId,
        before: f32,
        amount: f32,
    ) -> Result<(), GenieError> {
        let exponent = self.attribute_of(p, FARM_EXPONENT).get();
        if crate::math::fmath::below_or_unordered(exponent, 0.0) {
            return Ok(());
        }
        let mut ids = Vec::new();
        for farm in FARMS {
            ids.extend(self.tracked_key(p, farm)?);
        }
        for id in ids {
            let Some((q, u)) = self.object(id) else {
                continue;
            };
            if u.master(&self.masters, q)?.kind < master_type::ACTION {
                continue;
            }
            let (_, u) = self.object_mut(id).ok_or(OBJECT_GONE)?;
            let held = u.life.amount;
            let Some(make) = u.actions.make_mut() else {
                continue;
            };
            let f = (held + make.remaining).get();
            let grown = libm::powf(f / before, exponent);
            make.remaining = Amount::new((grown * amount + f) - held.get());
        }
        Ok(())
    }
}
