//! A unit's leave of the building or the transport it is inside (`FUN_1411e7220`), or its
//! place on the map.

use crate::GenieError;
use crate::combat::Firepower;
use crate::commands::GatherPoint;
use crate::commands::GatherPointState;
use crate::garrison::RaiderCountdown;
use crate::garrison::{INSIDE_SIGHT, NO_POINT, SIEGE_UNLOAD, attack_rate, share};
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Obstruction;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlayersMasters;
use crate::orders::Work;
use crate::orders::set_stance;
use crate::players::PlayerId;
use crate::unit_ai::StopOrder;
use crate::vision::SightCentre;
use crate::vision::SightLayer;
use crate::vision::SightShape;
use crate::vision::visibility::LosSight;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::slots::PlayerSlots;

impl World {
    /// The player and live index of the unit `id` inside a building or a transport, whoever's
    /// it is.
    pub(crate) fn live_inside(&self, id: ObjectId) -> Result<(PlayerId, ListIndex), GenieError> {
        match self.place(id) {
            Some(Place::Live(p, i)) => Ok((p, i)),
            _ => Err(GenieError::unsupported(
                "a unit inside a building outside the live lists",
            )),
        }
    }

    /// `FUN_1411e7220`: the unit `id` (any player's, with its own player's masters and orders)
    /// leaves the building at `building` of player `player` toward `point`, when a spot beside
    /// it is free ([`World::spot_beside`], toward the point, or the building's gather point when
    /// not on itself): it takes the spot at the building's
    /// height (`FUN_14119d610`), leaves the building (slot 0x1a8, `FUN_141213120`: the
    /// building's slot 0x1c8, `FUN_1411d6ad0` -> `FUN_141217910`, takes its share off the
    /// volleys, back to its master's volley fire when none is left inside or it fell below it,
    /// and lets it out of its list, `FUN_141197e10`; the building's sight lets the circle of 5
    /// go, slot 0x1d8 with 5) and goes on the map there (slot 0x58: its tile's list, its
    /// obstruction record, the mesh, its sight). With a gather point (`+0xc30` 1) not on the
    /// building and no point, it then goes to work on the gather point's target when the world
    /// holds it (its unit AI forgets its target, `FUN_140c5e1e0`; [`World::gather`]); else, or
    /// with a point, it moves there ([`World::gather_move`] after the same forgetting). False
    /// when no spot is free. With neither a point nor a gather point its unit AI stops, forgets
    /// its target and takes its stance again (slots 0xb0 and 0x1c0). With an object to work on
    /// (`work_on`, `GO_BACK_TO_WORK`'s) its work command (slot 0x128) goes to it at the point first.
    /// Not modelled: class 42's and livestock's counts in a building of a player with `+0x88`, a siege
    /// building's (class 13) slot 0x1b0; not held: the building's villager count (`+0xba8`) and
    /// raider count (`+0xc2c`, which the building's slot 0x1c8 `FUN_1411d6ad0` lowers for a unit
    /// of type 70 whose `+0x9a0` runs: such a unit inside is refused, [`World::garrisoned`]), its
    /// sound (slot 0x4c0), its garrison sprite. Its work command (slot 0x128) has `+0xa14` 0: the
    /// unit AI keeps its notifies and its queued orders.
    pub(crate) fn let_out(
        &mut self,
        player: PlayerId,
        building: ListIndex,
        id: ObjectId,
        point: Point,
        (work_on, _sound): (Option<ObjectId>, bool),
    ) -> Result<bool, GenieError> {
        let b = self.players.live(player, building)?;
        let bid = b.id;
        let bm = b.master(self.master_tables(), player)?.clone();
        let gather = self.parts.gather_point.get(bid).copied();
        // the spot's goal (`FUN_1411d12d0` with x −1): without a gather point the first location
        // of the building unit AI's command queue (slot 0x218, then slot 0x220), else a gather
        // point that is not on the building, else none
        let queue_first = self
            .parts
            .unit_ai
            .get(b.id)
            .and_then(|a| a.building_orders.first())
            .map(|o| o.point.ground());
        let toward = if point.x() == NO_POINT {
            match (gather, queue_first) {
                (None, Some(q)) => Some(q),
                _ => gather
                    .filter(|g| g.target != Some(bid))
                    .map(|g| g.point.ground()),
            }
        } else {
            Some(point)
        };
        // the unit is any player's (an ally's too): its own player's masters and orders
        let (up, ui) = self.live_inside(id)?;
        let unit = self.players.live(up, ui)?;
        let made = unit.master(self.master_tables(), up)?.clone();
        // The class of siege weapons, whose own place an Enter's point keeps (`FUN_140a85f30`, the
        // short `0xd`).
        if self
            .model
            .roles
            .of_class(bm.class)
            .contains(ClassRoles::SIEGE)
        {
            return Err(SIEGE_UNLOAD);
        }
        if self
            .model
            .roles
            .of_class(made.class)
            .contains(ClassRoles::WHOLE_GARRISON_SHARE)
            || self
                .model
                .roles
                .of_class(made.class)
                .contains(ClassRoles::LIVESTOCK)
        {
            return Err(GenieError::unsupported(
                "a class 42 or livestock unit leaving a building (its player's counts)",
            ));
        }
        if !unit.garrison.is_empty() {
            return Err(GenieError::unsupported(
                "a unit with units inside leaving a building",
            ));
        }
        let Ok(spot) = self.spot_beside(player, building, &made, toward) else {
            return Err(GenieError::unsupported(
                "an unload whose unit finds no spot (its later rings)",
            ));
        };
        let z = self.players.live(player, building)?.position().z();
        let at = spot.with_height(z);
        // slot 0x1a8 (`FUN_141213120`) does its part only for a unit inside an object (`+0x2f0`):
        // a new unit an effect could not take in (`FUN_1416136f0`) comes out from where it stands
        let inside = self.players.live(up, ui)?.life.inside.is_some();
        if !inside {
            return self.let_out_from_the_map(up, ui, (&made, at), (point, gather, bid, work_on));
        }
        self.leave_container((player, building), (up, ui), &made, at)?;
        self.came_out(up, ui, made.class, (point, gather, bid, work_on))
    }

    /// Slot 0x1a8 (`FUN_141213120`) of the unit at (`up`, `ui`), of master `made`, inside the
    /// object at (`player`, `container`), then its slot 0x58 at `at`. The container's slot 0x1c8
    /// (`FUN_141217910`; a building's `FUN_1411d6ad0` adds its counts, not held) takes the unit's
    /// share off its volleys, back to its master's volley fire when none is left inside or it fell
    /// below it, and lets it out of its list (`FUN_141197e10`); a building's sight lets the circle
    /// of 5 go (slot 0x1d8 with 5); the unit's countdown to its raid (`+0x9a0`) ends; a container
    /// of type 70 or more takes off its `+0x9a8` what the unit holds inside (slot 0x498), none in
    /// the slice. Slot 0x58 puts the unit on the map at `at` (`new_position`: its tile's list,
    /// its obstruction record, the mesh, its sight); its sprites (`FUN_14118a8f0`) are the
    /// interface's.
    ///
    /// # Errors
    ///
    /// A unit without its record, an unload without the masters, and the sight's unsupported
    /// cases.
    pub(crate) fn leave_container(
        &mut self,
        (player, container): (PlayerId, ListIndex),
        (up, ui): (PlayerId, ListIndex),
        made: &Master,
        at: Position,
    ) -> Result<(), GenieError> {
        let b = self.players.live(player, container)?;
        let (bid, id) = (b.id, self.players.live(up, ui)?.id);
        let bm = b.master(self.master_tables(), player)?.clone();
        let share = self.unloaded_share((player, container), (up, ui), made, &bm)?;
        let b = self.players.live_mut(player, container)?;
        if let Some(share) = share {
            self.parts.combat_mut(b.id)?.firepower -= share;
        }
        // the unit out of its list (`FUN_141197e10`)
        if let Some(k) = b.garrison.iter().position(|&g| g == id) {
            let _ = b.garrison.swap_remove(k);
        }
        // COMISS volley, firepower; JBE keeps it: a NaN firepower stays
        if b.garrison.is_empty() || self.parts.combat_of(b.id).firepower < bm.volley_fire {
            self.parts.combat_mut(b.id)?.firepower = bm.volley_fire;
        }
        // a building's sight lets the circle of 5 go (slot 0x1d8 with 5)
        let (building_at, building_radius) = (b.position(), bm.radius);
        if bm.kind == MasterKind::BUILDING
            && let Some(v) = self.spatial.sight.as_mut()
        {
            v.sight_off_los(
                bid,
                LosSight::tile_circle(building_at, building_radius, INSIDE_SIGHT),
                &self.model.roles,
                self.model.tuning.visibility,
            )?;
        }
        // slot 0x58: on the map at the spot (new_position: its tile's list, its sight)
        let mut placed = self.spatial.come_out(id).ok_or_else(|| {
            GenieError::unsupported("a unit inside a building without its record")
                .with("unit", id)
                .with("building", bid)
        })?;
        let solid = 0.0 < made.radius.x() && 0.0 < made.radius.y() && made.tall;
        placed.obstruction =
            (made.obstruction_kind != ObstructionKind::NONE && solid).then_some(Obstruction {
                kind: made.obstruction_kind,
                class: made.obstruction_class,
            });
        let world_time = self.world_time;
        let unit = self.players.live_mut(up, ui)?;
        let _ = unit
            .place
            .put_down(id, at, placed, (&mut self.spatial, &mut self.pathing));
        self.pathing.tiles_changed(self.spatial.take_changes());
        unit.life.inside = None;
        // a unit out of its container no longer counts down to its raid (`+0x9a0` 0)
        self.parts.combat_mut(unit.id)?.raider_countdown = RaiderCountdown::NONE;
        // the position setter's end: a place off where it last moved to is a move
        if let Some(m) = self.parts.mover_mut(unit.id) {
            m.mark_move(at, world_time);
        }
        let state = unit.life.state;
        if let Some(v) = self.spatial.sight.as_mut()
            && state == ObjectState::Alive
        {
            let _ = v.sight_on(
                id,
                at,
                made.radius,
                SightShape::Circle,
                SightCentre::Tile,
                SightLayer {
                    roles: &self.model.roles,
                    visibility_tuning: self.model.tuning.visibility,
                },
            )?;
        }
        if let Some(e) = self.take_sight_unsupported() {
            return Err(e);
        }
        Ok(())
    }

    /// [`World::leave_container`]'s share of the unit at (`up`, `ui`), of master `made`, off the
    /// volleys of the container at (`player`, `container`), of master `bm` (its slot 0x1c8,
    /// `FUN_141217910`, a building's through `FUN_1411d6ad0`): a unit (type 70) only, the
    /// container's rate by its player's masters, the unit's share by its own.
    ///
    /// # Errors
    ///
    /// An unload without the masters.
    fn unloaded_share(
        &self,
        (player, container): (PlayerId, ListIndex),
        (up, ui): (PlayerId, ListIndex),
        made: &Master,
        bm: &Master,
    ) -> Result<Option<Firepower>, GenieError> {
        if made.kind != MasterKind::COMBAT {
            return Ok(None);
        }
        let (Some(own), Some(theirs)) = (self.model.masters.of(player), self.model.masters.of(up))
        else {
            return Err(GenieError::unsupported("an unload without the masters"));
        };
        let b = self.players.live(player, container)?;
        let unit = self.players.live(up, ui)?;
        let rate = attack_rate(
            bm,
            own,
            self.parts.combat_of(b.id).reload_powerup,
            &self.model.roles,
            &self.model.tuning.garrison,
        );
        Ok(Some(share(
            made,
            theirs,
            self.parts.combat_of(unit.id).reload_powerup,
            rate,
            &self.model.roles,
        )?))
    }

    /// [`World::let_out`]'s end once the unit stands at its spot: the work given, the gather
    /// point, the stop and stance, or the move to the point.
    ///
    /// # Errors
    ///
    /// What the work, the gather and the move do not model.
    pub(super) fn came_out(
        &mut self,
        up: PlayerId,
        ui: ListIndex,
        class: ClassId,
        (point, gather, bid, work_on): (Point, Option<GatherPoint>, ObjectId, Option<ObjectId>),
    ) -> Result<bool, GenieError> {
        // an object to work on (GO_BACK_TO_WORK's): slot 0x128 on it at the point, the unit's
        // `+0xa14` 0 (its unit AI keeps its notifies)
        if let Some(target) = work_on {
            let work = Work {
                target: Some(target),
                point: point.with_height(0.0),
                carry_test: false,
                filter: None,
                notified: false,
            };
            self.order_work(up, ui, &work)?;
            return Ok(true);
        }
        // the gather point (`+0xc30` 1, not on the building) gives a unit without a point its
        // target, when the world holds it, else its point
        let to = match let_out_aim(gather, point, bid) {
            LetOutAim::Target(g) => {
                if self.object(g.target).is_some() {
                    if let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(up, ui)?.id) {
                        ai.remove_target();
                    }
                    // slot 0x128 with `+0xa14` 0: the unit AI keeps its notifies
                    self.gather(up, ui, g, bid, false, false)?;
                    return Ok(true);
                }
                g.point.ground()
            }
            LetOutAim::Point(p) => p,
        };
        if to.x() == NO_POINT || to.y() == NO_POINT {
            // neither a point nor a gather point: the unit AI stops its action (slot 0xb0 with
            // 1, whose stopAction runs on the unit), forgets its target (`FUN_140c5e1e0`) and
            // takes its own stance again at the unit's place (slot 0x1c0 with `+0x1a9`); a unit
            // without a unit AI stays where it came out
            let unit = self.players.live_mut(up, ui)?;
            let place = (unit.position().ground(), Some(class));
            if self.parts.unit_ai.contains(unit.id) {
                let roles = self.model.roles.clone();
                self.run_ai_calls((up, ui), |ai, side, rng| {
                    ai.stop_with(StopOrder::Keep, Some(place), side, rng, &roles)?;
                    ai.remove_target();
                    let stance = ai.stance;
                    set_stance(ai, stance, place, side, rng, &roles)
                })?;
            }
            return Ok(true);
        }
        // a move to the point: the unit AI forgets its target (`FUN_140c5e1e0`), then slot 0x2a0
        // and slot 0x128 with no target, `+0xa14` 0
        if let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(up, ui)?.id) {
            ai.remove_target();
        }
        self.gather_move(up, ui, to.with_height(0.0), false)?;
        Ok(true)
    }

    /// [`World::let_out`] for a unit that is not inside an object (a new unit an effect could not
    /// take in, `FUN_1416136f0`): slot 0x1a8 does nothing, then slot 0x58 moves it from where it
    /// stands to the spot `at` (`new_position`: its tile's list, its obstruction record, the mesh,
    /// its sight moved), then [`World::came_out`].
    ///
    /// # Errors
    ///
    /// A unit without a place, and what [`World::came_out`] does not model.
    pub(super) fn let_out_from_the_map(
        &mut self,
        up: PlayerId,
        ui: ListIndex,
        (made, at): (&Master, Position),
        rest: (Point, Option<GatherPoint>, ObjectId, Option<ObjectId>),
    ) -> Result<bool, GenieError> {
        let id = self.players.live(up, ui)?.id;
        let mut placed = *self
            .spatial
            .get(id)
            .ok_or(GenieError::unsupported("a unit let out without a place"))?;
        let from = placed.position;
        self.pathing.removed(id);
        self.spatial.remove(id);
        let solid = 0.0 < made.radius.x() && 0.0 < made.radius.y() && made.tall;
        placed.obstruction =
            (made.obstruction_kind != ObstructionKind::NONE && solid).then_some(Obstruction {
                kind: made.obstruction_kind,
                class: made.obstruction_class,
            });
        let world_time = self.world_time;
        let unit = self.players.live_mut(up, ui)?;
        let placed = unit
            .place
            .put_down(id, at, placed, (&mut self.spatial, &mut self.pathing));
        self.pathing.tiles_changed(self.spatial.take_changes());
        if let Some(v) = self.spatial.sight.as_mut() {
            v.moved(
                id,
                from,
                at,
                placed.radius,
                &self.model.roles,
                self.model.tuning.visibility,
            )?;
            v.registry_moved(id, placed.class, from, at);
        }
        let unit = self.players.live_mut(up, ui)?;
        if let Some(m) = self.parts.mover_mut(unit.id) {
            m.mark_move(at, world_time);
        }
        self.came_out(up, ui, made.class, rest)
    }
}

/// Where a unit let out of a building goes (`FUN_1411e7220`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum LetOutAim {
    /// To work on the gather point's target (state 1, no point given, not on the building).
    Target(GatherPoint),
    /// To a point.
    Point(Point),
}

/// The aim of a let-out toward `point` ([`NO_POINT`] for none) from the building `building`
/// with `gather`: a gather point of state 1 ([`GatherPointState::POINT`]) gives a unit without a
/// point its target, one of state 0xff ([`GatherPointState::KEPT`]) its x and y whatever point
/// was given, else the point.
pub(super) fn let_out_aim(
    gather: Option<GatherPoint>,
    point: Point,
    building: ObjectId,
) -> LetOutAim {
    match gather {
        Some(g)
            if point.x() == NO_POINT
                && g.state == GatherPointState::POINT
                && g.target != Some(building) =>
        {
            LetOutAim::Target(g)
        }
        Some(g) if g.state == GatherPointState::KEPT => LetOutAim::Point(g.point.ground()),
        _ => LetOutAim::Point(point),
    }
}
