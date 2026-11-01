//! The obstruction manager's shape mesh (`*(*(DAT_143f0a088 + 8))`): the rectangles the short
//! pather searches around, in fixed point (2^22 units per tile).
//!
//! A shape is an obstruction record's box grown by half a tile (an object that is not moving, or
//! a temporary shape of a search); each of its four sides keeps its contacts with the shapes that
//! touch or overlap it, and each tile the ids of the shapes over it. The game adds a shape with
//! `FUN_140fdb8d0`, removes one with `FUN_140fdc250` and replaces one with `FUN_140fdc730`.
//!
//! Ids, the free list and the order of every list follow the game's history, and the short
//! pather's ties follow that order (`docs/AoE2DE_s_exe_SHORT_PATHER.md` §5), so the mesh keeps them
//! exactly: a freed slot links the next free one through its first word, last in first out; the
//! tile lists and contact lists keep their order of insertion.

use crate::pathing::{FixedDistance, FixedPoint, FixedRect, ShapeId, ShapeSide};
use std::collections::HashSet;

/// One tile in fixed point.
pub const ONE: i32 = 0x40_0000;

/// A contact on a shape's side (28 bytes): where the outline that walks the side meets another
/// shape and goes on along one of its sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contact {
    /// w0: the other shape.
    pub other: ShapeId,
    /// w1: the side of the other shape the outline goes on along.
    pub side: ShapeSide,
    /// w2, w3: the junction.
    pub at: FixedPoint,
    /// w4: the junction along the owner side's axis (x for sides 0 and 2, y for 1 and 3).
    pub along: i32,
    /// w5 (its byte, 0 or 1): a collinear contact at the start of its side in one of the two
    /// shapes.
    pub at_side_start: bool,
    /// w6: half the overlap's smaller extent; the contact counts for a unit only while the
    /// shrinking of the shapes for it stays below this.
    pub half: FixedDistance,
}

/// The mesh.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mesh {
    /// `mesh[0..2]`: every slot; a free one holds the next free slot's id in its first word.
    pub shapes: Vec<FixedRect>,
    /// `mesh[3..5]`: each slot's four sides (side s runs from corner s to corner s + 1).
    pub sides: Vec<[Vec<Contact>; 4]>,
    /// `mesh[6]`: per tile, row by row, the ids of the shapes over it.
    pub tiles: Vec<Vec<ShapeId>>,
    /// `mesh + 0x38`: the map's width in tiles (the map is square).
    pub width: u32,
    /// `mesh + 0x60`: the first free slot (−1 none).
    pub free: Option<ShapeId>,
}

/// Which sides of the existing shape `o` (`lo`) and the new shape `n` (`hi`) a corner of their
/// overlap lies on, and whether it lies on both along one line (`FUN_140fdb8d0`,
/// 0x140fdbcb0..0x140fdbddc, transliterated from the disassembly: each test overwrites what an
/// earlier one set); None when the corner yields no contact.
fn classify(c: FixedPoint, o: FixedRect, n: FixedRect) -> Option<(ShapeSide, ShapeSide, bool)> {
    let [cx, cy] = c.get();
    let (mut lo, mut hi) = (ShapeSide::LowY, ShapeSide::LowY);
    let (mut collinear, mut found) = (false, false);
    let mut by_n_y = false;
    if cx == o.x0() {
        lo = ShapeSide::LowX;
        if cx == n.x0() {
            hi = ShapeSide::LowX;
            collinear = true;
            found = true;
        } else {
            by_n_y = true;
        }
    } else if cx == o.x1() {
        lo = ShapeSide::HighX;
        if cx == n.x1() {
            hi = ShapeSide::HighX;
            collinear = true;
            found = true;
        } else {
            by_n_y = true;
        }
    } else if cx == n.x0() {
        hi = ShapeSide::LowX;
        if cy == o.y0() {
            lo = ShapeSide::LowY;
            if cy == n.y0() {
                hi = ShapeSide::LowY;
                collinear = true;
            }
            found = true;
        } else if cy == o.y1() {
            lo = ShapeSide::HighY;
            if cy == n.y1() {
                hi = ShapeSide::HighY;
                collinear = true;
            }
            found = true;
        }
    } else if cx == n.x1() {
        hi = ShapeSide::HighX;
        if cy == o.y0() {
            lo = ShapeSide::LowY;
            if cy == n.y0() {
                hi = ShapeSide::LowY;
                collinear = true;
            }
            found = true;
        }
        if cy == o.y1() {
            lo = ShapeSide::HighY;
            if cy == n.y1() {
                hi = ShapeSide::HighY;
                collinear = true;
            }
            found = true;
        }
    }
    if by_n_y {
        if cy == n.y0() {
            hi = ShapeSide::LowY;
            if cy == o.y0() {
                lo = ShapeSide::LowY;
                collinear = true;
            }
            found = true;
        } else if cy == n.y1() {
            hi = ShapeSide::HighY;
            if cy == o.y1() {
                lo = ShapeSide::HighY;
                collinear = true;
            }
            found = true;
        }
    }
    found.then_some((lo, hi, collinear))
}

/// The tiles a shape covers (`FUN_140fdb8d0`, `FUN_140fdc250`): from its low edge's tile to the
/// tile of its high edge truncated toward zero, each held to the map; a high edge left of the map
/// counts as its last tile (an unsigned compare).
fn tile_range(r: FixedRect, width: u32) -> (u32, u32, u32, u32) {
    let last = width.wrapping_sub(1);
    let low = |v: i32| if v < 0 { 0 } else { (v >> 22) as u32 };
    let high = |v: i32| {
        let v = if v < 0 {
            v.wrapping_add(0x3f_ffff) & !0x3f_ffff
        } else {
            v
        };
        (v >> 22) as u32
    };
    let hold_low = |t: u32| if last < t { last } else { t };
    let hold_high = |t: u32| if t <= last { t } else { last };
    (
        hold_low(low(r.x0())),
        hold_low(low(r.y0())),
        hold_high(high(r.x1())),
        hold_high(high(r.y1())),
    )
}

/// The overlap of two rectangles (`FUN_14087c550` of the larger lows and smaller highs).
const fn overlap(a: FixedRect, b: FixedRect) -> FixedRect {
    let ([ax0, ay0, ax1, ay1], [bx0, by0, bx1, by1]) = (a.get(), b.get());
    FixedRect::new(
        if ax0 < bx0 { bx0 } else { ax0 },
        if ay0 < by0 { by0 } else { ay0 },
        if bx1 < ax1 { bx1 } else { ax1 },
        if by1 < ay1 { by1 } else { ay1 },
    )
}

/// Half of `v`, truncated: the game's `(v << 22 + bias) >> 23`.
fn half(v: i32) -> FixedDistance {
    let wide = i64::from(v) << 22;
    let bias = (wide >> 63) & 0x7f_ffff;
    FixedDistance::new(((wide + bias) >> 23) as i32)
}

impl Mesh {
    /// An empty mesh for a map `width` tiles wide.
    #[must_use]
    pub fn new(width: u32) -> Self {
        Self {
            shapes: Vec::new(),
            sides: Vec::new(),
            tiles: vec![Vec::new(); (width * width) as usize],
            width,
            free: None,
        }
    }

    /// Whether slot `id` holds a shape (a free slot is all zero past its link).
    #[must_use]
    pub fn is_shape(&self, id: ShapeId) -> bool {
        self.shapes
            .get(id.index())
            .is_some_and(|r| r.y0() != 0 || r.x1() != 0 || r.y1() != 0)
    }

    /// `FUN_140fdb8d0`: a new shape, rounded to even units, in the first free slot or a new one;
    /// the contacts of each corner of its overlap with every shape it overlaps, met tile by tile
    /// in the tiles' order. Returns its id.
    pub fn add(&mut self, rect: FixedRect) -> ShapeId {
        let n = FixedRect::new(
            rect.x0() & !1,
            rect.y0() & !1,
            rect.x1().wrapping_add(1) & !1,
            rect.y1().wrapping_add(1) & !1,
        );
        let id = if let Some(id) = self.free {
            self.free = self
                .shapes
                .get(id.index())
                .and_then(|s| ShapeId::of_link(s.x0()));
            if let Some(slot) = self.shapes.get_mut(id.index()) {
                *slot = n;
            }
            id
        } else {
            self.shapes.push(n);
            self.sides.push(Default::default());
            ShapeId::new((self.shapes.len() - 1) as u32)
        };
        let (tx0, ty0, tx1, ty1) = tile_range(n, self.width);
        let mut visited = HashSet::new();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let t = (ty * self.width + tx) as usize;
                let mut k = 0;
                while let Some(&o) = self.tiles.get(t).and_then(|list| list.get(k)) {
                    if visited.insert(o) {
                        self.touch(o, id);
                    }
                    k += 1;
                }
                if let Some(list) = self.tiles.get_mut(t) {
                    list.push(id);
                }
            }
        }
        id
    }

    /// The contacts between an existing shape `o` and the new shape `n`: one per corner of their
    /// overlap that lies on both outlines.
    fn touch(&mut self, o: ShapeId, n: ShapeId) {
        let (Some(&or), Some(&nr)) = (self.shapes.get(o.index()), self.shapes.get(n.index()))
        else {
            return;
        };
        let i = overlap(nr, or);
        if !(i.x0() < i.x1() && i.y0() < i.y1()) {
            return;
        }
        let w = i.x1() - i.x0();
        let h = i.y1() - i.y0();
        let half = half(if h < w { h } else { w });
        for k in 0..4 {
            let c = i.corner(k);
            let Some((lo, hi, collinear)) = classify(c, or, nr) else {
                continue;
            };
            let along = |side: ShapeSide| if side.along_y() { c.y() } else { c.x() };
            if collinear {
                // the owner: the shape with the junction as a corner, o before n at each corner
                let owner = (0..4).find_map(|j| {
                    if or.corner(j) == c {
                        Some((o, n))
                    } else if nr.corner(j) == c {
                        Some((n, o))
                    } else {
                        None
                    }
                });
                let Some((owner, other)) = owner else {
                    // the game would index slot -1 here (0x140fdbf7a)
                    continue;
                };
                let (Some(&a), Some(&b)) = (
                    self.shapes.get(other.index()),
                    self.shapes.get(owner.index()),
                ) else {
                    continue;
                };
                let flag = match lo {
                    ShapeSide::LowY => c.x() == a.x0() || c.x() == b.x0(),
                    ShapeSide::HighX => c.y() == a.y0() || c.y() == b.y0(),
                    ShapeSide::HighY => c.x() == a.x1() || c.x() == b.x1(),
                    ShapeSide::LowX => c.y() == a.y1() || c.y() == b.y1(),
                };
                self.contact(
                    (owner, lo),
                    Contact {
                        other,
                        side: hi,
                        at: c,
                        along: along(lo),
                        at_side_start: flag,
                        half,
                    },
                );
            } else if hi.next() == lo {
                self.contact(
                    (o, lo),
                    Contact {
                        other: n,
                        side: hi,
                        at: c,
                        along: along(lo),
                        at_side_start: false,
                        half,
                    },
                );
            } else if hi.prev() == lo {
                self.contact(
                    (n, hi),
                    Contact {
                        other: o,
                        side: lo,
                        at: c,
                        along: along(hi),
                        at_side_start: false,
                        half,
                    },
                );
            }
        }
    }

    /// A contact goes last on side `side` of shape `owner`.
    fn contact(&mut self, (owner, side): (ShapeId, ShapeSide), contact: Contact) {
        if let Some(list) = self
            .sides
            .get_mut(owner.index())
            .and_then(|s| s.get_mut(side.index()))
        {
            list.push(contact);
        }
    }

    /// `FUN_140fdc250`: the shape's contacts leave every shape over its tiles, its id leaves the
    /// tiles; the last slot is dropped, any other becomes the first free one.
    pub fn remove(&mut self, id: ShapeId) {
        let Some(&r) = self.shapes.get(id.index()) else {
            return;
        };
        let (tx0, ty0, tx1, ty1) = tile_range(r, self.width);
        let mut visited = HashSet::new();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let t = (ty * self.width + tx) as usize;
                let mut k = 0;
                while let Some(&s) = self.tiles.get(t).and_then(|list| list.get(k)) {
                    if visited.insert(s)
                        && let Some(sides) = self.sides.get_mut(s.index())
                    {
                        for side in sides {
                            side.retain(|c| c.other != id);
                        }
                    }
                    k += 1;
                }
                if let Some(list) = self.tiles.get_mut(t)
                    && let Some(at) = list.iter().position(|&x| x == id)
                {
                    let _ = list.remove(at);
                }
            }
        }
        if id.index() == self.shapes.len() - 1 {
            let _ = self.shapes.pop();
            let _ = self.sides.pop();
        } else {
            if let Some(slot) = self.shapes.get_mut(id.index()) {
                *slot = FixedRect::new(ShapeId::link(self.free), 0, 0, 0);
            }
            for side in self.sides.get_mut(id.index()).into_iter().flatten() {
                side.clear();
            }
            self.free = Some(id);
        }
    }

    /// `FUN_140fdc730`: the shape moved: removed and added again when the new box differs from
    /// the stored one (compared before rounding, so an odd box always differs). Returns its id.
    pub fn replace(&mut self, id: ShapeId, rect: FixedRect) -> ShapeId {
        if self.shapes.get(id.index()) == Some(&rect) {
            return id;
        }
        self.remove(id);
        self.add(rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile_box(x: i32, y: i32, w: i32, h: i32) -> FixedRect {
        FixedRect::new(x * ONE, y * ONE, (x + w) * ONE, (y + h) * ONE)
    }

    #[test]
    fn a_shape_is_rounded_to_even_units_and_listed_on_its_tiles() {
        let mut m = Mesh::new(8);
        let id = m.add(FixedRect::new(ONE + 1, ONE + 3, 2 * ONE + 1, 3 * ONE - 2));
        assert_eq!(id, ShapeId::new(0));
        assert_eq!(
            m.shapes[0],
            FixedRect::new(ONE, ONE + 2, 2 * ONE + 2, 3 * ONE - 2)
        );
        // x from tile 1 to tile 2 (the high edge 2 + 2 units), y from 1 to 2
        let on = |x: u32, y: u32| m.tiles[(y * 8 + x) as usize].contains(&id);
        assert!(on(1, 1) && on(2, 1) && on(1, 2) && on(2, 2));
        assert!(!on(3, 1) && !on(1, 3));
    }

    #[test]
    fn two_boxes_side_by_side_meet_where_their_outlines_cross() {
        // MarvinBlue/426048731 at tick 679: tree shapes 61, (93.5, 44.5)-(95.5, 46.5), and 63, one
        // tile right of and below it; 63's left side crosses 61's bottom at (94.5, 44.5)
        let mut m = Mesh::new(120);
        let at = |x: f32, y: f32| [(x * ONE as f32) as i32, (y * ONE as f32) as i32];
        let rect = |a: [i32; 2], b: [i32; 2]| FixedRect::new(a[0], a[1], b[0], b[1]);
        let a = m.add(rect(at(93.5, 44.5), at(95.5, 46.5)));
        let b = m.add(rect(at(94.5, 43.5), at(96.5, 45.5)));
        // the game's contact on the left shape's bottom: `3f 3 17a00000 b200000 17a00000 0 200000`
        assert_eq!(
            m.sides[a.index()][0],
            [Contact {
                other: b,
                side: ShapeSide::LowX,
                at: FixedPoint::new(0x17a0_0000, 0x0b20_0000),
                along: 0x17a0_0000,
                at_side_start: false,
                half: FixedDistance::new(0x20_0000),
            }]
        );
        // removing the second shape clears every contact with it
        m.remove(b);
        assert_eq!(m.sides[a.index()].iter().map(Vec::len).sum::<usize>(), 0);
        assert_eq!(m.shapes.len(), 1);
    }

    #[test]
    fn a_freed_slot_is_reused_last_in_first_out() {
        let mut m = Mesh::new(16);
        let ids: Vec<ShapeId> = (0..4).map(|i| m.add(tile_box(2 * i, 0, 1, 1))).collect();
        m.remove(ids[1]);
        m.remove(ids[2]);
        assert_eq!(m.free, Some(ids[2]));
        assert_eq!(m.shapes[2], FixedRect::new(1, 0, 0, 0));
        assert_eq!(m.add(tile_box(10, 10, 1, 1)), ids[2]);
        assert_eq!(m.add(tile_box(12, 10, 1, 1)), ids[1]);
        assert_eq!(m.free, None);
        // the last slot is dropped, not freed
        m.remove(ids[3]);
        assert_eq!(m.shapes.len(), 3);
        assert_eq!(m.free, None);
    }

    #[test]
    fn a_replace_with_an_odd_box_always_moves_the_shape() {
        let mut m = Mesh::new(16);
        let a = m.add(tile_box(1, 1, 1, 1));
        assert_eq!(m.replace(a, tile_box(1, 1, 1, 1)), a);
        let odd = FixedRect::new(ONE + 1, ONE, 2 * ONE, 2 * ONE);
        assert_eq!(m.replace(a, odd), a);
        assert_eq!(
            m.shapes[a.index()],
            FixedRect::new(ONE, ONE, 2 * ONE, 2 * ONE)
        );
    }
}
