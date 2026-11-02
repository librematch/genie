//! The sight of a combat object's update and its registry, an ownership change and a
//! construction's start and completion.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::ownership::CaptureKind;
use crate::players::PlayerId;
use crate::vision::SightCentre;
use crate::vision::SightShape;
use crate::vision::VisionBits;
use crate::vision::registry::UnitUpdate;
use crate::vision::visibility::Visibility;
use crate::world::player::Player;

/// An object update past the registry check finds the registry gone.
const NO_REGISTRY: GenieError =
    GenieError::unreachable("an object update without the objects-in-sight registry");

/// `RGE_Combat_Object::update`'s takeover by sight (`0x14107dfb0`, once the registry took a
/// changed `mask` of an object with `+0x924` 1, [`Visibility::combat_sight`]): the players from
/// 1 on, in order, each with its type (`player+0x10`, [`Player::kind`]) and
/// its `mAllowCapturingGaiaUnits` (`+0x670`, `allows`, the world's
/// `allow_gaia_capture`), as they stand. Outside the game modes whose descriptor's `+0x9c` is 1
/// (`standard`, `FUN_141302450`) a computer player that does not allow it is passed over; the
/// test `DAT_143e76fc3` guards before (`FUN_140ca6be0` on the AI's table) is never run, the
/// flag being 0 in the image and never written. The first other player than `owner` whose bit
/// is in `mask` ends the loop: it is the answer, which takes the object when it has its master.
#[must_use]
pub fn sight_taker(
    mask: VisionBits,
    owner: PlayerId,
    standard: bool,
    players: &[Player],
    allows: &[bool],
) -> Option<PlayerId> {
    PlayerId::each(players.len())
        .zip(players)
        .skip(1)
        .filter(|&(q, player)| {
            standard
                || !player.kind.is_computer()
                || allows.get(q.index()).copied().unwrap_or(false)
        })
        .map(|(q, _)| q)
        .find(|&q| q != owner && mask.intersects(VisionBits::visible(q)))
}

impl Visibility {
    /// `RGE_Combat_Object::update`'s sight (`0x14107dfb0`, after the actions): the object's mask
    /// is the unified map's word at its tile with those of its UMV entry's tiles on the map; an
    /// annex not yet dead (state below 7) reads its main building's (`main`: its place and UMV
    /// entry), with no test of the map's edge. A changed mask updates the registry with the place
    /// read, the master's radii and class (`Update_Unit_Info`), and is the answer for an object
    /// another player takes over by seeing it (`+0x924` 1, [`sight_taker`]).
    ///
    /// # Panics
    ///
    /// Never: the registry is checked before it is updated.
    pub fn combat_sight(
        &mut self,
        id: ObjectId,
        owner: PlayerId,
        at: Position,
        radius: Footprint,
        class: ClassId,
        main: Option<(Position, u8)>,
    ) -> Result<Option<VisionBits>, GenieError> {
        let Some(s) = self.seers.get(id).copied() else {
            return Ok(None);
        };
        if self.registry.is_none() || id.is_recyclable() {
            return Ok(None);
        }
        let annex = main.filter(|_| s.state < ObjectState::Removed && s.building.is_some());
        let (place, umv, edge) = match annex {
            Some((place, umv)) => (place, umv, false),
            None => (at, s.umv, true),
        };
        let (x, y) = (place.x(), place.y());
        let mut mask = self.word_at(Point::new(x, y))?;
        if umv != 0 {
            let offsets = self
                .extents
                .get(usize::from(umv) - 1)
                .map(|e| e.offsets.clone())
                .ok_or(GenieError::unsupported(
                    "an object's UMV entry beyond the list",
                ))?;
            for pair in offsets {
                let (dx, dy) = (pair.dx(), pair.dy());
                let (fx, fy) = (dx as f32 + x, dy as f32 + y);
                let on_map = 0.0 <= fx
                    && 0.0 <= fy
                    && fx < self.size.width() as f32
                    && fy < self.size.height() as f32;
                if on_map || !edge {
                    mask |= self.word_at(Point::new(fx, fy))?;
                }
            }
        }
        let registry = self.registry.as_mut().ok_or(NO_REGISTRY)?;
        let old = registry.entry(id).map_or(VisionBits::NONE, |e| e.mask);
        if mask == old {
            return Ok(None);
        }
        registry
            .entries
            .get_or_insert_with(id, crate::vision::registry::Entry::default)?
            .mask = mask;
        registry.update_unit(UnitUpdate {
            id,
            owner,
            position: Point::new(x, y),
            radii: radius,
            class,
            old: old & VisionBits::VISIBLE_HALF,
            new: mask,
        })?;
        Ok((s.capture == CaptureKind::BY_SIGHT).then_some(mask))
    }

    /// The registry's part of a combat object's new place (`FUN_14108c320`): its records follow.
    pub fn registry_moved(&mut self, id: ObjectId, class: ClassId, from: Position, to: Position) {
        if id.is_recyclable() || (from.x() == to.x() && from.y() == to.y()) {
            return;
        }
        if let Some(r) = self.registry.as_mut() {
            r.moved(id, class, to.ground());
        }
    }

    /// A master change's registry part (`FUN_14108c990`): the object leaves every list of its
    /// old class and joins those of its new one, with its mask as it was.
    pub fn registry_master(
        &mut self,
        id: ObjectId,
        owner: PlayerId,
        at: Position,
        old: (Footprint, ClassId),
        new: (Footprint, ClassId),
    ) -> Result<(), GenieError> {
        let Some(r) = self.registry.as_mut().filter(|_| !id.is_recyclable()) else {
            return Ok(());
        };
        let mask = r.entry(id).map_or(VisionBits::NONE, |e| e.mask);
        let place = at.ground();
        r.update_unit(UnitUpdate {
            id,
            owner,
            position: place,
            radii: old.0,
            class: old.1,
            old: mask & VisionBits::VISIBLE_HALF,
            new: VisionBits::NONE,
        })?;
        r.update_unit(UnitUpdate {
            id,
            owner,
            position: place,
            radii: new.0,
            class: new.1,
            old: VisionBits::NONE,
            new: mask,
        })
    }

    /// The combat object's destructor (`FUN_141073060`): it leaves every list.
    pub fn registry_leave(
        &mut self,
        id: ObjectId,
        owner: PlayerId,
        at: Position,
        radius: Footprint,
        class: ClassId,
    ) -> Result<(), GenieError> {
        let Some(r) = self.registry.as_mut().filter(|_| !id.is_recyclable()) else {
            return Ok(());
        };
        let mask = r.entry(id).map_or(VisionBits::NONE, |e| e.mask);
        r.update_unit(UnitUpdate {
            id,
            owner,
            position: at.ground(),
            radii: radius,
            class,
            old: mask & VisionBits::VISIBLE_HALF,
            new: VisionBits::NONE,
        })?;
        let _ = r.entries.remove(id);
        Ok(())
    }

    /// An ownership change's records (`FUN_14120f800`, first of all and whatever the new owner's
    /// masters): an object with an id (`+0x8` not below 0) writes the new owner's id into each of
    /// its records (`+0x8b0`..`+0x8f0`), which keep their place in every list.
    pub fn records_owner(&mut self, id: ObjectId, new: PlayerId) -> Result<(), GenieError> {
        match self.registry.as_mut() {
            Some(r) if !id.is_recyclable() => r.owner_changed(id, new),
            _ => Ok(()),
        }
    }

    /// An ownership change's sight (`FUN_14120f800`, once the new owner has the master): an
    /// object that `sees` (state 2 and not inside another) loses its sight on the old owner's map
    /// (slot 0x1d8 with the old owner) and gains it on the new one's (slot 0x1d0) with `los`, the
    /// line of sight of the master of its own the change gave it (a livestock's takes the new
    /// owner's).
    pub fn owner_changed(
        &mut self,
        id: ObjectId,
        position: Position,
        radius: Footprint,
        (new, los): (PlayerId, f32),
        sees: bool,
    ) -> Result<(), GenieError> {
        if !self.seers.contains_key(id) {
            return Ok(());
        }
        if sees {
            self.sight_off(id, position, radius, SightShape::Circle, SightCentre::Tile)?;
        }
        if let Some(s) = self.seers.get_mut(id) {
            s.player = new;
            s.los = Distance::new(los);
        }
        if sees {
            let _ = self.sight_on(id, position, radius, SightShape::Circle, SightCentre::Tile)?;
        }
        Ok(())
    }

    /// A building's construction completes (`FUN_1411dbb70` with 2): once built, the sight of its
    /// footprint goes (slot 0x1d8, square) and that of its line of sight comes (slot 0x1d0), each
    /// on a tile corner when its radii add up to an even number.
    pub fn construction_completed(
        &mut self,
        id: ObjectId,
        position: Position,
        radius: Footprint,
    ) -> Result<(), GenieError> {
        let Some(b) = self.seers.get_mut(id).and_then(|s| s.building.as_mut()) else {
            return Ok(());
        };
        if b.built {
            return Ok(());
        }
        b.built = true;
        self.sight_off(
            id,
            position,
            radius,
            SightShape::Footprint,
            SightCentre::Tile,
        )?;
        let _ = self.sight_on(id, position, radius, SightShape::Circle, SightCentre::Tile)?;
        Ok(())
    }

    /// A foundation's construction starts (`FUN_1411e5670`): its build points become a sliver,
    /// its footprint comes into sight and its tiles link again.
    pub fn construction_started(
        &mut self,
        id: ObjectId,
        position: Position,
        radius: Footprint,
    ) -> Result<(), GenieError> {
        if let Some(b) = self.seers.get_mut(id).and_then(|s| s.building.as_mut()) {
            b.started = true;
        }
        let _ = self.sight_on(
            id,
            position,
            radius,
            SightShape::Footprint,
            SightCentre::Tile,
        )?;
        let _ = self.link_building(position, radius)?;
        Ok(())
    }
}
