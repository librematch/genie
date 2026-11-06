//! INTERACT and the order splitter: which units move loose and which form groups.

use crate::map::Distance;
use crate::map::Point;
use crate::math::fmath::{at_most_or_unordered, sqrt};
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::orders::ControlKey;
use crate::orders::OrderFlags;
use crate::orders::SplitPath;
use crate::orders::{Interact, Split, Splittable, centroid};
use crate::pathing::tile_path::Found;
use crate::pathing::tile_path::{Path, Request};
use crate::players::PlayerId;
use crate::world::Formations;
use crate::world::World;
use crate::world::layer::generated::OrderTuning;
use crate::world::slots::CheckedIndexExt;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};
use std::collections::BTreeSet;

/// Which units of an order the splitter keeps after its sort ([`World::split`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kept<'world> {
    /// Every unit.
    All,
    /// The units listed: the garrison command's test (`FUN_140fd9ae0` for command `0x2713`).
    Listed(&'world [ObjectId]),
}

impl Kept<'_> {
    /// Whether the splitter keeps unit `id`.
    fn keeps(self, id: ObjectId) -> bool {
        match self {
            Self::All => true,
            Self::Listed(ids) => ids.contains(&id),
        }
    }
}

impl World {
    /// INTERACT (`0x140e68b90`) to its units, after their unit AIs reset. The splitter
    /// ([`World::split`]) forms groups ([`World::interact_groups`]) and leaves the others loose;
    /// several loose units of a human player whose box ([`World::order_spread`]) is narrow
    /// enough take places around the order's point at their offsets from their centroid (the
    /// loop of `LAB_140e6920a`), any other each the point itself (the loop of `LAB_140e69a3c`),
    /// in the splitter's order ([`World::interact_unit`]). A single unit the world does not hold
    /// takes nothing.
    pub(crate) fn interact(
        &mut self,
        player: PlayerId,
        units: &[Option<ObjectId>],
        order: &Interact,
    ) -> GenieResult<()> {
        match units {
            [] => return Ok(()),
            &[id] if self.place(id).is_none() => return Ok(()),
            _ => {}
        }
        let loose = self
            .split(units, Kept::All, ControlKey::Up)
            .and_then(|mut split| {
                self.interact_groups(&mut split, order)
                    .map(|()| split.loose)
            });
        self.groups.count_command();
        let loose = loose?;
        self.leave_groups(player, &loose, order.target)?;
        // FUN_140d61870: a player whose lobby entry is of type 4, which a human's is not
        if 1 < loose.len() && self.players.player(player)?.kind.is_computer() {
            return Err(GenieError::unsupported(
                "an order to several units of a computer player (FUN_140d61870)",
            ));
        }
        let centroid = if 1 < loose.len() {
            self.order_spread(&loose, order.flags.contains(OrderFlags::QUEUED))?
        } else {
            None
        };
        for (index, &id) in loose.iter().enumerate() {
            let others: Vec<ObjectId> = loose
                .iter()
                .enumerate()
                .filter(|&(other, _)| other != index)
                .map(|(_, &object_id)| object_id)
                .collect();
            self.interact_unit(player, id, &others, centroid, order)?;
        }
        Ok(())
    }

    /// The units of an order that move alone ([`World::split`] without Ctrl held); a group it
    /// makes is not modelled.
    pub(crate) fn loose_units(&mut self, units: &[Option<ObjectId>]) -> GenieResult<Vec<ObjectId>> {
        self.loose_units_kept(units, Kept::All)
    }

    /// [`World::loose_units`] where, after the sort, `kept` drops units (the garrison command's
    /// test, `FUN_140fd9ae0` for command `0x2713`).
    pub(crate) fn loose_units_kept(
        &mut self,
        units: &[Option<ObjectId>],
        kept: Kept<'_>,
    ) -> GenieResult<Vec<ObjectId>> {
        let split = self.split(units, kept, ControlKey::Up)?;
        if !split.groups.is_empty() {
            return Err(GenieError::unsupported(
                "a unit group the order splitter forms",
            ));
        }
        Ok(split.loose)
    }

    /// The order splitter (`FUN_140fd9dc0` -> `FUN_140fd9ae0` -> `FUN_140fd76d0`): it sorts the
    /// units by id (`FUN_140fd7280` with `LAB_140fd89b0`), drops those `kept` refuses (the
    /// garrison command's test, `FUN_140fd9ae0` for command `0x2713`), takes at most 60 and drops
    /// an id the next one repeats. An order whose first unit is gone splits into nothing
    /// (`FUN_140fd9ae0` reads the owner of the first).
    ///
    /// Then, in this order (each pass in the list's order, the loose units in the order the
    /// passes leave them):
    /// - a unit in a group the world holds (`+0x6d0`, `FUN_14152ff40`) takes the order's other
    ///   members of that group (`FUN_1410112b0`): two or more are a group of the split, one alone
    ///   moves loose;
    /// - without the owner's auto formations (`+0x6c0`) every other unit moves loose;
    /// - a unit above type 69 whose master's formation category is 0 (villagers) moves loose (the
    ///   dock test `FUN_141158210` is 1 only on an `E` tile in game type 12);
    /// - of the rest, the unit with the most others that join it ([`splits_together`]), the first
    ///   of equals, takes those that join it and are in a state of 2 or more: two or more are a
    ///   group, one alone moves loose; with Ctrl held (`control`) it is a group of its own.
    ///
    /// The split keeps at most 60 groups of at most 60 units: a group past them is dropped with
    /// its units. An order to more than 60 units splits its sorted list, the gone units in it, in
    /// runs of 60, each on its own into the one split (`FUN_140fd9ae0`'s `FixedMemoryResource`
    /// of 60): an id the next run repeats is in both.
    pub(crate) fn split(
        &mut self,
        units: &[Option<ObjectId>],
        kept: Kept<'_>,
        control: ControlKey,
    ) -> GenieResult<Split> {
        // FUN_140fd9ae0 splits only when the list's first unit exists (its owner): else no unit
        // takes the order
        let Some((owner, _)) = units.first().and_then(|&id| self.object(id)) else {
            return Ok(Split::default());
        };
        // FUN_140fd9dc0's objects, null for a gone id; LAB_140fd89b0 sorts a null as -1, and
        // equal ids are one object, so any sort gives the game's order
        let mut sorted: Vec<Option<ObjectId>> =
            units.iter().map(|&id| self.object(id).and(id)).collect();
        sorted.sort_unstable_by_key(|object_id| object_id.map_or(-1, ObjectId::get));
        for object_id in &mut sorted {
            if object_id.is_some_and(|id| !kept.keeps(id)) {
                *object_id = None;
            }
        }
        let mut split = Split {
            owner: Some(owner),
            ..Split::default()
        };
        for run in sorted.chunks(60) {
            self.split_run(owner, run, control, &mut split)?;
        }
        Ok(split)
    }

    /// One run of at most 60 of the order splitter (`FUN_140fd76d0`) into `split`: a null entry
    /// and one the run's next entry repeats are left out.
    pub(super) fn split_run(
        &mut self,
        owner: PlayerId,
        run: &[Option<ObjectId>],
        control: ControlKey,
        split: &mut Split,
    ) -> GenieResult<()> {
        let left = self.splittables(run)?;
        let mut left = self.keep_world_groups(left, split)?;
        let formations = self.players.player(owner)?.formations;
        if !formations.auto {
            split
                .loose
                .extend(left.iter().map(|splittable| splittable.id));
            return Ok(());
        }
        // units above type 69 of formation category 0 stay loose (the dock test FUN_141158210 is
        // 1 only on an `E` tile in game type 12)
        let (loose, rest): (Vec<Splittable>, Vec<Splittable>) = left
            .into_iter()
            .partition(|splittable| MAX_UNGROUPED < splittable.kind && !splittable.grouped);
        split
            .loose
            .extend(loose.into_iter().map(|splittable| splittable.id));
        left = rest;
        // the path between two near units: the tile planner of the second's restriction, from
        // it to the first (FUN_140f7c530, its result and the measured path's length)
        let tiles = self.spatial.tiles();
        let pathing = &mut self.pathing;
        let mut path =
            |target: &Splittable, walker: &Splittable| -> GenieResult<Option<(Found, Distance)>> {
                let Some(restriction) = walker.restriction else {
                    return Ok(None);
                };
                let Some(planner) = pathing.planner(restriction, tiles)? else {
                    return Ok(None);
                };
                let req = Request {
                    from: walker.position.ground(),
                    to: target.position.ground(),
                    radii: target.radius,
                    range: walker.radius,
                    closed: Vec::new(),
                    full: false,
                };
                let mut path = Path::default();
                let found = planner.plan::<GenieError>(
                    &req,
                    false,
                    &mut path,
                    None,
                    &self.model.tuning.tile_path,
                )?;
                path.measure();
                Ok(Some((found, path.length)))
            };
        // then the unit with the most others that join it, the first of equals, with them
        while !left.is_empty() {
            let mut best = (0, 0);
            for (index, first) in left.iter().enumerate() {
                let mut joined = 1;
                for splittable in left.iter().filter(|splittable| splittable.id != first.id) {
                    let joins = splits_together(
                        first,
                        splittable,
                        formations,
                        JoinTest::Count,
                        &mut path,
                        &mut split.far,
                        &self.model.tuning.order,
                    )?;
                    joined += usize::from(joins);
                }
                if best.1 < joined {
                    best = (index, joined);
                }
            }
            let chosen = left.remove(best.0);
            let mut group = vec![chosen.id];
            if !control.is_held() {
                let mut index = 0;
                while index < left.len() {
                    if splits_together(
                        &chosen,
                        left.checked(index)?,
                        formations,
                        JoinTest::Group,
                        &mut path,
                        &mut split.far,
                        &self.model.tuning.order,
                    )? {
                        group.push(left.remove(index).id);
                    } else {
                        index += 1;
                    }
                }
                if group.len() < 2 {
                    split.loose.push(chosen.id);
                    continue;
                }
            }
            split.keep(group);
        }
        Ok(())
    }

    /// What the order splitter (`FUN_140fd76d0`) reads of each unit of a run that the world
    /// holds: a null entry and one the run's next entry repeats are left out.
    fn splittables(&self, run: &[Option<ObjectId>]) -> GenieResult<Vec<Splittable>> {
        let mut left: Vec<Splittable> = Vec::with_capacity(run.len());
        for (index, &object_id) in run.iter().enumerate() {
            let Some(id) = object_id else {
                continue;
            };
            if run.get(index + 1).copied().flatten() == Some(id) {
                continue;
            }
            let Some((player_id, unit)) = self.object(id) else {
                continue;
            };
            let master = unit.master(self.master_tables(), player_id)?;
            left.push(Splittable {
                id,
                group: unit.group.id,
                kind: master.kind,
                grouped: master.creatable_part().formation_category.grouped(),
                turns: master.object_flags.contains(ObjectFlags::SHIP),
                speed: self.top_speed(player_id, unit)?,
                position: unit.position(),
                moved_at: self
                    .parts
                    .mover_of(unit.id)
                    .map_or_else(|| unit.position().ground(), |mover| mover.moved_at.ground()),
                state: unit.life.state,
                radius: Distance::new(master.radius.x()),
                restriction: self
                    .parts
                    .mover_of(unit.id)
                    .map(|mover| mover.stats.restriction),
            });
        }
        Ok(left)
    }

    /// The units of `left` in a group the world holds go into `split` with the run's other
    /// members of their group (`FUN_140fd76d0`: one alone is loose); the others are returned.
    fn keep_world_groups(
        &self,
        left: Vec<Splittable>,
        split: &mut Split,
    ) -> GenieResult<Vec<Splittable>> {
        // the units in a group the world holds, with the order's other members of their group
        let mut taken = vec![false; left.len()];
        for (position, first) in left.iter().enumerate() {
            if *taken.checked(position)? {
                continue;
            }
            let Some(group) = first.group.and_then(|id| self.groups.get(id)) else {
                continue;
            };
            *taken.checked_mut(position)? = true;
            let mut members = vec![first.id];
            for (index, splittable) in left.iter().enumerate() {
                if !*taken.checked(index)? && group.has(splittable.id) {
                    members.push(splittable.id);
                    *taken.checked_mut(index)? = true;
                }
            }
            match members[..] {
                [alone] => split.loose.push(alone),
                _ => split.keep(members),
            }
        }
        let left: Vec<Splittable> = left
            .into_iter()
            .zip(taken)
            .filter_map(|(splittable, taken)| (!taken).then_some(splittable))
            .collect();
        Ok(left)
    }

    /// The loose units of a MOVE or an INTERACT of `player` leave their groups (`FUN_14102c160`
    /// with 0 on the group of each one's `+0x6d0` the world holds, [`World::leave_group_of`]),
    /// in order. With more than one loose unit of a player whose lobby entry is not a computer's
    /// (`FUN_140d61870`) the routine's target is -1 (MOVE `FUN_140e67800` at `0x140e67e5d`,
    /// INTERACT `FUN_140e68b90` at `0x140e6913c`); else it is the order's target object's id
    /// when the world holds it (`FUN_140e6b820`), -1 without (`0x140e6873b`, `0x140e6991e`).
    pub(super) fn leave_groups(
        &mut self,
        player: PlayerId,
        loose: &[ObjectId],
        target: Option<ObjectId>,
    ) -> GenieResult<()> {
        let target = if 1 < loose.len() && !self.players.player(player)?.kind.is_computer() {
            None
        } else {
            self.world_object(target)
                .map(|world_object| world_object.id())
        };
        for &id in loose {
            self.leave_group_of(id, target)?;
        }
        Ok(())
    }

    /// The centroid of an order's loose units ([`centroid`]) over those that move (above type 29
    /// with speed), or all of a queued order.
    pub(super) fn order_spread(
        &self,
        loose: &[ObjectId],
        queued: bool,
    ) -> GenieResult<Option<Point>> {
        let mut points = Vec::with_capacity(loose.len());
        for &id in loose {
            let Some((player_id, unit)) = self.object(id) else {
                continue;
            };
            let master = unit.master(self.master_tables(), player_id)?;
            // slot 0xe8: the master's speed (`+0x740`), before a sprite's factor
            let speed = master.animated_part().max_speed;
            if (MasterKind::MOVING <= master.kind && Speed::ZERO < speed) || queued {
                points.push(unit.position().ground());
            }
        }
        Ok(centroid(&points, &self.model.tuning.order))
    }
}

/// The splitter groups units of a type above this (`0x45`).
pub(super) const MAX_UNGROUPED: MasterKind = MasterKind::of_data(69);
/// Units nearer than `OrderTuning::split_path_reach` are measured by a path while
/// `DAT_1439e60e3` (1 at run time) holds.
const SPLIT_BY_PATH: bool = true;

/// What [`splits_together`] tests for: the count of the units that join each, or the chosen
/// unit's group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum JoinTest {
    /// The count: who joins whom.
    Count,
    /// The chosen unit's group: who joins it, in a state of 2 or more.
    Group,
}

/// `FUN_140fd76d0`'s test whether `splittable` joins `other`: both above type 69, both of formation
/// category 0 or neither, the same object flags' bit 1, speeds within
/// `OrderTuning::split_speed_gap` where the player breaks formations by speed (`COMISS`, `JA`: a
/// NaN gap is within), then (for [`JoinTest::Group`], also `splittable` in a state of 2 or more)
/// within the player's influence distance on the ground (between where each last moved to, slot
/// 0x520: `FUN_14087e560` with the height zeroed; `COMISS`, `JA`: a NaN distance is within). Below
/// `OrderTuning::split_path_reach` (`COMISS`, `JBE`: a NaN distance is not) the game measures a
/// path instead, when the planner of `splittable`'s restriction exists: from `splittable`'s place
/// to `other`'s (`FUN_140f7c530` with `other`'s radius on x as the radii and `splittable`'s as the
/// range, no tile search), which joins `splittable` when it reaches the goal (result 1) no longer
/// than the influence distance (a NaN length is not longer); else `splittable` joins the splitter's
/// far units (`FUN_140a3e380`).
pub(super) fn splits_together(
    other: &Splittable,
    splittable: &Splittable,
    formations: Formations,
    test: JoinTest,
    path: &mut SplitPath<'_>,
    far: &mut BTreeSet<ObjectId>,
    order_tuning: &OrderTuning,
) -> GenieResult<bool> {
    if other.kind <= MAX_UNGROUPED || splittable.kind <= MAX_UNGROUPED {
        return Ok(false);
    }
    if other.grouped != splittable.grouped || other.turns != splittable.turns {
        return Ok(false);
    }
    if formations.by_speed
        && order_tuning.split_speed_gap < (other.speed - splittable.speed).get().abs()
    {
        return Ok(false);
    }
    if test == JoinTest::Group && splittable.state < ObjectState::Alive {
        return Ok(false);
    }
    let (dx, dy) = (
        other.moved_at.x() - splittable.moved_at.x(),
        other.moved_at.y() - splittable.moved_at.y(),
    );
    let distance = sqrt(dx * dx + 0.0 + dy * dy);
    if SPLIT_BY_PATH
        && distance < order_tuning.split_path_reach
        && let Some((found, length)) = path(other, splittable)?
    {
        let joins =
            found == Found::Goal && at_most_or_unordered(length.get(), formations.influence.get());
        if !joins {
            let _ = far.insert(splittable.id);
        }
        return Ok(joins);
    }
    Ok(at_most_or_unordered(distance, formations.influence.get()))
}
