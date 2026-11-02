//! The construction's parts that run on the whole world ([`crate::world::World`]), outside a
//! builder's update: a building's work and completion, its storage's leave, the built object's
//! stacking and its new objects.

use crate::GenieError;
use crate::commands::GatherPointState;
use crate::construction::build::{Completion, Construction, StackUnit};
use crate::map::Position;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::masters::{Master, player_master};
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::index::Place;
use crate::world::slots::PlayerSlots;

impl crate::world::World {
    /// What a built building's storage gave its owner leaves (`FUN_1411e63f0`, then
    /// `FUN_1411e6580`, [`crate::objects::lifecycle::death_parts::storage_leaves`]) outside its update.
    ///
    /// # Errors
    ///
    /// An object the slice does not hold, its own store, and an attribute the owner lacks.
    pub(crate) fn storage_leaves(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        master: &Master,
    ) -> Result<(), GenieError> {
        let held = self.object_and_combat_mut(id)?.ok_or(NOT_HELD)?;
        for (kind, amount) in crate::objects::lifecycle::death_parts::storage_leaves(held, master) {
            self.players
                .player_mut(owner)?
                .add_attribute(kind, amount)?;
        }
        Ok(())
    }

    /// The completion's first part (`FUN_1411c6bd0`, [`Completion::first_part`]) of the
    /// foundation `id` of `owner` outside its update: its owner gains its attributes, a pasture
    /// fills ([`crate::world::World::setup_pasture`]), then a mule cart's search runs from state 0
    /// ([`crate::world::World::cart_search`]).
    ///
    /// # Errors
    ///
    /// What the part does not model.
    pub(crate) fn completion_part(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        master: &Master,
    ) -> Result<(), GenieError> {
        let (_, u) = self.object(id).ok_or(NOT_HELD)?;
        let construction = self
            .parts
            .building_of(u.id)
            .construction
            .ok_or(NO_CONSTRUCTION)?;
        let completion = Completion {
            master: Some(master),
            farm_bonus: crate::objects::lifecycle::death_parts::farm_bonus(
                &self.players.player(owner)?.attributes,
            ),
            masters: self.masters.of(owner),
            player_type: self.players.player(owner)?.kind,
            ai: self.parts.unit_ai.contains(u.id),
        };
        let part = completion.first_part(&construction)?;
        // its tracker part (FUN_14104eac0): the foundation leaves the pending lists
        self.track_complete(owner, id, master.id)?;
        for (kind, amount) in part.attributes {
            self.players
                .player_mut(owner)?
                .add_attribute(kind, amount)?;
        }
        if part.pasture {
            self.setup_pasture(owner, crate::gathering::PastureId::new(id))?;
        }
        if part.cart {
            self.cart_search(owner, id, ObjectState::Foundation)?;
        }
        Ok(())
    }

    /// The construction's start (`FUN_1411e5670`, [`Construction::begin`]) of the foundation
    /// `id` of `owner`: a desolid one starts, its missiles per volley are its master's (its own
    /// first, `+0xa0`), and its owner watches it at once (`FUN_141090430`,
    /// [`crate::world::World::watch_started`]).
    ///
    /// # Errors
    ///
    /// What the start does not model.
    pub(crate) fn construction_starts(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
    ) -> Result<(), GenieError> {
        let place = self.place(id).ok_or(NOT_HELD)?;
        let (Place::Live(p, _) | Place::Sleeping(p, _)) = place;
        let volley = held(&mut self.players, place)?
            .master(
                crate::objects::masters::MasterTables::new(&self.masters, &self.parts.own_master),
                p,
            )?
            .volley_fire;
        let now = self.world_time;
        let u = held(&mut self.players, place)?;
        let (layer, mover) = self.parts.building_and_mover_mut(u.id)?;
        let Some(c) = layer.construction.as_mut() else {
            return Ok(());
        };
        if Construction::begin(id, c, mover, &mut self.spatial, &mut self.pathing, now)? {
            self.parts.combat_mut(u.id)?.firepower = volley;
            self.watch_started(owner, id)?;
        }
        Ok(())
    }
}

/// The seconds of work a new object's building gets at once from its owner's slot 0x1c0 with its
/// flag (`FUN_14162ce00`'s `FUN_1411cc430` with `DAT_142eea788`).
pub(crate) const STACK_WORK: Seconds = Seconds::new(crate::math::image::build::STACK_WORK);

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
pub(crate) fn held(
    players: &mut [crate::world::Player],
    place: Place,
) -> Result<&mut crate::objects::unit::Unit, GenieError> {
    match place {
        Place::Live(p, i) => players.live_mut(p, i),
        Place::Sleeping(p, i) => players.asleep_mut(p, i),
    }
}

impl crate::world::World {
    /// `FUN_1411cc430` outside a builder's update, `amount` seconds on the building `id`: first
    /// each annex (`+0xbc0`..`+0xbd8`) of a building type that is desolid starts
    /// (`FUN_1411cd0d0`, [`crate::world::World::clear_foundations_under`], then `FUN_1411e5670`,
    /// [`crate::world::World::construction_starts`]), then while the building is an annex of a
    /// main building in state 0 with at least 1 hit point (COMISS: a NaN too) the work goes to
    /// the main building, whose annexes start alike. A building in state 0 with at least 1 hit
    /// point (a NaN not) starts if desolid and takes the work ([`Construction::add_work`]); one
    /// that reaches its build time completes (`set_state(2)`, [`crate::world::World::set_state`]).
    /// True while its construction goes on (the routine's 0): false for one done before the work
    /// (not in state 0, or below 1 hit point) and one the work completed.
    ///
    /// # Errors
    ///
    /// What the starts and the completion do not model.
    pub(crate) fn work_building(
        &mut self,
        id: crate::construction::FoundationId,
        amount: Seconds,
    ) -> Result<bool, GenieError> {
        let mut at = id.id();
        loop {
            for annex in self.objects().annexes(at).ids() {
                let Some((q, a)) = self.object(annex) else {
                    continue;
                };
                let desolid = self
                    .parts
                    .building_of(a.id)
                    .construction
                    .is_some_and(|c| c.desolid);
                if a.master(self.master_tables(), q)?.kind < BUILDING || !desolid {
                    continue;
                }
                self.clear_foundations_under(annex)?;
                self.construction_starts(q, annex)?;
            }
            let Some(main) = self
                .object(at)
                .and_then(|(_, u)| self.parts.building_of(u.id).annex_of)
            else {
                break;
            };
            match self.object(main) {
                Some((_, m))
                    if m.life.state == ObjectState::Foundation
                        && crate::math::fmath::at_most_or_unordered(1.0, m.life.hp.get()) =>
                {
                    at = main.id();
                }
                _ => break,
            }
        }
        let (q, u) = self.object(at).ok_or(NOT_HELD)?;
        if u.life.state != ObjectState::Foundation
            || crate::math::fmath::below_or_unordered(u.life.hp.get(), 1.0)
        {
            return Ok(false);
        }
        if self
            .parts
            .building_of(u.id)
            .construction
            .is_some_and(|c| c.desolid)
        {
            self.clear_foundations_under(at)?;
            self.construction_starts(q, at)?;
        }
        let q = self.object(at).ok_or(NOT_HELD)?.0;
        let (u, layer) = self.object_and_layer_mut(at)?.ok_or(NOT_HELD)?;
        let c = layer.construction.as_mut().ok_or(NO_CONSTRUCTION)?;
        if !c.add_work(&mut u.life.hp, amount) {
            return Ok(true);
        }
        self.set_state(q, at, ObjectState::Alive)?;
        Ok(false)
    }

    /// The stack unit (`+0x986`) of the completed building `building` of player `p`
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
        p: PlayerId,
        building: ObjectId,
        stack: StackUnit,
    ) -> Result<(), GenieError> {
        let (_, old) = self.object(building).ok_or(NOT_HELD)?;
        let (orders, gather) = (
            self.parts
                .unit_ai
                .get(old.id)
                .map(|ai| ai.building_orders.clone())
                .unwrap_or_default(),
            self.parts.gather_point.get(building).copied(),
        );
        let old_class = old.master(self.master_tables(), p)?.class;
        let old_lock = self.parts.building_of(old.id).gate.map(|g| g.locked);
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
        let Some(new) = self.make_new_object(p, stack.master, stack.at, true)? else {
            return Ok(());
        };
        // the old building keeps the new object's id (`+0xcb0`, FUN_1411c5f70)
        if self.object(building).is_some() {
            self.parts.building_mut(building)?.stack_made = Some(new);
        }
        let (q, nu) = self.object(new).ok_or(NOT_HELD)?;
        let new_master = nu.master(self.master_tables(), q)?;
        let mut point = None;
        let building_made = new_master.kind == BUILDING;
        let gate_lock =
            (building_made && old_class == class::GATE && new_master.class == class::GATE)
                .then(|| {
                    old_lock.ok_or(GenieError::unreachable(
                        "a building without its gate state (every building holds one)",
                    ))
                })
                .transpose()?;
        if building_made {
            // a gather point on the old building names the new one; one whose target the world
            // no longer holds keeps its point, none at x or y −1 (compared as floats)
            point = gather.map(|g| {
                let target = if g.target == Some(building) {
                    Some(new)
                } else {
                    g.target
                };
                match self.object(target) {
                    Some((_, t)) => Some(crate::commands::GatherPoint {
                        state: GatherPointState::POINT,
                        point: Position::new(t.position().x(), t.position().y(), 0.0),
                        target,
                        target_master: Some(t.life.master_id),
                    }),
                    None => (g.point.x() != -1.0 && g.point.y() != -1.0).then_some(
                        crate::commands::GatherPoint {
                            state: GatherPointState::POINT,
                            point: Position::new(g.point.x(), g.point.y(), 0.0),
                            target: None,
                            target_master: None,
                        },
                    ),
                }
            });
        }
        if let Some(g) = point.filter(|_| building_made) {
            self.set_part(new, crate::world::parts::Part::GatherPoint(g))?;
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
        (orders, gate_lock): (Vec<crate::unit_ai::QueuedOrder>, Option<bool>),
    ) -> Result<(), GenieError> {
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
            crate::orders::lock_gate((mover, layer), locked, now)?;
        }
        Ok(())
    }

    /// The player's slot 0x1c0 (`FUN_14162ce00`) for `master` at `at`: the master's slot 0x40
    /// makes the object (none for a master the player has not got, `FUN_141666a60`); with its
    /// flag its footprint is cleared with `param_6` ([`crate::world::World::clear_footprint`]) and
    /// a building is worked 10000 seconds at once ([`crate::world::World::work_building`]: a stack
    /// unit completes there); a master that gathers by itself joins the visible-resource lists
    /// ([`crate::world::World::made_new_object`]). The editor's part (mode 4) never runs in a game.
    /// Its id.
    ///
    /// # Errors
    ///
    /// What the parts do not model ([`crate::world::World::combat_object`],
    /// [`crate::world::World::plain_object`], [`crate::world::World::make_building`]).
    pub(crate) fn make_new_object(
        &mut self,
        p: PlayerId,
        master: MasterId,
        at: Position,
        flagged: bool,
    ) -> Result<Option<ObjectId>, GenieError> {
        let Some(made) = player_master(&self.masters, p, master).cloned() else {
            return Ok(None);
        };
        // a combat unit (type 70, an effect's spawn): the master's slot 0x40 there, its footprint
        // cleared with the flag, its visible resources for one that gathers by itself
        if made.kind == COMBAT {
            let idx = self.combat_object(p, master, &made, at)?;
            let id = self.players.live(p, idx)?.id;
            if flagged {
                self.clear_footprint(p, master, at.ground(), true)?;
            }
            if made.auto_gather {
                self.made_new_object(p)?;
            }
            return Ok(Some(id));
        }
        // a plain object (types 10 and 20 to 39: a pasture's livestock): the master's slot 0x40
        // there, facing its drawn angle
        if made.kind != BUILDING {
            let idx = self.plain_object(
                p,
                master,
                &made,
                at,
                crate::production::spawn::Facing::Drawn,
            )?;
            let id = self.players.live(p, idx)?.id;
            if flagged {
                self.clear_footprint(p, master, at.ground(), true)?;
            }
            if made.auto_gather {
                self.made_new_object(p)?;
            }
            return Ok(Some(id));
        }
        let ai = Ok(made.unit_ai_type(master));
        let (id, _) = self.make_building(p, master, ai, at)?;
        if flagged {
            self.clear_footprint(p, master, at.ground(), true)?;
            // the building made on a flagged stack is the foundation its work builds
            let _ = self.work_building(crate::construction::FoundationId::new(id), STACK_WORK)?;
        }
        if made.auto_gather {
            if self.players.player(p)?.objects.last().map(|u| u.id) != Some(id) {
                return Err(GenieError::unsupported(
                    "an auto-gathered new building with annexes (its visible resources)",
                ));
            }
            self.made_new_object(p)?;
        }
        Ok(Some(id))
    }
}
