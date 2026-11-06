//! The objects-in-sight registry (`Visible_Unit_Manager`, `DAT_143f69170`): per player and per
//! master class, a record of each combat object the player sees, kept by the objects' own updates
//! from the unified map's visible bits, and the query the unit AIs search it with
//! (`GetVisibleUnits`, `FUN_141288c00`).
//!
//! The save holds neither the lists nor the objects' masks: the slice starts from the game's own
//! (`load_oracle/probes/registry.py`). A list's order is its history's: records join at its end and
//! leave by a swap with its last.

use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::math::fmath::below_or_unordered;
use crate::math::fmath::minss;
use crate::math::fmath::sqrt;
use crate::math::image::registry::HALF;
use crate::objects::IdTable;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::players::ByRelation;
use crate::players::PlayerId;
use crate::players::RelationMask;
#[cfg(test)]
use crate::players::UnitRelation;
use crate::players::UnitRelations;
use crate::vision::VisionBits;
use crate::world::layer::generated::RegistryTuning;
use crate::world::slots::CheckedIndexExt;
use crate::{GenieError, GenieResult};

/// The players a mask has room for in `Update_Unit_Info`'s loop: the object's record pointers
/// (`+0x8b0`) are nine.
pub const SLOTS: usize = 9;

/// The records a query keeps per relation (`0x1c00` bytes per output list).
pub const MOST_FOUND: usize = 256;

/// The class lists per player: the map's setup (`FUN_140f6ce50`) makes the registry with 0x40 + 1
/// (`FUN_141287d20`), empty; a load or a new game starts there, and the objects' updates fill it.
pub const CLASSES: usize = 65;

/// A player below the registry's count without its lists: [`Registry::new`] makes one per player.
const NO_PLAYER_LISTS: GenieError =
    GenieError::unreachable("a player of the registry without its lists");

/// A visible-units query of a class past the player's lists: the game reads past them.
const NO_CLASS_LIST: GenieError =
    GenieError::invalid_input("a visible-units query of a class past the registry's lists");

/// A record of an owner past the relations the query reads: the game reads past them.
const NO_RELATION: GenieError =
    GenieError::invalid_input("a visible-units record of an owner past the relations");

/// A record that the query keeps by its owner's saved value, whose low byte names none of the
/// five lists: the game writes past them.
const NO_LIST: GenieError =
    GenieError::invalid_input("a visible-units record whose relation lists past the five");

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
    pub(crate) id: ObjectId,
    /// The owner's id (`player+0x718`).
    pub(crate) owner: PlayerId,
    /// Its place (x, y).
    pub(crate) position: Point,
    /// Its master's radii (x, y).
    pub(crate) radii: Footprint,
    /// Its master's class.
    pub(crate) class: ClassId,
    /// The mask of the players that saw it.
    pub(crate) old: VisionBits,
    /// The mask of the players that see it.
    pub(crate) new: VisionBits,
}

/// A query for the visible units of a player ([`Registry::visible_units`]'s arguments,
/// `FUN_141288c00`'s).
#[derive(Clone, Copy, Debug)]
pub struct VisibleQuery<'world> {
    /// The player whose lists it reads.
    pub(crate) player: PlayerId,
    /// The searcher's place (x, y).
    pub(crate) position: Point,
    /// How far, at most 44.
    pub(crate) radius: Distance,
    /// Its area's shape.
    pub(crate) shape: QueryShape,
    /// The classes it reads.
    pub(crate) classes: &'world [ClassId],
    /// What the owners are to the player: each owner's saved `unit_diplomacy` dword
    /// (`player+0x278`), as the save holds it.
    pub(crate) relations: &'world UnitRelations,
    /// The searcher's radii (x, y).
    pub(crate) size: Footprint,
    /// The relations kept.
    pub(crate) wanted: RelationMask,
}

/// The shape of a visible-units query's area (`FUN_141288c00`'s mode): a rectangle with rounded
/// corners (0) or a rectangle (1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QueryShape {
    /// 0: within the radius of the searcher's rectangle, its corners rounded.
    RoundedRectangle,
    /// 1: within the radius of the searcher's rectangle.
    Rectangle,
}

impl QueryShape {
    /// The mode, as the game passes and an aura's key hashes it.
    #[must_use]
    pub const fn get(self) -> i32 {
        match self {
            Self::RoundedRectangle => 0,
            Self::Rectangle => 1,
        }
    }
}

/// A visible-units query's area, as `FUN_141288c00` makes it before its loop: the capped radius
/// grown by the searcher's radii, the box that bounds the records' places, and its shape's tests
/// of a record's edge gaps.
#[derive(Clone, Copy, Debug)]
struct QueryArea {
    /// Its shape.
    shape: QueryShape,
    /// The searcher's radii (x, y).
    size: Footprint,
    /// The capped radius plus the searcher's radius on x.
    wide: f32,
    /// The capped radius plus the searcher's radius on y.
    tall: f32,
    /// The box's least corner: the place less `wide` and `tall`, less the margin.
    least: Point,
    /// The box's most corner: the place plus `wide` and `tall`, plus the margin.
    most: Point,
    /// The corners' rounding, squared.
    round_squared: f32,
}

impl QueryArea {
    /// The area of `query`: its radius capped at the tuning's most (`MINSS` of the most and the
    /// radius, so a NaN radius stays NaN), the box grown by the tuning's margin, and the
    /// corners' rounding, the mean of the radius on both axes:
    /// `((wide - rx) + (tall - ry)) * 0.5`.
    fn new(query: &VisibleQuery<'_>, registry_tuning: RegistryTuning) -> Self {
        let (x, y) = (query.position.x(), query.position.y());
        let (rx, ry) = (query.size.x(), query.size.y());
        let radius = minss(registry_tuning.most_radius, query.radius.get());
        let wide = radius + rx;
        let tall = radius + ry;
        let margin = registry_tuning.margin;
        let round = ((wide - rx) + (tall - ry)) * HALF;
        Self {
            shape: query.shape,
            size: query.size,
            wide,
            tall,
            least: Point::new((x - wide) - margin, (y - tall) - margin),
            most: Point::new((x + wide) + margin, (y + tall) + margin),
            round_squared: round * round,
        }
    }

    /// Whether its box bounds a record at `at` (`COMISS` with each bound, `JC` past it: a NaN
    /// place lies outside).
    fn bounds(&self, at: Point) -> bool {
        let (px, py) = (at.x(), at.y());
        px <= self.most.x() && self.least.x() <= px && py <= self.most.y() && self.least.y() <= py
    }

    /// Whether it holds a record whose edges lie `dx` and `dy` off the place.
    ///
    /// The rounded box (mode 0) tests the gap on y within `tall` where the gap on x is within the
    /// searcher's radius, else the gap on x within `wide` where the gap on y is, else the corner:
    /// the gaps past the searcher's radii within the rounding. Each of its first two compares
    /// takes the unordered case to the corner's side (`COMISS`, `JC` at `0x141288e5a` and
    /// `0x141288e68`), and each last test fails on a NaN (`SETA`, `SETNC`): no record of a NaN
    /// gap lies within it. The box (mode 1) tests both gaps within `wide` and `tall`.
    fn holds(&self, dx: f32, dy: f32) -> bool {
        let (rx, ry) = (self.size.x(), self.size.y());
        match self.shape {
            QueryShape::RoundedRectangle => {
                if below_or_unordered(rx, dx) {
                    if below_or_unordered(ry, dy) {
                        (dy - ry) * (dy - ry) + (dx - rx) * (dx - rx) <= self.round_squared
                    } else {
                        dx < self.wide
                    }
                } else {
                    dy < self.tall
                }
            }
            QueryShape::Rectangle => dx <= self.wide && dy <= self.tall,
        }
    }
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
    pub entries: IdTable<Entry>,
}

impl Registry {
    /// An empty registry of `players` and `classes`.
    #[must_use]
    pub fn new(players: usize, classes: usize) -> Self {
        Self {
            players,
            classes,
            lists: vec![vec![Vec::new(); classes]; players],
            entries: IdTable::new(),
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
    pub fn update_unit(&mut self, unit: UnitUpdate) -> GenieResult<()> {
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
        for index in 0..self.players {
            if changed == 0 {
                return Ok(());
            }
            if changed & 1 != 0 {
                let slot = self
                    .entries
                    .get_or_insert_with(id, Entry::default)?
                    .slots
                    .get(index)
                    .copied()
                    .flatten();
                let list = self.lists.checked_mut(index)?.checked_mut(class)?;
                if seen & 1 == 0 {
                    if !list.is_empty() {
                        let slot_index = slot.ok_or(GenieError::unsupported(
                            "a registry removal of an object without its record",
                        ))?;
                        let last = list.len() - 1;
                        if slot_index != last {
                            list.swap(slot_index, last);
                            let moved = list.checked(slot_index)?.id;
                            if let Some(entry) = self
                                .entries
                                .get_mut(moved)
                                .and_then(|moved_entry| moved_entry.slots.get_mut(index))
                            {
                                *entry = Some(slot_index);
                            }
                        }
                        let _ = self.lists.checked_mut(index)?.checked_mut(class)?.pop();
                    }
                    if let Some(entry) = self
                        .entries
                        .get_mut(id)
                        .and_then(|entry| entry.slots.get_mut(index))
                    {
                        *entry = None;
                    }
                } else {
                    list.push(Record {
                        id,
                        distance: Distance::MAX,
                        owner,
                        at: position,
                        radius: radii,
                    });
                    let count = list.len() - 1;
                    if let Some(entry) = self
                        .entries
                        .get_mut(id)
                        .and_then(|entry| entry.slots.get_mut(index))
                    {
                        *entry = Some(count);
                    }
                }
            }
            changed >>= 1;
            seen >>= 1;
        }
        Ok(())
    }

    /// An ownership change (`FUN_14120f800`): the object's records take the new owner's id.
    pub fn owner_changed(&mut self, id: ObjectId, owner: PlayerId) -> GenieResult<()> {
        let Some(entry) = self.entries.get(id).copied() else {
            return Ok(());
        };
        for (index, slot) in entry.slots.iter().enumerate().take(self.players) {
            let Some(position) = *slot else { continue };
            let record = self
                .lists
                .checked_mut(index)?
                .iter_mut()
                .filter_map(|list| list.get_mut(position))
                .find(|record| record.id == id)
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
        let Some(entry) = self.entries.get(id).copied() else {
            return;
        };
        let Some(class) = class.index() else {
            return;
        };
        for (index, slot) in entry.slots.iter().enumerate().take(self.players) {
            if let Some(record) = slot
                .and_then(|position| self.lists.get_mut(index)?.get_mut(class)?.get_mut(position))
            {
                record.at = Point::new(x, y);
            }
        }
    }

    /// `GetVisibleUnits` (`FUN_141288c00`) for `player`: the records of its lists of `classes`
    /// whose object is within `radius` (at most 44) of (`x`, `y`) for a searcher of radii `rx`,
    /// `ry` (its [`QueryShape`]), and whose owner's saved relation (`relations[owner]`) has its
    /// bit in `wanted` ([`RelationMask::keeps`]); by the relation of its low byte
    /// ([`UnitDiplomacy::list`]: 0 Gaia, 1 self, 2 friend, 3 neutral, 4 enemy), each with its
    /// edge distance, at most 256 each. None when the query reads nothing (no classes, a player
    /// out of range), which leaves the game's last answer in place.
    ///
    /// # Errors
    ///
    /// The game's reads and writes past its arrays: a class past the player's lists, a record
    /// of an owner past `relations`, and a record that the query keeps whose relation lists past
    /// the five.
    ///
    /// [`UnitDiplomacy::list`]: crate::players::UnitDiplomacy::list
    pub fn visible_units(
        &self,
        query: VisibleQuery<'_>,
        registry_tuning: RegistryTuning,
    ) -> GenieResult<Option<ByRelation<Vec<Record>>>> {
        let VisibleQuery {
            player,
            position,
            classes,
            relations,
            wanted,
            ..
        } = query;
        let (x, y) = (position.x(), position.y());
        if classes.is_empty() || self.players <= player.index() {
            return Ok(None);
        }
        let lists = self.lists.get(player.index()).ok_or(NO_PLAYER_LISTS)?;
        let area = QueryArea::new(&query, registry_tuning);
        let mut found: ByRelation<Vec<Record>> = ByRelation::default();
        for &class in classes {
            let list = class
                .index()
                .and_then(|class_index| lists.get(class_index))
                .ok_or_else(|| NO_CLASS_LIST.with("class", class))?;
            for record in list {
                let diplomacy = *relations
                    .get(record.owner)
                    .ok_or_else(|| NO_RELATION.with("owner", record.owner))?;
                if !wanted.keeps(diplomacy) {
                    continue;
                }
                if !area.bounds(record.at) {
                    continue;
                }
                let dx = (record.at.x() - x).abs() - record.radius.x();
                let dy = (record.at.y() - y).abs() - record.radius.y();
                if !area.holds(dx, dy) {
                    continue;
                }
                let relation = diplomacy
                    .list()
                    .ok_or_else(|| NO_LIST.with("owner", record.owner))?;
                let out = &mut found[relation];
                if out.len() < MOST_FOUND {
                    out.push(Record {
                        distance: Distance::new(sqrt(dy * dy + dx * dx)),
                        ..*record
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
    use crate::ErrorKind;
    use crate::players::relations;
    use crate::test_ids::{class_id, object_id};
    use crate::testing::class;
    use crate::testing::tuning;

    fn registry() -> Registry {
        Registry::new(3, 65)
    }

    #[test]
    fn an_object_joins_list_when_seen_and_leaves_by_swap_with_the_last() {
        let mut registry = registry();
        for id in [1, 2, 3] {
            registry
                .update_unit(UnitUpdate {
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
            registry.lists[0][4]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(1), object_id(2), object_id(3)]
        );
        assert_eq!(registry.lists[2][4].len(), 3);
        assert_eq!(registry.lists[1][4], Vec::<Record>::new());
        // 1 leaves player 0's sight: 3 takes its place
        registry
            .update_unit(UnitUpdate {
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
            registry.lists[0][4]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(3), object_id(2)]
        );
        assert_eq!(registry.entry(object_id(3)).unwrap().slots[0], Some(0));
        assert_eq!(registry.entry(object_id(1)).unwrap().slots[0], None);
        assert_eq!(registry.entry(object_id(1)).unwrap().slots[2], Some(0));
        registry.moved(object_id(3), class::VILLAGER, Point::new(7.0, 7.0));
        assert_eq!(
            (
                registry.lists[0][4][0].at.x(),
                registry.lists[2][4][2].at.y()
            ),
            (7.0, 7.0)
        );
    }

    #[test]
    fn query_finds_the_records_within_its_rounded_box_by_relation() {
        let mut registry = registry();
        registry
            .update_unit(UnitUpdate {
                id: object_id(1),
                owner: PlayerId::new(1),
                position: Point::new(12.0, 10.0),
                radii: Footprint::new(0.2, 0.2),
                class: class::VILLAGER,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(1),
            })
            .unwrap();
        registry
            .update_unit(UnitUpdate {
                id: object_id(2),
                owner: PlayerId::new(2),
                position: Point::new(10.0, 13.5),
                radii: Footprint::new(0.2, 0.2),
                class: class::VILLAGER,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(1),
            })
            .unwrap();
        registry
            .update_unit(UnitUpdate {
                id: object_id(3),
                owner: PlayerId::new(2),
                position: Point::new(13.0, 13.0),
                radii: Footprint::new(0.2, 0.2),
                class: class::VILLAGER,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(1),
            })
            .unwrap();
        registry
            .update_unit(UnitUpdate {
                id: object_id(4),
                owner: PlayerId::new(0),
                position: Point::new(10.5, 10.5),
                radii: Footprint::new(0.2, 0.2),
                class: class::PREY,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(1),
            })
            .unwrap();
        let relations = relations([1, 4, 4]);
        let found = registry
            .visible_units(
                VisibleQuery {
                    player: PlayerId::new(0),
                    position: Point::new(10.0, 10.0),
                    radius: Distance::new(3.0),
                    shape: QueryShape::RoundedRectangle,
                    classes: &[class_id(4), class_id(9)],
                    relations: &relations,
                    size: Footprint::new(0.2, 0.2),
                    wanted: RelationMask::ALL,
                },
                tuning().registry,
            )
            .unwrap()
            .unwrap();
        // 1 is 1.8 off along x, 2 is 3.3 off along y (beyond 3.2), 3 is at the corner and beyond
        // the rounding
        assert_eq!(
            found[UnitRelation::Enemy]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(1)]
        );
        assert_eq!(
            found[UnitRelation::Own]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(4)]
        );
        // the edge distances, the overlap along y included: (1.8, -0.2)
        assert!(
            (found[UnitRelation::Enemy][0].distance - Distance::new((1.8f32 * 1.8 + 0.04).sqrt()))
                .get()
                .abs()
                < 1e-6
        );
        assert_eq!(
            registry
                .visible_units(
                    VisibleQuery {
                        player: PlayerId::new(0),
                        position: Point::new(10.0, 10.0),
                        radius: Distance::new(3.0),
                        shape: QueryShape::RoundedRectangle,
                        classes: &[],
                        relations: &relations,
                        size: Footprint::new(0.2, 0.2),
                        wanted: RelationMask::ALL
                    },
                    tuning().registry
                )
                .unwrap(),
            None
        );
    }

    #[test]
    fn query_caps_its_radius_at_44() {
        let mut registry = registry();
        registry
            .update_unit(UnitUpdate {
                id: object_id(1),
                owner: PlayerId::new(1),
                position: Point::new(53.5, 10.0),
                radii: Footprint::new(0.2, 0.2),
                class: class::VILLAGER,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(1),
            })
            .unwrap();
        registry
            .update_unit(UnitUpdate {
                id: object_id(2),
                owner: PlayerId::new(1),
                position: Point::new(60.0, 10.0),
                radii: Footprint::new(0.2, 0.2),
                class: class::VILLAGER,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(1),
            })
            .unwrap();
        let found = registry
            .visible_units(
                VisibleQuery {
                    player: PlayerId::new(0),
                    position: Point::new(10.0, 10.0),
                    radius: Distance::new(100.0),
                    shape: QueryShape::Rectangle,
                    classes: &[class_id(4)],
                    relations: &relations([1, 4, 4]),
                    size: Footprint::new(0.2, 0.2),
                    wanted: RelationMask::ALL,
                },
                tuning().registry,
            )
            .unwrap()
            .unwrap();
        // the box reaches 44.2 along x: 1's edge is 43.3 off, 2's 49.8
        assert_eq!(
            found[UnitRelation::Enemy]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(1)]
        );
    }

    #[test]
    fn query_keeps_wide_object_whose_centre_lies_beyond_the_radius() {
        let mut registry = registry();
        registry
            .update_unit(UnitUpdate {
                id: object_id(1),
                owner: PlayerId::new(1),
                position: Point::new(15.0, 10.0),
                radii: Footprint::new(2.0, 2.0),
                class: class::VILLAGER,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(1),
            })
            .unwrap();
        let found = registry
            .visible_units(
                VisibleQuery {
                    player: PlayerId::new(0),
                    position: Point::new(10.0, 10.0),
                    radius: Distance::new(3.0),
                    shape: QueryShape::RoundedRectangle,
                    classes: &[class_id(4)],
                    relations: &relations([1, 4, 4]),
                    size: Footprint::new(0.2, 0.2),
                    wanted: RelationMask::ALL,
                },
                tuning().registry,
            )
            .unwrap()
            .unwrap();
        // the centre is 5 off, beyond 3.2, but the edge is 3 off: the 7-tile margin keeps it in
        // the box
        assert_eq!(
            found[UnitRelation::Enemy]
                .iter()
                .map(|x| x.id)
                .collect::<Vec<_>>(),
            [object_id(1)]
        );
    }

    /// The villagers' class, the one the queries below read.
    const VILLAGERS: [ClassId; 1] = [class::VILLAGER];

    /// A registry whose player 0 sees one villager of player 1 at `position`, of `radii`.
    fn registry_seeing(position: Point, radii: Footprint) -> Registry {
        let mut registry = registry();
        registry
            .update_unit(UnitUpdate {
                id: object_id(1),
                owner: PlayerId::new(1),
                position,
                radii,
                class: class::VILLAGER,
                old: VisionBits::NONE,
                new: VisionBits::from_bits_retain(1),
            })
            .unwrap();
        registry
    }

    /// Player 0's query of the villagers from (10, 10) within 3 of a rounded box, for a
    /// searcher of radii 0.2, keeping every relation that `relations` gives.
    fn centre_query(relations: &UnitRelations) -> VisibleQuery<'_> {
        VisibleQuery {
            player: PlayerId::new(0),
            position: Point::new(10.0, 10.0),
            radius: Distance::new(3.0),
            shape: QueryShape::RoundedRectangle,
            classes: &VILLAGERS,
            relations,
            size: Footprint::new(0.2, 0.2),
            wanted: RelationMask::ALL,
        }
    }

    /// Whether `query` finds nothing of any relation.
    fn nothing_found(registry: &Registry, query: VisibleQuery<'_>) -> bool {
        registry
            .visible_units(query, tuning().registry)
            .unwrap()
            .unwrap()
            .values()
            .iter()
            .all(Vec::is_empty)
    }

    /// The ids `query` finds of `relation`.
    fn found_ids(
        registry: &Registry,
        query: VisibleQuery<'_>,
        relation: UnitRelation,
    ) -> Vec<ObjectId> {
        let found = registry
            .visible_units(query, tuning().registry)
            .unwrap()
            .unwrap();
        found[relation].iter().map(|record| record.id).collect()
    }

    /// A NaN radius passes the cap (`MINSS` of 44 and the radius gives the radius): the box's
    /// bounds are NaN and every record fails them.
    #[test]
    fn query_of_nan_radius_finds_nothing() {
        let registry = registry_seeing(Point::new(11.0, 10.0), Footprint::new(0.2, 0.2));
        let relations = relations([1, 4, 4]);
        for shape in [QueryShape::RoundedRectangle, QueryShape::Rectangle] {
            let query = VisibleQuery {
                radius: Distance::new(f32::NAN),
                shape,
                ..centre_query(&relations)
            };
            assert_eq!(found_ids(&registry, query, UnitRelation::Enemy), []);
        }
    }

    /// A record whose radius on x is NaN has a NaN gap on x: the rounded box's first compare
    /// takes the unordered case to the corner's side (`COMISS`, `JC`), whose tests a NaN fails,
    /// though its gap on y lies within the box.
    #[test]
    fn rounded_query_finds_no_record_of_nan_radius_x() {
        let registry = registry_seeing(Point::new(11.0, 10.0), Footprint::new(f32::NAN, 0.2));
        let relations = relations([1, 4, 4]);
        assert_eq!(
            found_ids(&registry, centre_query(&relations), UnitRelation::Enemy),
            []
        );
    }

    /// A record whose radius on y is NaN and whose gap on x is past the searcher's radius goes
    /// to the corner (the second compare's unordered case, `COMISS`, `JC`), whose distance is
    /// NaN, though its gap on x lies within the box.
    #[test]
    fn rounded_query_finds_no_record_of_nan_radius_y() {
        let registry = registry_seeing(Point::new(12.0, 10.0), Footprint::new(0.2, f32::NAN));
        let relations = relations([1, 4, 4]);
        assert_eq!(
            found_ids(&registry, centre_query(&relations), UnitRelation::Enemy),
            []
        );
    }

    /// The corners round by the mean of the radius on both axes, here 5:
    /// `((wide - rx) + (tall - ry)) * 0.5`. A record whose gaps past the searcher's radii are 3
    /// and 4 lies on the rounding and is found (`SETNC`), one a little farther is not.
    #[test]
    fn rounded_query_finds_record_on_its_corner_rounding() {
        let relations = relations([1, 4, 4]);
        let query = VisibleQuery {
            radius: Distance::new(5.0),
            size: Footprint::new(0.5, 0.25),
            ..centre_query(&relations)
        };
        let on_edge = registry_seeing(Point::new(13.5, 14.25), Footprint::new(0.0, 0.0));
        assert_eq!(
            found_ids(&on_edge, query, UnitRelation::Enemy),
            [object_id(1)]
        );
        let beyond = registry_seeing(Point::new(13.5, 14.375), Footprint::new(0.0, 0.0));
        assert_eq!(found_ids(&beyond, query, UnitRelation::Enemy), []);
    }

    /// A record's list is the low byte of its owner's saved relation (`MOVZX`), and its bit the
    /// value's low five bits: a saved 260 keeps and lists an enemy's record.
    #[test]
    fn query_lists_record_by_low_byte_of_its_owner_saved_relation() {
        let registry = registry_seeing(Point::new(11.0, 10.0), Footprint::new(0.2, 0.2));
        let relations = relations([1, 260, 4]);
        assert_eq!(
            found_ids(&registry, centre_query(&relations), UnitRelation::Enemy),
            [object_id(1)]
        );
    }

    /// A saved relation whose bit lies past the mask's byte (8; -1 shifts by 31) lists nowhere,
    /// even for a query that keeps every bit, and one whose bit the mask lacks lists nowhere.
    #[test]
    fn query_skips_record_whose_relation_bit_it_lacks() {
        let registry = registry_seeing(Point::new(11.0, 10.0), Footprint::new(0.2, 0.2));
        for value in [8, -1] {
            let relations = relations([1, value, 4]);
            assert!(nothing_found(&registry, centre_query(&relations)));
        }
        let relations = relations([1, 5, 4]);
        assert!(nothing_found(
            &registry,
            VisibleQuery {
                wanted: RelationMask::of(&[UnitRelation::Enemy]),
                ..centre_query(&relations)
            }
        ));
    }

    /// A record within the area that the mask keeps by a saved relation whose low byte names
    /// none of the five lists (5, 36) is an error: the game writes past its lists. One outside
    /// the area goes to no list.
    #[test]
    fn record_kept_past_the_five_lists_is_an_error() {
        let registry = registry_seeing(Point::new(11.0, 10.0), Footprint::new(0.2, 0.2));
        for value in [5, 36] {
            let relations = relations([1, value, 4]);
            assert_eq!(
                registry
                    .visible_units(centre_query(&relations), tuning().registry)
                    .map_err(|error| error.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
        let far = registry_seeing(Point::new(30.0, 10.0), Footprint::new(0.2, 0.2));
        let relations = relations([1, 5, 4]);
        assert_eq!(
            found_ids(&far, centre_query(&relations), UnitRelation::Enemy),
            []
        );
    }

    /// A class past the player's lists (65, -1) and a record of an owner past the relations the
    /// query reads are errors: the game reads past its arrays.
    #[test]
    fn query_past_its_lists_or_relations_is_an_error() {
        let registry = registry_seeing(Point::new(11.0, 10.0), Footprint::new(0.2, 0.2));
        let enemies = relations([1, 4, 4]);
        for class in [class_id(65), ClassId::of_master(-1)] {
            let classes = [class];
            let query = VisibleQuery {
                classes: &classes,
                ..centre_query(&enemies)
            };
            assert_eq!(
                registry
                    .visible_units(query, tuning().registry)
                    .map_err(|error| error.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
        let gaia_only = relations([1]);
        assert_eq!(
            registry
                .visible_units(centre_query(&gaia_only), tuning().registry)
                .map_err(|error| error.kind()),
            Err(ErrorKind::InvalidInput)
        );
    }
}
