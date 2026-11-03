//! The Transport in the world: what it reads of its unit, the unload command, a move, the
//! unload and the units put ashore.

use crate::GenieError;
use crate::garrison::SIEGE_UNLOAD;
use crate::garrison::transport::TransportState;
use crate::garrison::transport::{
    Around, Changed, DropDraws, MapShore, NONE, Ship, Shore, TRANSPORT, TransportAction, in_zone,
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
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::{Master, OwnerMasters};
use crate::pathing::zones::ZoneId;
use crate::pathing::zones::{NO_TERRAIN, UNKNOWN_RESTRICTION};
use crate::players::PlayerId;
use crate::production::spawn::{NOT_MOVING, Placing, Tested, WorldMap, placement};
use crate::unit_ai::alert_owner;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::parts::Parts;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;
use crate::world::{Player, World};

/// An unloading ship whose top action is no Transport where it was read as one.
const NO_TRANSPORT: GenieError = GenieError::unreachable("an unloading ship without a Transport");

/// The transports' parts that reach the world: the unload command's install, the unload and the
/// units put ashore.
impl World {
    /// The unit's unload command (slot 0x3a8, `FUN_14108d050`) at `to` for player `p`'s object at
    /// `i`, after the unit AI that gave it: a Transport on top (type 12, `FUN_140a7be70`) is told
    /// to work at the unit's own place ("Telling existing action to work": slot 9 with the unit
    /// itself as its object, [`TransportAction::work`]); else the actions go (`FUN_140a98620`,
    /// [`crate::world::World::clear_actions`]) and a Transport at the unit's place
    /// ([`TransportAction::new`]) replaces them (slot 0x808: its slot 0x70, the base's, does
    /// nothing; the list's add starts it, [`TransportAction::start`]), then takes the work order
    /// at `to` from the unit itself.
    ///
    /// # Errors
    ///
    /// A map without its terrain, and what the actions' teardowns do not model.
    pub(crate) fn transport_order(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        to: Position,
    ) -> Result<(), GenieError> {
        let existing = self
            .parts
            .actions_of(self.players.live(p, i)?.id)
            .top_action_kind()
            == Some(TRANSPORT);
        if !existing {
            self.clear_actions(p, i)?;
        }
        // a new Transport has no task (`TransportAction::new`)
        let task = self
            .parts
            .actions_of(self.players.live(p, i)?.id)
            .list
            .transport()
            .filter(|_| existing)
            .and_then(|t| t.task);
        let ship = {
            let unit = self.players.live(p, i)?;
            let master = unit.master(self.master_tables(), p)?;
            Ship::of_unit(
                (unit, self.parts.mover_of(unit.id)),
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
            .live(p, i)?
            .master(self.master_tables(), p)?
            .terrain_restriction;
        let mut shore = MapShore::new(
            &self.spatial,
            &mut self.zones,
            &mut self.pathing,
            restriction,
            ship.position,
            &self.model.tuning.tile_path,
        )?;
        let unit = self.players.live_mut(p, i)?;
        let changes = if let Some(t) = self
            .parts
            .actions
            .of_mut(unit.id)?
            .list
            .transport_mut()
            .filter(|_| existing)
        {
            vec![t.work(
                ship.position,
                true,
                &ship,
                &mut shore,
                &self.model.tuning.transport,
                &self.model.tuning.move_to,
            )?]
        } else {
            let mut t = TransportAction::new(ship.position);
            let started = t.start(
                true,
                &ship,
                &mut shore,
                &self.model.tuning.transport,
                &self.model.tuning.move_to,
            )?;
            let worked = t.work(
                to,
                true,
                &ship,
                &mut shore,
                &self.model.tuning.transport,
                &self.model.tuning.move_to,
            )?;
            self.parts
                .actions
                .of_mut(unit.id)?
                .list
                .set_transport(Some(t));
            vec![started, worked]
        };
        for c in changes {
            self.transport_changed(p, i, c)?;
        }
        Ok(())
    }

    /// The Transport of player `p`'s object at `i` takes a move to `point` (its slot 8,
    /// [`crate::objects::unit::Unit::transport_sail`]).
    ///
    /// # Errors
    ///
    /// A map without its terrain, a unit of a master the slice does not have.
    pub(crate) fn transport_move(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        point: Position,
    ) -> Result<(), GenieError> {
        let rng = self.streams.get(Stream::Sprite);
        let id = self.players.live(p, i)?.id;
        self.players.live_mut(p, i)?.transport_sail(
            self.parts.actions.of_mut(id)?,
            self.parts.motion.layer_mut(id),
            point,
            (&self.spatial, &mut self.zones, &mut self.pathing),
            &self.model,
            (
                OwnerMasters::new(&self.parts.own_master, self.model.masters.of(p)),
                self.parts.combat.of(id).abilities.look.as_ref(),
                rng,
            ),
        )
    }

    /// What a state change of the Transport of player `p`'s object at `i` leaves ([`Changed`]):
    /// the unit stops (its `MoveTo`'s teardown, slot 0x5d0), then takes its standing sprite (slot
    /// 0x68, [`crate::objects::unit::Unit::show_standing`]).
    pub(super) fn transport_changed(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        c: Changed,
    ) -> Result<(), GenieError> {
        let unit = self.players.live_mut(p, i)?;
        if c.stopped
            && let Some(m) = self.parts.mover_mut(unit.id)
        {
            m.stop()?;
        }
        if c.stands {
            let rng = self.streams.get(Stream::Sprite);
            unit.show_standing(
                self.parts.motion.layer_mut(unit.id),
                OwnerMasters::new(&self.parts.own_master, self.model.masters.of(p)),
                &self.model.graphics,
                rng,
                self.model.tuning.sprite,
            )?;
        }
        Ok(())
    }

    /// The unload of the Transport of player `p`'s object at `i` in its update's state 6
    /// (`FUN_140aa3ba0`, [`Updated::Unload`]): the units inside go ashore in the zone of
    /// the point's tile ([`Shore::zone`]; any zone without a point) ([`World::unload_all`]).
    /// With none ashore the point moves ([`retry_point`], the Action stream) and the Transport
    /// sails there to unload (`set_state(4)`, then `+0x68`) when a draw found land, else the
    /// owner hears 0x207 (no unload there); with some ashore and more inside the owner hears
    /// 0x206 (its slot 0x2e8 with the transport's action value: the interface's messages, the
    /// old pather's hulled area, a computer player's AI, [`crate::unit_ai::alert_owner`]). Then
    /// the update's end ([`TransportAction::unloaded`]).
    ///
    /// # Errors
    ///
    /// A map without its terrain, a computer player's AI hearing it, and what the units put
    /// ashore do not model.
    ///
    /// [`Updated::Unload`]: crate::garrison::transport::Updated::Unload
    pub(crate) fn transport_unload(&mut self, p: PlayerId, i: ListIndex) -> Result<(), GenieError> {
        let unit = self.players.live(p, i)?;
        let t = self
            .parts
            .actions_of(unit.id)
            .list
            .transport()
            .copied()
            .ok_or(GenieError::unreachable("an unload without its Transport"))?;
        let restriction = unit.master(self.master_tables(), p)?.terrain_restriction;
        let (x, y) = (t.point.x(), t.point.y());
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
            Some(shore.zone(t.point.tile())?)
        };
        let count = self.unload_all(p, i, zone)?;
        let player_type = self.players.player(p)?.kind;
        let ship = {
            let unit = self.players.live(p, i)?;
            let master = unit.master(self.master_tables(), p)?;
            Ship::of_unit(
                (unit, self.parts.mover_of(unit.id)),
                master,
                self.spatial.map,
                (
                    &self.model.graphics,
                    self.parts.combat_of(unit.id).abilities.look.as_ref(),
                ),
                t.task,
                &self.model.roles,
            )?
        };
        let mut changes = Vec::new();
        if count == 0 {
            let terrain = self.spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
            let rng = self.streams.get(Stream::Action);
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
            let t = transport_mut(&self.players, &mut self.parts, p, i)?;
            let (x, y) = (point.x(), point.y());
            t.point = t.point.with_x(x).with_y(y);
            if sails {
                changes.push(t.set_state(
                    TransportState::SailToUnload { move_to: None },
                    &ship,
                    &mut shore,
                    &self.model.tuning.transport,
                    &self.model.tuning.move_to,
                )?);
                t.retarget = true;
            } else {
                alert_owner(player_type)?;
            }
        } else if ship.loaded {
            alert_owner(player_type)?;
        }
        let mut shore = MapShore::new(
            &self.spatial,
            &mut self.zones,
            &mut self.pathing,
            restriction,
            ship.position,
            &self.model.tuning.tile_path,
        )?;
        let t = transport_mut(&self.players, &mut self.parts, p, i)?;
        changes.push(t.unloaded(
            &ship,
            &mut shore,
            &self.model.tuning.transport,
            &self.model.tuning.move_to,
        )?);
        for c in changes {
            self.transport_changed(p, i, c)?;
        }
        Ok(())
    }

    /// `FUN_141191b40`: the objects inside the transport at (`p`, `i`) go ashore in `zone` (none,
    /// the game's −1, for any), from the last in: each where a drop location is free ([`World::drop_location`])
    /// about the last spot one took (the first: the transport's place and radii), else about the
    /// spot before, and so on back; one that finds none stays inside, and so does every one
    /// after it (the spots are used up). A unit put ashore ([`World::come_ashore`]) makes its
    /// place, with its master's radii, the next spot (up to 29). Returns how many went ashore,
    /// a byte.
    pub(super) fn unload_all(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        zone: Option<ZoneId>,
    ) -> Result<u8, GenieError> {
        const SPOTS: usize = 0x1d;
        let ship = self.players.live(p, i)?;
        let radius = ship.master(self.master_tables(), p)?.radius;
        let mut spots = [Around {
            center: Point::ORIGIN,
            radius: Footprint::ZERO,
        }; SPOTS + 1];
        spots[1] = Around {
            center: ship.position().ground(),
            radius,
        };
        let mut k = 1;
        let mut count = 0u8;
        // the drop locations' last place, across the units (FUN_141191b40's locals)
        let mut last = None;
        let mut j = ship.garrison.len();
        while 0 < j {
            j -= 1;
            let Some(&id) = self.players.live(p, i)?.garrison.get(j) else {
                continue;
            };
            // the game skips a null entry; an id the world no longer holds is one
            let Some((up, u)) = self.object(id) else {
                continue;
            };
            let made = u.master(self.master_tables(), up)?.clone();
            while 0 < k {
                if let Some(spot) =
                    self.drop_location((p, i), id, &made, *spots.checked(k)?, zone, &mut last)?
                {
                    self.come_ashore((p, i), id, &made, spot)?;
                    count = count.wrapping_add(1);
                    if k < SPOTS {
                        k += 1;
                        *spots.checked_mut(k)? = Around {
                            center: spot,
                            radius: made.radius,
                        };
                    }
                    break;
                }
                k -= 1;
            }
        }
        Ok(count)
    }

    /// `FUN_141192d10` for the unit `id` (of master `made`) about `around`, in `zone` of the
    /// transport at (`p`, `i`) ([`DropDraws`] on the Object stream, the place `last` across the
    /// caller's units): the first place on the map in the zone ([`in_zone`] on the transport's
    /// zone map) that is free. A place is free where the unit's master may stand for the
    /// transport's owner (its slot 0x50, [`crate::production::spawn::placement`] with the spawn's arguments:
    /// its arguments 9 and 12, 1 and 0 here, change nothing the slice reads) and the unit could
    /// stand (its slot 0x278, [`crate::map::spatial::Spatial::point_clear`] by its radius and its
    /// restriction's mask).
    pub(crate) fn drop_location(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        id: ObjectId,
        made: &Master,
        around: Around,
        zone: Option<ZoneId>,
        last: &mut Option<Point>,
    ) -> Result<Option<Point>, GenieError> {
        let ship = self.players.live(p, i)?;
        let restriction = ship.master(self.master_tables(), p)?.terrain_restriction;
        let terrain = self.spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
        let zone_map = match restriction {
            Some(r) if zone.is_some() => {
                let values = terrain.restriction(r).ok_or(UNKNOWN_RESTRICTION)?;
                Some(
                    self.zones
                        .find(self.spatial.map, &terrain.kinds, values, false),
                )
            }
            _ => None,
        };
        let moving = made.moving.as_ref().ok_or(NOT_MOVING)?;
        let terrain = self.spatial.terrain.as_ref().ok_or(NO_TERRAIN)?;
        let _ = terrain
            .restriction(moving.restriction)
            .ok_or(UNKNOWN_RESTRICTION)?;
        let unit = self.object(id).map(|(_, u)| u);
        let (grow, mask) = unit.and_then(|u| self.parts.mover_of(u.id)).map_or_else(
            || {
                (
                    made.radius.x(),
                    terrain.restriction_mask(moving.restriction),
                )
            },
            |m| (m.stats.radius.x(), m.stats.restriction_mask),
        );
        let zone_map = zone_map.map(|k| self.zones.map(k)).transpose()?;
        let mut rng = self.streams.get(Stream::Object).clone();
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
                    player: p,
                    radius: made.radius,
                    tall: made.tall,
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
                    false,
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
        *self.streams.get(Stream::Object) = rng;
        Ok(found)
    }

    /// `FUN_141191b40`'s put ashore of the unit `id` (of master `made`) from the transport at
    /// (`p`, `i`) at `spot`: it takes the transport's place (its place's fields), leaves it (slot
    /// 0x1a8) for the spot at the transport's height (slot 0x58, [`World::leave_container`]); its
    /// `+0x688`, its box (`+0x6fc`..`+0x704`) and its sprites' places (`FUN_141189d50`), the
    /// interface's, are not held; its unit AI's state position (`+0x1ac`, `+0x1b0`) becomes the
    /// spot. A siege tower's (class 13) unload is not modelled.
    pub(super) fn come_ashore(
        &mut self,
        (p, i): (PlayerId, ListIndex),
        id: ObjectId,
        made: &Master,
        spot: Point,
    ) -> Result<(), GenieError> {
        let ship = self.players.live(p, i)?;
        let at = ship.position();
        if self
            .model
            .roles
            .of_class(ship.master(self.master_tables(), p)?.class)
            .contains(ClassRoles::SIEGE)
        {
            return Err(SIEGE_UNLOAD);
        }
        let (up, ui) = self.live_inside(id)?;
        let unit = self.players.live_mut(up, ui)?;
        let inner_id = unit.id;
        unit.place.inside_at(inner_id, at, &mut self.spatial);
        self.leave_container((p, i), (up, ui), made, spot.with_height(at.z()))?;
        let id = self.players.live(up, ui)?.id;
        if let Some(ai) = self.parts.unit_ai.get_mut(id) {
            ai.state_position = spot;
        }
        Ok(())
    }
}

/// The Transport of player `p`'s object at `i` in `players`, to change in its action layer in
/// `parts`, beside borrows of the world's map tables.
///
/// # Errors
///
/// A slot the world does not hold, and an object without its Transport.
fn transport_mut<'a>(
    players: &[Player],
    parts: &'a mut Parts,
    p: PlayerId,
    i: ListIndex,
) -> Result<&'a mut TransportAction, GenieError> {
    let id = players.live(p, i)?.id;
    parts
        .actions_mut(id)?
        .list
        .transport_mut()
        .ok_or(NO_TRANSPORT)
}
