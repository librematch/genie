//! The quick path (`findQuickPath`) and the walks out of a hull and into one
//! (`pathWithinHull`).

use crate::GenieResult;
use crate::construction::MainBuildingId;
use crate::map::Distance;
use crate::map::Rect;
use crate::map::spatial::RecordId;
use crate::math::fmath::{at_most_or_unordered, maxss, sqrt};
use crate::objects::ObjectId;
use crate::pathing::hull_path::convex::AtStart;
use crate::pathing::hull_path::node_lists::{NewNode, UnrefusedNode};
use crate::pathing::hull_path::{
    DIRECTIONS, HALF, HULL_ITERATIONS, HullIndex, HullPather, NUDGE, Node, NodeIndex, NodeList,
    ONE, Point, SIDES, SQRT_2, Target, WalkResult, Waypoints, Within, walk_length,
};
use crate::world::slots::CheckedIndex;

/// A walk out of a hull ([`HullPather::walk_out`]'s arguments, `FUN_140f8cf00`'s order).
#[derive(Debug)]
pub(super) struct WalkOut<'a> {
    /// The hull the start is inside.
    pub(super) hull: HullIndex,
    /// The start.
    pub(super) start: Point,
    /// The goal (none: the ways out are kept).
    pub(super) goal: Option<Point>,
    /// The target whose range ends the walk too.
    pub(super) target: Option<Target>,
    /// The range of the goal or the target.
    pub(super) range: Distance,
    /// Whether each way out is nudged back into the hull.
    pub(super) nudge: bool,
    /// The paths it leaves.
    pub(super) paths: &'a mut [Waypoints],
    /// How many ways out it keeps at most.
    pub(super) most: usize,
    /// How many it kept.
    pub(super) count: &'a mut usize,
}

/// How a walk out stands at a node: on, at its goal or at its target.
enum WalkEnd {
    Going,
    Goal,
    Target,
}

/// Whether a node's rays left the hull's boxes ([`HullPather::cast_rays`],
/// [`HullPather::cast_into`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(super) enum Rays {
    /// A ray left the boxes.
    LeftBoxes,
    /// Every ray stayed on the boxes.
    InBoxes,
}

/// Whether a walk's first steps or a box's side points made a node ([`HullPather::first_steps`],
/// [`HullPather::side_points`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(super) enum Steps {
    /// One or more were made.
    Made,
    /// None was.
    NoneMade,
}

impl HullPather<'_> {
    /// The gap between `p` and the target's box grown by the radius; each axis held at 0 or
    /// more by `MAXSS` (0x140f86bcf, 0x140f85a18): an unordered one is 0.
    pub(super) fn target_gap(&self, t: &Target, p: Point) -> Distance {
        let r = self.radius;
        let gx = maxss((p.x() - t.at.x()).abs() - (t.radius.x() + r.get()), 0.0);
        let gy = maxss((p.y() - t.at.y()).abs() - (t.radius.y() + r.get()), 0.0);
        Distance::new(sqrt(gy * gy + gx * gx))
    }

    // The quick path.

    /// `MShortPather::findQuickPath` (`FUN_140f8c5a0`): the straight walk to `to`, when it meets
    /// nothing or only the target; else around the one square obstruction it meets (with the
    /// target, the other), by a point `(half side + radius) √2` from its middle on either side of
    /// the walk, when both walks by that point meet nothing (the second the target alone).
    pub(super) fn quick_path(
        &mut self,
        from: Point,
        to: Point,
        target: Option<ObjectId>,
        path: &mut Waypoints,
    ) -> GenieResult<bool> {
        let hits = self.hits(from, to)?;
        let is_target = |id: RecordId| target.is_some_and(|t| id == t);
        let obstacle = match hits[..] {
            [] => {
                path.push(from);
                path.push(to);
                return Ok(true);
            }
            [h] if is_target(h.id) => {
                path.push(from);
                path.push(to);
                return Ok(true);
            }
            [h] => h,
            [a, b] if target.is_some() => {
                if is_target(a.id) {
                    b
                } else if is_target(b.id) {
                    a
                } else {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        };
        let b = obstacle.bounds;
        let (x0, y0, x1, y1) = (b.x0(), b.y0(), b.x1(), b.y1());
        let half = (x1 - x0) * HALF;
        if half != (y1 - y0) * HALF {
            return Ok(false);
        }
        let (mut dx, mut dy) = (from.x() - to.x(), from.y() - to.y());
        let (cx, cy) = ((x1 + x0) * HALF, (y1 + y0) * HALF);
        let len = dy * dy + dx * dx;
        if 0.0 < len {
            let len = sqrt(len);
            dx *= ONE / len;
            dy *= ONE / len;
        }
        let off = (half + self.radius.get()) * SQRT_2;
        let (px, py) = (-dy * off, off * dx);
        for k in 0..2 {
            let p = if k == 0 {
                Point::new(cx + px, cy + py)
            } else {
                Point::new(cx - px, cy - py)
            };
            if !self.bounds.holds(p) {
                continue;
            }
            let first = self.hits(from, p)?;
            if !first.is_empty() {
                continue;
            }
            let second = self.hits(p, to)?;
            let fine = match second[..] {
                [] => true,
                [h] => target.is_some_and(|t| {
                    h.id == t || self.by_id.annex_of(h.id).map(MainBuildingId::id) == Some(t)
                }),
                _ => false,
            };
            if fine {
                path.push(from);
                path.push(p);
                path.push(to);
                return Ok(true);
            }
        }
        Ok(false)
    }

    // The walks within a hull.

    /// The points about box `b` a walk within a hull starts from: from the sides facing the region
    /// of `p` about the box (inside it, its nearest side and those within the radius), a point every
    /// `step` of the side's length, [`NUDGE`] off the box; with `reach`, only those within it of
    /// `p`; none inside a box of the hull. [`Steps::Made`] when one was.
    pub(super) fn side_points(
        &mut self,
        hull: HullIndex,
        root: NodeIndex,
        b: Rect,
        p: Point,
        step: f32,
        reach: Option<(Point, f32)>,
    ) -> GenieResult<Steps> {
        let (x0, y0, x1, y1) = (b.x0(), b.y0(), b.x1(), b.y1());
        // `COMISS`/`JA`, then `SETBE`; `JBE`, then `CMOVA` (0x140f8d1fc..0x140f8d235,
        // 0x140f893a4..0x140f893d0): an unordered coordinate counts past the box's far side
        let col = if p.x() < x0 {
            0
        } else if p.x() < x1 {
            1
        } else {
            2
        };
        let row = if p.y() < y0 {
            0
        } else if p.y() < y1 {
            3
        } else {
            6
        };
        let sides = match row + col {
            0 => [true, false, false, true],
            1 => [true, false, false, false],
            2 => [true, true, false, false],
            3 => [false, false, false, true],
            4 => {
                let (down, right, up) = (p.y() - y0, x1 - p.x(), y1 - p.y());
                let mut m = down;
                let mut side = usize::from(right < down);
                if right < down {
                    m = right;
                }
                let r = self.radius;
                let mut sides = [
                    down < r.get(),
                    right < r.get(),
                    up < r.get(),
                    p.x() - x0 < r.get(),
                ];
                if up < m {
                    side = 2;
                    m = up;
                }
                if p.x() - x0 < m {
                    side = 3;
                }
                sides.get_mut(side).into_iter().for_each(|s| *s = true);
                sides
            }
            5 => [false, true, false, false],
            6 => [false, false, true, true],
            7 => [false, false, true, false],
            _ => [false, true, true, false],
        };
        // a point farther than the reach is left out (`COMISS`/`JA`, 0x140f8d620)
        let near = |q: Point| {
            reach.is_none_or(|(c, d)| {
                at_most_or_unordered(
                    (c.x() - q.x()) * (c.x() - q.x()) + (c.y() - q.y()) * (c.y() - q.y()),
                    d * d,
                )
            })
        };
        let mut made = Steps::NoneMade;
        let mut add = |me: &mut Self, q: Point| -> GenieResult<()> {
            if near(q) && me.hulls.checked(hull.index())?.box_holding(q).is_none() {
                let _ = me.add_node(NewNode {
                    list: NodeList::WALKS,
                    open: true,
                    parent: Some(root),
                    position: q,
                    goal: None,
                    hull: -1,
                    vertex: -1,
                    direction: -1,
                    by_place: false,
                })?;
                made = Steps::Made;
            }
            Ok(())
        };
        if sides[0] {
            let y = y0 - NUDGE;
            let mut x = x0 + NUDGE;
            let d = (x1 - x0) * step;
            while x < x1 {
                add(self, Point::new(x, y))?;
                x += d;
            }
        }
        if sides[1] {
            let x = x1 + NUDGE;
            let mut y = y0 + NUDGE;
            while y < y1 {
                add(self, Point::new(x, y))?;
                y += (y1 - y0) * step;
            }
        }
        if sides[2] {
            let y = y1 + NUDGE;
            let mut x = x1 - NUDGE;
            while x0 <= x {
                add(self, Point::new(x, y))?;
                x -= (x1 - x0) * step;
            }
        }
        if sides[3] {
            let x = x0 - NUDGE;
            let mut y = y1 - NUDGE;
            while y0 <= y {
                add(self, Point::new(x, y))?;
                y -= (y1 - y0) * step;
            }
        }
        Ok(made)
    }

    /// The 16 first steps out of the box holding the start of a walk within a hull: each a
    /// diameter toward one of [`DIRECTIONS`], or to where it leaves the hull, unless inside a box.
    pub(super) fn first_steps(
        &mut self,
        hull: HullIndex,
        root: NodeIndex,
        p: Point,
        goal: Option<Point>,
    ) -> GenieResult<Steps> {
        let hull_path_tuning = &self.by_id.tuning().hull_path;
        let d = self.radius + self.radius;
        let mut made = Steps::NoneMade;
        for dir in DIRECTIONS {
            let mut q = Point::new(d.get() * dir.x() + p.x(), d.get() * dir.y() + p.y());
            if let Some((at, _)) = self.hulls.checked(hull.index())?.intersection(
                AtStart::Ignored,
                p,
                q,
                hull_path_tuning,
            ) {
                q = at;
            }
            if self.hulls.checked(hull.index())?.box_holding(q).is_none() {
                let _ = self.add_node(NewNode {
                    list: NodeList::WALKS,
                    open: true,
                    parent: Some(root),
                    position: q,
                    goal,
                    hull: -1,
                    vertex: -1,
                    direction: -1,
                    by_place: false,
                })?;
                made = Steps::Made;
            }
        }
        Ok(made)
    }

    /// Whether a node of a walk within a hull may cast a ray toward `side`: not back along the
    /// ray that made it, nor into the side of the box it lies on.
    pub(super) const fn may_cast(n: &Node, side: i32) -> bool {
        if n.direction == side {
            return false;
        }
        let back = matches!((n.direction, side), (0, 1) | (1, 0) | (2, 3) | (3, 2));
        let into = matches!((n.vertex, side), (0, 0) | (1, 2) | (2, 1) | (3, 3));
        !(back || into)
    }

    /// Keeps a walk's way out of a hull at `end` as one of `paths` (`count` kept, at most `most`):
    /// a path ending within `HullPathTuning::same_end` of one kept replaces it when it costs no more, else is
    /// dropped. Returns the path's slot.
    pub(super) fn keep_path(
        &mut self,
        paths: &mut [Waypoints],
        count: &mut usize,
        end: NodeIndex,
    ) -> GenieResult<Option<usize>> {
        let hull_path_tuning = &self.by_id.tuning().hull_path;
        let at = self.nodes.checked(end)?.at;
        let cost = self.nodes.checked(end)?.cost;
        let mut k = 0;
        while k < *count {
            let last = *paths
                .checked(k)?
                .points
                .last()
                .unwrap_or(&Point::new(f32::NAN, f32::NAN));
            if (at.x() - last.x()).abs() < hull_path_tuning.same_end
                && (at.y() - last.y()).abs() < hull_path_tuning.same_end
            {
                if paths.checked(k)?.cost < cost {
                    return Ok(None);
                }
                break;
            }
            k += 1;
        }
        if k == *count {
            *count += 1;
        } else {
            paths.checked_mut(k)?.points.clear();
        }
        self.build_path(paths.checked_mut(k)?, end)?;
        paths.checked_mut(k)?.cost = cost;
        Ok(Some(k))
    }

    /// `MShortPather::pathWithinHull` out of a hull (`FUN_140f8cf00`): from `from` inside hull
    /// `hull`, a search of rays along the hull's boxes (list 1, [`HULL_ITERATIONS`]). Toward a
    /// `goal`, a node that sees the goal (or is within range of it or of the target) ends it in
    /// `paths[0]` (result 0), else the node nearest the goal does (1); without one, each ray that
    /// leaves the hull keeps its way out (`most` paths, result 4, or 5 when the start was inside a
    /// box). 2: nothing found, 3: no first step. From inside a box, the start's region about the
    /// box picks the sides whose points start the walk too (the jump table at 0x140f8d261,
    /// [`HullPather::side_points`]). `FindShortPath`, its only caller, gives it a hull, a start and
    /// paths (its result 6 is for none) and no array for the sides the ways out leave by (its
    /// argument 13).
    pub(super) fn walk_out(&mut self, walk: WalkOut<'_>) -> GenieResult<Within> {
        let WalkOut {
            hull,
            start: from,
            goal,
            target,
            range,
            nudge,
            paths,
            most,
            count,
        } = walk;
        *count = 0;
        let mut left_boxes = false;
        self.empty(NodeList::WALKS)?;
        let mut nearest: Option<NodeIndex> = None;
        let mut best = goal.map_or(Distance::ZERO, |g| walk_length(from, g));
        let (inside, stuck) = self.begin_walk_out(hull, from, goal)?;
        if stuck {
            return Ok(Within {
                result: WalkResult::Stuck,
                left_boxes,
            });
        }
        let mut iterations = 0;
        let mut found: Option<NodeIndex> = None;
        let mut ended_at: Option<NodeIndex> = None;
        'search: loop {
            let node = loop {
                if HULL_ITERATIONS <= iterations || most <= *count {
                    break 'search;
                }
                let Some(n) = self.nodes.checked(NodeList::WALKS.head())?.next else {
                    break 'search;
                };
                iterations += 1;
                self.close(NodeList::WALKS, n)?;
                if self.on_map(self.nodes.checked(n)?.at) {
                    break n;
                }
            };
            let at = self.nodes.checked(node)?.at;
            if !self.in_area(at) {
                break;
            }
            match self.walk_ends(target.as_ref(), goal, at, range) {
                WalkEnd::Goal => {
                    ended_at = Some(node);
                    break 'search;
                }
                WalkEnd::Target => {
                    found = Some(node);
                    break 'search;
                }
                WalkEnd::Going => {}
            }
            if let Some(g) = goal {
                if self.nodes.checked(node)?.to_goal < best {
                    best = self.nodes.checked(node)?.to_goal;
                    nearest = Some(node);
                }
                if !self.blocked(at, g)? {
                    let end = self.add_node(NewNode {
                        list: NodeList::WALKS,
                        open: false,
                        parent: Some(node),
                        position: g,
                        goal: Some(g),
                        hull: -1,
                        vertex: -1,
                        direction: 1,
                        by_place: false,
                    })?;
                    ended_at = end;
                    if end.is_some() {
                        break 'search;
                    }
                    // a node the list cannot take ends the walk at the nearest
                    break 'search;
                }
            }
            left_boxes |=
                self.cast_rays(hull, node, goal, nudge, paths, (count, most))? == Rays::LeftBoxes;
        }
        // the search's end
        // without a goal the walk's ways out are its answer, even past a target within range
        let Some(_) = goal else {
            let result = match (*count, inside) {
                (0, _) => WalkResult::NoWay,
                (_, Some(_)) => WalkResult::WaysFromBox,
                (_, None) => WalkResult::Ways,
            };
            return Ok(Within { result, left_boxes });
        };
        self.finish_walk_out(paths, found.or(ended_at), nearest, left_boxes)
    }

    /// Whether the walk out ends at `at`: within the radius and range of the goal (without a
    /// target), or within the range of the target.
    fn walk_ends(
        &self,
        target: Option<&Target>,
        goal: Option<Point>,
        at: Point,
        range: Distance,
    ) -> WalkEnd {
        match (target, goal) {
            (None, None) => WalkEnd::Going,
            (None, Some(g)) => {
                let d = (at.y() - g.y()) * (at.y() - g.y()) + (at.x() - g.x()) * (at.x() - g.x());
                if d <= (self.radius + range).squared().get() + NUDGE {
                    WalkEnd::Goal
                } else {
                    WalkEnd::Going
                }
            }
            (Some(t), _) => {
                if self.target_gap(t, at) <= range + Distance::new(NUDGE) {
                    WalkEnd::Target
                } else {
                    WalkEnd::Going
                }
            }
        }
    }

    /// The walk out's root node at `from`, and, when the start lies in a box of hull `hull`, its
    /// first steps and the points about that box. Returns the box (if any) and whether the walk
    /// is stuck (no step made).
    fn begin_walk_out(
        &mut self,
        hull: HullIndex,
        from: Point,
        goal: Option<Point>,
    ) -> GenieResult<(Option<usize>, bool)> {
        let hull_path_tuning = &self.by_id.tuning().hull_path;
        let root = self.add_unrefused(UnrefusedNode {
            list: NodeList::WALKS,
            open: true,
            parent: None,
            position: from,
            goal,
            hull: -1,
            direction: -1,
        })?;
        let inside = self.hulls.checked(hull.index())?.box_holding(from);
        let Some(b) = inside else {
            return Ok((inside, false));
        };
        self.close(NodeList::WALKS, root)?;
        let mut made = self.first_steps(hull, root, from, goal)? == Steps::Made;
        let bounds = self.hulls.checked(hull.index())?.objects.checked(b)?.bounds;
        let d = self.radius + self.radius;
        made |= self.side_points(
            hull,
            root,
            bounds,
            from,
            hull_path_tuning.exit_step,
            Some((from, d.get())),
        )? == Steps::Made;
        Ok((inside, !made))
    }

    /// The walk out's result toward a goal: the path to the node it ended at (`end`, result 0),
    /// else to the nearest one (result 1), or none (result 2).
    fn finish_walk_out(
        &mut self,
        paths: &mut [Waypoints],
        end: Option<NodeIndex>,
        nearest: Option<NodeIndex>,
        left_boxes: bool,
    ) -> GenieResult<Within> {
        let Some(n) = end.or(nearest) else {
            return Ok(Within {
                result: WalkResult::NoWay,
                left_boxes,
            });
        };
        self.build_path(paths.checked_mut(0)?, n)?;
        Ok(Within {
            result: if end.is_some() {
                WalkResult::Reached
            } else {
                WalkResult::Nearest
            },
            left_boxes,
        })
    }

    /// The rays of the walk out's `node` (`FUN_140f8cf00`), to each side it may cast to
    /// ([`HullPather::may_cast`]): a ray onto a box of hull `hull` goes on from the box's side; one
    /// that leaves the boxes ends on the hull's edge, where toward a `goal` the walk goes on, and
    /// without one it is a way out (nudged past the edge with `nudge`), kept in `paths` while
    /// `count` is below `most`. [`Rays::LeftBoxes`] when a ray left the boxes.
    pub(super) fn cast_rays(
        &mut self,
        hull: HullIndex,
        node: NodeIndex,
        goal: Option<Point>,
        nudge: bool,
        paths: &mut [Waypoints],
        (count, most): (&mut usize, usize),
    ) -> GenieResult<Rays> {
        let at = self.nodes.checked(node)?.at;
        let mut left_boxes = false;
        for side in 0..4 {
            let n = *self.nodes.checked(node)?;
            if !Self::may_cast(&n, side) {
                continue;
            }
            if let Some((q, b, box_side)) = self
                .hulls
                .checked(hull.index())?
                .ray_to_box(at, side, n.hull)
            {
                let _ = self.add_node(NewNode {
                    list: NodeList::WALKS,
                    open: true,
                    parent: Some(node),
                    position: q,
                    goal,
                    hull: b as i32,
                    vertex: box_side,
                    direction: side,
                    by_place: true,
                })?;
                continue;
            }
            left_boxes = true;
            let Some((mut q, edge)) = self.hulls.checked(hull.index())?.ray_to_hull(at, side)
            else {
                continue;
            };
            if goal.is_some() {
                let _ = self.add_node(NewNode {
                    list: NodeList::WALKS,
                    open: true,
                    parent: Some(node),
                    position: q,
                    goal,
                    hull: -1,
                    vertex: edge as i32,
                    direction: side,
                    by_place: true,
                })?;
                continue;
            }
            if *count < most {
                if nudge {
                    let s = *SIDES.checked(side as usize)?;
                    q = Point::new(q.x() + s.x() * NUDGE, q.y() + s.y() * NUDGE);
                }
                if let Some(end) = self.add_node(NewNode {
                    list: NodeList::WALKS,
                    open: false,
                    parent: Some(node),
                    position: q,
                    goal: None,
                    hull: -1,
                    vertex: -1,
                    direction: -1,
                    by_place: false,
                })? {
                    let _ = self.keep_path(paths, count, end)?;
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
