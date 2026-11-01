//! An object's sight on and off (slots 0x1d0 and 0x1d8), and after its move, its state's and
//! its master's change.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::TileRect;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterId;
use crate::objects::masters::ids::MARKET;
use crate::vision::SightTiles;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::visibility::{CIRCLE_MASTER, EDGE, Seer, Visibility, tile_of};
use crate::world::slots::CheckedIndex;

/// An object's own sight, as the base class explores it ([`Visibility::base_sight_on`]'s
/// arguments, `FUN_141192560`'s).
#[derive(Clone, Copy, Debug)]
pub(super) struct SightOn {
    /// The object.
    pub(super) id: ObjectId,
    /// Its seer.
    pub(super) seer: Seer,
    /// Its place (x, y, z).
    pub(super) position: Position,
    /// Its master's radii (x, y).
    pub(super) radius: Footprint,
    /// Whether it explores its footprint as a rectangle.
    pub(super) square: bool,
    /// Its line of sight.
    pub(super) line_of_sight: SightTiles,
    /// The circle's offset.
    pub(super) offset: bool,
}

impl Visibility {
    /// Slot 0x1d0: the object's sight comes, on its owner's map: a circle of its line of sight, or
    /// its footprint when `square` (`FUN_141192560`; a building's `FUN_1411d0190`). Returns the
    /// tiles explored.
    pub fn sight_on(
        &mut self,
        id: ObjectId,
        position: Position,
        radius: Footprint,
        square: bool,
        offset: bool,
    ) -> Result<TileCount, GenieError> {
        let Some(s) = self.seers.get(&id).copied() else {
            return Ok(TileCount::ZERO);
        };
        self.sight_on_los(id, position, radius, square, SightTiles::of(s.los), offset)
    }

    /// Slot 0x1d0 with a line of sight of its own (its fourth argument not −1): a building takes
    /// 5 when a unit enters it (`FUN_141212e10`).
    pub fn sight_on_los(
        &mut self,
        id: ObjectId,
        position: Position,
        radius: Footprint,
        square: bool,
        los: SightTiles,
        offset: bool,
    ) -> Result<TileCount, GenieError> {
        let Some(s) = self.seers.get(&id).copied() else {
            return Ok(TileCount::ZERO);
        };
        let (square, offset) = match s.building {
            Some(b) if !b.built => {
                if !b.started {
                    if let Some(s) = self.seers.get_mut(&id) {
                        s.explored = false;
                    }
                    return Ok(TileCount::ZERO);
                }
                (true, offset)
            }
            Some(_) => {
                // The master whose sight the AI players' market lists follow (`FUN_1411d0190`: id 84).
                if s.master == MARKET
                    && !self
                        .computer
                        .get(s.player.index())
                        .copied()
                        .unwrap_or(false)
                {
                    // its tile goes to the allied computer players' lists (player +0x180)
                    if self.computer.iter().any(|&c| c) {
                        self.unsupported("a market's sight with computer players");
                    }
                }
                (square, Self::building_offset(radius, offset))
            }
            None => (square, offset),
        };
        self.base_sight_on(SightOn {
            id,
            seer: s,
            position,
            radius,
            square,
            line_of_sight: los,
            offset,
        })
    }

    /// `FUN_141192560`.
    pub(super) fn base_sight_on(&mut self, on: SightOn) -> Result<TileCount, GenieError> {
        let SightOn {
            id,
            seer: s,
            position,
            radius,
            square,
            line_of_sight: los,
            offset,
        } = on;
        let square = square && s.master != CIRCLE_MASTER;
        if !los.sees() {
            return Ok(TileCount::ZERO);
        }
        let (x, y) = (position.x(), position.y());
        let newly = if square {
            let (rx, ry) = (radius.x(), radius.y());
            if rx <= 0.0 || ry <= 0.0 {
                return Ok(TileCount::ZERO);
            }
            self.explore_rect(
                s.player,
                TileRect::new(
                    (x - rx) as i32,
                    (y - ry) as i32,
                    ((x + rx) - EDGE) as i32,
                    ((y + ry) - EDGE) as i32,
                ),
            )?
        } else {
            self.explore(s.player, Tile::new(x as i32, y as i32), los, offset)?
        };
        if let Some(s) = self.seers.get_mut(&id) {
            s.explored = true;
        }
        Ok(newly)
    }

    /// Slot 0x1d8: the object's sight goes (`FUN_1411a6f90`; a building's `FUN_1411e6f60`, which
    /// does nothing for a building that never explored).
    pub fn sight_off(
        &mut self,
        id: ObjectId,
        position: Position,
        radius: Footprint,
        square: bool,
        offset: bool,
    ) -> Result<(), GenieError> {
        let Some(s) = self.seers.get(&id).copied() else {
            return Ok(());
        };
        self.sight_off_los(id, position, radius, square, SightTiles::of(s.los), offset)?;
        Ok(())
    }

    /// Slot 0x1d8 with a line of sight of its own (its fourth argument not −1): a building lets 5
    /// go when a unit leaves it (`FUN_141213120`).
    pub fn sight_off_los(
        &mut self,
        id: ObjectId,
        position: Position,
        radius: Footprint,
        square: bool,
        los: SightTiles,
        offset: bool,
    ) -> Result<(), GenieError> {
        let Some(s) = self.seers.get(&id).copied() else {
            return Ok(());
        };
        let offset = match s.building {
            Some(_) if !s.explored => return Ok(()),
            Some(b) if !b.built => {
                if !b.started {
                    return Ok(());
                }
                offset
            }
            Some(_) => Self::building_offset(radius, offset),
            None => offset,
        };
        let square = square && s.master != CIRCLE_MASTER;
        if !los.sees() {
            return Ok(());
        }
        let (x, y) = (position.x(), position.y());
        if square {
            let (rx, ry) = (radius.x(), radius.y());
            if rx <= 0.0 || ry <= 0.0 {
                return Ok(());
            }
            self.unexplore_rect(
                s.player,
                TileRect::new(
                    (x - rx) as i32,
                    (y - ry) as i32,
                    ((x + rx) - EDGE) as i32,
                    ((y + ry) - EDGE) as i32,
                ),
            )?;
        } else {
            self.unexplore(s.player, Tile::new(x as i32, y as i32), los, offset)?;
        }
        Ok(())
    }

    /// `new_position`'s sight (`0x1411a58b0`): when the object's tile changes, its sight leaves
    /// the old tile (whatever its state) and, in state 2, comes to the new one.
    pub fn moved(
        &mut self,
        id: ObjectId,
        from: Position,
        to: Position,
        radius: Footprint,
    ) -> Result<(), GenieError> {
        let tile = |p: Position| {
            [
                tile_of(p.x(), self.size.width()),
                tile_of(p.y(), self.size.height()),
            ]
        };
        if tile(from) == tile(to) {
            return Ok(());
        }
        self.sight_off(id, from, radius, false, false)?;
        if self
            .seers
            .get(&id)
            .is_some_and(|s| s.state == ObjectState::Alive)
        {
            let _ = self.sight_on(id, to, radius, false, false)?;
        }
        Ok(())
    }

    /// `set_state`'s sight (`FUN_14119d710`): leaving state 2 for a higher one (or 0), the
    /// object's sight shrinks to its footprint, which goes with state 7.
    pub fn state_changed(
        &mut self,
        id: ObjectId,
        position: Position,
        radius: Footprint,
        new: ObjectState,
    ) -> Result<(), GenieError> {
        let Some(old) = self.seers.get(&id).map(|s| s.state) else {
            return Ok(());
        };
        if old == new || (new < old && new != ObjectState::Foundation) {
            return Ok(());
        }
        let master_sees = self.seers.get(&id).is_some_and(|s| Distance::ZERO < s.los);
        if old == ObjectState::Alive && (ObjectState::Alive < new || new == ObjectState::Foundation)
        {
            self.sight_off(id, position, radius, false, false)?;
            if master_sees {
                let _ = self.sight_on(id, position, radius, true, false)?;
            }
        }
        if old < ObjectState::Removed && new == ObjectState::Removed && master_sees {
            self.sight_off(id, position, radius, true, false)?;
        }
        if let Some(s) = self.seers.get_mut(&id) {
            s.state = new;
        }
        Ok(())
    }

    /// A master change in a state below 3 (`FUN_1411a6a70`): the old master's sight goes and the
    /// new one's comes, whatever the state.
    pub fn master_changed(
        &mut self,
        id: ObjectId,
        position: Position,
        old_radius: Footprint,
        new_radius: Footprint,
        master: MasterId,
        los: Distance,
    ) -> Result<(), GenieError> {
        self.sight_off(id, position, old_radius, false, false)?;
        if let Some(s) = self.seers.get_mut(&id) {
            s.master = master;
            s.los = los;
        }
        let _ = self.sight_on(id, position, new_radius, false, false)?;
        Ok(())
    }

    /// The master and line of sight of seer `id` without its sight (a master change inside
    /// another, whose unit has no tile, `+0x2e8` 0: its sight comes with them on its way out).
    pub fn seer_master(&mut self, id: ObjectId, master: MasterId, los: Distance) {
        if let Some(s) = self.seers.get_mut(&id) {
            s.master = master;
            s.los = los;
        }
    }

    /// The unified map's word at the tile of point `at` (its x and y truncated), masked
    /// ([`MASK`]).
    pub(super) fn word_at(&self, at: Point) -> Result<VisionBits, GenieError> {
        let tile = Tile::new(at.x() as i32, at.y() as i32);
        Ok(*self.unified.checked(self.index(tile))? & VisionBits::OBJECT_MASK)
    }
}
