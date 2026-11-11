//! The world's objects by id to read ([`ObjectsRef`]): the players' lists through the table by
//! id ([`crate::world::index::ObjectIndex`]), with the players' masters and the objects' parts.

use crate::GenieResult;
use crate::construction::{Annexes, MainBuildingId};
use crate::map::spatial::{Gait, RecordId};
use crate::movement::Mover;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::layers::{Action, Building, Combat, Motion};
use crate::objects::masters::Master;
use crate::objects::masters::{MasterTables, Masters};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::players::treaty::TreatyMonitor;
#[cfg(test)]
use crate::testing::static_roles;
#[cfg(test)]
use crate::testing::tuning;
use crate::world::Unread;
use crate::world::index::ObjectIndex;
use crate::world::layer::Roles;
use crate::world::layer::generated::Tuning;
use crate::world::motions::Lent;
#[cfg(test)]
use crate::world::parts::no_parts;
use crate::world::parts::{PartTable, Parts};
use crate::world::player::Player;
use std::sync::Arc;

/// The part tables [`ObjectsRef::of_tables`] reads: the building, combat and action layers, the
/// objects the load could not read whole and the motion layers (one of them lent out).
pub type PartTables<'world> = (
    &'world PartTable<Building>,
    &'world PartTable<Combat>,
    &'world PartTable<Action>,
    &'world PartTable<Unread>,
    Lent<'world, Motion>,
);

/// The world's objects by id, to read, beside the parts a caller holds apart (a walk holds the
/// spatial index and the planners): the players' lists, the table by id, the players' masters,
/// the objects' parts, the build's roles of their classes and masters and the world's treaty
/// monitor, which the players' ally test reads. An object out for its update is none.
#[derive(Clone, Copy, Debug)]
pub struct ObjectsRef<'world> {
    players: &'world [Player],
    index: &'world ObjectIndex,
    masters: MasterTables<'world>,
    buildings: &'world PartTable<Building>,
    combats: &'world PartTable<Combat>,
    actions: &'world PartTable<Action>,
    unread: &'world PartTable<Unread>,
    motions: Lent<'world, Motion>,
    roles: &'world Roles,
    tuning: &'world Tuning,
    treaty: Option<&'world TreatyMonitor>,
}

impl<'world> ObjectsRef<'world> {
    /// The objects of `players`, which the world's table `index` finds, with the players'
    /// `masters`, the objects' `parts` (their own masters, their building, combat and motion
    /// layers), the build's `roles` and its `tuning`, and the world's treaty monitor `treaty`
    /// (`+0x820`).
    #[must_use]
    pub fn new(
        players: &'world [Player],
        index: &'world ObjectIndex,
        masters: &'world [Arc<Masters>],
        parts: &'world Parts,
        roles: &'world Roles,
        tuning: &'world Tuning,
        treaty: Option<&'world TreatyMonitor>,
    ) -> Self {
        Self::of_tables(
            players,
            index,
            MasterTables::new(masters, &parts.own_master),
            (
                &parts.building,
                &parts.combat,
                &parts.actions,
                &parts.unread,
                parts.motion.unlent(),
            ),
            roles,
            tuning,
            treaty,
        )
    }

    /// [`ObjectsRef::new`] of the part tables it reads, for a caller that holds another part
    /// table to change: the world's `masters` (the players' and the objects' own), the building
    /// layers `buildings`, the combat layers `combats`, the action layers `actions`, the objects
    /// the load could not read whole `unread` and the motion layers `motions` (one of them lent
    /// out to a walk,
    /// [`crate::world::parts::PartTable::lend`]), the build's `roles` and its `tuning`, and the
    /// world's treaty monitor `treaty`.
    #[must_use]
    pub const fn of_tables(
        players: &'world [Player],
        index: &'world ObjectIndex,
        masters: MasterTables<'world>,
        (buildings, combats, actions, unread, motions): PartTables<'world>,
        roles: &'world Roles,
        tuning: &'world Tuning,
        treaty: Option<&'world TreatyMonitor>,
    ) -> Self {
        Self {
            players,
            index,
            masters,
            buildings,
            combats,
            actions,
            unread,
            motions,
            roles,
            tuning,
            treaty,
        }
    }

    /// Object `id`'s mover; none for an object that does not move, one the world does not hold
    /// or one out for its update.
    #[must_use]
    pub fn mover_of(&self, id: impl ObjectKey) -> Option<&'world Mover> {
        let unit = self.get(id)?;
        self.motions.get(unit.id).and_then(Motion::mover)
    }

    /// Object `id`'s combat layer, an object out for its update's too: a new object's where
    /// the table holds none ([`crate::world::parts::Parts::combat_of`]).
    #[must_use]
    pub fn combat_of(&self, id: ObjectId) -> &'world Combat {
        self.combats.of(id)
    }

    /// The world's masters these objects read: the players' tables and the objects' own.
    #[must_use]
    pub const fn masters(&self) -> MasterTables<'world> {
        self.masters
    }

    /// The build's roles of these objects' classes and masters (FD-046-D45).
    #[must_use]
    pub const fn roles(&self) -> &'world Roles {
        self.roles
    }

    /// The build's tuning values these objects' systems read (FD-046-D49).
    #[must_use]
    pub const fn tuning(&self) -> &'world Tuning {
        self.tuning
    }

    /// The world's treaty monitor (`+0x820`), which the ally test reads
    /// ([`crate::unit_ai::task_finder::allied`]).
    #[must_use]
    pub const fn treaty(&self) -> Option<&'world TreatyMonitor> {
        self.treaty
    }

    /// No objects but the players' `masters`: a test's reader of masters through the objects'
    /// records, which have no masters of their own.
    #[cfg(test)]
    pub(crate) fn of_masters(masters: &[Arc<Masters>]) -> ObjectsRef<'_> {
        ObjectsRef::new(
            &[],
            ObjectIndex::scanning(),
            masters,
            no_parts(),
            static_roles(),
            tuning(),
            None,
        )
    }

    /// No objects: a test's walk that meets none by id.
    #[cfg(test)]
    pub(crate) fn empty() -> ObjectsRef<'static> {
        ObjectsRef::new(
            &[],
            ObjectIndex::scanning(),
            &[],
            no_parts(),
            static_roles(),
            tuning(),
            None,
        )
    }

    /// Object `id`, live or sleeping.
    #[must_use]
    pub fn get(&self, id: impl ObjectKey) -> Option<&'world Unit> {
        self.index.locate(self.players, id).map(|(_, unit, _)| unit)
    }

    /// Object `id`, live or sleeping, with its owner and its master ([`Unit::master`]); none
    /// for an object the world does not hold or one out for its update.
    ///
    /// # Errors
    ///
    /// A master its owner's table does not hold.
    pub fn with_master(
        &self,
        id: impl ObjectKey,
    ) -> GenieResult<Option<(PlayerId, &'world Unit, &'world Master)>> {
        let Some((player_id, unit, _)) = self.index.locate(self.players, id) else {
            return Ok(None);
        };
        Ok(Some((
            player_id,
            unit,
            unit.master(self.masters, player_id)?,
        )))
    }

    /// What a walk reads of the moving object `id` ([`Unit::gait`]); none for an object without
    /// movement state, one the world does not hold or one out for its update.
    #[must_use]
    pub fn gait(&self, id: impl ObjectKey) -> Option<Gait> {
        let unit = self.get(id)?;
        unit.gait(
            self.actions.of(unit.id),
            self.motions.get(unit.id).and_then(Motion::mover),
        )
    }

    /// The building layer of object `id` ([`crate::world::parts::Parts::building_of`]); none
    /// for an object the world does not hold or one out for its update.
    #[must_use]
    pub fn building(&self, id: impl ObjectKey) -> Option<&'world Building> {
        Some(self.buildings.of(self.get(id)?.id))
    }

    /// The action layer of object `id` ([`crate::world::parts::Parts::actions_of`]); none for
    /// an object the world does not hold or one out for its update.
    #[must_use]
    pub fn actions(&self, id: impl ObjectKey) -> Option<&'world Action> {
        Some(self.actions.of(self.get(id)?.id))
    }

    /// Attribute `attribute` of player `player_id` ([`Player::attribute`]); none for a player
    /// the world does not have, or past its attributes.
    #[must_use]
    pub fn attribute(&self, player_id: PlayerId, attribute: AttributeId) -> Option<Amount> {
        self.players.get(player_id.index())?.attribute(attribute)
    }

    /// Why the load could not read object `id` whole ([`crate::world::Unread`]), none for one it
    /// read. The updating object's too: its entry stays in the table while its row is out.
    #[must_use]
    pub fn unread(&self, id: ObjectId) -> Option<&'world Unread> {
        self.unread.get(id)
    }

    /// The main building of the annex `id` (its `+0xbb8`); none for an object that is no annex,
    /// one the world does not hold or one out for its update.
    #[must_use]
    pub fn annex_of(&self, id: impl ObjectKey) -> Option<MainBuildingId> {
        self.buildings.get(self.get(id)?.id)?.annex_of
    }

    /// The annexes of the building `main` ([`crate::construction::Annexes`]): none for an
    /// object the world does not hold or one out for its update, which reads its own.
    #[must_use]
    pub fn annexes(&self, main: impl ObjectKey) -> Annexes {
        self.get(main)
            .and_then(|unit| self.buildings.get(unit.id))
            .map_or(Annexes::NONE, |building| building.annexes)
    }

    /// The annexes of the building `main` and theirs, each building's in slot order before the
    /// next level's: the objects `FUN_14123d090` finds through the slots `+0xbc0` to `+0xbd8`,
    /// depth first, which the building master's placement scan passes over. None for an object
    /// without annexes.
    #[must_use]
    pub fn annex_tree(&self, main: ObjectId) -> Vec<ObjectId> {
        let mut tree: Vec<ObjectId> = self.annexes(main).ids().collect();
        let mut next = 0;
        while let Some(&annex) = tree.get(next) {
            tree.extend(self.annexes(annex).ids());
            next += 1;
        }
        tree
    }

    /// The obstruction query's ignore list for `ids` (−1 for none), as `FUN_14114e8a0` adds each
    /// object: its id, then a building's annexes (`+0xbc0`..`+0xbd8`, in slot order).
    #[must_use]
    pub fn query_ignore(&self, ids: &[Option<ObjectId>]) -> Vec<RecordId> {
        let mut ignore = Vec::new();
        for &id in ids.iter().flatten() {
            ignore.push(RecordId::from(id));
            ignore.extend(self.annexes(id).ids().map(RecordId::from));
        }
        ignore
    }

    /// Whether object `id` moves now ([`crate::map::spatial::Gait::moving`]).
    #[must_use]
    pub fn moving(&self, id: impl ObjectKey) -> bool {
        self.gait(id).is_some_and(|gait| gait.moving)
    }
}
