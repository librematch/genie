//! A unit entering a building or a transport, and its update inside.

use crate::combat::Firepower;
use crate::combat::powerup::{InfantryChange, count_infantry, speeds_up_siege};
use crate::garrison::ContainerId;
use crate::garrison::EnterOutcome;
use crate::garrison::EnterSlot;
use crate::garrison::EnterState;
use crate::garrison::{
    ENTERING_UNPLACED, EnterAction, EnterReads, INSIDE_SIGHT, NO_MASTERS, WalkStart, attack_rate,
    enter_view, share,
};
use crate::gathering::gather::unit::give_load;
use crate::groups::LeavingWalk;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::spatial::Placed;
use crate::math::rng::Stream;
use crate::movement::Me;
use crate::movement::move_to::MoveDefaults;
use crate::movement::move_to::Patience;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::orders::town_bell::BellShelter;
use crate::orders::town_bell::{BackToWork, TownBell};
use crate::ownership::artifact::ArtifactSub;
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::{allied, drops_at};
use crate::vision::SightCentre;
use crate::vision::SightLayer;
use crate::vision::SightShape;
use crate::vision::VisionBits;
use crate::vision::registry::Entry;
use crate::vision::registry::UnitUpdate;
use crate::vision::visibility::LosSight;
use crate::world::AllySharing;
use crate::world::Seconds;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::objects_ref::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

/// The units inside object `id`, any player's (its slot 0x498).
fn units_inside(others: OthersRef<'_>, id: ObjectId) -> usize {
    others
        .every()
        .filter(|unit| unit.life.inside.key() == Some(id))
        .count()
}

impl Unit {
    /// The Enter's update ([`crate::garrison::EnterAction::update`]) with what it reads as it
    /// starts ([`Unit::enter_reads`]), on the Enter copied out of the action list and put back
    /// after it; its code (1, an Enter that got the unit in and the unit out again before this
    /// update, or a failed one's 3 delete it, its destructor letting its building go). Nothing
    /// without an Enter.
    ///
    /// # Errors
    ///
    /// The reads' and the update's.
    pub(crate) fn enter_part(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let Some(mut enter) = actions.list.enter().copied() else {
            return Ok(HeadCode::Keeps);
        };
        let reads = self.enter_reads(step, &enter, roles)?;
        let updated = enter.update(step, self, actions, me, &reads, roles);
        if let Some(enter_action) = actions.list.enter_mut() {
            *enter_action = enter;
        }
        updated
    }

    /// What the Enter's update reads as it starts: its target, one of its own player's or an
    /// ally's (another player's is not modelled), with room for the unit (its count, the units
    /// of the player inside it, below its master's capacity) and the rest of
    /// [`crate::garrison::enter_view`]; the `MoveTo`'s give-up (a human player's villager waits,
    /// by its unit AI's stance) and the sprites its constructors take for none
    /// (`FUN_140aa0d30`, `FUN_140aa0df0`).
    ///
    /// # Errors
    ///
    /// The target's master's and [`crate::garrison::enter_view`]'s.
    fn enter_reads(
        &self,
        step: &mut Step<'_>,
        enter: &EnterAction,
        roles: &Roles,
    ) -> GenieResult<EnterReads> {
        let by_id = ObjectsRef::new(
            &step.world.players,
            &step.world.index,
            &step.world.model.masters,
            &step.world.parts,
            &step.world.model.roles,
            &step.world.model.tuning,
        );
        let others = OthersRef::of(&step.world.players, &step.world.index, step.player)?;
        let (target, start) = match others.find(enter.target) {
            Some((tp, unit)) => {
                let master = unit.master(
                    MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
                    tp,
                )?;
                let room = units_inside(others, unit.id) < master.garrison.units();
                let target_speed = step.world.top_speed(tp, unit)?;
                let (mut view, start) = enter_view(
                    &mut step.world.spatial,
                    (self, step.world.parts.mover_of(self.id)),
                    (
                        (unit, step.world.parts.mover_of(unit.id), target_speed),
                        master,
                        room,
                    ),
                    enter.task.work_range,
                    by_id,
                )?;
                // FUN_141663dc0 with allies, or Gaia's
                view.allied = tp == PlayerId::GAIA
                    || allied(
                        &step.world.players.player(step.player)?.relations,
                        step.player,
                        tp,
                        AllyTest::WithRelation4,
                    );
                (Some(view), start)
            }
            None => (
                None,
                WalkStart {
                    gap: Distance::ZERO,
                },
            ),
        };
        let master = self.master(step.world.master_tables(), step.player)?;
        let villager = roles.of_class(master.class).contains(ClassRoles::VILLAGER);
        Ok(EnterReads {
            target,
            start,
            patience: if villager && !step.world.players.player(step.player)?.kind.is_computer() {
                Patience::WaitsOnce
            } else {
                Patience::Fails
            },
            stance: step
                .world
                .parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::Aggressive, |ai| ai.stance),
            defaults: MoveDefaults::of(
                master,
                step.world.parts.combat_of(self.id).abilities.look.as_ref(),
                roles,
            ),
        })
    }

    /// The Enter's enter in its update (`FUN_140a867b0` into `target`, [`Unit::enter_building`],
    /// then the update's own `set_state(1)`).
    pub(crate) fn enter_in_update(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        target: ContainerId,
        roles: &Roles,
    ) -> GenieResult<()> {
        let inside = units_inside(
            OthersRef::of(&step.world.players, &step.world.index, step.player)?,
            target.id(),
        );
        if self.enter_building(actions, step, target, inside, roles)? == EnterOutcome::NoRoom {
            return Err(GenieError::unsupported(
                "an Enter that did not get in (state 2)",
            ));
        }
        // the update's own set_state(1) after the enter's
        self.entered(actions, step)
    }

    /// `FUN_140a867b0` on a unit inside its target already (the update's walk again put it in):
    /// notify 0x1fa with the enter action value and its `set_state(1)`, then the update's own
    /// `set_state(1)` ([`Unit::entered`] twice).
    ///
    /// # Errors
    ///
    /// The notify's and the `set_state(1)`'s.
    pub(crate) fn entered_again(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        self.post_own(
            &mut step.world.parts.unit_ai,
            NotifyKind::ACTION_COMPLETED,
            [AiAction::Enter.value(), 0, 0],
        )?;
        for _ in 0..2 {
            self.entered(actions, step)?;
        }
        Ok(())
    }

    /// The Enter's `set_state(1)` (`FUN_140a87320` case 1): its sub-actions go (`FUN_140a98620`: a
    /// `MoveTo`'s teardown stops the unit, slot 0x5d0), the unit shows its master's standing
    /// sprite ([`Unit::shows_standing_sprite`]) and, the Enter not being quiet, its unit AI gets
    /// notify 0x1f9 with the enter action value.
    ///
    /// # Errors
    ///
    /// A unit without its Enter, and the stop's, the sprite's and the notify's.
    pub(crate) fn entered(&mut self, actions: &mut Action, step: &mut Step<'_>) -> GenieResult<()> {
        let enter = actions
            .list
            .enter_mut()
            .ok_or(GenieError::unsupported("an entered unit without its Enter"))?;
        if enter.state.take_move_to().is_some()
            && let Some(mover) = step.world.parts.mover_mut(self.id)
        {
            mover.stop()?;
        }
        enter.state = EnterState::Done;
        self.shows_standing_sprite(
            step.world.parts.motion.layer_mut(self.id),
            OwnerMasters::new(
                &step.world.parts.own_master,
                step.world.model.masters.of(step.player),
            ),
            &step.world.model.graphics,
            step.world.streams.get_mut(Stream::Sprite),
            step.world.model.tuning.sprite,
        )?;
        self.post_own(
            &mut step.world.parts.unit_ai,
            NotifyKind::ACTION_FAILED,
            [AiAction::Enter.value(), 0, 0],
        )
    }

    /// The enter (`FUN_140a867b0`) into `target`, one of the unit's player's buildings: a building
    /// that takes what the unit carries (`FUN_140fdd190`, as a drop site) takes its load first (the
    /// resource for kinds 15 to 17 is food, 0 to 3 and 56 their own, others none: [`give_load`];
    /// the unit carries nothing after, `FUN_14119d310` with −1). A building at no more than a fifth
    /// of its master's hit points (in floats) and a master with a transform task (0x88) on the
    /// building's are not modelled. Then slot 0x1a0 (`FUN_141212e10` -> `FUN_141084b30` ->
    /// `FUN_1411921d0`): with room in the building, the unit's sight goes from its tile (slot
    /// 0x1d8), it leaves its tile's list (`FUN_140f5f560`), the obstruction manager
    /// (`FUN_141055b40`) and the mesh (`+0x660`, `FUN_141053840`), a member of a unit group leaves
    /// it (`FUN_14102c160`, the caller's) and stands (slot 0x560), and it joins the building's
    /// (slot 0x1c0, `FUN_1411cb850` -> `FUN_14118c890`: `+0x2f0`, at the building's x and y); its
    /// records leave the registry's lists (`FUN_14128a1d0` from its mask to 0); the building's
    /// sight takes a circle of 5 (slot 0x1d0 with 5) and the villager's town-bell flag follows the
    /// garrison (`FUN_1412067f0` with 3). Then notify 0x1fa with the enter action value and
    /// `set_state(1)` ([`Unit::entered`]). The building's firepower (`+0xa00`) and villager count
    /// (`+0xba8`) follow from the units inside; the sound, the tracker and a Gaia building's
    /// capture (slot 0x70) are not held. [`EnterOutcome::NoRoom`] without room in the building.
    pub(crate) fn enter_building(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        target: ContainerId,
        inside: usize,
        roles: &Roles,
    ) -> GenieResult<EnterOutcome> {
        // the callers found it: the Enter's update in its reads, `World::enter_now` by its place
        let (tp, building) = step.world.object(target).ok_or(GenieError::unreachable(
            "an enter into a building the world does not hold",
        ))?;
        let own = step.world.model.masters.of(step.player);
        let masters = own.ok_or(NO_MASTERS)?;
        // copies: going inside changes the unit and the world, which hold them
        let bm = building
            .master_in(OwnerMasters::new(
                &step.world.parts.own_master,
                step.world.model.masters.of(tp).or(own),
            ))?
            .clone();
        let hp = building.life.hp;
        let me = self
            .master_in(OwnerMasters::new(
                &step.world.parts.own_master,
                Some(masters),
            ))?
            .clone();
        let owner = step.world.players.player(step.player)?;
        let actor = Actor {
            id: self.id,
            master: &me,
            player: step.player,
            held: self.life.held(),
            attribute_95: Amount::new(
                owner
                    .attributes
                    .get(95)
                    .copied()
                    .unwrap_or(Amount::ZERO)
                    .get(),
            ),
            civ: owner.civ,
        };
        if drops_at(&step.world.model.drop_sites, bm.id, &actor)
            && Amount::ZERO < self.life.amount
            && bm.kind == MasterKind::BUILDING
        {
            // the building takes food for the foods a unit carries (15 to 17), the four resources
            // and the ore themselves, nothing else
            let resource = self.life.attribute_kind.and_then(|kind| {
                if !roles.of_attribute(kind).contains(AttributeRoles::FOOD) && kind.is_food_load() {
                    Some(step.world.model.slots.attribute.food)
                } else {
                    (kind.is_resource() || roles.of_attribute(kind).contains(AttributeRoles::ORE))
                        .then_some(kind)
                }
            });
            if let Some(resource) = resource {
                // the building takes the load: the owner's attributes gain it at once
                let mut adds = Vec::new();
                give_load(
                    (
                        &step.world.spatial.relations,
                        owner.ally_sharing.contains(AllySharing::RESOURCES),
                    ),
                    step.player,
                    &mut adds,
                    resource,
                    self.life.amount,
                    roles,
                    step.world.model.slots,
                )?;
                step.world
                    .players
                    .player_mut(step.player)?
                    .add_attributes(adds)?;
                self.life.attribute_kind = None;
                self.life.amount = Amount::ZERO;
            }
        }
        if bm.kind == MasterKind::BUILDING
            && hp
                <= HitPoints::new(
                    HitPoints::of_master(bm.hit_points).get()
                        * step.world.model.tuning.garrison.enter_hp_share,
                )
        {
            return Err(GenieError::unsupported(
                "an enter into a building at a fifth of its hit points (state 13)",
            ));
        }
        if me
            .action_part()
            .tasks
            .iter()
            .find(|task| task.action_kind == ActionKind::DEPOSIT)
            .is_some_and(|task| task.object_id == Some(bm.id))
        {
            return Err(GenieError::unsupported(
                "an enter of a master that transforms (task 0x88)",
            ));
        }
        if bm.garrison.units() <= inside {
            // slot 0x1a0's room test (`FUN_1411921d0`) keeps the unit out; its tail runs all the
            // same
            self.enter_tail(step, target, &me, &bm, roles)?;
            return Ok(EnterOutcome::NoRoom);
        }
        self.go_inside(actions, step, target, &me, &bm, roles)?;
        self.post_own(
            &mut step.world.parts.unit_ai,
            NotifyKind::ACTION_COMPLETED,
            [AiAction::Enter.value(), 0, 0],
        )?;
        self.entered(actions, step)?;
        Ok(EnterOutcome::Inside)
    }

    /// The unit's slot 0x1a0 into building `target` (`FUN_141212e10`; the room test, slot 0x198,
    /// is the caller's), the unit's master `me` and the building's `bm`: its sight goes from its
    /// tile, it leaves its tile's list, the obstruction manager and the mesh; a member of a unit
    /// group leaves the group when the world holds it (removeUnit, `FUN_14102c160` with the
    /// building and 0, [`crate::world::World::remove_unit`] at once on the unit lent back to its
    /// slot; `FUN_14152ff40`) and stands; it joins the building's (slot 0x1c0: the building's
    /// list and, for a unit of type 70, its volleys' share, [`crate::garrison::share`]); its
    /// records leave the registry's lists; the building's sight takes a circle of 5 and the
    /// villager's town-bell flag follows the garrison and a siege unit counts its infantry
    /// ([`Unit::enter_tail`]). The building's `+0x9a8` (the units inside the unit) is not
    /// modelled.
    pub(crate) fn go_inside(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        target: ContainerId,
        me: &Master,
        bm: &Master,
        roles: &Roles,
    ) -> GenieResult<()> {
        let id = self.id;
        if self.life.inside.is_some() {
            return Err(GenieError::unsupported("an enter of a unit inside another"));
        }
        // the building's `+0x9a8` takes the units inside the unit (slot 0x498)
        if !self.garrison.is_empty() {
            return Err(GenieError::unsupported(
                "an enter of a unit with units inside it",
            ));
        }
        let placed = step
            .world
            .spatial
            .get(id)
            .copied()
            .ok_or(ENTERING_UNPLACED)?;
        if let Some(visibility) = step.world.spatial.sight.as_mut() {
            visibility.sight_off(
                id,
                self.position(),
                placed.radius,
                SightShape::Circle,
                SightCentre::Tile,
                SightLayer {
                    roles,
                    visibility_tuning: step.world.model.tuning.visibility,
                },
            )?;
        }
        // the callers found it (`Unit::enter_building`, `World::enter_now`), and the sight's
        // change removes no object
        let (bp, building) = step.world.object(target).ok_or(GenieError::unreachable(
            "an enter into a building the world does not hold",
        ))?;
        let (at, reload) = (
            building.position(),
            step.world.parts.combat_of(building.id).reload_powerup,
        );
        self.place.go_inside(id, at, &mut step.world.spatial);
        step.world.pathing.removed(id);
        // a member of a unit group (+0x6d0) leaves the group the world holds, then stands
        // (slot 0x560)
        if let Some(group) = self.group.id {
            if step.world.groups.get(group).is_some() {
                step.lend((&mut *self, &mut *actions), |world| {
                    world
                        .remove_unit(group, id, Some(target.id()), LeavingWalk::Stands)
                        .map(drop)
                })?;
            }
            if let Some(mover) = step.world.parts.mover_mut(self.id) {
                mover.stand()?;
            }
        }
        let building_radius = step
            .world
            .spatial
            .get(target)
            .map_or(bm.radius, |target_placed| target_placed.radius);
        self.life.inside = Some(target);
        self.join_building(step, Some(target), bp, reload, me, bm)?;
        self.enter_sight(step, target, at, building_radius, placed, bm)?;
        self.enter_tail(step, target, me, bm, roles)
    }

    /// The end of slot 0x1a0 (`FUN_141212e10`) into building `target`, the unit's master `me` and
    /// the building's `bm`, whether its room test let the unit in or not: the villager's town-bell
    /// flag follows the garrison (`FUN_1412067f0` with 3, [`Unit::enter_town_bell`]), and a siege
    /// unit counts the infantry that enters it ([`speeds_up_siege`]; slot 0x1b0 with 1,
    /// [`count_infantry`]: `+0xad8` and its speed powerup `+0xa2c`). The count's test of the
    /// editor's screen mode (`FUN_140c87f20` not 4) holds in a game.
    fn enter_tail(
        &self,
        step: &mut Step<'_>,
        target: ContainerId,
        me: &Master,
        bm: &Master,
        roles: &Roles,
    ) -> GenieResult<()> {
        let id = self.id;
        let (_, building) = step.world.object(target).ok_or(GenieError::unreachable(
            "an enter into a building the world does not hold",
        ))?;
        let (at, state) = (building.position(), building.life.state);
        let mut bell = step.world.parts.town_bell_of(id);
        let ai = step.world.parts.unit_ai.get(id);
        self.enter_town_bell((&mut bell, ai), Some(target), at, state, (me, bm), roles)?;
        step.world.parts.set_town_bell(id, bell);
        if speeds_up_siege(me, bm, roles) {
            let tuning = step.world.model.tuning.powerup;
            let combat = step.world.parts.combat_mut(target.id())?;
            count_infantry(combat, InfantryChange::Entered, tuning);
        }
        Ok(())
    }

    /// [`Unit::go_inside`]'s slot 0x1c0 of building `target` (`FUN_1411cb850` -> `FUN_14118c890`,
    /// `FUN_14120b790`; its owner `bp`, its reload powerup `reload`): its list takes the unit,
    /// and a unit (type 70) adds its share to the building's volleys.
    fn join_building(
        &self,
        step: &mut Step<'_>,
        target: Option<ContainerId>,
        bp: PlayerId,
        reload: Seconds,
        me: &Master,
        bm: &Master,
    ) -> GenieResult<()> {
        let roles = &step.world.model.roles.clone();
        let masters = step
            .world
            .model
            .masters
            .get(step.player.index())
            .ok_or(NO_MASTERS)?;
        let firepower = if me.kind == MasterKind::COMBAT {
            let own = step.world.model.masters.of(bp).unwrap_or(masters);
            let rate = attack_rate(bm, own, reload, roles, &step.world.model.tuning.garrison);
            share(
                me,
                masters,
                step.world.parts.combat_of(self.id).reload_powerup,
                rate,
                roles,
            )?
        } else {
            Firepower::ZERO
        };
        if let Some((_, unit)) = step.world.object_mut(target) {
            unit.garrison.push(self.id);
            let id = unit.id;
            step.world.parts.combat_mut(id)?.firepower += firepower;
        }
        Ok(())
    }

    /// [`Unit::go_inside`]'s sight: the unit's records leave the registry's lists
    /// (`FUN_14128a1d0` from its mask to 0, `placed` its record as it was) and a building
    /// `target` at `at` (radius `radius`) sees a circle of 5 (slot 0x1d0 with 5).
    fn enter_sight(
        &self,
        step: &mut Step<'_>,
        target: ContainerId,
        at: Position,
        radius: Footprint,
        placed: Placed,
        bm: &Master,
    ) -> GenieResult<()> {
        let id = self.id;
        let Some(visibility) = step.world.spatial.sight.as_mut() else {
            return Ok(());
        };
        if let Some(registry) = visibility.registry.as_mut().filter(|_| 0 <= id.get()) {
            let mask = registry
                .entry(id)
                .map_or(VisionBits::NONE, |entry| entry.mask);
            registry.update_unit(UnitUpdate {
                id,
                owner: step.player,
                position: self.position().ground(),
                radii: placed.radius,
                class: placed.class,
                old: mask & VisionBits::VISIBLE_HALF,
                new: VisionBits::NONE,
            })?;
            // its mask (`+0x8f8`) is none after
            registry
                .entries
                .get_or_insert_with(id, Entry::default)?
                .mask = VisionBits::NONE;
        }
        // a building's sight takes a circle of 5 (FUN_141212e10, type 80 only)
        if bm.kind == MasterKind::BUILDING {
            let _ = visibility.sight_on_los(
                target.id(),
                LosSight::tile_circle(at, radius, INSIDE_SIGHT),
                &step.world.model.roles,
                step.world.model.tuning.visibility,
            )?;
        }
        Ok(())
    }

    /// [`Unit::go_inside`]'s `FUN_1412067f0` with 3, for a villager or master 1808 (`me`; the
    /// building's master `bm`), on its town bell `bell` (the world's part): the town-bell record
    /// of its work (`FUN_141206a20` with the flag clear: the Enter's target
    /// `target` at its place `at`, no target class, the unit AI's action, no second target),
    /// then the flag set for a building in state 2 (`state`; its own player's) that takes
    /// villagers; with 3 it is never cleared.
    fn enter_town_bell(
        &self,
        (bell, ai): (&mut TownBell, Option<&UnitAi>),
        target: Option<ContainerId>,
        at: Position,
        state: ObjectState,
        (me, bm): (&Master, &Master),
        roles: &Roles,
    ) -> GenieResult<()> {
        if !roles.of_class(me.class).contains(ClassRoles::VILLAGER)
            && !roles.of_master(me.id).contains(MasterRoles::MULE_CART)
        {
            return Ok(());
        }
        if !bell.flag.is_sheltered() {
            let action = ai
                .map(|ai| ai.current_action)
                .ok_or(GenieError::unsupported(
                    "a town-bell record of a unit without a unit AI",
                ))?;
            bell.record(BackToWork {
                target: target.map(ContainerId::id),
                target_class: None,
                action,
                target2: None,
                point: at.ground(),
            });
        }
        if bm.kind == MasterKind::BUILDING
            && state == ObjectState::Alive
            && bm.building.is_some_and(|building_master| {
                building_master
                    .garrison_kind
                    .contains(GarrisonKinds::VILLAGERS)
            })
        {
            bell.flag = BellShelter::Sheltered;
        }
        Ok(())
    }
}

impl Unit {
    /// The inside update of an object inside another (`FUN_140a98790`), which runs in place of
    /// its actions' update (`RGE_Action_Object::update`): the head's idle update
    /// (`FUN_140a78480` -> `FUN_140a784a0`: a target or second target past state 6 let go, its
    /// sub-actions' idle update; the head's deletion right after lets them go alike), the idle
    /// update of each action below it, and the head's deletion ([`Unit::end_head_update`] with
    /// its 1). A unit its actions got in this update waits for the next. An empty list takes the
    /// default task instead: livestock inside makes its Artifact in one update and loses it in
    /// the next.
    pub(crate) fn inside_update(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        if !actions.list.is_empty() {
            return self.end_head_update(actions, step, HeadCode::Deletes);
        }
        if step.world.parts.unread.contains(self.id) {
            return Err(GenieError::unsupported(
                "the inside update of an action the slice does not hold",
            ));
        }
        Ok(())
    }
}

impl Action {
    /// The unit's new Enter in its `slot`: its own action, or the sub-action of its Artifact on
    /// top.
    pub(crate) fn place_enter(&mut self, action: EnterAction, slot: EnterSlot) {
        match self
            .list
            .artifact_mut()
            .filter(|_| slot == EnterSlot::UnderArtifact)
        {
            Some(art) => {
                art.sub = Some(ArtifactSub::Enter { enter: action });
            }
            None => self.list.set_enter(Some(action)),
        }
    }
}
