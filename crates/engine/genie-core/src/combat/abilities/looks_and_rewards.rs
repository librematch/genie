//! The terrain look tasks (action type 159) and the removal reward tasks (161).

use crate::combat::abilities::{Look, ONE, Reward};
use crate::map::TerrainFlags;
use crate::objects::ListIndex;
use crate::objects::SpriteId;
use crate::objects::masters::CostSlot;
use crate::objects::masters::Task;
use crate::objects::masters::TaskTerrain;
use crate::players::PlayerId;
use crate::world::World;

/// A terrain look task's terrain mask (`0x141207501`, a `std::bitset<8>`): a terrain below 0
/// is the mask of its negation's low byte, 1 to 8 sets the bit of that less 1, above 8 none.
/// None for 0: the bit's `set` throws (`out_of_range`, `FUN_14120b5d0`), which the game does not
/// catch; no task of the `.dat` has terrain 0.
#[must_use]
pub const fn terrain_mask(terrain: TaskTerrain) -> Option<TerrainFlags> {
    let terrain = terrain.get();
    match terrain {
        t if t < 0 => Some(TerrainFlags::from_bits_retain(
            t.wrapping_neg().to_le_bytes()[0],
        )),
        1..=8 => Some(TerrainFlags::from_bits_retain(1 << (terrain - 1))),
        0 => None,
        _ => Some(TerrainFlags::NONE),
    }
}

/// The look a terrain look task gives on a tile whose terrain's flag byte (the terrain table's
/// `+0x2`, [`crate::map::terrain::TerrainKind::flags`]) has a bit of its mask (`0x14120754e`): its
/// sprites, each a null sprite where the world's sprite table (`world+0xc0`) has none
/// (`exists`): the task's ids, −1 for none, its load resolved, and its sound event ids
/// (`FUN_141533c70`: from 0 to the table's count, and not null). A task's id below −1 would
/// read before the table at its load; no `.dat` task has one. None off its terrains.
#[must_use]
pub fn look(
    task: &Task,
    mask: TerrainFlags,
    flags: TerrainFlags,
    exists: impl Fn(i32) -> bool,
) -> Option<Look> {
    if !flags.intersects(mask) {
        return None;
    }
    let sprite = |id: i32| {
        i16::try_from(id)
            .ok()
            .and_then(SpriteId::of_data)
            .filter(|_| exists(id))
    };
    let task_sprite = |id: Option<SpriteId>| id.and_then(|id| sprite(id.get().into()));
    let sprites = task.sprites;
    Some(Look {
        standing: task_sprite(sprites.carrying),
        walking: task_sprite(sprites.moving),
        attacking: task_sprite(sprites.working),
        running: task_sprite(sprites.proceeding),
        // the two sound events' ids, read as sprite ids
        dying: sprite(task.sound_events.gathering.get()),
        undead: sprite(task.sound_events.deposit.get()),
    })
}

/// A removal reward task's command (`FUN_14120b5f0`): the task's `work_val1`, times the player's
/// attribute its `attribute_type2` names when that is not negative and the player has it
/// (`FUN_1416625b0`: `multiplier`); with any flag of the task's `combat_level_flag`, the object's
/// master cost (the player's master of its id, its three entries at `+0x880`: resource, amount,
/// paid) of the task's `attribute_type3` decides instead: the amount of the last paid entry of that
/// resource times the rate (`CVTDQ2PS`, `MULSS`), 0 without one. The amount is truncated
/// (`CVTTSS2SI`).
#[must_use]
pub fn reward(
    task: &Task,
    player: PlayerId,
    multiplier: Option<crate::players::Amount>,
    cost: &[CostSlot],
) -> Reward {
    let out = task.attribute_type3;
    let mut rate = task.work_val1.get();
    if task.attribute_type2.is_some()
        && let Some(m) = multiplier
    {
        rate *= m.get();
    }
    let mut amount = rate;
    if !task.combat_level_flag.is_empty() {
        amount = 0.0;
        for (resource, count, paid) in crate::objects::masters::cost_entries(cost) {
            if Some(resource) == out && paid {
                amount = count.as_f32() * rate;
            }
        }
    }
    Reward {
        player,
        attribute: out,
        amount: crate::math::conversions::cvttss2si(amount),
    }
}

impl World {
    /// A terrain look task of the object at `(p, i)` (`0x141207501`): its walk's distance factor
    /// (`+0x878`, [`crate::movement::Mover::factor`]) and its attack timer's rate (`+0x87c`) go
    /// back to 1 and its look to its master's, then on a tile of the task's terrains
    /// ([`terrain_mask`], [`look`]) take the task's `work_val1`, `work_val2` and look. The tile
    /// is the one under where the object last moved to (slot 0x520: `+0x824`, `+0x82c`,
    /// truncated; a building's place); without the map's terrain no tile matches. False when
    /// the task's terrain mask throws ([`terrain_mask`]): the routine ends.
    pub(super) fn terrain_look(&mut self, (p, i): (usize, ListIndex), task: &Task) -> bool {
        let Some(mask) = terrain_mask(task.terrain) else {
            return false;
        };
        let unit = &self.players[p].objects[i];
        let at = unit
            .body
            .mover()
            .map_or_else(|| unit.position(), |m| m.moved_at);
        let flags = self
            .spatial
            .terrain
            .as_ref()
            .map_or(TerrainFlags::NONE, |t| {
                t.kind(at.ground().tile())
                    .and_then(|k| t.terrains.get(k.index()))
                    .map_or(TerrainFlags::NONE, |k| k.flags)
            });
        let graphics = &self.graphics;
        let exists = |id: i32| {
            i16::try_from(id)
                .ok()
                .and_then(SpriteId::of_data)
                .is_some_and(|id| graphics.contains_key(&id))
        };
        let look = look(task, mask, flags, exists);
        let unit = &mut self.players[p].objects[i];
        let (factor, rate) = if look.is_some() {
            (task.work_val1.get(), task.work_val2.get())
        } else {
            (ONE, ONE)
        };
        if let Some(m) = unit.body.mover_mut() {
            m.factor = factor;
        }
        unit.abilities.attack_rate = rate;
        unit.abilities.look = look;
        true
    }
}
