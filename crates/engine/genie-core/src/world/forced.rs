//! The forced replay (dev only): the game's state after a tick ([`GameState`]) and the step
//! that forces it on the world.

use crate::GenieError;
use crate::combat::volley::same_members;
use crate::groups::UnitGroups;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::movement::Mover;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::lifecycle::death_parts::forget_resource;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::game_status::GameStatus;
use crate::technology::TechState;
use crate::unit_ai::UnitAi;
use crate::vision::VisionBits;
use crate::vision::visibility::VisibleMap;
use crate::world::World;
use crate::world::parts::{Object, Part};
use crate::world::slots::PlayerSlots;
use crate::world::{FrameSeed, PlayState};

/// The game's state after a tick, for the oracle's forced replay (dev only: the oracle reads it
/// from the tick oracle's saves, genie-core never reads the game): the frame seed and every
/// player's live units and resources as the game holds them.
#[derive(Clone, Debug, Default)]
pub struct GameState {
    /// `world+0x7d0`, the frame seed.
    pub frame_seed: FrameSeed,
    /// Every player's live objects.
    pub players: Vec<Vec<Object>>,
    /// Every player's food, wood, stone and gold.
    pub resources: Vec<[f32; 4]>,
    /// Every player's attributes as saved (the first four are the resources), which the oracle
    /// compares; forcing leaves them alone.
    pub attributes: Vec<Vec<f32>>,
    /// The players' pending debits and technology states.
    pub pending_debit: Vec<[Amount; 4]>,
    /// Every player's technology states.
    pub techs: Vec<Vec<TechState>>,
    /// Every player's game status and resignation (`+0x89`, `+0x8b`).
    pub statuses: Vec<(GameStatus, bool)>,
    /// The world's game state (`+0x88`).
    pub game_state: PlayState,
    /// Objects the slice modelled but got wrong this tick: they take the game's state too.
    pub resync: Vec<ObjectId>,
    /// The players' visible maps and the unified map, when the slice's differ.
    pub sight: Option<(Vec<VisibleMap>, Vec<VisionBits>)>,
    /// `next_reusable_object_id`: the fog ghosts that take reusable ids are not modelled.
    pub next_reusable_id: ObjectId,
    /// The map tiles' object lists in the game's order (`x`, `y`, ids; each tile of two objects
    /// or more), when the oracle dumped them: the order is the game's history, no save holds it.
    pub tile_lists: Option<Vec<(Tile, Vec<ObjectId>)>>,
    /// The pool of recycled missiles ([`Recycling::missiles`]), when the oracle dumped it: no
    /// save holds it.
    ///
    /// [`Recycling::missiles`]: crate::world::Recycling::missiles
    pub missile_pool: Option<Vec<(ObjectId, bool)>>,
    /// The world's pool of recycled ghosts, when the oracle dumped it.
    pub ghost_pool: Option<Vec<ObjectId>>,
    /// The world's unit groups, when the slice's differ.
    pub groups: Option<UnitGroups>,
}

impl World {
    /// The objects' inside lists in the game's order (`+0x2f8`, the tick oracle's dump): each
    /// container listed takes the order when it holds the same objects. Returns how many did.
    pub fn force_garrisons(&mut self, lists: &[(ObjectId, Vec<ObjectId>)]) -> usize {
        let mut n = 0;
        for (id, order) in lists {
            let Some(c) = self
                .players
                .iter_mut()
                .flat_map(|p| p.objects.iter_mut().chain(p.sleeping.iter_mut()))
                .find(|u| u.id == *id)
            else {
                continue;
            };
            if same_members(&c.garrison, order) {
                c.garrison.clone_from(order);
                n += 1;
            }
        }
        n
    }

    /// The forced replay's step after a tick: the frame seed is the game's, and so is every
    /// object the slice did not model (it froze, or its player stopped) or got wrong (`resync`),
    /// and the players' sight where it differs: its unit is the game's
    /// (with what the save does not hold, a hunted animal's origin and a boar's flag, kept),
    /// placed where the game has it, and it runs again from the next tick; a frozen object the
    /// game no longer holds goes. A stopped player takes the game's resources and runs again.
    /// The world's next reusable id is the game's (the fog ghosts that take them are not
    /// modelled). A stopped player's pending debit and technologies are the game's too, and so
    /// is the tiles' objects' order where the game's dump has it, and a forced object's inside
    /// list keeps the slice's order while it holds the same objects. Returns
    /// what could not be forced (an object that changed hands or masters, or
    /// that the game made and the slice did not). Not part of a replay: a dev hook to see past a
    /// freeze.
    pub fn force(&mut self, game: &GameState) -> Vec<(PlayerId, Option<ObjectId>, GenieError)> {
        let mut notes = Vec::new();
        self.force_world_values(game);
        // the objects the slice holds, sorted: forcing only takes away objects the game no
        // longer holds
        let mut known: Vec<ObjectId> = self
            .players
            .iter()
            .flat_map(|q| q.objects.iter().chain(&q.sleeping).map(|u| u.id))
            .collect();
        known.sort_unstable();
        for p in PlayerId::each(self.players.len()) {
            let Some(theirs) = game.players.get(p.index()) else {
                continue;
            };
            // an error here belongs to the player, not to an object
            let stopped = match self.force_player_values(p, game) {
                Ok(stopped) => stopped,
                Err(e) => {
                    notes.push((p, None, e));
                    continue;
                }
            };
            if let Err(e) = self.force_objects(p, theirs, (game, stopped), &mut notes) {
                notes.push((p, None, e));
            }
            for g in theirs.iter().map(|g| &g.unit) {
                if known.binary_search(&g.id).is_err() {
                    notes.push((
                        p,
                        Some(g.id),
                        GenieError::unsupported("an object the game made and the slice did not"),
                    ));
                }
            }
        }
        if let Some(lists) = game.tile_lists.as_ref() {
            // the tiles whose objects differ keep the slice's order: their objects' places
            // differ, and that is reported
            let _ = self.spatial.force_tile_lists(lists);
        }
        self.pathing.tiles_changed(self.spatial.take_changes());
        notes
    }

    /// The forced world's own values: the frame seed, the game state and the next reusable id
    /// are the game's, and so are the pools, the unit groups and the players' sight the
    /// oracle dumped.
    fn force_world_values(&mut self, game: &GameState) {
        self.frame_seed = game.frame_seed;
        self.game_state = game.game_state;
        self.recycling.next_id = game.next_reusable_id;
        if let Some(pool) = game.ghost_pool.as_ref() {
            self.recycling.ghosts.clone_from(pool);
        }
        if let Some(pool) = game.missile_pool.as_ref() {
            self.recycling.missiles.clone_from(pool);
        }
        if let Some(groups) = game.groups.as_ref() {
            self.groups.clone_from(groups);
        }
        if let (Some((maps, unified)), Some(sight)) =
            (game.sight.as_ref(), self.spatial.sight.as_mut())
        {
            sight.maps.clone_from(maps);
            sight.unified.clone_from(unified);
        }
    }

    /// Player `p`'s forced values: a stopped player takes the game's resources, pending debit
    /// and technologies and runs again; every player takes its game status and resignation, and
    /// its first unmodelled part goes. True when it had stopped.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    fn force_player_values(&mut self, p: PlayerId, game: &GameState) -> Result<bool, GenieError> {
        let player = self.players.player_mut(p)?;
        let stopped = player.stopped;
        if stopped {
            if let Some(r) = game.resources.get(p.index()) {
                for (v, r) in player.attributes.iter_mut().zip(r) {
                    *v = Amount::new(*r);
                }
            }
            if let Some(d) = game.pending_debit.get(p.index()) {
                player.pending_debit = *d;
            }
            if let Some(t) = game.techs.get(p.index()) {
                player.techs.clone_from(t);
            }
            player.stopped = false;
        }
        if let Some(&(status, resigned)) = game.statuses.get(p.index()) {
            player.game_status = status;
            player.resigned = resigned;
        }
        // each update's first unmodelled part is its own: a freeze's would hide a later stop
        player.unsupported = None;
        Ok(stopped)
    }

    /// Player `p`'s live objects against the game's (`theirs`): an object that froze (or whose
    /// player had `stopped`, or that the game's `resync` names) takes the game's
    /// ([`World::take_game_object`]); one the game no longer holds leaves the world. A dead
    /// object never freezes, but one the game resyncs takes its state too. An object that
    /// changed hands or masters stays, noted.
    fn force_objects(
        &mut self,
        p: PlayerId,
        theirs: &[Object],
        (game, stopped): (&GameState, bool),
        notes: &mut Vec<(PlayerId, Option<ObjectId>, GenieError)>,
    ) -> Result<(), GenieError> {
        let mut i = ListIndex::FIRST;
        while self.players.player(p)?.objects.holds(i) {
            let unit = &self.players.live(p, i)?;
            let resync = game.resync.contains(&unit.id);
            let (id, frozen) = (unit.id, stopped || unit.stopped || resync);
            // a dead object never freezes, but one that differs takes the game's state too
            // (a felled tree's wood that a frozen gatherer did not take)
            if !frozen || (!unit.life.is_alive() && !resync) {
                i = i.next();
                continue;
            }
            let Some(g) = theirs.iter().find(|g| g.unit.id == id) else {
                let elsewhere = game
                    .players
                    .iter()
                    .any(|q| q.iter().any(|g| g.unit.id == id));
                if elsewhere {
                    notes.push((
                        p,
                        Some(id),
                        GenieError::unsupported("a forced object changed hands"),
                    ));
                    i = i.next();
                    continue;
                }
                let gone = self.swap_remove_live(p, i)?;
                if let Err(e) = self.leave_world(p, &gone) {
                    notes.push((p, Some(id), e));
                }
                continue;
            };
            if g.unit.life.master_id != unit.life.master_id {
                notes.push((
                    p,
                    Some(id),
                    GenieError::unsupported("a forced object changed masters"),
                ));
                i = i.next();
                continue;
            }
            self.take_game_object(p, i, g)?;
            i = i.next();
        }
        Ok(())
    }

    /// Object `i` of player `p` becomes the game's `g`, with what the save does not hold kept
    /// (a hunted animal's origin, a boar's flag) and its inside list in the slice's order while
    /// it holds the same objects; the world places it where the game has it, follows its state
    /// (a resource past state 6 is forgotten), unfreezes it and files its mesh place.
    ///
    /// # Errors
    ///
    /// A player or a slot the world does not hold.
    fn take_game_object(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        g: &Object,
    ) -> Result<(), GenieError> {
        // the parts the import reads are the game's (the dead master, its gap, stays genie's)
        let id = self.players.live(p, i)?.id;
        self.set_part(id, Part::GatherPoint(g.parts.gather_point))?;
        self.set_part(
            id,
            Part::Production(g.parts.production.clone().map(Box::new)),
        )?;
        self.set_part(id, Part::Missile(g.parts.missile))?;
        self.set_part(id, Part::TownBell(g.parts.town_bell))?;
        self.set_part(id, Part::Building(Box::new(g.parts.building.clone())))?;
        self.set_part(id, Part::Combat(Box::new(g.parts.combat.clone())))?;
        let mut ai = g.parts.unit_ai.clone();
        keep_unsaved_ai(self.parts.unit_ai.get(id), ai.as_mut());
        self.set_part(id, Part::UnitAi(ai.map(Box::new)))?;
        self.set_part(id, Part::Motion(g.parts.motion.clone().map(Box::new)))?;
        self.set_part(id, Part::Actions(Box::new(g.parts.actions.clone())))?;
        self.set_part(id, Part::Unread(g.parts.unread.clone()))?;
        let unit = &self.players.live(p, i)?;
        let mut forced = g.unit.clone();
        keep_unsaved(unit, &mut forced);
        let before = unit.life.state;
        if forced.position() != unit.position() {
            self.spatial.set_position(id, forced.position())?;
        }
        self.follow_state(p, &forced, before)?;
        self.spatial.unfreeze(id);
        if let Some(placed) = self.spatial.get(id).copied() {
            let moving = self.parts.mover_of(forced.id).is_some_and(Mover::is_moving);
            self.pathing.moved(id, &placed, moving);
        }
        forced.stopped = false;
        *self.players.live_mut(p, i)? = forced;
        Ok(())
    }

    /// The spatial index follows object `forced`'s (of player `p`) state from `before`: its
    /// sight dies with it and takes its state, and a resource past state 6 is forgotten.
    ///
    /// # Errors
    ///
    /// An object the spatial index does not hold, or a master the world does not.
    fn follow_state(
        &mut self,
        p: PlayerId,
        forced: &Unit,
        before: ObjectState,
    ) -> Result<(), GenieError> {
        let (id, state) = (forced.id, forced.life.state);
        if state == before {
            return Ok(());
        }
        sight_follows(&mut self.spatial, id, (before, state))?;
        if before < ObjectState::DyingSprite && ObjectState::Undead < state {
            let master =
                forced.master(MasterTables::new(&self.masters, &self.parts.own_master), p)?;
            forget_resource(&mut self.spatial, Some(master), id, forced.position())?;
        }
        Ok(())
    }
}

/// Object `id`'s sight follows its state from `before` to `state`: it dies with the object
/// (state 3 and up from 2 or below), then takes the state unless that is the dying one.
///
/// # Errors
///
/// An object the spatial index does not hold.
fn sight_follows(
    spatial: &mut Spatial,
    id: ObjectId,
    (before, state): (ObjectState, ObjectState),
) -> Result<(), GenieError> {
    if before <= ObjectState::Alive && ObjectState::Dying <= state {
        spatial.sight_state(id, ObjectState::Dying)?;
    }
    if state != ObjectState::Dying {
        spatial.sight_state(id, state)?;
    }
    Ok(())
}

/// What the game's object `forced` takes from the slice's `unit` that the save does not hold:
/// the order of the objects inside it while they are the same (the game's history).
fn keep_unsaved(unit: &Unit, forced: &mut Unit) {
    if same_members(&unit.garrison, &forced.garrison) {
        forced.garrison.clone_from(&unit.garrison);
    }
}

/// What the game's unit AI `forced` takes from the slice's `old` that the save does not hold: a
/// hunted animal's origin and a boar's flag.
const fn keep_unsaved_ai(old: Option<&UnitAi>, forced: Option<&mut UnitAi>) {
    if let (Some(old), Some(new)) = (old, forced) {
        new.hunted = old.hunted;
        if let (Some(o), Some(n)) = (old.home, new.home.as_mut()) {
            n.go_home = o.go_home;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::commands::{GatherPoint, GatherPointState};
    #[cfg(test)]
    use crate::gathering::hunt::tests::walker;
    use crate::map::Position;
    #[cfg(test)]
    use crate::objects::layers::Motion;
    #[cfg(test)]
    use crate::objects::lifecycle::Lifecycle;
    use crate::objects::masters::ids;
    use crate::players::game_status::GameStatus;
    #[cfg(test)]
    use crate::production::Production;
    use crate::test_ids::group_id;
    use crate::test_ids::object_id;
    #[cfg(test)]
    use crate::test_ids::{master_id, tech_id};
    #[cfg(test)]
    use crate::unit_ai::Hunted;
    use crate::world::parts::Object;
    use crate::world::tests::forage_world;
    #[cfg(test)]
    use crate::world::tests::hands_world;
    #[cfg(test)]
    use crate::world::{Tick, Unread};

    /// A forced object's sight follows its state ([`sight_follows`]): from 2 to 3 it dies; from
    /// 3 on it takes the state; from 2 past 3 it dies and takes the state.
    #[test]
    fn forced_object_sight_follows_its_state() {
        use crate::map::Distance;
        use crate::map::spatial::Spatial;
        use crate::objects::masters::class;
        let id = object_id(21712);
        let world = || {
            hands_world(
                21712,
                PlayerId::new(0),
                class::CLASS_0,
                master_id(4),
                Distance::new(6.0),
            )
        };
        let state = |s: &Spatial| s.sight.as_ref().and_then(|v| v.seer(id)).map(|s| s.state);
        let mut s = world();
        assert_eq!(state(&s), Some(ObjectState::Alive));
        sight_follows(&mut s, id, (ObjectState::Alive, ObjectState::Dying)).unwrap();
        assert_eq!(state(&s), Some(ObjectState::Dying));
        sight_follows(&mut s, id, (ObjectState::Dying, ObjectState::Removed)).unwrap();
        assert_eq!(state(&s), Some(ObjectState::Removed));
        let mut s = world();
        sight_follows(&mut s, id, (ObjectState::Alive, ObjectState::DyingSprite)).unwrap();
        assert_eq!(state(&s), Some(ObjectState::DyingSprite));
    }

    /// The world's own forced values: the frame seed, the game state, the next reusable id and
    /// the pools and unit groups the oracle dumped; a pool not dumped stays the slice's.
    #[test]
    fn forced_world_takes_the_game_seed_ids_and_dumped_pools() {
        let mut t = forage_world();
        t.recycling.missiles = vec![(object_id(5), false)];
        let groups = UnitGroups {
            counter: group_id(4),
            ..UnitGroups::default()
        };
        let game = GameState {
            frame_seed: FrameSeed::of_data(7),
            game_state: PlayState::of_data(3),
            next_reusable_id: object_id(-5),
            ghost_pool: Some(vec![object_id(-3)]),
            groups: Some(groups.clone()),
            ..GameState::default()
        };
        t.force_world_values(&game);
        assert_eq!(
            (
                t.frame_seed.get(),
                t.game_state.get(),
                t.recycling.next_id.get()
            ),
            (7, 3, -5)
        );
        assert_eq!(t.recycling.ghosts, [object_id(-3)]);
        assert_eq!(t.recycling.missiles, [(object_id(5), false)]);
        assert_eq!(t.groups, groups);
    }

    /// A stopped player takes the game's resources, pending debit and technologies and runs
    /// again; every player takes its status and resignation and loses its first unmodelled
    /// part; one that runs keeps its resources.
    #[test]
    fn stopped_player_takes_the_game_resources_and_runs_again() {
        let game = GameState {
            resources: vec![[0.0; 4], [1.0, 2.0, 3.0, 4.0]],
            pending_debit: vec![[Amount::ZERO; 4], [Amount::new(5.0); 4]],
            techs: vec![vec![], vec![TechState::default(); 2]],
            statuses: vec![(GameStatus::PLAYING, false), (GameStatus::LOST, true)],
            ..GameState::default()
        };
        let mut t = forage_world();
        let before = t.players[1].resources();
        t.players[1].unsupported = Some((
            Tick::new(1),
            ListIndex::new(0),
            GenieError::unsupported("a part"),
        ));
        assert!(!t.force_player_values(PlayerId::new(1), &game).unwrap());
        let player = &t.players[1];
        assert_eq!(player.resources(), before);
        assert_eq!(
            (player.game_status, player.resigned),
            (GameStatus::LOST, true)
        );
        assert!(player.unsupported.is_none());
        t.players[1].stopped = true;
        assert!(t.force_player_values(PlayerId::new(1), &game).unwrap());
        let player = &t.players[1];
        assert_eq!(
            (
                player.resources().map(Amount::get),
                player.pending_debit,
                player.techs.len()
            ),
            ([1.0, 2.0, 3.0, 4.0], [Amount::new(5.0); 4], 2)
        );
        assert!(!player.stopped);
    }

    /// The forced objects: a frozen forager takes the game's (placed where the game has it,
    /// running again, its gather point, production, town bell and motion part the game's) but
    /// keeps its hunted record,
    /// which no save holds; a frozen object the game no longer holds leaves; one that changed
    /// hands or masters stays, noted; an object that runs, and a dead one the game does not
    /// resync, are the slice's; an object the game made and the slice did not is noted.
    #[test]
    fn forced_replay_takes_the_game_frozen_objects_and_notes_what_it_cannot() {
        let mut t = forage_world();
        let hunted = Some(Hunted::default());
        let forager = &mut t.players[1].objects[ListIndex::new(0)];
        forager.stopped = true;
        t.parts.unit_ai.get_mut(forager.id).unwrap().hunted = hunted;
        // the game's objects with their unit AIs; the save holds no hunted origin
        let mut theirs: Vec<Object> = t.players[1]
            .objects
            .iter()
            .map(|u| Object::with_ai(u.clone(), t.parts.unit_ai.get(u.id).cloned()))
            .collect();
        theirs[0]
            .unit
            .place_for_test(Position::new(29.5, 88.5, 0.0));
        theirs[0].unit.stopped = false;
        theirs[0].parts.unit_ai.as_mut().unwrap().hunted = None;
        let mut game = GameState {
            players: vec![vec![], theirs],
            ..GameState::default()
        };
        let gather = GatherPoint {
            state: GatherPointState::POINT,
            point: Position::new(30.0, 80.0, 0.0),
            target: None,
            target_master: None,
        };
        game.players[1][0].parts.gather_point = Some(gather);
        let production = Production {
            disabled_techs: vec![tech_id(22)],
            ..Production::default()
        };
        game.players[1][0].parts.production = Some(production.clone());
        game.players[1][0].parts.town_bell.flag = true;
        let mut walker = walker();
        walker.waiting = 3;
        let motion = Motion::Moving(walker);
        game.players[1][0].parts.motion = Some(motion.clone());
        // the game's import could not read it whole: the force takes its reason
        let unread = Unread::new(GenieError::unsupported("a saved list"));
        game.players[1][0].parts.unread = Some(unread.clone());
        let notes = t.force(&game);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(t.parts.motion.get(object_id(9175)), Some(&motion));
        assert_eq!(t.parts.unread.get(object_id(9175)), Some(&unread));
        assert_eq!(t.parts.gather_point.get(object_id(9175)), Some(&gather));
        assert_eq!(t.parts.production.get(object_id(9175)), Some(&production));
        assert!(t.parts.town_bell_of(object_id(9175)).flag);
        let forager = &t.players[1].objects[ListIndex::new(0)];
        assert_eq!(
            (forager.position(), forager.stopped),
            (Position::new(29.5, 88.5, 0.0), false)
        );
        assert_eq!(t.parts.unit_ai.get(forager.id).unwrap().hunted, hunted);
        assert_eq!(
            t.spatial.get(object_id(9175)).map(|p| p.position),
            Some(Position::new(29.5, 88.5, 0.0))
        );
        // the game made 800, then the forager changed hands, then masters, then left
        game.players[1].push(
            Unit {
                id: object_id(800),
                ..Unit::default()
            }
            .into(),
        );
        let reasons = |t: &mut World, game: &GameState| -> Vec<(Option<ObjectId>, &'static str)> {
            t.players[1].objects[ListIndex::new(0)].stopped = true;
            t.force(game)
                .into_iter()
                .map(|(_, id, e)| (id, e.reason()))
                .collect()
        };
        assert_eq!(
            reasons(&mut t, &game),
            [(
                Some(object_id(800)),
                "an object the game made and the slice did not"
            )]
        );
        let mut moved = game.clone();
        let forager = moved.players[1].remove(0);
        moved.players[0].push(forager);
        let _ = moved.players[1].pop();
        assert_eq!(
            reasons(&mut t, &moved),
            [(Some(object_id(9175)), "a forced object changed hands")]
        );
        let mut changed = moved.clone();
        let forager = changed.players[0].pop().unwrap().unit;
        changed.players[1].insert(
            0,
            Unit {
                life: Lifecycle {
                    master_id: ids::VILLAGER_MALE,
                    ..forager.life
                },
                ..forager
            }
            .into(),
        );
        assert_eq!(
            reasons(&mut t, &changed),
            [(Some(object_id(9175)), "a forced object changed masters")]
        );
        let mut gone = changed;
        let _ = gone.players[1].remove(0);
        let before = t.players[1].objects.len();
        assert_eq!(reasons(&mut t, &gone), []);
        assert_eq!(t.players[1].objects.len(), before - 1);
        assert!(t.players[1].objects.iter().all(|u| u.id != object_id(9175)));
    }
}
