//! `explore_terrain` and `unexplore_terrain` over a circle and a rectangle, and the reveal of
//! a rectangle.

use crate::GenieResult;
use crate::map::Tile;
use crate::map::TileRect;
use crate::players::PlayerId;
use crate::vision::SightCentre;
use crate::vision::SightTiles;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::visibility::{ANCHOR, LINKED, MOST, Visibility};
use crate::world::slots::CheckedIndexExt;

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
    ) -> GenieResult<TileCount> {
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
                let index = self.index(tile);
                let mut count = self
                    .maps
                    .checked(player.index())?
                    .counts
                    .checked(index)?
                    .wrapping_add(1);
                if count == 0 {
                    newly += TileCount::ONE;
                    self.maps.checked_mut(player.index())?.explored += TileCount::ONE;
                    self.new_tile(player, tile);
                    if self
                        .player_kinds
                        .get(player.index())
                        .is_some_and(|player_kind| player_kind.is_computer())
                    {
                        self.ai_tiles.push((player, tile));
                    }
                    if self.links.checked(index)?.0 & (LINKED | ANCHOR) != 0 {
                        self.reveal_building(player, tile)?;
                    }
                    count = 1;
                    *self.unified.checked_mut(index)? |= seen;
                }
                if count == 1 {
                    *self.unified.checked_mut(index)? |= visible;
                }
                *self
                    .maps
                    .checked_mut(player.index())?
                    .counts
                    .checked_mut(index)? = count.min(MOST);
            }
        }
        Ok(newly)
    }

    /// `explore_terrain`'s case of a building's tile (`FUN_140f68f50`, `FUN_140f68e30`): when the
    /// building's anchor was never explored, the tile counts one seer and the building's tiles
    /// count as explored.
    pub(super) fn reveal_building(&mut self, player: PlayerId, tile: Tile) -> GenieResult<()> {
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
        let index = self.index(tile);
        *self
            .maps
            .checked_mut(player.index())?
            .counts
            .checked_mut(index)? = 1;
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
    ) -> GenieResult<()> {
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
                let index = self.index(Tile::new(tx, ty));
                let checked = *self.maps.checked(player.index())?.counts.checked(index)?;
                let count = if checked != 0 {
                    checked.wrapping_sub(1)
                } else {
                    0
                };
                if count == 0 {
                    *self.unified.checked_mut(index)? &= hidden;
                }
                *self
                    .maps
                    .checked_mut(player.index())?
                    .counts
                    .checked_mut(index)? = count;
            }
        }
        Ok(())
    }

    /// The tiles of rectangle `rect` held on the map, None when it misses the map.
    pub(super) fn held(&self, rect: TileRect) -> Option<TileRect> {
        let [x0, y0, x1, y1] = rect.get();
        let (width, height) = (self.size.width(), self.size.height());
        (x0 < width && 0 <= x1 && y0 < height && 0 <= y1)
            .then(|| TileRect::new(x0.max(0), y0.max(0), x1.min(width - 1), y1.min(height - 1)))
    }

    /// `explore_terrain_sq` (`FUN_141284f10`): `explore_terrain` over a rectangle, without the
    /// buildings' case.
    pub fn explore_rect(&mut self, player: PlayerId, rect: TileRect) -> GenieResult<TileCount> {
        let Some(held) = self.held(rect) else {
            return Ok(TileCount::ZERO);
        };
        let [x0, y0, x1, y1] = held.get();
        let (visible, seen) = (VisionBits::visible(player), VisionBits::explored(player));
        let mut newly = TileCount::ZERO;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let tile = Tile::new(x, y);
                let index = self.index(tile);
                let mut count = self
                    .maps
                    .checked(player.index())?
                    .counts
                    .checked(index)?
                    .wrapping_add(1);
                if count == 0 {
                    newly += TileCount::ONE;
                    self.maps.checked_mut(player.index())?.explored += TileCount::ONE;
                    self.new_tile(player, tile);
                    count = 1;
                    *self.unified.checked_mut(index)? |= seen;
                }
                if count == 1 {
                    *self.unified.checked_mut(index)? |= visible;
                }
                *self
                    .maps
                    .checked_mut(player.index())?
                    .counts
                    .checked_mut(index)? = count.min(MOST);
            }
        }
        Ok(newly)
    }

    /// `unexplore_terrain_sq` (`FUN_141287480`): `unexplore_terrain` over a rectangle.
    pub fn unexplore_rect(&mut self, player: PlayerId, rect: TileRect) -> GenieResult<()> {
        let Some(held) = self.held(rect) else {
            return Ok(());
        };
        let [x0, y0, x1, y1] = held.get();
        let hidden = !VisionBits::visible(player);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let tile = Tile::new(x, y);
                let index = self.index(tile);
                let checked = *self.maps.checked(player.index())?.counts.checked(index)?;
                let count = if checked != 0 {
                    checked.wrapping_sub(1)
                } else {
                    0
                };
                if count == 0 {
                    *self.unified.checked_mut(index)? &= hidden;
                }
                *self
                    .maps
                    .checked_mut(player.index())?
                    .counts
                    .checked_mut(index)? = count;
            }
        }
        Ok(())
    }

    /// `reveal_terrain_sq` (`FUN_141285da0`): the rectangle's never explored tiles count as
    /// explored.
    pub fn reveal_rect(&mut self, player: PlayerId, rect: TileRect) -> GenieResult<TileCount> {
        let Some(held) = self.held(rect) else {
            return Ok(TileCount::ZERO);
        };
        let [x0, y0, x1, y1] = held.get();
        let seen = VisionBits::explored(player);
        let mut newly = TileCount::ZERO;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let tile = Tile::new(x, y);
                let index = self.index(tile);
                if *self.maps.checked(player.index())?.counts.checked(index)? == -1 {
                    self.maps.checked_mut(player.index())?.explored += TileCount::ONE;
                    newly += TileCount::ONE;
                    self.new_tile(player, tile);
                    *self.unified.checked_mut(index)? |= seen;
                    *self
                        .maps
                        .checked_mut(player.index())?
                        .counts
                        .checked_mut(index)? = 0;
                }
            }
        }
        Ok(newly)
    }
}
