//! An object's sight on and off (slots 0x1d0 and 0x1d8), and after its move, its state's and
//! its master's change.

use crate::GenieResult;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::TileRect;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterId;
use crate::vision::SightCentre;
use crate::vision::SightLayer;
use crate::vision::SightShape;
use crate::vision::SightTiles;
use crate::vision::TileCount;
use crate::vision::VisionBits;
use crate::vision::visibility::BuildingSight;
use crate::vision::visibility::{Seer, Visibility, tile_of};
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::VisibilityTuning;
use crate::world::slots::CheckedIndexExt;

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
    /// What it explores: a circle of its line of sight or its footprint.
    pub(super) shape: SightShape,
    /// Its line of sight.
    pub(super) line_of_sight: SightTiles,
    /// Where the circle is centred.
    pub(super) centre: SightCentre,
}

/// The sight of a line of sight of its own (slots 0x1d0 and 0x1d8 with their fourth argument
/// not −1, [`Visibility::sight_on_los`] and [`Visibility::sight_off_los`]): the seer's place,
/// its master's radii, the shape, the line of sight and the centre asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LosSight {
    /// The seer's place (x, y, z).
    pub(crate) position: Position,
    /// Its master's radii (x, y).
    pub(crate) radius: Footprint,
    /// What it explores: a circle of the line of sight or the footprint.
    pub(crate) shape: SightShape,
    /// The line of sight.
    pub(crate) los: SightTiles,
    /// Where the circle is centred.
    pub(crate) centre: SightCentre,
}

impl LosSight {
    /// A circle of line of sight `los` centred on the seer's tile, the sight every caller of
    /// its own line of sight asks for (a garrison's inside sight, a line of sight's upgrade).
    #[must_use]
    pub const fn tile_circle(position: Position, radius: Footprint, los: SightTiles) -> Self {
        Self {
            position,
            radius,
            shape: SightShape::Circle,
            los,
            centre: SightCentre::Tile,
        }
    }
}

/// A seer's master change ([`Visibility::master_changed`]): the new master and its line of
/// sight, and the old and the new master's radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MasterChange {
    /// The new master.
    pub(crate) master: MasterId,
    /// Its line of sight.
    pub(crate) los: Distance,
    /// The old master's radii (x, y).
    pub(crate) old_radius: Footprint,
    /// The new master's radii (x, y).
    pub(crate) new_radius: Footprint,
}

impl Visibility {
    /// Slot 0x1d0: the object's sight comes, on its owner's map, of `shape`: a circle of its line
    /// of sight centred at `centre`, or its footprint (`FUN_141192560`; a building's
    /// `FUN_1411d0190`). Returns the tiles explored.
    pub fn sight_on(
        &mut self,
        id: ObjectId,
        position: Position,
        radius: Footprint,
        shape: SightShape,
        centre: SightCentre,
        sight: SightLayer<'_>,
    ) -> GenieResult<TileCount> {
        let SightLayer {
            roles,
            visibility_tuning,
        } = sight;
        let Some(seer) = self.seers.get(id).copied() else {
            return Ok(TileCount::ZERO);
        };
        self.sight_on_los(
            id,
            LosSight {
                position,
                radius,
                shape,
                los: SightTiles::of(seer.sight_radius()),
                centre,
            },
            roles,
            visibility_tuning,
        )
    }

    /// Slot 0x1d0 with a line of sight of its own (its fourth argument not −1): a building takes
    /// 5 when a unit enters it (`FUN_141212e10`).
    pub fn sight_on_los(
        &mut self,
        id: ObjectId,
        sight: LosSight,
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> GenieResult<TileCount> {
        let LosSight {
            position,
            radius,
            shape,
            los,
            centre,
        } = sight;
        let Some(found) = self.seers.get(id).copied() else {
            return Ok(TileCount::ZERO);
        };
        let (shape, centre) = match found.building {
            Some(building_sight) if building_sight != BuildingSight::Built => {
                if building_sight == BuildingSight::Unstarted {
                    if let Some(seer) = self.seers.get_mut(id) {
                        seer.explored = false;
                    }
                    return Ok(TileCount::ZERO);
                }
                (SightShape::Footprint, centre)
            }
            Some(_) => {
                // The master whose sight the AI players' market lists follow (`FUN_1411d0190`: id 84).
                if roles.of_master(found.master).contains(MasterRoles::MARKET)
                    && !self
                        .player_kinds
                        .get(found.player.index())
                        .is_some_and(|player_kind| player_kind.is_computer())
                {
                    // its tile goes to the allied computer players' lists (player +0x180)
                    if self
                        .player_kinds
                        .iter()
                        .any(|player_kind| player_kind.is_computer())
                    {
                        self.unsupported("a market's sight with computer players");
                    }
                }
                (shape, Self::building_centre(radius, centre))
            }
            None => (shape, centre),
        };
        self.base_sight_on(
            SightOn {
                id,
                seer: found,
                position,
                radius,
                shape,
                line_of_sight: los,
                centre,
            },
            roles,
            visibility_tuning,
        )
    }

    /// `FUN_141192560`.
    pub(super) fn base_sight_on(
        &mut self,
        on: SightOn,
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> GenieResult<TileCount> {
        let SightOn {
            id,
            seer,
            position,
            radius,
            shape,
            line_of_sight: los,
            centre,
        } = on;
        let shape = if roles
            .of_master(seer.master)
            .contains(MasterRoles::CIRCLE_SIGHT)
        {
            SightShape::Circle
        } else {
            shape
        };
        if !los.sees() {
            return Ok(TileCount::ZERO);
        }
        let (x, y) = (position.x(), position.y());
        let newly = if shape == SightShape::Footprint {
            let (rx, ry) = (radius.x(), radius.y());
            if rx <= 0.0 || ry <= 0.0 {
                return Ok(TileCount::ZERO);
            }
            self.explore_rect(
                seer.player,
                TileRect::new(
                    (x - rx) as i32,
                    (y - ry) as i32,
                    ((x + rx) - visibility_tuning.edge) as i32,
                    ((y + ry) - visibility_tuning.edge) as i32,
                ),
            )?
        } else {
            self.explore(seer.player, Tile::new(x as i32, y as i32), los, centre)?
        };
        if let Some(seer) = self.seers.get_mut(id) {
            seer.explored = true;
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
        shape: SightShape,
        centre: SightCentre,
        sight: SightLayer<'_>,
    ) -> GenieResult<()> {
        let SightLayer {
            roles,
            visibility_tuning,
        } = sight;
        let Some(seer) = self.seers.get(id).copied() else {
            return Ok(());
        };
        self.sight_off_los(
            id,
            LosSight {
                position,
                radius,
                shape,
                los: SightTiles::of(seer.sight_radius()),
                centre,
            },
            roles,
            visibility_tuning,
        )?;
        Ok(())
    }

    /// Slot 0x1d8 with a line of sight of its own (its fourth argument not −1): a building lets 5
    /// go when a unit leaves it (`FUN_141213120`).
    pub fn sight_off_los(
        &mut self,
        id: ObjectId,
        sight: LosSight,
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> GenieResult<()> {
        let LosSight {
            position,
            radius,
            shape,
            los,
            centre,
        } = sight;
        let Some(seer) = self.seers.get(id).copied() else {
            return Ok(());
        };
        let centre = match seer.building {
            Some(_) if !seer.explored => return Ok(()),
            Some(building_sight) if building_sight != BuildingSight::Built => {
                if building_sight == BuildingSight::Unstarted {
                    return Ok(());
                }
                centre
            }
            Some(_) => Self::building_centre(radius, centre),
            None => centre,
        };
        let shape = if roles
            .of_master(seer.master)
            .contains(MasterRoles::CIRCLE_SIGHT)
        {
            SightShape::Circle
        } else {
            shape
        };
        if !los.sees() {
            return Ok(());
        }
        let (x, y) = (position.x(), position.y());
        if shape == SightShape::Footprint {
            let (rx, ry) = (radius.x(), radius.y());
            if rx <= 0.0 || ry <= 0.0 {
                return Ok(());
            }
            self.unexplore_rect(
                seer.player,
                TileRect::new(
                    (x - rx) as i32,
                    (y - ry) as i32,
                    ((x + rx) - visibility_tuning.edge) as i32,
                    ((y + ry) - visibility_tuning.edge) as i32,
                ),
            )?;
        } else {
            self.unexplore(seer.player, Tile::new(x as i32, y as i32), los, centre)?;
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
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> GenieResult<()> {
        let tile = |position: Position| {
            [
                tile_of(position.x(), self.size.width()),
                tile_of(position.y(), self.size.height()),
            ]
        };
        if tile(from) == tile(to) {
            return Ok(());
        }
        self.sight_off(
            id,
            from,
            radius,
            SightShape::Circle,
            SightCentre::Tile,
            SightLayer {
                roles,
                visibility_tuning,
            },
        )?;
        if self
            .seers
            .get(id)
            .is_some_and(|seer| seer.state == ObjectState::Alive)
        {
            let _ = self.sight_on(
                id,
                to,
                radius,
                SightShape::Circle,
                SightCentre::Tile,
                SightLayer {
                    roles,
                    visibility_tuning,
                },
            )?;
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
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> GenieResult<()> {
        let Some(old) = self.seers.get(id).map(|old_seer| old_seer.state) else {
            return Ok(());
        };
        if old == new || (new < old && new != ObjectState::Foundation) {
            return Ok(());
        }
        let master_sees = self
            .seers
            .get(id)
            .is_some_and(|seer| Distance::ZERO < seer.los);
        if old == ObjectState::Alive && (ObjectState::Alive < new || new == ObjectState::Foundation)
        {
            self.sight_off(
                id,
                position,
                radius,
                SightShape::Circle,
                SightCentre::Tile,
                SightLayer {
                    roles,
                    visibility_tuning,
                },
            )?;
            if master_sees {
                let _ = self.sight_on(
                    id,
                    position,
                    radius,
                    SightShape::Footprint,
                    SightCentre::Tile,
                    SightLayer {
                        roles,
                        visibility_tuning,
                    },
                )?;
            }
        }
        if old < ObjectState::Removed && new == ObjectState::Removed && master_sees {
            self.sight_off(
                id,
                position,
                radius,
                SightShape::Footprint,
                SightCentre::Tile,
                SightLayer {
                    roles,
                    visibility_tuning,
                },
            )?;
        }
        if let Some(seer) = self.seers.get_mut(id) {
            seer.state = new;
        }
        Ok(())
    }

    /// A master change in a state below 3 (`FUN_1411a6a70`): the old master's sight goes and the
    /// new one's comes, whatever the state.
    pub fn master_changed(
        &mut self,
        id: ObjectId,
        position: Position,
        change: MasterChange,
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> GenieResult<()> {
        let MasterChange {
            master,
            los,
            old_radius,
            new_radius,
        } = change;
        self.sight_off(
            id,
            position,
            old_radius,
            SightShape::Circle,
            SightCentre::Tile,
            SightLayer {
                roles,
                visibility_tuning,
            },
        )?;
        if let Some(seer) = self.seers.get_mut(id) {
            seer.master = master;
            seer.los = los;
        }
        let _ = self.sight_on(
            id,
            position,
            new_radius,
            SightShape::Circle,
            SightCentre::Tile,
            SightLayer {
                roles,
                visibility_tuning,
            },
        )?;
        Ok(())
    }

    /// The sight powerup of seer `id` (`+0xad4`), set between its sight's going and coming
    /// ([`crate::map::spatial::Spatial::raise_sight_powerup`]).
    pub fn set_los_powerup(&mut self, id: ObjectId, powerup: Distance) {
        if let Some(seer) = self.seers.get_mut(id) {
            seer.los_powerup = powerup;
        }
    }

    /// The master and line of sight of seer `id` without its sight (a master change inside
    /// another, whose unit has no tile, `+0x2e8` 0: its sight comes with them on its way out).
    pub fn seer_master(&mut self, id: ObjectId, master: MasterId, los: Distance) {
        if let Some(seer) = self.seers.get_mut(id) {
            seer.master = master;
            seer.los = los;
        }
    }

    /// The unified map's word at the tile of point `at` (its x and y truncated), masked
    /// ([`VisionBits::OBJECT_MASK`]).
    pub(super) fn word_at(&self, at: Point) -> GenieResult<VisionBits> {
        let tile = Tile::new(at.x() as i32, at.y() as i32);
        Ok(*self.unified.checked(self.index(tile))? & VisionBits::OBJECT_MASK)
    }
}
