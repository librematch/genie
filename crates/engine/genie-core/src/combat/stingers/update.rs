//! The stinger update (`FUN_1412092e0`): the sums `+0xb38`..`+0xb53` rebuilt from the object's
//! active stingers in each update.

use crate::combat::AttackBonus;
use crate::combat::ClassOrMaster;
use crate::combat::damage::ArmorBonuses;
use crate::combat::powerup::{PowerupAttribute, armor_of_class};
use crate::combat::stingers::ActiveStinger;
use crate::math::conversions::cvttss2si;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::Regeneration;
use crate::objects::layers::Combat;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::DamageClass;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::TaskFlags;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};
use std::collections::BTreeSet;

/// What the stinger update reads of its object besides its combat layer: its master, whose
/// values a scaled stinger takes a share of, and its shown attack (slot 0x878,
/// [`crate::world::World::shown_attack_of`]), which only a scaled attack stinger reads (None
/// when no active stinger is one).
#[derive(Clone, Copy, Debug)]
pub(crate) struct StingerReads<'model> {
    /// Its master.
    pub(crate) master: &'model Master,
    /// Its shown attack, for a scaled attack stinger.
    pub(crate) shown_attack: Option<DamageAmount>,
}

impl World {
    /// The stinger update ([`update_stingers`]) of the combat object in `slot` (an object of
    /// type 70 or 80, after its powerup update), with its master and, when a scaled attack
    /// stinger needs it, its shown attack ([`World::shown_attack_of`]).
    ///
    /// # Errors
    ///
    /// A master its player has not got, or what its shown attack does not model.
    pub(crate) fn object_stingers(
        &mut self,
        slot: (PlayerId, ListIndex, ObjectId),
    ) -> GenieResult<()> {
        let Some((player, index)) = self.live_slot(slot) else {
            return Ok(());
        };
        let unit = self.players.live(player, index)?;
        let id = unit.id;
        let master = unit.master(self.master_tables(), player)?;
        if !matches!(master.kind, MasterKind::COMBAT | MasterKind::BUILDING) {
            return Ok(());
        }
        // the master's copy only for an object with stingers (the update's reads borrow it
        // beside the combat layer)
        let stingers = &self.parts.combat_of(id).active_stingers;
        let master = if stingers.is_empty() {
            None
        } else {
            Some(master.clone())
        };
        let scaled_attack = stingers.iter().any(|stinger| {
            stinger.kind == PowerupAttribute::ATTACK
                && stinger.flags.contains(TaskFlags::STINGER_SCALED)
        });
        let shown_attack = if scaled_attack {
            Some(self.shown_attack_of(id)?)
        } else {
            None
        };
        let now = self.world_time;
        let combat = self.parts.combat_mut(id)?;
        let Some(master) = master.as_ref() else {
            zero_stingers(combat);
            return Ok(());
        };
        update_stingers(
            combat,
            StingerReads {
                master,
                shown_attack,
            },
            now,
        )
    }
}

/// A scaled attack stinger the caller read no shown attack for.
const NO_SHOWN_ATTACK: GenieError =
    GenieError::unreachable("a scaled attack stinger without its object's shown attack");

/// `FUN_1412092e0`, the stinger update of a combat object (`TRIBE_Combat_Object::update` after
/// the powerup update): its sums (`+0xb38`..`+0xb53`) go to 0, then each active stinger in
/// turn (`+0xb58`) that has not ended (`now` below its end, unsigned) adds to its kind's sum
/// ([`add_stinger`]), and an ended one goes. A [`TaskFlags::STINGER_UNIQUE`] one whose kind
/// and key an earlier active one of the pass had goes too (a local `unordered_set` of the
/// pairs, which every active one joins; only its membership counts). The loop's index advances
/// past an erase as well, so the stinger that moved into the erased one's place waits for the
/// next pass. An active one's sprite (`+0x18`) goes to `+0x9f8`, the renderer's.
///
/// # Errors
///
/// A scaled attack stinger without `reads`' shown attack.
pub(crate) fn update_stingers(
    combat: &mut Combat,
    reads: StingerReads<'_>,
    now: WorldTime,
) -> GenieResult<()> {
    zero_stingers(combat);
    let mut counted: BTreeSet<(PowerupAttribute, ClassOrMaster)> = BTreeSet::new();
    let mut index = 0;
    while let Some(&stinger) = combat.active_stingers.get(index) {
        let pair = (stinger.kind, stinger.key);
        let gone = stinger.end <= now
            || (stinger.flags.contains(TaskFlags::STINGER_UNIQUE) && counted.contains(&pair));
        if gone {
            let _ = combat.active_stingers.remove(index);
        } else {
            let _ = counted.insert(pair);
            add_stinger(combat, &stinger, reads)?;
        }
        index += 1;
    }
    Ok(())
}

/// The stinger update's start (`FUN_1412092e0`): the sums `+0xb38`..`+0xb53` go to 0, with or
/// without active stingers.
const fn zero_stingers(combat: &mut Combat) {
    combat.speed_stinger = Speed::ZERO;
    combat.regen_stinger = Regeneration::ZERO;
    combat.reload_stinger = Seconds::ZERO;
    combat.hp_stinger = HitPoints::ZERO;
    combat.attack_stinger = AttackBonus::ZERO;
    combat.armor_stingers = ArmorBonuses::ZERO;
}

/// The stinger update's switch on an active stinger's kind (`+0x8`): its value, with
/// [`TaskFlags::STINGER_SCALED`] times its master's value, adds to (`ADDSS`, the value first):
/// - the hit points (0) `+0xb44`, scaled by the master's hit points;
/// - the speed (5) `+0xb38`, by the master's speed (slot 0xe8);
/// - the attack (9) `+0xb48`, by the shown attack (slot 0x878's 16 bits), unscaled its floor
///   (`floorf`);
/// - the reload time (10) `+0xb40`, by the master's reload (`+0x838`);
/// - the regeneration (109) `+0xb3c`, by the master's regeneration (`+0x8b4`);
/// - the melee and pierce armor (116, 117) `+0xb4c` and `+0xb50` as whole numbers (`CVTTSS2SI`,
///   a 32-bit wrapping `ADD`), by the master's armor of class 4 or 3 ([`armor_of_class`]);
/// - the regeneration (120) `+0xb3c` only unscaled: the master's hit points times the value.
///
/// Any other kind adds nothing (the switch's default, and 120 scaled).
///
/// # Errors
///
/// A scaled attack stinger without `reads`' shown attack.
fn add_stinger(
    combat: &mut Combat,
    stinger: &ActiveStinger,
    reads: StingerReads<'_>,
) -> GenieResult<()> {
    let master = reads.master;
    let scaled = stinger.flags.contains(TaskFlags::STINGER_SCALED);
    let value = stinger.value.get();
    let hit_points = f32::from(master.hit_points.get());
    let whole_armor = |class: DamageClass| {
        if scaled {
            cvttss2si(f32::from(armor_of_class(master, class).get()) * value)
        } else {
            cvttss2si(value)
        }
    };
    match stinger.kind {
        PowerupAttribute::HIT_POINTS => {
            let add = if scaled { hit_points * value } else { value };
            combat.hp_stinger = HitPoints::new(add + combat.hp_stinger.get());
        }
        PowerupAttribute::MOVEMENT_SPEED => {
            let add = if scaled {
                master.animated_part().max_speed.get() * value
            } else {
                value
            };
            combat.speed_stinger = Speed::new(add + combat.speed_stinger.get());
        }
        PowerupAttribute::ATTACK => {
            let add = if scaled {
                f32::from(reads.shown_attack.ok_or(NO_SHOWN_ATTACK)?.get()) * value
            } else {
                libm::floorf(value)
            };
            combat.attack_stinger = AttackBonus::new(add + combat.attack_stinger.get());
        }
        PowerupAttribute::ATTACK_RELOAD_TIME => {
            let add = if scaled {
                value * master.combat_part().reload.get()
            } else {
                value
            };
            combat.reload_stinger = Seconds::new(add + combat.reload_stinger.get());
        }
        PowerupAttribute::REGENERATION_RATE => {
            let add = if scaled {
                value * master.creatable_part().regeneration.get()
            } else {
                value
            };
            combat.regen_stinger = Regeneration::new(add + combat.regen_stinger.get());
        }
        PowerupAttribute::MELEE_ARMOR => {
            let armors = &mut combat.armor_stingers;
            armors.melee = armors.melee.wrapping_add(whole_armor(DamageClass::MELEE));
        }
        PowerupAttribute::PIERCE_ARMOR => {
            let armors = &mut combat.armor_stingers;
            armors.pierce = armors.pierce.wrapping_add(whole_armor(DamageClass::PIERCE));
        }
        PowerupAttribute::REGENERATION_HP_PERCENT if !scaled => {
            combat.regen_stinger =
                Regeneration::new(hit_points * value + combat.regen_stinger.get());
        }
        // the switch's default, and the scaled 120: nothing
        _ => {}
    }
    Ok(())
}
