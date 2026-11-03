//! A building's tile links (the map tile's `+0x19` bits) and its UMV extent entry.

use crate::GenieError;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::Tile;
use crate::map::TileOffset;
use crate::map::TileRect;
use crate::vision::SightCentre;
use crate::vision::visibility::{ANCHOR, Extent, LINKED, STEP, Visibility};
use crate::world::layer::generated::VisibilityTuning;
use crate::world::slots::CheckedIndex;

impl Visibility {
    // The map's building links (`RGE_Map`).

    /// `FUN_140f68f50`: from a tile, step back along the directions to the first tile that is not
    /// a building's non-anchor tile.
    #[must_use]
    pub fn anchor_of(&self, from: Tile) -> Option<Tile> {
        let mut tile = from;
        while self.size.holds(tile) {
            let (bits, byte) = *self.links.get(self.index(tile))?;
            if bits & LINKED == 0 {
                return Some(tile);
            }
            if bits & (LINKED | ANCHOR) != LINKED {
                return None;
            }
            let (sx, sy) = (
                *STEP.get(usize::from(byte & 0xf))?,
                *STEP.get(usize::from(byte >> 4))?,
            );
            tile = tile.stepped_back(TileOffset::new(sx, sy));
            if sx == 0 && sy == 0 {
                return None;
            }
        }
        None
    }

    /// Whether `tile` is a building's tile: its map tile's `+0x19` holds the linked or the
    /// anchor bit (`& 0x60`).
    #[must_use]
    pub fn building_tile(&self, tile: Tile) -> bool {
        self.size.holds(tile)
            && self
                .links
                .get(self.index(tile))
                .is_some_and(|&(bits, _)| bits & (LINKED | ANCHOR) != 0)
    }

    /// `FUN_140f68e30`: the building's tile rectangle from one of its tiles, through its
    /// anchor's UMV entry.
    #[must_use]
    pub fn extent_of(&self, from: Tile) -> Option<TileRect> {
        let mut tile = from;
        loop {
            if !self.size.holds(tile) {
                return None;
            }
            let (bits, byte) = *self.links.get(self.index(tile))?;
            match bits & (LINKED | ANCHOR) {
                ANCHOR => {
                    if byte == 0 {
                        return None;
                    }
                    let [g0, g1, g2, g3, ..] =
                        self.extents.get(usize::from(byte) - 1)?.guid.to_le_bytes();
                    let b = |v: u8| i32::from(v as i8);
                    let (x, y) = (tile.x(), tile.y());
                    return Some(TileRect::new(x - b(g0), y - b(g1), x + b(g2), y + b(g3)));
                }
                LINKED => {}
                _ => return None,
            }
            let (sx, sy) = (
                *STEP.get(usize::from(byte & 0xf))?,
                *STEP.get(usize::from(byte >> 4))?,
            );
            tile = tile.stepped_back(TileOffset::new(sx, sy));
            if sx == 0 && sy == 0 {
                return None;
            }
        }
    }

    /// `FUN_140f68650`: the UMV entry (from 1) of the extent of rectangle `rect` around tile
    /// `anchor`, added when the list has none; 0 for a rectangle of one tile or off the map.
    pub fn extent_entry(&mut self, anchor: Tile, rect: TileRect) -> i32 {
        let (ax, ay) = (anchor.x(), anchor.y());
        let (x0, y0, x1, y1) = (rect.x0(), rect.y0(), rect.x1(), rect.y1());
        let (lo_x, hi_x) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (lo_y, hi_y) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        if self.size.width() <= lo_x
            || self.size.height() <= lo_y
            || hi_x < 0
            || hi_y < 0
            || (lo_x == hi_x && lo_y == hi_y)
        {
            return 0;
        }
        let byte = |v: i32| v as u8;
        let guid = u32::from_le_bytes([
            byte(ax - lo_x),
            byte(ay - lo_y),
            byte(hi_x - ax),
            byte(hi_y - ay),
        ]);
        if 0 < self.extents_capacity
            && let Some(k) = self.extents.iter().position(|e| e.guid == guid)
        {
            return k as i32 + 1;
        }
        if self.extents_capacity <= self.extents.len() as i32 || self.extents_capacity == 0 {
            self.extents_capacity += 8;
        }
        let w = hi_x - lo_x + 1;
        let nx = match w {
            1 => 1,
            2 => 2,
            w if 2 < w => (w + 1) >> 1,
            _ => 0,
        };
        let h = hi_y - lo_y + 1;
        let (ny, slots) = match h {
            1 => (1, nx),
            2 => (2, nx * 2),
            h if 2 < h => {
                let ny = (h + 1) >> 1;
                (ny, (nx + ny) * 2 - 4)
            }
            _ => (0, 0),
        };
        let capacity = slots + 1;
        let mut offsets: Vec<TileOffset> = Vec::new();
        let push = |offsets: &mut Vec<TileOffset>, dx: i32, dy: i32| {
            let o = TileOffset::new(dx, dy);
            if (dx != 0 || dy != 0) && (offsets.len() as i32) < capacity && !offsets.contains(&o) {
                offsets.push(o);
            }
        };
        let (dx0, dx1) = (lo_x - ax, hi_x - ax);
        let (mut dy0, mut dy1) = (lo_y - ay, hi_y - ay);
        push(&mut offsets, dx0, dy0);
        push(&mut offsets, dx1, dy0);
        push(&mut offsets, dx0, dy1);
        push(&mut offsets, dx1, dy1);
        if 2 < nx {
            let (mut a, mut b) = (dx0, dx1);
            for _ in 0..nx - 2 {
                a += 2;
                b -= 2;
                push(&mut offsets, a, dy0);
                if 1 < ny {
                    push(&mut offsets, b, dy1);
                }
            }
        }
        if 2 < ny {
            for _ in 0..ny - 2 {
                dy1 -= 2;
                dy0 += 2;
                push(&mut offsets, dx0, dy0);
                if 1 < nx {
                    push(&mut offsets, dx1, dy1);
                }
            }
        }
        self.extents.push(Extent {
            guid,
            offsets,
            capacity,
        });
        self.extents.len() as i32
    }

    /// `FUN_140f73900`: a building's tiles, `x0`..=`x1` by `y0`..=`y1` (held on the map), link
    /// to its anchor (`ax`, `ay`), which takes the building's UMV entry. A building of one tile
    /// links nothing; nor does an anchor outside the rectangle. Returns the anchor's entry.
    pub fn link_tiles(&mut self, anchor: Tile, rect: TileRect) -> Result<i32, GenieError> {
        let (ax, ay) = (anchor.x(), anchor.y());
        let (x0, y0, x1, y1) = (rect.x0(), rect.y0(), rect.x1(), rect.y1());
        if x0 == x1 && y0 == y1 {
            return Ok(0);
        }
        let (lo_x, hi_x) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (lo_y, hi_y) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let inside = lo_x <= ax && ax <= hi_x && lo_y <= ay && ay <= hi_y;
        let Some(held) = self
            .held(TileRect::new(lo_x, lo_y, hi_x, hi_y))
            .filter(|_| inside)
        else {
            return Ok(0);
        };
        let [x0, y0, x1, y1] = held.get();
        let nibble = |v: i32| (v.clamp(-8, 7) & 0xf) as u8;
        let mut entry = 0;
        for y in y0..=y1 {
            let dy = y - ay;
            for x in x0..=x1 {
                let dx = x - ax;
                let i = self.index(Tile::new(x, y));
                if dx == 0 && dy == 0 {
                    entry = self.extent_entry(anchor, rect);
                    *self.links.checked_mut(i)? = (
                        (self.links.checked_mut(i)?.0 & !LINKED) | ANCHOR,
                        entry as u8,
                    );
                } else {
                    let byte = nibble(dx) | nibble(dy) << 4;
                    *self.links.checked_mut(i)? =
                        ((self.links.checked_mut(i)?.0 & !ANCHOR) | LINKED, byte);
                }
            }
        }
        Ok(entry)
    }

    /// `FUN_140f65580`: the rectangle's tiles lose their links.
    pub fn unlink_tiles(&mut self, rect: TileRect) -> Result<(), GenieError> {
        let (x0, y0, x1, y1) = (rect.x0(), rect.y0(), rect.x1(), rect.y1());
        if x0 == x1 && y0 == y1 {
            return Ok(());
        }
        let (lo_x, hi_x) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (lo_y, hi_y) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let Some(held) = self.held(TileRect::new(lo_x, lo_y, hi_x, hi_y)) else {
            return Ok(());
        };
        let [x0, y0, x1, y1] = held.get();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = self.index(Tile::new(x, y));
                *self.links.checked_mut(i)? =
                    (self.links.checked_mut(i)?.0 & !(LINKED | ANCHOR), 0);
            }
        }
        Ok(())
    }

    /// A building's footprint as `FUN_1411e6780` links it: from its low edge to its high edge
    /// less 0.01, around the tile of its centre.
    pub(super) fn footprint(
        position: Position,
        radius: Footprint,
        visibility_tuning: VisibilityTuning,
    ) -> (Tile, TileRect) {
        let (x, y) = (position.x(), position.y());
        let (rx, ry) = (radius.x(), radius.y());
        (
            Tile::new(x as i32, y as i32),
            TileRect::new(
                (x - rx) as i32,
                (y - ry) as i32,
                ((x + rx) - visibility_tuning.edge) as i32,
                ((y + ry) - visibility_tuning.edge) as i32,
            ),
        )
    }

    /// A building (not an annex) on its tile links its footprint (`FUN_1411e6780`, after the
    /// base `new_position`); returns its UMV entry (`+0x904`).
    pub fn link_building(
        &mut self,
        position: Position,
        radius: Footprint,
        visibility_tuning: VisibilityTuning,
    ) -> Result<i32, GenieError> {
        let (anchor, rect) = Self::footprint(position, radius, visibility_tuning);
        self.link_tiles(anchor, rect)
    }

    /// A building's UMV entry for its footprint (`FUN_1411e6780`'s `FUN_140f68650`), without
    /// linking its tiles.
    pub fn building_entry(
        &mut self,
        position: Position,
        radius: Footprint,
        visibility_tuning: VisibilityTuning,
    ) -> i32 {
        let (anchor, rect) = Self::footprint(position, radius, visibility_tuning);
        self.extent_entry(anchor, rect)
    }

    /// The building leaves its tiles (`FUN_1411e6780` before a move, `FUN_1411dbb70`).
    pub fn unlink_building(
        &mut self,
        position: Position,
        radius: Footprint,
        visibility_tuning: VisibilityTuning,
    ) -> Result<(), GenieError> {
        let (_, rect) = Self::footprint(position, radius, visibility_tuning);
        self.unlink_tiles(rect)
    }

    // An object's sight (its slots 0x1d0 and 0x1d8).

    /// A building's slots pick the table: one whose radii add up to an odd whole number centres
    /// on a tile (the caller's table), any other on a tile corner (`DAT_143f51790`, probed 0,
    /// would pick the centred one).
    pub(super) fn building_centre(radius: Footprint, centre: SightCentre) -> SightCentre {
        if ((radius.x() + radius.y()) as i32) & 1 != 0 {
            centre
        } else {
            SightCentre::Corner
        }
    }
}
