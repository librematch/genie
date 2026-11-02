//! A unit entering a building or a transport, and the transport's sail.

use crate::GenieError;
use crate::garrison::ContainerId;
use crate::garrison::EnterState;
use crate::gathering::gather::unit::give_load;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::rng::{Stream, Well512};
use crate::movement::Me;
use crate::movement::move_to::MoveDefaults;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::layers::MotionMut;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::Master;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::masters::ids::MASTER_1808;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::pathing::Pathing;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::unit_ai::ActionKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::Actor;
use crate::vision::VisionBits;
use crate::vision::registry::UnitUpdate;
use crate::world::Seconds;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

/// The units inside object `id`, any player's (its slot 0x498).
fn units_inside(others: OthersRef<'_>, id: ObjectId) -> usize {
    others
        .every()
        .filter(|u| u.life.inside.key() == Some(id))
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
    pub(crate) fn enter_part(&mut self, c: &mut Step<'_>, me: Me) -> Result<HeadCode, GenieError> {
        let Some(mut enter) = self.actions.list.enter().copied() else {
            return Ok(HeadCode::Keeps);
        };
        let reads = self.enter_reads(c, &enter)?;
        let updated = enter.update(c, self, me, &reads);
        if let Some(e) = self.actions.list.enter_mut() {
            *e = enter;
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
        c: &mut Step<'_>,
        enter: &crate::garrison::EnterAction,
    ) -> Result<crate::garrison::EnterReads, GenieError> {
        let by_id = crate::world::index::ObjectsRef::new(
            &c.w.players,
            &c.w.index,
            &c.w.masters,
            &c.w.parts,
        );
        let others = OthersRef::of(&c.w.players, &c.w.index, c.player)?;
        let (target, start) = match others.find(enter.target) {
            Some((tp, t)) => {
                let master = t.master(
                    crate::objects::masters::MasterTables::new(&c.w.masters, &c.w.parts.own_master),
                    tp,
                )?;
                let room = units_inside(others, t.id) < master.garrison.units();
                let (mut view, start) = crate::garrison::enter_view(
                    &mut c.w.spatial,
                    &c.w.masters,
                    (self, c.w.parts.mover_of(self.id)),
                    ((t, c.w.parts.mover_of(t.id)), master, room),
                    enter.range,
                    by_id,
                )?;
                // FUN_141663dc0 with allies, or Gaia's
                view.allied = tp == PlayerId::GAIA
                    || crate::unit_ai::task_finder::allied(
                        &c.w.players.player(c.player)?.relations,
                        c.player,
                        tp,
                        true,
                    );
                (Some(view), start)
            }
            None => (
                None,
                crate::garrison::WalkStart {
                    gap: Distance::ZERO,
                },
            ),
        };
        let master = self.master(c.w.master_tables(), c.player)?;
        let villager = master.class == class::VILLAGER;
        Ok(crate::garrison::EnterReads {
            target,
            start,
            waits: villager && !c.w.players.player(c.player)?.computer,
            stance: c
                .w
                .parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::AGGRESSIVE, |ai| ai.stance),
            defaults: MoveDefaults::of(
                master,
                c.w.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
        })
    }

    /// The Enter's enter in its update (`FUN_140a867b0` into `target`, [`Unit::enter_building`],
    /// then the update's own `set_state(1)`).
    pub(crate) fn enter_in_update(
        &mut self,
        c: &mut Step<'_>,
        target: Option<ContainerId>,
    ) -> Result<(), GenieError> {
        let target = target.ok_or(GenieError::unreachable(
            "an Enter's enter without its target",
        ))?;
        let inside = units_inside(
            OthersRef::of(&c.w.players, &c.w.index, c.player)?,
            target.id(),
        );
        if !self.enter_building(c, target, inside)? {
            return Err(GenieError::unsupported(
                "an Enter that did not get in (state 2)",
            ));
        }
        // the update's own set_state(1) after the enter's
        self.entered(c)
    }

    /// `FUN_140a867b0` on a unit inside its target already (the update's walk again put it in):
    /// notify 0x1fa with the enter action value and its `set_state(1)`, then the update's own
    /// `set_state(1)` ([`Unit::entered`] twice).
    ///
    /// # Errors
    ///
    /// The notify's and the `set_state(1)`'s.
    pub(crate) fn entered_again(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        self.post_own(
            &mut c.w.parts.unit_ai,
            crate::unit_ai::NotifyKind::ACTION_COMPLETED,
            [ActionKind::Enter.value(), 0, 0],
        )?;
        for _ in 0..2 {
            self.entered(c)?;
        }
        Ok(())
    }

    /// The Transport's update ([`crate::garrison::transport::TransportAction::update`] on the
    /// step, the action copied out of its list and written back after): what it reads of the
    /// unit's master ([`crate::garrison::transport::TransportReads`]: the sprite its `MoveTo`'s
    /// start shows, `FUN_140aa0d30`'s default for none); its code, whose 1 or 3 deletes it.
    pub(crate) fn transport_update(
        &mut self,
        c: &mut Step<'_>,
        me: Me,
    ) -> Result<HeadCode, GenieError> {
        let mut t = *self
            .actions
            .list
            .transport()
            .ok_or(crate::garrison::transport::NO_TRANSPORT)?;
        let master = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.masters.of(c.player),
        )?;
        let given = crate::garrison::transport::TransportAction::given_sprite(t.task, master)?;
        let reads = crate::garrison::transport::TransportReads {
            radius: master.radius,
            restriction: master.terrain_restriction,
            defaults: MoveDefaults::of(
                master,
                c.w.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
            given,
            standing: master
                .standing_sprite
                .and_then(|s| c.w.graphics.get(s))
                .copied(),
        };
        // written back before an error too: the stop's context reads the list
        let code = t.update(c, self, me, &reads);
        if let Some(own) = self.actions.list.transport_mut() {
            *own = t;
        }
        Ok(HeadCode::of(code?))
    }

    /// The Transport's slot 8 on a move to `point` ([`crate::garrison::transport::TransportAction::move_to`])
    /// with the map about the unit, then what its state change leaves: the unit stops (its
    /// `MoveTo`'s teardown) and takes its standing sprite; `motion` is the unit's motion layer.
    pub(crate) fn transport_sail(
        &mut self,
        mut motion: MotionMut<'_>,
        point: Position,
        (spatial, zones, pathing): (&Spatial, &mut crate::pathing::zones::Zones, &mut Pathing),
        (masters, (graphics, look), sprite_rng): (
            crate::objects::masters::OwnerMasters<'_>,
            (
                &crate::objects::Graphics,
                Option<&crate::combat::abilities::Look>,
            ),
            &mut Well512,
        ),
    ) -> Result<(), GenieError> {
        let master = self.master_in(masters)?;
        let task = self
            .actions
            .list
            .transport()
            .ok_or(GenieError::unreachable("a sail without its Transport"))?
            .task;
        let ship = crate::garrison::transport::Ship::of_unit(
            (self, motion.mover()),
            master,
            spatial.map,
            (graphics, look),
            task,
        )?;
        let restriction = master.terrain_restriction;
        let mut shore = crate::garrison::transport::MapShore::new(
            spatial,
            zones,
            pathing,
            restriction,
            self.position(),
        )?;
        let t = self
            .actions
            .list
            .transport_mut()
            .ok_or(GenieError::unreachable("a sail without its Transport"))?;
        let changed = t.move_to(point, &ship, &mut shore)?;
        if changed.stopped
            && let Some(m) = motion.mover_mut()
        {
            m.stop()?;
        }
        if changed.stands {
            self.show_standing(motion, masters, graphics, sprite_rng)?;
        }
        Ok(())
    }

    /// The unit's new Enter: its own action, or with `sub` the sub-action of its Artifact on top.
    pub(crate) fn place_enter(&mut self, action: crate::garrison::EnterAction, sub: bool) {
        match self.actions.list.artifact_mut().filter(|_| sub) {
            Some(art) => art.enter = Some(action),
            None => self.actions.list.set_enter(Some(action)),
        }
    }

    /// The Enter's `set_state(1)` (`FUN_140a87320` case 1): its sub-actions go (`FUN_140a98620`: a
    /// `MoveTo`'s teardown stops the unit, slot 0x5d0), the unit shows its master's standing
    /// sprite ([`Unit::shows_standing_sprite`]) and, the Enter not being quiet, its unit AI gets
    /// notify 0x1f9 with the enter action value.
    ///
    /// # Errors
    ///
    /// A unit without its Enter, and the stop's, the sprite's and the notify's.
    pub(crate) fn entered(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let enter = self
            .actions
            .list
            .enter_mut()
            .ok_or(GenieError::unsupported("an entered unit without its Enter"))?;
        if enter.move_to.take().is_some()
            && let Some(m) = c.w.parts.mover_mut(self.id)
        {
            m.stop()?;
        }
        enter.state = EnterState::Done;
        self.shows_standing_sprite(
            c.w.parts.motion.layer_mut(self.id),
            crate::objects::masters::OwnerMasters::new(
                &c.w.parts.own_master,
                c.w.masters.of(c.player),
            ),
            &c.w.graphics,
            c.w.streams.get(Stream::Sprite),
        )?;
        self.post_own(
            &mut c.w.parts.unit_ai,
            crate::unit_ai::NotifyKind::ACTION_FAILED,
            [ActionKind::Enter.value(), 0, 0],
        )
    }

    /// The enter (`FUN_140a867b0`) into `target`, one of the unit's player's buildings: a
    /// building that takes what the unit carries (`FUN_140fdd190`, as a drop site) takes its load
    /// first (the resource for kinds 15 to 17 is food, 0 to 3 and 56 their own, others none:
    /// [`give_load`]; the unit carries nothing after, `FUN_14119d310` with −1). A building at no
    /// more than a fifth of its master's hit points (in floats) and a master with a transform
    /// task (0x88) on the building's are not modelled. Then slot 0x1a0 (`FUN_141212e10` ->
    /// `FUN_141084b30` -> `FUN_1411921d0`): with room in the building, the unit's sight goes from
    /// its tile (slot 0x1d8), it leaves its tile's list (`FUN_140f5f560`), the obstruction
    /// manager (`FUN_141055b40`) and the mesh (`+0x660`, `FUN_141053840`), a member of a unit
    /// group leaves it (`FUN_14102c160`, the caller's: [`EnterWorld::left_group`]) and stands
    /// (slot 0x560), and it joins the
    /// building's (slot 0x1c0, `FUN_1411cb850` -> `FUN_14118c890`: `+0x2f0`, at the building's
    /// x and y); its records leave the registry's lists (`FUN_14128a1d0` from its mask to 0);
    /// the building's sight takes a circle of 5 (slot 0x1d0 with 5) and the villager's town-bell
    /// flag follows the garrison (`FUN_1412067f0` with 3). Then notify 0x1fa with the enter
    /// action value and `set_state(1)` ([`Unit::entered`]). The building's firepower (`+0xa00`)
    /// and villager count (`+0xba8`) follow from the units inside; the sound, the tracker and
    /// a Gaia building's capture (slot 0x70) are not held.
    pub(crate) fn enter_building(
        &mut self,
        c: &mut Step<'_>,
        target: ContainerId,
        inside: usize,
    ) -> Result<bool, GenieError> {
        let (tp, building) = c.w.object(target).ok_or(GenieError::unreachable(
            "an enter into a building the world does not hold",
        ))?;
        let own = c.w.masters.of(c.player);
        let masters = own.ok_or(crate::garrison::NO_MASTERS)?;
        // copies: going inside changes the unit and the world, which hold them
        let bm = building
            .master_in(crate::objects::masters::OwnerMasters::new(
                &c.w.parts.own_master,
                c.w.masters.of(tp).or(own),
            ))?
            .clone();
        let hp = building.life.hp;
        let me = self
            .master_in(crate::objects::masters::OwnerMasters::new(
                &c.w.parts.own_master,
                Some(masters),
            ))?
            .clone();
        let owner = c.w.players.player(c.player)?;
        let actor = Actor {
            id: self.id,
            master: &me,
            player: c.player,
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
        if crate::unit_ai::task_finder::drops_at(&c.w.drop_sites, bm.id, &actor)
            && Amount::ZERO < self.life.amount
            && bm.kind == BUILDING
        {
            // the building takes food for the foods a unit carries (15 to 17), the four resources
            // and the ore themselves, nothing else
            let resource = self.life.attribute_type.and_then(|kind| {
                if kind != attribute::FOOD && kind.is_food_load() {
                    Some(attribute::FOOD)
                } else {
                    (kind.is_resource() || kind == attribute::ORE).then_some(kind)
                }
            });
            if let Some(resource) = resource {
                // the building takes the load: the owner's attributes gain it at once
                let mut adds = Vec::new();
                give_load(
                    (&c.w.spatial.relations, owner.sharing),
                    c.player,
                    &mut adds,
                    resource,
                    self.life.amount,
                )?;
                c.w.players.player_mut(c.player)?.add_attributes(adds)?;
                self.life.attribute_type = None;
                self.life.amount = Amount::ZERO;
            }
        }
        if bm.kind == BUILDING
            && hp
                <= HitPoints::new(
                    HitPoints::of_master(bm.hit_points).get()
                        * crate::math::image::garrison::ENTER_HP_SHARE,
                )
        {
            return Err(GenieError::unsupported(
                "an enter into a building at a fifth of its hit points (state 13)",
            ));
        }
        if me
            .tasks
            .iter()
            .find(|t| t.action_type == crate::objects::masters::action_type::DEPOSIT)
            .is_some_and(|t| t.object_id == Some(bm.id))
        {
            return Err(GenieError::unsupported(
                "an enter of a master that transforms (task 0x88)",
            ));
        }
        if bm.garrison.units() <= inside {
            return Ok(false);
        }
        self.go_inside(c, target, &me, &bm)?;
        self.post_own(
            &mut c.w.parts.unit_ai,
            crate::unit_ai::NotifyKind::ACTION_COMPLETED,
            [ActionKind::Enter.value(), 0, 0],
        )?;
        self.entered(c)?;
        Ok(true)
    }

    /// The unit's slot 0x1a0 into building `target` (`FUN_141212e10`; the room test, slot 0x198,
    /// is the caller's), the unit's master `me` and the building's `bm`: its sight goes from its
    /// tile, it leaves its tile's list, the obstruction manager and the mesh; a member of a unit
    /// group leaves the group when the world holds it (removeUnit, `FUN_14102c160` with the
    /// building and 0, [`crate::world::World::remove_unit`] at once on the unit lent back to its
    /// slot; `FUN_14152ff40`) and stands; it joins the building's (slot 0x1c0: the building's
    /// list and, for a unit of type 70, its volleys' share, [`crate::garrison::share`]); its
    /// records leave the registry's lists; the building's sight takes a circle of 5 and the
    /// villager's town-bell flag follows the garrison (`FUN_1412067f0` with 3). The building's
    /// `+0x9a8` (the units inside the unit), and a siege building's slot 0x1b0 are not modelled.
    pub(crate) fn go_inside(
        &mut self,
        c: &mut Step<'_>,
        target: ContainerId,
        me: &Master,
        bm: &Master,
    ) -> Result<(), GenieError> {
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
        let placed =
            c.w.spatial
                .get(id)
                .copied()
                .ok_or(GenieError::unsupported("an entering unit without a place"))?;
        if let Some(v) = c.w.spatial.sight.as_mut() {
            v.sight_off(id, self.position(), placed.radius, false, false)?;
        }
        let (bp, building) = c.w.object(target).ok_or(GenieError::unreachable(
            "an enter into a building the world does not hold",
        ))?;
        let (at, state, reload) = (
            building.position(),
            building.life.state,
            c.w.parts.combat_of(building.id).reload_powerup,
        );
        self.place.go_inside(id, at, &mut c.w.spatial);
        c.w.pathing.removed(id);
        // a member of a unit group (+0x6d0) leaves the group the world holds, then stands
        // (slot 0x560)
        if let Some(group) = self.group.id {
            if c.w.groups.get(group).is_some() {
                c.lend(self, |w| {
                    w.remove_unit(group, id, Some(target.id()), false).map(drop)
                })?;
            }
            if let Some(m) = c.w.parts.mover_mut(self.id) {
                m.stand()?;
            }
        }
        let building_radius = c.w.spatial.get(target).map_or(bm.radius, |p| p.radius);
        self.life.inside = Some(target);
        self.join_building(c, Some(target), bp, reload, me, bm)?;
        self.enter_sight(c, target, at, building_radius, placed, bm)?;
        let mut bell = c.w.parts.town_bell_of(id);
        let ai = c.w.parts.unit_ai.get(id);
        self.enter_town_bell((&mut bell, ai), Some(target), at, state, (me, bm))?;
        c.w.parts.set_town_bell(id, bell);
        Ok(())
    }

    /// [`Unit::go_inside`]'s slot 0x1c0 of building `target` (`FUN_1411cb850` -> `FUN_14118c890`,
    /// `FUN_14120b790`; its owner `bp`, its reload powerup `reload`): its list takes the unit,
    /// and a unit (type 70) adds its share to the building's volleys.
    fn join_building(
        &self,
        c: &mut Step<'_>,
        target: Option<ContainerId>,
        bp: PlayerId,
        reload: Seconds,
        me: &Master,
        bm: &Master,
    ) -> Result<(), GenieError> {
        let masters =
            c.w.masters
                .get(c.player.index())
                .ok_or(crate::garrison::NO_MASTERS)?;
        let firepower = if me.kind == COMBAT {
            let own = c.w.masters.of(bp).unwrap_or(masters);
            let rate = crate::garrison::attack_rate(bm, own, reload);
            crate::garrison::share(
                me,
                masters,
                c.w.parts.combat_of(self.id).reload_powerup,
                rate,
            )?
        } else {
            crate::combat::Firepower::ZERO
        };
        if let Some((_, b)) = c.w.object_mut(target) {
            b.garrison.push(self.id);
            let id = b.id;
            c.w.parts.combat_mut(id)?.firepower += firepower;
        }
        Ok(())
    }

    /// [`Unit::go_inside`]'s sight: the unit's records leave the registry's lists
    /// (`FUN_14128a1d0` from its mask to 0, `placed` its record as it was) and a building
    /// `target` at `at` (radius `radius`) sees a circle of 5 (slot 0x1d0 with 5).
    fn enter_sight(
        &self,
        c: &mut Step<'_>,
        target: ContainerId,
        at: Position,
        radius: Footprint,
        placed: crate::map::spatial::Placed,
        bm: &Master,
    ) -> Result<(), GenieError> {
        let id = self.id;
        let Some(v) = c.w.spatial.sight.as_mut() else {
            return Ok(());
        };
        if let Some(r) = v.registry.as_mut().filter(|_| 0 <= id.get()) {
            let mask = r.entry(id).map_or(VisionBits::NONE, |e| e.mask);
            r.update_unit(UnitUpdate {
                id,
                owner: c.player,
                position: self.position().ground(),
                radii: placed.radius,
                class: placed.class,
                old: mask & VisionBits::VISIBLE_HALF,
                new: VisionBits::NONE,
            })?;
            // its mask (`+0x8f8`) is none after
            r.entries
                .get_or_insert_with(id, crate::vision::registry::Entry::default)?
                .mask = VisionBits::NONE;
        }
        // a building's sight takes a circle of 5 (FUN_141212e10, type 80 only)
        if bm.kind == BUILDING {
            let _ = v.sight_on_los(
                target.id(),
                at,
                radius,
                false,
                crate::garrison::INSIDE_SIGHT,
                false,
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
        (bell, ai): (&mut crate::orders::town_bell::TownBell, Option<&UnitAi>),
        target: Option<ContainerId>,
        at: Position,
        state: ObjectState,
        (me, bm): (&Master, &Master),
    ) -> Result<(), GenieError> {
        if me.class != class::VILLAGER && me.id != MASTER_1808 {
            return Ok(());
        }
        if !bell.flag {
            let action = ai
                .map(|ai| ai.current_action)
                .ok_or(GenieError::unsupported(
                    "a town-bell record of a unit without a unit AI",
                ))?;
            bell.record(crate::orders::town_bell::BackToWork {
                target: target.map(ContainerId::id),
                target_class: None,
                action,
                target2: None,
                point: at.ground(),
            });
        }
        if bm.kind == BUILDING
            && state == ObjectState::Alive
            && bm
                .building
                .is_some_and(|b| b.garrison_type.contains(GarrisonKinds::VILLAGERS))
        {
            bell.flag = true;
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
    pub(crate) fn inside_update(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        if !self.actions.list.is_empty() {
            return self.end_head_update(c, HeadCode::Deletes);
        }
        if self.actions.unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "the inside update of an action the slice does not hold",
            ));
        }
        Ok(())
    }
}
