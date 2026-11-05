//! An object's hit point transforms (`+0xb20`..`+0xb30`, 48-byte entries that slot 277
//! `0x141206d50` fills from its master's tasks of type 158 in their order): after a loss it
//! survives (slot 22, `FUN_141205c80`) and after a gain below its most (slot 0xb8,
//! `FUN_141203be0`), the first whose test holds gives it another master (`FUN_141204450`).

use crate::math::conversions::cvttss2si;
use crate::objects::ListIndex;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::Task;
use crate::orders::ActionsOnChange;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::world::World;
use crate::world::player::Player;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

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

/// A player without a masters table, whose objects the world holds.
const NO_MASTERS: GenieError =
    GenieError::missing_data("a hit point transform's player without a masters table");

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
    /// `player`, whose masters are `masters`: its enablement holds (none, or the attribute not
    /// at most 0: `COMISS` with 0 and `JNC`, so a NaN does); its threshold plus its attribute,
    /// times the master's hit points when scaled, is above 0 and below the hit points, or below
    /// 0 with the hit points below its size (a NaN holds neither); and the player has its
    /// master (`FUN_141659d20`: from 0 to below the table's count, not null). Then that master.
    ///
    /// # Errors
    ///
    /// An attribute its player does not hold (the game reads past its attributes).
    fn master_of<'masters>(
        &self,
        master: &Master,
        hp: f32,
        player: &Player,
        masters: &'masters Masters,
    ) -> GenieResult<Option<&'masters Master>> {
        let attribute = |id: AttributeId| {
            player
                .attribute(id)
                .map(Amount::get)
                .ok_or_else(|| NO_ATTRIBUTE.with("attribute", id))
        };
        // COMISS with 0, JNC: a NaN enables it
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
        let holds =
            (0.0 < threshold && threshold < hp) || (threshold < 0.0 && hp < threshold.abs());
        if !holds {
            return Ok(None);
        }
        Ok(i16::try_from(self.into)
            .ok()
            .and_then(MasterId::of_data)
            .and_then(|id| masters.get(id)))
    }
}

impl World {
    /// `FUN_141204450`, the hit point transforms of object `idx` of player `player` (a combat
    /// unit or building, types 70 and 80): the first of its master's transforms
    /// ([`hp_transforms`]) whose test holds gives it that master, its actions kept (slot 0x90
    /// with 1: [`ActionsOnChange::Keep`], a building's [`World::transform_building`]). The
    /// interface's note (`FUN_140a50690`), the sprite the renderer attaches and the sounds
    /// (slot 0x4d0, `FUN_14106abd0`) are not the simulation's; slot 0x8a8's caches (slot 277)
    /// are read from the master where they are needed.
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
        let owner = self.players.player(player)?;
        let masters = self
            .model
            .masters
            .of(player)
            .ok_or_else(|| NO_MASTERS.with("player", player))?;
        let hp = unit.life.hp.get();
        let mut into = None;
        for transform in hp_transforms(master) {
            if let Some(found) = transform.master_of(master, hp, owner, masters)? {
                into = Some(found.id);
                break;
            }
        }
        let Some(new) = into else {
            return Ok(());
        };
        // slot 0x90 is the object's: a building's, else the combat unit's
        if master.kind == MasterKind::BUILDING {
            self.transform_building(player, idx, new, ActionsOnChange::Keep)
        } else {
            self.change_master(player, idx, new, ActionsOnChange::Keep)
        }
    }
}

#[cfg(test)]
mod tests;
