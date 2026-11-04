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

/// Player `p`'s units inside a building of its own, in list order, each with its building and
/// its share ([`garrison_firepower`]).
fn entered_shares(turns: &World, p: usize) -> Vec<Entered> {
    let (Some(masters), Some(player)) = (turns.model.masters.get(p), turns.players.get(p)) else {
        return Vec::new();
    };
    let lists = || player.objects.iter().chain(&player.sleeping);
    let mut entered = Vec::new();
    for u in lists() {
        let Some(building) = u.life.inside else {
            continue;
        };
        let Some(b) = lists().find(|unit| unit.id == building.id()) else {
            continue;
        };
        let share = masters
            .get(u.life.master_id)
            .filter(|master| master.kind == MasterKind::COMBAT)
            .and_then(|me| {
                let bm = masters.get(b.life.master_id)?;
                let rate = attack_rate(
                    bm,
                    masters,
                    turns.parts.combat_of(b.id).reload_powerup,
                    &turns.model.roles,
                    &turns.model.tuning.garrison,
                );
                Some(garrison_share(
                    me,
                    masters,
                    turns.parts.combat_of(u.id).reload_powerup,
                    rate,
                    &turns.model.roles,
                ))
            });
        entered.push((b.id, u.id, share));
    }
    entered
}

/// Each building of player `p` lists the units `entered` inside it and adds their shares to
/// its volleys; a share the world does not model leaves the building unmodelled.
fn file_garrisons(turns: &mut World, p: usize, entered: Vec<Entered>) {
    for (object_id, id, share) in entered {
        let unmodelled = match share {
            Some(Ok(share)) => {
                let mut combat = turns.parts.combat_of(object_id).clone();
                combat.firepower += share;
                turns
                    .set_part(object_id, Part::Combat(Box::new(combat)))
                    .err()
            }
            Some(Err(e)) => Some(e),
            None => None,
        };
        let Some(building) = turns.players.get_mut(p).and_then(|player| {
            player
                .objects
                .iter_mut()
                .chain(&mut player.sleeping)
                .find(|unit| unit.id == object_id)
        }) else {
            continue;
        };
        building.garrison.push(id);
        if let Some(e) = unmodelled {
            let unread = Part::Unread(Some(Unread::new(e)));
            // the world holds the building (it is in its player's lists): the write cannot fail
            if let Err(error) = turns.set_part(object_id, unread) {
                stop_all(turns, &error);
            }
        }
    }
}
