//! The notifies' handling, the artifact's and the base's.

use crate::GenieError;
use crate::math::rng::Well512;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::WorkerCount;
use crate::objects::masters::master_type::MOVING;
use crate::players::PlayerType;
use crate::unit_ai::ActionKind;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAiType;
use crate::unit_ai::{
    ARTIFACT_AI, ARTIFACT_CARED, Effect, FISHING_AI, GO_ON, Memory, Notify, Pick, SIEGE_AI,
    SIEGE_TOWER_AI, Search, Surroundings, TRADE_AI, UnitAi, UnitSide, search_radius,
};
use crate::world::Millis;

impl UnitAi {
    /// Slot 0x238 per factory type: each type's handler (`type_notifies.rs`), most of which pass
    /// the notify on to the base handler ([`UnitAi::base_notify`]).
    pub(super) fn handle(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match self.ai_type {
            UnitAiType::HUNTED => self.hunted_notify(n, time, at, player_type, ai_rng, side),
            UnitAiType::BOAR => self.boar_notify(n, time, at, player_type, ai_rng, side),
            UnitAiType::LION => self.lion_notify(n, time, at, player_type, ai_rng, side),
            UnitAiType::CIVILIAN => self.villager_notify(n, time, at, player_type, ai_rng, side),
            UnitAiType::TRANSPORT | SIEGE_TOWER_AI => {
                self.transport_notify(n, time, at, player_type, ai_rng, side)
            }
            TRADE_AI => self.trader_notify(n, time, at, player_type, ai_rng, side),
            FISHING_AI => self.fishing_notify(n, time, at, player_type, ai_rng, side),
            SIEGE_AI => self.siege_notify(n, time, at, player_type, ai_rng, side),
            UnitAiType::SOLDIER => self.soldier_notify(n, time, at, player_type, ai_rng, side),
            UnitAiType::TOWER => self.tower_notify(n, time, at, player_type, ai_rng, side),
            UnitAiType::BUILDING => self.building_notify(n, time, at, player_type, ai_rng, side),
            UnitAiType::LIVESTOCK => self.livestock_notify(n, time, at, player_type, ai_rng, side),
            ARTIFACT_AI => self.artifact_notify(n, time, at, player_type, ai_rng, side),
            _ => Err(GenieError::unsupported(
                "the notify handler of a unit-AI type not modelled",
            )),
        }
    }

    /// The artifact's notify handler (slot 0x238, `FUN_140c21f70`): the kinds it lets be (500,
    /// 0x1f6 to 0x1f8, 0x1fd to 0x200, 0x202, 0x203, 0x20b, 0x20f, 0x259 to 0x262, 0x265 to
    /// 0x269, 0x278, 699 and 0x2be) go on; a unit attacking it (600) joins its attackers
    /// (`FUN_140b43f30`) and puts its unit under attack (`FUN_14119d2d0` with 1); a unit sent off
    /// a footprint (0x216, 0x21b) is the base handler's. A completion, failure or invalidation
    /// of its action (without an order its action value, with one the order's action: action +
    /// 100 = order) and a moved target the world no longer holds or that its owner no longer
    /// sees (`FUN_1416635e0` not 0xf) end in its stop, as any other kind does ("Unprocessed
    /// notify"): the target goes (`FUN_140c5e1e0`), the action stops (slot 0xb0 with 1) and slot
    /// 0x48 hands the notify to the owner (its alerts, not held). Each answers 2.
    pub(super) fn artifact_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        let stops = match n.kind {
            NotifyKind::DAMAGED
            | NotifyKind(
                0x1f6..=0x1f8 | 0x1fd..=0x200 | 0x259..=0x262 | 0x265..=0x269 | 0x278 | 0x2be,
            )
            | NotifyKind::REQUIRES_SEARCH
            | NotifyKind::NO_DROP_SITE
            | NotifyKind::OWNER_CHANGED
            | NotifyKind::MISSILE_WARNING
            | NotifyKind::NO_LONGER_ON_ME => false,
            NotifyKind::ACTION_FAILED
            | NotifyKind::ACTION_COMPLETED
            | NotifyKind::ACTION_INVALIDATED => self.current_order.map_or_else(
                || ActionKind::field(self.current_action) == n.p[0],
                |o| o.owns(self.current_action),
            ),
            // "Target is still visible.": the world holds the target, which its owner sees
            NotifyKind::TARGET_MOVED => !at
                .target
                .is_some_and(|t| self.current_target.is_some() && t.sight.seen()),
            NotifyKind::SENT_OFF | NotifyKind::SENT_OFF_GROUP => {
                return self.base_notify(n, time, at, player_type, ai_rng, side);
            }
            NotifyKind::ATTACKED => {
                let _ = self.attackers.add(n.caller);
                side.set_under_attack(true);
                false
            }
            _ => true,
        };
        if stops {
            self.remove_target();
            self.stop_current(true, at, side, ai_rng)?;
        }
        Ok(GO_ON)
    }

    /// The artifact's processIdle (slot 72, `FUN_140c1e410`, factory type 14): `ClosestObject`
    /// (`FUN_140c488b0`) of any class it cares about, relation, state, master and workers from
    /// its place, the reach test only for a unit that moves (slot 0x208 above 0), which sets the
    /// alert mode ([`UnitAi::alert`]). While the lists hold the objects of at most one player
    /// but Gaia (`DAT_143e6f92c` below 2, [`Found::players`]), the object found, when the world
    /// holds it and another player owns it, takes the artifact over: the unit's slot 0x70 to its
    /// owner ([`Effect::ChangeOwner`]), and processIdle produced (6, true); else 5.
    ///
    /// [`Found::players`]: crate::unit_ai::Found::players
    pub(super) fn artifact_idle(
        &mut self,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<bool, GenieError> {
        let found = side.closest(&Search {
            at: at.position.ground(),
            radius: search_radius(at.los, at.search_radius),
            classes: &ARTIFACT_CARED,
            size: at.size,
            pick: Pick::Closest {
                group: None,
                relation: None,
                state: None,
                master: None,
                workers: WorkerCount::NO_CAP,
                spared: None,
                reach: Speed::ZERO < at.speed,
            },
            memory: Memory::default(),
        })?;
        self.alert(found, at.dt_seconds);
        if found.players < 2
            && let Some(id) = found.id
            && let Some(u) = side.seen_unit(id)?
            && u.owner != at.player
        {
            side.effect_now(Effect::ChangeOwner(u.owner), self, rng)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// The artifact's slot 73 (`FUN_140c21d40`): its processIdle (slot 0x240 with 1,
    /// [`UnitAi::artifact_idle`]); once another player took it over (6) its target goes
    /// (`FUN_140c5e1e0`), its action stops (slot 0xb0 with 1) and its idle timer is 0 (it
    /// answers 10), else 9.
    pub(super) fn artifact_misc(
        &mut self,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        if self.artifact_idle(at, side, rng)? {
            self.remove_target();
            self.stop_current(true, at, side, rng)?;
            self.idle_timer = Millis::ZERO;
        }
        Ok(())
    }

    /// The base handler (`FUN_140c589c0`) for the kinds the slice posts: a completed action
    /// ([`UnitAi::action_completed`]), an ownership change (stopAction, `FUN_140c5e1e0`, slot 0xb0
    /// with 1, `FUN_140c5dc10` with the handler's time, the order queue emptied), the kinds it
    /// ignores; anything else is not modelled.
    pub(super) fn base_notify(
        &mut self,
        n: Notify,
        time: Millis,
        at: &Surroundings,
        player_type: PlayerType,
        ai_rng: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<u32, GenieError> {
        match n.kind {
            NotifyKind::ACTION_COMPLETED => self.action_completed(n, at, player_type, ai_rng, side),
            NotifyKind::ACTION_FAILED => self.action_failed(n, at, player_type, ai_rng, side),
            NotifyKind::ACTION_INVALIDATED => {
                self.action_invalidated(n, at, player_type, ai_rng, side)
            }
            NotifyKind::REQUIRES_SEARCH if self.explorer() => {
                self.seek_unexplored(n, at, player_type, ai_rng, side)
            }
            NotifyKind::OWNER_CHANGED => {
                side.effect_now(Effect::Stop, self, ai_rng)?;
                self.remove_target();
                self.stop_current(true, at, side, ai_rng)?;
                self.end_notifies(time, at, side, ai_rng)?;
                self.orders.clear();
                Ok(GO_ON)
            }
            // a MoveTo's moved target: with a saved patrol processIdle runs (slot 0x240 with 1),
            // its result unread
            NotifyKind::TARGET_MOVED if self.patrol_path.is_some() => {
                let _ = self.process_idle(at, player_type, ai_rng, side)?;
                Ok(GO_ON)
            }
            // a moved target, then the kinds "I'm ignoring that." and the action values
            NotifyKind::TARGET_MOVED
            | NotifyKind(0x1f6..=0x1f8 | 0x1ff | 0x259..=0x262 | 0x265..=0x269 | 0x2be)
            | NotifyKind::ACTION_EXECUTED
            | NotifyKind::NO_DROP_SITE => Ok(GO_ON),
            // readyToAttack's target out of reach (0x1fd): a unit on the attack order that cannot
            // move (FUN_140fdd080: a master of type 30 and up with speed) gives the order up
            NotifyKind::OUT_OF_REACH => {
                if self.current_order == Some(OrderKind::Attack)
                    && !(MOVING <= at.kind && Speed::ZERO < at.speed)
                {
                    self.current_order = None;
                }
                Ok(GO_ON)
            }
            NotifyKind::DAMAGED | NotifyKind::MISSILE_WARNING => {
                self.damaged(n, time, at, player_type, ai_rng, side)
            }
            NotifyKind::IN_THE_WAY => self.in_the_way(n, at, side, ai_rng),
            NotifyKind::SENT_OFF => self.sent_off(n, side, ai_rng),
            // a unit of a group sent off a footprint: the action value 0x2c0
            NotifyKind::SENT_OFF_GROUP => {
                self.set_action(Some(ActionKind::GroupAside));
                Ok(GO_ON)
            }
            // "Unit #%d is no longer doing an action on me.": an attacker leaves the set
            NotifyKind::NO_LONGER_ON_ME => {
                self.attackers.remove(n.caller);
                Ok(GO_ON)
            }
            _ => Err(
                GenieError::unsupported("a notify kind the base handler does not model")
                    .with("notify", format_args!("{:#x}", n.kind))
                    .with("caller", ObjectId::field(n.caller)),
            ),
        }
    }
}
