//! The artifact's owner search, an ownership change, vision and sight, ghosts and the standable
//! spots.

use crate::GenieError;
use crate::construction::MainBuildingId;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::Rect;
use crate::map::TerrainMask;
use crate::map::Tile;
use crate::map::spatial::TileSight;
use crate::map::spatial::{BLOCKED, Spatial};
use crate::movement::footprint_gap_squared;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::game_status::GameStatus;
use crate::vision::own_masks;
use crate::vision::visibility::Visibility;
use crate::vision::{VisionBits, VisionMasks};
use crate::world::index::ObjectsRef;
use crate::world::player::Player;
impl Spatial {
    /// The Artifact action's owner search (`FUN_140aabc40`) for the animal `me`: the objects on
    /// the lists of the tiles within the animal's radius (whole) plus 3 tiles of its own tile
    /// (row by row, each list in its order; a full tile's list lacks the objects it did not file),
    /// combat units (type 70) in state 2 of a player that is not out (its game status, `+0x89`,
    /// by player in `game_status`, is 0), Gaia's only with `+0x924` 0, of no class of 1, 60, 42,
    /// 11, 14, 30, 58, 3, 52, 27, 39, 49, 37, 9, 10 or 38, counted per player with the nearest
    /// footprint gap (squared). A unit of its own owner keeps it, unless that owner's attribute
    /// 97 (of `players`, as they stand) is above 0 and it is livestock (class 58), which then takes
    /// it from nobody; a player whose attribute 97 is above 0 counts 100 more (the most units then
    /// win). One other player takes it; of several, the nearest (Gaia never); a player out of the
    /// game takes it never. A frozen unit that counts and may have moved onto the tiles fails it.
    pub fn artifact_owner(
        &self,
        me: ObjectId,
        players: &[Player],
    ) -> Result<Option<PlayerId>, GenieError> {
        const KEPT: [ClassId; 16] = [
            class::ARTIFACT,
            class::CLASS_60,
            class::RELIC,
            class::CLASS_11,
            class::CLASS_14,
            class::CLASS_30,
            class::LIVESTOCK,
            class::BUILDING,
            class::TOWER,
            class::WALL,
            class::GATE,
            class::FARM,
            class::CLASS_37,
            class::PREY,
            class::PREDATOR,
            class::CLASS_38,
        ];
        // a world without the players' sight (the unit tests') holds nobody near
        let Some(v) = self.sight.as_ref() else {
            return Ok(None);
        };
        // an animal inside another object (in this update, its Artifact's Enter got it in)
        // searches about its record there
        let p =
            self.objects
                .get(me)
                .or_else(|| self.inside.get(me))
                .ok_or(GenieError::unsupported(
                    "an owner search of an object the world does not hold",
                ))?;
        let reach = (p.radius.x() as i32 as i16).wrapping_add(3);
        let (x, y) = (p.position.x(), p.position.y());
        let (x0, x1) = (
            (x as i32 as i16).wrapping_sub(reach),
            (x as i32 as i16).wrapping_add(reach),
        );
        let (y0, y1) = (
            (y as i32 as i16).wrapping_sub(reach),
            (y as i32 as i16).wrapping_add(reach),
        );
        let (x0, y0) = (i32::from(x0.max(0)), i32::from(y0.max(0)));
        let x1 = i32::from(x1).min(self.map.width() - 1);
        let y1 = i32::from(y1).min(self.map.height() - 1);
        let out = |o: usize| {
            players
                .get(o)
                .is_some_and(|q| q.game_status != GameStatus::PLAYING)
        };
        // a frozen unit that could count (of the combat type and a class that counts) may have
        // moved onto the tiles
        let area = Rect::new(x0 as f32, y0 as f32, (x1 + 1) as f32, (y1 + 1) as f32);
        let unsure = self.frozen().any(|(id, q)| {
            q.unknown.is_some_and(|u| {
                let counts = v.seer(id).is_some_and(|s| s.kind == MasterKind::COMBAT)
                    && !out(q.player.index())
                    && !KEPT.contains(&q.class);
                // the box grown by the unknown reach (`bx1 + u`: the sum only meets a
                // comparison, so its operands' order does not show)
                counts && 0.0 < u && area.overlaps(q.bounds().grown(Distance::new(u)))
            })
        });
        if unsure {
            return Err(GenieError::unsupported(
                "an owner search near a stopped unit that counts",
            ));
        }
        let attribute_97 = |o: usize| players.get(o).and_then(|q| q.attributes.get(97).copied());
        let players = self.relations.len().max(players.len());
        let mut counts = vec![0i32; players];
        let mut nearest = vec![DistanceSquared::new(999.0); players];
        let mut own_seen = false;
        let mut taker = false;
        let livestock = p.class == class::LIVESTOCK;
        // the tiles' lists row by row, each in its order: an object a full tile did not file is
        // not met
        let on_tiles = (y0..=y1)
            .flat_map(|ty| (x0..=x1).flat_map(move |tx| self.objects_on(Tile::new(tx, ty))));
        for (id, q) in on_tiles {
            let Some(s) = v.seer(id) else { continue };
            if s.kind != MasterKind::COMBAT
                || s.state != ObjectState::Alive
                || KEPT.contains(&q.class)
            {
                continue;
            }
            if (q.player == PlayerId::GAIA && s.capture.any()) || out(q.player.index()) {
                continue;
            }
            let o = q.player;
            let (Some(count), Some(near)) = (counts.get_mut(o.index()), nearest.get_mut(o.index()))
            else {
                return Err(GenieError::unsupported(
                    "an owner search's unit of a player beyond the list",
                ));
            };
            let strong = livestock && attribute_97(o.index()).is_some_and(|a| Amount::ZERO < a);
            if o == p.player {
                if strong {
                    return Ok(None);
                }
                own_seen = true;
                *count += 1;
            } else {
                *count += 1;
                if strong {
                    taker = true;
                    *count += 100;
                }
            }
            let gap = footprint_gap_squared(q.position, q.radius, p.position, p.radius);
            if gap < *near {
                *near = gap;
            }
        }
        if own_seen && !taker {
            return Ok(None);
        }
        let takers: Vec<PlayerId> = PlayerId::each(players)
            .zip(&counts)
            .filter(|&(o, &c)| 0 < c && !out(o.index()))
            .map(|(o, _)| o)
            .collect();
        match takers[..] {
            [] => Ok(None),
            [one] => Ok(Some(one)),
            _ if !taker => {
                let mut best = (DistanceSquared::new(999.0), None);
                for (o, &d) in PlayerId::each(nearest.len()).zip(&nearest) {
                    if d < best.0 && !out(o.index()) {
                        best = (d, Some(o));
                    }
                }
                Ok(best.1.filter(|&o| o != PlayerId::GAIA))
            }
            _ => {
                let mut best = (0, None);
                for (o, &n) in PlayerId::each(counts.len()).zip(&counts) {
                    if best.0 < n && !out(o.index()) {
                        best = (n, Some(o));
                    }
                }
                Ok(best.1)
            }
        }
    }

    /// An ownership change's records ([`Visibility::records_owner`]), which come first.
    pub fn records_owner(&mut self, id: ObjectId, new: PlayerId) -> Result<(), GenieError> {
        self.sight
            .as_mut()
            .map_or(Ok(()), |v| v.records_owner(id, new))
    }

    /// An ownership change's world part (`FUN_14120f800`, once the new owner has the master):
    /// the object's place takes the new owner, and an object that `sees` moves its sight to the
    /// new owner's map with `los` ([`Visibility::owner_changed`]).
    pub fn change_owner(
        &mut self,
        id: ObjectId,
        new: PlayerId,
        los: Distance,
        sees: bool,
    ) -> Result<(), GenieError> {
        let Some(p) = self.objects.get_mut(id) else {
            return Err(GenieError::unsupported(
                "an ownership change of an object the world does not hold",
            ));
        };
        p.player = new;
        let (position, radius) = (p.position, p.radius);
        if let Some(v) = self.sight.as_mut() {
            v.owner_changed(id, position, radius, (new, los.get()), sees)?;
        }
        Ok(())
    }

    /// The vision masks of player `player` (`+0x2a8`, `+0x2a0`; [`crate::vision`]): the world's
    /// rebuild's, else its own bits and the ones no tile has.
    #[must_use]
    pub fn vision(&self, player: PlayerId) -> VisionMasks {
        self.vision
            .get(player)
            .copied()
            .unwrap_or_else(|| own_masks(player, false))
    }

    /// The map revealed to player `player` (`FUN_141639860` -> `FUN_1412847b0` with
    /// `0x4000000040000000`): every word of the unified map takes the ended bits 30 and 62, and
    /// the player's masks (`+0x2a8`, `+0x2a0`) too.
    pub fn reveal_to(&mut self, player: PlayerId) {
        self.reveal_map(VisionBits::ENDED | VisionBits::ENDED_EXPLORED);
        if let Some(m) = self.vision.get_mut(player) {
            m.visible |= VisionBits::ENDED;
            m.explored |= VisionBits::ENDED_EXPLORED;
        }
    }

    /// The unified map's word (`+0x5e30`) at `tile`, none off the map.
    pub(super) fn unified_word(v: &Visibility, tile: Tile) -> Option<VisionBits> {
        Some(*v.unified.get(v.size.index(tile)?)?)
    }

    /// `FUN_141663500`, the player's tile test: what player `player` knows of `tile`. 0xf when it
    /// sees the tile (its visible mask `+0x2a8` meets the unified map's word under
    /// [`crate::vision::VisionBits::OBJECT_MASK`]), or when the word is revealed to all
    /// ([`crate::vision::VisionBits::NO_TILE`]) and the player explored it (its explored mask
    /// `+0x2a0` meets the word under the mask) or the word is explored by all
    /// ([`crate::vision::VisionBits::NO_TILE_EXPLORED`]); else 0x80 when it explored it or the word
    /// is explored by all; else 0, also off the map. None without the players' sight.
    #[must_use]
    pub fn tile_sight(&self, player: PlayerId, tile: Tile) -> Option<TileSight> {
        let Some(word) = Self::unified_word(self.sight.as_ref()?, tile) else {
            return Some(TileSight::UNKNOWN);
        };
        let VisionMasks {
            visible: seen,
            explored,
        } = self.vision(player);
        let explored = explored.intersects(word & VisionBits::OBJECT_MASK);
        let all_explored = word.intersects(VisionBits::NO_TILE_EXPLORED);
        let revealed = word.intersects(VisionBits::NO_TILE);
        if seen.intersects(word & VisionBits::OBJECT_MASK)
            || (revealed && (explored || all_explored))
        {
            return Some(TileSight::SEEN);
        }
        Some(if explored || all_explored {
            TileSight::EXPLORED
        } else {
            TileSight::UNKNOWN
        })
    }

    /// A building's slot 0x1e0 (`FUN_1411d3280`) for `player`, as far as the world holds it:
    /// the player's tile test of its place (`FUN_141193840` -> `FUN_1416635e0`, its coordinates
    /// truncated, [`Spatial::tile_sight`]); short of 0xf, 0xf when the building (type 50 and up)
    /// follows its footprint's UMV entry (`+0x904`) and its visibility mask (`+0x8f8`, the
    /// registry's) meets the player's visible or explored mask (`player+0x2a8`, `+0x2a0`). None
    /// when neither: a ghost of it on its tile that the player is not among the blind of decides
    /// then (0xf), which the world does not hold. Without the players' sight: 0.
    #[must_use]
    pub fn building_sight(
        &self,
        id: ObjectId,
        position: Position,
        player: PlayerId,
    ) -> Option<TileSight> {
        let Some(v) = self.sight.as_ref() else {
            return Some(TileSight::UNKNOWN);
        };
        if self
            .tile_sight(player, position.ground().tile())
            .is_some_and(TileSight::seen)
        {
            return Some(TileSight::SEEN);
        }
        let umv = v.seer(id).is_some_and(|s| s.umv != 0);
        let mask = v
            .registry
            .as_ref()
            .and_then(|r| r.entries.get(id))
            .map_or(VisionBits::NONE, |e| e.mask);
        let VisionMasks { visible, explored } = self.vision(player);
        if umv && (visible.intersects(mask) || explored.intersects(mask)) {
            return Some(TileSight::SEEN);
        }
        None
    }

    /// `FUN_141285350`, the visible map's tile test (`InfluenceMap` and the scout map read it): 0xf
    /// when player `player`'s visible mask (`+0x2a8`) meets the unified map's word at `tile` under
    /// [`crate::vision::VisionBits::OBJECT_MASK`], else 0x80 when its explored mask (`+0x2a0`)
    /// does, else 0, also off the map (the visible map's own size, `+0x20`, `+0x24`, the map's).
    /// The revealed bits 29 and 61 lie outside the mask. None without the players' sight.
    #[must_use]
    pub fn scout_sight(&self, player: PlayerId, tile: Tile) -> Option<TileSight> {
        let Some(word) = Self::unified_word(self.sight.as_ref()?, tile) else {
            return Some(TileSight::UNKNOWN);
        };
        let word = word & VisionBits::OBJECT_MASK;
        let VisionMasks {
            visible: seen,
            explored,
        } = self.vision(player);
        Some(if seen.intersects(word) {
            TileSight::SEEN
        } else if explored.intersects(word) {
            TileSight::EXPLORED
        } else {
            TileSight::UNKNOWN
        })
    }

    /// `FUN_140c68a00`, a unit's tile test (`FUN_141285280` reads the unified map's word, its
    /// player the unit's `+0x138`): 0xf when player `player`'s visible mask (`+0x2a8`) meets the
    /// word at `tile` under [`crate::vision::VisionBits::OBJECT_MASK`], else 0x80 when its explored
    /// mask (`+0x2a0`) does, else 0. The game reads the word unchecked; its three callers
    /// (`FUN_140ab79e0`, `FUN_140c567c0`, `FUN_140c26f60`) pass an object's tile, which the
    /// position setter holds on the map, so off the map is 0 here. None without the players' sight.
    #[must_use]
    pub fn visibility(&self, player: PlayerId, tile: Tile) -> Option<TileSight> {
        let Some(word) = Self::unified_word(self.sight.as_ref()?, tile) else {
            return Some(TileSight::UNKNOWN);
        };
        let word = word & VisionBits::OBJECT_MASK;
        let VisionMasks {
            visible: seen,
            explored,
        } = self.vision(player);
        Some(if seen.intersects(word) {
            TileSight::SEEN
        } else if explored.intersects(word) {
            TileSight::EXPLORED
        } else {
            TileSight::UNKNOWN
        })
    }

    /// `FUN_1412847b0`: every word of the unified map (`+0x5e30`, width × height) takes the bits
    /// of `mask` (the end of the game `0x4000000040000000`, a revealed map `0x2000000020000000`,
    /// both `0x6000000060000000`). Nothing without the players' sight.
    pub fn reveal_map(&mut self, mask: VisionBits) {
        if let Some(v) = self.sight.as_mut() {
            for w in &mut v.unified {
                *w |= mask;
            }
        }
    }

    /// Whether player `player` sees `tile`: [`Spatial::tile_sight`] is 0xf. False without the
    /// players' sight.
    #[must_use]
    pub fn sees_tile(&self, player: PlayerId, tile: Tile) -> bool {
        self.tile_sight(player, tile).is_some_and(TileSight::seen)
    }

    /// [`Spatial::tile_sight`] is not 0: player `player` sees `tile` or explored it. Unknown
    /// without the players' sight.
    pub fn explored(&self, player: PlayerId, tile: Tile) -> Result<bool, GenieError> {
        let answer = self
            .tile_sight(player, tile)
            .ok_or(GenieError::unsupported(
                "a test of explored tiles without the players' sight",
            ))?;
        Ok(answer != TileSight::UNKNOWN)
    }

    /// [`Spatial::visibility`] is 0xf: player `player` sees `tile` now. False without the
    /// players' sight.
    #[must_use]
    pub fn visible(&self, player: PlayerId, tile: Tile) -> bool {
        self.visibility(player, tile).is_some_and(TileSight::seen)
    }

    /// `RGE_Combat_Object::update`'s sight ([`Visibility::combat_sight`]); an annex reads its
    /// main building's place and UMV entry. The changed mask of an object others take over by
    /// seeing it is the answer.
    pub fn combat_sight(
        &mut self,
        id: ObjectId,
        annex_of: Option<MainBuildingId>,
    ) -> Result<Option<VisionBits>, GenieError> {
        let (Some(v), Some(p)) = (self.sight.as_mut(), self.objects.get(id)) else {
            return Ok(None);
        };
        let main = annex_of.as_ref().and_then(|main| {
            let place = self.objects.get(main.id())?.position;
            Some((place, v.seer(*main).map_or(0, |s| s.umv)))
        });
        v.combat_sight(id, p.player, p.position, p.radius, p.class, main)
    }

    /// The object's `set_state` to `state` ([`Visibility::state_changed`]).
    pub fn sight_state(&mut self, id: ObjectId, state: ObjectState) -> Result<(), GenieError> {
        if let (Some(v), Some(p)) = (self.sight.as_mut(), self.objects.get(id)) {
            v.state_changed(id, p.position, p.radius, state)?;
        }
        Ok(())
    }

    /// The object's master changes to `master` with radii `radius`, class `class` and line of
    /// sight `los` (`FUN_14108c990`: its records leave the registry's lists of its old class,
    /// [`Visibility::master_changed`], and join those of the new one); the caller files the
    /// object's new radii and class.
    pub fn sight_master(
        &mut self,
        id: ObjectId,
        radius: Footprint,
        class: ClassId,
        master: MasterId,
        los: Distance,
    ) -> Result<(), GenieError> {
        if let (Some(v), Some(p)) = (self.sight.as_mut(), self.objects.get(id)) {
            v.master_changed(id, p.position, p.radius, radius, master, los)?;
            v.registry_master(
                id,
                p.player,
                p.position,
                (p.radius, p.class),
                (radius, class),
            )?;
        }
        Ok(())
    }

    /// The first part of the players' sight the slice met and does not model.
    pub fn take_sight_unsupported(&mut self) -> Option<GenieError> {
        self.sight.as_mut().and_then(Visibility::take_unsupported)
    }

    /// A player's ghost `id` of an object it saw (a doppelganger, type 25): it is on its tile's
    /// list, and movement sees nothing else of it. The slice keeps the ghosts of the save and
    /// makes none. The tile's tall count is redone ([`Tiles`]' append, `FUN_140f5f0a0`); the
    /// game counts the ghost by its master's height radius, which the slice does not hold, so
    /// the ghost counts as not tall.
    ///
    /// [`Tiles`]: crate::map::spatial::Tiles
    pub fn insert_ghost(&mut self, id: ObjectId, position: Position) {
        if let Some(i) = self.tiles.tile_of(position) {
            let objects = &self.objects;
            self.tiles.append(i, id, |o| Self::is_tall(objects, o));
        }
    }

    /// A ghost leaves its tile's list (its last object takes its place) and the tile's tall count
    /// is redone (`FUN_140f5f560`).
    pub fn remove_ghost(&mut self, id: ObjectId, position: Position) {
        if let Some(i) = self.tiles.tile_of(position) {
            let objects = &self.objects;
            self.tiles.take_off(i, id, |o| Self::is_tall(objects, o));
        }
    }

    /// The tiles' lists in the game's order (the tile, its ids), each a tile of two objects or more
    /// (a single object's tile follows from the positions); the order is the game's history,
    /// which no save holds. Returns the tiles whose objects the slice does not hold as the game
    /// does (the same ids, in any order), with the slice's list; those keep the slice's.
    pub fn force_tile_lists(
        &mut self,
        lists: &[(Tile, Vec<ObjectId>)],
    ) -> Vec<(Tile, Vec<ObjectId>)> {
        let mut differ = Vec::new();
        for (tile, ids) in lists {
            let Some(list) = self
                .tiles
                .size
                .index(*tile)
                .and_then(|i| self.tiles.lists.get_mut(i))
            else {
                differ.push((*tile, Vec::new()));
                continue;
            };
            let mut mine = list.clone();
            let mut theirs = ids.clone();
            mine.sort_unstable();
            theirs.sort_unstable();
            if mine == theirs {
                list.clone_from(ids);
            } else {
                differ.push((*tile, list.clone()));
            }
        }
        differ
    }

    /// `FUN_141185690`: tile (`x`, `y`) lies on the obstruction manager's map, its terrain class
    /// is in `mask` and no static record blocks it.
    #[must_use]
    pub fn standable(&self, tile: Tile, mask: TerrainMask) -> bool {
        self.tiles
            .word(tile)
            .is_some_and(|w| w & (mask.blocked() | BLOCKED) == 0)
    }

    /// Whether unit `me` (its x radius `radius_x`) with terrain mask `mask` can stand on tile
    /// (`x`, `y`) (`FUN_14114d460`): the tile is on the map, its class is in the mask and no
    /// static record blocks it; on a tile with two tall objects or more, the obstruction
    /// manager's point query at the tile's corner (`FUN_141051900` with flags 0x20, the records
    /// grown by half the x radius, `FUN_1410562d0`, the unit itself ignored, `FUN_1410562e0`)
    /// finds none ([`Spatial::point_clear`]).
    ///
    /// # Panics
    ///
    /// Never: the tall-object grid covers the map as the word grid does.
    pub fn passable(
        &mut self,
        tile: Tile,
        mask: TerrainMask,
        me: ObjectId,
        radius_x: Distance,
        by_id: ObjectsRef<'_>,
    ) -> Result<bool, GenieError> {
        let (Some(word), Some(tall)) = (self.tiles.word(tile), self.tiles.tall(tile)) else {
            return Ok(false);
        };
        if word & (mask.blocked() | BLOCKED) != 0 {
            return Ok(false);
        }
        if 1 < tall {
            return self.point_clear(me, tile.corner(), radius_x * 0.5, mask, by_id);
        }
        Ok(true)
    }

    /// The tile beside `site`'s footprint a unit at `from` with terrain mask `mask` goes to
    /// (`FUN_1411566b0`, closest): [`Spatial::spot_off`] with the site's place and radii.
    ///
    /// # Errors
    ///
    /// As [`Spatial::spot_off`].
    pub fn spot_beside(
        &mut self,
        from: Position,
        me: (ObjectId, f32),
        mask: TerrainMask,
        site: (Position, Footprint),
        by_id: ObjectsRef<'_>,
    ) -> Result<Option<Point>, GenieError> {
        self.spot_off(from, me, mask, site, false, by_id)
    }
}
