//! The world's unit groups ([`UnitGroups`]), an object's link to its group and the group
//! every listed unit is in.

use crate::GenieError;
use crate::groups::{GroupId, UnitGroup};
use crate::objects::{ObjectId, ObjectKey};
use crate::players::PlayerId;
use genie_diff::Diff;

/// An object's link to its group: `mGroupID` (`+0x6d0`) and `mGroupFlag` (`+0x6d4`), -1 and -2
/// from the object's setup (`FUN_141196df0`) and in every object of a save in no group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Diff)]
pub struct GroupLink {
    /// `mGroupID`: the group's id, none for -1.
    pub id: Option<GroupId>,
    /// `mGroupFlag`.
    pub flag: i32,
}

impl Default for GroupLink {
    fn default() -> Self {
        Self { id: None, flag: -2 }
    }
}

/// The world's unit groups (`mNumberUnitGroups`, `mUnitGroupCounter`, `FUN_141536ef0`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitGroups {
    /// The groups in the world list's order.
    pub groups: Vec<UnitGroup>,
    /// The id the next group takes (`mUnitGroupCounter`, `world+0x5c`).
    pub counter: GroupId,
    /// The commands that ran the order splitter so far (`DAT_143f10134`, a process global no
    /// save holds): each group a command forms or orders takes it as `mDifferentiatorID`, then it
    /// counts up. A group's load raises it past the group's (`FUN_14101d350`).
    pub commands: crate::groups::Differentiator,
    /// The scratch array of a group's unit types (`DAT_143f23b70`, 50 words, a process global):
    /// each count of a sub-formation's unit types (`FUN_1410116c0`) writes its types from the
    /// front and copies the whole array into the group's `mUnitTypes`, so the words past the
    /// types are an earlier count's. Empty is all zero, as the process starts.
    pub unit_types_scratch: Vec<crate::groups::UnitType>,
}

impl UnitGroups {
    /// The long pather's list for a unit in group `group` (`FUN_140f9eb70`: `+0x6d0`, the group
    /// by id `FUN_14152ff40`, then `FUN_14100bbe0` with 255): None for no group (-1) or one the
    /// world does not hold.
    #[must_use]
    pub fn group_units(&self, group: Option<GroupId>) -> Option<Vec<ObjectId>> {
        /// The long pather's list holds this many (`0xff`).
        const LOWERED: usize = 255;
        self.get(group?)
            .map(|g| self.differentiator_units(g, LOWERED))
    }

    /// `FUN_14100bbe0` of group `group` with `cap`: the members of each group of its
    /// differentiator groups the world holds (by id), in order, at most `cap`.
    #[must_use]
    pub fn differentiator_units(&self, group: &UnitGroup, cap: usize) -> Vec<ObjectId> {
        let mut out = Vec::new();
        for &id in &group.differentiator_groups {
            let Some(g) = self.get(id) else {
                continue;
            };
            for m in &g.members {
                out.push(m.id);
                if cap <= out.len() {
                    return out;
                }
            }
        }
        out
    }

    /// A command ran the order splitter to its end (`DAT_143f10134` up by one): MOVE
    /// (`FUN_140e67800`) every time, INTERACT (`FUN_140e68b90`) with a unit and a single unit
    /// held, FOLLOW (`FUN_14161c920`) with its target, BUILD (`FUN_140e7a400`) past its
    /// placement tests, after the groups took the old count as their differentiator; the shelter
    /// garrison (`FUN_141621220`) once per group its splitter makes. WORK (2), PATROL (0x15,
    /// 0x21, 0x22, `FUN_14161b970`), `FORM_FORMATION` (0x17, `FUN_14161dba0`), command 0x1f
    /// (`FUN_140e67150`) and REPAIR (0x6e, `FUN_140e7da90`) count too; make-way
    /// (`FUN_140f88750`) would, but returns at once while `DAT_143f01836` is 0, as nothing
    /// writes it.
    pub const fn count_command(&mut self) {
        self.commands = self.commands.next();
    }

    /// The group of id `id` (`FUN_14152ff40`; a link's −1, which it finds none for, is none
    /// before the call).
    #[must_use]
    pub fn get(&self, id: GroupId) -> Option<&UnitGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// The group of id `id` that the caller holds: its layout reads it again where the world's
    /// spatial index is written, which a borrow of the group would block.
    ///
    /// # Errors
    ///
    /// A group the table does not hold.
    pub fn held(&self, id: GroupId) -> Result<&UnitGroup, GenieError> {
        self.get(id).ok_or_else(|| group_gone(id))
    }

    /// The group of id `id` that the caller holds, to change.
    ///
    /// # Errors
    ///
    /// A group the table does not hold.
    pub fn held_mut(&mut self, id: GroupId) -> Result<&mut UnitGroup, GenieError> {
        self.get_mut(id).ok_or_else(|| group_gone(id))
    }

    /// The group of id `id`, to change (`FUN_14152ff40`).
    pub fn get_mut(&mut self, id: GroupId) -> Option<&mut UnitGroup> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    /// A new group at the world list's end with the next id (`FUN_14152efc0` with 1: the
    /// constructor, `FUN_141039640`'s id, then the count and the counter up by one); its state
    /// is the constructor's ([`UnitGroup::new`]) and `owner`'s, which the order's handler sets
    /// (`+0xdc`). Answers its id.
    pub fn create(&mut self, owner: Option<PlayerId>) -> GroupId {
        let id = self.counter;
        self.counter = id.successor();
        let mut g = UnitGroup::new(id);
        g.owner = owner;
        self.groups.push(g);
        id
    }
}

/// A group that holds no current formation where the game reads one.
pub(crate) const NO_FORMATION: GenieError =
    GenieError::unreachable("a group without a current formation where one is read");

/// A formation slot the formation does not hold.
pub(crate) fn slot_gone(slot: usize) -> GenieError {
    GenieError::unreachable("a slot the formation does not hold").with("slot", slot)
}

/// A group's member that the world does not hold, or that has no movement state.
pub(crate) const NO_MOVER: GenieError =
    GenieError::unreachable("a group's member is a unit of type 70 or more, which moves");

/// A member of a group that the world does not hold.
pub(crate) fn member_gone(id: impl ObjectKey) -> GenieError {
    let gone = GenieError::unreachable("a member of a group that the world does not hold");
    match id.key() {
        Some(id) => gone.with("object", id),
        None => gone.with("object", "none"),
    }
}

/// A group the caller holds that the world does not.
fn group_gone(id: GroupId) -> GenieError {
    GenieError::unreachable("a group the caller holds that the world does not hold")
        .with("group", id)
}

/// The group that every listed unit is in (`FUN_14152ece0`), from each unit's group link
/// (`link_of`, None for a unit the world does not hold, which is passed over, as a null entry
/// is): none for an empty list, for a unit in no group, for two groups, and when the first unit
/// is gone (the others are then compared with no group).
pub fn common_group<K: ObjectKey>(
    ids: &[K],
    link_of: impl Fn(ObjectId) -> Option<GroupLink>,
) -> Option<GroupId> {
    let first = ids.first().map(|&id| id.key())?;
    let first_held = first.and_then(&link_of).is_some();
    let mut common = None;
    for &id in ids {
        let id = id.key();
        let Some(link) = id.and_then(&link_of) else {
            continue;
        };
        let group = link.id?;
        if id == first && first_held {
            common = Some(group);
        } else if common != Some(group) {
            return None;
        }
    }
    common
}
