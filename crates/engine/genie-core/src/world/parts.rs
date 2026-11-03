//! The parts only some objects hold, each in a table by id on the world (FD-045-D51), so that an
//! object's row holds only what every object has.
//!
//! The bookkeeping holds by construction: a table's [`PartTable::insert`] and
//! [`PartTable::remove`] are the world's alone, and its list operations (the one writer of the
//! table by id, FD-045-D49) are their writers. An object enters the world as one value with its
//! parts ([`Object`]) and a removal for good returns it with them, so a move between lists or
//! players carries its parts and no part outlives its object or misses it; taking an object out
//! for its update leaves its parts in the tables. [`World::check_parts`] tells when a table holds
//! an id the world does not. A comparison reads an object whole too ([`ObjectView`]), so no
//! part escapes the comparison with the game's save.

use crate::GenieError;
use crate::combat::missile::Missile;
use crate::commands::GatherPoint;
use crate::objects::IdTable;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::layers::Building;
use crate::objects::layers::Combat;
use crate::objects::layers::Motion;
use crate::objects::masters::Master;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::orders::town_bell::TownBell;
use crate::production::Production;
use crate::unit_ai::UnitAi;
use crate::world::Unread;
use crate::world::World;
pub use crate::world::whole::{Object, ObjectParts, ObjectView};
use std::borrow::Cow;

use crate::construction::BuilderCount;
use crate::objects::ObjectState;
use crate::unit_ai::Inbox;
#[cfg(test)]
pub(crate) use crate::world::parts_fixtures::{no_own_masters, no_parts};
use std::mem::replace;
use std::sync::LazyLock;

/// One part's table: the parts packed in a dense vector in the order they came, each id's slot
/// in a table by id. A removal moves the last part into the gap.
#[derive(Clone, Debug)]
pub struct PartTable<T> {
    /// The parts, packed.
    parts: Vec<T>,
    /// The object of each part, by the part's slot.
    ids: Vec<ObjectId>,
    /// Each object's slot in `parts`.
    slots: IdTable<usize>,
}

impl<T> Default for PartTable<T> {
    fn default() -> Self {
        Self {
            parts: Vec::new(),
            ids: Vec::new(),
            slots: IdTable::new(),
        }
    }
}

impl<T> PartTable<T> {
    /// Object `id`'s part, if it holds one.
    #[must_use]
    pub fn get(&self, id: ObjectId) -> Option<&T> {
        self.parts.get(*self.slots.get(id)?)
    }

    /// Object `id`'s part to change, if it holds one.
    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut T> {
        self.parts.get_mut(*self.slots.get(id)?)
    }

    /// Whether object `id` holds the part.
    #[must_use]
    pub fn contains(&self, id: ObjectId) -> bool {
        self.slots.contains_key(id)
    }

    /// How many objects hold the part.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.parts.len()
    }

    /// Whether no object holds the part.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The objects that hold the part and their parts, packed (in the order they came, but for
    /// the removals' moves).
    pub fn iter(&self) -> impl Iterator<Item = (ObjectId, &T)> {
        self.ids.iter().copied().zip(&self.parts)
    }

    /// Object `id` holds `part` (the world's list operations and [`World::set_part`] alone);
    /// the part it held before, if any.
    pub(in crate::world) fn insert(&mut self, id: ObjectId, part: T) -> Option<T> {
        if let Some(held) = self.get_mut(id) {
            return Some(replace(held, part));
        }
        let _ = self.slots.insert(id, self.parts.len());
        self.parts.push(part);
        self.ids.push(id);
        None
    }

    /// Object `id`'s part to change, `make`'s put in first where it holds none (the world's
    /// writers alone).
    ///
    /// # Errors
    ///
    /// None: a part put in is there (the error is unreachable).
    pub(in crate::world) fn get_or_insert_with(
        &mut self,
        id: ObjectId,
        make: impl FnOnce() -> T,
    ) -> Result<&mut T, GenieError> {
        if !self.contains(id) {
            let _ = self.insert(id, make());
        }
        self.get_mut(id)
            .ok_or_else(|| GenieError::unreachable("a part the table just took").with("object", id))
    }

    /// The parts, packed ([`PartTable::unlent`]).
    pub(in crate::world) fn parts(&self) -> &[T] {
        &self.parts
    }

    /// Each object's slot ([`PartTable::unlent`]).
    pub(in crate::world) const fn slots(&self) -> &IdTable<usize> {
        &self.slots
    }

    /// The parts to change beside each object's slot to read ([`PartTable::lend`]).
    pub(in crate::world) fn parts_and_slots_mut(&mut self) -> (&mut [T], &IdTable<usize>) {
        (&mut self.parts, &self.slots)
    }

    /// Object `id`'s part taken out of the table (the world's list operations and
    /// [`World::set_part`] alone).
    pub(in crate::world) fn remove(&mut self, id: ObjectId) -> Option<T> {
        let slot = self.slots.remove(id)?;
        let part = self.parts.swap_remove(slot);
        let _ = self.ids.swap_remove(slot);
        if let Some(&moved) = self.ids.get(slot) {
            let _ = self.slots.insert(moved, slot);
        }
        Some(part)
    }
}

impl PartTable<Building> {
    /// Object `id`'s building layer: [`Building::NONE`] where the table holds none.
    #[must_use]
    pub fn of(&self, id: ObjectId) -> &Building {
        self.get(id).unwrap_or(&NO_BUILDING)
    }
}

impl PartTable<Combat> {
    /// Object `id`'s combat layer: a new object's ([`Combat::default`]) where the table holds
    /// none.
    #[must_use]
    pub fn of(&self, id: ObjectId) -> &Combat {
        self.get(id).unwrap_or(&NO_COMBAT)
    }
}

impl PartTable<UnitAi> {
    /// The notifies object `id` in `state` posts to its own unit AI ([`crate::unit_ai::Inbox`]),
    /// none for an object without one.
    pub(crate) fn inbox(&mut self, id: ObjectId, state: ObjectState) -> Inbox<'_> {
        Inbox {
            ai: self.get_mut(id),
            id,
            state,
        }
    }
}

impl World {
    /// Object `id`'s unit AI lent out of the world's table: the unit AI's side holds it while
    /// it runs ([`crate::objects::unit::Unit::with_side`]); none for an object without one.
    pub(crate) fn lend_unit_ai(&mut self, id: ObjectId) -> Option<UnitAi> {
        self.parts.unit_ai.remove(id)
    }

    /// Object `id`'s lent unit AI `ai` back in the world's table.
    pub(crate) fn return_unit_ai(&mut self, id: ObjectId, ai: UnitAi) {
        let _ = self.parts.unit_ai.insert(id, ai);
    }
}

impl<T: PartialEq> PartialEq for PartTable<T> {
    /// Two tables are equal when the same objects hold equal parts, in any order.
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|(id, p)| other.get(id) == Some(p))
    }
}

/// The parts the world keeps by id: one table per part.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts {
    /// `+0x6b8`: the copy of an old owner's death-spawn master an ownership change took
    /// ([`crate::ownership::change::change_hands`]), which the object's death spawn makes its
    /// dead object of (`FUN_1411a5090`).
    pub dead_master: PartTable<Master>,
    /// A building's gather point (`+0xc30`..`+0xc44`, the save's `gather_point_*`), which its
    /// trained units go to: SET GATHER POINT's and SET GATHER STATE's.
    pub gather_point: PartTable<GatherPoint>,
    /// A building's production lists (`+0xc48`..`+0xd48`, the save's production queue, its make
    /// action and its local lists): QUEUE's first entry makes them.
    pub production: PartTable<Production>,
    /// A missile's own fields (`+0x990`..`+0x99c`, its step `+0x7dc`): its volley makes them.
    pub missile: PartTable<Missile>,
    /// `+0x6c0` (`FUN_141185010`): the master of its own an object's master pointer (`+0xa0`)
    /// reads, a copy (`FUN_141186d10`, the master's slot 0x48) of the one it read when it took
    /// it; an object without one reads its player's. An ownership change gives it one
    /// ([`crate::ownership::change::change_hands`]), an upgrade of its own masters changes it.
    pub own_master: PartTable<Master>,
    /// A combat object's town-bell flag and back-to-work records (`+0x9ac`..`+0x9dc`), only
    /// where they differ from a new unit's ([`TownBell::default`]): a villager that worked or
    /// sheltered, a building that sheltered.
    pub town_bell: PartTable<TownBell>,
    /// A unit's AI (`+0x9e0`, `TRIBE_Unit_AI`): its orders, its notifies and its targets.
    pub unit_ai: PartTable<UnitAi>,
    /// A building's layer (`TRIBE_Building_Object`, `+0xb78`..`+0xce0`): its construction,
    /// links, gate, livestock and relics. An object the table does not hold reads
    /// [`Building::NONE`] ([`Parts::building_of`]); a write puts one in
    /// ([`Parts::building_mut`]), so the table may hold a layer equal to it.
    pub building: PartTable<Building>,
    /// A combat object's layer (`RGE_Combat_Object` and `TRIBE_Combat_Object`, types 50 and up):
    /// its attack's timers and counts, its volleys, its charge, its powerups and stingers, its
    /// store. An object the table does not hold reads a new object's ([`Parts::combat_of`]); a
    /// write puts one in ([`Parts::combat_mut`]), so the table may hold a layer equal to it.
    pub combat: PartTable<Combat>,
    /// An object's layers between its place and its action layer (types 20 and up): the
    /// animated layer, and a moving object's mover with its movement state (FD-045-D58). A
    /// static object has none.
    pub motion: PartTable<Motion>,
    /// An object's action layer (`RGE_Action_Object`, types 40 and up): its action list and its
    /// master's default task (FD-045-D59). An object the table does not hold reads an empty one
    /// ([`Parts::actions_of`], a tree's); a write puts one in ([`Parts::actions_mut`]). The
    /// updating object's layer is out of the table with its row ([`Parts::take_actions`]).
    pub actions: PartTable<Action>,
    /// The objects the load could not read whole ([`Unread`], FD-045-D61): each one's reason,
    /// the import's gap. A routine that reaches such an object counts it busy or stops the run
    /// with the reason. The updating object's entry stays in the table.
    pub unread: PartTable<Unread>,
}

/// The building layer of an object the table does not hold.
static NO_BUILDING: Building = Building::NONE;

/// Object `id`'s repairers or builders to change ([`Parts::repairers_of`]) in the tables
/// `buildings` and `combats`: a building's construction's, else its combat layer's (a new
/// object's put in where the table holds none).
///
/// # Errors
///
/// None (an unreachable error where a table lost the part it took).
pub(in crate::world) fn repairers_in<'a>(
    buildings: &'a mut PartTable<Building>,
    combats: &'a mut PartTable<Combat>,
    id: ObjectId,
) -> Result<&'a mut BuilderCount, GenieError> {
    if buildings.get(id).is_some_and(|b| b.construction.is_some()) {
        return buildings
            .get_mut(id)
            .and_then(|b| b.construction.as_mut())
            .map(|k| &mut k.builders)
            .ok_or_else(|| {
                GenieError::unreachable("a construction the table just held").with("object", id)
            });
    }
    Ok(&mut combats.get_or_insert_with(id, Combat::default)?.builders)
}

/// The combat layer of an object the table does not hold: a new object's.
static NO_COMBAT: LazyLock<Combat> = LazyLock::new(Combat::default);

impl Parts {
    /// Object `id`'s parts filed in the tables (it enters the world).
    pub(in crate::world) fn file(&mut self, id: ObjectId, parts: ObjectParts) {
        let ObjectParts {
            dead_master,
            gather_point,
            production,
            missile,
            own_master,
            town_bell,
            unit_ai,
            building,
            combat,
            motion,
            actions,
            unread,
        } = parts;
        if let Some(m) = dead_master {
            let _ = self.dead_master.insert(id, m);
        }
        if let Some(g) = gather_point {
            let _ = self.gather_point.insert(id, g);
        }
        if let Some(p) = production {
            let _ = self.production.insert(id, p);
        }
        if let Some(m) = missile {
            let _ = self.missile.insert(id, m);
        }
        if let Some(m) = own_master {
            let _ = self.own_master.insert(id, m);
        }
        if town_bell != TownBell::default() {
            let _ = self.town_bell.insert(id, town_bell);
        }
        if let Some(ai) = unit_ai {
            let _ = self.unit_ai.insert(id, ai);
        }
        if building != Building::NONE {
            let _ = self.building.insert(id, building);
        }
        if combat != Combat::default() {
            let _ = self.combat.insert(id, combat);
        }
        if let Some(m) = motion {
            let _ = self.motion.insert(id, m);
        }
        if actions != Action::default() {
            let _ = self.actions.insert(id, actions);
        }
        if let Some(u) = unread {
            let _ = self.unread.insert(id, u);
        }
    }

    /// Object `id`'s parts taken out of the tables (it leaves the world).
    pub(in crate::world) fn take(&mut self, id: ObjectId) -> ObjectParts {
        ObjectParts {
            dead_master: self.dead_master.remove(id),
            gather_point: self.gather_point.remove(id),
            production: self.production.remove(id),
            missile: self.missile.remove(id),
            own_master: self.own_master.remove(id),
            town_bell: self.town_bell.remove(id).unwrap_or_default(),
            unit_ai: self.unit_ai.remove(id),
            building: self.building.remove(id).unwrap_or_default(),
            combat: self.combat.remove(id).unwrap_or_default(),
            motion: self.motion.remove(id),
            actions: self.actions.remove(id).unwrap_or_default(),
            unread: self.unread.remove(id),
        }
    }

    /// Object `id`'s parts, copied (a comparison's whole object).
    #[must_use]
    pub fn of(&self, id: ObjectId) -> ObjectParts {
        ObjectParts {
            dead_master: self.dead_master.get(id).cloned(),
            gather_point: self.gather_point.get(id).copied(),
            production: self.production.get(id).cloned(),
            missile: self.missile.get(id).copied(),
            own_master: self.own_master.get(id).cloned(),
            town_bell: self.town_bell.get(id).copied().unwrap_or_default(),
            unit_ai: self.unit_ai.get(id).cloned(),
            building: self.building_of(id).clone(),
            combat: self.combat_of(id).clone(),
            motion: self.motion.get(id).cloned(),
            actions: self.actions_of(id).clone(),
            unread: self.unread.get(id).cloned(),
        }
    }

    /// Object `id`'s building layer: [`Building::NONE`] where the table holds none.
    #[must_use]
    pub fn building_of(&self, id: ObjectId) -> &Building {
        self.building.of(id)
    }

    /// Object `id`'s building layer to change, [`Building::NONE`] put in where the table holds
    /// none. The caller holds the object: its row is borrowed from the world's lists, which this
    /// borrow leaves alone ([`World::set_part`] checks the id where no row is held).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn building_mut(&mut self, id: ObjectId) -> Result<&mut Building, GenieError> {
        self.building.get_or_insert_with(id, || Building::NONE)
    }

    /// Object `id`'s combat layer: a new object's ([`Combat::default`]) where the table holds
    /// none.
    #[must_use]
    pub fn combat_of(&self, id: ObjectId) -> &Combat {
        self.combat.of(id)
    }

    /// Object `id`'s combat layer to change, a new object's put in where the table holds none.
    /// The caller holds the object: its row is borrowed from the world's lists, which this
    /// borrow leaves alone ([`World::set_part`] checks the id where no row is held).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn combat_mut(&mut self, id: ObjectId) -> Result<&mut Combat, GenieError> {
        self.combat.get_or_insert_with(id, Combat::default)
    }

    /// Object `id`'s repairers or builders (`+0x9e8`, `num_builders`): a building's in its
    /// construction, any other's in its combat layer.
    #[must_use]
    pub fn repairers_of(&self, id: ObjectId) -> BuilderCount {
        self.building_of(id)
            .construction
            .map_or_else(|| self.combat_of(id).builders, |k| k.builders)
    }

    /// [`Parts::repairers_of`] to change ([`repairers_in`]).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where a table lost the part it took).
    pub(crate) fn repairers_mut(&mut self, id: ObjectId) -> Result<&mut BuilderCount, GenieError> {
        repairers_in(&mut self.building, &mut self.combat, id)
    }

    /// Object `id`'s unit AI (none without one) and its combat layer ([`Parts::combat_mut`]),
    /// to change together.
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn ai_and_combat_mut(
        &mut self,
        id: ObjectId,
    ) -> Result<(Option<&mut UnitAi>, &mut Combat), GenieError> {
        let combat = self.combat.get_or_insert_with(id, Combat::default)?;
        Ok((self.unit_ai.get_mut(id), combat))
    }

    /// Object `id`'s town bell ([`Parts::town_bell`]): a new unit's where the table holds none.
    #[must_use]
    pub fn town_bell_of(&self, id: ObjectId) -> TownBell {
        self.town_bell.get(id).copied().unwrap_or_default()
    }

    /// Object `id`'s town bell becomes `bell`, a new unit's leaving the table. The caller holds
    /// the object: its row is borrowed from the world's lists, which this borrow leaves alone
    /// ([`World::set_part`] checks the id where no row is held).
    pub(crate) fn set_town_bell(&mut self, id: ObjectId, bell: TownBell) {
        if bell == TownBell::default() {
            let _ = self.town_bell.remove(id);
        } else {
            let _ = self.town_bell.insert(id, bell);
        }
    }

    /// Object `id`'s town-bell flag (`+0x9dc`) becomes `flag` ([`Parts::set_town_bell`]).
    pub(crate) fn set_town_bell_flag(&mut self, id: ObjectId, flag: bool) {
        let bell = TownBell {
            flag,
            ..self.town_bell_of(id)
        };
        self.set_town_bell(id, bell);
    }

    /// Every object id a table holds, each once per table that holds it.
    fn ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.dead_master
            .iter()
            .map(|(id, _)| id)
            .chain(self.gather_point.iter().map(|(id, _)| id))
            .chain(self.production.iter().map(|(id, _)| id))
            .chain(self.missile.iter().map(|(id, _)| id))
            .chain(self.own_master.iter().map(|(id, _)| id))
            .chain(self.town_bell.iter().map(|(id, _)| id))
            .chain(self.unit_ai.iter().map(|(id, _)| id))
            .chain(self.building.iter().map(|(id, _)| id))
            .chain(self.combat.iter().map(|(id, _)| id))
            .chain(self.motion.iter().map(|(id, _)| id))
            .chain(self.actions.iter().map(|(id, _)| id))
            .chain(self.unread.iter().map(|(id, _)| id))
    }
}

/// A part of an object: which table [`World::set_part`] writes.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    /// The dead master ([`Parts::dead_master`]), boxed: a master is about 700 bytes.
    DeadMaster(Box<Master>),
    /// The gather point ([`Parts::gather_point`]), none clearing it.
    GatherPoint(Option<GatherPoint>),
    /// The production lists ([`Parts::production`]), boxed, none clearing them.
    Production(Option<Box<Production>>),
    /// The missile's own fields ([`Parts::missile`]), none clearing them.
    Missile(Option<Missile>),
    /// The own master ([`Parts::own_master`]), boxed: a master is about 700 bytes.
    OwnMaster(Box<Master>),
    /// The town bell ([`Parts::town_bell`]); a new unit's leaves the table.
    TownBell(TownBell),
    /// The unit AI ([`Parts::unit_ai`]), boxed, none clearing it.
    UnitAi(Option<Box<UnitAi>>),
    /// The building layer ([`Parts::building`]), boxed; [`Building::NONE`] leaves the table.
    Building(Box<Building>),
    /// The combat layer ([`Parts::combat`]), boxed; a new object's leaves the table.
    Combat(Box<Combat>),
    /// The animated and moving layers ([`Parts::motion`]), boxed, none clearing them.
    Motion(Option<Box<Motion>>),
    /// The action layer ([`Parts::actions`]), boxed; an empty one leaves the table.
    Actions(Box<Action>),
    /// Why the load could not read the object whole ([`Parts::unread`]), none clearing it.
    Unread(Option<Unread>),
}

impl World {
    /// Object `id` holds `part` now, in its life (the routine that makes the part). The world
    /// must hold the object: a part of an object it does not hold would outlive it.
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    pub fn set_part(&mut self, id: ObjectId, part: Part) -> Result<(), GenieError> {
        if self.index.filed(id).is_none() && !self.index.scans() {
            return Err(
                GenieError::unreachable("a part of an object the world does not hold")
                    .with("object", id),
            );
        }
        match part {
            Part::DeadMaster(m) => {
                let _ = self.parts.dead_master.insert(id, *m);
            }
            Part::GatherPoint(Some(g)) => {
                let _ = self.parts.gather_point.insert(id, g);
            }
            Part::GatherPoint(None) => {
                let _ = self.parts.gather_point.remove(id);
            }
            Part::Production(Some(p)) => {
                let _ = self.parts.production.insert(id, *p);
            }
            Part::Production(None) => {
                let _ = self.parts.production.remove(id);
            }
            Part::Missile(Some(m)) => {
                let _ = self.parts.missile.insert(id, m);
            }
            Part::Missile(None) => {
                let _ = self.parts.missile.remove(id);
            }
            Part::OwnMaster(m) => {
                let _ = self.parts.own_master.insert(id, *m);
            }
            Part::TownBell(b) => self.parts.set_town_bell(id, b),
            Part::UnitAi(Some(ai)) => {
                let _ = self.parts.unit_ai.insert(id, *ai);
            }
            Part::UnitAi(None) => {
                let _ = self.parts.unit_ai.remove(id);
            }
            Part::Building(b) if *b == Building::NONE => {
                let _ = self.parts.building.remove(id);
            }
            Part::Building(b) => {
                let _ = self.parts.building.insert(id, *b);
            }
            Part::Combat(b) if *b == Combat::default() => {
                let _ = self.parts.combat.remove(id);
            }
            Part::Combat(b) => {
                let _ = self.parts.combat.insert(id, *b);
            }
            Part::Motion(Some(m)) => {
                let _ = self.parts.motion.insert(id, *m);
            }
            Part::Motion(None) => {
                let _ = self.parts.motion.remove(id);
            }
            Part::Actions(a) => self.parts.put_actions(id, *a),
            Part::Unread(Some(u)) => {
                let _ = self.parts.unread.insert(id, u);
            }
            Part::Unread(None) => {
                let _ = self.parts.unread.remove(id);
            }
        }
        Ok(())
    }

    /// The masters an object's master pointer reads: the players' tables and the objects' own
    /// ([`MasterTables`]).
    #[must_use]
    pub fn master_tables(&self) -> MasterTables<'_> {
        MasterTables {
            players: &self.model.masters,
            own: &self.parts.own_master,
        }
    }

    /// Object `id`'s production lists, made empty when it has none (QUEUE's first entry at a
    /// building, a technology's local list). The world must hold the object.
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    pub fn production_or_new(&mut self, id: ObjectId) -> Result<&mut Production, GenieError> {
        if !self.parts.production.contains(id) {
            self.set_part(id, Part::Production(Some(Box::default())))?;
        }
        self.parts
            .production
            .get_mut(id)
            .ok_or(GenieError::unreachable("a production the world just filed"))
    }

    /// A loaded object's saved parts go into the tables (the import, after the world's lists
    /// stand). The world must hold the object.
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    pub fn load_parts(&mut self, id: ObjectId, parts: ObjectParts) -> Result<(), GenieError> {
        if self.index.filed(id).is_none() && !self.index.scans() {
            return Err(GenieError::unreachable(
                "a loaded part of an object the world does not hold",
            )
            .with("object", id));
        }
        self.parts.file(id, parts);
        Ok(())
    }

    /// Object `unit` of the world's lists with its parts from the world's tables, as a
    /// comparison reads it.
    #[must_use]
    pub fn object_view<'a>(&'a self, unit: &'a Unit) -> ObjectView<'a> {
        ObjectView {
            unit,
            parts: Cow::Owned(self.parts.of(unit.id)),
        }
    }

    /// Object `unit` of the world's lists with its parts from the world's tables, as one value
    /// apart from the world (a run's result).
    #[must_use]
    pub fn object_of(&self, unit: &Unit) -> Object {
        Object {
            unit: unit.clone(),
            parts: self.parts.of(unit.id),
        }
    }

    /// The parts' bookkeeping: every id a table holds is an object the world files (live,
    /// sleeping, or out for its update).
    ///
    /// # Errors
    ///
    /// The first id a table holds that the world does not.
    pub fn check_parts(&self) -> Result<(), GenieError> {
        if self.index.scans() {
            return Ok(());
        }
        self.parts
            .ids()
            .find(|&id| self.index.filed(id).is_none())
            .map_or(Ok(()), |id| {
                Err(
                    GenieError::unreachable("a part whose object left the world")
                        .with("object", id),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::combat::damage::ArmorPair;
    #[cfg(test)]
    use crate::commands::GatherPointState;
    #[cfg(test)]
    use crate::map::{Distance, Offset, Point, Position};
    #[cfg(test)]
    use crate::objects::HitPoints;
    #[cfg(test)]
    use crate::objects::lifecycle::Lifecycle;
    #[cfg(test)]
    use crate::orders::town_bell::BackToWork;
    use crate::test_ids::object_id;
    #[cfg(test)]
    use crate::test_ids::tech_id;
    #[cfg(test)]
    use crate::world::index::ObjectsRef;
    #[cfg(test)]
    use crate::world::player::Player;
    #[cfg(test)]
    use crate::world::{FrameSeed, Millis, WorldTime};
    use std::sync::Arc;

    /// The table finds each object's part by id; a removal moves the last part into the gap and
    /// its slot follows; a second insert replaces; two tables of the same parts in another
    /// order are equal.
    #[test]
    fn part_table_packs_its_parts_and_finds_them_by_id() {
        let mut t = PartTable::default();
        assert_eq!(t.insert(object_id(5), 'a'), None);
        assert_eq!(t.insert(object_id(-3), 'b'), None);
        assert_eq!(t.insert(object_id(9), 'c'), None);
        assert_eq!(t.insert(object_id(9), 'd'), Some('c'));
        assert_eq!((t.len(), t.get(object_id(-3))), (3, Some(&'b')));
        assert_eq!(t.remove(object_id(5)), Some('a'));
        assert_eq!(t.remove(object_id(5)), None);
        assert_eq!(t.get(object_id(9)), Some(&'d'));
        assert_eq!(
            t.iter().collect::<Vec<_>>(),
            [(object_id(9), &'d'), (object_id(-3), &'b')]
        );
        *t.get_mut(object_id(-3)).unwrap() = 'e';
        let mut u = PartTable::default();
        let _ = u.insert(object_id(-3), 'e');
        let _ = u.insert(object_id(9), 'd');
        assert_eq!(t, u);
        assert!(!t.contains(object_id(5)) && !t.is_empty());
    }

    /// The world files an object's parts with it: a part made in its life only for an object
    /// it files; the lists' removal returns the object with its parts, which leave the tables,
    /// and a push files them again; the world's view and its whole object carry them, and an
    /// object without them differs at the part's old path; a table entry whose object the
    /// world does not file fails the check.
    #[test]
    fn the_world_files_an_object_parts_with_it() {
        use crate::objects::ListIndex;
        use crate::players::PlayerId;
        use genie_diff::{Scope, differences};
        let id = object_id(5);
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: vec![Unit {
                    id,
                    ..Unit::default()
                }]
                .into(),
                ..Default::default()
            }],
        );
        let dead = Master::default();
        assert!(
            t.set_part(object_id(6), Part::DeadMaster(Box::new(dead.clone())))
                .is_err()
        );
        t.set_part(id, Part::DeadMaster(Box::new(dead.clone())))
            .unwrap();
        t.check_parts().unwrap();
        let unit = t.players[0].objects.by_id(id).unwrap();
        let view = t.object_view(unit);
        assert_eq!(view.parts.dead_master.as_ref(), Some(&dead));
        let whole = t.object_of(unit);
        assert_eq!(differences(&view, &whole.view(), Scope::All), []);
        let bare = Object::from(unit.clone());
        let paths: Vec<String> = differences(&view, &bare.view(), Scope::SavedWithGaps)
            .into_iter()
            .map(|d| d.path)
            .collect();
        assert_eq!(paths, ["dead_master"]);
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::FIRST)
            .unwrap();
        assert_eq!(gone.parts.dead_master.as_ref(), Some(&dead));
        assert!(t.parts.dead_master.is_empty());
        t.check_parts().unwrap();
        let _ = t.push_live(PlayerId::new(0), gone).unwrap();
        assert!(t.parts.dead_master.contains(id));
        let _ = t.parts.dead_master.insert(object_id(9), dead);
        assert!(t.check_parts().is_err());
    }

    /// A loaded object's parts go into the tables only for an object the world holds; the
    /// gather point's and the missile's parts are set and cleared in the object's life, and its
    /// removal from the lists carries them.
    #[test]
    fn loaded_object_parts_go_into_the_tables() {
        use crate::objects::ListIndex;
        use crate::players::PlayerId;
        let id = object_id(5);
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: vec![Unit {
                    id,
                    ..Unit::default()
                }]
                .into(),
                ..Default::default()
            }],
        );
        let gather = gather_point(3.5);
        let missile = Missile {
            max_range: Distance::new(7.0),
            fired_from: None,
            charge: true,
            flying: true,
            step: Offset::ZERO,
        };
        let parts = ObjectParts {
            gather_point: Some(gather),
            missile: Some(missile),
            ..ObjectParts::default()
        };
        assert!(t.load_parts(object_id(6), parts.clone()).is_err());
        assert!(t.parts.gather_point.is_empty());
        t.load_parts(id, parts).unwrap();
        assert_eq!(t.parts.gather_point.get(id), Some(&gather));
        assert_eq!(t.parts.missile.get(id), Some(&missile));
        t.set_part(id, Part::Missile(None)).unwrap();
        assert!(t.parts.missile.is_empty());
        t.set_part(id, Part::Missile(Some(missile))).unwrap();
        t.check_parts().unwrap();
        t.set_part(id, Part::GatherPoint(None)).unwrap();
        assert!(t.parts.gather_point.is_empty());
        t.set_part(id, Part::GatherPoint(Some(gather))).unwrap();
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::FIRST)
            .unwrap();
        assert_eq!(
            (gone.parts.gather_point, gone.parts.missile),
            (Some(gather), Some(missile))
        );
        assert!(t.parts.gather_point.is_empty() && t.parts.missile.is_empty());
    }

    /// The unit AI goes into the table with its object and leaves with it. Its unit's notifies
    /// go to it by id; while the AI's side lends it out of the table they go nowhere, and it
    /// comes back as the side left it.
    #[test]
    fn the_unit_ai_is_lent_out_of_the_table_and_given_back() {
        use crate::objects::{ListIndex, ObjectState};
        use crate::players::PlayerId;
        use crate::unit_ai::NotifyKind;
        let id = object_id(5);
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player::default()],
        );
        let unit = Unit {
            id,
            ..Unit::default()
        };
        let _ = t
            .push_live(PlayerId::new(0), Object::with_ai(unit, UnitAi::default()))
            .unwrap();
        t.check_parts().unwrap();
        let post = |t: &mut World| {
            t.parts
                .unit_ai
                .inbox(id, ObjectState::Alive)
                .post(NotifyKind::DAMAGED, [1, 2, 3])
                .unwrap();
        };
        let heard = |t: &World| t.parts.unit_ai.get(id).map(|ai| ai.notifies.len());
        post(&mut t);
        assert_eq!(heard(&t), Some(1));
        let ai = t.lend_unit_ai(id).unwrap();
        post(&mut t);
        assert_eq!(heard(&t), None);
        t.return_unit_ai(id, ai);
        assert_eq!(heard(&t), Some(1));
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::FIRST)
            .unwrap();
        assert_eq!(gone.parts.unit_ai.map(|ai| ai.notifies.len()), Some(1));
        assert!(t.parts.unit_ai.is_empty());
    }

    /// A building's production lists are made empty at their first use and found again after;
    /// an object the world does not hold gets none.
    #[test]
    fn production_is_made_empty_once_and_found_again() {
        use crate::players::PlayerId;
        let id = object_id(5);
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: vec![Unit {
                    id,
                    ..Unit::default()
                }]
                .into(),
                ..Default::default()
            }],
        );
        assert!(t.production_or_new(object_id(6)).is_err());
        assert!(t.parts.production.is_empty());
        let made = t.production_or_new(id).unwrap();
        assert_eq!(*made, Production::default());
        made.disabled_techs.push(tech_id(3));
        let again = t.production_or_new(id).unwrap();
        assert_eq!(again.disabled_techs, [tech_id(3)]);
        assert_eq!(t.parts.production.len(), 1);
        t.set_part(id, Part::Production(None)).unwrap();
        assert!(t.parts.production.is_empty());
        t.check_parts().unwrap();
    }

    /// A town bell that differs from a new unit's is in the table, a new unit's is not: the
    /// flag's set files the object, its clear lets it go; a record keeps it; a loaded new unit's
    /// bell files nothing, and a removal gives back a new unit's for an object without one.
    #[test]
    fn the_table_holds_only_the_town_bells_that_differ_from_new_unit_s() {
        use crate::objects::ListIndex;
        use crate::players::PlayerId;
        let id = object_id(5);
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: vec![Unit {
                    id,
                    ..Unit::default()
                }]
                .into(),
                ..Default::default()
            }],
        );
        t.load_parts(id, ObjectParts::default()).unwrap();
        assert!(t.parts.town_bell.is_empty());
        t.parts.set_town_bell_flag(id, true);
        assert!(t.parts.town_bell_of(id).flag && t.parts.town_bell.contains(id));
        t.parts.set_town_bell_flag(id, false);
        assert!(t.parts.town_bell.is_empty());
        let mut bell = t.parts.town_bell_of(id);
        bell.record(BackToWork {
            point: Point::new(2.0, 3.0),
            ..BackToWork::NONE
        });
        t.set_part(id, Part::TownBell(bell)).unwrap();
        assert_eq!(t.parts.town_bell_of(id), bell);
        t.check_parts().unwrap();
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::FIRST)
            .unwrap();
        assert_eq!(gone.parts.town_bell, bell);
        assert!(t.parts.town_bell.is_empty());
        assert_eq!(t.parts.town_bell_of(id), TownBell::default());
    }

    /// An object the load could not read whole holds its reason in the table: the load files
    /// it, the objects by id read it, a removal carries it, a write of none clears it.
    #[test]
    fn an_unread_object_holds_its_reason_in_the_table() {
        use crate::objects::ListIndex;
        use crate::players::PlayerId;
        let id = object_id(5);
        let why = Unread::new(GenieError::unsupported("a saved production list"));
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: vec![Unit {
                    id,
                    ..Unit::default()
                }]
                .into(),
                ..Default::default()
            }],
        );
        t.load_parts(
            id,
            ObjectParts {
                unread: Some(why.clone()),
                ..ObjectParts::default()
            },
        )
        .unwrap();
        assert_eq!(t.parts.unread.get(id), Some(&why));
        assert_eq!(t.parts.of(id).unread, Some(why.clone()));
        let by_id = ObjectsRef::new(
            &t.players,
            &t.index,
            &t.model.masters,
            &t.parts,
            &t.model.roles,
            &t.model.tuning,
        );
        assert_eq!(by_id.unread(id), Some(&why));
        t.check_parts().unwrap();
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::FIRST)
            .unwrap();
        assert_eq!(gone.parts.unread.as_ref(), Some(&why));
        assert!(t.parts.unread.is_empty());
        let _ = t.push_live(PlayerId::new(0), gone).unwrap();
        assert!(t.parts.unread.contains(id));
        t.set_part(id, Part::Unread(None)).unwrap();
        assert!(t.parts.unread.is_empty());
    }

    /// An object's own master is a part the world's master lookups read before its player's
    /// table ([`World::master_tables`], [`MasterTables::of`]); an object without one reads its
    /// player's, and its removal from the lists carries the part.
    #[test]
    fn an_object_reads_its_own_master_from_the_world_table() {
        use crate::objects::ListIndex;
        use crate::objects::MasterHitPoints;
        use crate::objects::masters::Masters;
        use crate::players::PlayerId;
        use crate::test_ids::master_id;
        let (id, other) = (object_id(5), object_id(6));
        let unit = |id| Unit {
            id,
            life: Lifecycle {
                master_id: master_id(4),
                ..Default::default()
            },
            ..Unit::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: vec![unit(id), unit(other)].into(),
                ..Default::default()
            }],
        );
        let master = |hp| Master {
            id: master_id(4),
            hit_points: MasterHitPoints::of_data(hp),
            ..Master::default()
        };
        t.model.masters = vec![Arc::new(Masters::new([master(25)]))];
        t.set_part(id, Part::OwnMaster(Box::new(master(40))))
            .unwrap();
        let hp = |t: &World, i: usize| {
            let u = &t.players[0].objects[ListIndex::new(i)];
            let own = u.master(t.master_tables(), PlayerId::new(0)).unwrap();
            let in_owner = u.master_in(t.master_tables().of(PlayerId::new(0))).unwrap();
            assert_eq!(own, in_owner);
            own.hit_points.get()
        };
        assert_eq!((hp(&t, 0), hp(&t, 1)), (40, 25));
        let gone = t
            .swap_remove_live(PlayerId::new(0), ListIndex::FIRST)
            .unwrap();
        assert_eq!(gone.parts.own_master.map(|m| m.hit_points.get()), Some(40));
        assert!(t.parts.own_master.is_empty());
    }

    /// A gather point at `x` on no target.
    fn gather_point(x: f32) -> GatherPoint {
        GatherPoint {
            state: GatherPointState::Point,
            point: Position::new(x, 1.0, 0.0),
            target: None,
            target_master: None,
        }
    }

    /// A whole object compares its row's fields and its parts' at the paths they had on the
    /// object: the dead and own masters are known gaps (left out of the saved scope), the
    /// gather point and the production compare at the building layer's paths, the town bell and
    /// the unit AI at the combat layer's, the missile at its own, a row's field keeps its own
    /// path.
    #[test]
    fn whole_object_compares_its_parts_at_their_old_paths() {
        use genie_diff::{Scope, differences};
        let a = Object::from(Unit {
            id: object_id(1),
            ..Unit::default()
        });
        let mut b = a.clone();
        b.parts.dead_master = Some(Master::default());
        b.unit.life.hp = HitPoints::new(7.0);
        b.parts.gather_point = Some(gather_point(2.0));
        b.parts.production = Some(Production::default());
        b.parts.own_master = Some(Master::default());
        b.parts.town_bell.flag = true;
        b.parts.unit_ai = Some(UnitAi::default());
        b.parts.missile = Some(Missile {
            max_range: Distance::ZERO,
            fired_from: None,
            charge: false,
            flying: true,
            step: Offset::ZERO,
        });
        b.parts.building.garrison_time = WorldTime::from_millis(5);
        b.parts.combat.armor_removed = ArmorPair::new(1, 0);
        let paths = |scope| {
            differences(&a, &b, scope)
                .into_iter()
                .map(|d| d.path)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            paths(Scope::Saved),
            [
                "life.hp",
                "building.gather_point",
                "building.production",
                "missile",
                "combat.town_bell.flag",
                "combat.ai",
                "building.garrison_time",
                "combat.armor_removed.melee"
            ]
        );
        assert_eq!(
            paths(Scope::SavedWithGaps),
            [
                "life.hp",
                "dead_master",
                "building.gather_point",
                "building.production",
                "missile",
                "own_master",
                "combat.town_bell.flag",
                "combat.ai",
                "building.garrison_time",
                "combat.armor_removed.melee"
            ]
        );
    }
}
