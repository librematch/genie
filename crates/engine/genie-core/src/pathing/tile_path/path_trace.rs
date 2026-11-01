//! The path's points: the tiles a node's path crosses into and aims at, traced back from the
//! goal.

use crate::math::fmath::BVector;
use crate::pathing::tile_path::{
    Cell, CellEdges, CellKind, GridLevel, HALF, NodeIndex, PlanTile, Planner,
};

impl Planner {
    /// `FUN_140f7b9e0`: the steps from a child cell to the goal's tile at that level (`to`),
    /// counted only where the goal lies right of or below it; `0xffff` for a cell off the map or
    /// without edges.
    pub(super) fn steps_to(&self, to: PlanTile, level: GridLevel, cell: PlanTile) -> u16 {
        let dy = (to.y() - cell.y()).max(0);
        let dx = (to.x() - cell.x()).max(0);
        let c = self.cell(level, cell);
        if !c.kind.contains(CellKind::OFF_MAP) && !c.edges.is_empty() {
            (dy + dx) as u16
        } else {
            0xffff
        }
    }

    /// `FUN_140f77d00`: the tile of a node cell nearest the goal's tile, child by child down to
    /// level 0 (ties to the earlier child).
    pub(super) fn tile_toward(&self, n: NodeIndex, goal: PlanTile) -> PlanTile {
        let node = self.nodes[n];
        let mut at = node.at;
        for l in (1..=node.level.get()).rev().map(GridLevel::new) {
            let cl = l.down();
            // the children in the order (2x, 2y), (2x, 2y + 1), (2x + 1, 2y), (2x + 1, 2y + 1)
            let [c0, c1, c2, c3] = at.children();
            let to = goal.at_level(l.down());
            let d = [
                self.steps_to(to, cl, c0),
                self.steps_to(to, cl, c2),
                self.steps_to(to, cl, c1),
                self.steps_to(to, cl, c3),
            ];
            let mut best = if d[0] < 9999 { d[0] } else { 9999 };
            let mut which: i32 = if 9998 < d[0] { -1 } else { 0 };
            for (k, &dk) in d.iter().enumerate().skip(1).take(2) {
                if -1 < dk as i16 && (dk as i16) < best as i16 {
                    which = k as i32;
                    best = dk;
                }
            }
            at = if -1 < d[3] as i16 && (d[3] as i16) < best as i16 {
                c3
            } else {
                match which {
                    1 => c2,
                    2 => c1,
                    _ => c0,
                }
            };
        }
        at
    }

    /// The scan along a boundary for the tile nearest `at` that has edge `dir`: first from `at` up
    /// to `hi`, then down to `lo` while nearer than the one found (`FUN_140f775a0`,
    /// `FUN_140f794f0`).
    pub(super) fn nearest_on_boundary(
        &self,
        at: i16,
        lo: i16,
        hi: i16,
        dir: CellEdges,
        tile_at: impl Fn(i16) -> Cell,
    ) -> Option<i16> {
        let mut off: i16 = 9999;
        let mut found = None;
        let mut s = at;
        while s <= hi {
            if tile_at(s).edges.intersects(dir) {
                off = s - at;
                found = Some(s);
                break;
            }
            s += 1;
        }
        let mut s = at;
        while lo <= s && 0 < off {
            if tile_at(s).edges.intersects(dir) {
                return Some(s);
            }
            s -= 1;
            off -= 1;
        }
        found
    }

    /// `FUN_140f775a0`: the tile where a node's path crosses into its parent: on the node's edge
    /// toward the parent, nearest the middle of the smaller of the two cells.
    pub(super) fn crossing(&self, n: NodeIndex) -> Option<PlanTile> {
        let node = self.nodes[n];
        let parent = self.nodes[node.parent?];
        // the levels as the shifts from a cell to its tiles
        let (smaller, l) = if parent.level < node.level {
            (parent.at, parent.level.get())
        } else {
            (node.at, node.level.get())
        };
        let nl = node.level.get();
        let (x, y) = (smaller.x(), smaller.y());
        let (nx, ny) = (node.at.x(), node.at.y());
        let dir = node.dir;
        let span = |v: i16| {
            let lo = v << l;
            if 0 < l {
                (lo, (1 << (l - 1)) + lo, (1 << l) - 1 + lo)
            } else {
                (lo, lo, lo)
            }
        };
        let (mut ox, mut oy) = if dir == CellEdges::N || dir == CellEdges::S {
            let (lo, mid, hi) = span(x);
            let row = if dir == CellEdges::N {
                ny << nl
            } else {
                ((ny + 1) << nl) - 1
            };
            let x = if 0 < l {
                self.nearest_on_boundary(mid, lo, hi, dir, |s| {
                    self.cell(GridLevel::TILES, PlanTile::new(s, row))
                })
                .unwrap_or(mid)
            } else {
                mid
            };
            (x, row)
        } else if dir == CellEdges::W || dir == CellEdges::E {
            let (lo, mid, hi) = span(y);
            let col = if dir == CellEdges::W {
                nx << nl
            } else {
                ((nx + 1) << nl) - 1
            };
            let y = if 0 < l {
                self.nearest_on_boundary(mid, lo, hi, dir, |s| {
                    self.cell(GridLevel::TILES, PlanTile::new(col, s))
                })
                .unwrap_or(mid)
            } else {
                mid
            };
            (col, y)
        } else {
            return None;
        };
        let last = self.last_tile();
        if last.x() < ox {
            ox = last.x();
        }
        if last.y() < oy {
            oy = last.y();
        }
        Some(PlanTile::new(ox, oy))
    }

    /// `FUN_140f794f0`: the tile where the path from the last point `from` toward the look-ahead
    /// `toward` crosses a node's edge into its parent, moved along the edge to the nearest tile
    /// with that edge; -1 along the edge when none has it.
    pub(super) fn aim(&self, n: NodeIndex, toward: PlanTile, from: PlanTile) -> Option<PlanTile> {
        let node = self.nodes[n];
        let parent = self.nodes[node.parent?];
        // the levels as the shifts from a cell to its tiles
        let (smaller, bl) = if parent.level < node.level {
            (parent.at, parent.level.get())
        } else {
            (node.at, node.level.get())
        };
        let nl = node.level.get();
        let (bx, by) = (smaller.x(), smaller.y());
        let (nx, ny) = (node.at.x(), node.at.y());
        let (tx, ty, lx, ly) = (toward.x(), toward.y(), from.x(), from.y());
        let dir = node.dir;
        let (dxl, dyl) = (tx.wrapping_sub(lx), ty.wrapping_sub(ly));
        // where the line meets the edge, `d` tiles from the last point along the other axis
        let along = |base: i16, d: i16, num: i16, den: i16| {
            if d == 0 || den == 0 {
                base
            } else {
                (i32::from(base) + (i32::from(d) * i32::from(num)) / i32::from(den)) as i16
            }
        };
        let (lo, hi, at, fixed) = if dir == CellEdges::N || dir == CellEdges::S {
            let lo = bx << bl;
            let hi = ((bx + 1) << bl) - 1;
            let bottom = ((ny + 1) << nl) - 1;
            let top = ny << nl;
            let last = self.last_tile();
            let hi = hi.min(last.x());
            let bottom = bottom.min(last.y());
            if dir == CellEdges::N {
                let d = if top <= ly { top - ly } else { 0 };
                (lo, hi, along(lx, d, dxl, dyl), top)
            } else if ly <= bottom {
                (lo, hi, along(lx, bottom - ly, dxl, dyl), bottom)
            } else {
                (lo, hi, lx, bottom)
            }
        } else {
            let left = nx << nl;
            let right = ((nx + 1) << nl) - 1;
            let lo = by << bl;
            let hi = ((by + 1) << bl) - 1;
            if dir == CellEdges::W {
                let d = if left <= lx { left - lx } else { 0 };
                (lo, hi, along(ly, d, dyl, dxl), left)
            } else if lx <= right {
                (lo, hi, along(ly, right - lx, dyl, dxl), right)
            } else {
                (lo, hi, ly, right)
            }
        };
        let passes = |s: i16| {
            let c = if dir == CellEdges::N || dir == CellEdges::S {
                self.cell(GridLevel::TILES, PlanTile::new(s, fixed))
            } else {
                self.cell(GridLevel::TILES, PlanTile::new(fixed, s))
            };
            c.edges.intersects(dir)
        };
        let result = if lo < at {
            if at < hi {
                let mut off: i16 = 9999;
                let mut found = -1;
                let mut s = at;
                while s <= hi {
                    if passes(s) {
                        off = s - at;
                        found = s;
                        break;
                    }
                    s += 1;
                }
                let mut cur = at;
                loop {
                    if off < 1 {
                        break found;
                    }
                    if passes(cur) {
                        break cur;
                    }
                    cur -= 1;
                    off -= 1;
                    if cur < lo {
                        break found;
                    }
                }
            } else {
                (lo..=hi).rev().find(|&s| passes(s)).unwrap_or(-1)
            }
        } else {
            (lo..=hi).find(|&s| passes(s)).unwrap_or(-1)
        };
        Some(if dir == CellEdges::N || dir == CellEdges::S {
            PlanTile::new(result, fixed)
        } else {
            PlanTile::new(fixed, result)
        })
    }

    /// Whether the map holds tile `t`.
    pub(super) const fn on_map(&self, t: PlanTile) -> bool {
        let last = self.last_tile();
        0 <= t.x() && t.x() <= last.x() && 0 <= t.y() && t.y() <= last.y()
    }

    /// A tile's centre as a path point.
    pub(super) fn centre(t: PlanTile) -> BVector {
        BVector::new(f32::from(t.x()) + HALF, 0.0, f32::from(t.y()) + HALF)
    }

    /// The path's points from the goal's node back to the start (the loops of `FUN_140f77f20` and
    /// `FUN_140f78ba0`): per node the tile where the path enters its parent, aimed from the last
    /// point toward the crossing two nodes on (the start's tile when there is none).
    pub(super) fn trace_back(&mut self, n: NodeIndex, start: PlanTile) {
        let mut node = n;
        loop {
            let parent = self.nodes[node].parent;
            let toward = parent
                .and_then(|p| self.nodes[p].parent)
                .and_then(|gp| self.crossing(gp))
                .unwrap_or(start);
            if let Some(at) = self.aim(node, toward, self.last) {
                self.points.push(Self::centre(at));
                self.last = at;
            }
            match parent {
                Some(p) => node = p,
                None => break,
            }
        }
    }

    /// The searched cells' open and closed marks cleared and the pool emptied, after the goal's
    /// marks.
    pub(super) fn clear_nodes(&mut self) {
        for node in self.nodes.iter() {
            // the game keeps the byte's low nibble, the four edges
            self.cells[node.cell].edges &= CellEdges::EDGES;
        }
        self.nodes.clear();
    }
}
