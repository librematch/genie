//! The construction's parts that run on the whole world ([`crate::world::World`]), outside a
//! builder's update: a building's work and completion, its storage's leave, the built object's
//! stacking and its new objects.

use crate::commands::GatherPoint;
use crate::commands::GatherPointState;
use crate::construction::BuildProgress;
use crate::construction::ConstructionStart;
use crate::construction::FoundationId;
use crate::construction::build::Clearer;
use crate::construction::build::FarmBonusGain;
use crate::construction::build::{Completion, Construction, StackUnit};
use crate::gathering::PastureId;
use crate::map::Position;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::lifecycle::death_parts::{
    farm_bonus, storage_leaves as death_parts_storage_leaves,
};
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::{Master, player_master};
use crate::objects::unit::Unit;
use crate::orders::lock_gate;
use crate::players::PlayerId;
use crate::players::ResourceStat;
use crate::players::world_player::ResourceShare;
use crate::production::spawn::Facing;
use crate::unit_ai::QueuedOrder;
use crate::unit_ai::unit_ai_kind;
use crate::vision::SightLayer;
use crate::world::Seconds;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::parts::Part;
use crate::world::slots::PlayerSlotsExt;
use crate::world::{Player, World};
use crate::{GenieError, GenieResult};

/// How the player's slot 0x1c0 (`FUN_14162ce00`) makes an object: finished (its flag: the
/// footprint cleared, a building worked at once) or plain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MakeMode {
    /// Finished.
    Finished,
    /// Plain.
    Plain,
}

impl World {
    /// What a built building's storage gave its owner leaves (`FUN_1411e63f0`, then
    /// `FUN_1411e6580`, [`crate::objects::lifecycle::death_parts::storage_leaves`]) outside its
    /// update, each through its owner's slot 0xf8 with 0 and 1 ([`World::add_player_attribute`]).
    ///
    /// # Errors
    ///
    /// An object the slice does not hold, and its own store.
    pub(crate) fn storage_leaves(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        master: &Master,
    ) -> GenieResult<()> {
        let (roles, slots) = (self.model.roles.clone(), self.model.slots);
        let held = self.object_and_combat_mut(id)?.ok_or(NOT_HELD)?;
        for (kind, amount) in death_parts_storage_leaves(held, master, &roles, slots) {
            self.add_player_attribute(owner, kind, amount)?;
        }
        Ok(())
    }

    /// The completion's first part (`FUN_1411c6bd0`, [`Completion::first_part`]) of the
    /// foundation `id` of `owner` outside its update: a farm owes its bonus to its Make
    /// (`+0xcac`, [`Building::owes_bonus`]) and its owner gains it, a pasture fills
    /// ([`crate::world::World::setup_pasture`]), then a mule cart's search runs from state 0
    /// ([`crate::world::World::cart_search`]).
    ///
    /// [`Building::owes_bonus`]: crate::objects::layers::Building::owes_bonus
    ///
    /// # Errors
    ///
    /// What the part does not model.
    pub(crate) fn completion_part(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        master: &Master,
    ) -> GenieResult<()> {
        let (_, unit) = self.object(id).ok_or(NOT_HELD)?;
        let construction = self
            .parts
            .building_of(unit.id)
            .construction
            .ok_or(NO_CONSTRUCTION)?;
        let completion = Completion {
            master: Some(master),
            farm_bonus: farm_bonus(&self.players.player(owner)?.attributes, self.model.slots),
            masters: self.model.masters.of(owner),
            player_kind: self.players.player(owner)?.kind,
            ai: self.parts.unit_ai.contains(unit.id),
        };
        let part = completion.first_part(&construction, &self.model.roles)?;
        // its tracker part (FUN_14104eac0): the foundation leaves the pending lists
        self.track_complete(owner, id, master.id)?;
        if let Some(gain) = part.farm_bonus {
            self.give_farm_bonus(owner, id, gain)?;
        }
        if part.pasture {
            self.setup_pasture(owner, PastureId::new(id))?;
        }
        if part.cart {
            self.cart_search(owner, id, ObjectState::Foundation)?;
        }
        Ok(())
    }

    /// `FUN_1411c66a0`'s farm bonus ([`FirstPart::farm_bonus`]) of the building `id` of
    /// `owner`: the farm owes it to its Make (`+0xcac`, [`Building::owes_bonus`]), then its owner
    /// gains what `gain` names (slot 0xf8 with 1 and 1, [`World::add_resource`]: its gathered
    /// total, its escrow and its sharing too), nothing for an attribute its add does not hold
    /// ([`FarmBonusGain::Nothing`]).
    ///
    /// [`FirstPart::farm_bonus`]: crate::construction::build::FirstPart::farm_bonus
    /// [`Building::owes_bonus`]: crate::objects::layers::Building::owes_bonus
    ///
    /// # Errors
    ///
    /// A building the world does not hold, and the add's.
    pub(crate) fn give_farm_bonus(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        gain: FarmBonusGain,
    ) -> GenieResult<()> {
        self.parts.building_mut(id)?.owes_bonus = true;
        match gain {
            FarmBonusGain::Attribute(kind, amount) => self.add_resource(
                owner,
                kind,
                amount,
                ResourceStat::Gathered,
                ResourceShare::Shared,
            ),
            FarmBonusGain::Nothing => Ok(()),
        }
    }

    /// The construction's start (`FUN_1411e5670`, [`Construction::begin`]) of the foundation
    /// `id` of `owner`: a desolid one starts, its own master copy goes (`+0x6c0`, the master its
    /// player's again), its missiles per volley are its master's then, and its owner watches it
    /// at once (`FUN_141090430`, [`crate::world::World::watch_started`]).
    ///
    /// # Errors
    ///
    /// What the start does not model.
    pub(crate) fn construction_starts(&mut self, owner: PlayerId, id: ObjectId) -> GenieResult<()> {
        let place = self.place(id).ok_or(NOT_HELD)?;
        let (Place::Live(player_id, _) | Place::Sleeping(player_id, _)) = place;
        let now = self.world_time;
        let unit = held(&mut self.players, place)?;
        let (layer, mover) = self.parts.building_and_mover_mut(unit.id)?;
        let Some(construction) = layer.construction.as_mut() else {
            return Ok(());
        };
        if Construction::begin(
            id,
            construction,
            mover,
            &mut self.spatial,
            &mut self.pathing,
            now,
            SightLayer {
                roles: &self.model.roles,
                visibility_tuning: self.model.tuning.visibility,
            },
        )? == ConstructionStart::Begun
        {
            // its master is its owner's again: the desolid copy goes
            self.set_part(id, Part::OwnMaster(None))?;
            let volley = held(&mut self.players, place)?
                .master(
                    MasterTables::new(&self.model.masters, &self.parts.own_master),
                    player_id,
                )?
                .creatable_part()
                .volley_fire;
            self.parts.combat_mut(id)?.firepower = volley;
            self.watch_started(owner, id)?;
        }
        Ok(())
    }
}

/// The guard of an object the routines here reach through the object itself (their callers
/// hold it): the slice's lists hold every object the world does.
pub(crate) const NOT_HELD: GenieError =
    GenieError::unsupported("an object the slice does not hold");

/// The guard of a building without its construction: the slice gives every building one.
pub(crate) const NO_CONSTRUCTION: GenieError =
    GenieError::unsupported("a building without its construction");

/// The object at `place`, in its player's live or sleeping list (the world's table,
/// `FUN_1415417a0`, holds both).
///
/// # Errors
///
/// A place its player's lists do not hold.
pub(crate) fn held(players: &mut [Player], place: Place) -> GenieResult<&mut Unit> {
    match place {
        Place::Live(owner, slot) => players.live_mut(owner, slot),
        Place::Sleeping(player_id, list_index) => players.asleep_mut(player_id, list_index),
    }
}

impl World {
    /// `FUN_1411cc430` outside a builder's update, `amount` seconds on the building `id`: first
    /// each annex (`+0xbc0`..`+0xbd8`) of a building type that is desolid starts (`FUN_1411cd0d0`,
    /// [`crate::world::World::clear_foundations_under`], then `FUN_1411e5670`,
    /// [`crate::world::World::construction_starts`]), then while the building is an annex of a main
    /// building in state 0 with at least 1 hit point (COMISS: a NaN too) the work goes to the main
    /// building, whose annexes start alike. A building in state 0 with at least 1 hit point (a NaN
    /// not) starts if desolid and takes the work ([`Construction::add_work`]); one that reaches its
    /// build time completes (`set_state(2)`, [`crate::world::World::set_state`]).
    /// [`BuildProgress::Ongoing`] while its construction goes on (the routine's 0):
    /// [`BuildProgress::Ended`] for one done before the work (not in state 0, or below 1 hit point)
    /// and one the work completed.
    ///
    /// # Errors
    ///
    /// What the starts and the completion do not model.
    pub(crate) fn work_building(
        &mut self,
        id: FoundationId,
        amount: Seconds,
    ) -> GenieResult<BuildProgress> {
        let mut at = id.id();
        loop {
            for annex in self.objects().annexes(at).ids() {
                let Some((annex_player_id, annex_unit)) = self.object(annex) else {
                    continue;
                };
                let desolid = self
                    .parts
                    .building_of(annex_unit.id)
                    .construction
                    .is_some_and(|construction| construction.desolid);
                if annex_unit
                    .master(self.master_tables(), annex_player_id)?
                    .kind
                    < MasterKind::BUILDING
                    || !desolid
                {
                    continue;
                }
                self.clear_foundations_under(annex)?;
                self.construction_starts(annex_player_id, annex)?;
            }
            let Some(main) = self
                .object(at)
                .and_then(|(_, unit)| self.parts.building_of(unit.id).annex_of)
            else {
                break;
            };
            match self.object(main) {
                Some((_, unit))
                    if unit.life.state == ObjectState::Foundation
                        && at_most_or_unordered(1.0, unit.life.hp.get()) =>
                {
                    at = main.id();
                }
                _ => break,
            }
        }
        let (at_player_id, at_unit) = self.object(at).ok_or(NOT_HELD)?;
        if at_unit.life.state != ObjectState::Foundation
            || below_or_unordered(at_unit.life.hp.get(), 1.0)
        {
            return Ok(BuildProgress::Ended);
        }
        if self
            .parts
            .building_of(at_unit.id)
            .construction
            .is_some_and(|construction| construction.desolid)
        {
            self.clear_foundations_under(at)?;
            self.construction_starts(at_player_id, at)?;
        }
        let player_id = self.object(at).ok_or(NOT_HELD)?.0;
        let (unit, layer) = self.object_and_layer_mut(at)?.ok_or(NOT_HELD)?;
        let construction = layer.construction.as_mut().ok_or(NO_CONSTRUCTION)?;
        if construction.add_work(&mut unit.life.hp, amount) == BuildProgress::Ongoing {
            return Ok(BuildProgress::Ongoing);
        }
        self.set_state(player_id, at, ObjectState::Alive)?;
        Ok(BuildProgress::Ended)
    }

    /// The stack unit (`+0x986`) of the completed building `building` of player `owner`
    /// (`FUN_1411dbb70`'s block): its owner's slot 0x1c0 makes it at the building's place with
    /// its flag ([`crate::world::World::make_new_object`]); a new building takes the old one's
    /// command queue (the old unit AI's slot 0x218, its orders not empty, then slot 0x1f8: the
    /// new one's slot 0x208 per order, `FUN_140c15d00`) and its gather point (`+0xc30`..: a
    /// target that is the old building names the new one; a target the world no longer holds
    /// keeps the point alone, and with x or y −1 none); then the new object loses the hit points
    /// the old one lacked. Not modelled: the attacking units of the foundation the world still
    /// holds (`+0xce0`, [`crate::objects::layers::Building::attacking_units`]: slot 0x140 on each with the new
    /// object, a repeat of the last skipped). A gate's stack gate takes the old one's lock (slot
    /// 0x670 with `+0xc7c`, [`crate::orders::lock_gate`]). The old building keeps the new
    /// object's id (`+0xcb0`, [`crate::objects::layers::Building::stack_made`]). Not held: the local player's
    /// tracker (`FUN_14165c2a0`, `FUN_14165c470`).
    ///
    /// # Errors
    ///
    /// What the parts do not model.
    pub(crate) fn stack_unit(
        &mut self,
        owner: PlayerId,
        building: ObjectId,
        stack: StackUnit,
    ) -> GenieResult<()> {
        let (_, old) = self.object(building).ok_or(NOT_HELD)?;
        let (orders, gather) = (
            self.parts
                .unit_ai
                .get(old.id)
                .map(|ai| ai.building_orders.clone())
                .unwrap_or_default(),
            self.parts.gather_point.get(building).copied(),
        );
        let old_class = old.master(self.master_tables(), owner)?.class;
        let old_lock = self.parts.building_of(old.id).gate.map(|gate| gate.locked);
        // its attacking units (+0xce0) the world still holds each hear of the new object (their
        // slot 0x140), which is not modelled
        let enemies = self
            .parts
            .building_of(old.id)
            .attacking_units
            .iter()
            .any(|&id| self.object(id).is_some());
        if enemies {
            return Err(GenieError::unsupported(
                "a stack unit's completion with other players' units on it (+0xce0)",
            ));
        }
        let Some(new) = self.make_new_object(owner, stack.master, stack.at, MakeMode::Finished)?
        else {
            return Ok(());
        };
        // the old building keeps the new object's id (`+0xcb0`, FUN_1411c5f70)
        if self.object(building).is_some() {
            self.parts.building_mut(building)?.stack_made = Some(new);
        }
        let (player_id, nu) = self.object(new).ok_or(NOT_HELD)?;
        let new_master = nu.master(self.master_tables(), player_id)?;
        let mut point = None;
        let building_made = new_master.kind == MasterKind::BUILDING;
        let gate_lock = (building_made
            && self
                .model
                .roles
                .of_class(old_class)
                .contains(ClassRoles::GATE)
            && self
                .model
                .roles
                .of_class(new_master.class)
                .contains(ClassRoles::GATE))
        .then(|| {
            // the constructor and the save give every building its gate state
            old_lock.ok_or(GenieError::unreachable(
                "a building without its gate state (every building holds one)",
            ))
        })
        .transpose()?;
        if building_made {
            // a gather point on the old building names the new one; one whose target the world
            // no longer holds keeps its point, none at x or y −1 (compared as floats)
            point = gather.map(|gather_point| {
                let target = if gather_point.target == Some(building) {
                    Some(new)
                } else {
                    gather_point.target
                };
                match self.object(target) {
                    Some((_, unit)) => Some(GatherPoint {
                        state: GatherPointState::Point,
                        point: Position::new(unit.position().x(), unit.position().y(), 0.0),
                        target,
                        target_master: Some(unit.life.master_id),
                    }),
                    None => (gather_point.point.x() != -1.0 && gather_point.point.y() != -1.0)
                        .then_some(GatherPoint {
                            state: GatherPointState::Point,
                            point: Position::new(
                                gather_point.point.x(),
                                gather_point.point.y(),
                                0.0,
                            ),
                            target: None,
                            target_master: None,
                        }),
                }
            });
        }
        if let Some(gather_point) = point.filter(|_| building_made) {
            self.set_part(new, Part::GatherPoint(gather_point))?;
        }
        self.stack_takes_over(new, building_made, (orders, gate_lock))?;
        let (_, nu) = self.object_mut(new).ok_or(NOT_HELD)?;
        nu.life.hp -= stack.hp_lost;
        Ok(())
    }

    /// What the new object `new` of a stack unit takes from the old building
    /// ([`World::stack_unit`]): a new building (`building_made`) the command queue `orders` (not
    /// empty, into its unit AI), a stack gate the old gate's lock `gate_lock` (its slot 0x670
    /// with `+0xc7c`, [`crate::orders::lock_gate`]).
    ///
    /// # Errors
    ///
    /// A new object the world does not hold.
    fn stack_takes_over(
        &mut self,
        new: ObjectId,
        building_made: bool,
        (orders, gate_lock): (Vec<QueuedOrder>, Option<bool>),
    ) -> GenieResult<()> {
        let now = self.world_time;
        if building_made
            && let Some(ai) = self
                .parts
                .unit_ai
                .get_mut(new)
                .filter(|_| !orders.is_empty())
        {
            ai.building_orders.extend(orders);
        }
        // a gate's stack gate takes the old one's lock (its slot 0x670 with `+0xc7c`)
        if let Some(locked) = gate_lock {
            if self.place(new).is_none() {
                return Err(NOT_HELD);
            }
            let (layer, mover) = self.parts.building_and_mover_mut(new)?;
            lock_gate((mover, layer), locked, now)?;
        }
        Ok(())
    }

    /// The player's slot 0x1c0 (`FUN_14162ce00`) for `master` at `at`: the master's slot 0x40 makes
    /// the object (none for a master the player has not got, `FUN_141666a60`); with its flag
    /// ([`MakeMode::Finished`]) its footprint is cleared with `param_6`
    /// ([`crate::world::World::clear_footprint`]) and a building is worked 10000 seconds at once
    /// ([`crate::world::World::work_building`]: a stack unit completes there); a master that
    /// gathers by itself joins the visible-resource lists
    /// ([`crate::world::World::made_new_object`]). The editor's part (mode 4) never runs in a game.
    /// Its id.
    ///
    /// # Errors
    ///
    /// What the parts do not model ([`crate::world::World::combat_object`],
    /// [`crate::world::World::plain_object`], [`crate::world::World::make_building`]).
    pub(crate) fn make_new_object(
        &mut self,
        player_id: PlayerId,
        master: MasterId,
        at: Position,
        mode: MakeMode,
    ) -> GenieResult<Option<ObjectId>> {
        let Some(made) = player_master(&self.model.masters, player_id, master).cloned() else {
            return Ok(None);
        };
        // a combat unit (type 70, an effect's spawn): the master's slot 0x40 there, its footprint
        // cleared with the flag, its visible resources for one that gathers by itself
        if made.kind == MasterKind::COMBAT {
            let idx = self.combat_object(player_id, master, &made, at)?;
            let id = self.players.live(player_id, idx)?.id;
            if mode == MakeMode::Finished {
                self.clear_footprint(player_id, master, at.ground(), Clearer::Builder)?;
            }
            if made.auto_gather.is_on() {
                self.made_new_object(player_id)?;
            }
            return Ok(Some(id));
        }
        // a plain object (types 10 and 20 to 39: a pasture's livestock): the master's slot 0x40
        // there, facing its drawn angle
        if made.kind != MasterKind::BUILDING {
            let idx = self.plain_object(player_id, master, &made, at, Facing::Drawn)?;
            let id = self.players.live(player_id, idx)?.id;
            if mode == MakeMode::Finished {
                self.clear_footprint(player_id, master, at.ground(), Clearer::Builder)?;
            }
            if made.auto_gather.is_on() {
                self.made_new_object(player_id)?;
            }
            return Ok(Some(id));
        }
        let ai = Ok(unit_ai_kind(&made, master, &self.model.roles));
        let (id, _) = self.make_building(player_id, master, ai, at)?;
        if mode == MakeMode::Finished {
            self.clear_footprint(player_id, master, at.ground(), Clearer::Builder)?;
            // the building made on a flagged stack is the foundation its work builds
            let _ = self.work_building(
                FoundationId::new(id),
                Seconds::new(self.model.tuning.build.stack_work),
            )?;
        }
        if made.auto_gather.is_on() {
            if self
                .players
                .player(player_id)?
                .objects
                .last()
                .map(|unit| unit.id)
                != Some(id)
            {
                return Err(GenieError::unsupported(
                    "an auto-gathered new building with annexes (its visible resources)",
                ));
            }
            self.made_new_object(player_id)?;
        }
        Ok(Some(id))
    }
}
