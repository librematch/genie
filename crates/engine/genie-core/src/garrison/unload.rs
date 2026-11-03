//! UNLOAD and `GO_BACK_TO_WORK`: the units inside a building or a transport come out.

use crate::GenieError;
use crate::commands::unload::{UnloadCommand, UnloadPick};
use crate::garrison::ContainerId;
use crate::garrison::SIEGE_UNLOAD;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::orders::object_gone;
use crate::world::Seconds;
use crate::world::slots::PlayerSlots;

use crate::garrison::transport::Around;
use crate::math::image::garrison::NO_POINT as GARRISON_NO_POINT;
use crate::objects::unit::Unit;
use crate::orders::set_stance;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::{Issuer, NotifyKind, OrderPriority, OrderTargetType, QueuedOrder};
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;

/// UNLOAD's point without one (`DAT_14330ec20`).
pub(super) const NO_POINT: f32 = GARRISON_NO_POINT;

impl World {
    /// UNLOAD (`FUN_14161f410`) by player `player` of the objects `ids` its check keeps
    /// ([`World::unload_check`]: its own, and an ally's the command ties to it), any player's
    /// (the command's list, `FUN_140e6b750`), each alive (at least 1 hit point; `COMISS`/`JC`: a
    /// NaN is), its owner's from there on:
    /// - A unit inside a building leaves it toward the point (with its sound, no work), its
    ///   orders gone and, of type 70 and up, no longer under the town bell.
    /// - A unit (below type 80) inside a transport (type 70 or more) or, with `flag` 3, the unit
    ///   `unit_type` names inside this one, goes ashore ([`World::drop_out`]).
    /// - A siege unit (class 13) unloads its own way (`FUN_141225c40`, a siege tower's order 800),
    ///   which is not modelled ([`SIEGE_UNLOAD`]).
    /// - Any other unit takes the unload order at the point ([`World::unload_order`]): a transport
    ///   sails there and unloads (its Transport, [`crate::garrison::transport`]), any other walks there.
    /// - A building lets the units inside it out, from the last to come in, those `flag` and
    ///   `unit_type` pick ([`unload_picks`]): each through [`World::let_out`], toward the point
    ///   (or its unit AI's command queue location, which is not modelled); each one out has its
    ///   order queue emptied (`FUN_140c5e010`); a unit (type 70) out is no longer under the town
    ///   bell (`+0x9dc`).
    ///
    /// The local player's interface flag (`FUN_141663f50`) is not held.
    pub(crate) fn unload(
        &mut self,
        u: &UnloadCommand,
        ids: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        let UnloadCommand {
            player,
            point,
            pick,
            sound,
            ..
        } = *u;
        let Some(checked) = self.unload_check(player, ids, pick)? else {
            return Ok(());
        };
        // the drop locations' last place across the command's units (FUN_14161f410's locals)
        let mut last = None;
        for (&id, place) in checked.iter().zip(self.lookup(&checked)) {
            // any player's object the check kept; a sleeping one (below type 30) is not held
            let (Some(id), Some(Place::Live(player, idx))) = (id, place) else {
                continue;
            };
            let unit = self.players.live(player, idx)?;
            if unit.life.hp < HitPoints::new(1.0) {
                continue;
            }
            // the object it is inside, and that object's type
            let container = match unit.life.inside {
                Some(c) => {
                    let (cp, cu) = self.object(c).ok_or(GenieError::unreachable(
                        "a unit inside an object the world does not hold",
                    ))?;
                    Some((c, cu.master(self.master_tables(), cp)?.kind))
                }
                None => None,
            };
            // a unit inside a building leaves it toward the point (`FUN_1411e7220` with no work
            // and its sound); a combat unit is no longer under the town bell (`+0x9dc`), and its
            // unit AI's orders go (`FUN_140c5e010`)
            if let Some((c, kind)) = container
                && MasterKind::BUILDING <= kind
            {
                self.leave_building(c.id(), id, point)?;
                continue;
            }
            let unit = self.players.live(player, idx)?;
            let master = unit.master(self.master_tables(), player)?;
            if master.kind < MasterKind::BUILDING {
                let transport = container
                    .filter(|&(_, kind)| MasterKind::COMBAT <= kind)
                    .map(|(c, _)| c);
                if matches!(pick, UnloadPick::Object(_)) || transport.is_some() {
                    self.drop_out(player, idx, transport, pick.object(), &mut last)?;
                } else if self
                    .model
                    .roles
                    .of_class(master.class)
                    .contains(ClassRoles::SIEGE)
                {
                    return Err(SIEGE_UNLOAD);
                } else {
                    self.unload_order(player, idx, point)?;
                }
                continue;
            }
            self.unload_building(player, idx, point, pick, sound)?;
        }
        Ok(())
    }

    /// [`World::unload`] of unit `id` inside building `c`: it leaves toward `point`
    /// (`FUN_1411e7220` with no work and its sound); a combat unit is no longer under the town
    /// bell (`+0x9dc`), and its unit AI's orders go (`FUN_140c5e010`).
    ///
    /// # Errors
    ///
    /// A building outside the live lists, and what [`World::let_out`] does not model.
    pub(super) fn leave_building(
        &mut self,
        c: ObjectId,
        id: ObjectId,
        point: Point,
    ) -> Result<(), GenieError> {
        let Some(Place::Live(cp, ci)) = self.place(c) else {
            return Err(GenieError::unreachable(
                "a building that holds units outside the live lists",
            ));
        };
        let _ = self.let_out(cp, ci, id, point, (None, true))?;
        let (up, ui) = self.live_inside(id)?;
        let kind = self
            .players
            .live(up, ui)?
            .master(self.master_tables(), up)?
            .kind;
        let u = self.players.live_mut(up, ui)?;
        if MasterKind::COMBAT <= kind {
            self.parts.set_town_bell_flag(u.id, false);
        }
        if let Some(ai) = self.parts.unit_ai.get_mut(u.id) {
            ai.orders.clear();
        }
        Ok(())
    }

    /// [`World::unload`] of player `player`'s building `idx`: the units inside it come out from
    /// the last to come in, those `pick` names ([`unload_picks`]), each through
    /// [`World::let_out`] toward `point` (with `sound`), its order queue emptied
    /// (`FUN_140c5e010`) and, a unit (type 70), no longer under the town bell (`+0x9dc`).
    ///
    /// # Errors
    ///
    /// An unload toward the building's command queue location, and what [`World::let_out`] does
    /// not model.
    pub(super) fn unload_building(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        point: Point,
        pick: UnloadPick,
        sound: bool,
    ) -> Result<(), GenieError> {
        let unit = self.players.live(player, idx)?;
        if self
            .parts
            .unit_ai
            .get(unit.id)
            .is_some_and(|ai| !ai.building_orders.is_empty())
        {
            return Err(GenieError::unsupported(
                "an unload toward a building's command queue location",
            ));
        }
        let mut i = unit.garrison.len();
        while 0 < i {
            i -= 1;
            let Some(&inside) = self.players.live(player, idx)?.garrison.get(i) else {
                continue;
            };
            // the game skips a null entry; an id the world no longer holds is one
            let Some((up, u)) = self.object(inside) else {
                continue;
            };
            if !self.unload_picks(up, u, pick)? {
                continue;
            }
            let _ = self.let_out(player, idx, inside, point, (None, sound))?;
            let (up, ui) = self.live_inside(inside)?;
            let kind = self
                .players
                .live(up, ui)?
                .master(self.master_tables(), up)?
                .kind;
            let u = self.players.live_mut(up, ui)?;
            // a unit (type 70) is no longer under the town bell (`+0x9dc`)
            if MasterKind::COMBAT <= kind {
                self.parts.set_town_bell_flag(u.id, false);
            }
            if let Some(ai) = self.parts.unit_ai.get_mut(u.id) {
                ai.orders.clear();
            }
        }
        Ok(())
    }

    /// UNLOAD's unit at `idx` of player `player` inside the transport `transport` (`FUN_14161f410`
    /// with a container of type 70 or more) or, without one (flag 3), the unit `named` when it
    /// is inside this one (`FUN_1415417a0`; one the world does not hold, or elsewhere, stays):
    /// the unit inside, in state 2, goes ashore where a drop location about the holder is free
    /// ([`World::drop_location`] about the holder's place within twice its master's radii, in any
    /// zone; `last` the command's): it takes the holder's place (`FUN_14119d610`), leaves it
    /// (slot 0x1a8) for the place at the holder's height (slot 0x58,
    /// [`World::leave_container`]), takes its own stance again at its place (its unit AI's slot
    /// 0x1c0 with `+0x1a9`, [`crate::orders::set_stance`]) and, of a type above 69, is no longer
    /// under the town bell (`+0x9dc`). Without a free place the interface's message 0x85 is all.
    pub(super) fn drop_out(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        transport: Option<ContainerId>,
        named: Option<ObjectId>,
        last: &mut Option<Point>,
    ) -> Result<(), GenieError> {
        let own = self.players.live(player, idx)?.id;
        let (inner, holder) = match (transport, named) {
            (Some(t), _) => (own, t),
            (None, Some(named)) => match self.object(named) {
                Some((_, o)) if o.life.inside.key() == Some(own) => (named, ContainerId::new(own)),
                _ => return Ok(()),
            },
            (None, None) => return Ok(()),
        };
        let (ip, unit) = self.object(inner).ok_or_else(|| object_gone(inner))?;
        if unit.life.state != ObjectState::Alive {
            return Ok(());
        }
        let made = unit.master(self.master_tables(), ip)?.clone();
        let Some(Place::Live(hp, hi)) = self.place(holder) else {
            return Err(GenieError::unreachable(
                "a transport that holds units outside the live lists",
            ));
        };
        let h = self.players.live(hp, hi)?;
        let at = h.position();
        let r = h.master(self.master_tables(), hp)?.radius;
        let around = Around {
            center: at.ground(),
            radius: Footprint::new(r.x() + r.x(), r.y() + r.y()),
        };
        let Some(spot) = self.drop_location((hp, hi), inner, &made, around, None, last)? else {
            return Ok(());
        };
        let (up, ui) = self.live_inside(inner)?;
        let unit = self.players.live_mut(up, ui)?;
        let inner_id = unit.id;
        unit.place.inside_at(inner_id, at, &mut self.spatial);
        self.leave_container((hp, hi), (up, ui), &made, spot.with_height(at.z()))?;
        let unit = self.players.live_mut(up, ui)?;
        let place = (unit.position().ground(), Some(made.class));
        if self.parts.unit_ai.contains(unit.id) {
            let roles = self.model.roles.clone();
            self.run_ai_calls((up, ui), |ai, side, rng| {
                let stance = ai.stance;
                set_stance(ai, stance, place, side, rng, &roles)
            })?;
        }
        if MasterKind::COMBAT <= made.kind {
            let id = self.players.live(up, ui)?.id;
            self.parts.set_town_bell_flag(id, false);
        }
        Ok(())
    }

    /// UNLOAD's other unit at `idx` of player `player` (`FUN_14161f410`'s last branch): it leaves
    /// the unit group the world holds of its link (`FUN_14102c160` with −1 and 0), its ignore
    /// list empties (`FUN_141197da0`; an object without movement state holds none the slice
    /// keeps), and its owner gives its unit AI the unload order at once (slot 0x50,
    /// `FUN_141667db0` -> the unit AI's slot 0x30 -> slot 0x28 with Immediate set and inFront
    /// clear, [`crate::unit_ai::UnitAi::take_order_now_with`], run outside the unit's update):
    /// issued by the owner, on the unit itself with its master as the target type and the owner
    /// as the target's, at the point (the unit's own place for (−1, −1) unless its master's class
    /// is 3), height 0, within `GarrisonTuning::unload_range`, priority 100, ready (slot 0x30's last argument,
    /// 1); slot 0x30 shows the order at its x and y.
    pub(super) fn unload_order(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        point: Point,
    ) -> Result<(), GenieError> {
        let id = self.players.live(player, idx)?.id;
        self.leave_group_of(id, None)?;
        let unit = self.players.live_mut(player, idx)?;
        if let Some(m) = self.parts.mover_mut(unit.id) {
            m.ignore_units.clear();
        }
        let unit = self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        // UCOMISS with −1, JP and JNZ: a NaN coordinate is not −1
        let to = if !self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::BUILDING)
            && point == Point::new(NO_POINT, NO_POINT)
        {
            unit.position().ground()
        } else {
            point
        };
        let order = QueuedOrder {
            issuer: Some(Issuer::Player(player)),
            order: OrderKind::Unload,
            priority: OrderPriority::COMMAND,
            target: Some(id),
            target_owner: Some(player),
            point: to.with_height(0.0),
            range: Distance::new(self.model.tuning.garrison.unload_range),
            shown: to,
            ready: true,
            target_type: Some(OrderTargetType::of_master(master.id)),
        };
        if !self.parts.unit_ai.contains(unit.id) {
            return Ok(());
        }
        self.run_unit_ai((player, idx), |ai, at, side, player_type, rng| {
            ai.take_order_now_with(order, (at, side, player_type), rng)
        })
    }

    /// `GO_BACK_TO_WORK` (`do_command` 0x80 with one object, 0x86 with several:
    /// `FUN_141625590`) on the player's objects `ids` (another player's only while players may
    /// order each other's, [`crate::world::settings::Settings::cheats_enabled`]; the cheat `+0xb12` is not held): a building
    /// lets out, from the last in, each unit inside of type 70 or more that is a villager (class
    /// 4) or of master 1808, back to its work ([`World::back_to_work`]).
    pub(crate) fn go_back_to_work(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        for id in self.checked_units(player, ids).into_iter().flatten() {
            let Some((p, _)) = self.object(id) else {
                continue;
            };
            let _ = self.players.player(p)?;
            let Some(idx) = self.live_index(p, id) else {
                return Err(GenieError::unsupported(
                    "a GO_BACK_TO_WORK of a sleeping object",
                ));
            };
            let b = self.players.live(p, idx)?;
            if b.master(self.master_tables(), p)?.kind != MasterKind::BUILDING {
                continue;
            }
            let mut i = b.garrison.len();
            while 0 < i {
                i -= 1;
                let Some(&inside) = self.players.live(p, idx)?.garrison.get(i) else {
                    continue;
                };
                // any player's unit (an ally's too); the game skips a null entry, and an id the
                // world no longer holds is one
                let Some((up, u)) = self.object(inside) else {
                    continue;
                };
                let m = u.master(self.master_tables(), up)?;
                if MasterKind::COMBAT <= m.kind
                    && (self
                        .model
                        .roles
                        .of_class(m.class)
                        .contains(ClassRoles::VILLAGER)
                        || self
                            .model
                            .roles
                            .of_master(m.id)
                            .contains(MasterRoles::MULE_CART))
                {
                    let _ = self.back_to_work(Some((p, idx)), inside)?;
                }
            }
        }
        Ok(())
    }

    /// `FUN_1412081e0` for the unit `id`, `from` the building it is inside (player `p`'s object
    /// at an index) or outside (None): the work to go back to ([`TownBell::back_to_work`]: the
    /// town-bell records' object the world holds); with none but a record of an action and a
    /// target class, the unit AI takes the action (`FUN_140c64b50`) and the unit tells it to
    /// search (notify 0x202 with the action, the class and the old target). From a building the
    /// unit then leaves it toward the record's point with the object to work on
    /// ([`World::let_out`], `FUN_1411e7220` with its sound); outside it goes back
    /// ([`World::back_outside`]). Once out or back it is no longer under the town bell
    /// (`+0x9dc`; `+0xa14` takes 1, not held). True when it went.
    ///
    /// [`TownBell::back_to_work`]: crate::orders::town_bell::TownBell::back_to_work
    pub(crate) fn back_to_work(
        &mut self,
        from: Option<(PlayerId, ListIndex)>,
        id: ObjectId,
    ) -> Result<bool, GenieError> {
        use crate::orders::town_bell::RecordTarget;
        let (up, ui) = match from {
            Some(_) => self.live_inside(id)?,
            None => match self.place(id) {
                Some(Place::Live(p, i)) => (p, i),
                // the callers pass an object of type 70 or more the world holds, which never
                // sleeps
                _ => return Ok(false),
            },
        };
        let mut bell = self.parts.town_bell_of(id);
        let lookup = |t: ObjectId| -> Result<Option<RecordTarget>, GenieError> {
            let Some((op, o)) = self.object(t) else {
                return Ok(None);
            };
            let m = o.master(self.master_tables(), op)?;
            Ok(Some(RecordTarget {
                id: t,
                building: m.kind == MasterKind::BUILDING,
                class: m.class,
                built: self.parts.building_of(o.id).construction.is_some_and(|c| {
                    Seconds::of_master(m.creatable_part().build_time) <= c.progress
                }),
            }))
        };
        let (found, record) = bell.back_to_work(&lookup, &self.model.roles)?;
        self.parts.set_town_bell(id, bell);
        if found.is_none() && record.target_class.is_some() && record.action.is_some() {
            let unit = self.players.live_mut(up, ui)?;
            if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
                ai.set_action(record.action);
            }
            unit.post_own(
                &mut self.parts.unit_ai,
                NotifyKind::REQUIRES_SEARCH,
                [
                    AiAction::field(record.action),
                    i32::from(ClassId::field(record.target_class)),
                    ObjectId::field(record.target),
                ],
            )?;
        }
        let Some((p, building)) = from else {
            return self.back_outside(id, found.map(|f| f.id), record.point);
        };
        let out = self.let_out(p, building, id, record.point, (found.map(|f| f.id), true))?;
        if out && self.object(id).is_some() {
            self.parts.set_town_bell_flag(id, false);
        }
        Ok(out)
    }

    /// `FUN_14118a610`: whether UNLOAD's `pick` takes `unit` of player `player`, by its
    /// master's class and id ([`UnloadPick`]); with the object of flags 4 to 6 of the world
    /// (`FUN_1415417a0`; the unit's player's or none), 4 the units of its master (a villager:
    /// any villager for a villager), 5 every unit but it and class 42, 6 the units not of its
    /// master (no villager for a villager, never class 42).
    pub(super) fn unload_picks(
        &self,
        player: PlayerId,
        unit: &Unit,
        pick: UnloadPick,
    ) -> Result<bool, GenieError> {
        let id = unit.id;
        let master = unit.master(self.master_tables(), player)?;
        let class = master.class;
        // the object flags 4 to 6 name: its master, and whether it is the unit's player's (or
        // the world does not hold it)
        let named = match pick {
            UnloadPick::SameMaster(o) | UnloadPick::AllBut(o) | UnloadPick::OtherMasters(o) => self
                .object(o)
                .map(|(p, o)| o.master(self.master_tables(), p).map(|m| (p, m)))
                .transpose()?,
            _ => None,
        };
        let same_player = named.is_none_or(|(p, _)| p == player);
        let named = named.map(|(_, m)| m);
        Ok(match pick {
            UnloadPick::All(_) => !self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::WHOLE_GARRISON_SHARE),
            UnloadPick::Player(number) => player.index() as i32 == number,
            UnloadPick::Group(group) => unload_group_takes(group, class, &self.model.roles),
            UnloadPick::Object(o) => Some(id) == o,
            UnloadPick::SameMaster(_) => match named.filter(|_| same_player) {
                Some(m) if m.id == master.id => true,
                Some(m) => {
                    self.model
                        .roles
                        .of_class(class)
                        .contains(ClassRoles::VILLAGER)
                        && self
                            .model
                            .roles
                            .of_class(m.class)
                            .contains(ClassRoles::VILLAGER)
                }
                None => false,
            },
            UnloadPick::AllBut(o) => {
                same_player
                    && Some(id) != o
                    && !self
                        .model
                        .roles
                        .of_class(class)
                        .contains(ClassRoles::WHOLE_GARRISON_SHARE)
            }
            UnloadPick::OtherMasters(_) => match named.filter(|_| same_player) {
                Some(m) if m.id != master.id => {
                    if self
                        .model
                        .roles
                        .of_class(class)
                        .contains(ClassRoles::VILLAGER)
                    {
                        !self
                            .model
                            .roles
                            .of_class(m.class)
                            .contains(ClassRoles::VILLAGER)
                    } else {
                        !self
                            .model
                            .roles
                            .of_class(class)
                            .contains(ClassRoles::WHOLE_GARRISON_SHARE)
                    }
                }
                _ => false,
            },
            UnloadPick::Other(..) => false,
        })
    }
}

/// UNLOAD's flag 2 (`FUN_14118a610`): the classes of group `group` (1 villagers and class 19;
/// 2 classes 6, 45, 46, 50 and 56; 3 classes 12, 23, 36, 47 and 57; 5 classes 0 and 44; 6
/// classes 18, 42 and 43; no other group).
pub(super) fn unload_group_takes(group: i32, class: ClassId, roles: &Roles) -> bool {
    match group {
        1 => roles
            .of_class(class)
            .intersects(ClassRoles::VILLAGER | ClassRoles::TRADE_CART),
        2 => roles.of_class(class).intersects(
            ClassRoles::INFANTRY
                | ClassRoles::TWO_HANDED_SWORDSMAN
                | ClassRoles::PIKEMAN
                | ClassRoles::SPEARMAN
                | ClassRoles::RAIDER,
        ),
        3 => roles.of_class(class).intersects(
            ClassRoles::CAVALRY
                | ClassRoles::CAVALRY_CANNON
                | ClassRoles::CAVALRY_ARCHER
                | ClassRoles::SCOUT_CAVALRY
                | ClassRoles::CAVALRY_RAIDER,
        ),
        5 => roles
            .of_class(class)
            .intersects(ClassRoles::ARCHERY | ClassRoles::ARCHERY_CANNON),
        6 => roles.of_class(class).intersects(
            ClassRoles::MONK | ClassRoles::WHOLE_GARRISON_SHARE | ClassRoles::MONK_WITH_RELIC,
        ),
        _ => false,
    }
}
