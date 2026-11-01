//! The unit AI's state, gates, the auto scout and the STOP, EXPLORE and standing commands.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type::BUILDING;
use crate::orders::object_gone;
use crate::orders::{Place, lock_gate, set_stance};
use crate::players::PlayerId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::slots::PlayerSlots;

impl World {
    /// `UNIT_AI_STATE` (`do_command` case 0x12): the check ([`World::command_check`]), then
    /// `FUN_140e6a7c0` over its first 60 ids (500 for a computer player): each object the world
    /// holds (any player's the check let through) with a unit AI takes the stance `state` (slot
    /// 0x1c0, [`set_stance`]), but not stances 1 and 2 for classes 54 and 51 (and, the siege AI's
    /// slot, masters 42 and 331). The UI's flag is not held.
    pub(crate) fn unit_ai_state(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
        state: crate::unit_ai::UnitStance,
    ) -> Result<(), GenieError> {
        let Some(ids) = self.command_check(player, ids)? else {
            return Ok(());
        };
        let cap = if self.players.player(player)?.computer {
            500
        } else {
            60
        };
        for &id in ids.iter().take(cap) {
            let Some(place) = self.place(id) else {
                continue;
            };
            let (p, idx) = match place {
                Place::Live(p, i) => (p, i),
                // only slot 0x180 puts an object to sleep, one of a type below 30, which has no
                // unit AI: the handler passes it over
                Place::Sleeping(p, i) if self.players.asleep(p, i)?.ai.is_none() => continue,
                Place::Sleeping(..) => {
                    return Err(GenieError::unsupported(
                        "a UNIT_AI_STATE of a sleeping object with a unit AI",
                    ));
                }
            };
            let unit = &self.players.live(p, idx)?;
            if unit.ai.is_none() {
                continue;
            }
            let master = unit.master(&self.masters, p)?;
            if state.leashed()
                && (matches!(master.class, class::CLASS_51 | class::CLASS_54)
                    || matches!(master.id, ids::ID_42 | ids::ID_331))
            {
                continue;
            }
            let class = master.class;
            let unit = &self.players.live(p, idx)?;
            let place = (unit.position().ground(), Some(class));
            // slot 0x1c0, its stop a call on the unit at once
            self.run_ai_calls((p, idx), |ai, side, rng| {
                set_stance(ai, state, place, side, rng)
            })?;
        }
        Ok(())
    }

    /// GATE (`do_command` case 0x72): the command's check refuses an object `id` the world does
    /// not hold, or another player's while players may not order each other's units (the
    /// setting `+0x1074`, [`crate::world::settings::Settings::cheats_enabled`]; the world's `+0xb12` cheat is not held).
    /// The object, live or sleeping (`FUN_1415417a0`), of a master of type 80 toggles its gate
    /// lock: slot 0x670 ([`lock_gate`]) with slot 0x668's value negated. Its sound (slot 0x4c0)
    /// and the UI's flag are not held.
    pub(crate) fn gate(&mut self, player: PlayerId, id: ObjectId) -> Result<(), GenieError> {
        let Some((p, unit)) = self.object(id) else {
            return Ok(());
        };
        if p != player && !self.settings.cheats_enabled {
            return Ok(());
        }
        if unit.master(&self.masters, p)?.kind != BUILDING {
            return Ok(());
        }
        let now = self.world_time;
        let (_, unit) = self.object_mut(id).ok_or_else(|| object_gone(id))?;
        // slot 0x668: the lock's value (a building without its gate state is lock_gate's)
        let locked = unit.gate.is_some_and(|g| g.locked);
        lock_gate(unit, !locked, now)?;
        // the walks of this update read the last move time the lock set
        if let Some(t) = unit.body.mover().map(|m| m.last_move) {
            self.spatial.set_last_move(id, t);
        }
        Ok(())
    }

    /// `AUTO_SCOUT` (0x26, `FUN_140e666f0`): of the first 60 ids (500 for a computer player,
    /// `FUN_141659cf0`) each object the world holds (any player's: the handler has no owner
    /// test) with a unit AI that may scout (`FUN_140c89760`: its master's object flags `+0x654`
    /// have bit 4; class 58's rule behind a game setting, `FUN_140eca780`, is not modelled)
    /// takes an order at once (slot 0x30 -> slot 0x28 with Immediate,
    /// [`crate::unit_ai::UnitAi::take_order_now`]): the stop order when its action is the auto
    /// scout's already, else the auto scout order, from the object's owner (`+0x718`), without
    /// a target, at the point (−1, −1) at the unit's height, range 1, priority 100, shown at
    /// (−1, −1) and ready. Its sound and the UI's flags are not held.
    pub(crate) fn auto_scout(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        use crate::unit_ai::QueuedOrder;
        let cap = if self.players.player(player)?.computer {
            500
        } else {
            60
        };
        for &id in ids.iter().take(cap) {
            let Some(place) = self.place(id) else {
                continue;
            };
            let (p, idx) = match place {
                Place::Live(p, i) => (p, i),
                // only slot 0x180 puts an object to sleep, one of a type below 30, which has no
                // unit AI: the handler passes it over
                Place::Sleeping(p, i) if self.players.asleep(p, i)?.ai.is_none() => continue,
                Place::Sleeping(..) => {
                    return Err(GenieError::unsupported(
                        "an AUTO_SCOUT of a sleeping object with a unit AI",
                    ));
                }
            };
            let unit = &self.players.live(p, idx)?;
            if unit.ai.is_none() {
                continue;
            }
            let master = unit.master(&self.masters, p)?;
            if !master.object_flags.contains(ObjectFlags::SCOUT) {
                if master.class == class::LIVESTOCK {
                    return Err(GenieError::unsupported(
                        "AUTO_SCOUT's class 58 rule (FUN_140eca780)",
                    ));
                }
                continue;
            }
            let class = master.class;
            let (state, position) = (unit.life.state, unit.position());
            let scouts = unit
                .ai
                .as_ref()
                .is_some_and(|ai| ai.current_action == Some(ActionKind::AutoScout));
            let order = QueuedOrder {
                issuer: Some(crate::unit_ai::Issuer::Player(p)),
                order: if scouts {
                    OrderKind::Stop
                } else {
                    OrderKind::AutoScout
                },
                priority: crate::unit_ai::OrderPriority::COMMAND,
                target: None,
                target_owner: None,
                point: Position::new(-1.0, -1.0, position.z()),
                range: Distance::new(1.0),
                shown: Point::new(-1.0, -1.0),
                ready: true,
                target_type: None,
            };
            // the order at once, its calls on the unit at once
            self.run_ai_calls((p, idx), |ai, side, rng| {
                ai.take_order_now(state, order, (position.ground(), Some(class)), rng, side)
            })?;
        }
        Ok(())
    }

    /// STOP (0x01, `RGE_Command::do_command` case 1) of the player's units `ids`: the command's
    /// check ([`World::command_check`]), the lookup ([`World::lookup`]), then `FUN_14161e200`. First
    /// each unit AI resets as for an order (`FUN_141631040`, the loop of `FUN_140e6cfc0`:
    /// [`crate::unit_ai::UnitAi::reset_for_order`]). Several entries (a gone unit's null among
    /// them) that are exactly one unit group's members (`FUN_14152ec80`, the group's size
    /// `+0xe4`) stop as the group: each unit AI's order queue empties and its idle timer takes its
    /// adjusted timeout, then the group's stop (`FUN_1410110c0`), which is not modelled. Else each
    /// unit leaves the unit group the world holds of its link (`FUN_14102c160` with -1 and 0,
    /// [`World::remove_unit`]), its unit AI's order queue empties (`FUN_140c5e010`) and its idle
    /// timer takes its adjusted timeout (`+0x160` = `+0x164`), its ignore list empties
    /// (`FUN_141197da0`), the object stops (slot 0x148, [`World::object_stop`]) and its movement
    /// stands (slot 0x560, `FUN_1411616f0`: slot 0x5d0, then `FUN_140f98e80` on its walked
    /// level).
    pub(crate) fn stop_command(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        let Some(ids) = self.command_check(player, ids)? else {
            return Ok(());
        };
        let list: Vec<Option<ObjectId>> = self
            .lookup(&ids)
            .into_iter()
            .zip(&ids)
            .map(|(place, &id)| place.and(id))
            .collect();
        let units: Vec<ObjectId> = list.iter().flatten().copied().collect();
        for &id in &units {
            let unit = match self.place(id) {
                Some(Place::Live(p, i)) => self.players.live_mut(p, i)?,
                Some(Place::Sleeping(p, i)) => self.players.asleep_mut(p, i)?,
                None => continue,
            };
            if let Some(ai) = unit.ai.as_mut() {
                ai.reset_for_order(self.streams.get(Stream::Ai));
            }
        }
        if 1 < list.len() {
            let group =
                crate::groups::common_group(&list, |id| self.object(id).map(|(_, u)| u.group));
            if group
                .and_then(|id| self.groups.get(id))
                .is_some_and(|g| g.members.len() == list.len())
            {
                return Err(GenieError::unsupported(
                    "a STOP of a whole unit group (FUN_1410110c0)",
                ));
            }
        }
        for &id in &units {
            if let Some(link) = self.object(id).and_then(|(_, u)| u.group.id) {
                let _ = self.remove_unit(link, id, None, false)?;
            }
            let Some((_, u)) = self.object_mut(id) else {
                continue;
            };
            if let Some(ai) = u.ai.as_mut() {
                ai.orders.clear();
                ai.idle_timer = ai.adjusted_idle_timeout;
            }
            if let Some(m) = u.body.mover_mut() {
                m.ignore_units.clear();
            }
            // a sleeping object is of a type below 30 (only slot 0x180 puts one to sleep), whose
            // slots 0x148 and 0x560 are empty (0x141072730, 0x141072740)
            let Some(Place::Live(p, idx)) = self.place(id) else {
                continue;
            };
            self.object_stop(p, idx)?;
            if let Some(m) = self.players.live_mut(p, idx)?.body.mover_mut() {
                m.stand()?;
            }
        }
        Ok(())
    }

    /// The check of a command on units (STOP's `FUN_140e55a30` and `UNIT_AI_STATE`'s `FUN_140e55d30`,
    /// twins of `FUN_140e558b0`) of the ids of `player`: of the first 60 (500 for a computer,
    /// `FUN_141659cf0`) an id the world does not hold, or another player's without the setting
    /// `+0x1074` ([`crate::world::settings::Settings::cheats_enabled`]), becomes none (the game's
    /// −1); the ids past them stay unchecked. A human's command whose checked ids all went is
    /// refused (None).
    /// The packet's own tests (its size, a player the game lacks) are
    /// [`World::read_units`]'s, and the world's `+0xb12` (the cheat that would let either
    /// through) is not held.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub(crate) fn command_check(
        &self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
    ) -> Result<Option<Vec<Option<ObjectId>>>, GenieError> {
        let computer = self.players.player(player)?.computer;
        let checked = ids.len().min(if computer { 500 } else { 60 });
        let mut ids = ids.to_vec();
        let mut dropped = 0;
        for id in ids.iter_mut().take(checked) {
            let keep = self
                .object(*id)
                .is_some_and(|(p, _)| p == player || self.settings.cheats_enabled);
            if !keep {
                *id = None;
                dropped += 1;
            }
        }
        if checked <= dropped && !computer {
            return Ok(None);
        }
        Ok(Some(ids))
    }

    /// UNLOAD's check (`FUN_140e6f620`) of the ids of `player`: only the first 60 (500 for a
    /// computer, `FUN_140e6b250`) go on. An id the world does not hold (`FUN_140e6bad0`, live or
    /// sleeping) or none becomes none (the game's −1). Its own player's object stays. Another player's stays when
    /// the two count each other as allies ([`World::mutual_allies`]) and the command ties it to
    /// the issuer: `pick`'s player is the issuer (flag 1), or its object (flags 3 to 6) is the
    /// issuer's (`FUN_1415417a0`). Any other stays only with the setting `+0x1074`
    /// ([`crate::world::settings::Settings::cheats_enabled`]). A human's command whose ids all
    /// went is refused (None). The packet's size tests are the decoder's, and the world's
    /// `+0xb12` is not held.
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub(crate) fn unload_check(
        &self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
        pick: crate::commands::UnloadPick,
    ) -> Result<Option<Vec<Option<ObjectId>>>, GenieError> {
        let computer = self.players.player(player)?.computer;
        let mut ids: Vec<Option<ObjectId>> = ids
            .iter()
            .take(if computer { 500 } else { 60 })
            .copied()
            .collect();
        let tied = |owner: PlayerId| {
            self.mutual_allies(owner, player)
                && match pick {
                    crate::commands::UnloadPick::Player(number) => {
                        usize::try_from(number) == Ok(player.index())
                    }
                    crate::commands::UnloadPick::Object(o)
                    | crate::commands::UnloadPick::SameMaster(o)
                    | crate::commands::UnloadPick::AllBut(o)
                    | crate::commands::UnloadPick::OtherMasters(o) => {
                        self.place(o).is_some_and(|p| p.player() == player)
                    }
                    crate::commands::UnloadPick::All(_)
                    | crate::commands::UnloadPick::Group(_)
                    | crate::commands::UnloadPick::Other(..) => false,
                }
        };
        let mut dropped = 0;
        for id in &mut ids {
            let keep = self.place(*id).is_some_and(|p| {
                p.player() == player || tied(p.player()) || self.settings.cheats_enabled
            });
            if !keep {
                *id = None;
                dropped += 1;
            }
        }
        if ids.len() <= dropped && !computer {
            return Ok(None);
        }
        Ok(Some(ids))
    }

    /// `FUN_1416642c0` with the ally flag: players `a` and `b` each count the other as its own
    /// or allied ([`crate::unit_ai::task_finder::allied`] on each one's relations); not for a
    /// player the world lacks.
    pub(crate) fn mutual_allies(&self, a: PlayerId, b: PlayerId) -> bool {
        let allied = |from: PlayerId, to: PlayerId| {
            self.players
                .get(from.index())
                .is_some_and(|p| crate::unit_ai::task_finder::allied(&p.relations, from, to, true))
        };
        allied(a, b) && allied(b, a)
    }

    /// `FUN_140e6b750`: each id's object (`FUN_1415417a0`, live or sleeping), None where the
    /// world holds none; the list keeps the ids' count.
    pub(crate) fn lookup(&self, ids: &[Option<ObjectId>]) -> Vec<Option<Place>> {
        ids.iter().map(|&id| self.place(id)).collect()
    }

    /// Slot 0x68 with the master's standing sprite (`+0x470`): the unit's top speed is the
    /// sprite's ([`crate::movement::Mover::shows_standing`]), and the sprite itself where the
    /// slice holds it.
    pub(crate) fn standing_sprite(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
    ) -> Result<(), GenieError> {
        let master = self.players.live(p, idx)?.life.master_id;
        let standing = self.master(p, master)?.standing_sprite;
        let shown = standing.and_then(|s| self.graphics.get(&s)).copied();
        let unit = self.players.live_mut(p, idx)?;
        if let Some(m) = unit.body.mover_mut() {
            m.shows(shown.as_ref());
        }
        if unit.sprite.is_none() {
            return Ok(());
        }
        let g = shown.ok_or(GenieError::unsupported(
            "a unit without its standing sprite",
        ))?;
        let rng = self.streams.get(Stream::Sprite);
        let u = self.players.live_mut(p, idx)?;
        crate::objects::sprite::new_sprite(&mut u.sprite, u.body.move_stats_mut(), g, rng);
        Ok(())
    }
}
