//! The idle checks of ships, siege, soldiers, units that do not attack and towers.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::master_type::COMBAT;
use crate::players::PlayerId;
use crate::players::PlayerType;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAiType;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{
    ARTIFACT_AI, AttackAsk, BOAR_AI, CIVILIAN, ENEMY_SIGHTING_GAP, FISHING_AI, FISHING_CARED,
    Found, Memory, Pick, SHIP_EVADE_REACH, SIEGE_AI, SIEGE_CARED, SIEGE_TOWER_AI, SOLDIER_AI,
    SOLDIER_CARED, SOLDIER_IDLE_MASTER, Search, Seeker, Surroundings, TOWER_AI, TOWER_CARED,
    TRADE_AI, TRADE_CART_CARED, TRADE_SHIP_CARED, UnitAi, UnitSide, attack_search_radius,
    civilian_search_radius, computer_idle_check, search_radius,
};
use crate::world::Seconds;

impl UnitAi {
    /// processIdle, slot 72 (`+0x240`) of each factory type's vtable: the hunted animal's
    /// ([`UnitAi::hunted_idle`]), the boar's ([`UnitAi::boar_idle`]), the lion's
    /// ([`UnitAi::lion_idle`]), the soldier's ([`UnitAi::soldier_idle`]) and the tower's, the
    /// base `FUN_140c564c0` ([`UnitAi::tower_idle`]); the villager's (`FUN_140c1e610`) and the
    /// transport ship's (`0x140c21d20`) only lower the unit's last human order time
    /// ([`UnitAi::cap_human_order`]); the building's (`FUN_140c1e600`) and the livestock's
    /// (`FUN_140c1fea0`) do nothing. True when it produced an action (6), false for 5. Its
    /// second argument (1 from the idle part, else 0) lets an order the soldier's and the base's
    /// slot 0x250 retry give 6, which no modelled type's slot 0x60 allows.
    pub(super) fn process_idle(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        match self.ai_type {
            UnitAiType::HUNTED => self.hunted_idle(at, player_type, ai, side),
            BOAR_AI => Ok(self.boar_idle(at, ai, side)?),
            UnitAiType::LION => self.lion_idle(at, ai, side),
            // the civilian's (FUN_140c1e610), the transport ship's (0x140c21d20) and the siege
            // tower's (FUN_140c20690): the cap, then 5
            CIVILIAN | UnitAiType::TRANSPORT | SIEGE_TOWER_AI => {
                self.cap_human_order(side, ai)?;
                Ok(false)
            }
            SOLDIER_AI => self.soldier_idle(at, player_type, ai, side),
            TRADE_AI | FISHING_AI => self.ship_idle(at, player_type, ai, side),
            SIEGE_AI => self.siege_idle(at, player_type, ai, side),
            TOWER_AI => self.tower_idle(at, player_type, side, ai),
            ARTIFACT_AI => self.artifact_idle(at, side, ai),
            UnitAiType::BUILDING | UnitAiType::LIVESTOCK => Ok(false),
            _ => Err(GenieError::unsupported(
                "processIdle (slot 0x240) of a unit-AI type not modelled",
            )),
        }
    }

    /// The trade ship's and the fishing ship's idle check (slot 72, `FUN_140c21930`,
    /// `FUN_140c1eab0`): the cap ([`UnitAi::cap_human_order`]); a computer player's ship not
    /// inside another looks for the closest object of the classes it cares about
    /// (`ClosestObject` with every relation, state and master, reachable: [`Pick::Closest`]; the
    /// search adds the fishing ship's `+0x1e8`, 7, to its radius) and, finding one the world
    /// holds, moves away from it by its speed times [`SHIP_EVADE_REACH`] along the line between
    /// them (`evasiveMoveTo`, slot 0x150 with both flags, [`UnitAi::evasive_move`]): true (6).
    /// Else false (5).
    pub(super) fn ship_idle(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        self.cap_human_order(side, ai)?;
        if !player_type.is_computer() || at.garrisoned {
            return Ok(false);
        }
        let (classes, radius): (&[ClassId], f32) = if self.ai_type == FISHING_AI {
            (
                &FISHING_CARED,
                civilian_search_radius(at.los, at.search_radius).get(),
            )
        } else if at.class == Some(class::TRADE_BOAT) {
            (
                &TRADE_SHIP_CARED,
                search_radius(at.los, at.search_radius).get(),
            )
        } else {
            (
                &TRADE_CART_CARED,
                search_radius(at.los, at.search_radius).get(),
            )
        };
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius: Distance::new(radius),
            classes,
            size: at.size,
            pick: Pick::Closest {
                group: None,
                relation: None,
                state: None,
                master: None,
                workers: crate::objects::WorkerCount::NO_CAP,
                spared: None,
                reach: true,
            },
            memory: Memory::default(),
        })?;
        self.alert(found, at.dt_seconds);
        let Some(id) = found.id else {
            return Ok(false);
        };
        let Some(u) = side.seen_unit(id)? else {
            return Ok(false);
        };
        let reach = (at.speed * SHIP_EVADE_REACH).get();
        let (x, y, z) = (at.position.x(), at.position.y(), at.position.z());
        let dx = u.position.x() - x;
        let dy = u.position.y() - y;
        let d = crate::math::fmath::sqrt(dy * dy + dx * dx);
        let point = Position::new(x - (dx / d) * reach, y - (dy / d) * reach, z);
        let _ = self.evasive_move(point, (true, true), at, ai, side)?;
        Ok(true)
    }

    /// The siege unit's idle check (slot 72, `FUN_140c206b0`, factory type 0x10). Slot 0x250 first
    /// retries no order (the base's slot 0x60 takes none as retryable), then the cap
    /// ([`UnitAi::cap_human_order`]). A Gaia unit stops (5) unless of the combat type with
    /// `+0x924` 0 outside the scenario modes. In `NoAttack` (stance 3) it searches alone (slot 0x50
    /// with mode 1 from its place, [`Pick::Look`] over [`SIEGE_CARED`], [`UnitAi::attack_search_alert`])
    /// and but on the patrol orders walks back to its state position ([`UnitAi::walk_back`]);
    /// 5. Else its target is its top action's within its range (slot 400, slot 0x1e8); with
    /// retargeting off (`+0x1d6`, `FUN_140c4ee90`) and the world holding its current target, a
    /// top target the unit reaches within its range (slot 0x3d0 with slot 0x230) ends the check
    /// (5), else retargeting comes back on (`FUN_140c40ef0` with 0). A computer player's tests
    /// ([`computer_idle_check`]) follow. Without a target `bestUnitToAttack` from its place
    /// ([`Pick::Attack`] over [`SIEGE_CARED`]); a target found it attacks (slot 0xb8 forced, any
    /// 0, [`UnitAi::attack_object`]), a patrol order's patrol saved first: once it attacks, off
    /// the defend order (`0x2bd`) the order goes (`FUN_140c64b60` with −1, 0) and it gives 6; a
    /// failed attack on a patrol order loses the saved patrol. On a patrol order it then gives 5.
    /// Else (the alert mode's object `+0x1bc` taking the base's slot 0x1d8, −1, not held) a human
    /// player's unit in a group gives 5 ("I'm in a formation"), one in Defend or `StandGround`
    /// walks back; last the saved patrol, if any, goes on (`restorePatrolInformation`,
    /// [`UnitAi::restore_patrol`]: 6). Else 5.
    pub(super) fn siege_idle(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        self.cap_human_order(side, ai)?;
        let searches = !at.scenario && COMBAT <= at.kind && !at.capture.any();
        if at.player == PlayerId::GAIA && !searches {
            return Ok(false);
        }
        if self.stance == UnitStance::NO_ATTACK {
            self.siege_no_attack(at, player_type, side, ai)?;
            return Ok(false);
        }
        let mut target = Self::top_target_in_range(at, side)?;
        if self.retarget_disabled != 0 && at.target.is_some() {
            // slot 0x3d0 with the top target (−1 answers 0) and the weapon range
            if let Some(t) = target
                && side.reaches(t, at.weapon_range)?
            {
                return Ok(false);
            }
            self.retarget_disabled = 0;
        }
        if player_type.is_computer() {
            computer_idle_check()?;
        }
        if target.is_none() {
            target = self.siege_best(at, player_type, side)?;
        }
        if let Some(t) = target
            && self.siege_attack(t, at, ai, side)?
        {
            return Ok(true);
        }
        self.siege_idle_end(at, player_type, ai, side)
    }

    /// [`UnitAi::siege_idle`]'s attack on `target` (slot 0xb8 forced, any 0,
    /// [`UnitAi::attack_object`]), a patrol order's patrol saved first: once it attacks, off the
    /// defend order (`0x2bd`) the order goes (`FUN_140c64b60` with −1, 0); a failed attack on a
    /// patrol order loses the saved patrol. Whether it attacks.
    ///
    /// # Errors
    ///
    /// What the attack does not model.
    fn siege_attack(
        &mut self,
        target: ObjectId,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            self.save_patrol(side);
        }
        if self.attack_object(target, true, false, at, side)? {
            if self.current_order != Some(OrderKind::Defend) {
                self.set_order_with(None, false, ai);
            }
            return Ok(true);
        }
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            self.zero_patrol();
        }
        Ok(false)
    }

    /// [`UnitAi::siege_idle`] in `NoAttack` (stance 3): it searches alone (slot 0x50 with mode 1
    /// from its place, [`Pick::Look`] over [`SIEGE_CARED`], [`UnitAi::attack_search_alert`]) and
    /// but on the patrol orders walks back to its state position ([`UnitAi::walk_back`]).
    ///
    /// # Errors
    ///
    /// What the search and its alert do not model.
    pub(super) fn siege_no_attack(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius: attack_search_radius(at),
            classes: &SIEGE_CARED,
            size: at.size,
            pick: Pick::Look,
            memory: Memory::default(),
        })?;
        self.attack_search_alert(found, player_type)?;
        if !matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            let _ = self.walk_back(at, side, rng)?;
        }
        Ok(())
    }

    /// [`UnitAi::siege_idle`]'s `bestUnitToAttack` without a target, from its place
    /// ([`Pick::Attack`] over [`SIEGE_CARED`]): the object found, if any.
    ///
    /// # Errors
    ///
    /// What the search and its alert do not model.
    pub(super) fn siege_best(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        side: &mut dyn UnitSide,
    ) -> Result<Option<ObjectId>, GenieError> {
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius: attack_search_radius(at),
            classes: &SIEGE_CARED,
            size: at.size,
            pick: Pick::Attack {
                lowest: if at.scenario { 2 } else { 0 },
                seeker: self.seeker(at),
                ask: AttackAsk::IDLE,
            },
            memory: self.memory(),
        })?;
        self.add_retries(&found);
        self.attack_search_alert(found, player_type)?;
        Ok(found.id)
    }

    /// [`UnitAi::siege_idle`]'s end without an attack: on a patrol order 5; a human player's unit
    /// in a group 5 ("I'm in a formation"), one in Defend or `StandGround` walks back; last the
    /// saved patrol, if any, goes on ([`UnitAi::restore_patrol`]: 6). Else 5.
    pub(super) fn siege_idle_end(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            return Ok(false);
        }
        if !player_type.is_computer() {
            if at.group.is_some() {
                return Ok(false);
            }
            if self.stance.leashed() {
                let _ = self.walk_back(at, side, ai)?;
            }
        }
        self.restore_patrol(at, ai, side)
    }

    /// The soldier's idle check (slot 72, `FUN_140c20db0`, factory type 13). Before its cap
    /// (`FUN_140c41e50`, [`UnitAi::cap_human_order`]), a player that is out (`+0x89` 2; not held,
    /// 0 in every player probed) would stop, and slot 0x250 (`FUN_140c5d9a0`) retries no order,
    /// the soldier's slot 0x60 taking none as retryable. A Gaia unit stops unless it is of the combat type with `+0x924` 0 outside the
    /// scenario modes (`FUN_141302540`); one in `NoAttack` (stance 3) searches and returns to its
    /// state position ([`UnitAi::no_attack_idle`]); master 706 stops. Its target is the target of its top action
    /// within its range (slot 400, slot 0x1e8) with retargeting on (`+0x1d6` 0), else its current
    /// target; neither is modelled, nor a computer's branches (patrol data, priorities, evasion).
    /// Without one, `bestUnitToAttack` from its place ([`Pick::Attack`]): an object found it attacks
    /// (slot 0xb8 forced, [`UnitAi::attack_object`]; then, with retargeting on and off order
    /// 0x2bd, the order goes) and processIdle produced (true); a patrol order's saved patrol data
    /// is not modelled. Else a patrol order stops. The search leaves the alert mode clear, so `+0x1bc`
    /// stays. Then, outside a unit group (`+0x6d0` −1; its test is not held), a unit in Defend or
    /// `StandGround` (stance 1, 2) walks back to its state position ([`UnitAi::walk_back`]),
    /// which leaves the result 5; a patrol path (`+0x1c0`) and the healer's ally search
    /// (`FUN_140c16f00`, for a master with a task of action type 0x69) are not modelled.
    pub(super) fn soldier_idle(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        self.cap_human_order(side, ai)?;
        let searches = !at.scenario && COMBAT <= at.kind && !at.capture.any();
        if at.player == PlayerId::GAIA && !searches {
            return Ok(false);
        }
        if self.stance == UnitStance::NO_ATTACK {
            return self
                .no_attack_idle(at, player_type, side, ai)
                .map(|()| false);
        }
        if at.master == SOLDIER_IDLE_MASTER {
            return Ok(false);
        }
        // its target: with retargeting on, its top action's within its range (slot 400, slot
        // 0x1e8); with it off, its current target while the world holds it, else the top
        // action's
        let held =
            (self.retarget_disabled != 0 && at.target.is_some()).then_some(self.current_target);
        let target = match held {
            Some(t) => Some(t),
            None => Self::top_target_in_range(at, side)?.map(Some),
        };
        if player_type.is_computer() {
            computer_idle_check()?;
        }
        let target = match target {
            Some(t) => t,
            None => self.soldier_best(at, player_type, side)?,
        };
        if let Some(t) = target
            && self.soldier_attack(t, at, ai, side)?
        {
            return Ok(true);
        }
        self.soldier_idle_end(at, ai, side)
    }

    /// [`UnitAi::soldier_idle`]'s `bestUnitToAttack` without a target, from its place
    /// ([`Pick::Attack`] over [`SOLDIER_CARED`]): the object found, if any.
    ///
    /// # Errors
    ///
    /// What the search and its alert do not model.
    fn soldier_best(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        side: &mut dyn UnitSide,
    ) -> Result<Option<ObjectId>, GenieError> {
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius: attack_search_radius(at),
            classes: &SOLDIER_CARED,
            size: at.size,
            pick: Pick::Attack {
                lowest: if at.scenario { 2 } else { 0 },
                seeker: self.seeker(at),
                ask: AttackAsk::IDLE,
            },
            memory: self.memory(),
        })?;
        self.add_retries(&found);
        self.attack_search_alert(found, player_type)?;
        Ok(found.id)
    }

    /// [`UnitAi::soldier_idle`]'s attack on `target`, a patrol order's patrol saved first ("I need
    /// to save the patrol information in case the attack works."): slot 0xb8 forced
    /// (`FUN_140c17550`: the base's, then the unit's group); once it attacks, with retargeting on
    /// and off order 0x2bd the order goes (`FUN_140c64b60` with −1, 0); a failed attack on a
    /// patrol order loses the saved patrol. Whether it attacks.
    ///
    /// # Errors
    ///
    /// What the attack does not model.
    fn soldier_attack(
        &mut self,
        target: ObjectId,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            self.save_patrol(side);
        }
        if self.attack_slot(target, false, at, side, ai)? {
            if self.retarget_disabled == 0 && self.current_order != Some(OrderKind::Defend) {
                self.set_order_with(None, false, ai);
            }
            return Ok(true);
        }
        // "Attack failed, so I'm nuking the saved patrol information."
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            self.zero_patrol();
        }
        Ok(false)
    }

    /// [`UnitAi::soldier_idle`]'s end without an attack: a patrol order stops there ("I have a
    /// patrol order, so I'm stopping here."); a unit in Defend or `StandGround` walks back to its
    /// state position ("returning to state pos.", its result not the processIdle's); then the
    /// saved patrol, if any, goes on (`restorePatrolInformation`: processIdle produced). Else
    /// false.
    ///
    /// # Errors
    ///
    /// The healer's search for an ally (a master with a task of action type 0x69), and what the
    /// walk and the patrol do not model.
    fn soldier_idle_end(
        &mut self,
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            return Ok(false);
        }
        if self.stance.leashed() {
            let _ = self.walk_back(at, side, ai)?;
        }
        if self.restore_patrol(at, ai, side)? {
            return Ok(true);
        }
        if at.task_0x69 {
            return Err(GenieError::unsupported(
                "a healer's search for an ally to heal",
            ));
        }
        Ok(false)
    }

    /// The soldier's idle check in `NoAttack` (stance 3, `FUN_140c20db0`: "searching and returning
    /// to state pos."): the search alone from its place (slot 0x50, `FUN_140c63e20` with mode 1,
    /// [`Pick::Look`]) leaves the alert mode clear when it finds no enemies; then, but on the
    /// patrol orders, it walks back to its state position ([`UnitAi::walk_back`], its result
    /// unread); last the healer's ally search (`FUN_140c16f00`). Not modelled: enemies found (the
    /// alert mode on, its object `+0x1bc` from slot 0x1d8, and after 10 s without a sighting the
    /// player's notify 0x210, slot 0x2e8), a defend order's search (from its target's objects), a
    /// computer's cared classes (`+0x190` with 10) and the healer's search.
    pub(super) fn no_attack_idle(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        if self.current_order == Some(OrderKind::Defend) {
            return Err(GenieError::unsupported(
                "a defend order's search (its target's objects)",
            ));
        }
        if player_type.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's soldier's NoAttack search",
            ));
        }
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius: attack_search_radius(at),
            classes: &SOLDIER_CARED,
            size: at.size,
            pick: Pick::Look,
            memory: Memory::default(),
        })?;
        if found.enemies {
            return Err(GenieError::unsupported(
                "a NoAttack soldier's search that finds enemies",
            ));
        }
        if found.fresh {
            self.alert_mode = false;
        }
        if !matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            let _ = self.walk_back(at, side, rng)?;
        }
        if at.task_0x69 {
            return Err(GenieError::unsupported(
                "a healer's search for an ally to heal",
            ));
        }
        Ok(())
    }

    /// The base unit AI's idle check (slot 72, `FUN_140c564c0`), the tower's (factory type 15):
    /// slot 0x250 first retries no order (the tower's slot 0x60 taking none as retryable), then
    /// the inlined cap of `FUN_140c41e50` ([`UnitAi::cap_human_order`]). In `NoAttack` (stance
    /// 3) it only searches (slot 0x50 with mode 1 from its place, [`Pick::Look`] over its cared
    /// classes, which sets the alert mode) and gives 5; else `bestUnitToAttack` from its place
    /// ([`Pick::Attack`], as [`UnitAi::soldier_idle`]'s), then an object found is attacked
    /// ([`UnitAi::attack_object`] forced, slot 0xb8, which then sets `+0x1d5`, not held). True
    /// when it attacks (processIdle's 6).
    pub(super) fn tower_idle(
        &mut self,
        at: &Surroundings,
        player_type: PlayerType,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<bool, GenieError> {
        self.cap_human_order(side, rng)?;
        if self.stance == UnitStance::NO_ATTACK {
            let found = side.closest(&Search {
                at: at.position.ground(),
                radius: attack_search_radius(at),
                classes: &TOWER_CARED,
                size: at.size,
                pick: Pick::Look,
                memory: Memory::default(),
            })?;
            self.attack_search_alert(found, player_type)?;
            return Ok(false);
        }
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius: attack_search_radius(at),
            classes: &TOWER_CARED,
            size: at.size,
            pick: Pick::Attack {
                lowest: if at.scenario && !player_type.is_computer() {
                    2
                } else {
                    0
                },
                seeker: self.seeker(at),
                ask: AttackAsk::IDLE,
            },
            memory: self.memory(),
        })?;
        self.add_retries(&found);
        self.attack_search_alert(found, player_type)?;
        let Some(id) = found.id else {
            return Ok(false);
        };
        // slot 0xb8 with 1 (`+0x1d5` then 1, not held)
        self.attack_object(id, true, false, at, side)
    }

    /// What `bestUnitToAttack` reads of this unit AI ([`Seeker`]), its current target's master
    /// type as the unit's surroundings `at` hold it.
    pub(super) fn seeker(&self, at: &Surroundings) -> Seeker {
        self.seeker_with(
            at.target
                .filter(|_| self.current_target.is_some())
                .map(|t| t.kind),
        )
    }

    /// What the attack tests read of the unit AI ([`Seeker`]), its current target's master type
    /// `target_kind` as the world holds it.
    #[must_use]
    pub(crate) const fn seeker_with(&self, target_kind: Option<MasterKind>) -> Seeker {
        Seeker {
            stance: self.stance,
            state_position: self.state_position,
            order: self.current_order,
            action: self.current_action,
            target: self.current_target,
            target_class: self.target_class,
            last_target_class: self.last_target_class,
            patrol_path: self.patrol_path.is_some(),
            target_kind,
        }
    }

    /// The alert mode after `bestUnitToAttack`'s search (`FUN_140c63e20` with its fourth argument
    /// 1): a query that ran sets it to whether it found enemies (`+0x1b8`); with enemies, after
    /// more than 10 s without a sighting (`+0x1b4`) the owner hears of it (its slot 0x2e8 with
    /// `0x210`, `FUN_14162dab0`: no case for it, only the reset of the old short pather's hulled
    /// area, and a computer player's AI's notify, not modelled), and the time restarts.
    pub(super) fn attack_search_alert(
        &mut self,
        found: Found,
        player_type: PlayerType,
    ) -> Result<(), GenieError> {
        if !found.fresh {
            return Ok(());
        }
        self.alert_mode = found.enemies;
        if found.enemies {
            if ENEMY_SIGHTING_GAP < self.time_since_enemy && player_type.is_computer() {
                return Err(GenieError::unsupported(
                    "a computer player's notify of enemies sighted (0x210, slot 0x3c8)",
                ));
            }
            self.time_since_enemy = Seconds::ZERO;
        }
        Ok(())
    }
}
