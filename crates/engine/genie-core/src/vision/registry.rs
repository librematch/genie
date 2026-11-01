//! The objects-in-sight registry (`Visible_Unit_Manager`, `DAT_143f69170`): per player and per
//! master class, a record of each combat object the player sees, kept by the objects' own updates
//! from the unified map's visible bits, and the query the unit AIs search it with
//! (`GetVisibleUnits`, `FUN_141288c00`).
//!
//! The save holds neither the lists nor the objects' masks: the slice starts from the game's own
//! (`load_oracle/probes/registry.py`). A list's order is its history's: records join at its end and
//! leave by a swap with its last.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::players::ByRelation;
use crate::players::PlayerId;
#[cfg(test)]
use crate::players::UnitRelation;
use crate::vision::VisionBits;
use crate::world::slots::CheckedIndex;

/// The players a mask has room for in `Update_Unit_Info`'s loop: the object's record pointers
/// (`+0x8b0`) are nine.
pub const SLOTS: usize = 9;

/// The query's cap on the radius (`DAT_142f64e88`).
const MOST_RADIUS: f32 = crate::math::image::registry::MOST_RADIUS;

/// The query box's margin (`DAT_142ec9eb8`).
const MARGIN: f32 = crate::math::image::registry::MARGIN;

/// The records a query keeps per relation (`0x1c00` bytes per output list).
pub const MOST_FOUND: usize = 256;

/// The class lists per player: the map's setup (`FUN_140f6ce50`) makes the registry with 0x40 + 1
/// (`FUN_141287d20`), empty; a load or a new game starts there, and the objects' updates fill it.
pub const CLASSES: usize = 65;

/// One record (0x1c bytes): the object, a distance (`FLT_MAX` in the lists; a query's copy holds
/// its own), the owner's id (`player+0x718`), the place and the master's radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    /// The object.
    pub id: ObjectId,
    /// Its distance (`FLT_MAX` in the lists; a query's copy holds its own).
    pub distance: Distance,
    /// The owner's id (`player+0x718`).
    pub owner: PlayerId,
    /// Its place (x, y).
    pub at: Point,
    /// Its master's radii on x and y.
    pub radius: Footprint,
}

/// An object's part in the registry: its mask (`+0x8f8`) and, per player, its record's index in
/// that player's list of its class (`+0x8b0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// Its mask (`+0x8f8`): the players whose lists hold it.
    pub mask: VisionBits,
    /// Per player, its record's index in that player's list of its class (`+0x8b0`).
    pub slots: [Option<usize>; SLOTS],
}

/// An object's change of the players that see it ([`Registry::update_unit`]'s arguments,
/// `FUN_14128a1d0`'s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitUpdate {
    /// The object.
    pub id: ObjectId,
    /// The owner's id (`player+0x718`).
    pub owner: PlayerId,
    /// Its place (x, y).
    pub position: Point,
    /// Its master's radii (x, y).
    pub radii: Footprint,
    /// Its master's class.
    pub class: ClassId,
    /// The mask of the players that saw it.
    pub old: VisionBits,
    /// The mask of the players that see it.
    pub new: VisionBits,
}

/// A query for the visible units of a player ([`Registry::visible_units`]'s arguments,
/// `FUN_141288c00`'s).
#[derive(Clone, Copy, Debug)]
pub struct VisibleQuery<'a> {
    /// The player whose lists it reads.
    pub player: PlayerId,
    /// The searcher's place (x, y).
    pub position: Point,
    /// How far, at most 44.
    pub radius: Distance,
    /// 0: a rounded box, 1: a box.
    pub mode: i32,
    /// The classes it reads.
    pub classes: &'a [ClassId],
    /// What the owners are to the player (none for a saved value past the relations).
    pub relations: &'a crate::players::UnitRelations,
    /// The searcher's radii (x, y).
    pub size: Footprint,
    /// The relations kept, one bit each.
    pub wanted: u8,
}

/// The registry: `lists[player][class]`, and each object's [`Entry`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Registry {
    /// The players it holds lists for.
    pub players: usize,
    /// The classes per player ([`CLASSES`]).
    pub classes: usize,
    /// `lists[player][class]`: the records.
    pub lists: Vec<Vec<Vec<Record>>>,
    /// Each object's entry.
    pub entries: crate::objects::IdTable<Entry>,
}

impl Registry {
    /// An empty registry of `players` and `classes`.
    #[must_use]
    pub fn new(players: usize, classes: usize) -> Self {
        Self {
            players,
            classes,
            lists: vec![vec![Vec::new(); classes]; players],
            entries: crate::objects::IdTable::new(),
        }
    }

    /// The object `id`'s entry, if it has one.
    #[must_use]
    pub fn entry(&self, id: ObjectId) -> Option<&Entry> {
        self.entries.get(id)
    }

    /// `Update_Unit_Info` (`FUN_14128a1d0`): for each player whose bit (of the low 16) differs
    /// between `old` and `new`, the object joins the end of that player's list of its class, or
    /// leaves it (the list's last record takes its place).
    pub fn update_unit(&mut self, unit: UnitUpdate) -> Result<(), GenieError> {
        let UnitUpdate {
            id,
            owner,
            position,
            radii,
            class,
            old,
            new,
        } = unit;
        let mut changed = (old.bits() ^ new.bits()) & 0xffff;
        let mut seen = new.bits();
        let Some(class) = class.index() else {
            return Err(GenieError::unsupported(
                "a registry update for a master without a class",
            ));
        };
        if self.classes <= class {
            return Ok(());
        }
        for p in 0..self.players {
            if changed == 0 {
                return Ok(());
            }
            if changed & 1 != 0 {
                let slot = self
                    .entries
                    .get_or_insert_with(id, Entry::default)?
                    .slots
                    .get(p)
                    .copied()
                    .flatten();
                let list = self.lists.checked_mut(p)?.checked_mut(class)?;
                if seen & 1 == 0 {
                    if !list.is_empty() {
                        let i = slot.ok_or(GenieError::unsupported(
                            "a registry removal of an object without its record",
                        ))?;
                        let last = list.len() - 1;
                        if i != last {
                            list.swap(i, last);
                            let moved = list.checked(i)?.id;
                            if let Some(s) =
                                self.entries.get_mut(moved).and_then(|e| e.slots.get_mut(p))
                            {
                                *s = Some(i);
                            }
                        }
                        let _ = self.lists.checked_mut(p)?.checked_mut(class)?.pop();
                    }
                    if let Some(s) = self.entries.get_mut(id).and_then(|e| e.slots.get_mut(p)) {
                        *s = None;
                    }
                } else {
                    list.push(Record {
                        id,
                        distance: Distance::MAX,
                        owner,
                        at: position,
                        radius: radii,
                    });
                    let i = list.len() - 1;
                    if let Some(s) = self.entries.get_mut(id).and_then(|e| e.slots.get_mut(p)) {
                        *s = Some(i);
                    }
                }
            }
            changed >>= 1;
            seen >>= 1;
        }
        Ok(())
    }

    /// An ownership change (`FUN_14120f800`): the object's records take the new owner's id.
    pub fn owner_changed(&mut self, id: ObjectId, owner: PlayerId) -> Result<(), GenieError> {
        let Some(e) = self.entries.get(id).copied() else {
            return Ok(());
        };
        for (p, slot) in e.slots.iter().enumerate().take(self.players) {
            let Some(i) = *slot else { continue };
            let record = self
                .lists
                .checked_mut(p)?
                .iter_mut()
                .filter_map(|list| list.get_mut(i))
                .find(|r| r.id == id)
                .ok_or(GenieError::unsupported(
                    "an object's registry slot without its record",
                ))?;
            record.owner = owner;
        }
        Ok(())
    }

    /// The combat object's new place (`FUN_14108c320`): its records in every list follow.
    pub fn moved(&mut self, id: ObjectId, class: ClassId, at: Point) {
        let (x, y) = (at.x(), at.y());
        let Some(e) = self.entries.get(id).copied() else {
            return;
        };
        let Some(class) = class.index() else {
            return;
        };
        for (p, slot) in e.slots.iter().enumerate().take(self.players) {
            if let Some(r) = slot.and_then(|i| self.lists.get_mut(p)?.get_mut(class)?.get_mut(i)) {
                r.at = Point::new(x, y);
            }
        }
    }

    /// `GetVisibleUnits` (`FUN_141288c00`) for `player`: the records of its lists of `classes`
    /// whose object is within `radius` (at most 44) of (`x`, `y`) for a searcher of radii `rx`,
    /// `ry` (`mode` 0: a rounded box, 1: a box), and whose owner's relation (`relations[owner]`:
    /// 0 Gaia, 1 self, 2 friend, 3 neutral, 4 enemy) is in `wanted`; by relation, each with its
    /// edge distance, at most 256 each. None when the query reads nothing (no classes, a player
    /// out of range), which leaves the game's last answer in place.
    pub fn visible_units(
        &self,
        query: VisibleQuery<'_>,
    ) -> Result<Option<ByRelation<Vec<Record>>>, GenieError> {
        let VisibleQuery {
            player,
            position,
            radius,
            mode,
            classes,
            relations,
            size,
            wanted,
        } = query;
        let (x, y) = (position.x(), position.y());
        let (rx, ry) = (size.x(), size.y());
        if classes.is_empty() || self.players <= player.index() {
            return Ok(None);
        }
        let radius = radius.get();
        let radius = if radius <= MOST_RADIUS {
            radius
        } else {
            MOST_RADIUS
        };
        let wide = radius + rx;
        let tall = radius + ry;
        let (x0, x1) = ((x - wide) - MARGIN, x + wide + MARGIN);
        let (y0, y1) = ((y - tall) - MARGIN, y + tall + MARGIN);
        let round = f32::midpoint(wide - rx, tall - ry);
        let mut found: ByRelation<Vec<Record>> = ByRelation::default();
        for &class in classes {
            let list = class
                .index()
                .and_then(|c| self.lists.get(player.index())?.get(c))
                .ok_or(GenieError::unsupported(
                    "a query of a class beyond the registry's",
                ))?;
            for r in list {
                let relation = *relations.get(r.owner).ok_or(GenieError::unsupported(
                    "a record of an owner without a relation",
                ))?;
                let Some(relation) = relation.filter(|r| wanted & r.bit() != 0) else {
                    continue;
                };
                let (px, py) = (r.at.x(), r.at.y());
                if !(px <= x1 && x0 <= px && py <= y1 && y0 <= py) {
                    continue;
                }
                let dx = (px - x).abs() - r.radius.x();
                let dy = (py - y).abs() - r.radius.y();
                let inside = match mode {
                    0 => {
                        if rx < dx {
                            if ry < dy {
                                (dy - ry) * (dy - ry) + (dx - rx) * (dx - rx) <= round * round
                            } else {
                                dx < wide
                            }
                        } else {
                            dy < tall
                        }
                    }
                    1 => dx <= wide && dy <= tall,
                    _ => false,
                };
                if !inside {
                    continue;
                }
                let out = &mut found[relation];
                if out.len() < MOST_FOUND {
                    out.push(Record {
                        distance: Distance::new((dy * dy + dx * dx).sqrt()),
                        ..*r
                    });
                }
            }
        }
        Ok(Some(found))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::masters::class;
    use crate::test_ids::{class_id, object_id};

    fn registry() -> Registry {
        Registry::new(3, 65)
    }

    #[test]
    fn an_object_joins_a_list_when_seen_and_leaves_by_a_swap_with_the_last() {
        let mut r = registry();
        for id in [1, 2, 3] {
            r.update_unit(UnitUpdate {
                id: object_id(id),
                owner: PlayerId::new(1),
                position: Point::new(id as f32, 5.0),
                radii: Footprint::new(0.2, 0.2),
                class: class::VILLAGER,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(0b101),
            })
            .unwrap();
        }
        assert_eq!(
            r.lists[0][4].iter().map(|x| x.id).collect::<Vec<_>>(),
            [object_id(1), object_id(2), object_id(3)]
        );
        assert_eq!(r.lists[2][4].len(), 3);
        assert_eq!(r.lists[1][4], Vec::<Record>::new());
        // 1 leaves player 0's sight: 3 takes its place
        r.update_unit(UnitUpdate {
            id: object_id(1),
            owner: PlayerId::new(1),
            position: Point::new(1.0, 5.0),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::from_bits_retain(0b101),
            new: VisionBits::from_bits_retain(0b100),
        })
        .unwrap();
        assert_eq!(
            r.lists[0][4].iter().map(|x| x.id).collect::<Vec<_>>(),
            [object_id(3), object_id(2)]
        );
        assert_eq!(r.entry(object_id(3)).unwrap().slots[0], Some(0));
        assert_eq!(r.entry(object_id(1)).unwrap().slots[0], None);
        assert_eq!(r.entry(object_id(1)).unwrap().slots[2], Some(0));
        r.moved(object_id(3), class::VILLAGER, Point::new(7.0, 7.0));
        assert_eq!(
            (r.lists[0][4][0].at.x(), r.lists[2][4][2].at.y()),
            (7.0, 7.0)
        );
    }

    #[test]
    fn a_query_finds_the_records_within_its_rounded_box_by_relation() {
        let mut r = registry();
        r.update_unit(UnitUpdate {
            id: object_id(1),
            owner: PlayerId::new(1),
            position: Point::new(12.0, 10.0),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(1),
        })
        .unwrap();
        r.update_unit(UnitUpdate {
            id: object_id(2),
            owner: PlayerId::new(2),
            position: Point::new(10.0, 13.5),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(1),
        })
        .unwrap();
        r.update_unit(UnitUpdate {
            id: object_id(3),
            owner: PlayerId::new(2),
            position: Point::new(13.0, 13.0),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(1),
        })
        .unwrap();
        r.update_unit(UnitUpdate {
            id: object_id(4),
            owner: PlayerId::new(0),
            position: Point::new(10.5, 10.5),
            radii: Footprint::new(0.2, 0.2),
            class: class::PREY,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(1),
        })
        .unwrap();
        let relations = crate::players::relations([1, 4, 4]);
        let found = r
            .visible_units(VisibleQuery {
                player: PlayerId::new(0),
                position: Point::new(10.0, 10.0),
                radius: Distance::new(3.0),
                mode: 0,
                classes: &[class_id(4), class_id(9)],
                relations: &relations,
                size: Footprint::new(0.2, 0.2),
                wanted: 0xff,
            })
            .unwrap()
            .unwrap();
        // 1 is 1.8 off along x, 2 is 3.3 off along y (beyond 3.2), 3 is at the corner and beyond
        // the rounding
        assert_eq!(
            found[UnitRelation::ENEMY]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(1)]
        );
        assert_eq!(
            found[UnitRelation::OWN]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(4)]
        );
        // the edge distances, the overlap along y included: (1.8, -0.2)
        assert!(
            (found[UnitRelation::ENEMY][0].distance - Distance::new((1.8f32 * 1.8 + 0.04).sqrt()))
                .get()
                .abs()
                < 1e-6
        );
        assert_eq!(
            r.visible_units(VisibleQuery {
                player: PlayerId::new(0),
                position: Point::new(10.0, 10.0),
                radius: Distance::new(3.0),
                mode: 0,
                classes: &[],
                relations: &relations,
                size: Footprint::new(0.2, 0.2),
                wanted: 0xff
            })
            .unwrap(),
            None
        );
    }

    #[test]
    fn a_query_caps_its_radius_at_44() {
        let mut r = registry();
        r.update_unit(UnitUpdate {
            id: object_id(1),
            owner: PlayerId::new(1),
            position: Point::new(53.5, 10.0),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(1),
        })
        .unwrap();
        r.update_unit(UnitUpdate {
            id: object_id(2),
            owner: PlayerId::new(1),
            position: Point::new(60.0, 10.0),
            radii: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(1),
        })
        .unwrap();
        let found = r
            .visible_units(VisibleQuery {
                player: PlayerId::new(0),
                position: Point::new(10.0, 10.0),
                radius: Distance::new(100.0),
                mode: 1,
                classes: &[class_id(4)],
                relations: &crate::players::ByPlayer::new(vec![
                    Some(UnitRelation::OWN),
                    Some(UnitRelation::ENEMY),
                    Some(UnitRelation::ENEMY),
                ]),
                size: Footprint::new(0.2, 0.2),
                wanted: 0xff,
            })
            .unwrap()
            .unwrap();
        // the box reaches 44.2 along x: 1's edge is 43.3 off, 2's 49.8
        assert_eq!(
            found[UnitRelation::ENEMY]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(1)]
        );
    }

    #[test]
    fn a_query_keeps_a_wide_object_whose_centre_lies_beyond_the_radius() {
        let mut r = registry();
        r.update_unit(UnitUpdate {
            id: object_id(1),
            owner: PlayerId::new(1),
            position: Point::new(15.0, 10.0),
            radii: Footprint::new(2.0, 2.0),
            class: class::VILLAGER,
            old: VisionBits::NONE,
            new: VisionBits::from_bits_retain(1),
        })
        .unwrap();
        let found = r
            .visible_units(VisibleQuery {
                player: PlayerId::new(0),
                position: Point::new(10.0, 10.0),
                radius: Distance::new(3.0),
                mode: 0,
                classes: &[class_id(4)],
                relations: &crate::players::ByPlayer::new(vec![
                    Some(UnitRelation::OWN),
                    Some(UnitRelation::ENEMY),
                    Some(UnitRelation::ENEMY),
                ]),
                size: Footprint::new(0.2, 0.2),
                wanted: 0xff,
            })
            .unwrap()
            .unwrap();
        // the centre is 5 off, beyond 3.2, but the edge is 3 off: the 7-tile margin keeps it in
        // the box
        assert_eq!(
            found[UnitRelation::ENEMY]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(1)]
        );
    }
}
