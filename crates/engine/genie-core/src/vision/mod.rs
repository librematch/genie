//! What each player sees: the visible maps its objects' sight keeps, the vision masks that share
//! sight among allies, the objects-in-sight registry, the fog's ghosts and the newly explored
//! tiles.
//!
//! This file holds the players' vision masks (`player+0x2a8`, visible, and `+0x2a0`, explored): the
//! bits of the unified map a player's tile tests read (`FUN_141663500`, `FUN_140c68a00`). Each
//! holds the player's own bit and one no tile has (29; explored 61), bits 30 and 62 once the player
//! won or lost, and the bits of the players whose sight it shares: its mutual allies while its
//! allied vision is on (`allied_LOS_Enable`, `+0x2cc`, saved; attribute 50 turns it on) and every
//! player while its attribute 183 is not 0. The world rebuilds them all (`FUN_141552df0`, from its
//! slot 0x78 `FUN_1415e92b0`) at a load and when one of those changes.
//!
//! 158041: the rebuild `FUN_141552df0`; the attribute setter `FUN_141616820` (attributes 50, 183).

use crate::GenieError;
use crate::objects::masters::MasterId;
use crate::objects::masters::ids::TOWN_CENTRE;
use crate::players::Amount;
use crate::players::ByPlayer;
use crate::players::PlayerId;
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

pub(crate) mod explored;
pub mod ghost;
pub mod los_tables;
pub mod registry;
mod sight_tiles;
mod tile_count;
pub mod visibility;
mod vision_bits;

pub use self::sight_tiles::SightTiles;
pub use self::tile_count::TileCount;
pub use self::vision_bits::{VisionBits, VisionMasks};

/// The attribute whose change turns the allied vision on (`FUN_141616820`'s case 0x32).
pub const REVEAL_ALLY: usize = 50;
/// The attribute that shares every player's sight while it is not 0.
pub const REVEAL_ALL: usize = 183;
/// The attribute that reveals the other players' town centres (`FUN_141552df0`): a flare of
/// master 697 (FLARE4, slot 0x1c0) at each of their town centres (master 109, `FUN_141165d30`),
/// each player's once, recorded in `mTCFlared` ([`crate::world::Player::tc_flared`]).
const REVEAL_CENTRES: usize = 209;
const FLARE4: MasterId = MasterId::named(697);
/// The attributes that add a scenario's masks of visible and explored players, not modelled.
const SHARED_MASKS: [usize; 2] = [285, 286];

/// The masks of player `p` before any sharing: its own bits and the bits no tile has, with the
/// ended ones for a player who won or lost.
#[must_use]
pub fn own_masks(p: PlayerId, ended: bool) -> VisionMasks {
    let mut masks = VisionMasks {
        visible: VisionBits::visible(p) | VisionBits::NO_TILE,
        explored: VisionBits::explored(p) | VisionBits::NO_TILE_EXPLORED,
    };
    if ended {
        masks.visible |= VisionBits::ENDED;
        masks.explored |= VisionBits::ENDED_EXPLORED;
    }
    masks
}

impl World {
    /// `FUN_141552df0`: every player's masks again. A player's own bits first (the ended ones for
    /// a player who won or lost); then for every player past Gaia, with its allied vision on, the
    /// bits of each other player past Gaia it and who both count each other as allies (relation
    /// 0), and with attribute 183 not 0 the bits of every player past Gaia. The players' share
    /// flags (`+0x290`) and the third parties' table (`+0x2b0`, `+0x2b8`) are not held.
    pub fn rebuild_vision(&mut self) -> Result<(), GenieError> {
        let n = self.players.len();
        let mut masks: ByPlayer<VisionMasks> = PlayerId::each(self.players.len())
            .zip(&self.players)
            .map(|(p, player)| own_masks(p, player.game_status.ended()))
            .collect();
        let mut flares = Vec::new();
        for (p, me) in PlayerId::each(self.players.len())
            .zip(&self.players)
            .skip(1)
        {
            let attribute = |i: usize| me.attributes.get(i).copied().unwrap_or(Amount::ZERO);
            let allies = |q: PlayerId| {
                me.relations.get(q) == Some(&crate::players::Relation::ALLY)
                    && self.players.player(q).ok().and_then(|o| o.relations.get(p))
                        == Some(&crate::players::Relation::ALLY)
            };
            let mine = masks.checked_mut(p)?;
            if me.allied_vision {
                for q in PlayerId::each(n).skip(1) {
                    if q != p && allies(q) {
                        mine.visible |= VisionBits::visible(q);
                        mine.explored |= VisionBits::explored(q);
                    }
                }
            }
            if attribute(REVEAL_ALL) != Amount::ZERO {
                for q in PlayerId::each(n).skip(1) {
                    mine.visible |= VisionBits::visible(q);
                    mine.explored |= VisionBits::explored(q);
                }
            }
            // attribute 209 does not touch the masks: it flares the town centres of the players
            // past Gaia it and who do not both count each other as allies, each player's once
            // (`mTCFlared`), unless the screen's mode is 4 (`FUN_140c87f20`, the editor's: a
            // playback has no such screen, the probe reads 0)
            if REVEAL_CENTRES < me.attributes.len() && Amount::ZERO < attribute(REVEAL_CENTRES) {
                for q in PlayerId::each(n).skip(1) {
                    if allies(q) || me.tc_flared.get(q.index()).copied().unwrap_or(false) {
                        continue;
                    }
                    flares.extend(
                        self.players
                            .player(q)?
                            .objects
                            .iter()
                            // The town centre the reveal finds (`0x6d`) and the flare it makes there (`0x2b9`).
                            .filter(|u| u.life.master_id == TOWN_CENTRE)
                            .map(|u| (p, q, u.position())),
                    );
                }
            }
            if SHARED_MASKS[1] < me.attributes.len()
                && SHARED_MASKS.iter().any(|&i| attribute(i) != Amount::ZERO)
            {
                return Err(GenieError::unsupported(
                    "a scenario's shared vision masks (attributes 285, 286)",
                ));
            }
        }
        self.spatial.vision = masks;
        // the flares (slot 0x1c0 with master 697, [`World::remains`]), in the players' order, each
        // town centre in its owner's list order; each flared player is recorded
        for (p, q, at) in flares {
            self.remains(p, FLARE4, at, None)?;
            self.made_new_object(p)?;
            let flared = &mut self.players.player_mut(p)?.tc_flared;
            if flared.len() <= q.index() {
                flared.resize(q.index() + 1, false);
            }
            *flared.checked_mut(q.index())? = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Position;
    use crate::objects::ObjectState;
    use crate::players::Relation;
    use crate::test_ids::object_id;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;

    /// The masks of the visible and explored bits `visible` and `explored`.
    fn masks(visible: u64, explored: u64) -> VisionMasks {
        VisionMasks {
            visible: VisionBits::from_bits_retain(visible),
            explored: VisionBits::from_bits_retain(explored),
        }
    }

    /// Gaia and two teams of two (players 1 and 3 against 2 and 4), every player's allied
    /// vision on: each sees with its team's bits, as records/425906646's players do after the
    /// game's header load (`0xaa` for 1, 3, 5, 7 in its 4v4).
    fn teams() -> World {
        let player = |relations: Vec<u8>| Player {
            relations: crate::players::relation_row(relations),
            allied_vision: true,
            attributes: vec![Amount::ZERO; 250],
            ..Player::default()
        };
        World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 5],
            vec![
                Player::default(),
                player(vec![0, 1, 3, 0, 3]),
                player(vec![0, 3, 1, 3, 0]),
                player(vec![0, 0, 3, 1, 3]),
                player(vec![0, 3, 0, 3, 1]),
            ],
        )
    }

    #[test]
    fn mutual_allies_share_their_sight() {
        let mut t = teams();
        t.rebuild_vision().unwrap();
        assert_eq!(
            t.spatial.vision.get(PlayerId::new(0)),
            Some(&own_masks(PlayerId::new(0), false))
        );
        assert_eq!(
            t.spatial.vision.get(PlayerId::new(1)),
            Some(&masks(0x2000_000a, 0x2000_000a_0000_0000))
        );
        assert_eq!(
            t.spatial.vision.get(PlayerId::new(2)),
            Some(&masks(0x2000_0014, 0x2000_0014_0000_0000))
        );
    }

    #[test]
    fn an_ally_one_way_or_the_vision_off_shares_nothing() {
        let mut t = teams();
        t.players[3]
            .relations
            .set(PlayerId::new(1), Relation::ENEMY);
        t.players[2].allied_vision = false;
        t.rebuild_vision().unwrap();
        assert_eq!(
            t.spatial.vision.get(PlayerId::new(1)),
            Some(&own_masks(PlayerId::new(1), false))
        );
        assert_eq!(
            t.spatial.vision.get(PlayerId::new(2)),
            Some(&own_masks(PlayerId::new(2), false))
        );
        assert_eq!(
            t.spatial.vision.get(PlayerId::new(4)),
            Some(&masks(0x2000_0014, 0x2000_0014_0000_0000))
        );
    }

    #[test]
    fn attribute_183_shares_every_players_sight() {
        let mut t = teams();
        t.players[2].attributes[REVEAL_ALL] = Amount::new(1.0);
        t.rebuild_vision().unwrap();
        assert_eq!(
            t.spatial.vision.get(PlayerId::new(2)),
            Some(&masks(0x2000_001e, 0x2000_001e_0000_0000))
        );
    }

    #[test]
    fn a_player_alone_sees_with_its_own_bits() {
        assert_eq!(
            own_masks(PlayerId::new(2), false),
            masks(0x2000_0004, 0x2000_0004_0000_0000)
        );
        assert_eq!(
            own_masks(PlayerId::new(1), true),
            masks(0x6000_0002, 0x6000_0002_0000_0000)
        );
    }

    /// Attribute 209 (reveal the enemies' town centres) does not touch the masks: it leaves them
    /// exactly the allies' sharing, and without a town centre to flare records no player.
    #[test]
    fn attribute_209_leaves_the_masks_and_flares_only_town_centres() {
        let mut t = teams();
        t.players[2].attributes[REVEAL_CENTRES] = Amount::new(5.0);
        t.rebuild_vision().unwrap();
        assert_eq!(
            t.spatial.vision.get(PlayerId::new(2)),
            Some(&masks(0x2000_0014, 0x2000_0014_0000_0000))
        );
        assert!(!t.players[2].tc_flared.iter().any(|&f| f));
    }

    /// `World::update` ends with the world's slot 0x78 (`FUN_1415e92b0`, at `0x1415e9266`) after
    /// every update: a town centre that comes up later is flared in the update it is there, once
    /// (430876787's flare 12577 in update 2630, whose id the game's later objects count on).
    #[test]
    fn every_update_ends_with_the_masks_rebuilt_and_new_town_centres_flared() {
        let mut t = teams();
        t.players[2].attributes[REVEAL_CENTRES] = Amount::new(5.0);
        let flare = crate::objects::masters::Master {
            id: FLARE4,
            kind: crate::objects::masters::master_type::ANIMATED,
            standing_sprite: None,
            ..crate::objects::masters::Master::default()
        };
        t.masters = (0..t.players.len())
            .map(|_| crate::objects::masters::Masters::new([flare.clone()]))
            .collect();
        t.next_object_id = object_id(100);
        let flares = |t: &World| -> Vec<(i32, Position)> {
            t.players[2]
                .objects
                .iter()
                .filter(|u| u.life.master_id == FLARE4)
                .map(|u| (u.id.get(), u.position()))
                .collect()
        };
        t.tick_with(Millis::new(50), &[]);
        assert_eq!(flares(&t), []);
        // the masks are the rebuilt ones even where they were not
        assert_eq!(
            t.spatial.vision.get(PlayerId::new(2)),
            Some(&masks(0x2000_0014, 0x2000_0014_0000_0000))
        );
        let _ = t
            .push_live(
                PlayerId::new(1),
                crate::objects::unit::Unit {
                    id: object_id(50),
                    place: crate::objects::layers::Static::at(Position::new(7.0, 8.0, 0.0)),
                    life: crate::objects::lifecycle::Lifecycle {
                        master_id: TOWN_CENTRE,
                        state: ObjectState::Alive,
                        ..crate::objects::lifecycle::Lifecycle::default()
                    },
                    ..crate::objects::unit::Unit::default()
                },
            )
            .unwrap();
        t.tick_with(Millis::new(50), &[]);
        assert_eq!(flares(&t), [(100, Position::new(7.0, 8.0, 0.0))]);
        assert_eq!(t.players[2].tc_flared.get(1), Some(&true));
        t.tick_with(Millis::new(50), &[]);
        assert_eq!(flares(&t).len(), 1);
    }
}
