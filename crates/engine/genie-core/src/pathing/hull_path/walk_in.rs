//! The walk into a hull (`pathWithinHull` into a hull, `FUN_140f88ee0`).

use crate::GenieResult;
use crate::map::Rect;
use crate::pathing::hull_path::node_lists::{NewNode, UnrefusedNode};
use crate::pathing::hull_path::walks::Rays;
use crate::pathing::hull_path::walks::Steps;
use crate::pathing::hull_path::{
    HULL_ITERATIONS, HullIndex, HullPather, NUDGE, NodeIndex, NodeList, PATHS, Point, SIDES,
    Target, WalkResult, Waypoints, Within,
};
use crate::world::slots::CheckedIndexExt;

/// The ways in a walk into a hull keeps: its paths, the hull side each leaves by, how many it
/// keeps at most and how many it kept.
struct Ways<'ways> {
    paths: &'ways mut [Waypoints],
    sides_out: &'ways mut [usize; PATHS],
    most: usize,
    count: &'ways mut usize,
}

/// A walk into a hull ([`HullPather::walk_in`]'s arguments, `FUN_140f88ee0`'s order).
#[derive(Debug)]
pub(super) struct WalkIn<'ways> {
    /// The hull the goal is inside.
    pub(super) hull: HullIndex,
    /// The goal, where the walk starts.
    pub(super) goal: Point,
    /// The start it walks toward.
    pub(super) start: Point,
    /// The target.
    pub(super) target: Option<Target>,
    /// The entrance paths it leaves.
    pub(super) paths: &'ways mut [Waypoints],
    /// The hull side each way out leaves by.
    pub(super) sides_out: &'ways mut [usize; PATHS],
    /// How many ways out it keeps at most.
    pub(super) most: usize,
    /// How many it kept.
    pub(super) count: &'ways mut usize,
}

impl HullPather<'_> {
    /// `MShortPather::pathWithinHull` into a hull (`FUN_140f88ee0`): the walk out of hull `hull`
    /// from the goal `from` toward the start `to`, whose ways out (at most `most`) are the
    /// entrance paths, each with the hull side it leaves by. Result 4 (5 when the goal was inside
    /// a box) with paths, 2 without, 3 without a first step. `FindShortPath`, its only caller,
    /// gives it a hull, a start, paths and the sides' array (its result 6 is for none) and its
    /// argument 10 set: each way out is nudged back into the hull.
    pub(super) fn walk_in(&mut self, walk: WalkIn<'_>) -> GenieResult<Within> {
        let WalkIn {
            hull,
            goal: from,
            start: to,
            target,
            paths,
            sides_out,
            most,
            count,
        } = walk;
        *count = 0;
        let mut left_boxes = false;
        self.empty(NodeList::Walks)?;
        let (inside, stuck) = self.begin_walk_in(hull, (from, to), target)?;
        if stuck {
            return Ok(Within {
                result: WalkResult::Stuck,
                left_boxes,
            });
        }
        let mut ways = Ways {
            paths,
            sides_out,
            most,
            count,
        };
        let mut iterations = 0;
        'search: loop {
            let node = loop {
                if HULL_ITERATIONS <= iterations || ways.most <= *ways.count {
                    break 'search;
                }
                let Some(node_index) = self.nodes.checked(NodeList::Walks.head())?.next else {
                    break 'search;
                };
                iterations += 1;
                self.close(NodeList::Walks, node_index)?;
                if self.on_map(self.nodes.checked(node_index)?.at) {
                    break node_index;
                }
            };
            let at = self.nodes.checked(node)?.at;
            if !self.in_area(at) {
                break;
            }
            left_boxes |= self.cast_into(hull, node, to, &mut ways)? == Rays::LeftBoxes;
        }
        let result = if *ways.count < 1 {
            WalkResult::NoWay
        } else if inside.is_some() {
            WalkResult::WaysFromBox
        } else {
            WalkResult::Ways
        };
        Ok(Within { result, left_boxes })
    }

    /// The walk into a hull's root node at `from` and, when the goal lies in a box or a target
    /// is given, its first steps and the points about that box (the target's grown box). Returns
    /// the box the goal lies in (if any) and whether the walk is stuck (no step made).
    fn begin_walk_in(
        &mut self,
        hull: HullIndex,
        (from, to): (Point, Point),
        target: Option<Target>,
    ) -> GenieResult<(Option<usize>, bool)> {
        let hull_path_tuning = &self.by_id.tuning().hull_path;
        let root = self.add_unrefused(UnrefusedNode {
            list: NodeList::Walks,
            open: true,
            parent: None,
            position: from,
            goal: Some(to),
            hull: -1,
            direction: -1,
        })?;
        let inside = self.hulls.checked(hull.index())?.box_holding(from);
        if inside.is_none() && target.is_none() {
            return Ok((inside, false));
        }
        self.close(NodeList::Walks, root)?;
        let mut made = self.first_steps(hull, root, from, Some(to))? == Steps::Made;
        let bounds = match target {
            None => {
                self.hulls
                    .checked(hull.index())?
                    .objects
                    .checked(inside.unwrap_or(0))?
                    .bounds
            }
            Some(target) => {
                let distance = self.radius;
                Rect::new(
                    (target.at.x() - target.radius.x()) - distance.get(),
                    (target.at.y() - target.radius.y()) - distance.get(),
                    target.at.x() + target.radius.x() + distance.get(),
                    target.at.y() + target.radius.y() + distance.get(),
                )
            }
        };
        made |= self.side_points(hull, root, bounds, to, hull_path_tuning.entrance_step, None)?
            == Steps::Made;
        Ok((inside, !made))
    }

    /// The rays of the walk into a hull's `node` toward each side it may cast to: onto a box they
    /// go on from the box's side; one that leaves the boxes ends on the hull's edge, a way in kept
    /// in `ways` (nudged back into the hull) while it has room. [`Rays::LeftBoxes`] when a ray left
    /// the boxes.
    fn cast_into(
        &mut self,
        hull: HullIndex,
        node: NodeIndex,
        to: Point,
        ways: &mut Ways<'_>,
    ) -> GenieResult<Rays> {
        let at = self.nodes.checked(node)?.at;
        let mut left_boxes = false;
        {
            for side in 0..4 {
                let checked = *self.nodes.checked(node)?;
                if !Self::may_cast(&checked, side) {
                    continue;
                }
                if let Some((point, hull_index, box_side)) = self
                    .hulls
                    .checked(hull.index())?
                    .ray_to_box(at, side, checked.hull)
                {
                    let _ = self.add_node(NewNode {
                        list: NodeList::Walks,
                        open: true,
                        parent: Some(node),
                        position: point,
                        goal: Some(to),
                        hull: hull_index as i32,
                        vertex: box_side,
                        direction: side,
                        by_place: true,
                    })?;
                    continue;
                }
                left_boxes = true;
                let Some((mut point, edge)) =
                    self.hulls.checked(hull.index())?.ray_to_hull(at, side)
                else {
                    continue;
                };
                if ways.most <= *ways.count {
                    continue;
                }
                let ground_offset = *SIDES.checked(side as usize)?;
                point = Point::new(
                    point.x() - ground_offset.x() * NUDGE,
                    point.y() - ground_offset.y() * NUDGE,
                );
                if let Some(end) = self.add_node(NewNode {
                    list: NodeList::Walks,
                    open: false,
                    parent: Some(node),
                    position: point,
                    goal: Some(to),
                    hull: -1,
                    vertex: -1,
                    direction: -1,
                    by_place: false,
                })? && let Some(kept) = self.keep_path(ways.paths, ways.count, end)?
                {
                    *ways.sides_out.checked_mut(kept)? = edge;
                }
            }
        }
        Ok(if left_boxes {
            Rays::LeftBoxes
        } else {
            Rays::InBoxes
        })
    }
}
