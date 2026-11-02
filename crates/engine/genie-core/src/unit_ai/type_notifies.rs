//! Each unit-AI type's notify handler (slot 0x238): the animals', the villager's, the ships',
//! the siege's, the soldier's, the tower's and the building's.

use crate::GenieError;
use crate::math::fmath::below_or_unordered;
use crate::math::rng::Well512;
use crate::movement::footprint_gap_squared;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ObjectFlags;
use crate::players::PlayerType;
use crate::unit_ai::ActionKind;
use crate::unit_ai::AiCommand;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::StopOrder;
use crate::unit_ai::{
    BREAK, DEAD, Effect, GO_ON, LION_ATTACKED_TIMEOUT, Notify, PACKED_UNIT_CLASS, Surroundings,
    UnitAi, UnitSide, VILLAGER_NOTIFY, alert_owner, back_up, dodges,
};
use crate::world::Millis;

impl UnitAi {
    /// The hunted animal's handler (`TribeHuntedAnimalUnitAIModule`, `FUN_140c26420`): its
    /// damage notify evades ([`UnitAi::hunted_damaged`]), 0x20f goes on; any other notify is the
    /// base handler's.
    pub(super) fn hunted_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::DAMAGED => self.hunted_damaged(n, time, at, player_type, ai_rng, side),
            NotifyKind::MISSILE_WARNING => Ok(GO_ON),
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// The boar's handler (`FUN_140c25770`): its damage notify attacks
    /// ([`UnitAi::boar_damaged`]); a completion while it moves off a footprint (0x2bf) makes its
    /// idle timeout the adjusted one plus 1, drops the target (`FUN_140c5e1e0`) and sends it home
    /// on its next idle check (slot 0x1f0, `FUN_140c16d50`: `+0x1fd`), then goes to the base
    /// handler; 0x20f goes on.
    pub(super) fn boar_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::DAMAGED => self.boar_damaged(n, time, at, ai_rng, side),
            NotifyKind::ACTION_COMPLETED => {
                if self.current_action == Some(ActionKind::MoveAside) {
                    self.idle_timeout = self.adjusted_idle_timeout.wrapping_add(Millis::new(1));
                    self.remove_target();
                    if let Some(home) = self.home.as_mut() {
                        home.go_home = true;
                    }
                }
                self.base_notify(n, time, at, player_type, ai_rng, side)
            }
            NotifyKind::MISSILE_WARNING => Ok(GO_ON),
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// The lion's handler (`FUN_140c266f0`): its completed attack sets its idle timeout
    /// (`FUN_140c41bc0` with 1000, then slot 0x1a8), drops the target and stops; a moved target
    /// is [`UnitAi::lion_target_moved`]'s. Both cases, and any other notify, go on to the base
    /// handler.
    pub(super) fn lion_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::ACTION_COMPLETED => {
                if n.p[0] == ActionKind::Attack.value() {
                    // FUN_140c41bc0 with 1000: the idle timeout, then slot 0x1a8
                    self.set_idle_timeout(LION_ATTACKED_TIMEOUT, ai_rng);
                    self.remove_target();
                    self.stop_current(StopOrder::Keep, at, side, ai_rng)?;
                }
                self.base_notify(n, time, at, player_type, ai_rng, side)
            }
            NotifyKind::TARGET_MOVED => {
                self.lion_target_moved(n, time, at, player_type, ai_rng, side)
            }
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// The lion's moved target: a target the world holds within the lion's line of sight
    /// ("Target is still visible, tracking.", `FUN_14118f8c0` against its sight squared:
    /// `COMISS`, `JC` past) whose master has no flag 0x10 (`+0x654`) it attacks again (slot 0xb8:
    /// the target, forced, not any, its group) and the processing breaks. Else ("Target is
    /// invalid or no longer visible, stopping pursuit.") the target goes, the action stops (slot
    /// 0xb0 with 1) and slot 0x48 hands the notify to the owner (not held), the base handler's.
    pub(super) fn lion_target_moved(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        // "Target is still visible, tracking.": a target the world holds within the
        // lion's line of sight (FUN_14118f8c0 against its sight squared: COMISS, JC
        // past)
        let gap = at
            .target
            .filter(|_| self.current_target.is_some())
            .map(|t| footprint_gap_squared(at.position, at.size, t.position, t.radius));
        // a target within its sight whose master has no flag 0x10 (`+0x654`) it
        // attacks again (slot 0xb8: the target, forced, not any, its group) and the
        // processing breaks
        let flags = at.target.map_or(ObjectFlags::NONE, |t| t.flags);
        if let Some(gap) = gap
            && let Some(target) = self.current_target
            && !below_or_unordered(at.los.squared().get(), gap.get())
            && !flags.contains(ObjectFlags::SCOUT)
        {
            let _ = self.attack_object(target, true, false, at, side)?;
            return Ok(BREAK);
        }
        // "Target is invalid or no longer visible, stopping pursuit.": the target
        // goes, the action stops (slot 0xb0 with 1) and slot 0x48 hands the notify
        // to the owner (not held)
        self.remove_target();
        self.stop_current(StopOrder::Keep, at, side, ai_rng)?;
        self.base_notify(n, time, at, player_type, ai_rng, side)
    }

    /// The villager's handler (`FUN_140c22bf0`; the notifies it has no case for go to the base
    /// handler): a completion or failure of its action stops it, and slot 0x48 passes the notify
    /// to its owner's slot 0x2e8 (not held); a failed build (0x202, its
    /// Build waiting without work, action 0x25a) looks for another foundation to build
    /// (`FUN_140c1c7c0`, when the player's `+0x613` is 0: unsaved, 0 in this game): one the
    /// unit reaches within 0.1 (slot 0x3d0) it builds (slot 0xf0 with 1) and the processing
    /// breaks; finding none, or not reaching it, a human player's villager stops (slot
    /// 0x3c0) and the processing breaks. On the attack action (600) an invalidation, a
    /// moved target or a search (0x1fb, 0x1fc, 0x202) is the base handler's ("My attack
    /// target's moved, just doing normal target moved processing."), but a moved target
    /// with `+0x1a8` set: the unit's slot 0x328 posts the attack's failure to the AI
    /// (0x1f9 with 600, from and to the unit) and processing goes on. A missile's warning it
    /// does not dodge (the player's `+0xe10`) is its damage; the object its build or hunt made
    /// is [`UnitAi::work_on_made`]'s, a load of another resource [`UnitAi::drop_other_load`]'s.
    pub(super) fn villager_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::ACTION_INVALIDATED
            | NotifyKind::TARGET_MOVED
            | NotifyKind::REQUIRES_SEARCH
                if self.current_action == Some(ActionKind::Attack) =>
            {
                if n.kind == NotifyKind::TARGET_MOVED && self.stop_after_kill {
                    let failed = Notify::own(
                        at.id,
                        NotifyKind::ACTION_FAILED,
                        [ActionKind::Attack.value(), 0, 0],
                    );
                    self.post(failed, at.state)?;
                    return Ok(GO_ON);
                }
                self.base_notify(n, time, at, player_type, ai_rng, side)
            }
            NotifyKind::ACTION_INVALIDATED
            | NotifyKind::TARGET_MOVED
            | NotifyKind::REQUIRES_SEARCH => self.villager_search(n, at, player_type, side, ai_rng),
            // a completion or a failure of its action (0x1fa, 0x1f9) stops it
            NotifyKind::ACTION_COMPLETED | NotifyKind::ACTION_FAILED => {
                if ActionKind::field(self.current_action) == n.p[0] {
                    self.remove_target();
                    self.stop_current(StopOrder::Keep, at, side, ai_rng)?;
                }
                Ok(GO_ON)
            }
            NotifyKind::ACTION_EXECUTED if player_type.is_computer() => Err(
                GenieError::unsupported("a computer villager's executed action"),
            ),
            // a missile's warning it does not dodge (the player's `+0xe10`) is its damage
            NotifyKind::MISSILE_WARNING if dodges(at, ai_rng) => Ok(GO_ON),
            NotifyKind::DAMAGED | NotifyKind::MISSILE_WARNING => {
                self.villager_damaged(n, time, at, player_type, ai_rng, side)
            }
            // off the build or the hunt action, the object its action made (a pasture,
            // `p[1]`) which the world holds: the unit works it at its place (slot 0x7e8,
            // the caller's)
            NotifyKind::PASTURE_BUILT => self.work_on_made(n, at, side, ai_rng),
            // a load of another resource asked off (FUN_141658e20 from its place, its slot
            // 0x128 on the site, its slot 0x30 back to its action's target: the caller's)
            NotifyKind::OTHER_LOAD => self.drop_other_load(side, ai_rng),
            // its own cases the slice does not model; any other is the base handler's
            NotifyKind(0x217..=0x21a) => {
                Err(VILLAGER_NOTIFY.with("notify", format!("{:#x} {:?}", n.kind, n.p)))
            }
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// The villager's made object (0x21d): off the build or the hunt action, the object its
    /// action made (a pasture, `p[1]`) which the world holds: the unit works it at its place
    /// (slot 0x7e8, the caller's).
    pub(super) fn work_on_made(
        &mut self,
        n: Notify,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<u32, GenieError> {
        if (n.p[0] == ActionKind::Build.value() || n.p[0] == ActionKind::Hunt.value())
            && let Some(made) = Self::world_object(ObjectId::of_field(n.p[1]), at, side)?
        {
            side.effect_now(
                Effect::Command(AiCommand::WorkOn {
                    target: made.id,
                    point: made.position,
                }),
                self,
                rng,
            )?;
        }
        Ok(GO_ON)
    }

    /// The villager's load of another resource asked off (0x21c; `FUN_141658e20` from its place,
    /// its slot 0x128 on the site, its slot 0x30 back to its action's target: the caller's):
    /// slot 0x190 first, its top action's target, then the drop site the table names.
    pub(super) fn drop_other_load(
        &mut self,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<u32, GenieError> {
        let back = side.top_target_now()?;
        if let Some(site) = side.table_drop_site()? {
            side.effect_now(
                Effect::Command(AiCommand::DropElsewhere { site, back }),
                self,
                rng,
            )?;
        }
        Ok(GO_ON)
    }

    /// The transport ship's handler (`FUN_140c2a580`) and the siege tower's (`FUN_140c28400`,
    /// the same cases; [`UnitAi::transport_damaged`] tells them apart: a human player's tower in a
    /// group that is not idle ignores the damage): its damage notify, and a missile's warning it
    /// takes as damage unless it dodges; any other notify is the base handler's.
    pub(super) fn transport_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::MISSILE_WARNING if dodges(at, ai_rng) => Ok(GO_ON),
            NotifyKind::DAMAGED | NotifyKind::MISSILE_WARNING => {
                self.transport_damaged(n, time, at, player_type, ai_rng, side)
            }
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// The trade ship's and cart's handler (`FUN_140c29bf0`): its search when its action needs
    /// one, its damage notify, a missile's warning it takes as damage unless it dodges.
    pub(super) fn trader_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::REQUIRES_SEARCH => self.trader_search(at, side, ai_rng),
            NotifyKind::MISSILE_WARNING if dodges(at, ai_rng) => Ok(GO_ON),
            NotifyKind::DAMAGED | NotifyKind::MISSILE_WARNING => {
                self.trade_damaged(n, time, at, player_type, ai_rng, side)
            }
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// The fishing ship's handler (`FUN_140c25ca0`): a builder's search for more to build, its
    /// damage notify, a missile's warning it takes as damage unless it dodges.
    pub(super) fn fishing_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::REQUIRES_SEARCH if self.current_action == Some(ActionKind::Build) => {
                self.fishing_builder_search(at, player_type, side, ai_rng)
            }
            NotifyKind::MISSILE_WARNING if dodges(at, ai_rng) => Ok(GO_ON),
            NotifyKind::DAMAGED | NotifyKind::MISSILE_WARNING => {
                self.fishing_damaged(n, time, at, player_type, ai_rng, side)
            }
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// The siege unit's handler (`FUN_140c287e0`): a computer player's packed unit's damage
    /// notify ([`UnitAi::packed_damaged`]); a target under its least range (0x1fe) makes it back
    /// up, as does a computer player's ranged unit's executed attack on a drawn chance (0x200);
    /// any other notify is the base handler's.
    pub(super) fn siege_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::DAMAGED
                if at.class == Some(PACKED_UNIT_CLASS) && player_type.is_computer() =>
            {
                self.packed_damaged(n, time, at, player_type, ai_rng, side)
            }
            NotifyKind::TOO_CLOSE => back_up(),
            NotifyKind::ACTION_EXECUTED
                if player_type.is_computer() && self.keeps_distance(at, ai_rng) =>
            {
                if self.current_action == Some(ActionKind::Attack) {
                    back_up()
                } else {
                    Ok(GO_ON)
                }
            }
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// A computer player's packed siege unit's damage: dead (hit points below 1) it alerts its
    /// owner, its notifies end and it gives up; an attacker the world does not hold ends it
    /// (`FUN_140c54e30`); else it stops and runs away, and the processing breaks.
    pub(super) fn packed_damaged(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        if n.p[1] < 1 {
            alert_owner(player_type)?;
            self.end_notifies(time, at, side, ai_rng)?;
            return Ok(DEAD);
        }
        if at.unit(n.caller).is_none() {
            return Ok(GO_ON);
        }
        self.stop_current(StopOrder::Keep, at, side, ai_rng)?;
        let _ = self.run_away(true, at, player_type, ai_rng, side)?;
        Ok(BREAK)
    }

    /// The soldier's handler (`FUN_140c28bb0`): its own cases are the computer's, a backing-up
    /// and a healer's search.
    pub(super) fn soldier_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::TOO_CLOSE => back_up(),
            NotifyKind::ACTION_EXECUTED | NotifyKind::MISSILE_WARNING
                if player_type.is_computer() =>
            {
                Err(GenieError::unsupported("a computer soldier's notify"))
            }
            NotifyKind::REQUIRES_SEARCH if self.current_action == Some(ActionKind::Heal) => {
                Err(GenieError::unsupported("a healer's search notify"))
            }
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// The tower's handler (`FUN_140c299f0`): its damage notify, alive or dead, and its
    /// destroyed target.
    pub(super) fn tower_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            // "I'm dead, I give up.": the owner's alert (slot 0x2e8 with 0x209: a human
            // player's alert list and the old pather's hulled-area reset, not held; a
            // computer player's AI hears it), the notifies end and it gives up. Alive, the
            // other owners of the units inside it hear 0x201 and the base handler's damage
            // case follows
            NotifyKind::DAMAGED if n.p[1] < 1 => {
                alert_owner(player_type)?;
                self.end_notifies(time, at, side, ai_rng)?;
                Ok(DEAD)
            }
            NotifyKind::DAMAGED => {
                if at.guest_computer {
                    alert_owner(PlayerType::COMPUTER)?;
                }
                self.damaged(n, time, at, player_type, ai_rng, side)
            }
            // "I am a defensive building and my current target has been destroyed, purging
            // attack order.": a target gone or dead (above state 2) is forgotten
            // (`FUN_140c5e1e0`) and the current action stops (slot 0xb0 with 1)
            NotifyKind::ACTION_COMPLETED
                if n.p[0] == 600 && self.current_order == Some(OrderKind::Attack) =>
            {
                let gone = self.current_target.is_none()
                    || at.target.is_none_or(|t| ObjectState::Alive < t.state);
                if gone {
                    self.remove_target();
                    self.stop_current(StopOrder::Keep, at, side, ai_rng)?;
                }
                Ok(GO_ON)
            }
            _ => self.base_notify(n, time, at, player_type, ai_rng, side),
        }
    }

    /// The building's handler (`FUN_140c22610`): its damage notify, its attackers' coming and
    /// going; 0x202, 0x216 and 0x21b are the base handler's, any other notify ends there.
    pub(super) fn building_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::DAMAGED => self.building_damaged(n, time, at, player_type, side, ai_rng),
            // "Unit #%d is attacking me": it joins the attackers and the unit is under attack
            NotifyKind::ATTACKED => {
                let _ = self.attackers.add(n.caller);
                side.set_under_attack(true);
                Ok(GO_ON)
            }
            // "is no longer doing an action on me.": one in the set (FUN_140bf4cb0) leaves it
            NotifyKind::NO_LONGER_ON_ME => {
                self.attackers.remove(n.caller);
                Ok(GO_ON)
            }
            NotifyKind::REQUIRES_SEARCH | NotifyKind::SENT_OFF | NotifyKind::SENT_OFF_GROUP => {
                self.base_notify(n, time, at, player_type, ai_rng, side)
            }
            _ => Ok(GO_ON),
        }
    }

    /// The livestock's handler (`FUN_140c26ac0`): a unit sent off a footprint (0x216, with the
    /// new sending-off `DAT_1439e60e2` 1) is the base handler's; any notify but 0x202 drops the
    /// action value (`FUN_140c64b50`) and breaks off.
    pub(super) fn livestock_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::SENT_OFF => self.base_notify(n, time, at, player_type, ai_rng, side),
            NotifyKind::REQUIRES_SEARCH => Err(GenieError::unsupported(
                "a livestock's notify 0x202 (its settings' test)",
            )),
            _ => {
                self.set_action(None);
                Ok(BREAK)
            }
        }
    }
}
