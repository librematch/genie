//! The flight of the hunted animals, the boar and the villagers, and their calls for help.

use crate::GenieError;
use crate::construction::FoundationId;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::{at_most_or_unordered, sqrt};
use crate::math::rng::Well512;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::AiAction;
use crate::unit_ai::Answer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderForce;
use crate::unit_ai::Pace;
use crate::unit_ai::StopOrder;
use crate::unit_ai::TargetRules;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{
    AiMove, BREAK, DEAD, Effect, GO_ON, Memory, Notify, Pick, RUN_DRAW_SCALE, RUN_EPSILON, RUN_ONE,
    Search, Surroundings, UnitAi, UnitSide, VILLAGER_NOTIFY, alert_owner,
    closest_undiscovered_tile, search_area,
};
use crate::world::Millis;
use crate::world::layer::ClassRoles;

/// What `evasiveMoveTo` (`FUN_140c4d140`'s `tries`) does with a point the unit could not stand at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BadPoint {
    /// It moves, five times at most (1).
    Moves,
    /// It stays (0).
    Kept,
}

impl UnitAi {
    /// The hunted animal's damage notify (`FUN_140c26420`, case 500, "I took some damage from
    /// Unit #%d"): at no hit points left (`p[1]` below 1) it gives up: the notifies end
    /// (`FUN_140c5dc10`) and it returns 4. Else the attacker joins its attackers
    /// (`FUN_140b43f30`), the unit is under attack (`FUN_14119d2d0` with 1) and it runs from its
    /// attackers ([`UnitAi::run_away`]), unconditionally unless its action is 0x25f or 0x268 or
    /// its class 59; it returns 3. An animal that is not Gaia's first alerts its owner (slot
    /// 0x2e8 with 0x201): a human player's alert list, not held; a computer player's AI, not
    /// modelled.
    pub(super) fn hunted_damaged(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let roles = &at.roles;
        if n.p[1] < 1 {
            self.end_notifies(time, at, side, ai_rng)?;
            return Ok(DEAD);
        }
        // an animal that is not Gaia's alerts its owner (slot 0x2e8 with 0x201)
        if at.player != PlayerId::GAIA {
            alert_owner(player_kind)?;
        }
        let _ = self.attackers.add(n.caller);
        side.set_under_attack(true);
        // the game also tests the action values 0x25f and 0x268 here, which no unit AI holds
        // ([`AiAction`])
        // The class of the king's branch of the hunted animal's idle check (`-0xe71` raw).
        let force = if roles.of_some_class(at.class).contains(ClassRoles::KING) {
            OrderForce::Unforced
        } else {
            OrderForce::Forced
        };
        let _ = self.run_away(force, at, player_kind, ai_rng, side)?;
        Ok(BREAK)
    }

    /// `runAwayFromAttackers` (`FUN_140c5e900`) with no unit to run from but its attackers: a unit
    /// in stand ground or no attack (stances 2 and 3) of a player other than Gaia stays, and so
    /// does one with an action value unless forced (`uncon`). Else it runs away from the sum of the
    /// vectors to its attackers the world holds (a random one when they cancel out: two draws of
    /// the AI stream), by twice its speed, a defensive unit of a player at most 5 tiles from its
    /// state position, through `evasiveMoveTo` with both its flags ([`UnitAi::evasive_move`]). A
    /// computer's unit attacked by class 22 (its group's runs) and a computer's transport ship are
    /// not modelled; neither are unit groups (`+0x6d0`). Taken when the unit moves.
    pub(super) fn run_away(
        &mut self,
        force: OrderForce,
        at: &Surroundings,
        player_kind: PlayerKind,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<Answer, GenieError> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        let roles = &at.roles;
        // the siege unit's slot 0x160 (FUN_140c2a8f0): with a top action's target or of class 54
        // it does not run; a computer player's first lets the units inside it go
        // (FUN_141225c40 with −1, −1)
        if self.ai_kind == UnitAiKind::Siege {
            if roles
                .of_some_class(at.class)
                .contains(ClassRoles::UNPACKED_TREBUCHET)
                || Self::top_target_live(at, side)?.is_some()
            {
                return Ok(Answer::NotTaken);
            }
            if player_kind.is_computer() {
                return Err(GenieError::unsupported(
                    "a computer player's siege unit running away (FUN_141225c40)",
                ));
            }
        }
        if matches!(self.stance, UnitStance::StandGround | UnitStance::NoAttack)
            && at.player != PlayerId::GAIA
        {
            return Ok(Answer::NotTaken);
        }
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        let (x, y, z) = (at.position.x(), at.position.y(), at.position.z());
        let (mut vx, mut vy) = (0.0f32, 0.0f32);
        let mut class_22 = false;
        // none (−1) is an object the update does not hold
        for id in self.attackers.iter().flatten() {
            let Some(u) = at.unit(id) else {
                continue;
            };
            vx += u.position.x() - x;
            vy += u.position.y() - y;
            // The class whose attackers make a computer's unit warn its group (`FUN_140c5e900`: 22).
            class_22 |= roles.of_class(u.class).contains(ClassRoles::WARSHIP);
        }
        if player_kind.is_computer() && class_22 && Speed::ZERO < at.speed {
            return Err(GenieError::unsupported(
                "a computer's unit running from class 22 (its group's runs)",
            ));
        }
        if player_kind.is_computer()
            && roles
                .of_some_class(at.class)
                .contains(ClassRoles::TRANSPORT)
        {
            return Err(GenieError::unsupported(
                "a computer's transport ship running away",
            ));
        }
        let reach = (at.speed * unit_ai_tuning.run_reach).get();
        let mut d = sqrt(vy * vy + vx * vx);
        if d < RUN_EPSILON {
            let r = ai.rand15() as i32 as f32;
            vx = (r + r) / RUN_DRAW_SCALE - RUN_ONE;
            let r = ai.rand15() as i32 as f32;
            vy = (r + r) / RUN_DRAW_SCALE - RUN_ONE;
            d = sqrt(vy * vy + vx * vx);
        }
        let mut ex = x - (vx / d) * reach;
        let mut ey = y - (vy / d) * reach;
        if at.player != PlayerId::GAIA && self.stance == UnitStance::Defensive {
            let gx = self.state_position.x() - ex;
            let gy = self.state_position.y() - ey;
            if unit_ai_tuning.run_leash < sqrt(gy * gy + gx * gx) {
                ex = self.state_position.x() - (vx / d) * unit_ai_tuning.run_leash;
                ey = self.state_position.y() - (vy / d) * unit_ai_tuning.run_leash;
            }
        }
        self.evasive_move(
            Position::new(ex, ey, z),
            (OrderForce::Forced, BadPoint::Moves),
            at,
            ai,
            side,
        )
    }

    /// `evasiveMoveTo` (the unit AI's slot 0x150, `FUN_140c4d140`) to `point` with its flags
    /// `(uncon, tries)` (`force`, `bad`): unforced a unit with an action value stays. A coordinate
    /// that is not a number becomes the unit's own plus one (minus one above 5); the point is held
    /// on the map (below 0 to 0, at the far edge one tile in). With [`BadPoint::Moves`], a point
    /// the unit could not stand at (its slot 0x278) moves by up to 3 on each axis, five times at
    /// most, each move four draws of the AI stream (the x sign, its amount, the y sign, its
    /// amount), held on the map again. Then the unit's move there (slot 0x340, forced, running,
    /// range 0): the AI holds the move action and the point without a target. Taken when it moves.
    pub(super) fn evasive_move(
        &mut self,
        point: Position,
        (force, bad): (OrderForce, BadPoint),
        at: &Surroundings,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<Answer, GenieError> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        if force == OrderForce::Unforced && self.current_action.is_some() {
            return Ok(Answer::NotTaken);
        }
        let near = |p: f32, own: f32| {
            if !p.is_nan() {
                p
            } else if at_most_or_unordered(own, unit_ai_tuning.evasive_near) {
                // COMISS own, 5.0; JBE
                own + RUN_ONE
            } else {
                own - RUN_ONE
            }
        };
        let held = |v: f32, size: i32| {
            if v < 0.0 {
                0.0
            } else if size as f32 <= v {
                size as f32 - unit_ai_tuning.evade_edge
            } else {
                v
            }
        };
        let mut x = held(near(point.x(), at.position.x()), at.map.width());
        let mut y = held(near(point.y(), at.position.y()), at.map.height());
        if bad == BadPoint::Moves {
            for _ in 0..5 {
                if side.can_stand(Point::new(x, y))? {
                    break;
                }
                let step = |ai: &mut Well512, v: f32| {
                    let up = ai.rand15() & 1 != 0;
                    let by = (ai.rand15() & 3) as f32;
                    if up { v + by } else { v - by }
                };
                x = step(ai, x);
                y = step(ai, y);
                x = held(x, at.map.width());
                y = held(y, at.map.height());
            }
        }
        let m = self.move_to(AiMove {
            point: Position::new(x, y, point.z()),
            range: Distance::ZERO,
            force: OrderForce::Forced,
            pace: Pace::Run,
        });
        if let Some(m) = m {
            side.effect_now(Effect::Move(m), self, ai)?;
        }
        // the move (slot 0x340) gives the unit its MoveTo at once: the update's later checks see
        // it busy
        Ok(Answer::Taken)
    }

    /// The boar's damage notify (`FUN_140c25770`, case 500): at no hit points left
    /// (`p[1]` below 1) it gives up: the notifies end (`FUN_140c5dc10`) and it returns 4. Else
    /// the attacker joins its attackers (`FUN_140b43f30`), the unit is under attack
    /// (`FUN_14119d2d0` with 1) and it attacks its closest valid attacker
    /// ([`UnitAi::closest_valid_attacker`]) through `AttackObject` (slot 0xb8,
    /// [`UnitAi::attack_object`]): not forced while its action is the attack (600), which then
    /// fails, so a boar keeps attacking the unit it took first; else forced, the furious flag
    /// (`+0x1fc`) becomes whether that attacker is a villager and the idle timer (`+0x160`)
    /// restarts. With an attacker it returns 3, else 2.
    pub(super) fn boar_damaged(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let roles = &at.roles;
        if n.p[1] < 1 {
            self.end_notifies(time, at, side, ai_rng)?;
            return Ok(DEAD);
        }
        let _ = self.attackers.add(n.caller);
        side.set_under_attack(true);
        let Some((target, attacker)) = self
            .closest_valid_attacker(at, ai_rng)
            .and_then(|t| at.unit(t).map(|u| (t, u)))
        else {
            return Ok(GO_ON);
        };
        let villager = roles
            .of_class(attacker.class)
            .contains(ClassRoles::VILLAGER);
        let furious = if self.current_action == Some(AiAction::Attack) {
            let _ = self.attack_object(target, OrderForce::Unforced, TargetRules::Own, at, side)?;
            true
        } else {
            let _ = self.attack_object(target, OrderForce::Forced, TargetRules::Own, at, side)?;
            self.idle_timer = Millis::ZERO;
            villager
        };
        let home = self.home.as_mut().ok_or(GenieError::unsupported(
            "a boar's unit AI without its own part",
        ))?;
        home.furious = furious;
        Ok(BREAK)
    }

    /// `AskForHelp` (`FUN_140c42ce0`) of a unit attacked by a new `attacker`: without an attacker
    /// the world holds (`FUN_1415417a0`) nothing; else its search from its place (slot 0x50,
    /// `FUN_140c63e20` with mode 1: the classes it cares about; a search
    /// that ran sets the alert mode, [`UnitAi::attack_search_alert`]), whose lists of its own and
    /// its friends' objects ([`UnitSide::call_for_help`]) it walks for helpers.
    ///
    /// # Errors
    ///
    /// A helper the slice does not model ([`helper_answers`]), and what the search does not.
    ///
    /// [`helper_answers`]: crate::unit_ai::helper_answers
    pub(super) fn ask_for_help(
        &mut self,
        attacker: ObjectId,
        at: &Surroundings,
        player_kind: PlayerKind,
        side: &mut dyn UnitSide,
    ) -> Result<(), GenieError> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        if side.seen_unit(attacker)?.is_none() {
            return Ok(());
        }
        let (classes, radius) = self.search_area(at, player_kind);
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius,
            classes,
            size: at.size,
            pick: Pick::Look,
            memory: Memory::default(),
        })?;
        self.attack_search_alert(found, player_kind, unit_ai_tuning)?;
        side.call_for_help(attacker)
    }

    /// What a search of the unit AI covers (`FUN_140c63e20`): the classes its constructor gave
    /// it ([`cared_classes`]), within its search radius, the line of sight plus 1 if that is
    /// farther, plus its growth (`+0x1e8`, `FUN_140c2e080`'s fourth argument: 7 for the villager
    /// and the fishing ship, 0 for the others).
    ///
    /// [`cared_classes`]: crate::unit_ai::cared::cared_classes
    pub(super) fn search_area(
        &self,
        at: &Surroundings,
        player_kind: PlayerKind,
    ) -> (&'static [ClassId], Distance) {
        let roles = &at.roles;
        search_area(
            self.ai_kind,
            at.class,
            at.los,
            at.search_radius,
            player_kind,
            roles,
        )
    }

    /// The villager's cases 0x1fb, 0x1fc and 0x202 off the attack action (`FUN_140c22bf0`,
    /// "Action %d has been invalidated or requires a search."): on the hunt action the
    /// hunter's search ([`UnitAi::hunter_search`]), on the gather action the gatherer's
    /// ([`UnitAi::gatherer_search`]); on the build action the builder looks for another
    /// foundation (`FUN_140c1c7c0`, when the player's `+0x613` is 0: unsaved, 0 in this game):
    /// one the unit reaches within 0.1 (slot 0x3d0) it builds (slot 0xf0 with 1) and the
    /// processing breaks; finding none, or not reaching it, a human player's villager stops
    /// (slot 0x3c0) and the processing breaks. On the explore action the explorer looks for the
    /// closest undiscovered tile ([`closest_undiscovered_tile`]). The shepherd's search (action
    /// `0x278`: `ClosestObject` for livestock, then slot 600) and the search for the class of
    /// an invalidated gather or hunt (`p[0]` `0x261` or `0x265`, `p[1]` not 10) are not
    /// modelled. Else any notify but 0x202, off the actions `0x278` and `0x26a`, makes the
    /// target go (`FUN_140c5e1e0`) and the action stop (slot 0xb0 with 1; slot 0x48 hands the
    /// notify to the owner, not held); it returns 2.
    pub(super) fn villager_search(
        &mut self,
        n: Notify,
        at: &Surroundings,
        player_kind: PlayerKind,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<u32, GenieError> {
        let unit_ai_tuning = &at.unit_ai_tuning;
        match self.current_action {
            Some(AiAction::Hunt) => {
                return self.hunter_search(n, at, player_kind, side, rng);
            }
            Some(AiAction::Shepherd) => {
                return Err(VILLAGER_NOTIFY.with("action", AiAction::Shepherd.value()));
            }
            Some(AiAction::Gather) => {
                return self.gatherer_search(n, at, player_kind, side, rng);
            }
            Some(AiAction::Build) => {
                if let Some(found) = side.foundation_search()?
                    && side.reaches(found, Distance::new(unit_ai_tuning.foundation_reach))?
                {
                    let _ = self.build_object(
                        FoundationId::new(found),
                        OrderForce::Forced,
                        at,
                        side,
                        rng,
                    )?;
                    return Ok(BREAK);
                }
                if player_kind.is_computer() {
                    return Err(GenieError::unsupported(
                        "a computer builder without a foundation",
                    ));
                }
                side.effect_now(Effect::Stop, self, rng)?;
                return Ok(BREAK);
            }
            Some(AiAction::Explore) => closest_undiscovered_tile()?,
            _ => {}
        }
        if (n.p[0] == AiAction::Gather.value() || n.p[0] == AiAction::Hunt.value()) && n.p[1] != 10
        {
            return Err(VILLAGER_NOTIFY.with("notify", format!("{:#x} {:?}", n.kind, n.p)));
        }
        if n.kind != NotifyKind::REQUIRES_SEARCH
            && !matches!(
                self.current_action,
                Some(AiAction::Shepherd | AiAction::Repair)
            )
        {
            self.remove_target();
            self.stop_current(StopOrder::Keep, at, side, rng)?;
        }
        Ok(GO_ON)
    }
}
