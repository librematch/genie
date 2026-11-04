//! `FORM_FORMATION`'s handler (`FUN_14161dba0`): the listed units are split into groups, which
//! lock to the formation the command names.

use crate::GenieError;
use crate::groups::FarMembers;
use crate::groups::FormationKind;
use crate::groups::GroupId;
use crate::groups::common_group;
use crate::objects::ObjectId;
use crate::orders::ControlKey;
use crate::orders::Kept;
use crate::orders::Split;
use crate::orders::{LISTED, set_differentiator_groups};
use crate::players::PlayerId;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::index::Place;

impl World {
    /// `FORM_FORMATION` (`FUN_14161dba0`) of the units `ids` of `player` (the handler's vector:
    /// a unit the world does not hold is a null object) and the formation type `formation`. The
    /// ids pass the check STOP's twin makes ([`World::command_check`]: refused for a human when
    /// all went). Each unit with a unit AI whose order is a defend (`0x2bd`) or a follow
    /// (`0x2c8`) first has its ignore list emptied (`FUN_141197da0`), stops (slot 0x148,
    /// [`World::object_stop`]) and stands (slot 0x560). The order splitter ([`World::split`],
    /// no Ctrl, no filter) splits them; each of its groups with units (60 at most listed) is:
    /// - the group all its units are in (`FUN_14152ece0`), when the world holds it and it has
    ///   just them, kept by the splitter ([`World::keep_group`], no target); one the world does
    ///   not hold takes nothing;
    /// - else a new group of the split's owner that takes the units ([`World::add_to_group`]),
    ///   after each of which the unit of the group's own index (the game reads the unit at the
    ///   split's group number, not the unit's) is stopped as above, its statistics
    ///   ([`World::group_statistics`]) and its place ([`World::set_initial_position`]).
    ///
    /// The group takes the command count as its differentiator and locks to the formation
    /// ([`World::lock_formation`]). Then every listed group lists them all as its differentiator
    /// groups (`setDifferentiatorGroups`, `FUN_1410395a0`) and the command count goes up. The
    /// interface flag the game sets for the local player (`FUN_141663f50`) is no world state.
    ///
    /// # Errors
    ///
    /// What the stops, the groups, the statistics and the formation do not model.
    pub(crate) fn form_formation(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
        formation: FormationKind,
    ) -> Result<(), GenieError> {
        let Some(units) = self.command_check(player, ids)? else {
            return Ok(());
        };
        for &id in units.iter().flatten() {
            self.stop_marcher(id)?;
        }
        let mut split = self.split(&units, Kept::All, ControlKey::Up)?;
        let mut listed: Vec<GroupId> = Vec::new();
        for (number, group_units) in split.groups.clone().iter().enumerate() {
            if group_units.is_empty() {
                continue;
            }
            let common = common_group(group_units, |id| self.object(id).map(|(_, u)| u.group));
            let id = if let Some(common) = common {
                match self.groups.get(common) {
                    None => continue,
                    Some(g) if g.members.len() != group_units.len() => {
                        self.form_group(&split, group_units, number)?
                    }
                    Some(_) => {
                        self.keep_group(&mut split, common, None)?;
                        common
                    }
                }
            } else {
                self.form_group(&split, group_units, number)?
            };
            if listed.len() < LISTED {
                listed.push(id);
            }
            let commands = self.groups.commands;
            if let Some(g) = self.groups.get_mut(id) {
                g.differentiator_id = commands;
            }
            self.lock_formation(id, formation)?;
        }
        for &id in &listed {
            if let Some(g) = self.groups.get_mut(id) {
                set_differentiator_groups(g, &listed);
            }
        }
        self.groups.count_command();
        Ok(())
    }

    /// A new group of the split's owner with `units`, the split's group number `number`: each
    /// unit joins ([`World::add_to_group`]) and the unit at `number` in `units` (none past
    /// them: the game reads the splitter's stale slots there, which the slice does not hold) is
    /// stopped if it marches ([`World::stop_marcher`]); then the statistics and the place.
    /// Its id.
    ///
    /// # Errors
    ///
    /// What the joins, the stop and the statistics do not model.
    fn form_group(
        &mut self,
        split: &Split,
        units: &[ObjectId],
        number: usize,
    ) -> Result<GroupId, GenieError> {
        let owner = split.owner;
        let id = self.groups.create(owner);
        for &unit in units {
            let _ = self.add_to_group(id, unit)?;
            if let Some(&at) = units.get(number) {
                self.stop_marcher(at)?;
            }
        }
        self.group_statistics(id, FarMembers::Stay)?;
        self.set_initial_position(id)?;
        Ok(id)
    }

    /// A unit with a unit AI whose order is a defend (`0x2bd`) or a follow (`0x2c8`,
    /// `FUN_140c4cc20`): its ignore list empties (`FUN_141197da0`), it stops (slot 0x148) and
    /// stands (slot 0x560). Any other, one the world does not hold live, nothing.
    ///
    /// # Errors
    ///
    /// What the stop does not model.
    fn stop_marcher(&mut self, id: ObjectId) -> Result<(), GenieError> {
        let Some(Place::Live(p, idx)) = self.place(id) else {
            return Ok(());
        };
        let marching = self.parts.unit_ai.get(id).is_some_and(|ai| {
            matches!(
                ai.current_order,
                Some(OrderKind::Defend | OrderKind::Follow)
            )
        });
        if !marching {
            return Ok(());
        }
        if self.live_object(p, id).is_some()
            && let Some(m) = self.parts.mover_mut(id)
        {
            m.ignore_units.clear();
        }
        self.object_stop(p, idx)?;
        if self.live_object(p, id).is_some()
            && let Some(m) = self.parts.mover_mut(id)
        {
            m.stand()?;
        }
        Ok(())
    }
}
