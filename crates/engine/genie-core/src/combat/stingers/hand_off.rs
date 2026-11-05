//! A hit's stingers (`FUN_14107bdf0`): the attacker's stinger tasks that name its target, handed
//! to the target or to the attacker.

use crate::combat::damage::Victim;
use crate::combat::stingers::{StingerReceiver, StingerTask};
use crate::objects::ObjectId;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::MasterKind;

/// What a hit handed an object: the attacker's id and its stinger task, in the order of the
/// pushes (`FUN_141200830`, a `push_back` on `+0xb58`). Its write-back adds each as an
/// [`crate::combat::stingers::ActiveStinger`] at the world time.
pub(crate) type Handed = Vec<(ObjectId, StingerTask)>;

/// `FUN_14107bdf0`, the stingers a hit of `src` hands for its hit on `target` (`take_unit`
/// `FUN_1412108e0` and `take_building` `FUN_1411cea90` call it for an attacker of type 50 and up
/// that leaves the target at 1 hit point or more). Only an attacker whose master's combat
/// ability has [`CombatAbility::HIT_EFFECTS`], against a target whose player and its own are not
/// allied both ways (`FUN_141663d90` with the ally flag, [`crate::combat::damage::Allied`]),
/// hands any: each of its stinger tasks (its player's attribute enabling it,
/// [`Victim::stinger_tasks`]) that names the target ([`crate::combat::ClassOrMaster::names`], by
/// the target's master's class and id), in their order, to the target, or with
/// [`StingerReceiver::Attacker`] to the attacker when it is a TRIBE combat object (types 70 and
/// 80; `__RTDynamicCast`, else none).
pub(crate) fn hand_off(src: &mut Victim, target: &mut Victim) {
    if !src.source.ability.contains(CombatAbility::HIT_EFFECTS) || target.allied.both() {
        return;
    }
    let tribe = matches!(src.kind, MasterKind::COMBAT | MasterKind::BUILDING);
    let Victim {
        id,
        stinger_tasks,
        stung,
        ..
    } = src;
    for task in stinger_tasks.iter() {
        if !task.target.names(target.class, target.master_id) {
            continue;
        }
        match task.receiver {
            StingerReceiver::Target => target.stung.push((*id, *task)),
            StingerReceiver::Attacker if tribe => stung.push((*id, *task)),
            StingerReceiver::Attacker => {}
        }
    }
}
