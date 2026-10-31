//! What the villagers, transports, traders, fishing ships and buildings do when damaged.

use crate::GenieError;
use crate::map::Position;
use crate::math::image::unit_ai::{ATTACKER_FAR, ATTACKER_TIE};
use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::class;
use crate::players::PlayerId;
use crate::players::QueryRelation;
use crate::unit_ai::ActionKind;
use crate::unit_ai::AiCommand;
use crate::unit_ai::Issuer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderTargetType;
use crate::unit_ai::{
    BREAK, DEAD, Effect, FISHING_BUILD_FAR, FISHING_BUILDS, FISHING_CARED, FOUNDATION_REACH, GO_ON,
    Memory, NO_ANIM_SPEED, Notify, Pick, QueuedOrder, SIEGE_TOWER_AI, Search, Surroundings, UnitAi,
    UnitSeen, UnitSide, alert_owner, civilian_search_radius, closest_undiscovered_tile,
    held_action_state,
};
use crate::world::Millis;

impl UnitAi {
    /// The villager's damage notify (`FUN_140c22bf0`, case 500, "I took some damage from Unit
    /// #%d"): the unit's group (`+0x6d0`, `FUN_140c55c00`) would hear of it, and the slice holds
    /// none. At no hit points left (`p[1]` below 1) it gives up: the owner's alert (slot 0x2e8
    /// with `0x209`, the human player's), the notifies end (`FUN_140c5dc10`) and it returns 4.
    /// An attacker the world does not hold ends it, as does a player who is out (`+0x89` 2; not
    /// held, 0 in every player probed). A computer's unit AI (slot 0x50) is not modelled. Then the
    /// owner's alert (slot 0x2e8 with `0x201`); the attacker joins its attackers
    /// (`FUN_140b43f30`), and a new one that is not its target nor a class-10 attacker of
    /// combat level 1 (`FUN_140fde4f0`) makes it call for help ([`UnitAi::ask_for_help`]); the
    /// unit is under attack (`FUN_14119d2d0` with 1). It ignores the damage ("I have an
    /// attack/hunt/repair order") on a move or enter order while it animates (slot 0x1f8 above
    /// 0), the attack or the repair order; on the hunt, gather and boar luring orders with its
    /// top action in state 12 (slot 0x2b0), a target of class 10 the world holds and a Gaia
    /// attacker; or on those orders from a class-10 attacker of combat level 1 with its top
    /// action in state 4, 8, 12 or 15 (`0x9110`). A Gaia attacker, or another player's the
    /// villager does not fear ([`UnitSide::harmless`]), it attacks back: with an order not on
    /// the attacker, its current order goes back into the queue at once
    /// ([`UnitAi::requeue_current`]) and the order becomes the attack (700, the last order kept)
    /// at priority 100; then the civilian's `AttackObject` (slot 0xb8, `FUN_140c17370`, forced):
    /// `HuntObject` ([`UnitAi::hunt_object`]) on classes 9, 10 and 15, else `AttackObject`; then
    /// `+0x1a8` is set and it breaks off. From any other it evades ("I have %d attackers,
    /// evading them."), but on the move action (`0x262`) at a speed other than 0: its current
    /// order goes to the queue's end ([`UnitAi::queue_current`]), its action stops (slot 0xb0
    /// with 1) and it runs from its attackers ([`UnitAi::run_away`] with 1). Both return 3.
    pub(super) fn villager_damaged(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        computer: bool,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        if n.p[1] < 1 {
            self.end_notifies(time, at, side, ai_rng)?;
            return Ok(DEAD);
        }
        let Some(attacker) = at.unit(n.caller).copied() else {
            return Ok(GO_ON);
        };
        let caller = attacker.id;
        if computer {
            return Err(GenieError::unsupported(
                "a computer villager's damage notify (its slot 0x50)",
            ));
        }
        let added = self.attackers.add(Some(caller));
        let fierce = attacker.class == class::PREDATOR && attacker.combat_level == 1;
        if added && self.current_target != Some(caller) && !fierce {
            self.ask_for_help(caller, at, computer, side)?;
        }
        side.set_under_attack(true);
        if self.villager_ignores(at, &attacker, fierce)? {
            return Ok(GO_ON);
        }
        if attacker.owner != PlayerId::GAIA && !side.harmless(caller)? {
            // "I have %d attackers, evading them.", but on the move action at a speed (not 0,
            // UCOMISS: a NaN keeps moving): the current order goes to the queue's end (slot 0x30
            // neither immediate nor in front), the action stops (slot 0xb0 with 1) and it runs
            // from its attackers (slot 0x160 with 1 and no unit)
            if self.current_action == Some(ActionKind::Move)
                && at.anim_speed.ok_or(NO_ANIM_SPEED)? != 0.0
            {
                return Ok(BREAK);
            }
            self.queue_current(at)?;
            self.stop_current(true, at, side, ai_rng)?;
            let _ = self.run_away(true, at, computer, ai_rng, side)?;
            return Ok(BREAK);
        }
        // "I have a GAIA attacker" (or a harmless one): with an order not on the attacker, the current order goes back
        // into the queue at once (slot 0x30) and the order becomes the attack (700,
        // `FUN_140c64b60` with 0: the last order kept) at priority 100
        if self.current_order.is_some() && Some(caller) != self.current_target {
            self.requeue_current(at, side, computer, ai_rng)?;
            self.set_order_with(Some(OrderKind::Attack), false, ai_rng);
            self.order_priority = 100;
        }
        // slot 0xb8 forced, the civilian's
        let _ = self.civilian_attack(caller, at, side, ai_rng)?;
        self.stop_after_kill = true;
        Ok(BREAK)
    }

    /// [`UnitAi::villager_damaged`]'s orders that ignore the damage from `attacker` ("I have an
    /// attack/hunt/repair order"): a move or enter order while it animates (slot 0x1f8 above 0),
    /// the attack or the repair order; the hunt, gather and boar luring orders with its top
    /// action in state 12 (slot 0x2b0), a target of class 10 the world holds and a Gaia
    /// attacker; or those orders from a `fierce` attacker (class 10 of combat level 1) with its
    /// top action in state 4, 8, 12 or 15 (`0x9110`).
    ///
    /// # Errors
    ///
    /// A top action state or an animation speed the slice does not hold.
    pub(super) fn villager_ignores(
        &self,
        at: &Surroundings,
        attacker: &UnitSeen,
        fierce: bool,
    ) -> Result<bool, GenieError> {
        let order = self.current_order;
        let state = held_action_state(at.action_state)?.get();
        let gathers = matches!(
            order,
            Some(OrderKind::Hunt | OrderKind::Gather | OrderKind::AutoBoarLuring)
        );
        let moves = matches!(order, Some(OrderKind::Move | OrderKind::Enter))
            && 0.0 < at.anim_speed.ok_or(NO_ANIM_SPEED)?;
        Ok(moves
            || order == Some(OrderKind::Attack)
            || (gathers
                && state == 12
                && at.target.is_some_and(|t| t.class == class::PREDATOR)
                && attacker.owner == PlayerId::GAIA)
            || order == Some(OrderKind::Repair)
            || (fierce && gathers && state < 16 && (0x9110u32 >> state) & 1 != 0))
    }

    /// The transport ship's damage notify (`FUN_140c2a580`, cases 500 and a missile's warning
    /// it did not dodge, "I took some damage from Unit #%d"): the unit's group only logs it
    /// (`FUN_140c55c00`, `FUN_141027d60`). At no hit points left (`p[1]` below 1) it gives up
    /// ("I'm dead, I give up."): the owner's alert (slot 0x2e8 with 0x209, [`alert_owner`]),
    /// the notifies end (`FUN_140c5dc10`) and it returns 4. Else the owner's alert (0x201); a
    /// new attacker (`FUN_140bf4cb0`) that is not its target makes a human player's ship call
    /// for help ([`call_for_help`]); the attacker joins the set (`FUN_140b43f30`) and the unit
    /// is under attack (`FUN_14119d2d0` with 1). With an order it ignores the damage ("I have an
    /// order, I'm ignoring this damage notify.", 2); the siege tower's (`FUN_140c28400`) also
    /// when a human player's tower is in a group (`+0x6d0`) that is not idle (`FUN_141017ad0`:
    /// "I have a group that won't let me move and a human owner"). Else, with no action or its
    /// top action in state 2, it runs from its attackers (slot 0x160 with 1,
    /// [`UnitAi::run_away`]) and returns 3.
    pub(super) fn transport_damaged(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        computer: bool,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        alert_owner(computer)?;
        if n.p[1] < 1 {
            self.end_notifies(time, at, side, ai_rng)?;
            return Ok(DEAD);
        }
        let new = !self.attackers.contains(n.caller);
        // a help request on no attacker asks no one (`AskForHelp` finds none)
        if new
            && !computer
            && self.current_target != n.caller
            && let Some(caller) = n.caller
        {
            self.ask_for_help(caller, at, computer, side)?;
        }
        let _ = self.attackers.add(n.caller);
        side.set_under_attack(true);
        if self.current_order.is_some() {
            return Ok(GO_ON);
        }
        let held_by_group = at.group.is_some_and(|g| !g.idle) && !computer;
        if self.ai_type == SIEGE_TOWER_AI && held_by_group {
            return Ok(GO_ON);
        }
        if !at.busy || held_action_state(at.action_state)?.get() == 2 {
            let _ = self.run_away(true, at, computer, ai_rng, side)?;
        }
        Ok(BREAK)
    }

    /// The siege unit's test on its executed action (`FUN_140c287e0`, case 0x200, a computer
    /// player's): a weapon range of at least 1 (`COMISS`, `JA`: an unordered one passes), a target
    /// class (`+0x4c`) other than 3, 27, 39, 49 and 4, then a draw (`processNotify`) whose
    /// `r * 100 / 0x7fff` is at least the owner's `chance_for_archers_to_maintain_distance`.
    pub(super) fn keeps_distance(&self, at: &Surroundings, ai: &mut Well512) -> bool {
        if !crate::math::fmath::at_most_or_unordered(1.0, at.weapon_range.get())
            || matches!(
                self.target_class,
                Some(class::BUILDING | class::WALL | class::GATE | class::FARM | class::VILLAGER)
            )
        {
            return false;
        }
        let r = ai.rand15() as i32;
        i32::from(at.keep_distance_chance) <= r * 100 / 0x7fff
    }

    /// The trade ship's case 0x202 (`FUN_140c29bf0`, "Action %d has failed, been invalidated,
    /// or requires a search."): off the trade action an explorer (0x25d) looks for the closest
    /// undiscovered tile ([`closest_undiscovered_tile`]); otherwise the target goes
    /// (`FUN_140c5e1e0`), the action stops (slot 0xb0 with 1; slot 0x48 hands the notify to the
    /// owner, not held) and it returns 2. On the trade action the trader looks for another
    /// partner, the dock (45) for a trade boat, else the market (84): `ClosestObject` for its
    /// own, then a neutral's, each followed by slot 0xf8's trade command (the unit's slot 0x390
    /// and its Trade action, which the slice does not hold), 6 once one takes.
    pub(super) fn trader_search(
        &mut self,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<u32, GenieError> {
        if self.current_action == Some(ActionKind::Trade) {
            return Err(GenieError::unsupported(
                "a trader's search for another dock or market (slot 0xf8, its Trade action)",
            ));
        }
        if self.current_action == Some(ActionKind::Explore) {
            closest_undiscovered_tile()?;
        }
        self.remove_target();
        self.stop_current(true, at, side, rng)?;
        Ok(GO_ON)
    }

    /// The trade ship's damage notify (`FUN_140c29bf0`, cases 500 and a missile's warning it
    /// did not dodge): at no hit points left it gives up (the owner's alert 0x209, the notifies
    /// end, 4). Else the owner's alert (0x201) only with no attackers yet or a computer owner; a
    /// new attacker that is not its target makes a human player's ship call for help
    /// ([`call_for_help`]); the attacker joins the set and the unit is under attack. With an
    /// order it ignores the damage (2), but for a computer player's with `+0x621` set, which
    /// cannot get here (its alert stops the handler first, [`alert_owner`]). Off the move
    /// action (`0x262`) its order goes to the queue's end (slot 0x30, not immediate:
    /// `FUN_140c55c90` takes none of −1) and its action stops (slot 0xb0 with 1); then it runs
    /// from its attackers (slot 0x160 with 1) and returns 3.
    pub(super) fn trade_damaged(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        computer: bool,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        if n.p[1] < 1 {
            alert_owner(computer)?;
            self.end_notifies(time, at, side, ai_rng)?;
            return Ok(DEAD);
        }
        if self.attackers.is_empty() || computer {
            alert_owner(computer)?;
        }
        let new = !self.attackers.contains(n.caller);
        // a help request on no attacker asks no one (`AskForHelp` finds none)
        if new
            && !computer
            && self.current_target != n.caller
            && let Some(caller) = n.caller
        {
            self.ask_for_help(caller, at, computer, side)?;
        }
        let _ = self.attackers.add(n.caller);
        side.set_under_attack(true);
        if self.current_order.is_some() {
            return Ok(GO_ON);
        }
        if self.current_action != Some(ActionKind::Move) {
            self.queue_current(at)?;
            self.stop_current(true, at, side, ai_rng)?;
        }
        let _ = self.run_away(true, at, computer, ai_rng, side)?;
        Ok(BREAK)
    }

    /// The fishing ship's damage notify (`FUN_140c25ca0`, cases 500 and a missile's warning it
    /// did not dodge): as the transport ship's up to its attackers ([`UnitAi::transport_damaged`]);
    /// a human player's ship with an order ignores the damage (2; a computer player's cannot get
    /// here, its alert stops the handler first). Off the move action (`0x262`) its order goes to
    /// the queue's end (slot 0x30, not immediate), its action stops (slot 0xb0 with 1) and it
    /// runs from its attackers (slot 0x160 with 1). It returns 3.
    pub(super) fn fishing_damaged(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        computer: bool,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        alert_owner(computer)?;
        if n.p[1] < 1 {
            self.end_notifies(time, at, side, ai_rng)?;
            return Ok(DEAD);
        }
        let new = !self.attackers.contains(n.caller);
        // a help request on no attacker asks no one (`AskForHelp` finds none)
        if new
            && !computer
            && self.current_target != n.caller
            && let Some(caller) = n.caller
        {
            self.ask_for_help(caller, at, computer, side)?;
        }
        let _ = self.attackers.add(n.caller);
        side.set_under_attack(true);
        if self.current_order.is_some() {
            return Ok(GO_ON);
        }
        if self.current_action != Some(ActionKind::Move) {
            self.queue_current(at)?;
            self.stop_current(true, at, side, ai_rng)?;
            let _ = self.run_away(true, at, computer, ai_rng, side)?;
        }
        Ok(BREAK)
    }

    /// The fishing ship's case 0x202 on its build (`FUN_140c25ca0`, action `0x25a`: "I'm a
    /// builder, looking around for other objects of type %d to build."): `ClosestObject` for its
    /// own foundations (relation 3, state 0, reachable) of each class of [`FISHING_BUILDS`] over
    /// its search ([`FISHING_CARED`], its radius grown by its `+0x1e8`, 7); the nearest by the
    /// distance each answers, below [`FISHING_BUILD_FAR`], that the unit reaches within
    /// `FOUNDATION_REACH` (slot 0x3d0) it builds (slot 0xf0 with 1, [`AiCommand::BuildObject`]) and
    /// the processing breaks. Else a human player's ship goes on (2); a computer player's posts
    /// itself its build's completion (slot 0x328: `0x1fa` with `0x25a`) first.
    pub(super) fn fishing_builder_search(
        &mut self,
        at: &Surroundings,
        computer: bool,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<u32, GenieError> {
        let mut best = (None, FISHING_BUILD_FAR);
        for class in FISHING_BUILDS {
            let found = side.closest(&Search {
                at: at.position.ground(),
                radius: civilian_search_radius(at.los, at.search_radius),
                classes: &FISHING_CARED,
                size: at.size,
                pick: Pick::Closest {
                    group: Some(class),
                    relation: Some(QueryRelation::Itself),
                    state: Some(ObjectState::Foundation),
                    master: None,
                    workers: -1,
                    spared: None,
                    reach: true,
                },
                memory: Memory::default(),
            })?;
            self.alert(found, at.dt_seconds);
            if let Some(id) = found.id
                && found.distance < best.1
            {
                best = (Some(id), found.distance);
            }
        }
        if let Some(site) = best.0
            && side.reaches(site, FOUNDATION_REACH)?
        {
            let _ = self.build_object(site, true, at, side, rng)?;
            return Ok(BREAK);
        }
        if computer {
            self.post(
                Notify::own(
                    at.id,
                    NotifyKind::ACTION_COMPLETED,
                    [ActionKind::Build.value(), 0, 0],
                ),
                at.state,
            )?;
        }
        Ok(GO_ON)
    }

    /// Slot 0x30 with the current order, not immediate and not in front (`FUN_140c55c90`, as the
    /// ships' damage notifies call it: the issuer the unit, the order, its target and class,
    /// owner −1, the target point, the range `+0x5c`, the priority `+0x40`, the point shown,
    /// ready): a unit above state 2 or without an order takes none; else the order goes to the
    /// queue's end ([`UnitAi::queue_order`]).
    pub(super) fn queue_current(&mut self, at: &Surroundings) -> Result<(), GenieError> {
        // without an order the queue takes none, whatever the state
        let Some(order) = self.current_order else {
            return Ok(());
        };
        let record = QueuedOrder {
            issuer: Some(Issuer::Object(at.id)),
            order,
            priority: self.order_priority,
            target: self.current_target,
            target_owner: None,
            point: self.target_point,
            range: self.desired_distance,
            shown: self.target_point.ground(),
            ready: true,
            target_type: self.target_class.and_then(OrderTargetType::of_class),
        };
        self.queue_order(at.state, record, false, false).map(|_| ())
    }

    /// The building's damage notify (`FUN_140c22610`, case 500): at no hit points left it gives
    /// up ("I'm dead, I give up."): the owner's alert (slot 0x2e8 with 0x209), the notifies end
    /// (`FUN_140c5dc10`) and it returns 4. Else the owner hears 0x201, as does each other owner
    /// of the units inside it (`+0x2f8`); for a human player those are the alert list and the
    /// old pather's hulled-area reset (`FUN_14162dab0`), not held, and a computer player's AI
    /// hears them (slot 0x3c8), which is not modelled. A new attacker (`FUN_140bf4cb0`) that is
    /// not its target makes a human player's building ask for help (`AskForHelp`,
    /// `FUN_140c42ce0`), not modelled. Then the attacker joins the set (`FUN_140b43f30`), the
    /// unit is under attack (`FUN_14119d2d0` with 1) and it returns 2.
    pub(super) fn building_damaged(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        computer: bool,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<u32, GenieError> {
        let new = !self.attackers.contains(n.caller);
        alert_owner(computer)?;
        if n.p[1] >= 1 {
            alert_owner(at.guest_computer)?;
        }
        if n.p[1] >= 1
            && new
            && self.current_target != n.caller
            && let Some(caller) = n.caller
        {
            self.ask_for_help(caller, at, computer, side)?;
        }
        if n.p[1] < 1 {
            self.end_notifies(time, at, side, rng)?;
            return Ok(DEAD);
        }
        let _ = self.attackers.add(n.caller);
        side.set_under_attack(true);
        Ok(GO_ON)
    }

    /// `ClosestValidAttacker` (`FUN_140c158d0`): over the attackers in their order, those the
    /// world holds but of classes 3, 13, 52 and 54, the one with the least squared gap
    /// (`FUN_14118f8c0`); one within `DAT_142ef9004` (0.0625) of the best so far takes over on a
    /// coin from the Ai stream (`rand15`, odd). None for none.
    pub(super) fn closest_valid_attacker(
        &self,
        at: &Surroundings,
        ai_rng: &mut Well512,
    ) -> Option<ObjectId> {
        let mut best = (ATTACKER_FAR, None);
        // none (−1) is an object the update does not hold
        for id in self.attackers.iter().flatten() {
            let Some(u) = at.unit(id) else {
                continue;
            };
            if matches!(
                u.class,
                class::BUILDING | class::SIEGE | class::TOWER | class::CLASS_54
            ) {
                continue;
            }
            let d = u.gap_squared;
            if best.0 - ATTACKER_TIE <= d.get()
                && (best.0 + ATTACKER_TIE <= d.get() || ai_rng.rand15() & 1 == 0)
            {
                continue;
            }
            best = (d.get(), Some(id));
        }
        best.1
    }

    /// `HuntObject` (slot 0xc8, `FUN_140c4e220`) forced on `target` (its master's `class` and its
    /// `point`): the current action stops at once (slot 0xb0 with 1, [`UnitAi::stop_current`]),
    /// then the unit's hunt command runs on the target ([`AiCommand::HuntObject`]); taken as done
    /// (the caller stops on a failure): the hunt action value (`0x265`) becomes current, the
    /// stopped one the last, the target with its class and point (the last ones kept).
    pub(super) fn hunt_object(
        &mut self,
        target: ObjectId,
        class: ClassId,
        point: Position,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        self.stop_current(true, at, side, rng)?;
        side.effect_now(Effect::Command(AiCommand::HuntObject(target)), self, rng)?;
        self.set_action(Some(ActionKind::Hunt));
        self.set_target(Some(target), class.or_none(), point);
        Ok(())
    }

    /// Slot 0x30 with the current order, immediate and in front (`FUN_140c55c90` as the
    /// civilian's damage notify calls it: the issuer the unit, the order, target and its class,
    /// owner −1, the target point, the range `+0x5c`, the priority `+0x40`, the point shown,
    /// ready): a unit above state 2 or without an order takes none; else
    /// [`UnitAi::order_now`] with the unit's id, so the current order goes in front of the queue
    /// and the given one, the same record, in front of it.
    pub(super) fn requeue_current(
        &mut self,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        computer: bool,
        ai: &mut Well512,
    ) -> Result<(), GenieError> {
        let Some(order) = self
            .current_order
            .filter(|_| at.state <= ObjectState::Alive)
        else {
            return Ok(());
        };
        let record = QueuedOrder {
            issuer: Some(Issuer::Object(at.id)),
            order,
            priority: self.order_priority,
            target: self.current_target,
            target_owner: None,
            point: self.target_point,
            range: self.desired_distance,
            shown: self.target_point.ground(),
            ready: true,
            target_type: self.target_class.and_then(OrderTargetType::of_class),
        };
        let place = (at.position.ground(), at.class);
        self.order_now(
            record,
            Some(at.id),
            Some(place),
            Some((at, computer)),
            ai,
            side,
        )
    }
}
