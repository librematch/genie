//! The drop-site searches (`FUN_1411671e0`): from a unit, or from a point without one.

use crate::GenieResult;
use crate::gathering::DropSiteId;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::pathing::tile_path::Found;

use crate::map::spatial::Spatial;
use crate::math::image::turn::{DROP_NO_POINT, NO_DROP_DISTANCE as TURN_NO_DROP_DISTANCE};
use crate::movement::{Me, Mover, Planning, footprint_gap_squared};
use crate::objects::layers::Action;
use crate::objects::masters::Master;
use crate::objects::masters::OwnerMasters;
use crate::objects::unit::Unit;
use crate::pathing::zones::ZoneMap;
use crate::world::Unread;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::objects_ref::ObjectsRef;
use crate::world::others::OthersRef;

/// The unit a drop-site search goes from ([`drop_site_search`]).
pub(crate) struct Seeker<'world, 'plan> {
    /// The unit.
    pub(crate) unit: &'world Unit,
    /// Its action layer, whose top action's target the search reads (slot 0x190).
    pub(crate) actions: &'world Action,
    /// Why the load could not read it whole, if it could not ([`crate::world::Unread`]).
    pub(crate) unread: Option<&'world Unread>,
    pub(crate) me: Me,
    pub(crate) mover: &'world Mover,
    pub(crate) plan: &'world mut Planning<'plan>,
    pub(crate) spatial: &'world mut Spatial,
    pub(crate) zones: &'world ZoneMap,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// How the nearest is measured (`FUN_1411671e0`'s argument 9: by path a human player's).
    pub(crate) nearest: DropMeasure,
}

/// How a drop-site search (`FUN_1411671e0`'s argument 9) measures the nearest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DropMeasure {
    /// By path (1): the path's length, of the objects the planner reaches.
    ByPath,
    /// By the squared distance of the centres (0).
    ByDistance,
}

/// Which drop sites a search counts (`FUN_1411671e0`'s argument 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SitePlace {
    /// Only those not inside another object (1).
    Outside,
    /// Any (0).
    Anywhere,
}

/// A drop-site search's arguments 10, 9 and 8 (`FUN_1411671e0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SiteSearch {
    /// Which drop sites count.
    pub(crate) place: SitePlace,
    /// How the nearest is measured.
    pub(crate) nearest: DropMeasure,
    /// The action type a drop site at work on it is passed over for (−1 none).
    pub(crate) at_work: Option<ActionKind>,
}

/// The point of a search from no point (`DAT_14330ec20`): at or below it on either axis the
/// search has no zone and goes by distance; at it on both axes the first drop site wins.
pub(crate) const NO_POINT: f32 = DROP_NO_POINT;

impl Seeker<'_, '_> {
    /// A fishing ship ([`ClassRoles::FISHING_SHIP`]) seeks in every zone and counts a drop site its path
    /// does not reach while near it.
    fn fishing(&self, roles: &Roles) -> bool {
        // The fishing ship's class (21), whose drop-site search passes over the zones.
        // The villager's class (4), which searches from the farm it works.
        roles
            .of_class(self.class)
            .contains(ClassRoles::FISHING_SHIP)
    }

    /// What `FUN_1411671e0` measures to the drop site `unit` (of `master`, its centre `squared`
    /// away), or none when the planner passes it over: the planner's result decides whether it
    /// counts ([`counted`]); by path its path's length, within `TurnTuning::short_drop_reach` the shorter
    /// of that and the short path (`TurnTuning::drop_reach`), kept in `last`; without a planner `last`;
    /// else `squared`.
    ///
    /// # Errors
    ///
    /// The planner's and the short path's.
    fn measure(
        &mut self,
        unit: &Unit,
        master: &Master,
        nearest: DropMeasure,
        squared: f32,
        last: &mut f32,
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<Option<f32>> {
        let tile_path_tuning = &by_id.tuning().tile_path;
        let turn_tuning = &by_id.tuning().turn;
        let roles = by_id.roles();
        let radii = Distance::new(self.mover.stats.radius.x() + master.radius.x());
        let to = unit.position().ground();
        let ground = self.mover.stats.radius.ground();
        let near = || {
            footprint_gap_squared(self.unit.position(), ground, unit.position(), master.radius)
                <= DistanceSquared::new(turn_tuning.fishing_reach)
        };
        let planned = self.mover.plan_length(
            self.unit.position(),
            self.spatial,
            self.plan,
            to,
            radii,
            tile_path_tuning,
        )?;
        Ok(match planned {
            Some((found, length)) if counted(found, nearest, roles.of_class(self.class), near) => {
                if nearest == DropMeasure::ByDistance {
                    return Ok(Some(squared));
                }
                *last = length.get();
                if length.get() <= turn_tuning.short_drop_reach
                    && let Some(short) = self.mover.short_reach(
                        self.unit.position(),
                        &self.me,
                        self.spatial,
                        unit.id,
                        Distance::new(turn_tuning.drop_reach),
                        by_id,
                    )?
                    && short < length
                {
                    *last = short.get();
                }
                Some(*last)
            }
            // the planner's other results pass the object over
            Some(_) => None,
            None if nearest == DropMeasure::ByPath => Some(*last),
            None => Some(squared),
        })
    }

    /// The place `FUN_1411671e0` searches from instead of the point: for a villager, the farm
    /// (class 49) its top action targets (slot 0x190), if that is one.
    ///
    /// # Errors
    ///
    /// The villager's top action is not modelled, or the farm's place is unknown.
    fn farm_worked(&self, roles: &Roles) -> GenieResult<Option<Point>> {
        if !roles.of_class(self.class).contains(ClassRoles::VILLAGER) {
            return Ok(None);
        }
        let Some(target) = self.actions.top_action_object(self.unread)? else {
            return Ok(None);
        };
        Ok(self
            .spatial
            .get_known(target)?
            .filter(|placed| roles.of_class(placed.class).contains(ClassRoles::FARM))
            .map(|placed| placed.position.ground()))
    }
}

/// Whether the planner's result `found` lets a drop site count in [`drop_site_search`]: a path
/// to it or into its range (below 2), or for a fishing ship (a seeker whose class `seeker`
/// holds [`ClassRoles::FISHING_SHIP`]) seeking by path one the planner does not reach (3, the
/// game's `3 <= result`) whose footprint lies `near` (`TurnTuning::fishing_reach`).
pub(crate) fn counted(
    found: Found,
    nearest: DropMeasure,
    seeker: ClassRoles,
    near: impl FnOnce() -> bool,
) -> bool {
    found.reaches()
        || (nearest == DropMeasure::ByPath
            && seeker.contains(ClassRoles::FISHING_SHIP)
            && found == Found::Nothing
            && near())
}

/// A search for drop sites (`FUN_1411671e0`: in state 2, not at work on the action type `at_work`
/// (its argument 8: an object of type 40 up whose top action is of that type, or `0x7c` too for
/// `0x7d`, as the Gather's `set_state` 8 and 3 call it; −1 none), not inside another when
/// `place` is [`SitePlace::Outside`]): of the player's live objects (`units`, in list order) of a
/// master in `sites`, the nearest to `at`. With the `seeker`, only objects in its zone count (its
/// restriction's zone of its tile against each object's tile) and only those the tile planner
/// reaches (its goal or its range; radii the seeker's and the object's, [`Mover::plan_length`]),
/// the nearest by path ([`Seeker::nearest`]): the path's length, within 10 tiles the shorter of
/// it and the seeker's short path (slot 0x3e8 within 0.1, [`Mover::short_reach`]); else by the
/// squared distance of the centres. Without a planner the game keeps the distance it last
/// measured (of an earlier object, else the largest float) by path. Without a seeker, by the
/// squared distance of the centres. A fishing ship (class 21) seeks in every zone, and by path a
/// drop site the planner does not reach (its result 3 or more) counts by its path's length too
/// while the footprints' squared gap (`FUN_14118f8c0`) is at most `TurnTuning::fishing_reach`. A villager
/// working a farm measures from the farm ([`Seeker::farm_worked`]), by the squared distance. The
/// point [`NO_POINT`] on both axes takes the first object of a master in `sites` in state 2; a
/// point at or below it on either axis has no zone and goes by the squared distance. The first
/// of equals wins.
pub(crate) fn drop_site_search<'world>(
    units: impl IntoIterator<Item = &'world Unit>,
    masters: OwnerMasters<'_>,
    sites: &[MasterId],
    at: Position,
    mut seeker: Option<Seeker<'_, '_>>,
    (place, at_work): (SitePlace, Option<ActionKind>),
    by_id: ObjectsRef<'_>,
) -> GenieResult<Option<DropSiteId>> {
    let roles = by_id.roles();
    let (mut x, mut y) = (at.x(), at.y());
    let pointless = x <= NO_POINT || y <= NO_POINT;
    let fishing = seeker.as_ref().is_some_and(|seeker| seeker.fishing(roles));
    let own_zone = seeker
        .as_ref()
        .filter(|_| !fishing && !pointless)
        .map(|seeker| seeker.zones.zone(Tile::new(x as i32, y as i32)));
    let mut nearest = seeker
        .as_ref()
        .filter(|_| !pointless)
        .map_or(DropMeasure::ByDistance, |seeker| seeker.nearest);
    let mut best: Option<DropSiteId> = None;
    let mut best_d = NO_DROP_DISTANCE;
    let mut last = NO_DROP_DISTANCE;
    for unit in units {
        if !sites.contains(&unit.life.master_id) || unit.life.state != ObjectState::Alive {
            continue;
        }
        if x == NO_POINT && y == NO_POINT {
            return Ok(Some(DropSiteId::new(unit.id)));
        }
        let master = unit.master_in(masters)?;
        if let (Some(zone_seeker), Some(zone)) = (seeker.as_ref(), own_zone)
            && zone_seeker.zones.zone(Tile::new(
                unit.position().x() as i32,
                unit.position().y() as i32,
            )) != zone
        {
            continue;
        }
        let busy = at_work.is_some_and(|at_work| {
            MasterKind::ACTION <= master.kind
                && by_id
                    .actions(unit.id)
                    .and_then(Action::top_action_kind)
                    .is_some_and(|action_kind| {
                        // The action types the Gather's drop-site searches pass over (`FUN_1411671e0`'s
                        // argument 8 `0x7d`, which takes `0x7c` with it).
                        action_kind == at_work
                            || (at_work == ActionKind::UNPACK && action_kind == ActionKind::PACK)
                    })
        });
        if busy || (place == SitePlace::Outside && unit.life.inside.is_some()) {
            continue;
        }
        if let Some(farm) = seeker
            .as_ref()
            .map(|roles_seeker| roles_seeker.farm_worked(roles))
            .transpose()?
            .flatten()
        {
            (x, y) = (farm.x(), farm.y());
            nearest = DropMeasure::ByDistance;
        }
        let (dx, dy) = (x - unit.position().x(), y - unit.position().y());
        let squared = dy * dy + dx * dx;
        let measured = match seeker.as_mut() {
            Some(seeker) => {
                match seeker.measure(unit, master, nearest, squared, &mut last, by_id)? {
                    Some(distance) => distance,
                    None => continue,
                }
            }
            None => squared,
        };
        if measured < best_d {
            best = Some(DropSiteId::new(unit.id));
            best_d = measured;
        }
    }
    Ok(best)
}

/// The distance a drop-site search starts from (`DAT_14330ebf4`, the largest float).
pub(crate) const NO_DROP_DISTANCE: f32 = TURN_NO_DROP_DISTANCE;

/// `FUN_1411671e0` with a point and no unit: of the player's live objects (`others`, in list
/// order) of a master in `sites` and in state 2, the one nearest `at` (its centre's squared
/// distance, the first of equals), with that distance.
pub(crate) fn nearest_drop_site(
    others: OthersRef<'_>,
    sites: &[MasterId],
    at: Position,
) -> Option<(DropSiteId, DistanceSquared)> {
    let mut best: Option<(DropSiteId, DistanceSquared)> = None;
    for unit in others.own_iter() {
        if !sites.contains(&unit.life.master_id) || unit.life.state != ObjectState::Alive {
            continue;
        }
        let (x, y) = (at.x(), at.y());
        let (dx, dy) = (x - unit.position().x(), y - unit.position().y());
        let gap = DistanceSquared::new(dy * dy + dx * dx);
        if best.is_none_or(|(_, distance_squared)| gap < distance_squared) {
            best = Some((DropSiteId::new(unit.id), gap));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GenieError;
    use crate::construction::build::BuildTask;
    #[cfg(test)]
    use crate::construction::build::tests::started;
    #[cfg(test)]
    use crate::gathering::hunt::tests::walker;
    #[cfg(test)]
    use crate::groups::UnitGroups;
    use crate::map::Footprint;
    use crate::map::Restriction;
    use crate::map::spatial::Placed;
    #[cfg(test)]
    use crate::map::{GridSize, TerrainId};
    use crate::math::rng::Well512;
    use crate::movement::TopSpeed;
    use crate::objects::ActionState;
    use crate::objects::WorkRate;
    use crate::objects::actions::ActionList;
    #[cfg(test)]
    use crate::objects::layers::Static;
    use crate::objects::lifecycle::Lifecycle;
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::Master;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::Masters;
    use crate::objects::masters::ObstructionKind;
    use crate::objects::masters::TaskId;
    #[cfg(test)]
    use crate::objects::masters::WorkValue;
    use crate::pathing::Pathing;
    use crate::pathing::zones::ZoneMap;
    use crate::players::PlayerId;
    use crate::players::PlayerKind;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::testing::class;
    use crate::testing::ids;

    use crate::world::WorldTime;
    #[cfg(test)]
    use crate::world::parts::{Object, no_own_masters};

    /// The town centre (master 109) and the mill (master 68), the villager's drop sites.
    const SITES: [MasterId; 2] = [MasterId::named(109), MasterId::named(68)];

    fn object(id: i32, master_id: MasterId, at: Point) -> Unit {
        Unit {
            id: object_id(id),
            place: Static::at(at.with_height(0.0)),
            life: Lifecycle {
                state: ObjectState::Alive,
                master_id,
                ..Lifecycle::default()
            },
            ..Unit::default()
        }
    }

    /// The player's town centre 700 at (20, 14) and mill 701 at (20, 25), in that order.
    fn sites() -> Vec<Unit> {
        vec![
            object(700, ids::TOWN_CENTRE, Point::new(20.0, 14.0)),
            object(701, master_id(68), Point::new(20.0, 25.0)),
        ]
    }

    fn masters() -> Masters {
        let master = |id, class, radius| Master {
            id,
            kind: MasterKind::BUILDING,
            class,
            radius: Footprint::new(radius, radius),
            ..Master::default()
        };
        Masters::new([
            master(ids::TOWN_CENTRE, class::BUILDING, 2.0),
            master(master_id(68), class::BUILDING, 1.0),
            master(ids::FARM, class::FARM, 1.5),
        ])
    }

    /// The villager 10 at (20, 20), building the object `target` when there is one.
    fn villager(target: Option<i32>) -> Object {
        let task = BuildTask {
            range: Distance::new(0.1),
            work_value: WorkValue::new(1.0),
            work_rate: WorkRate::new(1.0),
            resource: None,
            id: TaskId::of_task(-1),
            moving: None,
        };
        Object::with_actions(
            object(10, ids::VILLAGER_MALE, Point::new(20.0, 20.0)),
            Action {
                list: ActionList::default().with_build(target.map(|id| {
                    started(
                        object_id(id),
                        Position::new(20.0, 17.0, 0.0),
                        Distance::ZERO,
                        task,
                    )
                })),
                ..Default::default()
            },
        )
    }

    /// The map's objects: a farm 800 (class 49) at `farm` and a house 801 (class 3) at (20, 17).
    fn spatial(farm: Point) -> Spatial {
        let placed = |at: Point, class| Placed {
            position: at.with_height(0.0),
            radius: Footprint::new(1.5, 1.5),
            class,
            player: PlayerId::new(1),
            master: ids::FARM,
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        };
        let mut spatial = Spatial::default();
        spatial.insert(object_id(800), placed(farm, class::FARM));
        spatial.insert(
            object_id(801),
            placed(Point::new(20.0, 17.0), class::BUILDING),
        );
        spatial
    }

    /// The search over `units` from `at` with `seeker`'s unit (of `class`) as the seeker by path,
    /// on an open map without planners: by path, no object gets a length.
    fn search(
        units: &[Unit],
        seeker: &Object,
        class: ClassId,
        at: Position,
        spatial: &Spatial,
    ) -> GenieResult<Option<DropSiteId>> {
        let (unit, actions) = (&seeker.unit, &seeker.parts.actions);
        let mover = walker();
        // the search writes the obstruction manager's scratch: it runs on the fixture's copy
        let mut spatial = spatial.clone();
        let zones = ZoneMap::new(
            GridSize::new(40, 40),
            &[TerrainId::named(0); 1600],
            &Restriction::new(vec![1.0]),
        );
        let mut pathing = Pathing::default();
        let mut object = Well512::default();
        let groups = UnitGroups::default();
        let mut plan = Planning {
            pathing: &mut pathing,
            now: WorldTime::ZERO,
            player_kind: PlayerKind::Human,
            object: &mut object,
            groups: &groups,
        };
        let seeker = Seeker {
            unit,
            actions,
            unread: seeker.parts.unread.as_ref(),
            me: Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                action_state: ActionState::NO_ACTION,
                group: None,
                top_speed: TopSpeed::Own,
            },
            mover: &mover,
            plan: &mut plan,
            spatial: &mut spatial,
            zones: &zones,
            class,
            nearest: DropMeasure::ByPath,
        };
        drop_site_search(
            units,
            OwnerMasters::new(no_own_masters(), Some(&masters())),
            &SITES,
            at,
            Some(seeker),
            (SitePlace::Anywhere, None),
            ObjectsRef::empty(),
        )
    }

    /// `FUN_1411671e0`'s farm (slot 0x190 of a class-4 seeker on a class-49 object): a villager
    /// building or working a farm measures from the farm by the squared distance, not by path.
    /// From the villager the mill is the nearer; from the farm at (20, 17) the town centre; at
    /// (20, 19.5) both lie 5.5 away and the first in the list wins (428339149's farmers at 420).
    #[test]
    fn villager_working_farm_searches_from_the_farm_by_distance() {
        let at = Position::new(20.0, 20.0, 0.0);
        let near_tc = spatial(Point::new(20.0, 17.0));
        // by path without planners: no length, no site
        assert_eq!(
            search(&sites(), &villager(None), class::VILLAGER, at, &near_tc),
            Ok(None)
        );
        assert_eq!(
            search(
                &sites(),
                &villager(Some(800)),
                class::VILLAGER,
                at,
                &near_tc
            ),
            Ok(Some(DropSiteId::new(object_id(700))))
        );
        let tie = spatial(Point::new(20.0, 19.5));
        assert_eq!(
            search(&sites(), &villager(Some(800)), class::VILLAGER, at, &tie),
            Ok(Some(DropSiteId::new(object_id(700))))
        );
        let reversed: Vec<Unit> = sites().into_iter().rev().collect();
        assert_eq!(
            search(&reversed, &villager(Some(800)), class::VILLAGER, at, &tie),
            Ok(Some(DropSiteId::new(object_id(701))))
        );
        // another class, or a target of another class: by path as before
        assert_eq!(
            search(&sites(), &villager(Some(800)), class::SIEGE, at, &near_tc),
            Ok(None)
        );
        assert_eq!(
            search(
                &sites(),
                &villager(Some(801)),
                class::VILLAGER,
                at,
                &near_tc
            ),
            Ok(None)
        );
        // a farm the map does not hold: none
        assert_eq!(
            search(
                &sites(),
                &villager(Some(802)),
                class::VILLAGER,
                at,
                &near_tc
            ),
            Ok(None)
        );
        let mut unmodelled = villager(None);
        unmodelled.parts.unread = Some(Unread::new(GenieError::unsupported("an action")));
        assert!(search(&sites(), &unmodelled, class::VILLAGER, at, &near_tc).is_err());
    }

    /// The point (−1, −1) takes the first object of the masters in state 2, before the zones and
    /// the work filters; a point at or below −1 on one axis has no zone and goes by the squared
    /// distance.
    #[test]
    fn search_from_no_point_takes_the_first_site_and_one_below_it_goes_by_distance() {
        let map = spatial(Point::new(20.0, 17.0));
        let none = Position::new(NO_POINT, NO_POINT, 0.0);
        assert_eq!(
            search(&sites(), &villager(None), class::VILLAGER, none, &map),
            Ok(Some(DropSiteId::new(object_id(700))))
        );
        let mut units = sites();
        units[0].life.state = ObjectState::Foundation;
        assert_eq!(
            search(&units, &villager(None), class::VILLAGER, none, &map),
            Ok(Some(DropSiteId::new(object_id(701))))
        );
        // (−2, 30): by distance, the mill (22² + 5²) before the town centre (22² + 16²)
        let below = Position::new(-2.0, 30.0, 0.0);
        assert_eq!(
            search(&sites(), &villager(None), class::VILLAGER, below, &map),
            Ok(Some(DropSiteId::new(object_id(701))))
        );
        let left = Position::new(5.0, NO_POINT, 0.0);
        assert_eq!(
            search(&sites(), &villager(None), class::VILLAGER, left, &map),
            Ok(Some(DropSiteId::new(object_id(700))))
        );
    }
}
