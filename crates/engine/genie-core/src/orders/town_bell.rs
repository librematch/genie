//! A villager's town-bell record (`+0x9ac`..`+0x9dc`): what it worked on when an order to a
//! target took it off its work (`FUN_141206a20`), which a later back-to-work order sends it back
//! to, and the flag a garrison order sets (`FUN_1412067f0`). Saved as `mTownBellFlag`,
//! `mBackToWork1` and `mBackToWork2`.

use crate::garrison::WorkReturn;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::HUNT;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::image::garrison::NO_POINT as GARRISON_NO_POINT;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::MasterKind;
use crate::orders::Work;
use crate::orders::{GARRISON, own_order_target};
use crate::players::AllyTest;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::task_finder::Diplomacy;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::unit_ai::{AiMove, OrderForce, Pace};
use crate::world::Seconds;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::{ClassRoles, MasterRoles, Roles};
use crate::world::objects_ref::ObjectsRef;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

mod commands;

/// One back-to-work record.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct BackToWork {
    /// `townBellTargetId` (`+0x9ac`): the top action's target (the pasture of a target in one),
    /// else the target of the unit AI's first own queued order.
    pub(crate) target: Option<ObjectId>,
    /// `town_bell_target_type` (`+0x9b0`): a Gather's (5) or Hunt's (`0x6e`) target class
    /// (`+0x68`), else −1.
    pub(crate) target_class: Option<ClassId>,
    /// `town_bell_action` (`+0x9b4`): the unit AI's current action (`+0x44`).
    pub(crate) action: Option<AiAction>,
    /// `townBellTargetId2` (`+0x9b8`): the top action's second target (`+0x34`).
    pub(crate) target2: Option<ObjectId>,
    /// `town_bell_target_x`, `_y` (`+0x9bc`, `+0x9c0`): the target's place, else the unit's own.
    pub(crate) point: Point,
}

impl BackToWork {
    /// A record of nothing, as a new unit's (`FUN_14121df60`) and the second record a work other
    /// than building drops: no ids, the point (−1, −1).
    pub const NONE: Self = Self {
        target: None,
        target_class: None,
        action: None,
        target2: None,
        point: Point::new(-1.0, -1.0),
    };
}

/// `mTownBellFlag` (`+0x9dc`): whether the town bell's shelter holds an object. The bell's
/// ring sets it on a building that takes villagers, a garrison order into such a building on
/// the villager; the bell's end and the villager's next orders clear it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BellShelter {
    /// The flag is 0.
    #[default]
    Free,
    /// The flag is 1.
    Sheltered,
}

impl BellShelter {
    /// The flag's saved byte: any byte but 0 is set.
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        if value == 0 {
            Self::Free
        } else {
            Self::Sheltered
        }
    }

    /// Whether the flag is set.
    #[must_use]
    pub const fn is_sheltered(self) -> bool {
        matches!(self, Self::Sheltered)
    }
}

impl Diff for BellShelter {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

/// The town-bell flag and the two records of a combat object (type 70 and up).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct TownBell {
    /// The town-bell flag ([`BellShelter`]); no record is made while it is set.
    pub(crate) flag: BellShelter,
    /// `mBackToWork1` (`+0x9ac`..`+0x9c0`): the work it was taken off last.
    pub(crate) work: BackToWork,
    /// `mBackToWork2` (`+0x9c4`..`+0x9d8`): the work before a building it was taken off.
    pub(crate) before_building: BackToWork,
}

impl Default for TownBell {
    fn default() -> Self {
        Self {
            flag: BellShelter::Free,
            work: BackToWork::NONE,
            before_building: BackToWork::NONE,
        }
    }
}

impl TownBell {
    /// `FUN_141206a20`: with the flag clear, the unit's present work replaces the first record
    /// when any of its fields differs. The first record moves to the second when the new work is
    /// building (`0x25a`) and the old was not; any other new work clears the second (its ids −1,
    /// its point (−1, −1)). The new record ([`WorkReturn`]) is the caller's: its target, target
    /// class, the unit AI's action, the second target and the point.
    pub fn record(&mut self, now: BackToWork) {
        if self.flag.is_sheltered() || now == self.work {
            return;
        }
        if now.action == Some(AiAction::Build) {
            if self.work.action != Some(AiAction::Build) {
                self.before_building = self.work;
            }
        } else {
            self.before_building = BackToWork::NONE;
        }
        self.work = now;
    }
}

/// The unit AI's actions whose record a finished building ends: building (`0x25a`), gathering
/// (`0x261`) and hunting (`0x265`) (`FUN_1411f8fa0`'s mask `0x881`).
const ENDED_ON_BUILDING: [AiAction; 3] = [AiAction::Build, AiAction::Gather, AiAction::Hunt];

/// What a back-to-work record's check (`FUN_1411f8fa0`) reads of an object the world holds:
/// whether it is a building (type 80), its master's class, and its build points (`+0xc50`)
/// against its master's build time (the first train location's train time).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordTarget {
    /// The object.
    pub(crate) id: ObjectId,
    /// Its master is of type 80.
    pub(crate) building: bool,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// It has had its master's build time of work (`build_time <= +0xc50`).
    pub(crate) built: bool,
}

impl RecordTarget {
    /// Object `id` as a record names it, while `objects` holds it (`FUN_1415417a0`): its
    /// master's type and class, and whether it has had its master's build time of work.
    ///
    /// # Errors
    ///
    /// An object whose master its owner's table does not hold.
    pub fn of(objects: ObjectsRef<'_>, id: ObjectId) -> GenieResult<Option<Self>> {
        let Some((_, _, master)) = objects.with_master(id)? else {
            return Ok(None);
        };
        let build_time = Seconds::of_master(master.creatable_part().build_time);
        Ok(Some(Self {
            id,
            building: master.kind == MasterKind::BUILDING,
            class: master.class,
            built: objects
                .building(id)
                .and_then(|building| building.construction.as_ref())
                .is_some_and(|construction| build_time <= construction.progress),
        }))
    }
}

/// `FUN_1411f8fa0`: the object a record names, its target, else its second target, the world holds
/// ([`RecordTarget::of`]); for the actions of [`ENDED_ON_BUILDING`] a building other than a farm
/// that is built ends the record (no object, no targets in the copy returned). A negative id names
/// none (`JL`), as −1 does.
fn record_object(
    record: BackToWork,
    objects: ObjectsRef<'_>,
    roles: &Roles,
) -> GenieResult<(Option<RecordTarget>, BackToWork)> {
    let mut out = record;
    let named = |id: Option<ObjectId>| id.filter(|id| !id.is_recyclable());
    let mut found = match named(record.target) {
        Some(object_id) => RecordTarget::of(objects, object_id)?,
        None => None,
    };
    if found.is_none() {
        let Some(t2) = named(record.target2) else {
            return Ok((None, out));
        };
        found = RecordTarget::of(objects, t2)?;
    }
    if let Some(record_target) = found
        && record
            .action
            .is_some_and(|ai_action| ENDED_ON_BUILDING.contains(&ai_action))
        && record_target.building
        // The farms' class, whose records a built farm does not end.
        && !roles.of_class(record_target.class).contains(ClassRoles::FARM)
        && record_target.built
    {
        out.target = None;
        out.target2 = None;
        return Ok((None, out));
    }
    Ok((found, out))
}

impl TownBell {
    /// `FUN_1412035a0`: the work to go back to and the object it is on. The first record's
    /// object ([`record_object`]) wins; else the second's, which then becomes the first (the
    /// second cleared); else no object and the first record as it is, or the second when the
    /// first has no point. The record returned keeps the point of the one whose object was
    /// found.
    ///
    /// # Errors
    ///
    /// An object of a record whose master the world lacks.
    pub fn back_to_work(
        &mut self,
        objects: ObjectsRef<'_>,
        roles: &Roles,
    ) -> GenieResult<(Option<RecordTarget>, BackToWork)> {
        let (found, out) = record_object(self.work, objects, roles)?;
        if found.is_some() {
            return Ok((found, out));
        }
        let before = self.before_building;
        let (found, out) = record_object(before, objects, roles)?;
        if found.is_some() {
            self.work = before;
            self.before_building = BackToWork::NONE;
            return Ok((found, out));
        }
        let none = Point::new(-1.0, -1.0);
        if self.work.point == none {
            Ok((None, before))
        } else {
            Ok((None, self.work))
        }
    }
}

/// A record's point on neither axis (`DAT_14330ec20`).
const NO_POINT: f32 = GARRISON_NO_POINT;

impl World {
    /// `FUN_1412081e0`'s part for a unit outside a building (its `param_2` null), after the
    /// records' lookup: with its bell flag (`+0x9dc`) and the record's point not −1 on either
    /// axis (`UCOMISS`/`JP`/`JZ`: only an exact −1 stops it, a NaN goes on), it goes there: with
    /// no object to work on, slot 0x340 (`FUN_141216c50`, [`crate::world::World::move_aside`]) to
    /// the point on the ground, range 0, forced, not walking; else slot 0x7e8 on the object at
    /// the point with the retargeting byte 0 and any task, its `+0xa14` 0 from
    /// [`World::back_to_work`]. Then it is no longer under the town bell. [`WorkReturn::Went`]
    /// when it went.
    pub(crate) fn back_outside(
        &mut self,
        id: ObjectId,
        work_on: Option<ObjectId>,
        point: Point,
    ) -> GenieResult<WorkReturn> {
        let Some(Place::Live(up, ui)) = self.place(id) else {
            return Ok(WorkReturn::Stayed);
        };
        if !self.parts.town_bell_of(id).flag.is_sheltered() {
            return Ok(WorkReturn::Stayed);
        }
        let (x, y) = (point.x(), point.y());
        if x == NO_POINT || y == NO_POINT {
            return Ok(WorkReturn::Stayed);
        }
        match work_on {
            None => self.move_aside(
                up,
                ui,
                AiMove {
                    point: Position::new(x, y, 0.0),
                    range: Distance::ZERO,
                    force: OrderForce::Forced,
                    pace: Pace::Run,
                },
            )?,
            Some(target) => {
                self.order_work(up, ui, &Work::on(Some(target), Position::new(x, y, 0.0)))?;
            }
        }
        if self.object(id).is_some() {
            self.parts.set_town_bell_flag(id, BellShelter::Free);
        }
        Ok(WorkReturn::Went)
    }

    /// `FUN_1412067f0` before slot 0x7e8's work command, for a villager or master 1808 and a
    /// target the world holds: the town-bell record ([`World::record_town_bell`]); then, with the
    /// garrison filter (3), the flag goes on for a building in state 2 of the player or an ally
    /// whose master takes villagers
    /// ([`crate::objects::masters::GarrisonKinds::VILLAGERS`]), with any other filter it goes off.
    /// The UI's flag for the viewing player's building (`+0x1874`) is not held.
    pub(crate) fn town_bell_order(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        filter: Option<ActionKind>,
    ) -> GenieResult<()> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        if !(self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::VILLAGER)
            || self
                .model
                .roles
                .of_master(master.id)
                .contains(MasterRoles::MULE_CART))
        {
            return Ok(());
        }
        let Some((tp, target_unit)) = self.object(target) else {
            return Ok(());
        };
        let takes = if filter == Some(GARRISON) {
            let tm = target_unit.master(self.master_tables(), tp)?;
            let world = TaskWorld {
                players: &self.players,
                drop_sites: &self.model.drop_sites,
            };
            (tm.kind == MasterKind::BUILDING
                && target_unit.life.state == ObjectState::Alive
                && world.allied(player, tp, AllyTest::WithRelation4)
                && tm.building.is_some_and(|building_master| {
                    building_master
                        .garrison_kind
                        .contains(GarrisonKinds::VILLAGERS)
                }))
            .then_some(BellShelter::Sheltered)
        } else {
            Some(BellShelter::Free)
        };
        self.record_town_bell(player, idx)?;
        if let Some(flag) = takes {
            let id = self.players.live(player, idx)?.id;
            self.parts.set_town_bell_flag(id, flag);
        }
        Ok(())
    }

    /// `FUN_141206a20`: the unit's present work as its town-bell record
    /// ([`crate::orders::town_bell::TownBell::record`]): its top action's target (slot 400), or, for one
    /// in a pasture the world holds, the pasture; without one the target of its unit AI's first
    /// own queued order (`FUN_140c4da50`: issued by the unit, with a target), else none at its own
    /// place. Then a Gather's or Hunt's target class (`+0x68`), its unit AI's current action
    /// (`+0x44`) and the top action's second target (`FUN_140a78430`). The targets count only
    /// where the world holds them.
    pub(crate) fn record_town_bell(&mut self, player: PlayerId, idx: ListIndex) -> GenieResult<()> {
        use crate::orders::town_bell::BackToWork;
        let unit = &self.players.live(player, idx)?;
        if self.parts.town_bell_of(unit.id).flag.is_sheltered() {
            return Ok(());
        }
        if self.parts.unread.contains(unit.id) {
            return Err(GenieError::unsupported(
                "a town-bell record of an action not held",
            ));
        }
        // an id below 0 names none (`JL`)
        let held = |id: ObjectId| (!id.is_recyclable()).then(|| self.object(id)).flatten();
        let mut found = self
            .parts
            .actions_of(unit.id)
            .top_target()
            .and_then(held)
            .map(|(_, unit)| unit);
        if let Some(target) = found
            && let Some(pasture) = target.pasture
        {
            found = held(pasture.id()).map(|(_, unit)| unit);
        }
        if found.is_none() {
            let own = self
                .parts
                .unit_ai
                .get(unit.id)
                .and_then(|ai| own_order_target(ai, unit.id));
            found = own.and_then(held).map(|(_, held_unit)| held_unit);
        }
        let (target, point) = found.map_or_else(
            || (None, unit.position().ground()),
            |unit| (Some(unit.id), unit.position().ground()),
        );
        let actions = &self.parts.actions_of(unit.id);
        let target_class = match actions.top_action_kind() {
            Some(HUNT) => actions
                .list
                .hunt()
                .and_then(|hunt_action| hunt_action.target_class),
            Some(GATHER) => actions
                .list
                .gather()
                .and_then(|gather_action| gather_action.target_class),
            _ => None,
        };
        let ai = self
            .parts
            .unit_ai
            .get(unit.id)
            .ok_or(GenieError::unsupported(
                "a town-bell record of a unit without a unit AI",
            ))?;
        let target2 = actions
            .top_second_target()
            .and_then(held)
            .map(|(_, unit)| unit.id);
        let now = BackToWork {
            target,
            target_class,
            action: ai.current_action,
            target2,
            point,
        };
        let id = self.players.live(player, idx)?.id;
        let mut bell = self.parts.town_bell_of(id);
        bell.record(now);
        self.parts.set_town_bell(id, bell);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::construction::BuilderCount;
    use crate::construction::build::Construction;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Point;
    use crate::map::Snow;
    use crate::map::spatial::Obstruction;
    use crate::objects::MasterHitPoints;
    use crate::objects::ObjectId;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionKind;
    use crate::objects::masters::{CreatablePart, Master};
    use crate::test_ids::master_id;
    use crate::test_ids::{class_of, object_id};
    use crate::testing::class;
    use crate::testing::roles;
    use crate::testing::scene::{Scene, SceneObject};
    use crate::world::WholeSeconds;
    use crate::world::World;

    use super::*;

    /// Object `id`'s town bell in `world`.
    fn bell(world: &World, id: i32) -> TownBell {
        world.parts.town_bell_of(object_id(id))
    }

    /// Object `id`'s town bell in `world` changed by `change`.
    fn set_bell(world: &mut World, id: i32, change: impl FnOnce(&mut TownBell)) {
        let mut town_bell = bell(world, id);
        change(&mut town_bell);
        world.parts.set_town_bell(object_id(id), town_bell);
    }

    fn work(target: Option<ObjectId>, action: Option<AiAction>, point: Point) -> BackToWork {
        BackToWork {
            target,
            action,
            point,
            ..BackToWork::NONE
        }
    }

    #[test]
    fn new_work_replaces_the_record_and_building_keeps_the_one_before() {
        // MarvinBlue/426048731's 9181: its first record (no target, its own place), then the
        // INTERACT of update 748 while it builds 10028, then the one of 797 while it cuts a tree
        let mut bell = TownBell {
            work: work(None, None, Point::new(29.0, 99.0)),
            ..TownBell::default()
        };
        let building = work(
            Some(object_id(10028)),
            Some(AiAction::Build),
            Point::new(30.0, 88.0),
        );
        bell.record(building);
        assert_eq!(bell.work, building);
        assert_eq!(
            bell.before_building,
            work(None, None, Point::new(29.0, 99.0))
        );
        let cutting = BackToWork {
            target_class: class_of(15),
            ..work(
                Some(object_id(9365)),
                Some(AiAction::Hunt),
                Point::new(24.5, 97.5),
            )
        };
        bell.record(cutting);
        assert_eq!(bell.work, cutting);
        assert_eq!(bell.before_building, BackToWork::NONE);
        // the same work again changes nothing, nor does any work while the flag holds
        bell.before_building = building;
        bell.record(cutting);
        assert_eq!(bell.before_building, building);
        bell.flag = BellShelter::Sheltered;
        bell.record(work(
            Some(object_id(1)),
            Some(AiAction::Wait),
            Point::new(3.0, 4.0),
        ));
        assert_eq!(bell.work, cutting);
    }

    /// The objects of a test world: a house 7 that is built, a house 8 that is not, a farm 9
    /// that is built, a sheep 10.
    /// The world of the records' objects ([`object`]): Gaia's house 7 built (its 25 seconds of
    /// work), house 8 not (10 seconds), farm 9 built and sheep 10.
    fn records_world() -> World {
        let building = |id, class| Master {
            id: master_id(id),
            kind: MasterKind::BUILDING,
            class,
            radius: Footprint::new(1.0, 1.0),
            creatable: Some(CreatablePart {
                build_time: WholeSeconds::of_data(25),
                ..CreatablePart::default()
            }),
            ..Master::default()
        };
        let sheep = Master {
            id: master_id(594),
            kind: MasterKind::COMBAT,
            class: class::LIVESTOCK,
            ..Master::default()
        };
        let mut world = Scene::open(GridSize::new(16, 16))
            .object(SceneObject::new(
                7,
                PlayerId::GAIA,
                building(70, class::BUILDING),
                Position::new(2.0, 2.0, 0.0),
            ))
            .object(SceneObject::new(
                8,
                PlayerId::GAIA,
                building(70, class::BUILDING),
                Position::new(6.0, 2.0, 0.0),
            ))
            .object(SceneObject::new(
                9,
                PlayerId::GAIA,
                building(50, class::FARM),
                Position::new(10.0, 2.0, 0.0),
            ))
            .object(SceneObject::new(
                10,
                PlayerId::GAIA,
                sheep,
                Position::new(2.5, 8.5, 0.0),
            ))
            .build();
        for (id, progress) in [(7, 25.0), (8, 10.0), (9, 25.0)] {
            world
                .parts
                .building_mut(object_id(id))
                .unwrap()
                .construction = Some(Construction {
                progress: Seconds::new(progress),
                desolid: false,
                builders: BuilderCount::of_data(0),
                hit_points: MasterHitPoints::of_data(550),
                build_time: WholeSeconds::of_data(25),
                record: Obstruction {
                    kind: ObstructionKind::KIND_2,
                    class: ObstructionClass::BUILDING,
                },
                snow: Snow::None,
                sprite_override: false,
            });
        }
        world
    }

    /// The records' objects as the world holds them, each what [`object`] names.
    #[test]
    fn record_target_reads_master_kind_class_and_build_progress() {
        let records = records_world();
        for id in 7..=12 {
            assert_eq!(
                RecordTarget::of(records.objects(), object_id(id)),
                Ok(object(object_id(id))),
                "object {id}"
            );
        }
    }

    fn object(id: ObjectId) -> Option<RecordTarget> {
        let (building, class, built) = match id.get() {
            7 => (true, class::BUILDING, true),
            8 => (true, class::BUILDING, false),
            9 => (true, class::FARM, true),
            10 => (false, class::LIVESTOCK, false),
            _ => return None,
        };
        Some(RecordTarget {
            id,
            building,
            class,
            built,
        })
    }

    #[test]
    fn record_names_its_target_else_its_second_target_unless_built_building_ended_it() {
        let records = records_world();
        // FUN_1411f8fa0: the target the world holds
        let shepherd = BackToWork {
            target2: Some(object_id(7)),
            ..work(
                Some(object_id(10)),
                Some(AiAction::Gather),
                Point::new(5.0, 6.0),
            )
        };
        assert_eq!(
            record_object(shepherd, records.objects(), &roles()).unwrap(),
            (object(object_id(10)), shepherd)
        );
        // a target the world no longer holds: the second target, unless it is below 0
        let gone = BackToWork {
            target2: Some(object_id(10)),
            ..work(
                Some(object_id(11)),
                Some(AiAction::Hunt),
                Point::new(5.0, 6.0),
            )
        };
        assert_eq!(
            record_object(gone, records.objects(), &roles()).unwrap(),
            (object(object_id(10)), gone)
        );
        let none = BackToWork {
            target2: None,
            ..gone
        };
        assert_eq!(
            record_object(none, records.objects(), &roles()).unwrap(),
            (None, none)
        );
        let lost = BackToWork {
            target2: Some(object_id(12)),
            ..gone
        };
        assert_eq!(
            record_object(lost, records.objects(), &roles()).unwrap(),
            (None, lost)
        );
        // a target below 0 is not looked up: the second target is
        let second = BackToWork {
            target2: Some(object_id(10)),
            ..work(None, Some(AiAction::Build), Point::new(5.0, 6.0))
        };
        assert_eq!(
            record_object(second, records.objects(), &roles()).unwrap(),
            (object(object_id(10)), second)
        );
        // building, gathering and hunting (mask 0x881 over 0x25a..0x265) on a built building
        // other than a farm end the record: no object, both targets −1 in the copy
        for action in [AiAction::Build, AiAction::Gather, AiAction::Hunt].map(Some) {
            let on_house = BackToWork {
                target2: Some(object_id(10)),
                ..work(Some(object_id(7)), action, Point::new(5.0, 6.0))
            };
            let ended = BackToWork {
                target: None,
                target2: None,
                ..on_house
            };
            assert_eq!(
                record_object(on_house, records.objects(), &roles()).unwrap(),
                (None, ended)
            );
            // reached through the second target, too
            let through_second = BackToWork {
                target2: Some(object_id(7)),
                ..work(Some(object_id(11)), action, Point::new(5.0, 6.0))
            };
            let ended = BackToWork {
                target: None,
                target2: None,
                ..through_second
            };
            assert_eq!(
                record_object(through_second, records.objects(), &roles()).unwrap(),
                (None, ended)
            );
            // a house not built yet and a built farm stay
            for kept in [8, 9] {
                let kept_back_to_work = work(Some(object_id(kept)), action, Point::new(5.0, 6.0));
                assert_eq!(
                    record_object(kept_back_to_work, records.objects(), &roles()).unwrap(),
                    (object(object_id(kept)), kept_back_to_work)
                );
            }
        }
        // the actions between them, and any other, keep a built building
        for action in [
            Some(AiAction::Heal),
            Some(AiAction::Move),
            Some(AiAction::Follow),
            Some(AiAction::Transport),
            Some(AiAction::Defend),
            None,
        ] {
            let back_to_work = work(Some(object_id(7)), action, Point::new(5.0, 6.0));
            assert_eq!(
                record_object(back_to_work, records.objects(), &roles()).unwrap(),
                (object(object_id(7)), back_to_work)
            );
        }
    }

    mod rings {
        use crate::commands::Bell;
        #[cfg(test)]
        use crate::garrison::ContainerId;
        use crate::garrison::tests::{
            ARCHER, VILLAGER, inside_tc, let_out_world, sized_world, tracked, walker,
        };
        use crate::map::{Point, Position};
        use crate::objects::ObjectId;
        #[cfg(test)]
        use crate::orders::town_bell::BackToWork as TownBellBackToWork;
        use crate::orders::town_bell::BellShelter;
        use crate::orders::town_bell::tests::{bell, set_bell};
        use crate::players::PlayerId;
        use crate::test_ids::object_id;
        use crate::test_ids::player_id;
        use crate::world::World;
        use crate::world::settings::OptionFlags;

        fn enter(world: &World, id: i32) -> Option<ObjectId> {
            world
                .parts
                .actions_of(object_id(id))
                .list
                .enter()
                .and_then(|enter_action| enter_action.target)
                .map(ContainerId::id)
        }

        #[test]
        fn the_bell_sends_the_villagers_in_reach_to_the_buildings_in_reach_and_flags_them() {
            // FUN_14163a650: villager 1000 is within 25 tiles of town centre 100, 1001 is not,
            // archer 1002 is no villager; the centre takes villagers and is in reach of itself
            let mut world = sized_world(60, 60);
            walker(
                &mut world,
                PlayerId::new(1),
                1000,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            walker(
                &mut world,
                PlayerId::new(1),
                1001,
                VILLAGER,
                Point::new(45.5, 45.5),
            );
            walker(
                &mut world,
                PlayerId::new(1),
                1002,
                ARCHER,
                Point::new(6.5, 5.5),
            );
            world
                .town_bell(player_id(1), object_id(100), Bell::Ring)
                .unwrap();
            assert_eq!(
                [1000, 1001, 1002].map(|id| enter(&world, id)),
                [Some(object_id(100)), None, None]
            );
            assert!(bell(&world, 100).flag.is_sheltered());
        }

        #[test]
        fn the_bell_is_its_owners_or_any_players_with_allied_control() {
            let mut world = let_out_world();
            walker(
                &mut world,
                PlayerId::new(1),
                1000,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            // player 2 rings player 1's centre: refused
            world
                .town_bell(player_id(2), object_id(100), Bell::Ring)
                .unwrap();
            assert_eq!(enter(&world, 1000), None);
            world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
            world
                .town_bell(player_id(2), object_id(100), Bell::Ring)
                .unwrap();
            assert_eq!(enter(&world, 1000), Some(object_id(100)));
            // an object that is no town centre rings nothing; nor does an id the world lacks
            let mut world = let_out_world();
            walker(
                &mut world,
                PlayerId::new(1),
                1000,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            world
                .town_bell(player_id(1), object_id(1000), Bell::Ring)
                .unwrap();
            world
                .town_bell(player_id(1), object_id(4242), Bell::Ring)
                .unwrap();
            assert_eq!(enter(&world, 1000), None);
        }

        #[test]
        fn the_bells_end_lets_the_villagers_out_and_sends_the_others_back_by_the_tracker() {
            // FUN_14163ab80: the centre drops its flag and lets villager 1000 out toward its
            // record; villager 1001 outside, filed under class 4, walks back to its record's
            // point (slot 0x340); 1002 has no flag and stays; 1003 is not filed and stays
            let mut game = let_out_world();
            set_bell(&mut game, 100, |town_bell| {
                town_bell.flag = BellShelter::Sheltered;
            });
            inside_tc(&mut game, PlayerId::new(1), 1000, VILLAGER);
            walker(
                &mut game,
                PlayerId::new(1),
                1001,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            walker(
                &mut game,
                PlayerId::new(1),
                1002,
                VILLAGER,
                Point::new(6.5, 5.5),
            );
            walker(
                &mut game,
                PlayerId::new(1),
                1003,
                VILLAGER,
                Point::new(7.5, 5.5),
            );
            for (id, point) in [
                (1000, Point::new(4.5, 4.5)),
                (1001, Point::new(3.5, 3.5)),
                (1002, Point::new(2.5, 2.5)),
            ] {
                set_bell(&mut game, id, |town_bell| {
                    town_bell.flag = if id == 1002 {
                        BellShelter::Free
                    } else {
                        BellShelter::Sheltered
                    };
                    town_bell.work = TownBellBackToWork {
                        point,
                        ..TownBellBackToWork::NONE
                    };
                });
            }
            set_bell(&mut game, 1003, |town_bell| {
                town_bell.flag = BellShelter::Sheltered;
            });
            set_bell(&mut game, 1003, |town_bell| {
                town_bell.work.point = Point::new(1.5, 1.5);
            });
            tracked(&mut game, VILLAGER, &[1000, 1001, 1002]);
            game.town_bell(player_id(1), object_id(100), Bell::End)
                .unwrap();
            assert!(!bell(&game, 100).flag.is_sheltered());
            let object = game.object(object_id(1000)).unwrap().1;
            assert_eq!(
                (
                    object.life.inside,
                    game.parts.town_bell_of(object.id).flag.is_sheltered()
                ),
                (None, false)
            );
            let goal = |world: &World, id: i32| {
                let unit = world.object(object_id(id)).unwrap().1;
                (
                    world
                        .parts
                        .actions_of(object_id(id))
                        .list
                        .move_to()
                        .copied()
                        .and_then(|move_to_action| move_to_action.goal()),
                    world.parts.town_bell_of(unit.id).flag.is_sheltered(),
                )
            };
            assert_eq!(
                goal(&game, 1001),
                (Some(Position::new(3.5, 3.5, 0.0)), false)
            );
            assert_eq!(goal(&game, 1002), (None, false));
            assert_eq!(goal(&game, 1003), (None, true));
            // a record without a point on an axis (an exact -1) keeps the unit and its flag
            let mut world = let_out_world();
            walker(
                &mut world,
                PlayerId::new(1),
                1001,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            set_bell(&mut world, 1001, |town_bell| {
                town_bell.flag = BellShelter::Sheltered;
                town_bell.work.point = Point::new(3.5, -1.0);
            });
            tracked(&mut world, VILLAGER, &[1001]);
            world
                .town_bell(player_id(1), object_id(100), Bell::End)
                .unwrap();
            assert_eq!(goal(&world, 1001), (None, true));
        }
        #[test]
        fn all_back_to_work_lets_the_villagers_out_and_sends_the_others_back_leaving_the_flag() {
            // ALL_BACK_TO_WORK (do_command 0x8c, FUN_141617420): as the bell's end, but with no
            // bell flag to test or drop: villager 1000 comes out of the centre toward its
            // record, 1001 outside, filed under class 4, walks back to its record's point
            let world = || {
                let mut let_out_world = let_out_world();
                set_bell(&mut let_out_world, 100, |town_bell| {
                    town_bell.flag = BellShelter::Sheltered;
                });
                inside_tc(&mut let_out_world, PlayerId::new(1), 1000, VILLAGER);
                walker(
                    &mut let_out_world,
                    PlayerId::new(1),
                    1001,
                    VILLAGER,
                    Point::new(5.5, 5.5),
                );
                for (id, point) in [(1000, Point::new(4.5, 4.5)), (1001, Point::new(3.5, 3.5))] {
                    set_bell(&mut let_out_world, id, |town_bell| {
                        town_bell.flag = BellShelter::Sheltered;
                        town_bell.work = TownBellBackToWork {
                            point,
                            ..TownBellBackToWork::NONE
                        };
                    });
                }
                tracked(&mut let_out_world, VILLAGER, &[1000, 1001]);
                let_out_world
            };
            let mut game = world();
            game.all_back_to_work(player_id(1), object_id(100)).unwrap();
            assert!(bell(&game, 100).flag.is_sheltered());
            let object = game.object(object_id(1000)).unwrap().1;
            assert_eq!(
                (
                    object.life.inside,
                    game.parts.town_bell_of(object.id).flag.is_sheltered()
                ),
                (None, false)
            );
            let unit = game.object(object_id(1001)).unwrap().1;
            assert_eq!(
                (
                    game.parts
                        .actions_of(object_id(1001))
                        .list
                        .move_to()
                        .copied()
                        .and_then(|move_to_action| move_to_action.goal()),
                    game.parts.town_bell_of(unit.id).flag.is_sheltered()
                ),
                (Some(Position::new(3.5, 3.5, 0.0)), false)
            );
            // another player's command does nothing without the game's cheats
            let mut world = world();
            world
                .all_back_to_work(player_id(2), object_id(100))
                .unwrap();
            assert_eq!(
                world.object(object_id(1000)).unwrap().1.life.inside,
                Some(ContainerId::new(object_id(100)))
            );
        }
    }

    #[test]
    fn back_to_work_takes_the_first_record_else_the_second_which_moves_up() {
        let records = records_world();
        // FUN_1412035a0: the first record's object wins; the records stay
        let first = work(
            Some(object_id(10)),
            Some(AiAction::Gather),
            Point::new(5.0, 6.0),
        );
        let second = work(
            Some(object_id(8)),
            Some(AiAction::Build),
            Point::new(1.0, 2.0),
        );
        let mut bell = TownBell {
            flag: BellShelter::Free,
            work: first,
            before_building: second,
        };
        assert_eq!(
            bell.back_to_work(records.objects(), &roles()).unwrap(),
            (object(object_id(10)), first)
        );
        assert_eq!((bell.work, bell.before_building), (first, second));
        // the first's object gone: the second's object, and the second becomes the first
        let gone = work(
            Some(object_id(11)),
            Some(AiAction::Gather),
            Point::new(5.0, 6.0),
        );
        bell.work = gone;
        assert_eq!(
            bell.back_to_work(records.objects(), &roles()).unwrap(),
            (object(object_id(8)), second)
        );
        assert_eq!(
            (bell.work, bell.before_building),
            (second, BackToWork::NONE)
        );
        // neither has an object: the first record as it is, its targets kept even when a built
        // building ended it
        let ended = work(
            Some(object_id(7)),
            Some(AiAction::Build),
            Point::new(5.0, 6.0),
        );
        let mut bell = TownBell {
            flag: BellShelter::Free,
            work: ended,
            before_building: work(
                Some(object_id(12)),
                Some(AiAction::Gather),
                Point::new(1.0, 2.0),
            ),
        };
        assert_eq!(
            bell.back_to_work(records.objects(), &roles()).unwrap(),
            (None, ended)
        );
        assert_eq!(bell.work, ended);
        // ... or the second when the first has no point (−1, −1 on both axes)
        let pointless = work(
            Some(object_id(11)),
            Some(AiAction::Gather),
            Point::new(-1.0, -1.0),
        );
        let before = work(
            Some(object_id(12)),
            Some(AiAction::Gather),
            Point::new(1.0, 2.0),
        );
        let mut bell = TownBell {
            flag: BellShelter::Free,
            work: pointless,
            before_building: before,
        };
        assert_eq!(
            bell.back_to_work(records.objects(), &roles()).unwrap(),
            (None, before)
        );
        // one axis at −1 is a point
        let half = work(
            Some(object_id(11)),
            Some(AiAction::Gather),
            Point::new(-1.0, 3.0),
        );
        bell.work = half;
        assert_eq!(
            bell.back_to_work(records.objects(), &roles()).unwrap(),
            (None, half)
        );
        assert_eq!((bell.work, bell.before_building), (half, before));
    }
}
