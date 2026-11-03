//! A unit's answers to damage and attackers, to a unit in its way and to being sent off;
//! the explorers' search.

use crate::GenieError;
use crate::construction::FoundationId;
use crate::map::Distance;
use crate::map::Footprint;
use crate::math::rng::Well512;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::CombatLevel;
use crate::objects::masters::MasterKind;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::AiAction;
use crate::unit_ai::AiCommand;
use crate::unit_ai::Issuer;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderPriority;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::StopOrder;
use crate::unit_ai::TargetRules;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{
    AiMove, AttackAsk, BREAK, DEAD, Effect, GO_ON, Named, Notify, OrderTargetKind, Pick,
    QueuedOrder, STILL, Search, Surroundings, UnitAi, UnitSeen, UnitSide, alert_owner,
    closest_undiscovered_tile,
};
use crate::world::Millis;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::UnitAiTuning;

impl UnitAi {
    /// The base handler's damage case (`FUN_140c589c0`, cases 500 and a missile's warning, 0x20f:
    /// "I took some damage from Unit #%d"), the attacker `n.caller`:
    ///
    /// 1. The retarget entries go (`+0x1d8` 0). An attacker already in the set (`FUN_140bf4cb0`)
    ///    of a unit whose top action has a target (slot 400) changes nothing (2).
    /// 2. The owner hears the notify (slot 0x48: slot 0x40, `FUN_140c55b80`, its slot 0x2e8) and
    ///    the unit's group only logs it (`FUN_141027d60`); the owner's alert 0x201
    ///    ([`alert_owner`]); the attacker joins the set (`FUN_140b43f30`) and the unit is under
    ///    attack (`FUN_14119d2d0` with 1). At no hit points left ("I'm dead, I give up.") the
    ///    owner's alert 0x209, the notifies end (`FUN_140c5dc10`) and it returns 4.
    /// 3. It ignores (2) an attacker the world no longer holds or of a player its owner counts as
    ///    its own or an ally. A new attacker that is not its target makes a human player's unit
    ///    outside a group that is not idle ask for help (`AskForHelp`, [`UnitAi::ask_for_help`]).
    ///    It ignores (2): its attack on the attacker; a human player's unit's order other than
    ///    the patrols (711, 733), or its group that is not idle; a computer player's unit on the
    ///    explore order (0x2c1) attacked by another player than Gaia.
    /// 4. `bestUnitToAttack` once per update ([`UnitAi::best_this_update`], `+0x1d0`): from the
    ///    current target's place (slot 0x520) while the world holds it, else its own; no
    ///    reference distance, no idle check, no path checks, the attacker as its extra object,
    ///    any class, no attack work state. With none and no target, order or action: a unit that
    ///    does not move (its top speed below [`STILL`]) ignores it (2), another runs from its
    ///    attackers (slot 0x160 with 1, [`UnitAi::run_away`]): 3 when it moves, else 2. Anything
    ///    but the attacker is ignored (2).
    /// 5. Its reach to the attacker within its weapon range (slot 0x3d0 with slot 0x230). Without
    ///    a current target the world holds: reached, it attacks back ([`UnitAi::strike_back`]);
    ///    else off the move action it runs from its attackers; 3 either way. With one: not
    ///    reached, 2; a human player's unit attacks back from a building, wall or tower target
    ///    (classes 3, 27, 52; 3), else 2.
    /// 6. A computer player's unit ignores it (2) on a wonder target (master 276), a tower
    ///    attacked by a tower (class 52), a missile attacker (type 60), and a tower attacker
    ///    under its owner's `disableTowerPriority` but for a siege weapon, unpacked trebuchet or
    ///    warship (classes 13, 54, 22). Then ([`UnitSide::duel`]) a target that cannot convert it
    ///    (its slot 0x1b8 with 1) but cannot harm it either (its attacks' sum, slot 0x228, or its
    ///    damage to the unit, the unit's slot 0x220, exactly 0) makes it ask for help and attack
    ///    back; and then still, as long as the target cannot convert it, an attacker that would
    ///    die sooner (its hit points over the unit's damage to it, `UnitAiTuning::never_killed` for none:
    ///    `COMISS`, `JA`) or could convert it makes it attack back. It returns 2.
    pub(super) fn damaged(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let known = self.attackers.contains(n.caller);
        self.retries.clear();
        if known && side.top_target_now()?.is_some() {
            return Ok(GO_ON);
        }
        // slot 0x48's notify and the alert 0x201 reach the same owner
        alert_owner(player_kind)?;
        let _ = self.attackers.add(n.caller);
        side.set_under_attack(true);
        if n.p[1] < 1 {
            self.end_notifies(time, at, side, ai_rng)?;
            return Ok(DEAD);
        }
        let Some(attacker) = Self::world_object(n.caller, at, side)? else {
            return Ok(GO_ON);
        };
        let attacker_id = attacker.id;
        if attacker.allied {
            return Ok(GO_ON);
        }
        // `IsIdle` (FUN_141017ad0) of the unit's group, read at the handler's start
        let held_by_group = at.group.is_some_and(|g| !g.idle);
        if !known
            && !player_kind.is_computer()
            && self.current_target != Some(attacker_id)
            && !held_by_group
        {
            self.ask_for_help(attacker_id, at, player_kind, side)?;
        }
        if self.ignores_attacker(&attacker, attacker_id, player_kind, held_by_group) {
            return Ok(GO_ON);
        }
        let current = Self::world_object(self.current_target, at, side)?;
        if self.best_this_update.is_none() {
            self.best_against(attacker_id, current.as_ref(), at, player_kind, side)?;
        }
        if self.best_this_update.is_none()
            && self.current_target.is_none()
            && self.current_order.is_none()
            && self.current_action.is_none()
        {
            // "BUTA returned -1 and I'm idle without a target": COMISS, JBE (a NaN runs)
            if at.speed < Speed::new(STILL) {
                return Ok(GO_ON);
            }
            let ran = self.run_away(OrderForce::Forced, at, player_kind, ai_rng, side)?;
            return Ok(if ran { BREAK } else { GO_ON });
        }
        if self.best_this_update != Some(attacker_id) {
            return Ok(GO_ON);
        }
        self.answer_attacker(&attacker, current, at, player_kind, ai_rng, side)
    }

    /// [`UnitAi::damaged`]'s step 3 after the call for help: true when the unit ignores
    /// `attacker` (`attacker_id`): its attack on it already; a human player's unit's order other
    /// than the patrols (711, 733), or its group that is not idle (`held_by_group`); a computer
    /// player's unit on the explore order (0x2c1) attacked by another player than Gaia.
    pub(super) fn ignores_attacker(
        &self,
        attacker: &UnitSeen,
        attacker_id: ObjectId,
        player_kind: PlayerKind,
        held_by_group: bool,
    ) -> bool {
        if matches!(self.current_action, Some(AiAction::Attack))
            && self.current_target == Some(attacker_id)
        {
            return true;
        }
        if !player_kind.is_computer()
            && !matches!(
                self.current_order,
                None | Some(OrderKind::Patrol | OrderKind::AttackMove)
            )
        {
            return true;
        }
        if !player_kind.is_computer() && held_by_group {
            return true;
        }
        player_kind.is_computer()
            && matches!(self.current_order, Some(OrderKind::Explore))
            && attacker.owner != PlayerId::GAIA
    }

    /// [`UnitAi::damaged`]'s step 4's search, `bestUnitToAttack` into
    /// [`UnitAi::best_this_update`]: from the `current` target's place while the world holds it,
    /// else its own; no reference distance, no idle check, no path checks, `attacker_id` as its
    /// extra object, any class, no attack work state.
    pub(super) fn best_against(
        &mut self,
        attacker_id: ObjectId,
        current: Option<&UnitSeen>,
        at: &Surroundings,
        player_kind: PlayerKind,
        side: &mut dyn UnitSide,
    ) -> Result<(), GenieError> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        let from = current.map_or(at.position, |t| t.position);
        let (classes, radius) = self.search_area(at, player_kind)?;
        let found = side.closest(&Search {
            at: from.ground(),
            radius,
            classes,
            size: at.size,
            pick: Pick::Attack {
                lowest: if at.scenario && !player_kind.is_computer() {
                    2
                } else {
                    0
                },
                seeker: self.seeker(at),
                ask: AttackAsk {
                    distance: Distance::ZERO,
                    idle: false,
                    paths: false,
                    extra: Some(attacker_id),
                    any_class: true,
                    work_state: false,
                },
            },
            memory: self.memory(),
        })?;
        self.add_retries(&found);
        self.attack_search_alert(found, player_kind, unit_ai_tuning)?;
        self.best_this_update = found.id;
        Ok(())
    }

    /// [`UnitAi::damaged`]'s step 5, for the `attacker` `bestUnitToAttack` gave: its reach within
    /// the weapon range. Without a `current` target the world holds: reached, it attacks back;
    /// else off the move action it runs from its attackers; 3 either way. With one: not reached,
    /// 2; a human player's unit attacks back from a building, wall or tower target (classes 3,
    /// 27, 52; 3), else 2; a computer player's unit weighs it ([`UnitAi::computer_weighs`]).
    pub(super) fn answer_attacker(
        &mut self,
        attacker: &UnitSeen,
        current: Option<UnitSeen>,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let roles = &at.roles;
        let attacker_id = attacker.id;
        let reaches = side.reaches(attacker_id, at.weapon_range)?;
        let Some(target) = current else {
            if reaches {
                return self.strike_back(attacker_id, at, ai_rng, side);
            }
            if self.current_action != Some(AiAction::Move) {
                let _ = self.run_away(OrderForce::Forced, at, player_kind, ai_rng, side)?;
            }
            return Ok(BREAK);
        };
        if !reaches {
            return Ok(GO_ON);
        }
        if !player_kind.is_computer() {
            if roles
                .of_class(target.class)
                .intersects(ClassRoles::BUILDING | ClassRoles::WALL | ClassRoles::TOWER)
            {
                return self.strike_back(attacker_id, at, ai_rng, side);
            }
            return Ok(GO_ON);
        }
        self.computer_weighs(attacker, &target, at, ai_rng, side)
    }

    /// The damage case's end for a computer player's unit ([`UnitAi::damaged`]'s step 6) that
    /// reaches `attacker` while the world holds its current `target`.
    pub(super) fn computer_weighs(
        &mut self,
        attacker: &UnitSeen,
        target: &UnitSeen,
        at: &Surroundings,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        let roles = &at.roles;
        let attacker_id = attacker.id;
        if roles.of_master(target.master).contains(MasterRoles::WONDER)
            || (roles.of_class(target.class).contains(ClassRoles::TOWER)
                && roles.of_class(attacker.class).contains(ClassRoles::TOWER))
            || attacker.kind == MasterKind::MISSILE
            || (roles.of_class(attacker.class).contains(ClassRoles::TOWER)
                && !roles.of_some_class(at.class).intersects(
                    ClassRoles::SIEGE | ClassRoles::UNPACKED_TREBUCHET | ClassRoles::WARSHIP,
                )
                && at.disable_tower_priority)
        {
            return Ok(GO_ON);
        }
        let t = side.duel(target.id)?;
        // "my current target cannot damage me": UCOMISS, JP, JNZ (exactly 0)
        if (t.attacks == 0.0 || t.harms == HitPoints::ZERO) && !t.converts {
            if matches!(
                self.current_order,
                Some(OrderKind::Patrol | OrderKind::AttackMove)
            ) {
                self.save_patrol(side);
            }
            self.ask_for_help(attacker_id, at, PlayerKind::Computer, side)?;
            if self.attack_slot(attacker_id, TargetRules::Any, at, side, ai_rng)? {
                self.leave_patrol_timers(ai_rng);
                self.current_order = None;
            }
        }
        let target_time = if HitPoints::ZERO < t.harmed {
            t.hp / t.harmed
        } else {
            unit_ai_tuning.never_killed
        };
        let a = side.duel(attacker_id)?;
        let attacker_time = if HitPoints::ZERO < a.harmed {
            a.hp / a.harmed
        } else {
            unit_ai_tuning.never_killed
        };
        // "my current target will take longer to kill"
        if !t.converts && (attacker_time < target_time || a.converts) {
            let _ = self.strike_back(attacker_id, at, ai_rng, side)?;
        }
        Ok(GO_ON)
    }

    /// The damage case's attack back on `attacker` (`FUN_140c589c0`): a unit on a patrol order
    /// (711, 733) saves its patrol first (`savePatrolInformation`, [`UnitAi::save_patrol`]);
    /// `AttackObject` (slot 0xb8, any: [`UnitAi::attack_slot`]); when it takes, a patrol order
    /// lets its timers go ([`UnitAi::leave_patrol_timers`]) and the order is −1. It answers 3.
    pub(super) fn strike_back(
        &mut self,
        attacker: ObjectId,
        at: &Surroundings,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        if matches!(
            self.current_order,
            Some(OrderKind::Patrol | OrderKind::AttackMove)
        ) {
            self.save_patrol(side);
        }
        if self.attack_slot(attacker, TargetRules::Any, at, side, ai_rng)? {
            self.leave_patrol_timers(ai_rng);
            self.current_order = None;
        }
        Ok(BREAK)
    }

    /// The base handler's case 0x215 ([`IN_THE_WAY`]): a unit not of a boar's master (class 10
    /// of combat level 1, `FUN_140fde4f0`) attacks the unit in its way (`AttackObject`, slot
    /// 0xb8, forced, its slot 0xa0 unforced: [`UnitAi::attack_slot`]) when the world's table
    /// holds it, retargeting is on (`+0x1d6` 0), its stance (`+0x1a9`) is 0 and it has no order;
    /// the other unit's group (`+0x6d0`), which would let it attack with one, the slice does not
    /// hold. Processing goes on.
    ///
    /// [`IN_THE_WAY`]: crate::unit_ai::NotifyKind::IN_THE_WAY
    pub(super) fn in_the_way(
        &mut self,
        n: Notify,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<u32, GenieError> {
        let roles = &at.roles;
        let boar = roles.of_some_class(at.class).contains(ClassRoles::PREDATOR)
            && at.combat_level == CombatLevel::ANIMAL;
        // p1 the unit met (−1 names none the update holds)
        if !boar
            && let Some(met) = ObjectId::of_field(n.p[0])
            && at.unit(met).is_some()
            && self.retarget_disabled == 0
            && self.current_order.is_none()
            && self.stance == UnitStance::Aggressive
        {
            // slot 0xb8 forced, by the type
            let _ = self.attack_slot(met, TargetRules::Own, at, side, rng)?;
        }
        Ok(GO_ON)
    }

    /// The base handler's case 0x216 ([`SENT_OFF`]): the closest spot beside the foundation's
    /// footprint (`FUN_1411566b0` with the notify's radii, 0) takes the unit's move off it (slot
    /// 0x148 forced, range 0, walking unless it is prey: the unit's slot 0x340, then the action
    /// value 0x2bf, the old one the last). A human player's villager whose current action has a
    /// target then queues the foundation's order in front (`FUN_140c42640` with 1: order 0x2de,
    /// priority 100, ready) and the work on that target behind (0x2db); the orders' points are
    /// the objects' slot 0x520 vectors' first two parts (x and the height) and 0, as the game
    /// passes them. Processing goes on.
    ///
    /// [`SENT_OFF`]: crate::unit_ai::NotifyKind::SENT_OFF
    pub(super) fn sent_off(
        &mut self,
        n: Notify,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
        roles: &Roles,
        unit_ai_tuning: &UnitAiTuning,
    ) -> Result<u32, GenieError> {
        let radius = Footprint::new(
            n.p[0] as f32 / unit_ai_tuning.radius_scale,
            n.p[1] as f32 / unit_ai_tuning.radius_scale,
        );
        // p3 the foundation: none (−1) has no footprint, so the unit has nowhere to go
        let Some(site) = FoundationId::of_field(n.p[2]) else {
            return Ok(GO_ON);
        };
        let off = side.off_footprint(site, radius)?;
        if let Some(spot) = off.spot {
            let _ = self.move_aside(
                AiMove {
                    point: spot.with_height(off.z),
                    range: Distance::ZERO,
                    force: OrderForce::Forced,
                    // The prey animals' class, whose move off a footprint runs (slot 0x148's seventh argument).
                    // The villagers' class.
                    walk: !roles.of_class(off.class).contains(ClassRoles::PREY),
                },
                side,
                rng,
            )?;
        }
        if !off.player_kind.is_computer()
            // The villager class (`-0xea8` raw): a boar attacking one turns furious.
            && roles.of_class(off.class).contains(ClassRoles::VILLAGER)
            && let (Some(site), Some(target)) = (off.site, off.action_target)
        {
            let order = |o: &Named, order: OrderKind| QueuedOrder {
                issuer: Some(Issuer::Player(off.player)),
                order,
                priority: OrderPriority::COMMAND,
                target: Some(o.id),
                target_owner: Some(o.owner),
                point: o.vector.with_height(0.0),
                range: Distance::ZERO,
                shown: o.vector,
                ready: true,
                target_kind: Some(OrderTargetKind::of_master(o.master)),
            };
            self.insert_order(order(&site, OrderKind::Foundation), QueueEnd::Front);
            self.insert_order(order(&target, OrderKind::OnTarget), QueueEnd::Back);
        }
        Ok(GO_ON)
    }

    /// Slot 0x148 (`FUN_140c4d0d0`), the move off a footprint: with its force flag (`m.force`)
    /// or without an action value, the unit's slot 0x340 takes the move with the same
    /// arguments ([`AiCommand::MoveAside`], which the caller runs after the update); on its 1 the
    /// action value becomes [`AiAction::MoveAside`], the old one the last, and the target stays.
    /// The caller's slot 0x340 is `FUN_1410866e0`, which always gives 1, but for classes 51 and
    /// 54 (`FUN_141216c50`, their packing), which it does not run. The move gives the unit its
    /// `MoveTo` at once: the update's later checks see it busy. True when it moves.
    pub(super) fn move_aside(
        &mut self,
        m: AiMove,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<bool, GenieError> {
        if m.force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(false);
        }
        side.effect_now(Effect::Command(AiCommand::MoveAside(m)), self, rng)?;
        self.set_action(Some(AiAction::MoveAside));
        Ok(true)
    }

    /// The action values of an explorer (the base handler's case 0x202, a bit test of the action
    /// less 605): 605, 623 and 630, and the auto scout's `0x2bd`.
    pub(super) const fn explorer(&self) -> bool {
        matches!(
            self.current_action,
            Some(
                AiAction::Explore
                    | AiAction::ExploreAndDestroy
                    | AiAction::Scout
                    | AiAction::AutoScout
            )
        )
    }

    /// The base handler's case 0x202 for an explorer ("I'm an explorer, looking for the closest
    /// undiscovered tile."): on the auto scout's action, with the player's scout map never made
    /// (`player+0x1370`), the search around the town centre (`FUN_140c65490`), else the unit AI's
    /// slot 0x90 (`FUN_140c4bd40`), both from its unit ([`UnitSide::seek_unexplored`]); then slot
    /// 0x100 with 1 to the point ([`UnitAi::explore_at`]), the explore's action value becomes the
    /// last and the action value comes back; processing breaks. "Couldn't find any tiles.": on
    /// the auto scout order its last search ([`UnitSide::seek_beyond_sight`]) sends the auto scout
    /// to its point the same way (slot 0x100 with 1; the last action stays) and processing breaks;
    /// with no point left the player's scout map is made from the tiles it sees and the notify
    /// posted to the owner (nothing for a human player), [`UnitSide::make_scout_map`]; with the
    /// map made the search about the town centre is skipped and the last search reads the map.
    /// On the scout order (`0x2da`) the player's slot 0x2e8
    /// with `0x211` is not modelled; on any other the unit AI forgets its target
    /// (`FUN_140c5e1e0`) and stops (slot 0xb0 with 1; slot 0x48 hands the notify to the owner,
    /// not held; the interface's flag a last auto scout order sets is not the sim's) and
    /// processing goes on. The other explorers' search is not modelled.
    pub(super) fn seek_unexplored(
        &mut self,
        n: Notify,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        let action = self.current_action;
        let auto = action == Some(AiAction::AutoScout);
        // a computer player's unit AI finds no tile (slot 0x90 gives 0 without a draw)
        if !auto && !player_kind.is_computer() {
            closest_undiscovered_tile()?;
        }
        let found = if player_kind.is_computer() && !auto {
            None
        } else {
            side.seek_unexplored(self, ai_rng)?
        };
        if let Some(t) = found {
            let _ = self.explore_at(t, OrderForce::Forced, side, ai_rng, unit_ai_tuning)?;
            self.last_action = self.current_action;
            self.current_action = action;
            return Ok(BREAK);
        }
        match self.current_order {
            Some(OrderKind::Scout) => Err(GenieError::unsupported(
                "a scout that finds no undiscovered tile (the player's slot 0x2e8 with 0x211)",
            )),
            Some(OrderKind::AutoScout) => {
                let Some(t) = side.seek_beyond_sight(ai_rng)? else {
                    // the player's scout map, from the tiles it sees; on the auto scout's action
                    // the AI's slot 0x40 hands the notify to the owner (slot 0x2e8 with 0x202,
                    // FUN_14162dab0: no case for it but the old short pather's hulled area, a
                    // computer player's AI would hear it); processing breaks
                    side.make_scout_map()?;
                    if player_kind.is_computer() && self.current_action == Some(AiAction::AutoScout)
                    {
                        return Err(GenieError::unsupported(
                            "a computer player's AI told its scout found nothing (0x202)",
                        ));
                    }
                    return Ok(BREAK);
                };
                if self.current_action == Some(AiAction::AutoScout) {
                    let _ = self.explore_at(t, OrderForce::Forced, side, ai_rng, unit_ai_tuning)?;
                }
                self.current_action = action;
                Ok(BREAK)
            }
            _ => {
                self.remove_target();
                self.stop_current(StopOrder::Keep, at, side, ai_rng)?;
                side.owner_hears(n)?;
                Ok(GO_ON)
            }
        }
    }
}
