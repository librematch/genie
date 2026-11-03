//! `explore_terrain` and `unexplore_terrain` over a circle and a rectangle, and the reveal of
//! a rectangle.

use crate::GenieError;
use crate::map::Tile;
use crate::map::TileRect;
use crate::players::PlayerId;
use crate::vision::SightCentre;
use crate::vision::SightTiles;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::visibility::{ANCHOR, LINKED, MOST, Visibility};
use crate::world::slots::CheckedIndex;

impl Visibility {
    // The visible map's routines (`RGE_Visible_Map`).

    /// `explore_terrain` (`FUN_141284a40`): the tiles of a circle of `los` (at most 20) around
    /// tile `origin`, centred on the tile's centre or, for a building whose centre is on a tile
    /// corner, on its corner (`centre`), count one more seer. A tile seen for the first time counts as explored, and its first sight
    /// of a building's tile whose anchor it never saw explores the building's tiles. Returns the
    /// tiles explored.
    pub fn explore(
        &mut self,
        player: PlayerId,
        origin: Tile,
        los: SightTiles,
        centre: SightCentre,
    ) -> Result<TileCount, GenieError> {
        let (x, y) = (origin.x(), origin.y());
        let table = *centre.table().checked(los.row())?;
        let (visible, seen) = (VisionBits::visible(player), VisionBits::explored(player));
        let mut newly = TileCount::ZERO;
        for &(dy, dx0, dx1) in table {
            let ty = y + i32::from(dy);
            if ty < 0 || self.size.height() <= ty {
                continue;
            }
            let x0 = (x + i32::from(dx0)).max(0);
            let x1 = (x + i32::from(dx1)).min(self.size.width() - 1);
            for tx in x0..=x1 {
                let tile = Tile::new(tx, ty);
                let i = self.index(tile);
                let mut c = self
                    .maps
                    .checked(player.index())?
                    .counts
                    .checked(i)?
                    .wrapping_add(1);
                if c == 0 {
                    newly += TileCount::ONE;
                    self.maps.checked_mut(player.index())?.explored += TileCount::ONE;
                    self.new_tile(player, tile);
                    if self
                        .player_kinds
                        .get(player.index())
                        .is_some_and(|t| t.is_computer())
                    {
                        self.ai_tiles.push((player, tile));
                    }
                    if self.links.checked(i)?.0 & (LINKED | ANCHOR) != 0 {
                        self.reveal_building(player, tile)?;
                    }
                    c = 1;
                    *self.unified.checked_mut(i)? |= seen;
                }
                if c == 1 {
                    *self.unified.checked_mut(i)? |= visible;
                }
                *self
                    .maps
                    .checked_mut(player.index())?
                    .counts
                    .checked_mut(i)? = c.min(MOST);
            }
        }
        Ok(newly)
    }

    /// `explore_terrain`'s case of a building's tile (`FUN_140f68f50`, `FUN_140f68e30`): when the
    /// building's anchor was never explored, the tile counts one seer and the building's tiles
    /// count as explored.
    pub(super) fn reveal_building(
        &mut self,
        player: PlayerId,
        tile: Tile,
    ) -> Result<(), GenieError> {
        let Some(anchor) = self.anchor_of(tile) else {
            return Ok(());
        };
        if *self
            .maps
            .checked(player.index())?
            .counts
            .checked(self.index(anchor))?
            != -1
        {
            return Ok(());
        }
        let Some(rect) = self.extent_of(tile) else {
            return Ok(());
        };
        let i = self.index(tile);
        *self
            .maps
            .checked_mut(player.index())?
            .counts
            .checked_mut(i)? = 1;
        let _ = self.reveal_rect(player, rect)?;
        Ok(())
    }

    /// `unexplore_terrain` (`FUN_141287170`): the circle's tiles count one seer less (a tile at 0
    /// stays 0); a tile left without seers is no longer visible.
    pub fn unexplore(
        &mut self,
        player: PlayerId,
        origin: Tile,
        los: SightTiles,
        centre: SightCentre,
    ) -> Result<(), GenieError> {
        let (x, y) = (origin.x(), origin.y());
        let table = *centre.table().checked(los.row())?;
        let hidden = !VisionBits::visible(player);
        for &(dy, dx0, dx1) in table {
            let ty = y + i32::from(dy);
            if ty < 0 || self.size.height() <= ty {
                continue;
            }
            let x0 = (x + i32::from(dx0)).max(0);
            let x1 = (x + i32::from(dx1)).min(self.size.width() - 1);
            for tx in x0..=x1 {
                let i = self.index(Tile::new(tx, ty));
                let c = *self.maps.checked(player.index())?.counts.checked(i)?;
                let c = if c != 0 { c.wrapping_sub(1) } else { 0 };
                if c == 0 {
                    *self.unified.checked_mut(i)? &= hidden;
                }
                *self
                    .maps
                    .checked_mut(player.index())?
                    .counts
                    .checked_mut(i)? = c;
            }
        }
        Ok(())
    }

    /// The tiles of rectangle `rect` held on the map, None when it misses the map.
    pub(super) fn held(&self, rect: TileRect) -> Option<TileRect> {
        let [x0, y0, x1, y1] = rect.get();
        let (w, h) = (self.size.width(), self.size.height());
        (x0 < w && 0 <= x1 && y0 < h && 0 <= y1)
            .then(|| TileRect::new(x0.max(0), y0.max(0), x1.min(w - 1), y1.min(h - 1)))
    }

    /// `explore_terrain_sq` (`FUN_141284f10`): `explore_terrain` over a rectangle, without the
    /// buildings' case.
    pub fn explore_rect(
        &mut self,
        player: PlayerId,
        rect: TileRect,
    ) -> Result<TileCount, GenieError> {
        let Some(held) = self.held(rect) else {
            return Ok(TileCount::ZERO);
        };
        let [x0, y0, x1, y1] = held.get();
        let (visible, seen) = (VisionBits::visible(player), VisionBits::explored(player));
        let mut newly = TileCount::ZERO;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let tile = Tile::new(x, y);
                let i = self.index(tile);
                let mut c = self
                    .maps
                    .checked(player.index())?
                    .counts
                    .checked(i)?
                    .wrapping_add(1);
                if c == 0 {
                    newly += TileCount::ONE;
                    self.maps.checked_mut(player.index())?.explored += TileCount::ONE;
                    self.new_tile(player, tile);
                    c = 1;
                    *self.unified.checked_mut(i)? |= seen;
                }
                if c == 1 {
                    *self.unified.checked_mut(i)? |= visible;
                }
                *self
                    .maps
                    .checked_mut(player.index())?
                    .counts
                    .checked_mut(i)? = c.min(MOST);
            }
        }
        Ok(newly)
    }

    /// `unexplore_terrain_sq` (`FUN_141287480`): `unexplore_terrain` over a rectangle.
    pub fn unexplore_rect(&mut self, player: PlayerId, rect: TileRect) -> Result<(), GenieError> {
        let Some(held) = self.held(rect) else {
            return Ok(());
        };
        let [x0, y0, x1, y1] = held.get();
        let hidden = !VisionBits::visible(player);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let tile = Tile::new(x, y);
                let i = self.index(tile);
                let c = *self.maps.checked(player.index())?.counts.checked(i)?;
                let c = if c != 0 { c.wrapping_sub(1) } else { 0 };
                if c == 0 {
                    *self.unified.checked_mut(i)? &= hidden;
                }
                *self
                    .maps
                    .checked_mut(player.index())?
                    .counts
                    .checked_mut(i)? = c;
            }
        }
        Ok(())
    }

    /// `reveal_terrain_sq` (`FUN_141285da0`): the rectangle's never explored tiles count as
    /// explored.
    pub fn reveal_rect(
        &mut self,
        player: PlayerId,
        rect: TileRect,
    ) -> Result<TileCount, GenieError> {
        let Some(held) = self.held(rect) else {
            return Ok(TileCount::ZERO);
        };
        let [x0, y0, x1, y1] = held.get();
        let seen = VisionBits::explored(player);
        let mut newly = TileCount::ZERO;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let tile = Tile::new(x, y);
                let i = self.index(tile);
                if *self.maps.checked(player.index())?.counts.checked(i)? == -1 {
                    self.maps.checked_mut(player.index())?.explored += TileCount::ONE;
                    newly += TileCount::ONE;
                    self.new_tile(player, tile);
                    *self.unified.checked_mut(i)? |= seen;
                    *self
                        .maps
                        .checked_mut(player.index())?
                        .counts
                        .checked_mut(i)? = 0;
                }
            }
        }
        Ok(newly)
    }
}
