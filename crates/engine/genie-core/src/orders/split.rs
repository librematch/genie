//! INTERACT and the order splitter: which units move loose and which form groups.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Point;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::master_type::MOVING;
use crate::orders::{Interact, MAX_UNGROUPED, Split, Splittable, centroid, splits_together};
use crate::pathing::tile_path::Found;
use crate::players::PlayerId;
use crate::world::World;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;

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
    ) -> Result<(), GenieError> {
        match units {
            [] => return Ok(()),
            &[id] if self.place(id).is_none() => return Ok(()),
            _ => {}
        }
        let loose = self
            .split(units, &|_| true, false)
            .and_then(|mut s| self.interact_groups(&mut s, order).map(|()| s.loose));
        self.groups.count_command();
        let loose = loose?;
        self.leave_groups(player, &loose, order.target)?;
        // FUN_140d61870: a player whose lobby entry is of type 4, which a human's is not
        if 1 < loose.len() && self.players.player(player)?.computer {
            return Err(GenieError::unsupported(
                "an order to several units of a computer player (FUN_140d61870)",
            ));
        }
        let centroid = if 1 < loose.len() {
            self.order_spread(&loose, order.queued)?
        } else {
            None
        };
        for (i, &id) in loose.iter().enumerate() {
            let others: Vec<ObjectId> = loose
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, &u)| u)
                .collect();
            self.interact_unit(player, id, &others, centroid, order)?;
        }
        Ok(())
    }

    /// The units of an order that move alone ([`World::split`] without Ctrl held); a group it
    /// makes is not modelled.
    pub(crate) fn loose_units(
        &mut self,
        units: &[Option<ObjectId>],
    ) -> Result<Vec<ObjectId>, GenieError> {
        self.loose_units_kept(units, &|_| true)
    }

    /// [`World::loose_units`] where, after the sort, `kept` drops units (the garrison command's
    /// test, `FUN_140fd9ae0` for command `0x2713`).
    pub(crate) fn loose_units_kept(
        &mut self,
        units: &[Option<ObjectId>],
        kept: &dyn Fn(ObjectId) -> bool,
    ) -> Result<Vec<ObjectId>, GenieError> {
        let split = self.split(units, kept, false)?;
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
        kept: &dyn Fn(ObjectId) -> bool,
        control: bool,
    ) -> Result<Split, GenieError> {
        // FUN_140fd9ae0 splits only when the list's first unit exists (its owner): else no unit
        // takes the order
        let Some((owner, _)) = units.first().and_then(|&id| self.object(id)) else {
            return Ok(Split::default());
        };
        // FUN_140fd9dc0's objects, null for a gone id; LAB_140fd89b0 sorts a null as -1, and
        // equal ids are one object, so any sort gives the game's order
        let mut sorted: Vec<Option<ObjectId>> =
            units.iter().map(|&id| self.object(id).and(id)).collect();
        sorted.sort_unstable_by_key(|u| u.map_or(-1, ObjectId::get));
        for u in &mut sorted {
            if u.is_some_and(|id| !kept(id)) {
                *u = None;
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
        control: bool,
        split: &mut Split,
    ) -> Result<(), GenieError> {
        let mut left: Vec<Splittable> = Vec::with_capacity(run.len());
        for (i, &u) in run.iter().enumerate() {
            let Some(id) = u else {
                continue;
            };
            if run.get(i + 1).copied().flatten() == Some(id) {
                continue;
            }
            let Some((p, u)) = self.object(id) else {
                continue;
            };
            let master = u.master(&self.masters, p)?;
            left.push(Splittable {
                id,
                group: u.group.id,
                kind: master.kind,
                grouped: master.formation_category.grouped(),
                turns: master.object_flags.contains(ObjectFlags::SHIP),
                speed: u.body.mover().map_or(Speed::ZERO, |m| m.stats.speed),
                position: u.position(),
                moved_at: u
                    .body
                    .mover()
                    .map_or_else(|| u.position().ground(), |m| m.moved_at.ground()),
                state: u.life.state,
                radius: Distance::new(master.radius.x()),
                restriction: u.body.mover().map(|m| m.stats.restriction),
            });
        }
        // the units in a group the world holds, with the order's other members of their group
        let mut taken = vec![false; left.len()];
        for (i, first) in left.iter().enumerate() {
            if *taken.checked(i)? {
                continue;
            }
            let Some(group) = first.group.and_then(|id| self.groups.get(id)) else {
                continue;
            };
            *taken.checked_mut(i)? = true;
            let mut members = vec![first.id];
            for (j, u) in left.iter().enumerate() {
                if !*taken.checked(j)? && group.has(u.id) {
                    members.push(u.id);
                    *taken.checked_mut(j)? = true;
                }
            }
            match members[..] {
                [alone] => split.loose.push(alone),
                _ => split.keep(members),
            }
        }
        let mut left: Vec<Splittable> = left
            .into_iter()
            .zip(taken)
            .filter_map(|(u, taken)| (!taken).then_some(u))
            .collect();
        let formations = self.players.player(owner)?.formations;
        if !formations.auto {
            split.loose.extend(left.iter().map(|u| u.id));
            return Ok(());
        }
        // units above type 69 of formation category 0 stay loose (the dock test FUN_141158210 is
        // 1 only on an `E` tile in game type 12)
        let (loose, rest): (Vec<Splittable>, Vec<Splittable>) = left
            .into_iter()
            .partition(|u| MAX_UNGROUPED < u.kind && !u.grouped);
        split.loose.extend(loose.into_iter().map(|u| u.id));
        left = rest;
        // the path between two near units: the tile planner of the second's restriction, from
        // it to the first (FUN_140f7c530, its result and the measured path's length)
        let tiles = self.spatial.tiles();
        let pathing = &mut self.pathing;
        let mut path =
            |a: &Splittable, b: &Splittable| -> Result<Option<(Found, Distance)>, GenieError> {
                let Some(restriction) = b.restriction else {
                    return Ok(None);
                };
                let Some(planner) = pathing.planner(restriction, tiles) else {
                    return Ok(None);
                };
                let req = crate::pathing::tile_path::Request {
                    from: b.position.ground(),
                    to: a.position.ground(),
                    radii: a.radius,
                    range: b.radius,
                    closed: Vec::new(),
                    full: false,
                };
                let mut p = crate::pathing::tile_path::Path::default();
                let found = planner.plan::<GenieError>(&req, false, &mut p, None)?;
                p.measure();
                Ok(Some((found, p.length)))
            };
        // then the unit with the most others that join it, the first of equals, with them
        while !left.is_empty() {
            let mut best = (0, 0);
            for (i, a) in left.iter().enumerate() {
                let mut n = 1;
                for b in left.iter().filter(|b| b.id != a.id) {
                    let joins =
                        splits_together(a, b, formations, false, &mut path, &mut split.far)?;
                    n += usize::from(joins);
                }
                if best.1 < n {
                    best = (i, n);
                }
            }
            let chosen = left.remove(best.0);
            let mut group = vec![chosen.id];
            if !control {
                let mut i = 0;
                while i < left.len() {
                    if splits_together(
                        &chosen,
                        left.checked(i)?,
                        formations,
                        true,
                        &mut path,
                        &mut split.far,
                    )? {
                        group.push(left.remove(i).id);
                    } else {
                        i += 1;
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
    ) -> Result<(), GenieError> {
        let target = if 1 < loose.len() && !self.players.player(player)?.computer {
            None
        } else {
            self.world_object(target).map(|t| t.id())
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
    ) -> Result<Option<Point>, GenieError> {
        let mut points = Vec::with_capacity(loose.len());
        for &id in loose {
            let Some((p, u)) = self.object(id) else {
                continue;
            };
            let master = u.master(&self.masters, p)?;
            let speed = master.moving.as_ref().map_or(Speed::ZERO, |m| m.speed);
            if (MOVING <= master.kind && Speed::ZERO < speed) || queued {
                points.push(u.position().ground());
            }
        }
        Ok(centroid(&points))
    }
}
