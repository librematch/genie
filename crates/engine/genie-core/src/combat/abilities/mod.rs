//! A combat object's passive abilities: `FUN_1412072f0`, which `TRIBE_Combat_Object::update`
//! (`0x141208440`) runs on every combat unit and building (types 70 and 80) after
//! `RGE_Combat_Object::update`, with what that one returned (the object leaves its list); a
//! building's update (`TRIBE_Building_Object::update`) runs its production after it. The routine
//! walks the object's master's task list (`+0x788`) in order and acts on four action types
//! (`+0xe`):
//!
//! - 151 ([`GENERATE`]): the player's attribute named by the task's `attribute_type3` (`+0x24`)
//!   grows by the world's dt times the task's `work_val1` times the player's attribute named by
//!   its `attribute_type2` (`+0x22`), through the player's store of what it made and did not give
//!   yet (`player+0x658`, [`crate::world::Player::generated`]): once that reaches 1 it is given
//!   (the player's slot 0xf8) and starts again at 0. A combat unit (or building) with the task's
//!   `combat_level_flag` bit 1 (bit 2) generates always; else a combat unit whose top action
//!   works (its state 7) on a target of the task's class, and a built building. The Feitoria,
//!   the castles, the King of the Hill monument, the Keshik and some villagers have it.
//! - 152 ([`MOVEMENT_DAMAGE`]): while it moves, a combat unit hits the enemies about it on a
//!   timer (`+0x9a4`, `mMovementDamageTimer`): each on the tiles within the task's `work_range`
//!   that is not its player's ally nor Gaia's and is close enough loses the task's `work_val1`
//!   hit points (its slot 0xb0), the timer takes the task's `work_val2` and the hit's player
//!   hears of it (its slot 0x2e8 with 0x201); one left below 1 hit point has 0.
//! - 159 ([`TERRAIN_LOOK`]): the object takes the task's look while it stands on a terrain of
//!   the task's ([`terrain_mask`]): its own sprites (`+0x8a9` set, `+0x960`..`+0x988`, [`Look`]),
//!   which its sprite slots then show ([`crate::objects::sprite::standing`] and the others), its walk's
//!   distance factor (`+0x878`, the task's `work_val1`) and its attack timer's rate (`+0x87c`,
//!   its `work_val2`); off it, its master's and 1.
//! - 161 ([`REMOVAL_REWARD`], `FUN_14120b5f0`): a dead object that leaves its list issues its
//!   player an `ADD_ATTRIBUTE` command for each such task within 1e-7 of range ([`Reward`]).
//!
//! The `.dat` of build 158041 has only 151 tasks (its current build adds 159 for the longboats,
//! Nessie and the snake), and the civilisations' bonuses add more to their gatherers' masters,
//! which the save then holds (`mObjectTaskFlags` bit 0): the Portuguese foragers' wood, the
//! Poles' stone miners' gold, the Shu lumberjacks' food. The routine runs whatever the masters
//! hold.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::math::fmath;
use crate::objects::ActionState;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::masters::ActionType;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::masters::{Task, player_master};
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::PlayerSlots;
use genie_diff::Diff;

mod generation;
mod looks_and_rewards;
mod movement_damage;

pub use self::generation::{TopWork, WorkTarget, generated, generates, store};
pub use self::looks_and_rewards::{look, reward, terrain_mask};
pub use self::movement_damage::{Bystander, Hitter, finish, movement_damage_due, reach, struck};
use crate::combat::attack::ATTACK;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::HUNT;
use crate::math::image::abilities::{EPSILON as ABILITIES_EPSILON, ONE as ABILITIES_ONE};
use crate::objects::masters::action_type::COMBAT as ACTION_TYPE_COMBAT;
use crate::world::index::Place;

/// The action type of a generation task (151): the Feitoria's trickle and the like.
pub const GENERATE: ActionType = ActionType::named(0x97);
/// The action type of a movement damage task (152).
pub const MOVEMENT_DAMAGE: ActionType = ActionType::named(0x98);
/// The action type of a terrain look task (159).
pub const TERRAIN_LOOK: ActionType = ActionType::named(0x9f);
/// The action type of a removal reward task (161).
pub const REMOVAL_REWARD: ActionType = ActionType::named(0xa1);

/// The top action types a generation task's test passes (`FUN_140a7be70`): the Attack (9),
/// type 7, the Gather (5) and the Hunt (0x6e).
const WORKING_ACTIONS: [ActionType; 4] = [ATTACK, ACTION_TYPE_COMBAT, GATHER, HUNT];
/// The action state of an action at work (`+0x18`).
const WORKING: ActionState = ActionState::new(7);

/// The building classes a movement damage hits (`0x141207ae8`, decoded 0x33 and 0x36); it hits
/// any object of another type.
const STRUCK_BUILDING_CLASSES: [ClassId; 2] = [class::CLASS_51, class::CLASS_54];
/// The blast defence from which an object is reached whatever the hitter's level (`+0x62b`).
const ALWAYS_REACHED: BlastDefense = BlastDefense::ALWAYS_REACHED;

/// Hit points below which an object counts dead, and the store's give threshold
/// (`DAT_14330eb8c`).
const ONE: f32 = ABILITIES_ONE;
/// The range below which a removal reward task pays (`DAT_142eb0c60`).
const EPSILON: f32 = ABILITIES_EPSILON;

/// What the routine keeps on its object. The constructor's values are the default: no timer,
/// rate 1 and the master's look.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Abilities {
    /// `+0x9a4`, `mMovementDamageTimer` (saved): the seconds before a movement damage task hits
    /// again.
    pub movement_damage_timer: Seconds,
    /// `+0x87c`: the rate the attack timer (`+0x91c`) runs down at (`RGE_Combat_Object::update`,
    /// `0x14107e28e`). Not saved: the constructor's 1 until the first update's abilities.
    #[diff(unsaved)]
    pub attack_rate: f32,
    /// `+0x8a9` set, with the sprites at `+0x960`..`+0x988`: a terrain look task's look; None
    /// while the object shows its master's (not saved: the constructor clears `+0x8a9`).
    #[diff(unsaved)]
    pub look: Option<Look>,
}

impl Default for Abilities {
    fn default() -> Self {
        Self {
            movement_damage_timer: Seconds::ZERO,
            attack_rate: 1.0,
            look: None,
        }
    }
}

/// The sprites a terrain look task gives its object (graphic ids; None for a null sprite), which
/// the object's sprite slots show while `+0x8a9` is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Diff)]
pub struct Look {
    /// `+0x960`, the task's carrying sprite (`+0x58`): slot 0x798's standing sprite.
    pub standing: Option<SpriteId>,
    /// `+0x968`, the task's moving sprite (`+0x40`): slot 0x7c8's walking sprite.
    pub walking: Option<SpriteId>,
    /// `+0x970`, the task's working sprite (`+0x50`): slot 0x898's attack sprite.
    pub attacking: Option<SpriteId>,
    /// `+0x978`, the task's proceeding sprite (`+0x48`): slot 0x7d0's running sprite.
    pub running: Option<SpriteId>,
    /// `+0x980`, the sprite the task's gathering sound event id names (`+0x60`): slot 0x7a0's
    /// dying sprite.
    pub dying: Option<SpriteId>,
    /// `+0x988`, the sprite its deposit sound event id names (`+0x78`): slot 0x7a8's undead
    /// sprite.
    pub undead: Option<SpriteId>,
}

/// The `ADD_ATTRIBUTE` command a removal reward task issues (`FUN_14120b5f0`: the player's slot
/// 0x360, `FUN_141616500` -> `FUN_140e618e0`, command type 5 into the command queue,
/// `FUN_140e6e180`): the attribute (its low byte goes into the command) and the amount, which
/// the command carries as a float.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reward {
    /// The player the command is for (`player+0x718`).
    pub player: PlayerId,
    /// The task's `attribute_type3` (`+0x24`), none for −1.
    pub attribute: Option<AttributeId>,
    /// The amount, truncated (`CVTTSS2SI`).
    pub amount: i32,
}

/// The routine's test of the object's hit points (`COMISS 1.0, hp` / `JBE`): at least 1 (or a
/// NaN) takes the living path, else the dead one.
#[must_use]
pub fn alive(hp: HitPoints) -> bool {
    fmath::at_most_or_unordered(ONE, hp.get())
}

/// The action types the routine acts on.
pub const ACTIONS: [ActionType; 4] = [GENERATE, MOVEMENT_DAMAGE, TERRAIN_LOOK, REMOVAL_REWARD];

/// What the routine reads of the object's master (`+0xa0`: its own one, else its player's).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Own {
    /// The master's id (`+0x58`).
    id: MasterId,
    /// Its type (`+0x8`).
    kind: MasterKind,
    /// Its `area_effect_level` (`+0x830`).
    blast_level: BlastLevel,
    /// Its radii (`+0x5d8`, `+0x5dc`).
    radius: Footprint,
}

impl World {
    /// `FUN_1412072f0` on the object `id`, the one at `i` of player `p`'s list, as player
    /// `owner`'s (its owner after its update: the new one when the update handed it over),
    /// once its combat update returned `leaves`. Only its master's tasks of the four types
    /// count ([`ACTIONS`]); an object of a master of another type than 70 or 80, or without one
    /// the slice has, has none. On a living object ([`alive`]) they act in their order
    /// ([`World::generate`], [`World::movement_damage`], [`World::terrain_look`]); on a dead one
    /// that leaves, its removal reward tasks within 1e-7 of range (`COMISS`/`JBE`: a NaN is not)
    /// issue their commands, which are returned: in a recording's replay the recording's own
    /// commands bring them.
    ///
    /// # Errors
    ///
    /// What the slice does not model of a task's action ([`World::generate`],
    /// [`World::movement_damage`]).
    pub(crate) fn abilities(
        &mut self,
        (id, p, i): (ObjectId, PlayerId, ListIndex),
        owner: PlayerId,
        leaves: bool,
    ) -> Result<Vec<Reward>, GenieError> {
        let i = match self.players.player(p)?.objects.get(i) {
            Some(u) if u.id == id => i,
            _ => match self.place(id) {
                Some(Place::Live(q, j)) if q == p => j,
                _ => return Ok(Vec::new()),
            },
        };
        let unit = &self.players.live(p, i)?;
        let master = unit.master(self.master_tables(), p)?;
        // The object types the routine runs on: a combat unit (`'F'`) and a building (`'P'`).
        if !matches!(master.kind, COMBAT | BUILDING) {
            return Ok(Vec::new());
        }
        let tasks: Vec<Task> = master
            .tasks
            .iter()
            .filter(|t| ACTIONS.contains(&t.action_type))
            .copied()
            .collect();
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        let own = Own {
            id: master.id,
            kind: master.kind,
            blast_level: master.blast_level,
            radius: master.radius,
        };
        let mut rewards = Vec::new();
        if !alive(unit.life.hp) {
            if !leaves {
                return Ok(rewards);
            }
            for task in &tasks {
                if task.action_type == REMOVAL_REWARD && task.work_range < Distance::new(EPSILON) {
                    let held = self.players.player(owner)?;
                    let multiplier = (task.attribute_type2).and_then(|a| held.attribute(a));
                    let cost = player_master(&self.masters, owner, own.id)
                        .map_or(&[][..], |m| &m.cost[..]);
                    rewards.push(reward(task, owner, multiplier, cost));
                }
            }
            return Ok(rewards);
        }
        for task in &tasks {
            match task.action_type {
                GENERATE => self.generate((p, i), owner, own, task)?,
                MOVEMENT_DAMAGE => self.movement_damage((p, i), owner, own, task)?,
                // the terrain mask's throw ends the game
                TERRAIN_LOOK if !self.terrain_look((p, i), task)? => return Ok(rewards),
                _ => {}
            }
        }
        Ok(rewards)
    }
}

#[cfg(test)]
mod tests;
