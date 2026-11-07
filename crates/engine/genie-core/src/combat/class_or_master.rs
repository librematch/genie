//! The object a combat task names ([`ClassOrMaster`]): a class or a master, as slot 277 keeps it in
//! the aura groups and the stinger tasks.

use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::Masters;
use crate::objects::masters::Task;

/// The offset of a class among a task's targets (`FUN_140fdde00`, `ADD 900`): a target from 900
/// to 999 names the class 900 below it (`FUN_140fde260`, `FUN_140fe0f60`), any other a master.
const CLASS_TARGETS: i32 = 900;
/// How many classes the targets name (`FUN_140fde260`: the target less 900 below 100,
/// unsigned).
const CLASS_TARGET_SPAN: u32 = 100;

/// The object a combat task names (an `int`: slot 277's entry `+0x0` of an aura group and of a
/// stinger task, and an active stinger's key `+0xc`): a class (its id plus 900) or a master (its
/// id), −1 (any value below 0) for any.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, genie_diff::Diff)]
pub struct ClassOrMaster(i32);

impl ClassOrMaster {
    /// The target of a task (slot 277): its class (`object_group`, `+0x10`) when it has one, else
    /// its master (`object_id`, `+0x12`), else any (−1).
    pub(crate) fn of_task(task: &Task) -> Self {
        match (task.object_group, task.object_id) {
            (Some(class), _) => Self::of_class(class),
            (None, Some(master)) => Self::of_master(master),
            (None, None) => Self(-1),
        }
    }

    /// Class `class` (`FUN_140fdde00`).
    pub(crate) fn of_class(class: ClassId) -> Self {
        Self(i32::from(class.get()) + CLASS_TARGETS)
    }

    /// Master `master`.
    pub(crate) fn of_master(master: MasterId) -> Self {
        Self(i32::from(master.get()))
    }

    /// The save's integer `value` (an active stinger's `+0xc`).
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The integer, as the save holds it.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// The class a target from 900 to 999 names (`FUN_140fde260`, `FUN_140fe0f60`); None for a
    /// master or any.
    pub(crate) fn class(self) -> Option<ClassId> {
        let class = self.0.wrapping_sub(CLASS_TARGETS);
        (class.cast_unsigned() < CLASS_TARGET_SPAN).then(|| ClassId::of_master(class as i16))
    }

    /// The class the aura's query reads for this target (slot 277): a class target's own, a
    /// master target's master's class in the object's player's table `masters` (`player+0x28`);
    /// None for a master the table has not got (or any), whose task then joins no group.
    pub(crate) fn query_class(self, masters: Option<&Masters>) -> Option<ClassId> {
        if let Some(class) = self.class() {
            return Some(class);
        }
        let master = i16::try_from(self.0).ok().and_then(MasterId::of_data)?;
        masters?.get(master).map(|target| target.class)
    }

    /// A speed-up building's test of an object of master `master` and class `class`
    /// (`FUN_1411c4a00`): a class target names its class (a 16-bit compare), any other its
    /// master's id; no value names any.
    pub(crate) fn targets(self, class: ClassId, master: MasterId) -> bool {
        self.class().map_or_else(
            || i32::from(master.get()) == self.0,
            |target_class| target_class == class,
        )
    }

    /// The hand-off's test of an object of master `master` and class `class` (`FUN_14107bdf0`):
    /// any target names it (below 0), a class target its class (a 16-bit compare), a master
    /// target its master's id.
    pub(crate) fn names(self, class: ClassId, master: MasterId) -> bool {
        if self.0 < 0 {
            return true;
        }
        self.class().map_or_else(
            || i32::from(master.get()) == self.0,
            |target_class| target_class == class,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::class_or_master::ClassOrMaster;
    use crate::objects::masters::ClassId;
    use crate::objects::masters::Master;
    use crate::objects::masters::MasterId;
    use crate::objects::masters::Masters;
    use crate::testing::class;

    /// A class target is its class plus 900; a master from 900 to 999 reads as the class 900 below
    /// it, as the game's test of the range does.
    #[test]
    fn class_or_master_names_class_by_900_and_else_master() {
        assert_eq!(ClassOrMaster::of_class(class::VILLAGER), ClassOrMaster(904));
        assert_eq!(ClassOrMaster(904).class(), Some(class::VILLAGER));
        assert_eq!(
            ClassOrMaster::of_master(MasterId::named(950)).class(),
            Some(ClassId::named(50))
        );
        assert_eq!(
            ClassOrMaster::of_master(MasterId::named(1000)).class(),
            None
        );
        assert_eq!(ClassOrMaster(-1).class(), None);
        let masters = Masters::new([Master {
            id: MasterId::named(83),
            class: class::VILLAGER,
            ..Master::default()
        }]);
        assert_eq!(
            ClassOrMaster::of_master(MasterId::named(83)).query_class(Some(&masters)),
            Some(class::VILLAGER)
        );
        assert_eq!(
            ClassOrMaster::of_master(MasterId::named(84)).query_class(Some(&masters)),
            None
        );
        assert_eq!(ClassOrMaster(-1).query_class(Some(&masters)), None);
    }

    /// The hand-off's test: below 0 names any object, a class target the objects of its class,
    /// a master target the objects of that master.
    #[test]
    fn hand_off_test_names_any_class_or_master() {
        let (villager, master) = (class::VILLAGER, MasterId::named(83));
        assert!(ClassOrMaster(-1).names(villager, master));
        assert!(ClassOrMaster(-7).names(villager, master));
        assert!(ClassOrMaster::of_class(villager).names(villager, MasterId::named(99)));
        assert!(!ClassOrMaster::of_class(villager).names(ClassId::named(6), master));
        assert!(ClassOrMaster::of_master(master).names(ClassId::named(6), master));
        assert!(!ClassOrMaster::of_master(master).names(villager, MasterId::named(84)));
        assert_eq!(ClassOrMaster::of_data(904).get(), 904);
    }
}
