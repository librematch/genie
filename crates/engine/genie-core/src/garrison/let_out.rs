//! A unit's leave of the building or the transport it is inside (`FUN_1411e7220`), or its
//! place on the map.

use crate::combat::Firepower;
use crate::combat::powerup::{InfantryChange, count_infantry, speeds_up_siege};
use crate::combat::reload::full_reload;
use crate::commands::GatherPoint;
use crate::commands::GatherPointState;
use crate::garrison::GarrisonCounts;
use crate::garrison::Kept;
use crate::garrison::LetOut;
use crate::garrison::RaiderCountdown;
use crate::garrison::{INSIDE_SIGHT, NO_POINT, attack_rate, share};
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Obstruction;
use crate::math::image::garrison::MELEE_REACH;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlayersMastersExt;
use crate::orders::OrderSource;
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
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// A unit inside a building that is not in its player's live list.
pub(crate) const HOUSED_NOT_LIVE: GenieError =
    GenieError::unsupported("a unit inside a building outside the live lists");

impl World {
    /// The player and live index of the unit `id` inside a building or a transport, whoever's
    /// it is.
    pub(crate) fn live_inside(&self, id: ObjectId) -> GenieResult<(PlayerId, ListIndex)> {
        match self.place(id) {
            Some(Place::Live(player_id, list_index)) => Ok((player_id, list_index)),
            _ => Err(HOUSED_NOT_LIVE),
        }
    }

    /// `FUN_1411e7220`: the unit `id` (any player's, with its own player's masters and orders)
    /// leaves the building at `building` of player `player` toward `point`, when a spot beside it
    /// is free ([`World::spot_beside`], toward the point, or the building's gather point when not
    /// on itself; its later rings take a spot inside the building's footprint): it takes the spot
    /// at the building's height (`FUN_14119d610`), leaves the building (slot 0x1a8,
    /// `FUN_141213120`: the building's slot 0x1c8, `FUN_1411d6ad0` -> `FUN_141217910`, takes its
    /// share off the volleys, back to its master's volley fire when none is left inside or it fell
    /// below it, lets it out of its list, `FUN_141197e10`, counts it out of its villagers and
    /// raiders, [`Unit::let_go`], and out of its relics or livestock, [`World::keep_out`]; the
    /// building's sight lets the circle of 5 go, slot 0x1d8 with 5) and goes on the map there (slot
    /// 0x58: its tile's list, its obstruction record, the mesh, its sight). With a gather point
    /// (`+0xc30` 1) not on the building and no point, it then goes to work on the gather point's
    /// target when the world holds it (its unit AI forgets its target, `FUN_140c5e1e0`;
    /// [`World::gather`]); else, or with a point, it moves there ([`World::gather_move`] after the
    /// same forgetting).
    /// [`LetOut::Stayed`] when no spot is free, before anything changes. With neither a point nor a
    /// gather point its unit AI stops, forgets its target and takes its stance again (slots 0xb0
    /// and 0x1c0). With an object to work on (`work_on`, `GO_BACK_TO_WORK`'s) its work command
    /// (slot 0x128) goes to it at the point first. A unit leaves a container of class 13 as any
    /// other: slot 0x1a8 then runs the container's slot 0x1b0 with −1 and the unit's reload
    /// ([`World::leave_siege`]). Not held: its sound (slot 0x4c0), its garrison sprite. Its work
    /// command (slot 0x128) has `+0xa14` 0: the unit AI keeps its notifies and its queued orders.
    ///
    /// [`Unit::let_go`]: crate::objects::unit::Unit::let_go
    pub(crate) fn let_out(
        &mut self,
        player: PlayerId,
        building: ListIndex,
        id: ObjectId,
        point: Point,
        (work_on, _sound): (Option<ObjectId>, bool),
    ) -> GenieResult<LetOut> {
        let live = self.players.live(player, building)?;
        let bid = live.id;
        let gather = self.parts.gather_point.get(bid).copied();
        // the unit is any player's (an ally's too): its own player's masters and orders
        let (up, ui) = self.live_inside(id)?;
        let made = self
            .players
            .live(up, ui)?
            .master(self.master_tables(), up)?
            .clone();
        // no spot answers 0 before anything changes
        // the search reads its goal from the point itself (`FUN_1411d12d0`)
        let Some(spot) = self.spot_beside(player, building, &made, point)? else {
            return Ok(LetOut::Stayed);
        };
        let z = self.players.live(player, building)?.position().z();
        let at = spot.with_height(z);
        // slot 0x1a8 (`FUN_141213120`) does its part only for a unit inside an object (`+0x2f0`):
        // a new unit an effect could not take in (`FUN_1416136f0`) comes out from where it stands
        let inside = self.players.live(up, ui)?.life.inside.is_some();
        if !inside {
            return self.let_out_from_the_map(
                up,
                ui,
                made.class,
                at,
                (point, gather, bid, work_on),
            );
        }
        self.leave_container((player, building), (up, ui), &made, at)?;
        self.came_out(up, ui, made.class, (point, gather, bid, work_on))
    }

    /// Slot 0x1a8 (`FUN_141213120`) of the unit at (`up`, `ui`), of master `made`, inside the
    /// object at (`player`, `container`), then its slot 0x58 at `at`. The container's slot 0x1c8
    /// (`FUN_141217910`, a building's `FUN_1411d6ad0`, [`Unit::let_go`]) takes the unit's share off
    /// its volleys, lets it out of its list and a building counts it out of its villagers and
    /// raiders, then out of the relics or livestock it keeps ([`Kept::of_unit`],
    /// [`World::keep_out`]); a container of type 70 or more gives back on its `+0x9a8` what a unit
    /// of type 70 or 80 holds ([`World::nest_out`]); a building's sight lets the circle of 5 go
    /// (slot 0x1d8 with 5); the unit's countdown to its raid (`+0x9a0`) ends. Slot 0x58 puts the
    /// unit on the map at `at` (`new_position`: its tile's list, its obstruction record, the mesh,
    /// its sight); the units inside it stay inside it where they are (`FUN_1411a58b0` moves no
    /// object it holds). Its sprites (`FUN_14118a8f0`) are the interface's.
    ///
    /// # Errors
    ///
    /// A unit without its record, an unload without the masters, the relics' withdrawal's, and
    /// the sight's unsupported cases.
    ///
    /// [`Unit::let_go`]: crate::objects::unit::Unit::let_go
    pub(crate) fn leave_container(
        &mut self,
        (player, container): (PlayerId, ListIndex),
        (up, ui): (PlayerId, ListIndex),
        made: &Master,
        at: Position,
    ) -> GenieResult<()> {
        let live = self.players.live(player, container)?;
        let (bid, id) = (live.id, self.players.live(up, ui)?.id);
        let bm = live.master(self.master_tables(), player)?.clone();
        let share = self.unloaded_share((player, container), (up, ui), made, &bm)?;
        let countdown = self.parts.combat_of(id).raider_countdown;
        let counts = GarrisonCounts::of_unit(made, countdown, &self.model.roles);
        let unit = self.players.live_mut(player, container)?;
        unit.let_go(
            &mut self.parts.combat,
            &mut self.parts.building,
            id,
            share,
            counts,
            &bm,
        )?;
        let (building_at, building_radius) = (unit.position(), bm.radius);
        // the building's slot 0x1c8 ends with the relics and the livestock it keeps
        if bm.kind == MasterKind::BUILDING
            && let Some(kept) = Kept::of_unit(
                self.players.player(player)?.culture,
                made,
                &self.model.roles,
            )
        {
            self.keep_out(bid, player, kept)?;
        }
        self.nest_out(id, made.kind, bid, bm.kind)?;
        // a building's sight lets the circle of 5 go (slot 0x1d8 with 5)
        if bm.kind == MasterKind::BUILDING
            && let Some(visibility) = self.spatial.sight.as_mut()
        {
            visibility.sight_off_los(
                bid,
                LosSight::tile_circle(building_at, building_radius, INSIDE_SIGHT),
                &self.model.roles,
                self.model.tuning.visibility,
            )?;
        }
        self.leave_siege(bid, id, made, &bm)?;
        // slot 0x58: on the map at the spot (new_position: its tile's list, its sight)
        let mut placed = self.spatial.come_out(id).ok_or_else(|| {
            GenieError::unsupported("a unit inside a building without its record")
                .with("unit", id)
                .with("building", bid)
        })?;
        let solid = 0.0 < made.radius.x() && 0.0 < made.radius.y() && made.is_tall();
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
        if let Some(mover) = self.parts.mover_mut(unit.id) {
            mover.mark_move(at, world_time);
        }
        let state = unit.life.state;
        if let Some(visibility) = self.spatial.sight.as_mut()
            && state == ObjectState::Alive
        {
            let _ = visibility.sight_on(
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
        if let Some(error) = self.take_sight_unsupported() {
            return Err(error);
        }
        Ok(())
    }

    /// Slot 0x1a8's end (`FUN_141213120`) for unit `id` of master `made` leaving the container
    /// `bid` of master `bm`, a siege unit (class 13) but masters 1704 and 1706: the siege unit no
    /// longer counts the infantry of a class that speeds it up ([`speeds_up_siege`]; slot 0x1b0
    /// with −1, [`count_infantry`]: `+0xad8` and its speed powerup `+0xa2c`), and a unit of a range
    /// above 1 (slot 0x230, `+0x828`, above `garrison::MELEE_REACH`, `COMISS`: a NaN is not) starts
    /// its attack timer `+0x91c` at its full reload time (`FUN_141087040`, [`full_reload`] of its
    /// master's reload, stinger and powerup).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where a combat table lost the layer it took).
    pub(super) fn leave_siege(
        &mut self,
        bid: ObjectId,
        id: ObjectId,
        made: &Master,
        bm: &Master,
    ) -> GenieResult<()> {
        let roles = &self.model.roles;
        if !roles.of_class(bm.class).contains(ClassRoles::SIEGE)
            || roles.of_master(bm.id).contains(MasterRoles::UNSPED_SIEGE)
        {
            return Ok(());
        }
        if speeds_up_siege(made, bm, roles) {
            let tuning = self.model.tuning.powerup;
            count_infantry(self.parts.combat_mut(bid)?, InfantryChange::Left, tuning);
        }
        if MELEE_REACH < made.combat_part().weapon_range.get() {
            let combat = self.parts.combat_mut(id)?;
            combat.attack_timer = full_reload(
                made.combat_part().reload,
                combat.reload_stinger,
                combat.reload_powerup,
            );
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
    ) -> GenieResult<Option<Firepower>> {
        if made.kind != MasterKind::COMBAT {
            return Ok(None);
        }
        let (Some(own), Some(theirs)) = (self.model.masters.of(player), self.model.masters.of(up))
        else {
            return Err(GenieError::unsupported("an unload without the masters"));
        };
        let live = self.players.live(player, container)?;
        let unit = self.players.live(up, ui)?;
        let rate = attack_rate(
            bm,
            own,
            self.parts.combat_of(live.id).reload_powerup,
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
    ) -> GenieResult<LetOut> {
        // an object to work on (GO_BACK_TO_WORK's): slot 0x128 with 0 on it at the point (its
        // unit AI keeps its order queue and notifies)
        if let Some(target) = work_on {
            let id = self.players.live(up, ui)?.id;
            let work = Work::on(Some(target), point.with_height(0.0));
            self.give_work(id, &work, OrderSource::Queued)?;
            return Ok(LetOut::Out);
        }
        // the gather point (`+0xc30` 1, not on the building) gives a unit without a point its
        // target, when the world holds it, else its point
        let to = match let_out_aim(gather, point, bid) {
            LetOutAim::Target(gather_point) => {
                if self.object(gather_point.target).is_some() {
                    if let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(up, ui)?.id) {
                        ai.remove_target();
                    }
                    // slot 0x2a0 with 1; on a building (type 80) the master's task finder
                    // (`+0x788`'s slot 0x10, filter 3) and on a find slot 0x7e8 with filter 3;
                    // else slot 0x128 with `+0xa14` 0: the unit AI keeps its notifies
                    self.gather(up, ui, gather_point, bid, OrderSource::Queued)?;
                    return Ok(LetOut::Out);
                }
                gather_point.point.ground()
            }
            LetOutAim::Point(point) => point,
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
                    ai.stop_with(StopOrder::Keep, place, side, rng, &roles)?;
                    ai.remove_target();
                    let stance = ai.stance;
                    set_stance(ai, stance, place, side, rng, &roles)
                })?;
            }
            return Ok(LetOut::Out);
        }
        // a move to the point: the unit AI forgets its target (`FUN_140c5e1e0`), then slot 0x2a0
        // and slot 0x128 with no target, `+0xa14` 0
        if let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(up, ui)?.id) {
            ai.remove_target();
        }
        self.gather_move(up, ui, to.with_height(0.0), OrderSource::Queued)?;
        Ok(LetOut::Out)
    }

    /// [`World::let_out`] for a unit that is not inside an object (a new unit an effect could not
    /// take in, `FUN_1416136f0`), of master class `class`: `FUN_14119d610` writes its x, y and z
    /// at the spot `at`, slot 0x1a8 does nothing, and slot 0x58 moves it there (`new_position`,
    /// which reads the spot for every read of its place: [`Static::new_position_written`]; its
    /// tile's list, its tall counts, the mesh, its sight; its registry records stay), then
    /// [`World::came_out`].
    ///
    /// # Errors
    ///
    /// The sight's unsupported cases, and what [`World::came_out`] does not model.
    ///
    /// [`Static::new_position_written`]: crate::objects::layers::Static::new_position_written
    pub(super) fn let_out_from_the_map(
        &mut self,
        up: PlayerId,
        ui: ListIndex,
        class: ClassId,
        at: Position,
        rest: (Point, Option<GatherPoint>, ObjectId, Option<ObjectId>),
    ) -> GenieResult<LetOut> {
        let world_time = self.world_time;
        let unit = self.players.live_mut(up, ui)?;
        let id = unit.id;
        unit.place.new_position_written(
            id,
            at,
            &mut self.spatial,
            &mut self.pathing,
            SightLayer {
                roles: &self.model.roles,
                visibility_tuning: self.model.tuning.visibility,
            },
        )?;
        // the position setter's end: a place off where it last moved to is a move
        if let Some(mover) = self.parts.mover_mut(id) {
            mover.mark_move(at, world_time);
        }
        self.came_out(up, ui, class, rest)
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
/// with `gather`: a gather point of state 1 ([`GatherPointState::Point`]) gives a unit without a
/// point its target, one of state 0xff ([`GatherPointState::Kept`]) its x and y whatever point
/// was given, else the point.
pub(super) fn let_out_aim(
    gather: Option<GatherPoint>,
    point: Point,
    building: ObjectId,
) -> LetOutAim {
    match gather {
        Some(gather)
            if point.x() == NO_POINT
                && gather.state == GatherPointState::Point
                && gather.target != Some(building) =>
        {
            LetOutAim::Target(gather)
        }
        Some(gather_point) if gather_point.state == GatherPointState::Kept => {
            LetOutAim::Point(gather_point.point.ground())
        }
        _ => LetOutAim::Point(point),
    }
}
