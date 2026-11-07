//! A building's test whether it takes a unit in (`FUN_14120eb10`), its room and what it holds.

use crate::GenieResult;
use crate::construction::MAIN_BUILDING_GONE;
use crate::garrison::ContainerId;
use crate::garrison::InsideCount;
use crate::garrison::{CLASS_FLAGS, ClassFlags};
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::{Task, player_master};
use crate::objects::unit::Unit;
use crate::orders::GARRISON;
use crate::players::AllyTest;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::Diplomacy;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;

/// `FUN_1411205e0`: whether a master's class (`+0x410`, the short `MOVSX` widens) has one of
/// `flags` in the class-flags table: a class below 0 or not below the table's count
/// (`CMP`/`JGE`, signed) has none.
#[must_use]
pub fn class_has_flags(class: ClassId, flags: ClassFlags) -> bool {
    usize::try_from(class.get())
        .ok()
        .and_then(|class| CLASS_FLAGS.get(class))
        .is_some_and(|&bits| ClassFlags::from_bits_retain(bits).intersects(flags))
}

/// Which copy of the garrison types' switch [`garrison_kind_takes`] runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GarrisonSwitch {
    /// The garrison's (`FUN_14120eb10`), which takes the relics (class 42) in.
    Garrison,
    /// The task finder's (`FUN_1412784d0`), which leaves them out.
    TaskFinder,
}

/// Whether `garrison_type` (a building master's `+0x9bc`) takes in a unit of `class` and master
/// `id` (the switch of `FUN_14120eb10` on the class, [`GarrisonKinds`]'s bits; infantry not of
/// masters 1263, 1811, monks not of masters 775, 1715, 1822); then cavalry for masters 1263,
/// 775, 1715 and 1822 and monks for masters 1811 and 1808, whatever their class. The task
/// finder's copy of the switch (`FUN_1412784d0`) leaves class 42 out ([`GarrisonSwitch`]).
#[must_use]
pub fn garrison_kind_takes(
    garrison_kind: GarrisonKinds,
    class: ClassId,
    id: MasterId,
    switch: GarrisonSwitch,
    roles: &Roles,
) -> bool {
    let bit = |kind| garrison_kind.contains(kind);
    let by_class = {
        let held = roles.of_class(class);
        if held.intersects(ClassRoles::ARCHERY | ClassRoles::ARCHERY_CANNON) {
            bit(GarrisonKinds::INFANTRY)
        } else if held.intersects(
            ClassRoles::TRADE_BOAT
                | ClassRoles::TRANSPORT
                | ClassRoles::FISHING_SHIP
                | ClassRoles::WARSHIP
                | ClassRoles::BOARDING_SHIP,
        ) {
            bit(GarrisonKinds::SHIPS)
        } else if held.intersects(ClassRoles::VILLAGER | ClassRoles::TRADE_CART | ClassRoles::KING)
        {
            bit(GarrisonKinds::VILLAGERS)
        } else if held.intersects(
            ClassRoles::INFANTRY
                | ClassRoles::PETARD
                | ClassRoles::TWO_HANDED_SWORDSMAN
                | ClassRoles::PIKEMAN
                | ClassRoles::SPEARMAN
                | ClassRoles::RAIDER,
        ) {
            !roles.of_master(id).contains(MasterRoles::FLAMING_CAMEL)
                && !roles.of_master(id).contains(MasterRoles::WARRIOR_PRIEST)
                && bit(GarrisonKinds::INFANTRY)
        } else if held.intersects(
            ClassRoles::CAVALRY
                | ClassRoles::CAVALRY_CANNON
                | ClassRoles::CAVALRY_ARCHER
                | ClassRoles::SCOUT_CAVALRY
                | ClassRoles::CAVALRY_RAIDER,
        ) {
            bit(GarrisonKinds::CAVALRY)
        } else if held.intersects(
            ClassRoles::SIEGE
                | ClassRoles::PACKED_TREBUCHET
                | ClassRoles::UNPACKED_TREBUCHET
                | ClassRoles::SCORPION,
        ) {
            bit(GarrisonKinds::SIEGE)
        } else if held.intersects(ClassRoles::MONK | ClassRoles::MONK_WITH_RELIC)
            || (held.contains(ClassRoles::RELIC) && switch == GarrisonSwitch::Garrison)
        {
            !roles.of_master(id).contains(MasterRoles::MISSIONARY)
                && !roles.of_master(id).contains(MasterRoles::JADWIGA)
                && !roles.of_master(id).contains(MasterRoles::TAMAR)
                && bit(GarrisonKinds::MONKS)
        } else if held.contains(ClassRoles::LIVESTOCK) {
            bit(GarrisonKinds::HERDABLES)
        } else {
            false
        }
    };
    let as_cavalry = roles.of_master(id).intersects(
        MasterRoles::FLAMING_CAMEL
            | MasterRoles::MISSIONARY
            | MasterRoles::JADWIGA
            | MasterRoles::TAMAR,
    ) && bit(GarrisonKinds::CAVALRY);
    let as_monk = roles
        .of_master(id)
        .intersects(MasterRoles::WARRIOR_PRIEST | MasterRoles::MULE_CART)
        && bit(GarrisonKinds::MONKS);
    by_class || as_cavalry || as_monk
}

/// `FUN_1415c9240`: the master's task list holds a task of action type `action` on objects of
/// class `class`, both bytes the game widens to compare with the task's ushorts (`+0xe`,
/// `+0x10`). A building's test asks for 3 on its own class (a transport) or on buildings (3).
pub(super) fn has_garrison_task(tasks: &[Task], action: ActionKind, class: ClassId) -> bool {
    let byte = |value: i16| u16::from(value.to_le_bytes()[0]);
    tasks.iter().any(|task| {
        task.action_kind.get().cast_unsigned() == byte(action.get())
            && ClassId::field(task.object_group).cast_unsigned() == byte(class.get())
    })
}

/// A garrison task (action type 3) on buildings (class 3).
#[must_use]
pub fn is_garrison_task(task: &Task, roles: &Roles) -> bool {
    task.action_kind == GARRISON
        && roles
            .of_some_class(task.object_group)
            .contains(ClassRoles::BUILDING)
}

impl World {
    /// Slot 0x198 of a building (`FUN_1412169b0`) `building` of player `bp` for `unit`: its count
    /// ([`World::garrisoned`]) plus the unit and what it holds (1 and its own count; 1 for no
    /// unit) within its master's capacity (`obj_max`).
    pub(crate) fn has_room(
        &self,
        bp: PlayerId,
        building: &Unit,
        unit: Option<ObjectId>,
    ) -> GenieResult<bool> {
        let capacity = building.master(self.master_tables(), bp)?.garrison;
        let with = unit.map_or(InsideCount::ONE, |object_id| {
            self.garrisoned(object_id).plus(InsideCount::ONE)
        });
        Ok(self.garrisoned(building.id).plus(with).within(capacity))
    }
}

impl World {
    /// `FUN_14120eb10(building, player, master, 1, 1)` ([`World::takes_in_as`]).
    pub(crate) fn takes_in(
        &self,
        building: ContainerId,
        player: PlayerId,
        master: Option<MasterId>,
    ) -> GenieResult<bool> {
        self.takes_in_as(building, player, master, true, true)
    }

    /// `FUN_14120eb10`: whether `building` takes a unit of `player`'s master `master` in. The
    /// player must have the master; another player's building must be allowed (`others`, the
    /// game's `param_4` 1) and allied both ways (`FUN_141663d90`); the building in state 2. A
    /// building whose `object_flags` bit 0 is set (a transport) takes a unit of type 70 or of a
    /// class with a flag of 0x10002 ([`class_has_flags`]) when the unit has a task of action 3 on
    /// its own class ([`has_garrison_task`]); a unit of neither goes on as for a building. Any
    /// other takes a unit of type 70 or more when it is of type 80
    /// (an annex asks its main building, `+0xbb8`), above `GarrisonTuning::least_hp_share` of its master's hit
    /// points, with a capacity (`obj_max`), the unit's master with a garrison task on buildings
    /// and of a kind the building's `garrison_type` takes ([`garrison_kind_takes`]). Either then
    /// needs room for one more (slot 0x198 with no unit, [`World::has_room`]) unless `unchecked`
    /// (`param_5` not 0).
    pub(crate) fn takes_in_as(
        &self,
        building: ContainerId,
        player: PlayerId,
        master: Option<MasterId>,
        others: bool,
        unchecked: bool,
    ) -> GenieResult<bool> {
        let Some(unit) =
            master.and_then(|master| player_master(&self.model.masters, player, master))
        else {
            return Ok(false);
        };
        let Some((bp, building_unit)) = self.object(building) else {
            return Ok(false);
        };
        if bp != player {
            let world = TaskWorld {
                players: &self.players,
                drop_sites: &self.model.drop_sites,
            };
            if !others
                || !world.allied(bp, player, AllyTest::WithRelation4)
                || !world.allied(player, bp, AllyTest::WithRelation4)
            {
                return Ok(false);
            }
        }
        if building_unit.life.state != ObjectState::Alive {
            return Ok(false);
        }
        let own = building_unit.master(self.master_tables(), bp)?;
        if own.object_flags.contains(ObjectFlags::GARRISON)
            && (unit.kind == MasterKind::COMBAT
                || class_has_flags(unit.class, ClassFlags::TRANSPORTED))
        {
            if !has_garrison_task(&unit.action_part().tasks, GARRISON, own.class) {
                return Ok(false);
            }
            return Ok(unchecked || self.has_room(bp, building_unit, None)?);
        }
        if unit.kind < MasterKind::COMBAT || own.kind != MasterKind::BUILDING {
            return Ok(false);
        }
        let (bp, building) = match self.parts.building_of(building_unit.id).annex_of {
            Some(main) => self.object(main).ok_or(MAIN_BUILDING_GONE)?,
            None => (bp, building_unit),
        };
        let bm = building.master(self.master_tables(), bp)?;
        // CVTPS2PD, CVTDQ2PD, MULSD, then COMISD with JBE: an unordered hit point count takes
        // nothing
        let max_hp = f64::from(HitPoints::of_master(bm.hit_points).get());
        let above =
            f64::from(building.life.hp.get()) > max_hp * self.model.tuning.garrison.least_hp_share;
        if !above || !bm.garrison.holds() {
            return Ok(false);
        }
        if !has_garrison_task(
            &unit.action_part().tasks,
            GARRISON,
            self.model.slots.class.building,
        ) {
            return Ok(false);
        }
        let garrison_kind = bm.building.map_or(GarrisonKinds::NONE, |building_master| {
            building_master.garrison_kind
        });
        if !garrison_kind_takes(
            garrison_kind,
            unit.class,
            unit.id,
            GarrisonSwitch::Garrison,
            &self.model.roles,
        ) {
            return Ok(false);
        }
        Ok(unchecked || self.has_room(bp, building, None)?)
    }

    /// Slot 0x498 of the object `id`: the objects inside it (`FUN_141193110`, `+0x2f8`'s count,
    /// relics too; every type's), what they hold (`FUN_141213500`: its `+0x9a8`,
    /// `inside_garrison_count`, [`crate::garrison::NestedCount`], types 70 and 80) less a
    /// building's raiders (`FUN_1411d0ce0`: its `+0xc2c`, the units of type 70 that came in with
    /// their `raider_build_countdown` set, [`crate::garrison::GarrisonCounts`]). An object below
    /// type 70 holds neither column (0: only a container of type 70 or more counts what its units
    /// hold, only a building its raiders), so the one sum serves every type. It counts the objects
    /// that name `id` as their container (`+0x2f0`), which its list holds, so it counts a
    /// container taken out of its list while it updates too.
    pub(crate) fn garrisoned(&self, id: ObjectId) -> InsideCount {
        let inside = self
            .players
            .iter()
            .flat_map(|player| player.objects.iter().chain(&player.sleeping))
            .filter(|unit| unit.life.inside.key() == Some(id))
            .count();
        // a list's count is an int (`+0x2f8`'s `+8`)
        InsideCount::new(inside as i32)
            .plus(self.parts.combat_of(id).nested.count())
            .minus(self.parts.building_of(id).garrison_counts.raiders)
    }
}
