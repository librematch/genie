//! A building's relics and livestock inside it (`FUN_1411cb850`, `FUN_1411d6ad0`): their counts,
//! the relics its player holds and the researched technologies paid in relics.

use crate::GenieResult;
use crate::math::image::garrison::ONE_RELIC;
use crate::objects::ObjectId;
use crate::objects::masters::Master;
use crate::players::Amount;
use crate::players::Culture;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::technology::effect::Change;
use crate::technology::{EffectCommand, Party, ResearchState};
use crate::world::PlayState;
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::player::NO_ATTRIBUTE;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};
use std::sync::Arc;

/// What a building keeps count of among the units that go in and out of it (its slots 0x1c0
/// and 0x1c8, `FUN_1411cb850` and `FUN_1411d6ad0`), by the unit's own master (`+0xa0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kept {
    /// A relic (class 42): the building's `relic_count` and its player's relics held.
    Relic,
    /// Livestock (class 58): the building's `mLivestockSummation` takes its master's amount
    /// held (`+0x610`, [`Master::amount_held`]).
    Livestock(Amount),
}

impl Kept {
    /// What a building of a player of culture `culture` keeps of a unit of master `me`: nothing
    /// for a player without a culture (`+0x88` 0) or a unit of another class.
    pub(crate) fn of_unit(culture: Culture, me: &Master, roles: &Roles) -> Option<Self> {
        if !culture.is_set() {
            return None;
        }
        let held = roles.of_class(me.class);
        if held.contains(ClassRoles::RELIC) {
            Some(Self::Relic)
        } else if held.contains(ClassRoles::LIVESTOCK) {
            Some(Self::Livestock(me.amount_held()))
        } else {
            None
        }
    }
}

impl World {
    /// The building's slot 0x1c0 (`FUN_1411cb850`) for what it keeps of a unit that went into
    /// `building` (type 80) of player `bp` ([`Kept::of_unit`]): a relic counts one more in the
    /// building's `relic_count` (`+0xc20`) and in the player's relics held (attribute 7, plus
    /// `garrison::ONE_RELIC`), and the building's `last_garrison_time` (`+0xbe0`) takes the world's
    /// time (`FUN_1415300d0`); livestock adds its amount held to the building's
    /// `mLivestockSummation` (`+0xc28`), the amount first. The sound when the player then holds as
    /// many relics as the world spawned (`FUN_1408910c0` 0, attribute 7 as CVTTSS2SI equal to the
    /// world's `+0x844`) is the interface's. After a relic the caller updates every technology of
    /// the player (`FUN_1413cce70` with −1, [`World::update_dependents`]), on the world that holds
    /// the unit again.
    ///
    /// # Errors
    ///
    /// A player without attribute 7.
    pub(crate) fn keep_in(
        &mut self,
        building: ObjectId,
        bp: PlayerId,
        kept: Kept,
    ) -> GenieResult<()> {
        match kept {
            Kept::Relic => {
                let layer = self.parts.building_mut(building)?;
                layer.relics.count = layer.relics.count.plus_one();
                let relics_held = self.model.slots.attribute.relics_held;
                *self.relics_held_mut(bp, relics_held)? += Amount::new(ONE_RELIC);
                self.parts.building_mut(building)?.garrison_time = self.world_time;
            }
            Kept::Livestock(held) => {
                let layer = self.parts.building_mut(building)?;
                layer.livestock_summation = held + layer.livestock_summation;
            }
        }
        Ok(())
    }

    /// The building's slot 0x1c8 (`FUN_1411d6ad0`) for what it keeps of a unit that left
    /// `building` (type 80) of player `bp` ([`Kept::of_unit`]): a relic counts one less in the
    /// building's `relic_count` (`+0xc20`) and in the player's relics held (attribute 7, less
    /// `garrison::ONE_RELIC`), and the player's researched technologies paid in more relics than
    /// it now holds go back ([`World::withdraw_relic_techs`]); livestock takes its amount held off
    /// the building's `mLivestockSummation` (`+0xc28`).
    ///
    /// # Errors
    ///
    /// A player without attribute 7, and the withdrawal's.
    pub(crate) fn keep_out(
        &mut self,
        building: ObjectId,
        bp: PlayerId,
        kept: Kept,
    ) -> GenieResult<()> {
        match kept {
            Kept::Relic => {
                let layer = self.parts.building_mut(building)?;
                layer.relics.count = layer.relics.count.minus_one();
                let relics_held = self.model.slots.attribute.relics_held;
                *self.relics_held_mut(bp, relics_held)? -= Amount::new(ONE_RELIC);
                self.withdraw_relic_techs(bp)
            }
            Kept::Livestock(held) => {
                let layer = self.parts.building_mut(building)?;
                layer.livestock_summation -= held;
                Ok(())
            }
        }
    }

    /// The tail of a relic's leave (`FUN_1411d6ad0`) for player `bp`, unless the screen's mode is 0
    /// (`FUN_140c87f20`; a playback's is 1 to 3, as [`crate::world::Recycling`]'s pools take it)
    /// or the world is cleared ([`PlayState::Clearing`]): each technology of the player's tree
    /// (`+0x760`, its count `+0x20`) that is researched (state 3), whose first cost (`+0x2a`) is
    /// in relics held (attribute 7) and whose amount (`+0x30`, a short) is above the relics the
    /// player holds now (`COMISS`, `JBE`: a NaN holding is not below), goes back: its `.dat`
    /// effect's (`+0x3a`, [`crate::technology::Effects::get`], `FUN_140ec4510`) own adds (kind 4)
    /// are each undone by the player's slot 0x158 (`FUN_14162d5b0`: slot 0x148 with 1,
    /// [`World::modify_attribute`]) with the amount negated, in order, and it turns available
    /// (state 1).
    ///
    /// # Errors
    ///
    /// A player without attribute 7, a tree longer than the `.dat`'s, and the attribute
    /// modifier's.
    pub(crate) fn withdraw_relic_techs(&mut self, bp: PlayerId) -> GenieResult<()> {
        if self.game_state == PlayState::Clearing {
            return Ok(());
        }
        let relics_held = self.model.slots.attribute.relics_held;
        let tree = self.players.player(bp)?.techs.len();
        for index in 0..tree {
            let def = self.model.techs.checked(index)?;
            let ([cost, ..], effect_id) = (def.costs, def.effect_id);
            let owner = self.players.player(bp)?;
            let held = owner.attribute(relics_held).ok_or(NO_ATTRIBUTE)?;
            if owner.techs.checked(index)?.state == ResearchState::Researched
                && self
                    .model
                    .roles
                    .of_some_attribute(cost.attribute)
                    .contains(AttributeRoles::RELICS_HELD)
                && held < Amount::new(cost.amount.as_f32())
            {
                let effects = Arc::clone(&self.model.effects);
                if let Some(effect) = effects.get(effect_id) {
                    self.undo_adds(bp, &effect.commands)?;
                }
                self.players.player_mut(bp)?.techs.checked_mut(index)?.state =
                    ResearchState::Available;
            }
        }
        Ok(())
    }

    /// [`World::withdraw_relic_techs`]'s undoing of an effect's `commands` for player `bp`: each
    /// own add (kind 4) through slot 0x158 with its amount negated (`XORPS` with the sign).
    ///
    /// # Errors
    ///
    /// The attribute modifier's.
    fn undo_adds(&mut self, bp: PlayerId, commands: &[EffectCommand]) -> GenieResult<()> {
        for effect_command in commands {
            if let EffectCommand::Attribute {
                party: Party::Own,
                change: Change::Add,
                master,
                class,
                attribute,
                amount,
            } = *effect_command
            {
                self.modify_attribute(bp, Change::Add, master, class, -amount, attribute)?;
            }
        }
        Ok(())
    }

    /// Player `bp`'s relics held (attribute `relics_held`, 7) to change.
    ///
    /// # Errors
    ///
    /// A player without it.
    fn relics_held_mut(
        &mut self,
        bp: PlayerId,
        relics_held: AttributeId,
    ) -> GenieResult<&mut Amount> {
        self.players
            .player_mut(bp)?
            .attributes
            .get_mut(relics_held.index())
            .ok_or(NO_ATTRIBUTE)
    }
}
