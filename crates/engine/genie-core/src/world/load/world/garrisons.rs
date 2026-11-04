//! The garrisons' firepower after a load ([`garrison_firepower`]): each unit inside a building
//! of its own in that building's list, its share in the building's volleys.

use crate::GenieResult;
use crate::combat::Firepower;
use crate::garrison::{attack_rate, share as garrison_share};
use crate::objects::ObjectId;
use crate::objects::masters::MasterKind;
use crate::world::Unread;
use crate::world::World;
use crate::world::load::world::stop_all;
use crate::world::parts::Part;

/// The garrisons of the running game: each object inside another of its player is in that one's
/// list (`+0x2f8`), in the order the objects are listed (the game's is the order they came in,
/// which no save holds); and each unit (type 70) added its share to a combat object's volleys at
/// its enter (`FUN_14120b790`, [`share`](crate::garrison::share)), which the load does not restore
/// (`FUN_14121ec90` sets the master's volley fire). A share the world does not model leaves the
/// building unmodelled.
pub fn garrison_firepower(turns: &mut World) {
    for index in 0..turns.players.len() {
        let entered = entered_shares(turns, index);
        file_garrisons(turns, index, entered);
    }
}

/// A unit inside a building, the building, and the unit's share of its volleys (none for a
/// unit that is not of type 70 or a building whose master the table lacks).
type Entered = (ObjectId, ObjectId, Option<GenieResult<Firepower>>);

/// Player `player_index`'s units inside a building of its own, in list order, each with its
/// building and its share ([`garrison_firepower`]).
fn entered_shares(turns: &World, player_index: usize) -> Vec<Entered> {
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
        let share = masters
            .get(listed.life.master_id)
            .filter(|master| master.kind == MasterKind::COMBAT)
            .and_then(|me| {
                let bm = masters.get(unit.life.master_id)?;
                let rate = attack_rate(
                    bm,
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
        entered.push((unit.id, listed.id, share));
    }
    entered
}

/// Each building of player `player_index` lists the units `entered` inside it and adds their shares
/// to its volleys; a share the world does not model leaves the building unmodelled.
fn file_garrisons(turns: &mut World, player_index: usize, entered: Vec<Entered>) {
    for (object_id, id, share) in entered {
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
        if let Some(why) = unmodelled {
            let unread = Part::Unread(Some(Unread::new(why)));
            // the world holds the building (it is in its player's lists): the write cannot fail
            if let Err(error) = turns.set_part(object_id, unread) {
                stop_all(turns, &error);
            }
        }
    }
}
