//! The Transport action's slots ([`TransportAction`]): its start, state, move, stop, work and
//! update, its walk and its test beside the shore.

use crate::GenieError;
use crate::garrison::transport::TransportMode;
use crate::garrison::transport::TransportState;
use crate::garrison::transport::{
    Changed, MOVE_RANGE, MapShore, NEAR, NO_TRANSPORT, NONE, SHORE_MARGIN, Ship, Shore,
    TransportAction, TransportReads, UNLOAD_REACH_SQUARED, Updated,
};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::TerrainFlags;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::math::rng::{Stream, Well512};
use crate::movement::Me;
use crate::movement::PathAsk;
use crate::movement::move_to::{ListUpdate, update_sub_actions};
use crate::movement::move_to::{MoveTo, MoveToReads, SpriteReads};
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
use crate::objects::masters::{Master, TaskId};
use crate::objects::sprite::{Graphic, new_sprite};
use crate::objects::unit::Unit;
use crate::unit_ai::{ActionKind, UnitAi};
use crate::world::World;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl TransportAction {
    /// The unload command's constructor (`FUN_140aa3ab0` with no task, `+0x48`): its point the
    /// unit's place `at`, state 0 and `+0x34` −1 (the base's, `FUN_140a73e90`), `+0x68` clear.
    #[must_use]
    pub const fn new(at: Position) -> Self {
        Self::on_task(None, at)
    }

    /// The constructor (`FUN_140aa3ab0`) on `task` (`+0x48`), the action list's factory's for a
    /// task of type 12 at the work command's point `at` (`+0x38`..`+0x40`): state 0 and `+0x34`
    /// −1 (the base's, `FUN_140a73e90`), `+0x68` clear.
    #[must_use]
    pub const fn on_task(task: Option<TaskId>, at: Position) -> Self {
        Self {
            state: TransportState::New,
            mode: TransportMode::NEW,
            point: at,
            move_to: None,
            retarget: false,
            task,
        }
    }

    /// Slot 5 (`FUN_140aa3e10`), the start: `first` (the action list's add passes 1) with a
    /// point not below 0 on either axis (`COMISS`/`JA`, `JBE`: a NaN is not) sails there
    /// (`set_state(11)`); else it idles (`set_state(2)`).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn start(
        &mut self,
        first: bool,
        ship: &Ship<'_>,
        shore: &mut impl Shore,
    ) -> Result<Changed, GenieError> {
        let (x, y) = (self.point.x(), self.point.y());
        let sails = first
            && crate::math::fmath::at_most_or_unordered(0.0, x)
            && crate::math::fmath::at_most_or_unordered(0.0, y);
        self.set_state(
            if sails {
                TransportState::Sail
            } else {
                TransportState::Idle
            },
            ship,
            shore,
        )
    }

    /// `set_state` (slot 16, `FUN_140aa42a0`): its sub-actions go first (`FUN_140a98620`: a
    /// `MoveTo`'s teardown stops the unit, [`Changed::stopped`]), then the state. State 2 shows
    /// the master's standing sprite ([`Changed::stands`]). States 4 and 11 sail: the point
    /// becomes the shore point on the way from the unit's place ([`Shore::shore_point`]) when
    /// there is one; then a quiet `MoveTo` to the point within [`MOVE_RANGE`] (`FUN_140aa0d30`
    /// with no task: the master's `+0x748` sprite; `FUN_140a7a170` with 1) joins the list
    /// (`FUN_140a977e0`, for a unit in a state below 3) and starts (`0x140aa2100`: a point is
    /// never within its start's gap, so state 3, or 13 for a unit without speed, which a quiet
    /// `MoveTo` shows no sprite for).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn set_state(
        &mut self,
        state: TransportState,
        ship: &Ship<'_>,
        shore: &mut impl Shore,
    ) -> Result<Changed, GenieError> {
        let mut changed = Changed {
            stopped: self.move_to.take().is_some(),
            stands: false,
        };
        self.state = state;
        match state {
            TransportState::Idle => changed.stands = true,
            TransportState::SailToUnload | TransportState::Sail => {
                if let Some(shore_at) =
                    shore.shore_point(ship.position.ground(), self.point.ground())?
                {
                    self.point = self.point.with_x(shore_at.x()).with_y(shore_at.y());
                }
                self.move_to =
                    ship.walk
                        .to_point(self.point, Distance::new(MOVE_RANGE), ship.given)?;
            }
            _ => {}
        }
        Ok(changed)
    }

    /// Slot 8 (`FUN_140aa3e40`), a move to `point` (x, y and the height; the object it names
    /// is not read): the point it sails to, `+0x34` 0, then `set_state(11)`; it takes the move
    /// (1). Its sprite field (`+0x58`, the master's `+0x748` or standing sprite) is the
    /// interface's.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn move_to(
        &mut self,
        point: Position,
        ship: &Ship<'_>,
        shore: &mut impl Shore,
    ) -> Result<Changed, GenieError> {
        self.point = point;
        self.mode = TransportMode::MOVE;
        self.set_state(TransportState::Sail, ship, shore)
    }

    /// Slot 7 (`FUN_140aa4620`), the stop: `set_state(2)`.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn stop(&mut self, ship: &Ship<'_>, shore: &mut impl Shore) -> Result<Changed, GenieError> {
        self.set_state(TransportState::Idle, ship, shore)
    }

    /// Slot 9 (`FUN_140aa4830`), the work order on `point` (x, y and the height), `by_self` when
    /// the object it names is the unit itself (the unload command's); `+0x34` becomes 1. Near
    /// enough to unload there — a tile of the zone of the point's tile (`CVTTSS2SI`,
    /// [`Shore::zone`]) within [`NEAR`] of the unit ([`Shore::near`], exactly 1), and the point
    /// within [`UNLOAD_REACH_SQUARED`] of the unit's footprint on the ground (each axis' gap past
    /// the master's radius, `MAXSS` with 0: a NaN gap stays; `COMISS`/`JC`: a NaN is not
    /// within) — it takes the point and unloads (`set_state(6)`). Else a point below 0 on either
    /// axis (`COMISS`/`JC`: a NaN too) changes nothing; any other becomes the point, and the
    /// unit sails there to unload (`set_state(4)`) from the unit itself, else just sails (11).
    ///
    /// # Errors
    ///
    /// What the map reads give.
    pub fn work(
        &mut self,
        point: Position,
        by_self: bool,
        ship: &Ship<'_>,
        shore: &mut impl Shore,
    ) -> Result<Changed, GenieError> {
        self.mode = TransportMode::WORK;
        let (x, y) = (point.x(), point.y());
        let zone = shore.zone(point.tile())?;
        let (ux, uy) = (ship.position.x(), ship.position.y());
        // MAXSS 0, gap: the gap when it is not below 0, a NaN gap too
        let dx = crate::math::fmath::maxss(0.0, (x - ux).abs() - ship.radius.x());
        let dy = crate::math::fmath::maxss(0.0, (y - uy).abs() - ship.radius.y());
        let near = shore.near(zone, Distance::new(NEAR))?;
        let within =
            !crate::math::fmath::below_or_unordered(UNLOAD_REACH_SQUARED, dy * dy + dx * dx);
        let state = if near && within {
            TransportState::Unload
        } else if crate::math::fmath::below_or_unordered(x, 0.0)
            || crate::math::fmath::below_or_unordered(y, 0.0)
        {
            return Ok(Changed::default());
        } else if by_self {
            TransportState::SailToUnload
        } else {
            TransportState::Sail
        };
        self.point = point;
        self.set_state(state, ship, shore)
    }

    /// One update (slot 6, `FUN_140aa4640`) of `unit`'s Transport on the step, the action copied
    /// out of its list: its `MoveTo` first ([`TransportAction::walk`]), then its state with the
    /// map about the unit where the walk left it ([`TransportAction::state_update`]). Returns
    /// the update's code, whose 1 or 3 deletes it. State 6's unload runs where the update
    /// reaches it ([`TransportAction::unload`]).
    ///
    /// # Errors
    ///
    /// What the map reads, the `MoveTo` and the state's effects give.
    pub(crate) fn update(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        r: &TransportReads,
    ) -> Result<UpdateCode, GenieError> {
        let walked = self.walk(c, unit, me)?;
        let computer = c.w.players.player(c.player)?.computer;
        let World {
            spatial,
            zones,
            pathing,
            streams,
            graphics,
            ..
        } = &mut *c.w;
        let sprites = SpriteReads {
            defaults: r.defaults,
            graphics,
        };
        let walk = MoveToReads::with_sprites(sprites, unit);
        let ship = Ship::of(unit, r.radius, spatial.map, walk, r.given);
        let mut shore = MapShore::new(spatial, zones, pathing, r.restriction, ship.position)?;
        let look = (r.standing, streams.get(Stream::Sprite));
        match self.state_update(walked, (&ship, &mut shore), unit, look, computer)? {
            Updated::Code(code) => Ok(code),
            Updated::Unload => {
                self.unload(c, unit)?;
                Ok(UpdateCode::UNDER_WAY)
            }
        }
    }

    /// State 6's unload (`FUN_140aa3ba0`) in the update: [`World::transport_unload`] on the
    /// unit lent back to its slot ([`Step::lend_at`]) with this copy of its Transport, which then
    /// is the one the unload left.
    ///
    /// # Errors
    ///
    /// The unload's and the lend's, or a unit whose Transport the unload took.
    fn unload(&mut self, c: &mut Step<'_>, unit: &mut Unit) -> Result<(), GenieError> {
        if let Some(own) = unit.actions.transport_mut() {
            *own = *self;
        }
        c.lend_at(unit, World::transport_unload)?;
        *self = *unit.actions.transport().ok_or(NO_TRANSPORT)?;
        Ok(())
    }

    /// The update's state after its `MoveTo`'s (`walked`, the code of [`TransportAction::walk`],
    /// which only states 4 and 11 read), its effects on `unit` at once: a state change stops it
    /// and state 2 shows the standing sprite (`look`: the master's and the Sprite stream,
    /// [`stand`]). State 1 is done (1). States 4 and 11: any end of the `MoveTo` (codes 1 to 5;
    /// without one the list answers 1) unloads (4: `set_state(6)`) or idles (11:
    /// `set_state(2)`). State 13 tells the unit's AI it failed (0x1f9 with the Transport's action
    /// value, slot 0x328), idles and returns 3. State 6 unloads:
    /// - with a point, its zone's tile ([`Shore::zone`]) must lie within [`NEAR`] of the unit
    ///   ([`Shore::near`]), else the owner hears that it cannot unload there (0x207, its slot
    ///   0x2e8: [`crate::unit_ai::alert_owner`] for a `computer` player); `+0x68` clears either
    ///   way.
    /// - with units inside, a unit beside the shore ([`TransportAction::next_to_shore`]) unloads
    ///   ([`Updated::Unload`]); one that is not, over water at the point (`FUN_1413fbf30`,
    ///   `FUN_1413fc2b0`: the terrain table's bits 0 to 2 at the point's tile) waits there, else
    ///   the owner hears that it cannot unload.
    /// - unless the unload sails on (`+0x68`), the action idles (`set_state(2)`).
    ///
    /// # Errors
    ///
    /// What the map reads give, the notify's, a computer player's alert and a standing sprite
    /// the unit lacks.
    pub(crate) fn state_update(
        &mut self,
        walked: UpdateCode,
        (ship, shore): (&Ship<'_>, &mut impl Shore),
        unit: &mut Unit,
        look: (Option<Graphic>, &mut Well512),
        computer: bool,
    ) -> Result<Updated, GenieError> {
        let (changed, code) = match self.state {
            TransportState::Done => return Ok(Updated::Code(UpdateCode::DONE)),
            TransportState::SailToUnload | TransportState::Sail
                if (1..=5).contains(&walked.get()) =>
            {
                let next = if self.state == TransportState::Sail {
                    TransportState::Idle
                } else {
                    TransportState::Unload
                };
                (self.set_state(next, ship, shore)?, UpdateCode::UNDER_WAY)
            }
            // Failed: the update returns 3.
            TransportState::Failed => {
                let changed = self.set_state(TransportState::Idle, ship, shore)?;
                stand(unit, changed, look)?;
                unit.post_own(
                    crate::unit_ai::NotifyKind::ACTION_FAILED,
                    [ActionKind::Transport.value(), 0, 0],
                )?;
                return Ok(Updated::Code(UpdateCode::FAILED));
            }
            TransportState::Unload => {
                let (x, y) = (self.point.x(), self.point.y());
                let mut reaches = true;
                if x == NONE {
                    self.retarget = false;
                } else {
                    let zone = shore.zone(self.point.tile())?;
                    self.retarget = false;
                    reaches = shore.near(zone, Distance::new(NEAR))?;
                }
                let mut cannot_unload = !reaches;
                if reaches && ship.loaded {
                    if self.next_to_shore(ship, shore)? {
                        return Ok(Updated::Unload);
                    }
                    let over_water = x != NONE
                        && shore
                            .flags(Point::new(x, y).tile().short())
                            .intersects(TerrainFlags::ANY_WATER);
                    cannot_unload = !over_water;
                }
                let changed = self.unloaded(ship, shore)?;
                stand(unit, changed, look)?;
                if cannot_unload {
                    crate::unit_ai::alert_owner(computer)?;
                }
                return Ok(Updated::Code(UpdateCode::UNDER_WAY));
            }
            _ => (Changed::default(), UpdateCode::UNDER_WAY),
        };
        stand(unit, changed, look)?;
        Ok(Updated::Code(code))
    }

    /// The end of state 6's update after the unload ([`Updated::Unload`]): unless the
    /// unload sails on (`+0x68`), `set_state(2)`.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn unloaded(
        &mut self,
        ship: &Ship<'_>,
        shore: &mut impl Shore,
    ) -> Result<Changed, GenieError> {
        if self.retarget {
            return Ok(Changed::default());
        }
        self.set_state(TransportState::Idle, ship, shore)
    }

    /// The sprite the `set_state` (`0x140aa42a0`, states 4 and 11) of a Transport on `task` gives
    /// its `MoveTo` (`FUN_140aa0d30`): the task's moving sprite (`+0x40`), without a task the
    /// master's walking sprite (`+0x748`); for none the constructor's default
    /// ([`MoveToReads::to_point`]).
    ///
    /// # Errors
    ///
    /// A task the unit's master (`master`) lacks.
    pub fn given_sprite(
        task: Option<TaskId>,
        master: &Master,
    ) -> Result<Option<SpriteId>, GenieError> {
        Ok(match task {
            Some(id) => {
                let task = master.tasks.iter().find(|t| t.id == id).ok_or_else(|| {
                    GenieError::missing_data("a Transport's task its master lacks")
                        .with("task", id.get())
                })?;
                task.sprites.moving
            }
            None => master.move_sprite,
        })
    }

    /// The update of its `MoveTo` sub-action (`FUN_140a992b0`) on the step
    /// ([`MoveToAction::sub_update`]): state 3 asks for its path by what the unit's AI asks of
    /// the path (`0x140aa2a70`, [`UnitAi::path_ask`]) and shows its sprite; state 4 moves; after
    /// either the units in the way go to the unit AI at once (notify 0x215). A motion step that
    /// gives up fails it (`set_state(13)`: only a human player's villager waits). Its code: 1
    /// once it ended at the point, 3 once it failed, 1 without one ("There is no list"), else 0.
    ///
    /// # Errors
    ///
    /// What the motion step does not model, and the notify's.
    pub(crate) fn walk(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
    ) -> Result<UpdateCode, GenieError> {
        let Some(state) = self.move_to.as_ref().map(|m| m.state) else {
            return Ok(UpdateCode::DONE);
        };
        let start = state == MoveTo::Start;
        let walks = start || state == MoveTo::Moving;
        let ask = if start {
            UnitAi::path_ask(
                unit.combat.ai.as_mut(),
                unit.body.position().ground(),
                self.point.ground(),
            )
        } else {
            PathAsk::default()
        };
        let how = ListUpdate {
            ask,
            ..ListUpdate::default()
        };
        let code = update_sub_actions(&mut self.move_to, c, unit, me, how)?;
        if walks {
            unit.post_in_the_way()?;
        }
        Ok(code)
    }

    /// `FUN_140aa3f40` (HD's `next_to_drop_off_terrain`), without a task (`+0x48`; a task whose
    /// `+0x16` is −1 would answer yes): a tile about the unit's footprint grown by
    /// [`SHORE_MARGIN`] (each edge cut by `CVTTSS2SI`, kept on the map) of the point's zone
    /// ([`Shore::zone`]), or without a point (x [`NONE`]) a land tile (the terrain table's bits 3
    /// and 4, `FUN_1413fbee0` and `FUN_1413fc270`); row by row, each from its least x.
    ///
    /// # Errors
    ///
    /// What the map reads give.
    pub fn next_to_shore(
        &self,
        ship: &Ship<'_>,
        shore: &mut impl Shore,
    ) -> Result<bool, GenieError> {
        let (ux, uy) = (ship.position.x(), ship.position.y());
        let (rx, ry) = (ship.radius.x(), ship.radius.y());
        let (w, h) = (ship.map.width(), ship.map.height());
        let x0 = cvttss2si((ux - rx) - SHORE_MARGIN).max(0);
        let y0 = cvttss2si((uy - ry) - SHORE_MARGIN).max(0);
        let x1 = cvttss2si((ux + rx) + SHORE_MARGIN).min(w - 1);
        let y1 = cvttss2si((uy + ry) + SHORE_MARGIN).min(h - 1);
        let (x, _y) = (self.point.x(), self.point.y());
        let zone = if x == NONE {
            None
        } else {
            Some(shore.zone(self.point.tile())?)
        };
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let found = match zone {
                    None => shore
                        .flags(Tile::new(tx, ty).short())
                        .intersects(TerrainFlags::SHORE),
                    Some(z) => shore.zone(Tile::new(tx, ty))? == z,
                };
                if found {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

/// A state change's effects on the unit at once ([`Changed`]): its `MoveTo`'s teardown stops it
/// (slot 0x5d0), then state 2 shows its master's standing sprite (slot 0x68 with `+0x470`,
/// [`Unit::show_standing`]: the top speed is the standing sprite's and, where the slice holds
/// its sprite, the sprite starts on the Sprite stream; `look`).
///
/// # Errors
///
/// The stop's, and a unit whose sprite the slice holds without its standing sprite.
pub(crate) fn stand(
    unit: &mut Unit,
    changed: Changed,
    (standing, rng): (Option<Graphic>, &mut Well512),
) -> Result<(), GenieError> {
    if changed.stopped
        && let Some(m) = unit.body.mover_mut()
    {
        m.stop()?;
    }
    if !changed.stands {
        return Ok(());
    }
    if let Some(m) = unit.body.mover_mut() {
        m.shows(standing.as_ref());
    }
    if unit.sprite.is_some() {
        let standing = standing.ok_or(GenieError::unsupported(
            "a stopped unit without its standing sprite",
        ))?;
        new_sprite(&mut unit.sprite, unit.body.move_stats_mut(), standing, rng);
    }
    Ok(())
}

impl<'a> Ship<'a> {
    /// What the Transport on `task` reads of `unit` of `master` on a map of `map` tiles, with the
    /// sprites' table `graphics` ([`Ship::of`]).
    ///
    /// # Errors
    ///
    /// A task the master lacks.
    pub(crate) fn of_unit(
        unit: &Unit,
        master: &Master,
        map: GridSize,
        graphics: &'a crate::objects::Graphics,
        task: Option<TaskId>,
    ) -> Result<Self, GenieError> {
        let walk = MoveToReads::of(unit, master, graphics);
        let given = TransportAction::given_sprite(task, master)?;
        Ok(Self::of(unit, master.radius, map, walk, given))
    }

    /// What the Transport reads of `unit` of master radii `radius` on a map of `map` tiles: its
    /// place and whether an object is inside it; a `MoveTo` it makes reads `walk` and takes the
    /// sprite `given`.
    pub(crate) const fn of(
        unit: &Unit,
        radius: Footprint,
        map: GridSize,
        walk: MoveToReads<'a>,
        given: Option<SpriteId>,
    ) -> Self {
        Self {
            position: unit.position(),
            radius,
            loaded: !unit.garrison.is_empty(),
            map,
            walk,
            given,
        }
    }
}
