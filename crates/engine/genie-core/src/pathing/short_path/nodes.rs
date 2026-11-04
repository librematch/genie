//! The search's nodes: the open list, the goal and foot tests, a polygon corner's test and
//! the clip to the box.

use crate::GenieResult;
use crate::pathing::short_path::{
    FixedPoint, Kind, Node, NodeIndex, Poly, PolyIndex, RECT_HEURISTIC, Run, Search, Token,
};
use crate::pathing::{FixedDistance, FixedDistanceSquared, FixedRect, ShapeId, ShapeSide};
use crate::world::slots::CheckedIndex;

/// A polygon corner reached as a goal candidate (`FUN_140fa3860`'s arguments, [`Search::goal_test`]).
#[derive(Clone, Copy, Debug)]
pub(super) struct Corner {
    /// The cost to the corner.
    pub(super) cost: FixedDistance,
    /// The node it was reached from.
    pub(super) parent: NodeIndex,
    /// The run of the polygon walked to it.
    pub(super) run: Run,
    /// The corner's point.
    pub(super) point: FixedPoint,
}

/// A polygon's axis edge whose foot the goal may lie on (`FUN_140fa8210`'s arguments,
/// [`Search::foot_test`]).
#[derive(Clone, Copy, Debug)]
pub(super) struct Foot {
    /// The edge's start.
    pub(super) start: FixedPoint,
    /// The edge's end.
    pub(super) end: FixedPoint,
    /// The edge's x extent: horizontal when not 0.
    pub(super) extent_x: i32,
    /// The run of the polygon walked to the edge's start, the edge included.
    pub(super) run: Run,
    /// The cost at the edge's start.
    pub(super) start_cost: FixedDistance,
    /// The node it was reached from.
    pub(super) parent: NodeIndex,
}

/// `FUN_140fa3bc0`: whether a polygon corner at `v` between sides `prev` and `next` emits a node
/// toward `g`, and the token its raycast ignores.
pub(super) fn corner_emits(
    prev: ShapeSide,
    next: ShapeSide,
    v: FixedPoint,
    g: FixedPoint,
) -> Option<Token> {
    use ShapeSide::{HighX, HighY, LowX, LowY};
    let ([vx, vy], [gx, gy]) = (v.get(), g.get());
    match prev {
        LowY if next == LowX => (gx <= vx && gy <= vy).then_some(Token::NONE),
        LowY => {
            (!(gx <= vx && vy <= gy)).then_some(Token::of_side(if gx <= vx { HighX } else { LowY }))
        }
        HighX if next == LowY => (vx <= gx && gy <= vy).then_some(Token::NONE),
        HighX => {
            (!(gx <= vx && gy <= vy)).then_some(Token::of_side(if vx < gx { HighY } else { HighX }))
        }
        HighY if next == HighX => (vx <= gx && vy <= gy).then_some(Token::NONE),
        HighY => {
            (!(vx <= gx && gy <= vy)).then_some(Token::of_side(if vx <= gx { LowX } else { HighY }))
        }
        LowX if next == HighY => (gx <= vx && vy <= gy).then_some(Token::NONE),
        LowX => {
            (!(vx <= gx && vy <= gy)).then_some(Token::of_side(if gx < vx { LowY } else { LowX }))
        }
    }
}

/// `FUN_140fa16d0`: Cohen-Sutherland: the segment `a`–`b` clipped to the box; None when it lies
/// outside.
pub(super) fn clip(
    mut a: FixedPoint,
    mut b: FixedPoint,
    bx: FixedRect,
) -> Option<(FixedPoint, FixedPoint)> {
    let bx = bx.get();
    let (xmin, xmax) = if bx[2] < bx[0] {
        (bx[2], bx[0])
    } else {
        (bx[0], bx[2])
    };
    let (ymin, ymax) = if bx[3] < bx[1] {
        (bx[3], bx[1])
    } else {
        (bx[1], bx[3])
    };
    let code = |fixed_point: FixedPoint| {
        let [x, y] = fixed_point.get();
        let mut c = if x < xmin {
            1
        } else if xmax < x {
            2
        } else {
            0
        };
        c |= if y < ymin {
            4
        } else if ymax < y {
            8
        } else {
            0
        };
        c
    };
    let part = |num: i32, mul: i32, den: i32| {
        let inner = ((i64::from(num) * i64::from(mul)) >> 22) as i32;
        ((i64::from(inner) << 22) / i64::from(den)) as i32
    };
    let (mut ca, mut cb) = (code(a), code(b));
    let (mut x, mut y) = (0, 0);
    while ca != 0 || cb != 0 {
        if ca & cb != 0 {
            return None;
        }
        let c = if ca != 0 { ca } else { cb };
        // the differences in 32 bits, the products and the division in 64
        let (dx, dy) = (b.x().wrapping_sub(a.x()), b.y().wrapping_sub(a.y()));
        if c & 1 != 0 {
            y = a.y().wrapping_add(part(dy, xmin.wrapping_sub(a.x()), dx));
            x = xmin;
        } else if c & 2 != 0 {
            y = a.y().wrapping_add(part(xmax.wrapping_sub(a.x()), dy, dx));
            x = xmax;
        } else if c & 4 != 0 {
            x = a.x().wrapping_add(part(ymin.wrapping_sub(a.y()), dx, dy));
            y = ymin;
        } else if c & 8 != 0 {
            x = a.x().wrapping_add(part(ymax.wrapping_sub(a.y()), dx, dy));
            y = ymax;
        }
        if c == ca {
            a = FixedPoint::new(x, y);
            ca = code(a);
        } else {
            b = FixedPoint::new(x, y);
            cb = code(b);
        }
    }
    Some((a, b))
}

pub(super) const fn in_box(fixed_point: FixedPoint, fixed_rect: FixedRect) -> bool {
    fixed_rect.x0() <= fixed_point.x()
        && fixed_rect.y0() <= fixed_point.y()
        && fixed_point.x() <= fixed_rect.x1()
        && fixed_point.y() <= fixed_rect.y1()
}

/// `fixed_point` held in the box `fixed_rect` on each axis (`[x0, y0, x1, y1]`).
pub(super) const fn clamp_to_box(fixed_point: FixedPoint, fixed_rect: FixedRect) -> FixedPoint {
    let b = fixed_rect.get();
    FixedPoint::new(
        if b[0] <= fixed_point.x() {
            if b[2] < fixed_point.x() {
                b[2]
            } else {
                fixed_point.x()
            }
        } else {
            b[0]
        },
        if b[1] <= fixed_point.y() {
            if b[3] < fixed_point.y() {
                b[3]
            } else {
                fixed_point.y()
            }
        } else {
            b[1]
        },
    )
}

/// Chebyshev distance (`max(|dx|, |dy|)`), the polygon walks' cost.
pub(super) const fn cheb(from: FixedPoint, to: FixedPoint) -> FixedDistance {
    let dy = (from.y().wrapping_sub(to.y())).wrapping_abs();
    let dx = (from.x().wrapping_sub(to.x())).wrapping_abs();
    FixedDistance::new(if dy < dx { dx } else { dy })
}

impl Search<'_> {
    /// The node at `node_index`.
    pub(super) fn node(&self, node_index: NodeIndex) -> GenieResult<&Node> {
        self.arena.checked(node_index.get())
    }

    /// The node at `node_index`, to change.
    pub(super) fn node_mut(&mut self, node_index: NodeIndex) -> GenieResult<&mut Node> {
        self.arena.checked_mut(node_index.get())
    }

    /// The polygon at `poly_index`.
    pub(super) fn poly(&self, poly_index: PolyIndex) -> GenieResult<&Poly> {
        self.polys.checked(poly_index.get())
    }

    /// The polygon at `poly_index`, to change.
    pub(super) fn poly_mut(&mut self, poly_index: PolyIndex) -> GenieResult<&mut Poly> {
        self.polys.checked_mut(poly_index.get())
    }

    /// No goal node yet, or `fixed_distance` cheaper than the goal node's cost: the game's
    /// `goal == -1 || fixed_distance < arena[goal].fixed_distance`.
    pub(super) fn beats_goal(&self, fixed_distance: FixedDistance) -> GenieResult<bool> {
        Ok(match self.goal_node {
            None => true,
            Some(goal) => fixed_distance < self.node(goal)?.g,
        })
    }

    /// Append a node to the arena (`FUN_140fa3fe0`, `FUN_140fa40d0`, `FUN_140fa4210`,
    /// `FUN_140fa4340`): `f = fixed_distance + h(at)`.
    pub(super) fn add(
        &mut self,
        fixed_distance: FixedDistance,
        parent: NodeIndex,
        at: FixedPoint,
        kind: Kind,
    ) -> Node {
        let node = Node {
            g: fixed_distance,
            f: self.goal.heuristic(at).wrapping_add(fixed_distance),
            idx: NodeIndex::next_of(&self.arena),
            parent: Some(parent),
            at,
            kind,
        };
        self.arena.push(node);
        node
    }

    /// `FUN_140fa2eb0`: into the open list, sorted by f descending, before the equal ones from the
    /// back (so equal f leave first in, first out).
    pub(super) fn push(&mut self, pushed: Node) {
        let at = self
            .open
            .iter()
            .rposition(|node| pushed.f < node.f)
            .map_or(0, |i| i + 1);
        self.open.insert(at, pushed);
    }

    pub(super) fn side_node(
        &mut self,
        fixed_distance: FixedDistance,
        parent: NodeIndex,
        shape: ShapeId,
        side: ShapeSide,
        at: FixedPoint,
    ) {
        let node = self.add(fixed_distance, parent, at, Kind::Side { shape, side });
        self.push(node);
    }

    /// `FUN_140fa3860`: a node at `pt` reached with cost `fixed_distance`: within range and better
    /// than the goal so far, it becomes the goal (true); else, nearer than the best so far, the
    /// best.
    pub(super) fn goal_test(&mut self, corner: Corner) -> GenieResult<bool> {
        let Corner {
            cost: fixed_distance,
            parent,
            run,
            point: pt,
        } = corner;
        let d2 = if RECT_HEURISTIC {
            self.goal.rect_distance2(pt)
        } else {
            let dx = i64::from(self.goal.at.x().wrapping_sub(pt.x()));
            let dy = i64::from(self.goal.at.y().wrapping_sub(pt.y()));
            FixedDistanceSquared::new(i64::from(
                (((dy * dy) >> 22) as i32).wrapping_add(((dx * dx) >> 22) as i32),
            ))
        };
        let kind = Kind::Corner {
            run: Some(run),
            emit: None,
        };
        let better = self.beats_goal(fixed_distance)?;
        if self.goal.range2 < d2 || !better {
            let b = *self.node(self.best)?;
            if d2 < FixedDistanceSquared::of(b.f.wrapping_sub(b.g)) {
                let added = self.add(fixed_distance, parent, pt, kind);
                self.best = added.idx;
            }
            return Ok(false);
        }
        let node = self.add(fixed_distance, parent, pt, kind);
        self.goal_node = Some(node.idx);
        Ok(true)
    }

    /// `FUN_140fa8210`: the goal's foot on an axis edge `a`–`fixed_point` (horizontal when `dx !=
    /// 0`) as a goal candidate.
    pub(super) fn foot_test(&mut self, edge: Foot) -> GenieResult<()> {
        let Foot {
            start: a,
            end: fixed_point,
            extent_x: dx,
            run,
            start_cost: g0,
            parent,
        } = edge;
        let (s, e, gv) = if dx != 0 {
            (a.x(), fixed_point.x(), self.goal.at.x())
        } else {
            (a.y(), fixed_point.y(), self.goal.at.y())
        };
        let low = if e < s { e } else { s };
        let high = if s < e { e } else { s };
        if low <= gv && gv <= high {
            let foot = if dx != 0 {
                FixedPoint::new(gv, a.y())
            } else {
                FixedPoint::new(a.x(), gv)
            };
            let fixed_distance =
                g0.wrapping_add(FixedDistance::new((gv.wrapping_sub(s)).wrapping_abs()));
            let _ = self.goal_test(Corner {
                cost: fixed_distance,
                parent,
                run,
                point: foot,
            })?;
        }
        Ok(())
    }

    /// `FUN_140fa2740`, a popped node's test: the best (lowest estimate) so far; in range and
    /// cheaper than the goal: the new goal (true); no cheaper than the goal: pruned (true).
    pub(super) fn pop_test(&mut self, popped: &Node) -> GenieResult<bool> {
        let fixed_distance = popped.f.wrapping_sub(popped.g);
        let node = *self.node(self.best)?;
        if fixed_distance < node.f.wrapping_sub(node.g) {
            self.best = popped.idx;
        }
        let better = self.beats_goal(popped.g)?;
        if (fixed_distance < self.goal.range
            || self.goal.rect_distance2(popped.at) <= self.goal.range2)
            && better
        {
            self.goal_node = Some(popped.idx);
            return Ok(true);
        }
        Ok(!better)
    }
}
