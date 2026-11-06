//! The garrisons after a load ([`garrison_firepower`]): each unit inside a building of its own
//! in that building's list, its share in the building's volleys, its count in the building's.

use crate::GenieResult;
use crate::combat::Firepower;
use crate::garrison::GarrisonCounts;
use crate::garrison::{attack_rate, share as garrison_share};
use crate::objects::ObjectId;
use crate::objects::masters::MasterKind;
use crate::world::Unread;
use crate::world::World;
use crate::world::load::world::stop_all;
use crate::world::parts::Part;

/// The garrisons of the running game: each object inside another of its player is in that one's
/// list (`+0x2f8`), in the order the objects are listed (the game's is the order they came in,
/// which no save holds); each unit (type 70) added its share to a combat object's volleys at
/// its enter (`FUN_14120b790`, [`share`](crate::garrison::share)), which the load does not restore
/// (`FUN_14121ec90` sets the master's volley fire); and a building counts its villagers and
/// raiders inside, which the save does not hold (its constructor's 0). The game's re-link
/// (`RGE_Static_Object::rehook`) enters each unit again (slot 0x1a0 with 1): the building's
/// slot 0x1c0 counts it by its saved countdown. A share the world does not model leaves the
/// building unmodelled.
pub fn garrison_firepower(turns: &mut World) {
    for index in 0..turns.players.len() {
        let entered = entered_units(turns, index);
        file_garrisons(turns, index, entered);
    }
}

/// A unit inside a building of its player, which the load's re-link enters again.
struct Entered {
    /// The building.
    building: ObjectId,
    /// The unit.
    unit: ObjectId,
    /// The unit's share of the building's volleys: none for a unit that is not of type 70 or a
    /// building whose master the table lacks.
    share: Option<GenieResult<Firepower>>,
    /// What a building (type 80) counts of the unit ([`GarrisonCounts::of_unit`]): none for
    /// another container, or a unit or a building whose master the table lacks.
    counts: Option<GarrisonCounts>,
}

/// Player `player_index`'s units inside a building of its own, in list order, each with its
/// building, its share and its counts ([`garrison_firepower`]).
fn entered_units(turns: &World, player_index: usize) -> Vec<Entered> {
    let (Some(masters), Some(player)) = (
        turns.model.masters.get(player_index),
        turns.players.get(player_index),
    ) else {
        return Vec::new();
    };
    let lists = || player.objects.iter().chain(&player.sleeping);
    let mut entered = Vec::new();
    for listed in lists() {
        let Some(building) = listed.life.inside else {
            continue;
        };
        let Some(unit) = lists().find(|unit| unit.id == building.id()) else {
            continue;
        };
        let me = masters.get(listed.life.master_id);
        let bm = masters.get(unit.life.master_id);
        let share = me
            .filter(|master| master.kind == MasterKind::COMBAT)
            .and_then(|me| {
                let rate = attack_rate(
                    bm?,
                    masters,
                    turns.parts.combat_of(unit.id).reload_powerup,
                    &turns.model.roles,
                    &turns.model.tuning.garrison,
                );
                Some(garrison_share(
                    me,
                    masters,
                    turns.parts.combat_of(listed.id).reload_powerup,
                    rate,
                    &turns.model.roles,
                ))
            });
        let counts = bm
            .filter(|master| master.kind == MasterKind::BUILDING)
            .and(me)
            .map(|me| {
                let countdown = turns.parts.combat_of(listed.id).raider_countdown;
                GarrisonCounts::of_unit(me, countdown, &turns.model.roles)
            });
        entered.push(Entered {
            building: unit.id,
            unit: listed.id,
            share,
            counts,
        });
    }
    entered
}

/// Each building of player `player_index` lists the units `entered` inside it, adds their shares
/// to its volleys and counts them in; a share the world does not model leaves the building
/// unmodelled.
fn file_garrisons(turns: &mut World, player_index: usize, entered: Vec<Entered>) {
    for Entered {
        building: object_id,
        unit: id,
        share,
        counts,
    } in entered
    {
        let unmodelled = match share {
            Some(Ok(share)) => {
                let mut combat = turns.parts.combat_of(object_id).clone();
                combat.firepower += share;
                turns
                    .set_part(object_id, Part::Combat(Box::new(combat)))
                    .err()
            }
            Some(Err(error)) => Some(error),
            None => None,
        };
        let Some(building) = turns.players.get_mut(player_index).and_then(|player| {
            player
                .objects
                .iter_mut()
                .chain(&mut player.sleeping)
                .find(|unit| unit.id == object_id)
        }) else {
            continue;
        };
        building.garrison.push(id);
        if let Some(counts) = counts {
            // the building is in its player's lists: its layer is the table's to give
            match turns.parts.building_mut(object_id) {
                Ok(layer) => layer.garrison_counts.count_in(counts),
                Err(error) => stop_all(turns, &error),
            }
        }
        if let Some(why) = unmodelled {
            let unread = Part::Unread(Some(Unread::new(why)));
            // the world holds the building (it is in its player's lists): the write cannot fail
            if let Err(error) = turns.set_part(object_id, unread) {
                stop_all(turns, &error);
            }
        }
    }
}
