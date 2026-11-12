//! An object's hit point transforms (`+0xb20`..`+0xb30`, 48-byte entries that slot 277
//! `0x141206d50` fills from its master's tasks of type 158 in their order): after a loss it
//! survives (slot 22, `FUN_141205c80`) and after a gain below its most (slot 0xb8,
//! `FUN_141203be0`), the first whose test holds gives it another master (`FUN_141204450`).

use crate::combat::damage::Victim;
use crate::math::conversions::cvttss2si;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::Masters;
use crate::objects::masters::Task;
use crate::objects::masters::player_masters;
use crate::objects::unit::Unit;
use crate::orders::ActionsOnChange;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::world::World;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};
use std::mem::take;

/// The task type of a hit point transform (158, `0x9e`).
pub(crate) const HP_TRANSFORM: ActionKind = ActionKind::named(0x9e);

/// A hit point transform (an entry of `+0xb20`). The renderer's parts stay out: the sprite it
/// attaches (`+0x10`, the task's proceeding sprite, `FUN_1411a52e0`) and the sound events it
/// plays (`+0x18`..`+0x30`, slot 0x4c0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HpTransform {
    /// `+0x0`: the master it gives (the task's `work_val1`, `CVTTSS2SI`).
    into: i32,
    /// `+0x4`: the threshold (`work_val2`): above 0 it holds for hit points above it, below 0 for
    /// hit points below its size.
    threshold: f32,
    /// `+0x8`: the player's attribute that enables it (`attribute_type2`, `+0x22`; −1 none).
    enablement: Option<AttributeId>,
    /// `+0xa`: the player's attribute added to the threshold (`attribute_type3`, `+0x24`; −1
    /// none). One below −1, which the game reads before its player's attributes and then tests
    /// as a technology that must not be researched (`FUN_141626760`), fails the task's import.
    addend: Option<AttributeId>,
    /// `+0xc`: the threshold is a share of the master's hit points (the task's
    /// `combat_level_flag` `+0x15` is 1).
    scaled: bool,
}

/// Slot 277's hit point transforms of an object of `master` (`0x141206d50`): its tasks of type
/// [`HP_TRANSFORM`], in their order.
#[must_use]
pub(crate) fn hp_transforms(master: &Master) -> Vec<HpTransform> {
    master
        .action_part()
        .tasks
        .iter()
        .filter(|task| task.action_kind == HP_TRANSFORM)
        .map(HpTransform::of_task)
        .collect()
}

/// An attribute a transform reads that its player does not hold.
const NO_ATTRIBUTE: GenieError =
    GenieError::invalid_input("a hit point transform's attribute its player does not hold");

impl HpTransform {
    /// The entry slot 277 makes of `task`.
    fn of_task(task: &Task) -> Self {
        Self {
            into: cvttss2si(task.work_val1.get()),
            threshold: task.work_val2.get(),
            enablement: task.attribute_kind2,
            addend: task.attribute_kind3,
            scaled: task.combat_level_flag.bits() == 1,
        }
    }

    /// `FUN_141204450`'s test of the transform for an object of `master` at `hp` hit points of
    /// a player holding `attributes`, whose masters are `masters`: its threshold
    /// ([`HpTransform::threshold_of`]) holds for the hit points ([`Threshold::holds`]) and the
    /// player has its master ([`HpTransform::master_given`]). Then that master.
    ///
    /// # Errors
    ///
    /// An attribute its player does not hold (the game reads past its attributes).
    fn master_of<'masters>(
        &self,
        master: &Master,
        hp: f32,
        attributes: &[Amount],
        masters: &'masters Masters,
    ) -> GenieResult<Option<&'masters Master>> {
        let Some(threshold) = self.threshold_of(master, attributes)? else {
            return Ok(None);
        };
        if !threshold.holds(hp) {
            return Ok(None);
        }
        Ok(self.master_given(masters))
    }

    /// The transform's threshold for an object of `master` of a player holding `attributes`:
    /// none when its enablement fails (an attribute at most 0: `COMISS` with 0 and `JNC`, so a
    /// NaN enables it); else its threshold plus its attribute, times the master's hit points
    /// when scaled.
    ///
    /// # Errors
    ///
    /// An attribute its player does not hold (the game reads past its attributes).
    pub(crate) fn threshold_of(
        &self,
        master: &Master,
        attributes: &[Amount],
    ) -> GenieResult<Option<Threshold>> {
        let attribute = |id: AttributeId| {
            attributes
                .get(id.index())
                .map(|amount| amount.get())
                .ok_or_else(|| NO_ATTRIBUTE.with("attribute", id))
        };
        if let Some(enablement) = self.enablement
            && attribute(enablement)? <= 0.0
        {
            return Ok(None);
        }
        let mut threshold = self.threshold;
        if let Some(addend) = self.addend {
            threshold += attribute(addend)?;
        }
        if self.scaled {
            threshold *= f32::from(master.hit_points.get());
        }
        Ok(Some(Threshold(threshold)))
    }

    /// The master the transform gives, when the player's table `masters` has it
    /// (`FUN_141659d20`: from 0 to below the table's count, not null).
    pub(crate) fn master_given<'masters>(
        &self,
        masters: &'masters Masters,
    ) -> Option<&'masters Master> {
        i16::try_from(self.into)
            .ok()
            .and_then(MasterId::of_data)
            .and_then(|id| masters.get(id))
    }
}

/// A hit point transform's threshold, its attribute added and its scaling applied
/// ([`HpTransform::threshold_of`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Threshold(f32);

impl Threshold {
    /// Whether it holds for an object at `hp` hit points: above 0 and below them, or below 0
    /// with them below its size (a NaN, threshold or hit points, holds neither).
    #[must_use]
    pub(crate) fn holds(self, hp: f32) -> bool {
        let Self(threshold) = self;
        (0.0 < threshold && threshold < hp) || (threshold < 0.0 && hp < threshold.abs())
    }
}

/// A hit's victim's hit point transforms (`FUN_141204450` from slot 22 in a hit,
/// [`crate::combat::damage::lose`]): the masters the victim may take in its hit, its own first,
/// each with what the hit reads of the object with it and its transforms whose enablement holds
/// and whose master its player has, their thresholds resolved. A hit changes neither its
/// player's attributes nor its masters, so the victim's read resolves them once
/// ([`crate::combat::victim_reads::victim_of`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitTransforms {
    /// The masters, the victim's own first.
    masters: Vec<HitMaster>,
}

/// A master a hit's victim may take ([`HitTransforms`]).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HitMaster {
    /// The master.
    pub(crate) id: MasterId,
    /// What the hit reads of the object with this master.
    pub(crate) reads: Victim,
    /// Its transforms that may give a master, in their order.
    pub(crate) transforms: Vec<HitTransform>,
}

/// A transform of a [`HitMaster`]: its resolved threshold and the master it gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HitTransform {
    /// Its threshold ([`HpTransform::threshold_of`]).
    pub(crate) threshold: Threshold,
    /// The master it gives.
    pub(crate) into: MasterId,
}

impl HitTransforms {
    /// The transforms of `masters`, the victim's own first.
    pub(crate) const fn new(masters: Vec<HitMaster>) -> Self {
        Self { masters }
    }

    /// The master the first transform of master `current` whose threshold holds at `hp` hit
    /// points gives, none when none holds.
    #[must_use]
    pub(crate) fn first_holding(&self, current: MasterId, hp: HitPoints) -> Option<&HitMaster> {
        let held = self.masters.iter().find(|master| master.id == current)?;
        let into = held
            .transforms
            .iter()
            .find(|transform| transform.threshold.holds(hp.get()))?
            .into;
        self.masters.iter().find(|master| master.id == into)
    }
}

impl World {
    /// `FUN_141204450`, the hit point transforms of object `idx` of player `player` (a combat
    /// unit or building, types 70 and 80): the first of its master's transforms
    /// ([`hp_transforms`]) whose test holds gives it that master, its actions kept (slot 0x90
    /// with 1, [`World::transform`]: [`ActionsOnChange::Keep`], a building's
    /// [`World::transform_building`]). The interface's note (`FUN_140a50690`), the sprite the
    /// renderer attaches and the sounds (slot 0x4d0, `FUN_14106abd0`) are not the simulation's;
    /// slot 0x8a8's caches (slot 277) are read from the master where they are needed.
    ///
    /// # Errors
    ///
    /// An object the world does not hold live, a player without a masters table, an attribute
    /// its player does not hold, and the master change's.
    pub(crate) fn transform_by_hit_points(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
    ) -> GenieResult<()> {
        let unit = self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        let attributes = &self.players.player(player)?.attributes;
        let masters = player_masters(&self.model.masters, player)?;
        let hp = unit.life.hp.get();
        let mut into = None;
        for transform in hp_transforms(master) {
            if let Some(found) = transform.master_of(master, hp, attributes, masters)? {
                into = Some(found.id);
                break;
            }
        }
        let Some(new) = into else {
            return Ok(());
        };
        self.transform(player, idx, new, ActionsOnChange::Keep)
    }
}

impl Unit {
    /// The masters a hit's own part took ([`Victim::master_changes`]: an attacker that a
    /// retaliation or reflected damage transformed in the hit), given to the updating object
    /// (`self` with its action layer `actions`) lent to the world ([`Step::lend_at`],
    /// [`World::transform`] with [`ActionsOnChange::Keep`]), before its write-back
    /// ([`Unit::hit_back`]) gives it the hit's hit points.
    ///
    /// # Errors
    ///
    /// The lend's and the master change's.
    pub(crate) fn take_own_master_changes(
        &mut self,
        step: &mut Step<'_>,
        actions: &mut Action,
        own: &mut Victim,
    ) -> GenieResult<()> {
        let changes = take(&mut own.master_changes);
        if changes.is_empty() {
            return Ok(());
        }
        step.lend_at((self, actions), |world, player, idx| {
            changes
                .iter()
                .try_for_each(|&new| world.transform(player, idx, new, ActionsOnChange::Keep))
        })
    }
}

#[cfg(test)]
mod tests;
