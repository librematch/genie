//! A player's diplomacy change (the world player's slot 0x30, `FUN_141639690`, which GAME's
//! sub-command 0 runs): its stance toward another player (`diplomacy[i]`, `+0xe12`), the
//! relation it makes (`relation[i]`, `+0x260`) and what its queries make of that player's objects
//! (`unit_diplomacy[i]`, `+0x278`); its objects hear of it (slot 0x120: units inside a building or
//! siege tower of a player no longer allied leave it); the players' sight masks and tile lists
//! are made again; then the world's teams (`FUN_1415e23f0`) and the units the players reveal
//! (`FUN_14152bcb0`).
//!
//! 158041: slot 0x30 `FUN_141639690`; the stance `FUN_1416397d0`; the relation `FUN_141676b80`;
//! slot 0x120 of a combat object `FUN_141217120` and of a building `FUN_1411d50e0`; the teams
//! `FUN_1415e23f0`; the revealed units `FUN_14152bcb0`; the tile list's refill `FUN_141285cb0`.

use crate::GenieError;
use crate::garrison::ContainerId;
use crate::garrison::SIEGE_UNLOAD;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::math::image::diplomacy::NO_POINT as DIPLOMACY_NO_POINT;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::ids::MARKET;
use crate::players::PlayerId;
use crate::players::Relation;
use crate::players::Stance;
use crate::players::Team;
use crate::players::UnitRelation;
use crate::vision::VisionBits;
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// The reveal of a player's units of a kind to the others (its attribute 204 not 0,
/// `FUN_14152bcb0`): their per-player marks (`+0x798`, `+0x79c`) and flares are not modelled.
pub(crate) const REVEAL_UNITS: GenieError =
    GenieError::unsupported("a player's attribute 204 (FUN_14152bcb0)");

/// The attribute naming the units a player reveals to the others (204, `+0x660`).
const REVEAL_UNIT: usize = 204;

/// `FUN_141676b80`'s kind of player `q`'s objects for player `p` of relation `relation` toward
/// it: Gaia's 0, its own 1, relations 0 and 4 an ally's (2), 1 a neutral's (3), any other an
/// enemy's (4).
#[must_use]
pub fn unit_kind(p: PlayerId, q: PlayerId, relation: Relation) -> UnitRelation {
    if q == PlayerId::GAIA {
        UnitRelation::GAIA
    } else if p == q {
        UnitRelation::OWN
    } else if relation.allied(true) {
        UnitRelation::ALLY
    } else if relation == Relation::NEUTRAL {
        UnitRelation::NEUTRAL
    } else {
        UnitRelation::ENEMY
    }
}

impl World {
    /// Slot 0x30 (`FUN_141639690`) of player `p` toward player `other` with the stance of the
    /// command's relation ([`Stance::of_diplomacy`], which the caller reads: another relation
    /// does nothing); a player outside the world does nothing either. A stance other than the
    /// one held (`+0xe12` at the other's number) is set ([`World::set_stance`]); the interface's
    /// event 8 aside, the world's sight masks are made again (its slot 0x78 with 0,
    /// [`World::rebuild_vision`]), both players' tile lists empty (`FUN_141661030`) and refill
    /// with the tiles their masks reach ([`World::refill_tiles`]). Then, whether or not it
    /// changed, the world's teams ([`World::resolve_teams`]) and the revealed units
    /// ([`World::reveal_units`]).
    pub(crate) fn set_diplomacy(
        &mut self,
        p: PlayerId,
        q: PlayerId,
        stance: Stance,
    ) -> Result<(), GenieError> {
        if self.players.len() <= q.index() {
            return Ok(());
        }
        if self
            .players
            .player(p)?
            .diplomacy
            .get(q)
            .copied()
            .unwrap_or(Stance::NONE)
            != stance
        {
            self.set_stance(p, q, stance)?;
            self.rebuild_vision()?;
            if let Some(v) = self.spatial.sight.as_mut() {
                v.clear_new_tiles(p);
                v.clear_new_tiles(q);
            }
            self.refill_tiles(p);
            self.refill_tiles(q);
        }
        self.resolve_teams();
        self.reveal_units()
    }

    /// `FUN_1416397d0`: player `p`'s stance toward `q` takes `stance`; one that makes a relation
    /// ([`Stance::relation`]) sets it ([`World::set_relation`]).
    pub(crate) fn set_stance(
        &mut self,
        p: PlayerId,
        q: PlayerId,
        stance: Stance,
    ) -> Result<(), GenieError> {
        self.players.player_mut(p)?.diplomacy.set(q, stance);
        stance
            .relation()
            .map_or(Ok(()), |relation| self.set_relation(p, q, relation))
    }

    /// `FUN_141676b80`: player `p`'s relation toward `q` (`+0x260`, which the walk's and the
    /// queries' copies follow) and what its queries make of `q`'s objects (`+0x278`,
    /// [`unit_kind`]); then each of its live objects, in the list's order, hears of it (slot
    /// 0x120, [`World::notify_of_relation`]; the list's count is read again after each).
    pub(crate) fn set_relation(
        &mut self,
        p: PlayerId,
        q: PlayerId,
        relation: Relation,
    ) -> Result<(), GenieError> {
        self.players.player_mut(p)?.relations.set(q, relation);
        if let Some(row) = self.spatial.relations.get_mut(p) {
            row.set(q, relation);
        }
        if let Some(row) = self.spatial.unit_diplomacy.get_mut(p) {
            row.set(q, Some(unit_kind(p, q, relation)));
        }
        let mut i = ListIndex::FIRST;
        while self.players.player(p)?.objects.holds(i) {
            let id = self.players.live(p, i)?.id;
            self.notify_of_relation(p, id, q, relation)?;
            i = i.next();
        }
        Ok(())
    }

    /// Slot 0x120 of player `p`'s object `id`, told its player's relation toward `q` is now
    /// `relation`, by its master's type: a combat object's (type 70, `FUN_141217120`) and a
    /// building's (type 80, `FUN_1411d50e0`: the combat object's first); any other's does
    /// nothing (`FUN_141196990`, `FUN_14106d4e0`).
    ///
    /// The combat object's, with a relation other than an ally's: a unit inside an object of
    /// `q` leaves it ([`World::leave_foreign_holder`]); then a siege tower lets out the units
    /// of `q` inside it, from the last. One that finds no spot would leave it and be destroyed
    /// (slots 0x1a8 and 0xd0), which the unload's own errors stand for.
    ///
    /// The building's, with an ally's relation: a market tells a computer ally of its tile
    /// ([`World::market_tells`]). With another relation the units of `q` inside it, from the
    /// last, leave it toward no point ([`World::let_out_foreign`]).
    pub(crate) fn notify_of_relation(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        q: PlayerId,
        relation: Relation,
    ) -> Result<(), GenieError> {
        let Some((_, u)) = self.object(id) else {
            return Ok(());
        };
        let master = u.master(self.master_tables(), p)?;
        let (kind, class, master_id) = (master.kind, master.class, master.id);
        if kind != MasterKind::COMBAT && kind != MasterKind::BUILDING {
            return Ok(());
        }
        let inside = u.life.inside;
        let (position, garrison) = (u.position(), u.garrison.clone());
        if relation != Relation::ALLY {
            self.leave_foreign_holder(p, id, q, inside)?;
            // The market (master 84), whose owner's diplomacy change toward a computer player shows it to
            // that player's tile list.
            // The siege tower's class (13), whose units leave it for a player no longer allied.
            if class == class::SIEGE
                && garrison
                    .iter()
                    .any(|&g| self.object(g).is_some_and(|(gp, _)| gp == q))
            {
                return Err(SIEGE_UNLOAD);
            }
        }
        if kind != MasterKind::BUILDING {
            return Ok(());
        }
        if relation == Relation::ALLY {
            if master_id == MARKET {
                self.market_tells(p, id, q, position)?;
            }
            return Ok(());
        }
        self.let_out_foreign(p, id, q, &garrison)
    }

    /// The combat object's part, with a relation other than an ally's: a unit `id` inside an
    /// object of `q` (`inside`) leaves it, a building's toward no point (`FUN_1411e7220` with
    /// −1, −1, [`World::let_out`]; finding no spot, it is destroyed), a siege tower's (class 13)
    /// as its unload, not modelled.
    fn leave_foreign_holder(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        q: PlayerId,
        inside: Option<ContainerId>,
    ) -> Result<(), GenieError> {
        let Some(holder) = inside else {
            return Ok(());
        };
        let Some((hp, h)) = self.object(holder) else {
            return Ok(());
        };
        if hp != q {
            return Ok(());
        }
        let holder_master = h.master(self.master_tables(), hp)?;
        if holder_master.kind == MasterKind::BUILDING {
            let hi = self.housed_index(hp, holder.id())?;
            if !self.let_out(hp, hi, id, Point::from([NO_POINT; 2]), (None, true))? {
                self.destroy(p, id)?;
            }
        } else if holder_master.class == class::SIEGE {
            return Err(SIEGE_UNLOAD);
        }
        Ok(())
    }

    /// A market (84) of player `p`, now an ally of `q`: a player not a computer's tells a
    /// computer player `q` of its tile (its tile list, `FUN_14165d500`), unless the lobby's rule
    /// table (`FUN_14152fbb0`'s, key 1) says not; the table's settings of a scenario are not
    /// held (taken as absent, the rule on).
    fn market_tells(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        q: PlayerId,
        position: Position,
    ) -> Result<(), GenieError> {
        if self.players.player(p)?.kind.is_computer()
            // the game compares the other player's number with the market's id (`+0x8`,
            // FUN_1411d50e0's `param_2 != param_1[1]`): its slip, kept
            || i32::from(q.get()) == id.get()
            || !self.players.player(q)?.kind.is_computer()
        {
            return Ok(());
        }
        if let Some(v) = self.spatial.sight.as_mut() {
            let tile = position.tile();
            if let Some(list) = v.new_tiles.get_mut(q.index()) {
                list.push(tile);
            }
        }
        Ok(())
    }

    /// The building's part with a relation other than an ally's: the units of `q` inside the
    /// building `id`, from the last, leave it toward no point; one that finds no spot is
    /// destroyed.
    fn let_out_foreign(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        q: PlayerId,
        garrison: &[ObjectId],
    ) -> Result<(), GenieError> {
        let bi = self.housed_index(p, id)?;
        for &g in garrison.iter().rev() {
            if self.object(g).is_some_and(|(gp, _)| gp == q)
                && !self.let_out(p, bi, g, Point::from([NO_POINT; 2]), (None, true))?
            {
                self.destroy(q, g)?;
            }
        }
        Ok(())
    }

    /// The index of the unit `id` inside a building in player `p`'s live list.
    fn housed_index(&self, p: PlayerId, id: ObjectId) -> Result<ListIndex, GenieError> {
        self.live_index(p, id).ok_or(GenieError::unsupported(
            "a unit inside a building outside the live lists",
        ))
    }

    /// `FUN_141285cb0` on player `p`'s map: each tile of the map (rows, then columns) whose
    /// unified word has a player bit of the player's visible or explored mask joins its tile
    /// list (`FUN_14165d500`).
    pub(crate) fn refill_tiles(&mut self, p: PlayerId) {
        let masks = self.spatial.vision(p);
        if let Some(sight) = self.spatial.sight.as_mut() {
            let (w, h) = (sight.size.width(), sight.size.height());
            let rows = sight.unified.chunks(usize::try_from(w).unwrap_or(0).max(1));
            for (y, row) in (0..h).zip(rows) {
                for (x, &word) in (0..w).zip(row) {
                    // the bits the masks name players by (`0x1ffffff01ffffff`)
                    let word = word & VisionBits::OBJECT_MASK;
                    let seen = masks.visible.intersects(word) || masks.explored.intersects(word);
                    if seen && let Some(list) = sight.new_tiles.get_mut(p.index()) {
                        list.push(Tile::new(x, y));
                    }
                }
            }
        }
    }

    /// `FUN_1415e23f0`: the world's teams again, unless the options lock them (`mTeamsLocked`,
    /// [`crate::world::settings::Settings::teams_locked`]). Every player's team is 1 (alone); then each
    /// player past Gaia still alone, while teams 2 to 5 are left, takes into the next team every
    /// other player still alone it and who count each other as allies (relation 0 both ways) and
    /// who both have allied victory at 1 (`FUN_1416625a0`); with any taken, it joins that team
    /// too and the next team is one more (past 5 none is left).
    pub fn resolve_teams(&mut self) {
        if self.settings.teams_locked {
            return;
        }
        let players = PlayerId::each(self.players.len()).skip(1);
        self.teams = [Team::ALONE; 8];
        let mut team = Team::FIRST;
        for i in players.clone() {
            if Team::FIRST <= self.team(i) || team == Team::ALONE {
                continue;
            }
            let mut joined = 0;
            for j in players.clone() {
                let both = |a: PlayerId, b: PlayerId| {
                    self.players.player(a).is_ok_and(|pl| {
                        pl.relations.get(b) == Some(&Relation::ALLY) && pl.allied_victory == 1
                    })
                };
                if j != i && self.team(j) < Team::FIRST && both(i, j) && both(j, i) {
                    joined += 1;
                    self.set_team(j, team);
                }
            }
            if 0 < joined {
                self.set_team(i, team);
                team = team.next();
                if Team::LAST < team {
                    team = Team::ALONE;
                }
            }
        }
    }

    /// Player `i`'s team (`+0x984` at `i − 1`), 1 for a player past the eight.
    fn team(&self, i: PlayerId) -> Team {
        self.teams
            .get(i.index() - 1)
            .copied()
            .unwrap_or(Team::ALONE)
    }

    fn set_team(&mut self, i: PlayerId, team: Team) {
        if let Some(t) = self.teams.get_mut(i.index() - 1) {
            *t = team;
        }
    }

    /// `FUN_14152bcb0`: outside the editor's screen (mode 4, which a playback is not) each
    /// player with attribute 204 (`+0x660`, truncated, `CVTTSS2SI`) not 0 reveals its units of
    /// that kind to the others: not modelled ([`REVEAL_UNITS`]); without one nothing happens.
    pub(crate) fn reveal_units(&self) -> Result<(), GenieError> {
        let reveals = self.players.iter().any(|q| {
            q.attributes
                .get(REVEAL_UNIT)
                .is_some_and(|&v| cvttss2si(v.get()) != 0)
        });
        if reveals {
            return Err(REVEAL_UNITS);
        }
        Ok(())
    }
}

/// The point of an unload toward none (`DAT_14330ec20`, the unload's own role).
const NO_POINT: f32 = DIPLOMACY_NO_POINT;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::Amount;
    #[cfg(test)]
    use crate::players::{ByPlayer, relation_row};
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;

    /// Gaia and three players, all enemies, with the walk's and the queries' copies.
    fn world() -> World {
        let player = |p: usize| Player {
            attributes: vec![Amount::ZERO; 300],
            relations: relation_row((0..4).map(|q| if q == p { 0 } else { 3 })),
            diplomacy: ByPlayer::new(vec![Stance::NONE; 9]),
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 4],
            (0..4).map(player).collect(),
        );
        t.spatial.relations = t.players.iter().map(|p| p.relations.clone()).collect();
        t.spatial.unit_diplomacy = (0..4)
            .map(|p| {
                (0..4)
                    .map(|q| {
                        Some(unit_kind(
                            PlayerId::new(p),
                            PlayerId::new(q),
                            Relation::ENEMY,
                        ))
                    })
                    .collect()
            })
            .collect();
        t
    }

    /// `FUN_141676b80`'s kinds: Gaia's, its own, relations 0 and 4 an ally's, 1 a neutral's,
    /// 2 and 3 an enemy's.
    #[test]
    fn the_queries_kind_follows_the_relation() {
        assert_eq!(
            unit_kind(PlayerId::new(1), PlayerId::new(0), Relation::ALLY),
            UnitRelation::GAIA
        );
        assert_eq!(
            unit_kind(PlayerId::new(1), PlayerId::new(1), Relation::ENEMY),
            UnitRelation::OWN
        );
        assert_eq!(
            unit_kind(PlayerId::new(1), PlayerId::new(2), Relation::ALLY),
            UnitRelation::ALLY
        );
        assert_eq!(
            unit_kind(PlayerId::new(1), PlayerId::new(2), Relation::RELATION_4),
            UnitRelation::ALLY
        );
        assert_eq!(
            unit_kind(PlayerId::new(1), PlayerId::new(2), Relation::NEUTRAL),
            UnitRelation::NEUTRAL
        );
        assert_eq!(
            unit_kind(PlayerId::new(1), PlayerId::new(2), Relation::from(2)),
            UnitRelation::ENEMY
        );
        assert_eq!(
            unit_kind(PlayerId::new(1), PlayerId::new(2), Relation::ENEMY),
            UnitRelation::ENEMY
        );
    }

    /// Slot 0x30: an ally's stance makes player 1 an ally of 2 one way, in its stance, its
    /// relation and the walk's and queries' copies; an enemy's makes it an enemy again; a
    /// player outside the world changes nothing.
    #[test]
    fn diplomacy_change_sets_the_stance_and_the_relation() {
        let mut t = world();
        t.set_diplomacy(PlayerId::new(1), PlayerId::new(2), Stance::ALLY)
            .unwrap();
        assert_eq!(
            t.players[1].diplomacy.get(PlayerId::new(2)),
            Some(&Stance::ALLY)
        );
        assert_eq!(
            t.players[1].relations.get(PlayerId::new(2)),
            Some(&Relation::ALLY)
        );
        assert_eq!(
            t.players[2].relations.get(PlayerId::new(1)),
            Some(&Relation::ENEMY)
        );
        assert_eq!(
            t.spatial
                .relations
                .get(PlayerId::new(1))
                .and_then(|r| r.get(PlayerId::new(2))),
            Some(&Relation::ALLY)
        );
        assert_eq!(
            t.spatial
                .unit_diplomacy
                .get(PlayerId::new(1))
                .and_then(|r| r.get(PlayerId::new(2))),
            Some(&Some(UnitRelation::ALLY))
        );
        t.set_diplomacy(PlayerId::new(1), PlayerId::new(2), Stance::NEUTRAL)
            .unwrap();
        assert_eq!(
            t.players[1].diplomacy.get(PlayerId::new(2)),
            Some(&Stance::NEUTRAL)
        );
        assert_eq!(
            t.spatial
                .unit_diplomacy
                .get(PlayerId::new(1))
                .and_then(|r| r.get(PlayerId::new(2))),
            Some(&Some(UnitRelation::NEUTRAL))
        );
        t.set_diplomacy(PlayerId::new(1), PlayerId::new(2), Stance::ENEMY)
            .unwrap();
        assert_eq!(
            t.players[1].diplomacy.get(PlayerId::new(2)),
            Some(&Stance::ENEMY)
        );
        assert_eq!(
            t.players[1].relations.get(PlayerId::new(2)),
            Some(&Relation::ENEMY)
        );
        assert_eq!(
            t.spatial
                .unit_diplomacy
                .get(PlayerId::new(1))
                .and_then(|r| r.get(PlayerId::new(2))),
            Some(&Some(UnitRelation::ENEMY))
        );
        let before = t.players.clone();
        t.set_diplomacy(PlayerId::new(1), PlayerId::new(9), Stance::ALLY)
            .unwrap();
        assert_eq!(t.players, before);
    }

    /// The world's teams: two players allied both ways with allied victory at 1 form team 2;
    /// one way, or without allied victory, they stay alone; locked teams stay.
    #[test]
    fn mutual_allies_with_allied_victory_form_team() {
        let mut t = world();
        t.set_diplomacy(PlayerId::new(1), PlayerId::new(3), Stance::ALLY)
            .unwrap();
        t.set_diplomacy(PlayerId::new(3), PlayerId::new(1), Stance::ALLY)
            .unwrap();
        assert_eq!(t.teams.map(Team::get), [1; 8]);
        t.players[1].allied_victory = 1;
        t.players[3].allied_victory = 2;
        t.resolve_teams();
        assert_eq!(t.teams.map(Team::get), [1; 8]);
        t.players[3].allied_victory = 1;
        t.resolve_teams();
        assert_eq!(
            t.teams[..3].iter().map(|t| t.get()).collect::<Vec<_>>(),
            [2, 1, 2]
        );
        t.settings.teams_locked = true;
        t.players[3].allied_victory = 0;
        t.resolve_teams();
        assert_eq!(
            t.teams[..3].iter().map(|t| t.get()).collect::<Vec<_>>(),
            [2, 1, 2]
        );
        t.settings.teams_locked = false;
        t.resolve_teams();
        assert_eq!(t.teams.map(Team::get), [1; 8]);
    }

    /// Teams run out past 5: the players after the fourth team stay alone.
    #[test]
    fn the_teams_run_out_past_five() {
        let player = || Player {
            allied_victory: 1,
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 9],
            (0..9).map(|_| player()).collect(),
        );
        // pairs (1, 2), (3, 4), (5, 6), (7, 8), each allied both ways
        for (a, b) in [(1, 2), (3, 4), (5, 6), (7, 8)] {
            for (x, y) in [(a, b), (b, a)] {
                let row = &mut t.players[usize::from(x)].relations;
                *row = ByPlayer::new(vec![Relation::ENEMY; 9]);
                row.set(PlayerId::new(x), Relation::ALLY);
                row.set(PlayerId::new(y), Relation::ALLY);
            }
        }
        t.resolve_teams();
        assert_eq!(t.teams.map(Team::get), [2, 2, 3, 3, 4, 4, 5, 5]);
    }

    /// Attribute 204 truncated to 0 reveals nothing; 1 or more is not modelled.
    #[test]
    fn player_attribute_204_reveals_its_units() {
        let mut t = world();
        assert_eq!(t.reveal_units(), Ok(()));
        t.players[2].attributes[REVEAL_UNIT] = Amount::new(0.5);
        assert_eq!(t.reveal_units(), Ok(()));
        t.players[2].attributes[REVEAL_UNIT] = Amount::new(-1.0);
        assert_eq!(t.reveal_units(), Err(REVEAL_UNITS));
        assert_eq!(
            t.set_diplomacy(PlayerId::new(1), PlayerId::new(2), Stance::ALLY),
            Err(REVEAL_UNITS)
        );
    }
}
