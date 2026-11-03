//! Treason (GAME's sub-command 0xa, the old Spies): a player with a castle pays gold to have its
//! allies see the kings of the players it is not allied with, a flare at each king for each of
//! them.
//!
//! 158041: the castle test `FUN_14160ffb0`; the treason `FUN_141620400`; the flare is the
//! player's slot 0x1c0 with master 697 ([`World::remains`]).

use crate::GenieError;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::tracker::ListKey;
use crate::unit_ai::task_finder::allied;
use crate::world::World;
use crate::world::layer::MasterRoles;
use crate::world::slots::{CheckedIndex, PlayerSlots};

impl World {
    /// `FUN_14160ffb0`: player `p` has a castle (0x52; 0xd1 with the loaded world's content mode
    /// `+0x6b4` at 1, which a load leaves at the game's, [`World::mode_680`]): a master it has
    /// (`FUN_140b26630`) with at least one in its tracker's done list (`FUN_14104eea0`).
    #[must_use]
    pub fn has_treason_building(&self, p: PlayerId) -> bool {
        let castle = if self.mode_680.is_one() {
            self.model.slots.master.castle_mode_1
        } else {
            self.model.slots.master.castle
        };
        self.model
            .masters
            .get(p.index())
            .is_some_and(|ms| ms.get(castle).is_some())
            && self
                .players
                .get(p.index())
                .is_some_and(|pl| !pl.tracker.list(ListKey::master(castle)).is_empty())
    }

    /// `FUN_141620400`: player `p`'s treason. It costs 400 gold, 200 with attribute 197 above 0
    /// (`COMISS`, `JBE`: a NaN costs 400); with less gold (`COMISS`, `JA`: a NaN gold passes)
    /// or outside the regicide rule ([`crate::world::settings::Settings::regicide`]) it fails with the
    /// interface's error. Else the gold goes straight off attribute 3 (the local player's
    /// statistics and the event 0x91 are not simulation); then for each player from 1 not the
    /// player's own or allied (`FUN_141663dc0` without allies, [`crate::unit_ai::task_finder::allied`]) its
    /// first king in its live list (`FUN_141166ab0`), or what the king is inside, gives each
    /// player from 1 the player counts as its own or allied a flare at its place (that player's
    /// slot 0x1c0 with master 697, [`World::remains`]); the sound and the minimap's mark
    /// (`FUN_140c7d6c0`, the game's `+0xa78`) are the interface's, as the local player's message.
    pub(crate) fn treason(&mut self, p: PlayerId) -> Result<(), GenieError> {
        let discount = self
            .players
            .player(p)?
            .attribute(self.model.slots.attribute.treason_discount)
            .unwrap_or(Amount::ZERO);
        let cost = if Amount::ZERO < discount {
            self.model.tuning.treason.treason_discount_cost
        } else {
            self.model.tuning.treason.treason_cost
        };
        let gold = *self
            .players
            .player(p)?
            .attributes
            .checked(self.model.slots.attribute.gold.index())?;
        if cost > gold.get() || !self.settings.regicide() {
            return Ok(());
        }
        *self
            .players
            .player_mut(p)?
            .attributes
            .checked_mut(self.model.slots.attribute.gold.index())? = gold - Amount::new(cost);
        let n = self.players.len();
        for i in PlayerId::each(n).skip(1) {
            if allied(&self.players.player(p)?.relations, p, i, false) {
                continue;
            }
            let Some(king) = self
                .players
                .player(i)?
                .objects
                .iter()
                // The king (`0x1b2`), whom the treason reveals.
                .find(|u| {
                    self.model
                        .roles
                        .of_master(u.life.master_id)
                        .contains(MasterRoles::KING)
                })
            else {
                continue;
            };
            let at = match king.life.inside.and_then(|h| self.object(h)) {
                Some((_, holder)) => holder.position(),
                None => king.position(),
            };
            for j in PlayerId::each(n).skip(1) {
                if allied(&self.players.player(p)?.relations, p, j, false) {
                    self.remains(j, self.model.slots.master.flare_4, at, None)?;
                    self.made_new_object(j)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garrison::ContainerId;
    use crate::map::Position;
    use crate::objects::ObjectId;
    #[cfg(test)]
    use crate::objects::layers::Static;
    use crate::objects::lifecycle::Lifecycle;
    #[cfg(test)]
    use crate::objects::masters::MasterKind;
    #[cfg(test)]
    use crate::objects::masters::{Master, Masters};
    use crate::objects::unit::Unit;
    #[cfg(test)]
    use crate::players::relation_row;
    use crate::test_ids::object_id;
    use crate::testing::attribute;
    use crate::testing::ids;
    use crate::testing::ids::CASTLE;
    use crate::testing::ids::KING;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;
    #[cfg(test)]
    use crate::world::{FrameSeed, GameMode};
    use std::sync::Arc;

    /// Gaia and three players; player 1 holds 500 gold and is allied with 3.
    fn world() -> World {
        let player = |p: usize| Player {
            attributes: vec![Amount::ZERO; 300],
            relations: relation_row(
                (0..4).map(|q| u8::from(!(q == p || (p, q) == (1, 3) || (p, q) == (3, 1))) * 3),
            ),
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 4],
            (0..4).map(player).collect(),
        );
        t.players[1].attributes[attribute::GOLD.index()] = Amount::new(500.0);
        t
    }

    /// Without the regicide rule, or with too little gold, nothing happens.
    #[test]
    fn treason_needs_the_regicide_rule_and_the_gold() {
        let mut t = world();
        t.treason(PlayerId::new(1)).unwrap();
        assert_eq!(
            t.players[1].attributes[attribute::GOLD.index()],
            Amount::new(500.0)
        );
        t.settings.game_mode = GameMode::of_data(1);
        t.players[1].attributes[attribute::GOLD.index()] = Amount::new(399.0);
        t.treason(PlayerId::new(1)).unwrap();
        assert_eq!(
            t.players[1].attributes[attribute::GOLD.index()],
            Amount::new(399.0)
        );
        t.players[1].attributes[attribute::TREASON_DISCOUNT.index()] = Amount::new(1.0);
        t.treason(PlayerId::new(1)).unwrap();
        assert_eq!(
            t.players[1].attributes[attribute::GOLD.index()],
            Amount::new(199.0)
        );
    }

    /// The treason pays 400 gold; without a king anywhere it flares nothing.
    #[test]
    fn treason_without_kings_only_pays() {
        let mut t = world();
        t.settings.game_mode = GameMode::of_data(1);
        let before = t.players[3].objects.len();
        t.treason(PlayerId::new(1)).unwrap();
        assert_eq!(
            t.players[1].attributes[attribute::GOLD.index()],
            Amount::new(100.0)
        );
        assert_eq!(t.players[3].objects.len(), before);
    }

    /// The flare's master (697, animated, no hit points) for every player.
    fn with_flares(t: &mut World) {
        let flare = Master {
            id: ids::FLARE_4,
            kind: MasterKind::ANIMATED,
            standing_sprite: None,
            ..Master::default()
        };
        t.model.masters = (0..t.players.len())
            .map(|_| Masters::new([flare.clone()]))
            .map(Arc::new)
            .collect();
    }

    fn king(id: i32, position: Position, inside: Option<ObjectId>) -> Unit {
        Unit {
            id: object_id(id),
            place: Static::at(position),
            life: Lifecycle {
                master_id: KING,
                inside: inside.map(ContainerId::new),
                ..Lifecycle::default()
            },
            ..Unit::default()
        }
    }

    /// Each king of a player not allied with player 1 (2's, the first in its list) gives
    /// player 1 and its ally 3 a flare at its place, or at the building it is inside; an
    /// allied player's king (3's) gives none.
    #[test]
    fn treason_flares_the_kings_for_the_player_and_its_allies() {
        let mut t = world();
        t.settings.game_mode = GameMode::of_data(1);
        with_flares(&mut t);
        t.next_object_id = object_id(100);
        t.players[2].objects = vec![
            king(20, Position::new(5.0, 6.0, 0.0), None),
            king(21, Position::new(9.0, 9.0, 0.0), None),
        ]
        .into();
        t.players[3].objects = vec![king(30, Position::new(1.0, 1.0, 0.0), None)].into();
        t.reindex();
        t.treason(PlayerId::new(1)).unwrap();
        let flares = |t: &World, p: usize| -> Vec<(i32, Position)> {
            t.players[p]
                .objects
                .iter()
                .filter(|u| u.life.master_id == ids::FLARE_4)
                .map(|u| (u.id.get(), u.position()))
                .collect()
        };
        assert_eq!(flares(&t, 1), [(100, Position::new(5.0, 6.0, 0.0))]);
        assert_eq!(flares(&t, 3), [(101, Position::new(5.0, 6.0, 0.0))]);
        assert_eq!(flares(&t, 2), []);
        // a king inside a building: the flares go to the building's place
        let mut t = world();
        t.settings.game_mode = GameMode::of_data(1);
        with_flares(&mut t);
        let mut castle = king(40, Position::new(12.0, 13.0, 0.0), None);
        castle.life.master_id = CASTLE;
        t.players[2].objects = vec![
            king(20, Position::new(5.0, 6.0, 0.0), Some(object_id(40))),
            castle,
        ]
        .into();
        t.reindex();
        t.treason(PlayerId::new(1)).unwrap();
        assert_eq!(flares(&t, 1)[0].1, Position::new(12.0, 13.0, 0.0));
    }

    /// The castle test asks the player's masters and its tracker.
    #[test]
    fn player_without_castle_has_no_treason() {
        let t = world();
        assert!(!t.has_treason_building(PlayerId::new(1)));
    }
}
