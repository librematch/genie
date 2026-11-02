//! A building's test whether it takes a unit in (`FUN_14120eb10`), its room and what it holds.

use crate::GenieError;
use crate::garrison::ContainerId;
use crate::garrison::InsideCount;
use crate::garrison::{CLASS_FLAGS, ClassFlags, LEAST_HP_SHARE};
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
use crate::objects::masters::class;
use crate::objects::masters::ids::MASTER_1808;
use crate::objects::masters::{Task, player_master};
use crate::objects::unit::Unit;
use crate::orders::GARRISON;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::Diplomacy;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::world::World;

/// `FUN_1411205e0`: whether a master's class (`+0x410`, the short `MOVSX` widens) has one of
/// `flags` in the class-flags table: a class below 0 or not below the table's count
/// (`CMP`/`JGE`, signed) has none.
#[must_use]
pub fn class_has_flags(class: ClassId, flags: ClassFlags) -> bool {
    usize::try_from(class.get())
        .ok()
        .and_then(|c| CLASS_FLAGS.get(c))
        .is_some_and(|&f| ClassFlags::from_bits_retain(f).intersects(flags))
}

/// The unit masters whose own rules `FUN_14120eb10` keeps apart from their class's (the game
/// compares their ids, + 0x609c).
pub(super) const MASTER_1263: MasterId = MasterId::named(1263);
pub(super) const MASTER_775: MasterId = MasterId::named(775);
pub(super) const MASTER_1715: MasterId = MasterId::named(1715);
pub(super) const MASTER_1811: MasterId = MasterId::named(1811);
pub(super) const MASTER_1822: MasterId = MasterId::named(1822);

/// Whether `garrison_type` (a building master's `+0x9bc`) takes in a unit of `class` and master
/// `id` (the switch of `FUN_14120eb10` on the class, [`GarrisonKinds`]'s bits; infantry not of
/// masters 1263, 1811, monks not of masters 775, 1715, 1822); then cavalry for masters 1263,
/// 775, 1715 and 1822 and monks for masters 1811 and 1808, whatever their class. The task
/// finder's copy of the switch (`FUN_1412784d0`) leaves class 42 out (`with_42` false).
#[must_use]
pub fn garrison_type_takes(
    garrison_type: GarrisonKinds,
    class: ClassId,
    id: MasterId,
    with_42: bool,
) -> bool {
    let bit = |kind| garrison_type.contains(kind);
    let by_class = match class {
        class::CLASS_0 | class::CLASS_44 => bit(GarrisonKinds::INFANTRY),
        class::TRADE_BOAT
        | class::TRANSPORT
        | class::FISHING_SHIP
        | class::WARSHIP
        | class::CLASS_53 => bit(GarrisonKinds::SHIPS),
        class::VILLAGER | class::TRADE_CART | class::KING => bit(GarrisonKinds::VILLAGERS),
        class::CLASS_6
        | class::CLASS_35
        | class::CLASS_45
        | class::CLASS_46
        | class::CLASS_50
        | class::CLASS_56 => id != MASTER_1263 && id != MASTER_1811 && bit(GarrisonKinds::INFANTRY),
        class::CLASS_12 | class::CLASS_23 | class::CLASS_36 | class::CLASS_47 | class::CLASS_57 => {
            bit(GarrisonKinds::CAVALRY)
        }
        class::SIEGE | class::CLASS_51 | class::CLASS_54 | class::CLASS_55 => {
            bit(GarrisonKinds::SIEGE)
        }
        class::MONK | class::CLASS_43 => {
            id != MASTER_775 && id != MASTER_1715 && id != MASTER_1822 && bit(GarrisonKinds::MONKS)
        }
        class::RELIC if with_42 => {
            id != MASTER_775 && id != MASTER_1715 && id != MASTER_1822 && bit(GarrisonKinds::MONKS)
        }
        class::LIVESTOCK => bit(GarrisonKinds::HERDABLES),
        _ => false,
    };
    let as_cavalry = matches!(id, MASTER_1263 | MASTER_775 | MASTER_1715 | MASTER_1822)
        && bit(GarrisonKinds::CAVALRY);
    let as_monk = matches!(id, MASTER_1811 | MASTER_1808) && bit(GarrisonKinds::MONKS);
    by_class || as_cavalry || as_monk
}

/// `FUN_1415c9240`: the master's task list holds a task of action type `action` on objects of
/// class `class`, both bytes the game widens to compare with the task's ushorts (`+0xe`,
/// `+0x10`). A building's test asks for 3 on its own class (a transport) or on buildings (3).
pub(super) fn has_garrison_task(tasks: &[Task], action: ActionKind, class: ClassId) -> bool {
    let byte = |v: i16| u16::from(v.to_le_bytes()[0]);
    tasks.iter().any(|t| {
        t.action_kind.get().cast_unsigned() == byte(action.get())
            && ClassId::field(t.object_group).cast_unsigned() == byte(class.get())
    })
}

/// A garrison task (action type 3) on buildings (class 3).
#[must_use]
pub fn is_garrison_task(task: &Task) -> bool {
    task.action_kind == GARRISON && task.object_group == Some(class::BUILDING)
}

impl World {
    /// Slot 0x198 of a building (`FUN_1412169b0`) `b` of player `bp` for `unit`: its count
    /// ([`World::garrisoned`]) plus the unit and what it holds (1 and its own count; 1 for no
    /// unit) within its master's capacity (`obj_max`).
    pub(crate) fn has_room(
        &self,
        bp: PlayerId,
        b: &Unit,
        unit: Option<ObjectId>,
    ) -> Result<bool, GenieError> {
        let capacity = b.master(self.master_tables(), bp)?.garrison;
        let with = match unit {
            Some(u) => self.garrisoned(u)?.plus(InsideCount::ONE),
            None => InsideCount::ONE,
        };
        Ok(self.garrisoned(b.id)?.plus(with).within(capacity))
    }
}

impl World {
    /// `FUN_14120eb10(building, player, master, 1, 1)` ([`World::takes_in_as`]).
    pub(crate) fn takes_in(
        &self,
        building: ContainerId,
        player: PlayerId,
        master: Option<MasterId>,
    ) -> Result<bool, GenieError> {
        self.takes_in_as(building, player, master, true, true)
    }

    /// `FUN_14120eb10`: whether `building` takes a unit of `player`'s master `master` in. The
    /// player must have the master; another player's building must be allowed (`others`, the
    /// game's `param_4` 1) and allied both ways (`FUN_141663d90`); the building in state 2. A
    /// building whose `object_flags` bit 0 is set (a transport) takes a unit of type 70 or of a
    /// class with a flag of 0x10002 ([`class_has_flags`]) when the unit has a task of action 3 on
    /// its own class ([`has_garrison_task`]); a unit of neither goes on as for a building. Any
    /// other takes a unit of type 70 or more when it is of type 80
    /// (an annex asks its main building, `+0xbb8`), above [`LEAST_HP_SHARE`] of its master's hit
    /// points, with a capacity (`obj_max`), the unit's master with a garrison task on buildings
    /// and of a kind the building's `garrison_type` takes ([`garrison_type_takes`]). Either then
    /// needs room for one more (slot 0x198 with no unit, [`World::has_room`]) unless `unchecked`
    /// (`param_5` not 0).
    pub(crate) fn takes_in_as(
        &self,
        building: ContainerId,
        player: PlayerId,
        master: Option<MasterId>,
        others: bool,
        unchecked: bool,
    ) -> Result<bool, GenieError> {
        let Some(unit) = master.and_then(|master| player_master(&self.masters, player, master))
        else {
            return Ok(false);
        };
        let Some((bp, b)) = self.object(building) else {
            return Ok(false);
        };
        if bp != player {
            let world = TaskWorld {
                players: &self.players,
                drop_sites: &self.drop_sites,
            };
            if !others || !world.allied(bp, player, true) || !world.allied(player, bp, true) {
                return Ok(false);
            }
        }
        if b.life.state != ObjectState::Alive {
            return Ok(false);
        }
        let own = b.master(self.master_tables(), bp)?;
        if own.object_flags.contains(ObjectFlags::GARRISON)
            && (unit.kind == MasterKind::COMBAT
                || class_has_flags(unit.class, ClassFlags::TRANSPORTED))
        {
            if !has_garrison_task(&unit.tasks, GARRISON, own.class) {
                return Ok(false);
            }
            return Ok(unchecked || self.has_room(bp, b, None)?);
        }
        if unit.kind < MasterKind::COMBAT || own.kind != MasterKind::BUILDING {
            return Ok(false);
        }
        let (bp, b) = match self.parts.building_of(b.id).annex_of {
            Some(main) => self.object(main).ok_or(GenieError::unsupported(
                "an annex whose main building is gone",
            ))?,
            None => (bp, b),
        };
        let bm = b.master(self.master_tables(), bp)?;
        // CVTPS2PD, CVTDQ2PD, MULSD, then COMISD with JBE: an unordered hit point count takes
        // nothing
        let max_hp = f64::from(HitPoints::of_master(bm.hit_points).get());
        let above = f64::from(b.life.hp.get()) > max_hp * LEAST_HP_SHARE;
        if !above || !bm.garrison.holds() {
            return Ok(false);
        }
        if !has_garrison_task(&unit.tasks, GARRISON, class::BUILDING) {
            return Ok(false);
        }
        let garrison_type = bm.building.map_or(GarrisonKinds::NONE, |m| m.garrison_type);
        if !garrison_type_takes(garrison_type, unit.class, unit.id, true) {
            return Ok(false);
        }
        Ok(unchecked || self.has_room(bp, b, None)?)
    }

    /// Slot 0x498 of the object `id`: the objects inside it (`+0x2f8`'s count, relics too) and
    /// what its own `+0x9a8` (`inside_garrison_count`, a saved byte) adds (`FUN_141213500`); a
    /// building (`FUN_1411d0ce0`) less its `+0xc2c`, the units of type 70 that came in with a
    /// running `raider_build_countdown` (`+0x9a0`, `FUN_1411cb850`/`FUN_1411d6ad0`). Not
    /// modelled: an object inside that holds others (the slice reads no `+0x9a8`) and, in a
    /// building, a unit with its countdown set (`+0xc2c`, which the slice does not keep).
    pub(crate) fn garrisoned(&self, building: ObjectId) -> Result<InsideCount, GenieError> {
        let holds = |id: ObjectId| {
            self.players.iter().any(|q| {
                q.objects
                    .iter()
                    .chain(&q.sleeping)
                    .any(|v| v.life.inside.key() == Some(id))
            })
        };
        let is_building = self
            .object(building)
            .map(|(bp, b)| {
                b.master(self.master_tables(), bp)
                    .map(|m| m.kind == MasterKind::BUILDING)
            })
            .transpose()?
            .unwrap_or(false);
        let mut count = 0;
        for (p, player) in PlayerId::each(self.players.len()).zip(&self.players) {
            for u in player.objects.iter().chain(&player.sleeping) {
                if u.life.inside.key() != Some(building) {
                    continue;
                }
                let kind = u.master(self.master_tables(), p)?.kind;
                if is_building
                    && kind == MasterKind::COMBAT
                    && self.parts.combat_of(u.id).raider_countdown.is_set()
                {
                    return Err(GenieError::unsupported(
                        "a unit inside a building with its raider_build_countdown (+0xc2c)",
                    ));
                }
                if holds(u.id) {
                    return Err(GenieError::unsupported(
                        "an object inside a building that holds others (+0x9a8)",
                    ));
                }
                count += 1;
            }
        }
        Ok(InsideCount::new(count))
    }
}
