//! A player's line of wall foundations: `BUILD_WALL` (0x69, `FUN_140e7b2e0`), which dragging a
//! wall sends.
//!
//! The command splits the line between its two tiles into at most two segments, each straight
//! or diagonal ([`segments`]), and walks their tiles (`FUN_140e7f670`): each tile's point is
//! snapped to the master's footprint (the master's slot 0x60, [`snap`]), the master's placement
//! test (slot 0x50) passes over a tile it refuses, and a tile the player cannot pay for ends the
//! segment; the others get a foundation (slot 0x108, the constructor BUILD's foundation has too).
//! Then the builders, the farthest from the line's end first, are shared out over the
//! foundations ([`share`]): each one builds its own (slot 0x128), or, on the unit AI's queue
//! path, queues a build order on every foundation of the line (slot 0x30, order 0x2be).
//! The segments' tails (`FUN_140e7fbb0`, `FUN_140e7f3a0`, the walls they join) run only for a
//! player that is both a computer and not (`FUN_141659cf0` decides their flags and their test),
//! so never.

use crate::GenieError;
use crate::commands::QueueId;
use crate::commands::UnitList;
use crate::construction::FoundationId;
use crate::construction::build::BuildAction;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::math::image::wall::{HALF as WALL_HALF, ONE as WALL_ONE};
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{MasterId, player_master};
use crate::orders::readiness;
use crate::players::PlayerId;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::generated::WallTuning;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;
use std::cmp::Ordering;

/// The snap's half tile and its step up (`FUN_141123ac0`).
const HALF: f32 = WALL_HALF;
const ONE: f32 = WALL_ONE;

/// A builder's master type is above this (`FUN_141262270`: `0x27`).
const BUILDER_KIND: MasterKind = MasterKind::of_data(0x27);

/// The builders the game's sort (`FUN_140e6eee0`) orders by insertion: 0x100 bytes of 8-byte
/// entries. More go to its quicksort, which is not stable.
const INSERTION_SORT_MAX: usize = 32;

/// `BUILD_WALL` (0x69, `FUN_140e7b2e0`): a line of the player's wall foundations and its
/// builders shared out over them, as the command reads its packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildWallCommand {
    /// The player who places it.
    pub player: PlayerId,
    /// The wall's master (`+0x10`).
    pub master: MasterId,
    /// The line's first tile (`+0x8`, `+0xa`).
    pub from: Tile,
    /// The line's last tile (`+0xc`, `+0xe`).
    pub to: Tile,
    /// `+0x14`: anything but −1 and the command does nothing.
    pub queue_id: QueueId,
    /// The units told to build it, in packet order (the count at `+0x4`, the ids from `+0x1c`).
    pub builders: UnitList,
    /// `+0x1a`: queued (a builder with a unit AI takes the line as orders in its queue).
    pub queued: bool,
    /// `+0x1b` is 0: a builder with a unit AI holding an order not ready takes the line as
    /// orders too, and the queue's orders become ready after (`FUN_140c42b60`).
    pub commits: bool,
}

impl BuildWallCommand {
    /// `do_command`'s case checks the builders as BUILD's (a count past the packet's ids
    /// refuses it, [`World::read_units`]), then the wall's line is placed
    /// ([`World::build_wall`]).
    pub(crate) fn run(&self, w: &mut World) -> Result<(), GenieError> {
        let Some(listed) = w.read_units(self.player, &self.builders) else {
            return Ok(());
        };
        let Some(builders) = w.command_check(self.player, listed)? else {
            return Ok(());
        };
        w.build_wall(self, &builders)
    }
}

/// The segments of the line from `from` to `to` (`FUN_140e7b2e0`): one when the line is a single
/// tile, straight or diagonal; else its straight part and its diagonal one, the straight one
/// first when it is longer than the diagonal. The second starts on the first's last tile.
#[must_use]
pub fn segments(from: Tile, to: Tile) -> Vec<(Tile, Tile)> {
    let (x1, y1, x2, y2) = (from.x(), from.y(), to.x(), to.y());
    let dx = (x2 - x1).abs() + 1;
    let dy = (y2 - y1).abs() + 1;
    if dx == 1 || dy == 1 || dx == dy {
        return vec![(from, to)];
    }
    if dy < dx {
        let d = (y1 - y2).abs();
        let s = if x2 <= x1 { d } else { -d };
        if d < (x1 - x2).abs() - d {
            let corner = Tile::new(x2 + s, y1);
            vec![(from, corner), (corner, to)]
        } else {
            let corner = Tile::new(x1 - s, y2);
            vec![(from, corner), (corner, to)]
        }
    } else {
        let d = (x1 - x2).abs();
        let s = if y2 <= y1 { d } else { -d };
        if d < (y1 - y2).abs() - d {
            let corner = Tile::new(x1, y2 + s);
            vec![(from, corner), (corner, to)]
        } else {
            let corner = Tile::new(x2, y1 - s);
            vec![(from, corner), (corner, to)]
        }
    }
}

/// The tiles of a straight or diagonal segment, its first to its last (`FUN_140e7f670`'s walk:
/// each step moves both coordinates toward the end). A segment of neither shape would not end.
///
/// # Errors
///
/// For a segment neither straight nor diagonal.
pub fn tiles(from: Tile, to: Tile) -> Result<Vec<Tile>, GenieError> {
    let (dx, dy) = (to.x() - from.x(), to.y() - from.y());
    if dx != 0 && dy != 0 && dx.abs() != dy.abs() {
        return Err(GenieError::unsupported(
            "a wall segment neither straight nor diagonal",
        ));
    }
    let (sx, sy) = (dx.signum(), dy.signum());
    let n = dx.abs().max(dy.abs());
    Ok((0..=n)
        .map(|k| Tile::new(from.x() + k * sx, from.y() + k * sy))
        .collect())
}

/// A building master's point for a tile (its slot 0x60, `FUN_141123ac0` with 1): the footprint
/// of `radius` around `at` moved inside the map of `map` tiles (its far edges less
/// `WallTuning::map_margin`), then each coordinate snapped: to the nearest whole one for a whole radius, to
/// the tile's centre for another.
#[must_use]
pub fn snap(radius: Footprint, map: GridSize, at: Point, wall_tuning: WallTuning) -> Point {
    let (mut x, mut y) = (at.x(), at.y());
    let (rx, ry) = (radius.x(), radius.y());
    let (low_x, mut high_x) = (x - rx, rx + x);
    let (low_y, mut high_y) = (y - ry, ry + y);
    let far_x = map.width() as f32 - wall_tuning.map_margin;
    let far_y = map.height() as f32 - wall_tuning.map_margin;
    if low_x < 0.0 {
        let d = 0.0 - low_x;
        high_x += d;
        x += d;
    }
    if low_y < 0.0 {
        let d = 0.0 - low_y;
        high_y += d;
        y += d;
    }
    if far_x <= high_x {
        x += far_x - high_x;
    }
    if far_y <= high_y {
        y += far_y - high_y;
    }
    Point::new(
        snap_one(x, Distance::new(rx)),
        snap_one(y, Distance::new(ry)),
    )
}

/// One coordinate of [`snap`].
fn snap_one(v: f32, radius: Distance) -> f32 {
    let whole = |f: f32| f32::from(f as i32 as i16);
    let t = whole(v);
    if radius - Distance::new(whole(radius.get())) == Distance::ZERO {
        if HALF < v - t { t + ONE } else { t }
    } else {
        t + HALF
    }
}

/// The foundation the `i`-th builder takes when `capable` of them can build and `placed`
/// foundations were placed (`FUN_140e7b2e0`'s last loop): with fewer builders than foundations
/// the first takes the first, the last capable one the last, the others the ones spread between;
/// else they take them in turn. None where the game's index falls outside the foundations.
#[must_use]
pub fn share(i: usize, capable: usize, placed: usize) -> Option<usize> {
    if capable < placed {
        if i == 0 {
            Some(0)
        } else if i == capable - 1 {
            Some(placed - 1)
        } else {
            let spread = (placed as i32 - 2) as f32 / (capable as i32 - 1) as f32;
            let at = spread * (i as i32 - 1) as f32;
            // the game's conversion of a value out of range is its own (0x80000000)
            if !at.is_finite() || i32::MAX as f32 <= at {
                return None;
            }
            usize::try_from(at as i32 + 1).ok().filter(|&k| k < placed)
        }
    } else {
        Some(i % placed)
    }
}

impl World {
    /// `BUILD_WALL` ([`BuildWallCommand`]) for its player: a `qId` other than −1, a master the player has not
    /// got or not a building does nothing, and so does a human's line without a builder that can
    /// build it ([`World::builds`]). The foundations go down tile by tile (the placement
    /// test with the explored test on, then the payment's, [`World::found`]); then each builder
    /// the command check holds, sorted by its footprint's distance to the line's last tile
    /// (squared, the farthest first), takes its foundation ([`share`]) directly
    /// ([`World::build_task`], [`World::build_directly`]) or, on the queue path, a build order on
    /// each foundation ([`World::queue_build`]). Not held: the interface's view
    /// (`FUN_140e6e140`); a committing line's queue path ends in the readiness pass with 0
    /// (`FUN_140c42b60` at 0x140e7be4b, [`crate::orders::readiness`]). Not modelled: the
    /// population test of a master without `+0x5f0` (as for BUILD), more builders than the game's
    /// insertion sort takes.
    pub(crate) fn build_wall(
        &mut self,
        w: &BuildWallCommand,
        builders: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        let (player, master) = (w.player, w.master);
        if w.queue_id.is_queued() {
            return Ok(());
        }
        let Some(site) = player_master(&self.model.masters, player, master) else {
            return Ok(());
        };
        // The buildings' master type: a wall of another type does nothing.
        if site.kind != MasterKind::BUILDING {
            return Ok(());
        }
        let (radius, trained_at) = (site.radius, site.creatable_part().trained_at.clone());
        let player_kind = self.players.player(player)?.kind;
        let builders: &[Option<ObjectId>] =
            if player_kind.is_computer() && builders.first() == Some(&None) {
                &[]
            } else {
                builders
            };
        let mut capable = 0;
        for &id in builders.iter().flatten() {
            if self.builds((master, &trained_at), id)? {
                capable += 1;
            }
        }
        if !player_kind.is_computer() && capable == 0 {
            return Ok(());
        }
        let placed = self.wall_foundations(w, radius)?;
        // the builders, the farthest from the line's last tile first
        let sorted = self.builders_by_gap(w, builders)?;
        if placed.is_empty() || capable == 0 {
            return Ok(());
        }
        for (i, &(id, _)) in sorted.iter().enumerate() {
            let k = share(i, capable, placed.len()).ok_or(GenieError::unsupported(
                "a wall builder past the foundations the game shares out",
            ))?;
            let site = *placed.checked(k)?;
            self.wall_builder_takes(w, id, &placed, site)?;
        }
        Ok(())
    }

    /// [`World::build_wall`]'s foundations, tile by tile along the line's segments: each tile
    /// snapped for a building of `radius` that passes the placement test (with the explored test
    /// on) gets a foundation ([`World::found`]) while the player affords one; the foundations'
    /// ids and places.
    ///
    /// # Errors
    ///
    /// What the placement test, the payment and the foundation do not model.
    fn wall_foundations(
        &mut self,
        w: &BuildWallCommand,
        radius: Footprint,
    ) -> Result<Vec<(FoundationId, Position)>, GenieError> {
        let (player, master) = (w.player, w.master);
        let mut placed: Vec<(FoundationId, Position)> = Vec::new();
        for (a, b) in segments(w.from, w.to) {
            for t in tiles(a, b)? {
                let at = snap(
                    radius,
                    self.spatial.map,
                    Point::new(t.x() as f32, t.y() as f32),
                    self.model.tuning.wall,
                );
                if !self.placement_code(player, master, at, true)?.is_allowed() {
                    continue;
                }
                if !self.affords(player, master)? {
                    break;
                }
                placed.push(self.found(player, master, at.with_height(0.0))?);
            }
        }
        Ok(placed)
    }

    /// [`World::build_wall`]'s builders the command check holds, sorted by their footprints'
    /// squared distance to the line's last tile, the farthest first (the game's insertion sort).
    ///
    /// # Errors
    ///
    /// A builder of a master the slice does not have, and more builders than the insertion sort
    /// takes.
    fn builders_by_gap(
        &self,
        w: &BuildWallCommand,
        builders: &[Option<ObjectId>],
    ) -> Result<Vec<(ObjectId, f32)>, GenieError> {
        let mut sorted = Vec::new();
        for id in self.checked_units(w.player, builders).into_iter().flatten() {
            let Some((owner, u)) = self.object(id) else {
                continue;
            };
            let r = u.master(self.master_tables(), owner)?.radius;
            // the packet's word converted (`CVTSI2SS` of a word: exact)
            let gap = |end: i32, at: f32, r: f32| {
                let g = ((end as f32 + HALF) - at).abs() - r;
                if 0.0 <= g { g } else { 0.0 }
            };
            let gx = gap(w.to.x(), u.position().x(), r.x());
            let gy = gap(w.to.y(), u.position().y(), r.y());
            sorted.push((id, gy * gy + gx * gx));
        }
        if INSERTION_SORT_MAX < sorted.len() {
            return Err(GenieError::unsupported(
                "BUILD_WALL with more builders than the insertion sort takes (FUN_140e6eee0)",
            ));
        }
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        Ok(sorted)
    }

    /// [`World::build_wall`]'s builder `id` and its foundation (`foundation` at `at`, its share
    /// of `placed`): directly ([`World::build_task`], [`World::build_directly`]) without a unit
    /// AI, or off the queue path (a queued line, or a committing one with an order not ready);
    /// else a build order on each foundation ([`World::queue_build`]), a committing line's
    /// readiness pass with 0 after.
    ///
    /// # Errors
    ///
    /// A builder the slice does not have, and what the build does not model.
    fn wall_builder_takes(
        &mut self,
        w: &BuildWallCommand,
        id: ObjectId,
        placed: &[(FoundationId, Position)],
        (foundation, at): (FoundationId, Position),
    ) -> Result<(), GenieError> {
        let (player, master) = (w.player, w.master);
        let Some(Place::Live(q, idx)) = self.place(id) else {
            return Err(GenieError::unsupported(
                "a wall builder the slice does not have",
            ));
        };
        let direct = self
            .parts
            .unit_ai
            .get(self.players.live(q, idx)?.id)
            .is_none_or(|ai| !w.queued && (!w.commits || !ai.has_unready_order()));
        if direct {
            let task: (MasterId, BuildAction) =
                self.build_task(q, id, (player, foundation, master), at)?;
            self.build_directly(q, idx, task, (player, foundation, master), at)?;
        } else {
            for &(f, p) in placed {
                self.queue_build(q, idx, (player, f, master), p)?;
            }
            if w.commits
                && let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(q, idx)?.id)
            {
                // the readiness pass with 0: every queued order is ready
                readiness(&mut ai.orders, false);
            }
        }
        Ok(())
    }

    /// Whether object `id` can build `building`, whose train locations are `trained_at`
    /// (`FUN_141262270`): it is an object the world holds of a master type above `0x27`, and a
    /// villager (class 4) when the train locations name the builder 118 or its own master; any
    /// other unit when its master's first task of action type 0x65 (`FUN_14109e100`) names the
    /// building (`+0x12`, its object id).
    pub(crate) fn builds(
        &self,
        (building, trained_at): (MasterId, &[MasterId]),
        id: ObjectId,
    ) -> Result<bool, GenieError> {
        let Some((owner, u)) = self.object(id) else {
            return Ok(false);
        };
        let m = u.master(self.master_tables(), owner)?;
        if m.kind <= BUILDER_KIND {
            return Ok(false);
        }
        if !self
            .model
            .roles
            .of_class(m.class)
            .contains(ClassRoles::WALL_BUILDER)
        {
            return Ok(m
                .action_part()
                .tasks
                .iter()
                // The builder the train locations name for every villager (`FUN_141262270`: `0x76`).
                // The action type of the task another unit builds with (`FUN_141262270`: `0x65`).
                .find(|t| t.action_kind == ActionKind::BUILD)
                .is_some_and(|t| t.object_id == Some(building)));
        }
        Ok(trained_at.contains(&self.model.slots.master.builder) || trained_at.contains(&m.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::tuning;

    /// The tile at `x`, `y`.
    const fn t(x: i32, y: i32) -> Tile {
        Tile::new(x, y)
    }

    #[test]
    fn line_is_one_segment_when_straight_or_diagonal() {
        assert_eq!(segments(t(3, 4), t(3, 4)), [(t(3, 4), t(3, 4))]);
        assert_eq!(segments(t(3, 4), t(9, 4)), [(t(3, 4), t(9, 4))]);
        assert_eq!(segments(t(3, 4), t(3, 1)), [(t(3, 4), t(3, 1))]);
        assert_eq!(segments(t(3, 4), t(6, 7)), [(t(3, 4), t(6, 7))]);
    }

    #[test]
    fn bent_line_puts_its_longer_part_first() {
        // 8 across, 2 down: the straight part (6) is longer than the diagonal (2)
        assert_eq!(
            segments(t(0, 0), t(8, 2)),
            [(t(0, 0), t(6, 0)), (t(6, 0), t(8, 2))]
        );
        // 4 across, 3 down: the diagonal (3) first, then the straight part (1)
        assert_eq!(
            segments(t(0, 0), t(4, 3)),
            [(t(0, 0), t(3, 3)), (t(3, 3), t(4, 3))]
        );
        // leftwards and upwards, taller than wide
        assert_eq!(
            segments(t(10, 10), t(8, 2)),
            [(t(10, 10), t(10, 4)), (t(10, 4), t(8, 2))]
        );
        assert_eq!(
            segments(t(10, 10), t(7, 6)),
            [(t(10, 10), t(7, 7)), (t(7, 7), t(7, 6))]
        );
        for (a, b) in [
            (t(0, 0), t(8, 2)),
            (t(0, 0), t(4, 3)),
            (t(10, 10), t(8, 2)),
            (t(10, 10), t(7, 6)),
        ] {
            let s = segments(a, b);
            assert!(s.iter().all(|&(f, t)| tiles(f, t).is_ok()));
            assert_eq!((s[0].0, s[s.len() - 1].1), (a, b));
        }
    }

    #[test]
    fn segment_walks_both_coordinates() {
        assert_eq!(
            tiles(t(5, 5), t(7, 3)).unwrap(),
            [t(5, 5), t(6, 4), t(7, 3)]
        );
        assert_eq!(tiles(t(5, 5), t(5, 5)).unwrap(), [t(5, 5)]);
        assert!(tiles(t(0, 0), t(2, 1)).is_err());
    }

    #[test]
    fn half_tile_footprint_snaps_to_the_tile_centre() {
        assert_eq!(
            snap(
                Footprint::new(0.5, 0.5),
                GridSize::new(120, 120),
                Point::new(10.0, 20.0),
                tuning().wall
            ),
            Point::new(10.5, 20.5)
        );
        // a whole radius rounds to the nearest whole point
        assert_eq!(
            snap(
                Footprint::new(1.0, 1.0),
                GridSize::new(120, 120),
                Point::new(10.0, 20.0),
                tuning().wall
            ),
            Point::new(10.0, 20.0)
        );
        // the map's edges push the footprint in
        assert_eq!(
            snap(
                Footprint::new(0.5, 0.5),
                GridSize::new(120, 120),
                Point::new(0.0, 119.0),
                tuning().wall
            ),
            Point::new(0.5, 119.5)
        );
        assert_eq!(
            snap(
                Footprint::new(1.0, 1.0),
                GridSize::new(120, 120),
                Point::new(0.0, 119.0),
                tuning().wall
            ),
            Point::new(1.0, 119.0)
        );
    }

    #[test]
    fn builders_are_shared_out_over_the_foundations() {
        // three builders over seven foundations: the ends and the middle
        let k: Vec<_> = (0..3).map(|i| share(i, 3, 7)).collect();
        assert_eq!(k, [Some(0), Some(1), Some(6)]);
        let k: Vec<_> = (0..4).map(|i| share(i, 4, 7)).collect();
        assert_eq!(k, [Some(0), Some(1), Some(2), Some(6)]);
        // as many builders as foundations or more: in turn
        let k: Vec<_> = (0..5).map(|i| share(i, 5, 2)).collect();
        assert_eq!(k, [Some(0), Some(1), Some(0), Some(1), Some(0)]);
        // a builder past the capable ones lands outside
        assert_eq!(share(1, 1, 3), None);
    }
}
