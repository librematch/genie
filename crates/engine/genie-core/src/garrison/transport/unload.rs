//! The Transport in the world: what it reads of its unit, the unload command, a move, the
//! unload and the units put ashore.

use crate::garrison::SIEGE_UNLOAD;
use crate::garrison::transport::StartCall;
use crate::garrison::transport::TransportState;
use crate::garrison::transport::TransportTarget;
use crate::garrison::transport::unit::SailWorld;
use crate::garrison::transport::{
    Around, Changed, DropDraws, MapShore, NONE, Ship, TRANSPORT, TransportAction, in_zone,
    retry_point,
};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::Rect;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::{Master, OwnerMasters};
use crate::pathing::zones::Relabel;
use crate::pathing::zones::ZoneId;
use crate::pathing::zones::{NO_TERRAIN, UNKNOWN_RESTRICTION};
use crate::players::PlayerId;
use crate::production::spawn::ZoneTest;
use crate::production::spawn::{NOT_MOVING, Placing, Tested, WorldMap, placement};
use crate::unit_ai::alert_owner;
use crate::world::layer::ClassRoles;
use crate::world::objects_ref::ObjectsRef;
use crate::world::parts::Parts;
use crate::world::slots::CheckedIndexExt;
use crate::world::slots::PlayerSlotsExt;
use crate::world::{Player, World};
use crate::{GenieError, GenieResult};

/// An unloading ship without the Transport it was lent with: the unload runs on a ship whose
/// Transport `TransportAction::unload` wrote back to its list before the lend.
const NO_TRANSPORT: GenieError = GenieError::unreachable("an unloading ship without a Transport");

/// The transports' parts that reach the world: the unload command's install, the unload and the
/// units put ashore.
impl World {
    /// The unit's unload command (slot 0x3a8, `FUN_14108d050`) at `to` for player `player_id`'s
    /// object at `list_index`, after the unit AI that gave it: a Transport on top (type 12,
    /// `FUN_140a7be70`) is told to work at the unit's own place ("Telling existing action to work":
    /// slot 9 with the unit itself as its object, [`TransportAction::work`]); else the actions go
    /// (`FUN_140a98620`, [`crate::world::World::clear_actions`]) and a Transport at the unit's
    /// place ([`TransportAction::new`]) replaces them (slot 0x808: its slot 0x70, the base's, does
    /// nothing; the list's add starts it, [`TransportAction::start`]), then takes the work order at
    /// `to` from the unit itself.
    ///
    /// # Errors
    ///
    /// A map without its terrain, and what the actions' teardowns do not model.
    pub(crate) fn transport_order(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        to: Position,
    ) -> GenieResult<()> {
        let existing = self
            .parts
            .actions_of(self.players.live(player_id, list_index)?.id)
            .top_action_kind()
            == Some(TRANSPORT);
        if !existing {
            self.clear_actions(player_id, list_index)?;
        }
        // a new Transport has no task (`TransportAction::new`)
        let task = self
            .parts
            .actions_of(self.players.live(player_id, list_index)?.id)
            .list
            .transport()
            .filter(|_| existing)
            .and_then(|transport_action| transport_action.task);
        let ship = {
            let unit = self.players.live(player_id, list_index)?;
            let master = unit.master(self.master_tables(), player_id)?;
            Ship::of_unit(
                unit,
                self.top_speed(player_id, unit)?,
                master,
                self.spatial.map,
                (
                    &self.model.graphics,
                    self.parts.combat_of(unit.id).abilities.look.as_ref(),
                ),
                task,
                &self.model.roles,
            )?
        };
        let restriction = self
            .players
            .live(player_id, list_index)?
            .master(self.master_tables(), player_id)?
            .terrain_restriction;
        let mut shore = MapShore::new(
            &self.spatial,
            &mut self.zones,
            &mut self.pathing,
            restriction,
            ship.position,
            &self.model.tuning.tile_path,
        )?;
        let unit = self.players.live_mut(player_id, list_index)?;
        let changes = if let Some(transport_action) = self
            .parts
            .actions
            .of_mut(unit.id)?
            .list
            .transport_mut()
            .filter(|_| existing)
        {
            vec![transport_action.work(
                ship.position,
                TransportTarget::Ship,
                &ship,
                &mut shore,
                &self.model.tuning.transport,
                &self.model.tuning.move_to,
            )?]
        } else {
            let mut transport_action = TransportAction::new(ship.position);
            let started = transport_action.start(
                StartCall::Added,
                &ship,
                &mut shore,
                &self.model.tuning.transport,
                &self.model.tuning.move_to,
            )?;
            let worked = transport_action.work(
                to,
                TransportTarget::Ship,
                &ship,
                &mut shore,
                &self.model.tuning.transport,
                &self.model.tuning.move_to,
            )?;
            self.parts
                .actions
                .of_mut(unit.id)?
                .list
                .set_transport(Some(transport_action));
            vec![started, worked]
        };
        for changed in changes {
            self.transport_changed(player_id, list_index, changed)?;
        }
        Ok(())
    }

    /// The Transport `transport_action` of player `player_id`'s object at `list_index`, its head's
    /// copy, takes a move to `point` (its slot 8, [`crate::objects::unit::Unit::transport_sail`]).
    ///
    /// # Errors
    ///
    /// A map without its terrain, a unit of a master the slice does not have.
    pub(crate) fn transport_move(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        transport_action: TransportAction,
        point: Position,
    ) -> GenieResult<()> {
        let id = self.players.live(player_id, list_index)?.id;
        let top_speed =
            self.top_speed_terms(player_id, self.players.live(player_id, list_index)?)?;
        let rng = self.streams.get_mut(Stream::Sprite);
        self.players
            .live_mut(player_id, list_index)?
            .transport_sail(
                transport_action,
                self.parts.actions.of_mut(id)?,
                self.parts.motion.layer_mut(id),
                top_speed,
                point,
                SailWorld {
                    spatial: &self.spatial,
                    zones: &mut self.zones,
                    pathing: &mut self.pathing,
                    model: &self.model,
                    masters: OwnerMasters::new(
                        &self.parts.own_master,
                        self.model.masters.of(player_id),
                    ),
                    look: self.parts.combat.of(id).abilities.look.as_ref(),
                    sprite_rng: rng,
                },
            )
    }

    /// What a state change of the Transport of player `player_id`'s object at `list_index` leaves
    /// ([`Changed`]): the unit stops (its `MoveTo`'s teardown, slot 0x5d0), then takes its standing
    /// sprite (slot 0x68, [`crate::objects::unit::Unit::show_standing`]).
    pub(super) fn transport_changed(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        changed: Changed,
    ) -> GenieResult<()> {
        let unit = self.players.live_mut(player_id, list_index)?;
        if changed.stopped
            && let Some(mover) = self.parts.mover_mut(unit.id)
        {
            mover.stop()?;
        }
        if changed.stands {
            let rng = self.streams.get_mut(Stream::Sprite);
            unit.show_standing(
                self.parts.motion.layer_mut(unit.id),
                OwnerMasters::new(&self.parts.own_master, self.model.masters.of(player_id)),
                &self.model.graphics,
                rng,
                self.model.tuning.sprite,
            )?;
        }
        Ok(())
    }

    /// The unload of the Transport of player `player_id`'s object at `list_index` in its update's
    /// state 6 (`FUN_140aa3ba0`, [`Updated::Unload`]): the units inside go ashore in the zone of
    /// the point's tile ([`MapShore::zone`]; any zone without a point) ([`World::unload_all`]).
    /// With none ashore the point moves ([`retry_point`], the Action stream) and the Transport
    /// sails there to unload (`set_state(4)`, then `+0x68`) when a draw found land, else the owner
    /// hears 0x207 (no unload there); with some ashore and more inside the owner hears 0x206 (its
    /// slot 0x2e8 with the transport's action value: the interface's messages, the old pather's
    /// hulled area, a computer player's AI, [`crate::unit_ai::alert_owner`]). Then the update's end
    /// ([`TransportAction::unloaded`]).
    ///
    /// # Errors
    ///
    /// A map without its terrain, a computer player's AI hearing it, and what the units put
    /// ashore do not model.
    ///
    /// [`Updated::Unload`]: crate::garrison::transport::Updated::Unload
    pub(crate) fn transport_unload(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
    ) -> GenieResult<()> {
        let unit = self.players.live(player_id, list_index)?;
        let transport = self
            .parts
            .actions_of(unit.id)
            .list
            .transport()
            .copied()
            .ok_or(NO_TRANSPORT)?;
        let restriction = unit
            .master(self.master_tables(), player_id)?
            .terrain_restriction;
        let (x, y) = (transport.point.x(), transport.point.y());
        let zone = if x == NONE {
            None
        } else {
            let mut shore = MapShore::new(
                &self.spatial,
                &mut self.zones,
                &mut self.pathing,
                restriction,
                unit.position(),
                &self.model.tuning.tile_path,
            )?;
            Some(shore.zone(transport.point.tile())?)
        };
        let count = self.unload_all(player_id, list_index, zone)?;
        let player_kind = self.players.player(player_id)?.kind;
        let ship = {
            let unit = self.players.live(player_id, list_index)?;
            let master = unit.master(self.master_tables(), player_id)?;
            Ship::of_unit(
                unit,
                self.top_speed(player_id, unit)?,
                master,
                self.spatial.map,
                (
                    &self.model.graphics,
                    self.parts.combat_of(unit.id).abilities.look.as_ref(),
                ),
                transport.task,
                &self.model.roles,
            )?
        };
        let mut changes = Vec::new();
        if count == 0 {
            let terrain = self.spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
            let rng = self.streams.get_mut(Stream::Action);
            let (point, sails) = retry_point(
                Point::new(x, y),
                ship.map,
                terrain,
                rng,
                &self.model.tuning.transport,
            );
            let mut shore = MapShore::new(
                &self.spatial,
                &mut self.zones,
                &mut self.pathing,
                restriction,
                ship.position,
                &self.model.tuning.tile_path,
            )?;
            let transport_action =
                transport_mut(&self.players, &mut self.parts, player_id, list_index)?;
            let (x, y) = (point.x(), point.y());
            transport_action.point = transport_action.point.with_x(x).with_y(y);
            if sails {
                changes.push(transport_action.set_state(
                    TransportState::SailToUnload { move_to: None },
                    &ship,
                    &mut shore,
                    &self.model.tuning.transport,
                    &self.model.tuning.move_to,
                )?);
                transport_action.retarget = true;
            } else {
                alert_owner(player_kind)?;
            }
        } else if ship.loaded {
            alert_owner(player_kind)?;
        }
        let mut shore = MapShore::new(
            &self.spatial,
            &mut self.zones,
            &mut self.pathing,
            restriction,
            ship.position,
            &self.model.tuning.tile_path,
        )?;
        let transport_action =
            transport_mut(&self.players, &mut self.parts, player_id, list_index)?;
        changes.push(transport_action.unloaded(
            &ship,
            &mut shore,
            &self.model.tuning.transport,
            &self.model.tuning.move_to,
        )?);
        for changed in changes {
            self.transport_changed(player_id, list_index, changed)?;
        }
        Ok(())
    }

    /// `FUN_141191b40`: the objects inside the transport at (`player_id`, `list_index`) go ashore
    /// in `zone` (none, the game's −1, for any), from the last in: each where a drop location is
    /// free ([`World::drop_location`]) about the last spot one took (the first: the transport's
    /// place and radii), else about the spot before, and so on back; one that finds none stays
    /// inside, and so does every one after it (the spots are used up). A unit put ashore
    /// ([`World::come_ashore`]) makes its place, with its master's radii, the next spot (up to 29).
    /// Returns how many went ashore, a byte.
    pub(super) fn unload_all(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        zone: Option<ZoneId>,
    ) -> GenieResult<u8> {
        const SPOTS: usize = 0x1d;
        let ship = self.players.live(player_id, list_index)?;
        let radius = ship.master(self.master_tables(), player_id)?.radius;
        let mut spots = [Around {
            center: Point::ORIGIN,
            radius: Footprint::ZERO,
        }; SPOTS + 1];
        spots[1] = Around {
            center: ship.position().ground(),
            radius,
        };
        let mut spot_index = 1;
        let mut count = 0u8;
        // the drop locations' last place, across the units (FUN_141191b40's locals)
        let mut last = None;
        let mut slot = ship.garrison.len();
        while 0 < slot {
            slot -= 1;
            let Some(&id) = self.players.live(player_id, list_index)?.garrison.get(slot) else {
                continue;
            };
            // the game skips a null entry; an id the world no longer holds is one
            let Some((up, unit)) = self.object(id) else {
                continue;
            };
            let made = unit.master(self.master_tables(), up)?.clone();
            while 0 < spot_index {
                if let Some(spot) = self.drop_location(
                    (player_id, list_index),
                    id,
                    &made,
                    *spots.checked(spot_index)?,
                    zone,
                    &mut last,
                )? {
                    self.come_ashore((player_id, list_index), id, &made, spot)?;
                    count = count.wrapping_add(1);
                    if spot_index < SPOTS {
                        spot_index += 1;
                        *spots.checked_mut(spot_index)? = Around {
                            center: spot,
                            radius: made.radius,
                        };
                    }
                    break;
                }
                spot_index -= 1;
            }
        }
        Ok(count)
    }

    /// `FUN_141192d10` for the unit `id` (of master `made`) about `around`, in `zone` of the
    /// transport at (`player_id`, `list_index`) ([`DropDraws`] on the Object stream, the place
    /// `last` across the caller's units): the first place on the map in the zone ([`in_zone`] on
    /// the transport's zone map) that is free. A place is free where the unit's master may stand
    /// for the transport's owner (its slot 0x50, [`crate::production::spawn::placement`] with the
    /// spawn's arguments: its arguments 9 and 12, 1 and 0 here, change nothing the slice reads) and
    /// the unit could stand (its slot 0x278, [`crate::map::spatial::Spatial::point_clear`] by its
    /// radius and its restriction's mask).
    pub(crate) fn drop_location(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        id: ObjectId,
        made: &Master,
        around: Around,
        zone: Option<ZoneId>,
        last: &mut Option<Point>,
    ) -> GenieResult<Option<Point>> {
        let ship = self.players.live(player_id, list_index)?;
        let restriction = ship
            .master(self.master_tables(), player_id)?
            .terrain_restriction;
        let terrain = self.spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
        let zone_map = match restriction {
            Some(restriction_id) if zone.is_some() => {
                let values = terrain
                    .restriction(restriction_id)
                    .ok_or(UNKNOWN_RESTRICTION)?;
                Some(
                    self.zones
                        .find(self.spatial.map, &terrain.kinds, values, Relabel::Kept),
                )
            }
            _ => None,
        };
        let moving = made.move_stats.as_ref().ok_or(NOT_MOVING)?;
        let terrain = self.spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
        let _ = terrain
            .restriction(moving.restriction)
            .ok_or(UNKNOWN_RESTRICTION)?;
        let unit = self.object(id).map(|(_, unit)| unit);
        let (grow, mask) = unit
            .and_then(|unit| self.parts.mover_of(unit.id))
            .map_or_else(
                || {
                    (
                        made.radius.x(),
                        terrain.restriction_mask(moving.restriction),
                    )
                },
                |mover| (mover.stats.radius.x(), mover.stats.restriction_mask),
            );
        let zone_map = zone_map.map(|index| self.zones.map(index)).transpose()?;
        let mut rng = self.streams.get_mut(Stream::Object).clone();
        let mut draws = DropDraws::new(around, made.radius, self.spatial.map, last, &mut rng);
        let mut found = None;
        // the place's tests, each only after the one before passed
        while let Some(spot) = draws.next_on_map(&self.model.tuning.transport)? {
            let (x, y) = (spot.x(), spot.y());
            if !in_zone(spot, zone, zone_map)? {
                continue;
            }
            let (rx, ry) = (made.radius.x(), made.radius.y());
            self.spatial
                .check_known(Rect::new(x - rx, y - ry, x + rx, y + ry))?;
            // the placing reads the terrain, which `point_clear` below borrows with the map
            let free = {
                let terrain = self.spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
                let values = terrain
                    .restriction(moving.restriction)
                    .ok_or(UNKNOWN_RESTRICTION)?;
                let placing = Placing {
                    player: player_id,
                    radius: made.radius,
                    tall: made.is_tall(),
                    class: made.class,
                    restriction: values,
                    hill_mode: made.hill_mode,
                    placement: made.placement,
                };
                let map = WorldMap { turns: &*self };
                placement(
                    &placing,
                    &map,
                    spot,
                    ZoneTest::Skipped,
                    &self.model.roles,
                    &self.model.tuning.spawn,
                )? == Tested::Free
            };
            if free
                && self.spatial.point_clear(
                    id,
                    spot,
                    Distance::new(grow),
                    mask,
                    ObjectsRef::new(
                        &self.players,
                        &self.index,
                        &self.model.masters,
                        &self.parts,
                        &self.model.roles,
                        &self.model.tuning,
                    ),
                )?
            {
                found = Some(spot);
                break;
            }
        }
        *self.streams.get_mut(Stream::Object) = rng;
        Ok(found)
    }

    /// `FUN_141191b40`'s put ashore of the unit `id` (of master `made`) from the transport at
    /// (`player_id`, `list_index`) at `spot`: it takes the transport's place (its place's fields),
    /// leaves it (slot 0x1a8) for the spot at the transport's height (slot 0x58,
    /// [`World::leave_container`]); its `+0x688`, its box (`+0x6fc`..`+0x704`) and its sprites'
    /// places (`FUN_141189d50`), the interface's, are not held; its unit AI's state position
    /// (`+0x1ac`, `+0x1b0`) becomes the spot. A siege tower's (class 13) unload is not modelled.
    pub(super) fn come_ashore(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        id: ObjectId,
        made: &Master,
        spot: Point,
    ) -> GenieResult<()> {
        let ship = self.players.live(player_id, list_index)?;
        let at = ship.position();
        if self
            .model
            .roles
            .of_class(ship.master(self.master_tables(), player_id)?.class)
            .contains(ClassRoles::SIEGE)
        {
            return Err(SIEGE_UNLOAD);
        }
        let (up, ui) = self.live_inside(id)?;
        let unit = self.players.live_mut(up, ui)?;
        let inner_id = unit.id;
        unit.place.inside_at(inner_id, at, &mut self.spatial);
        self.leave_container(
            (player_id, list_index),
            (up, ui),
            made,
            spot.with_height(at.z()),
        )?;
        let id = self.players.live(up, ui)?.id;
        if let Some(ai) = self.parts.unit_ai.get_mut(id) {
            ai.state_position = spot;
        }
        Ok(())
    }
}

/// The Transport of player `player_id`'s object at `list_index` in `players`, to change in its
/// action layer in `parts`, beside borrows of the world's map tables.
///
/// # Errors
///
/// A slot the world does not hold, and an object without its Transport.
fn transport_mut<'world>(
    players: &[Player],
    parts: &'world mut Parts,
    player_id: PlayerId,
    list_index: ListIndex,
) -> GenieResult<&'world mut TransportAction> {
    let id = players.live(player_id, list_index)?.id;
    parts
        .actions_mut(id)?
        .list
        .transport_mut()
        .ok_or(NO_TRANSPORT)
}
