//! A position's change, the sight check and the unit AI's searches: the closest object and the
//! best unit to attack.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::WholeDistance;
use crate::map::spatial::{
    ClosestChecks, Filter, Placed, QUERY_HITS, Reach, RecordId, Spatial, players_seen,
    record_meets_segment, tile_record_id,
};
use crate::map::{Rect, Segment};
use crate::math::conversions::cvttss2si;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::player_master;
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::players::PlayerId;
use crate::players::UnitRelations;
use crate::players::{ByRelation, QueryRelation, UnitRelation};
use crate::unit_ai::best_target::AttackField;
use crate::unit_ai::best_target::{Choice, best_unit_to_attack as best_target_best_unit_to_attack};
use crate::unit_ai::world_view::WorldPicker;
use crate::unit_ai::{AttackAsk, Memory};
use crate::unit_ai::{Found, Pick, Search};
use crate::vision::SightTiles;
use crate::vision::registry::Record;
use crate::vision::registry::VisibleQuery;
use crate::vision::visibility::Seer;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::BestTargetTuning;
use crate::world::layer::generated::RegistryTuning;
use crate::world::layer::generated::SpatialTuning;
use crate::world::layer::generated::VisibilityTuning;
use std::fmt::{Debug, Error, Formatter};

/// Whether a unit AI's search queried afresh or read the cache's last answer
/// ([`Spatial::search_query`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum SearchLists {
    /// The registry's query ran: the lists are this search's.
    Fresh,
    /// The cache held near the place, or the query read nothing: the last answer stands.
    Cached,
}

impl Spatial {
    /// `FormationTransform`'s line query (`FUN_141051900` with flags 0x40, any player, the
    /// segment test `FUN_141051e00`) with the buffer `grow` the formation finds
    /// ([`Spatial::manager_grow`]): whether the line `from` -> `to` meets a record, the
    /// `ignore`d objects' and the moving objects' (flag 0x40, [`Gait::moving`]; the game's list
    /// of moving objects still tested holds only ignored ones here) left out. The records: the
    /// map edge's where the line's rectangle, grown by `grow`, leaves the map
    /// ([`Spatial::edge_records`]), the temporary tile records still put
    /// ([`Spatial::put_tile_records`]: the formation's over its box, kind 3, the tile's box) and
    /// the objects' of a tested kind (kind 10 filed as 2), in the cells the query visits; a
    /// record counts when its box, grown by `grow`, meets the line's rectangle and the line
    /// (`FUN_1410512f0`; a circle's alike).
    ///
    /// # Errors
    ///
    /// A record whose place the slice does not know near the line.
    ///
    /// [`Gait::moving`]: crate::map::spatial::Gait::moving
    pub fn still_line_hit(
        &self,
        from: Point,
        to: Point,
        grow: Distance,
        ignore: &[RecordId],
        by_id: ObjectsRef<'_>,
    ) -> Result<bool, GenieError> {
        let rect = Rect::new(
            if to.x() <= from.x() { to.x() } else { from.x() },
            if to.y() <= from.y() { to.y() } else { from.y() },
            if from.x() <= to.x() { to.x() } else { from.x() },
            if from.y() <= to.y() { to.y() } else { from.y() },
        );
        let query = rect.grown(grow);
        if query.leaves_map(self.map) && !self.edge_records(query, QUERY_HITS).is_empty() {
            return Ok(true);
        }
        self.check_known_records(query)?;
        let walk = Segment { from, to };
        let meets = |kind: ObstructionKind, bx: Rect| {
            rect.overlaps(bx.grown(grow)) && record_meets_segment(kind, bx, grow, walk)
        };
        let level = self.grid.level(0);
        let shift = self.grid.shift(level);
        let tile = self.tile_records.iter().any(|&(y, x)| {
            self.visits_cell(level, x >> shift, y >> shift, query)
                && !ignore.contains(&tile_record_id(Tile::new(x, y)))
                && meets(
                    ObstructionKind::KIND_3,
                    Rect::new(x as f32, y as f32, x as f32 + 1.0, y as f32 + 1.0),
                )
        });
        if tile {
            return Ok(true);
        }
        Ok(self.records_in(query).any(|(id, p)| {
            let Some(o) = p.obstruction else {
                return false;
            };
            let kind = o.kind.filed();
            kind.tested()
                && !ignore.contains(&RecordId::from(id))
                && !by_id.moving(id)
                && self.visits(p, kind, query)
                && meets(kind, p.bounds())
        }))
    }

    /// The index's part of the object's new place (`new_position`, `0x1411a58b0`): its record's
    /// place, its tile's counts and, on a new tile, its sight follow. The engine calls it only
    /// through the object's place setter ([`crate::objects::layers::Static::new_position`]),
    /// which moves the object with it (FD-045-D15); a tool that asks a query from another place
    /// moves the index alone.
    pub fn set_position(
        &mut self,
        id: ObjectId,
        position: Position,
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> Result<(), GenieError> {
        if let Some(p) = self.objects.get_mut(id) {
            let from = p.position;
            p.position = position;
            let (old, new) = (self.tiles.tile_of(from), self.tiles.tile_of(position));
            if old != new {
                let objects = &self.objects;
                if let Some(i) = old {
                    self.tiles.take_off(i, id, |o| Self::is_tall(objects, o));
                }
                if let Some(i) = new {
                    self.tiles.append(i, id, |o| Self::is_tall(objects, o));
                }
            }
            self.refile(id);
            let Some(p) = self.objects.get(id) else {
                return Ok(());
            };
            if let Some(v) = self.sight.as_mut() {
                v.moved(id, from, position, p.radius, roles, visibility_tuning)?;
                v.registry_moved(id, p.class, from, position);
            }
        }
        Ok(())
    }

    /// Fails when a frozen object of `player` that may have moved sees, or saw, near `rect`: the
    /// player's visible map there, and the registry's records the objects' masks keep, may differ
    /// from the game's (its sight, plus a tile, beyond what it may have moved).
    pub fn check_sight(&self, player: PlayerId, rect: Rect) -> Result<(), GenieError> {
        let Some(v) = self.sight.as_ref() else {
            return Ok(());
        };
        let near = self.frozen().any(|(id, p)| {
            let Some(u) = p.unknown.filter(|&u| 0.0 < u && p.player == player) else {
                return false;
            };
            let los = v
                .seer(id)
                .map_or(SightTiles::NONE, |s| SightTiles::of(s.los));
            if !los.sees() {
                return false;
            }
            let grow = u + los.get() as f32 + 1.0;
            rect.overlaps(p.bounds().grown(Distance::new(grow)))
        });
        if near {
            return Err(GenieError::unsupported(
                "a stopped object's sight may reach the search",
            ));
        }
        Ok(())
    }

    /// A unit AI's search ([`Search`]) for object `me` of `player` (`FUN_140c63e20`): the query
    /// over the player's registry ([`crate::vision::registry::Registry::visible_units`]) unless the
    /// [`SearchCache`] holds near the place (`fresh` false: the last answer stands); then its
    /// [`Pick`], an attack by `attack`'s tests of the objects found and its choice among them. A query that reads nothing leaves the last answer too (and so is not fresh). A
    /// query near a frozen object, or a last answer the slice has not got, fails. The query's
    /// `objects` answer what [`Spatial::closest_object`] reads of a found object.
    ///
    /// [`SearchCache`]: crate::map::spatial::SearchCache
    pub fn search(
        &mut self,
        query: SearchQuery<'_>,
        roles: &Roles,
        best_target_tuning: &BestTargetTuning,
        spatial_tuning: &SpatialTuning,
        registry_tuning: RegistryTuning,
    ) -> Result<Found, GenieError> {
        let SearchQuery {
            searcher: me,
            player,
            search,
            attack,
            reach,
            objects,
        } = query;
        let _ = self.sight.as_ref().ok_or(GenieError::unsupported(
            "a search without the players' sight",
        ))?;
        let fresh = self.search_query(player, search, spatial_tuning, registry_tuning)?
            == SearchLists::Fresh;
        let _ = self.last_found()?;
        self.pick_found(
            (me, player),
            search,
            fresh,
            attack,
            reach,
            ClosestChecks {
                objects,
                roles,
                best_target_tuning,
                spatial_tuning,
            },
        )
    }

    /// The lists the last search left, as the cache holds them, or the error of a search that
    /// reads the registry's last answer before any query.
    fn last_found(&self) -> Result<&ByRelation<Vec<Record>>, GenieError> {
        self.search_cache
            .last
            .as_ref()
            .ok_or(GenieError::unsupported(
                "a search that reads the registry's last answer (before any query)",
            ))
    }

    /// What player `player`'s queries make of the first `players` players' objects
    /// ([`Spatial::unit_diplomacy`], `player+0x278`).
    ///
    /// # Errors
    ///
    /// A player whose kinds the slice did not load.
    pub fn query_kinds(
        &self,
        player: PlayerId,
        players: usize,
    ) -> Result<UnitRelations, GenieError> {
        let kinds = self
            .unit_diplomacy
            .get(player)
            .ok_or(GenieError::unsupported(
                "a search of a player without its unit diplomacy",
            ))?;
        Ok(kinds.values().iter().take(players).copied().collect())
    }

    /// The query of a unit AI's search (`FUN_140c63e20`'s first part): over the player's
    /// registry unless the [`SearchCache`] holds near the place ([`SearchLists::Fresh`] when it
    /// queried afresh, else [`SearchLists::Cached`]). The
    /// lists it leaves are [`Spatial::searched`]'s.
    ///
    /// [`SearchCache`]: crate::map::spatial::SearchCache
    pub fn search_query(
        &mut self,
        player: PlayerId,
        search: &Search<'_>,
        spatial_tuning: &SpatialTuning,
        registry_tuning: RegistryTuning,
    ) -> Result<SearchLists, GenieError> {
        let v = self.sight.as_ref().ok_or(GenieError::unsupported(
            "a search without the players' sight",
        ))?;
        let r = v.registry.as_ref().ok_or(GenieError::unsupported(
            "a search without the objects-in-sight registry",
        ))?;
        let relations = self.query_kinds(player, r.players)?;
        let (x, y) = (search.at.x(), search.at.y());
        let mut answer = SearchLists::Cached;
        let (dx, dy) = (x - self.search_cache.at.x(), y - self.search_cache.at.y());
        if spatial_tuning.search_reuse_squared < dy * dy + dx * dx || !self.search_cache.holds {
            self.search_cache.hold(search.at);
            let radius = search.radius.get().min(1000.0);
            let (rx, ry) = (search.size.x(), search.size.y());
            let rect = Rect::new(
                (x - (radius + rx)) - 0.001,
                (y - (radius + ry)) - 0.001,
                x + (radius + rx) + 0.001,
                y + (radius + ry) + 0.001,
            );
            self.check_known_classes(rect, search.classes)?;
            self.check_sight(player, rect)?;
            if let Some(lists) = r.visible_units(
                VisibleQuery {
                    player,
                    position: search.at,
                    radius: search.radius,
                    mode: 0,
                    classes: search.classes,
                    relations: &relations,
                    size: search.size,
                    wanted: 0xff,
                },
                registry_tuning,
            )? {
                self.search_cache.remember(lists);
                answer = SearchLists::Fresh;
            }
        }
        Ok(answer)
    }

    /// A unit AI's full update starts (`0x140c66b10`): the search cache lets go.
    pub const fn forget_search(&mut self) {
        self.search_cache.holds = false;
    }

    /// The lists the last search left (`DAT_143f69178`, by relation kind; 4 the enemies), as
    /// [`Spatial::search_query`] left them.
    #[must_use]
    pub const fn searched(&self) -> Option<&ByRelation<Vec<Record>>> {
        self.search_cache.last.as_ref()
    }

    /// The lists a last search left, as a test sets them.
    #[cfg(test)]
    pub(crate) fn set_searched(&mut self, lists: ByRelation<Vec<Record>>) {
        self.search_cache.remember(lists);
    }

    /// The search's pick of what it found ([`Spatial::search`]).
    pub(super) fn pick_found(
        &mut self,
        (me, player): (ObjectId, PlayerId),
        search: &Search<'_>,
        fresh: bool,
        attack: Option<&WorldPicker<'_>>,
        reach: &mut Reach<'_>,
        checks: ClosestChecks<'_>,
    ) -> Result<Found, GenieError> {
        let roles = checks.roles;
        let best_target_tuning = checks.best_target_tuning;
        match search.pick {
            Pick::Closest {
                group,
                relation,
                state,
                master,
                workers,
                spared,
                reach: reaching,
            } => {
                let filter = Filter {
                    group,
                    relation,
                    state,
                    master,
                    workers,
                    spared,
                    reach: reaching,
                };
                let (id, distance) = self.closest_object(me, player, filter, reach, checks)?;
                let found = self.last_found()?;
                Ok(Found {
                    id,
                    enemies: !found[UnitRelation::Enemy].is_empty(),
                    position: id.and_then(|id| self.objects.get(id)).map(|p| p.position),
                    fresh,
                    players: players_seen(found),
                    distance: WholeDistance::of_data(cvttss2si(distance)),
                    retries: [None; 3],
                })
            }
            Pick::Look => {
                let found = self.last_found()?;
                Ok(Found {
                    id: None,
                    enemies: !found[UnitRelation::Enemy].is_empty(),
                    position: None,
                    fresh,
                    players: 0,
                    distance: WholeDistance::NONE,
                    retries: [None; 3],
                })
            }
            Pick::Attack { ask, .. } => {
                let choice = self.best_unit_to_attack(
                    &ask,
                    search.memory,
                    attack.ok_or(NO_PICKER)?,
                    reach,
                    roles,
                    best_target_tuning,
                )?;
                let id = choice.target;
                let mut retries = [None; 3];
                for (slot, r) in retries.iter_mut().zip(choice.retries) {
                    *slot = Some(r);
                }
                Ok(Found {
                    id,
                    enemies: !self.last_found()?[UnitRelation::Enemy].is_empty(),
                    position: id.and_then(|id| self.objects.get(id)).map(|p| p.position),
                    fresh,
                    players: 0,
                    distance: WholeDistance::NONE,
                    retries,
                })
            }
        }
    }

    /// `ClosestObject` (`FUN_140c488b0`) for object `me` over what its search `found`, its
    /// reachability check on (argument 8, every caller's): each record by relation and order:
    /// itself, an object that is gone and one dying (state 7 and up) are passed, so is one the
    /// [`Filter`] leaves out; then one the unit reaches within `SpatialTuning::closest_range` (slot 0x3d0,
    /// `reach`) and nearer than the nearest so far (by the search's distance; a tie goes to the
    /// lower id) is the nearest. Livestock (class 58) counts when the searcher's player owns it or
    /// it is not in state 2, and not while its top action is the Artifact with a point
    /// ([`herded`]: slot 0x288 not −1 and slot 0x530 107, "Sheep is moving"). A searcher of class
    /// 10 passes the objects [`shuns`] names (of master 440 or a master with object flag 0x10; the
    /// game's `+0x6b4` of 1, not met, would let it take them). Not modelled: a workers cap
    /// (argument 6; the objects' `+0x644`), the unbuilt farm test of a search for state 0, the
    /// pastures and farms (class 49). Nothing found answers −1.
    pub(super) fn closest_object(
        &mut self,
        me: ObjectId,
        player: PlayerId,
        filter: Filter,
        reach: &mut Reach<'_>,
        checks: ClosestChecks<'_>,
    ) -> Result<(Option<ObjectId>, f32), GenieError> {
        let (roles, spatial_tuning) = (checks.roles, checks.spatial_tuning);
        let at = self.objects.get(me);
        let searcher = Searcher {
            class_10: at.is_some_and(|m| roles.of_class(m.class).contains(ClassRoles::PREDATOR)),
            // The villagers' class, which gathers from farms (`FUN_141195750`).
            farmer: at.is_some_and(|m| roles.of_class(m.class).contains(ClassRoles::VILLAGER)),
            trapper: at.is_some_and(|m| roles.of_master(m.master).contains(MasterRoles::TRAPPER)),
        };
        let spared = filter.spared.map(QueryRelation::list);
        let mut best: Option<(ObjectId, f32)> = None;
        for relation in UnitRelation::ALL {
            // no filter (−1) keeps every list
            let related = filter.relation.is_none_or(|f| f.keeps(relation));
            // each record is read where it is tested: the reachability check is handed the
            // spatial index, which it writes
            let mut next = 0;
            while let Some(record) = self.last_found()?[relation].get(next).copied() {
                next += 1;
                if record.id == me {
                    continue;
                }
                let sight = self.sight.as_ref().ok_or(GenieError::unsupported(
                    "a search without the players' sight",
                ))?;
                let (Some(seer), Some(p)) = (sight.seer(record.id), self.objects.get(record.id))
                else {
                    continue;
                };
                if ObjectState::Removed <= seer.state || !related || spared == Some(relation) {
                    continue;
                }
                if !closest_kept(record.id, (seer, p), filter, (player, searcher), checks)? {
                    continue;
                }
                if filter.reach
                    && !reach(self, record.id, Distance::new(spatial_tuning.closest_range))?
                {
                    continue;
                }
                best = nearer(best, &record);
            }
        }
        // the distance starts at `DAT_14330ec20` (−1) without a find
        Ok(best.map_or((None, -1.0), |(id, d)| (Some(id), d)))
    }

    /// `bestUnitToAttack` (`FUN_140c44680`, [`crate::unit_ai::best_target::best_unit_to_attack`]) over
    /// the relation lists its search `found` (each record's id in order), with the unit AI's
    /// `memory`: `attack` reads the unit and the objects, the paths (slot 0x3d0) are `reach`'s.
    ///
    /// # Errors
    ///
    /// What the reads and tests do not model.
    pub(super) fn best_unit_to_attack(
        &mut self,
        ask: &AttackAsk,
        memory: Memory<'_>,
        attack: &WorldPicker<'_>,
        reach: &mut Reach<'_>,
        roles: &Roles,
        best_target_tuning: &BestTargetTuning,
    ) -> Result<Choice, GenieError> {
        let me = attack.searcher(memory)?;
        let found = self.last_found()?;
        let lists: ByRelation<Vec<ObjectId>> = found.map(|v| v.iter().map(|r| r.id).collect());
        let mut field = AttackField {
            picker: attack,
            reach,
            world: self,
        };
        best_target_best_unit_to_attack(
            &me,
            &ask.ask(),
            &lists,
            &mut field,
            roles,
            best_target_tuning,
        )
    }
}

/// An attack pick in a search that has no attack picker ([`SearchQuery::attack`]).
const NO_PICKER: GenieError =
    GenieError::unsupported("an attack test in a search that picks no attack");

/// What a unit AI's search takes ([`Spatial::search`]): the searcher, its [`Search`], and the
/// callers' answers for what the search reads of the world beyond the spatial index.
pub struct SearchQuery<'a> {
    /// The searching object.
    pub(crate) searcher: ObjectId,
    /// Its player.
    pub(crate) player: PlayerId,
    /// What it searches for.
    pub(crate) search: &'a Search<'a>,
    /// An attack pick's reads of the unit and the objects; a search that picks no attack has
    /// none.
    pub(crate) attack: Option<&'a WorldPicker<'a>>,
    /// The reachability of a found object (slot 0x3d0).
    pub(crate) reach: &'a mut Reach<'a>,
    /// The world's objects by id: whether livestock is herded ([`herded`]), the masters a
    /// class-10 searcher passes ([`shuns`]), a farm's workers and pasture ([`farm_kept`]).
    pub(crate) objects: ObjectsRef<'a>,
}

impl Debug for SearchQuery<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        f.debug_struct("SearchQuery")
            .field("searcher", &self.searcher)
            .field("player", &self.player)
            .field("search", &self.search)
            .finish_non_exhaustive()
    }
}

/// What `ClosestObject` reads of its searcher (`FUN_140c488b0`): of class 10 it passes the
/// objects `shuns` names; a farmer takes farms, a trapper fish traps ([`farm_kept`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Searcher {
    pub(super) class_10: bool,
    pub(super) farmer: bool,
    pub(super) trapper: bool,
}

/// `ClosestObject`'s tests of the object `id` it found (its seer and its place `p`) past the
/// relation: the [`Filter`]'s class, master and state; a farm ([`farm_kept`]); livestock
/// (class 58) the searcher's player owns or not in state 2, and not [`herded`]; for a searcher of
/// class 10, not what [`shuns`] names. A workers cap and the unbuilt farm test (a state of 0)
/// are not modelled.
pub(super) fn closest_kept(
    id: ObjectId,
    (seer, p): (&Seer, &Placed),
    filter: Filter,
    (player, searcher): (PlayerId, Searcher),
    ClosestChecks { objects, roles, .. }: ClosestChecks<'_>,
) -> Result<bool, GenieError> {
    if filter.group.is_some_and(|g| p.class != g) {
        return Ok(false);
    }
    if filter.master.is_some_and(|m| p.master != m) {
        return Ok(false);
    }
    if filter.state.is_some_and(|state| seer.state != state) {
        return Ok(false);
    }
    if filter.workers != WorkerCount::NO_CAP {
        return Err(GenieError::unsupported(
            "ClosestObject's workers cap (+0x644)",
        ));
    }
    if filter.state == Some(ObjectState::Foundation) {
        return Err(GenieError::unsupported("ClosestObject's unbuilt farm test"));
    }
    if roles
        .of_some_class(filter.group)
        .contains(ClassRoles::CLOSEST_PASTURE)
        && !farm_kept(id, p, filter, searcher, objects, roles)?
    {
        return Ok(false);
    }
    if roles
        .of_some_class(filter.group)
        .contains(ClassRoles::CLOSEST_LIVESTOCK)
        && ((p.player != player && seer.state == ObjectState::Alive) || herded(objects, id))
    {
        return Ok(false);
    }
    if searcher.class_10 && shuns(objects, roles, p)? {
        return Ok(false);
    }
    Ok(true)
}

/// `ClosestObject`'s "Sheep is moving" (`FUN_140c488b0`): the object's top action is the Artifact
/// (slot 0x530 107) with a point (slot 0x288, the action's `+0x38`, not −1); `by_id` reads its
/// action layer.
pub(crate) fn herded(by_id: ObjectsRef<'_>, id: ObjectId) -> bool {
    by_id.actions(id).is_some_and(|a| {
        a.top_action_kind() == Some(ARTIFACT_KIND)
            && a.list
                .artifact()
                .is_some_and(|a| a.goal.is_some_and(|g| g.x() != -1.0))
    })
}

/// What a searcher of class 10 passes (`FUN_140c488b0`): the found object's master, read from its
/// player's table, is the petard (master 440) or holds object flag 0x10.
///
/// # Errors
///
/// A found object of a master its player's table does not hold.
pub(super) fn shuns(
    objects: ObjectsRef<'_>,
    roles: &Roles,
    p: &Placed,
) -> Result<bool, GenieError> {
    let m = player_master(objects.masters().players, p.player, p.master).ok_or(
        GenieError::unsupported("a found object of a master the slice does not have"),
    )?;
    Ok(roles.of_master(p.master).contains(MasterRoles::PETARD)
        || m.object_flags.contains(ObjectFlags::SCOUT))
}

/// A farm (class 49) `ClosestObject` found, its workers (`+0x644`) and pasture (`+0x790`,
/// `parent_pasture_id`) read from `objects`: not a pasture's livestock
/// (`+0x790`); a pasture is not modelled; without a master asked for, one without workers that
/// the searcher gathers from (a fish trap for a trapper, any other for a farmer).
pub(super) fn farm_kept(
    id: ObjectId,
    p: &Placed,
    filter: Filter,
    searcher: Searcher,
    objects: ObjectsRef<'_>,
    roles: &Roles,
) -> Result<bool, GenieError> {
    let (workers, parent) =
        objects
            .get(id)
            .map(|u| (u.workers, u.pasture))
            .ok_or(GenieError::unsupported(
                "a found farm the slice does not hold",
            ))?;
    if parent.is_some() {
        return Ok(false);
    }
    if roles
        .of_master(p.master)
        .contains(MasterRoles::COUNTED_PASTURE)
    {
        return Err(GenieError::unsupported(
            "ClosestObject over pastures (their workers and livestock)",
        ));
    }
    if filter.master.is_none() {
        if WorkerCount::ONE <= workers {
            return Ok(false);
        }
        // The fish trap (raw `0x6163`), which only the fishing ships gather from (`FUN_1411945a0`).
        if !searcher.trapper && roles.of_master(p.master).contains(MasterRoles::FISH_TRAP) {
            return Ok(false);
        }
        if !searcher.farmer && !roles.of_master(p.master).contains(MasterRoles::FISH_TRAP) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The nearer of the nearest so far, `best`, and `record`, by the search's distance; a tie
/// goes to the lower id (a NaN distance so far gives way).
pub(super) fn nearer(best: Option<(ObjectId, f32)>, record: &Record) -> Option<(ObjectId, f32)> {
    match best {
        Some((id, d)) if d <= record.distance.get() => {
            if record.distance == Distance::new(d) && record.id < id {
                Some((record.id, d))
            } else {
                best
            }
        }
        _ => Some((record.id, record.distance.get())),
    }
}
