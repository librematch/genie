//! The Explore action (`RGE_Action_Explore`, type 4, made by `FUN_140a87d30`) and the auto scout's
//! search for a point to explore (`FUN_140c65490`, the unit AI's "unexploredArea").
//!
//! 158041: first (slot 0x28) `0x140a87e10`, update (slot 0x30) `0x140a880c0`, the point (slot
//! 0x48) `0x140a881a0`, `set_state` (slot 0x80) `0x140a87e30`. An Explore without a point waits in
//! state 3 for its unit AI to pick one (notify 0x202 with action 605, which the AI handles on its
//! player's turn); with a point it walks there in state 11 with a quiet `MoveTo`, and when the `MoveTo`
//! is done it goes back to state 3.

use crate::GenieError;
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::{Tile, TileOffset};
use crate::math::rng::Stream;
use crate::movement::Me;
use crate::movement::Mover;
use crate::movement::move_to::{GiveUp, MoveDefaults, MoveToReads, Patience, SpriteReads};
use crate::movement::move_to::{ListUpdate, update_sub_actions};
use crate::movement::scout_area::ScoutArea;
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::PlayersMasters;
use crate::objects::unit::{Unit, object_master};
use crate::players::PlayerKind;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitStance;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use genie_diff::Diff;

/// The Explore action's type.
pub const EXPLORE: ActionKind = ActionKind::named(4);

use crate::combat::abilities::Look;
#[cfg(test)]
use crate::map::Footprint;
use crate::math::image::explore::NO_POINT as EXPLORE_NO_POINT;
use crate::math::rng::Well512;
#[cfg(test)]
use crate::movement::Speed;
pub use crate::movement::explore_state::ExploreState;
use crate::movement::stop_mover;
use crate::objects::Graphics;
#[cfg(test)]
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::sprite::new_sprite;
use crate::unit_ai::NotifyKind;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::MoveToTuning;

/// State 11 made without what its `MoveTo` reads of the unit.
const WALK_WITHOUT_READS: GenieError =
    GenieError::unreachable("an Explore's set_state(11) without the unit's reads");

/// A point of −1 is no point (`DAT_14330ec20`).
const NO_POINT: f32 = EXPLORE_NO_POINT;

/// What an Explore's `set_state` asks of its owner besides its own fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    /// Nothing.
    Nothing,
    /// State 3: notify 0x202 (605) to its unit AI (slot 0x328 with its own id twice), then the
    /// standing sprite (slot 0x68 with the master's `+0x470`).
    WantsPoint,
    /// States 1, 13 and 14: the standing sprite.
    Standing,
    /// State 2: the owner's slot 0x68 and the action's own, its target none: not modelled.
    Stopped,
}

/// An Explore action.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct ExploreAction {
    /// `+0x18`, with state 11's `MoveTo` to the point (quiet, range 0, the master's walking
    /// sprite, [`ExploreMove::given`]).
    pub(crate) state: ExploreState,
    /// The point it walks to (`+0x38`..`+0x40`), x −1 for none.
    pub(crate) point: Position,
}

/// What an Explore's `MoveTo` reads of its unit as `set_state(11)` makes it: [`MoveToReads`], and
/// the sprite its maker gives (`FUN_140aa0d30` takes the task's moving sprite, an Explore has no
/// task: the master's walking sprite, `+0x748`; none leaves the constructor's default).
#[derive(Clone, Copy, Debug)]
pub struct ExploreMove<'a> {
    /// What every new sub-action `MoveTo` reads of the unit.
    pub(crate) reads: MoveToReads<'a>,
    /// The master's walking sprite (`+0x748`).
    pub(crate) given: Option<SpriteId>,
}

impl<'a> ExploreMove<'a> {
    /// What the `MoveTo` of `unit` with its mover of `master` reads, with the sprites' table
    /// and the unit's terrain look (`graphics`).
    #[must_use]
    pub fn of(
        unit: (&Unit, Option<&Mover>),
        master: &Master,
        graphics: (&'a Graphics, Option<&Look>),
        roles: &Roles,
    ) -> Self {
        Self {
            reads: MoveToReads::of(unit, master, graphics, roles),
            given: master.moving_part().move_sprite,
        }
    }
}

impl ExploreAction {
    /// `FUN_140a87d30`, then its first (slot 0x28, `0x140a87e10`) as the action list adds it:
    /// state 3 without a point, else 11, whose `MoveTo` is made from `walk`
    /// ([`ExploreAction::set_state`]).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn new(
        point: Position,
        walk: &ExploreMove<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<(Self, Owner), GenieError> {
        let mut a = Self {
            state: ExploreState::New,
            point,
        };
        let state = if a.point.x() == NO_POINT {
            ExploreState::Wait
        } else {
            ExploreState::Walk { move_to: None }
        };
        let owner = a.set_state(state, Some(walk), move_to_tuning)?;
        Ok((a, owner))
    }

    /// `Set_state` (slot 0x80, `0x140a87e30`): its sub-actions go (`FUN_140a98620`), then by state:
    /// 3 asks its unit AI for a point, 11 makes the `MoveTo` to the point from `walk`
    /// ([`MoveToReads::to_point`]: `FUN_140aa0d30` with range 0, made quiet by `FUN_140a7a170`;
    /// the action list starts it, `FUN_140a977e0`). The other states read nothing of the unit.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks, and state 11 without `walk`.
    pub fn set_state(
        &mut self,
        state: ExploreState,
        walk: Option<&ExploreMove<'_>>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<Owner, GenieError> {
        // the sub-actions go: a new state holds none until it makes its own
        self.state = match state {
            ExploreState::Walk { .. } => ExploreState::Walk { move_to: None },
            other => other,
        };
        Ok(match state {
            ExploreState::Stopped => {
                self.point = Position::from([-1.0; 3]);
                Owner::Stopped
            }
            ExploreState::Wait => Owner::WantsPoint,
            ExploreState::Done | ExploreState::Failed | ExploreState::Ended => Owner::Standing,
            ExploreState::Walk { .. } => {
                let walk = walk.ok_or(WALK_WITHOUT_READS)?;
                self.state = ExploreState::Walk {
                    move_to: walk.reads.to_point(
                        self.point,
                        Distance::ZERO,
                        walk.given,
                        move_to_tuning,
                    )?,
                };
                Owner::Nothing
            }
            ExploreState::New => Owner::Nothing,
        })
    }

    /// The point (slot 0x48, `0x140a881a0`): one on the map's quarter (both coordinates at least
    /// 0) becomes its point and it walks there from `walk` (`set_state` 11); any other makes it
    /// wait for one (`set_state` 3).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn take_point(
        &mut self,
        point: Position,
        walk: &ExploreMove<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<Owner, GenieError> {
        if 0.0 <= point.x() && 0.0 <= point.y() {
            self.point = point;
            return self.set_state(
                ExploreState::Walk { move_to: None },
                Some(walk),
                move_to_tuning,
            );
        }
        self.set_state(ExploreState::Wait, None, move_to_tuning)
    }

    /// What the update does after its `MoveTo`'s update returned `code` (`0x140a880c0` in state 11,
    /// `FUN_140a992b0`): 0 goes on; 1, 2, 3 and 5 make it wait for a new point (`set_state` 3); 4
    /// runs the action's slot 0x68 and walks again (`set_state` 11), which is not modelled.
    pub fn after_walk(
        &mut self,
        code: UpdateCode,
        move_to_tuning: &MoveToTuning,
    ) -> Result<Owner, GenieError> {
        match code.get() {
            1 | 2 | 3 | 5 => self.set_state(ExploreState::Wait, None, move_to_tuning),
            4 => Err(GenieError::unsupported(
                "an Explore whose MoveTo returned 4 (its slot 0x68)",
            )),
            // 0 (and any other code) goes on
            _ => Ok(Owner::Nothing),
        }
    }
}

impl ExploreAction {
    /// One update (`0x140a880c0`) of the Explore of `explorer` (`me`) through its step: state 11
    /// updates its quiet `MoveTo` to the point (the sprite its start shows is the master's
    /// walking one, [`ExploreMove::given`]) and goes on by the code
    /// ([`ExploreAction::after_walk`]); a `MoveTo` the update dropped stops the unit (its
    /// teardown, slot 0x5d0), and what the new state asks of the unit follows at once
    /// ([`Unit::explore_owner`]). States 1 (done) and 13 (failed) are not modelled, and the
    /// other states wait.
    ///
    /// # Errors
    ///
    /// The states and codes the slice does not model, and the walk's.
    pub(crate) fn update(
        &mut self,
        c: &mut Step<'_>,
        explorer: &mut Unit,
        me: Me,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        // 0x140a880c0: state 1 is done (the action list removes it, the default task's restart
        // is not modelled), 11 walks, 13 fails; the others wait
        match self.state {
            ExploreState::Done => Err(GenieError::unsupported(
                "a done Explore (the action list after it)",
            )),
            ExploreState::Failed => Err(GenieError::unsupported(
                "a failed Explore (notify 0x1f9, set_state 2)",
            )),
            ExploreState::Walk { .. } => self.walk(c, explorer, me, roles),
            _ => Ok(()),
        }
    }

    /// State 11's walk: the `MoveTo`'s update code (`FUN_140a992b0`): 1 once it arrived or
    /// without one, 3 once it failed. The motion step giving up makes a human player's villager
    /// wait once and fails any other unit's `MoveTo` (`LAB_140aa320c`).
    fn walk(
        &mut self,
        c: &mut Step<'_>,
        explorer: &mut Unit,
        me: Me,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let master = object_master(
            c.w.parts.own_master.get(explorer.id),
            (explorer.id, explorer.life.master_id),
            c.w.model.masters.of(c.player),
        )
        .ok();
        let villager =
            master.is_some_and(|m| roles.of_class(m.class).contains(ClassRoles::VILLAGER));
        let gives_up = GiveUp {
            patience: if villager && !c.w.players.player(c.player)?.kind.is_computer() {
                Patience::WaitsOnce
            } else {
                Patience::Fails
            },
            stance: c
                .w
                .parts
                .unit_ai
                .get(explorer.id)
                .map_or(UnitStance::Aggressive, |ai| ai.stance),
        };
        let how = ListUpdate {
            gives_up,
            ..ListUpdate::default()
        };
        let mut move_to = self.state.move_to().copied();
        let code = update_sub_actions(&mut move_to, c, explorer, me, how)?;
        self.state = ExploreState::Walk { move_to };
        let walked = move_to.is_some();
        let owner = self.after_walk(code, &c.w.model.tuning.move_to)?;
        if walked && self.state.move_to().is_none() {
            stop_mover(c.w.parts.mover_mut(explorer.id))?;
        }
        explorer.explore_owner(owner, c)
    }
}

/// The scout map's size, which `FUN_140c589c0` gives the `InfluenceMap` it makes (`0x1e0`).
pub const SCOUT_MAP_SIZE: i32 = 0x1e0;

/// An `InfluenceMap` (`FUN_140f4a240`): a byte per tile from a reference point (`+0x10`,
/// `+0x14`), `size` wide and high (`+0x8`, `+0xc`), row by row (`+0x20`). A player's scout map
/// (`player+0x1370`) is one, 1 marking a tile its auto scouts need not go to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfluenceMap {
    /// Its width and height.
    pub size: GridSize,
    /// The tile its first byte stands for.
    pub(crate) origin: Tile,
    /// `unchangeableLimitValue` (`+0x18`), which the save keeps.
    pub(crate) limit: u8,
    /// The bytes, row by row.
    pub cells: Vec<u8>,
}

impl InfluenceMap {
    /// `FUN_140f4a240`: `size` wide and high, every byte `fill`, from tile (0, 0).
    #[must_use]
    pub fn new(size: GridSize, fill: u8) -> Self {
        let n = (size.width().max(0) * size.height().max(0)) as usize;
        Self {
            size,
            origin: Tile::new(0, 0),
            limit: 0,
            cells: vec![fill; n],
        }
    }

    /// `tile`'s cell, counted from the map's origin; none off the map.
    const fn index(&self, tile: Tile) -> Option<usize> {
        self.size.index(Tile::new(
            tile.x() - self.origin.x(),
            tile.y() - self.origin.y(),
        ))
    }

    /// The byte of `tile`, None off the map (`FUN_140c4b130` reads it unchecked).
    #[must_use]
    pub fn get(&self, tile: Tile) -> Option<u8> {
        self.index(tile).and_then(|i| self.cells.get(i).copied())
    }

    /// `FUN_140f4d5e0`: `tile` takes `value`; one off the map is let be.
    pub fn set(&mut self, tile: Tile, value: u8) {
        if let Some(cell) = self.index(tile).and_then(|i| self.cells.get_mut(i)) {
            *cell = value;
        }
    }
}

/// The search's rings around the town centre: 10, 15 and 20 tiles (`FUN_140c65490`).
const RINGS: [i32; 3] = [10, 15, 20];

/// The eight points of a ring in the order the search tries them (`FUN_140c65490`'s switch):
/// down-left first, then round through down, down-right, right, up-right, up, up-left and left.
const AROUND: [TileOffset; 8] = TileOffset::table([
    [-1, -1],
    [0, -1],
    [1, -1],
    [1, 0],
    [1, 1],
    [0, 1],
    [-1, 1],
    [-1, 0],
]);

/// `FUN_140c65490`: the closest point the unit may explore around its player's first town centre
/// (`FUN_1416597e0`, its tile `tc`), None without a town centre or a point. The points lie 10,
/// 15 and 20 tiles from the centre's tile in eight directions each; a point counts when it is on
/// the map (`map`, the player's visible map's size), the player has not seen its tile, it lies in
/// the unit's zone (their low bytes equal), the unit could stand there and reaches it; the first such point, or a
/// strictly closer one, wins. The player's explored-points grid (`player+0x1370`, which the
/// search would read instead of the explored tiles and mark the point found in) is never made
/// in the slice: the base notify makes it only when no point is found, which it does not model.
pub(crate) fn unexplored_area(
    tc: Option<Tile>,
    map: GridSize,
    area: &mut ScoutArea<'_>,
) -> Result<Option<Tile>, GenieError> {
    let Some(tc) = tc else {
        return Ok(None);
    };
    let (cx, cy) = (tc.x(), tc.y());
    let own = area.own_zone();
    let mut best: Option<(Tile, f32)> = None;
    for ring in RINGS {
        for a in AROUND {
            let tile = Tile::new(cx + a.dx() * ring, cy + a.dy() * ring);
            if !map.holds(tile) || area.explored(tile)? {
                continue;
            }
            // the zones' low bytes only (`CMP byte ptr [RSP+0x48],AL` at 0x140c65812)
            if area.zone(tile).byte() != own.byte() || !area.clear(tile)? || !area.reaches(tile)? {
                continue;
            }
            let d = area.distance(tile);
            if best.is_none_or(|(_, b)| d < b) {
                best = Some((tile, d));
            }
        }
    }
    Ok(best.map(|(p, _)| p))
}

/// How far past its line of sight the search looks, and about an explore's target point, in tiles.
const SEARCH_PAST_SIGHT: i32 = 3;
const SEARCH_ABOUT_POINT: i32 = 7;

/// What `closestUndiscoveredTile` reads of its unit and unit AI.
#[derive(Clone, Copy, Debug)]
pub struct Seeker {
    /// The unit's place.
    pub(crate) at: Position,
    /// Its master's line of sight (`+0x5d0`).
    pub(crate) line_of_sight: Distance,
    /// The unit AI's current order (`+0x3c`).
    pub(crate) order: Option<OrderKind>,
    /// The unit AI's target point (`+0x50`, `+0x54`; x is −1 without one).
    pub(crate) target_point: Point,
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub(crate) player_kind: PlayerKind,
}

/// `FUN_140c4bd40`, the unit AI's slot 0x90 (`closestUndiscoveredTile`, "the closest
/// undiscovered tile"): none for a computer player's unit. Else, after one draw of the AI stream
/// (`ai`), the tiles within the unit's line of sight and 3 more of its tile (within 7 of the target
/// point under an explore to it, order `0x2da`), held on the map (`map`), row by row: a tile the
/// player never saw, in the unit's zone, where the unit could stand (slot 0x278) counts by the
/// squared distance between the tiles (plus that from the target point's, when the AI has one),
/// and takes the place of the best when it is nearer than the best by more than the draw's slack
/// (`d < best + draw % 3`). The best then counts only when the unit reaches it (slot 0x3d8 within
/// its line of sight less a tile). The waypoint queue the search passes by (`+0xd8`, count
/// `+0x158`, not saved) is empty in the slice.
pub(crate) fn closest_undiscovered(
    area: &mut ScoutArea<'_>,
    seeker: Seeker,
    map: GridSize,
    ai: &mut Well512,
) -> Result<Option<Tile>, GenieError> {
    if seeker.player_kind.is_computer() {
        return Ok(None);
    }
    let (ux, uy) = (seeker.at.x(), seeker.at.y());
    let reach = (seeker.line_of_sight.get() as i32 + SEARCH_PAST_SIGHT) as f32;
    let (mut x0, mut x1) = ((ux - reach) as i32, (reach + ux) as i32);
    let (mut y0, mut y1) = ((uy - reach) as i32, (reach + uy) as i32);
    let (px, py) = (seeker.target_point.x(), seeker.target_point.y());
    if seeker.order == Some(OrderKind::Scout) {
        (x0, x1) = (
            px as i32 - SEARCH_ABOUT_POINT,
            px as i32 + SEARCH_ABOUT_POINT,
        );
        (y0, y1) = (
            py as i32 - SEARCH_ABOUT_POINT,
            py as i32 + SEARCH_ABOUT_POINT,
        );
    }
    let (x0, y0) = (x0.max(0), y0.max(0));
    let (x1, y1) = (x1.min(map.width() - 1), y1.min(map.height() - 1));
    let slack = (ai.rand15() % 3) as i32;
    let own = area.own_zone();
    let (tx, ty) = (ux as i32, uy as i32);
    let mut best: Option<(Tile, i32)> = None;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let tile = Tile::new(x, y);
            if area.explored(tile)? || area.zone(tile) != own || !area.clear(tile)? {
                continue;
            }
            let mut d = (y - ty) * (y - ty) + (x - tx) * (x - tx);
            if px != NO_POINT {
                d += (y - py as i32) * (y - py as i32) + (x - px as i32) * (x - px as i32);
            }
            if best.is_none_or(|(_, b)| d < slack + b) {
                best = Some((tile, d));
            }
        }
    }
    match best {
        Some((t, _)) if area.reaches(t)? => Ok(Some(t)),
        _ => Ok(None),
    }
}

/// How far past its line of sight the auto scout's last search draws its square, in tiles.
const SQUARE_PAST_SIGHT: i32 = 2;

/// `FUN_140c4b130`, the auto scout's last search when neither the search around the town centre nor
/// the closest undiscovered tile found a point (the base handler's case 0x202 on the auto scout
/// order): the edge of a square about the unit's tile (`at`), its line of sight and 2 more tiles
/// out, side by side (top from its left corner, right, bottom and left from the next corner on, the
/// tiles off the map left out; `map`), in an order `std::shuffle` draws from the AI stream (`ai`):
/// the first tile the player never saw, in the unit's zone, where it could stand (slot 0x278) and
/// that it reaches (slot 0x3d8 within its line of sight less a tile) wins. Else a flood from the
/// unit's tile through the tiles the player saw (left, right, up and down, on the map, each once;
/// the start tile is not marked and comes again) stops at the first unseen tile it meets that
/// counts as above; unseen tiles that do not count end their branch. None when the flood runs out.
/// With the player's scout map (`player+0x1370`) both read it instead of the seen tiles (a tile
/// scouted is 1; [`ScoutArea::explored`] answers by it) and the point found is marked in it (the
/// caller's).
pub(crate) fn beyond_sight(
    area: &mut ScoutArea<'_>,
    at: Position,
    line_of_sight: Distance,
    map: GridSize,
    ai: &mut Well512,
) -> Result<Option<Tile>, GenieError> {
    let own = area.own_zone();
    let (w, h) = (map.width(), map.height());
    let r = line_of_sight.get() as i32 + SQUARE_PAST_SIGHT;
    let (ux, uy) = (at.x() as i32, at.y() as i32);
    let (x0, x1, y0, y1) = (ux - r, ux + r, uy - r, uy + r);
    let on_x = |x: i32| 0 <= x && x < w;
    let on_y = |y: i32| 0 <= y && y < h;
    let mut edge: Vec<Tile> = Vec::new();
    if 0 <= y0 {
        edge.extend((x0..x1).filter(|&x| on_x(x)).map(|x| Tile::new(x, y0)));
    }
    if x1 < w {
        edge.extend((y0..y1).filter(|&y| on_y(y)).map(|y| Tile::new(x1, y)));
    }
    if y1 < h {
        edge.extend((x0 + 1..=x1).filter(|&x| on_x(x)).map(|x| Tile::new(x, y1)));
    }
    if 0 <= x0 {
        edge.extend((y0 + 1..=y1).filter(|&y| on_y(y)).map(|y| Tile::new(x0, y)));
    }
    ai.shuffle(&mut edge);
    let counts = |area: &mut ScoutArea<'_>, tile: Tile| -> Result<bool, GenieError> {
        Ok(area.zone(tile) == own && area.clear(tile)? && area.reaches(tile)?)
    };
    for &t in &edge {
        if !area.explored(t)? && counts(area, t)? {
            return Ok(Some(t));
        }
    }
    let mut seen = vec![false; (w * h).max(0) as usize];
    let mut queue = vec![Tile::new(ux, uy)];
    let mut i = 0;
    while let Some(&t) = queue.get(i) {
        i += 1;
        if !area.explored(t)? {
            if counts(area, t)? {
                return Ok(Some(t));
            }
            continue;
        }
        for n in t.beside() {
            if let Some(s) = map.index(n).and_then(|c| seen.get_mut(c))
                && !*s
            {
                *s = true;
                queue.push(n);
            }
        }
    }
    Ok(None)
}

impl Unit {
    /// The unit's explore (slot 0x398, `FUN_141084db0` with its argument 4 set) to the point at
    /// its own height: its top Explore takes the point (slot 0x48), else a new Explore
    /// (`FUN_140a87d30`) becomes its only action (`set_only_action`, `FUN_14106e990`: the actions
    /// it had go, `FUN_140a98620`, and the new one starts, `FUN_140a977e0`). A `MoveTo` of the
    /// Explore that goes stops the unit (its teardown, slot 0x5d0); other actions it would
    /// replace are not modelled.
    pub(crate) fn explore_to(
        &mut self,
        actions: &mut Action,
        point: Point,
        c: &mut Step<'_>,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let at = Position::new(point.x(), point.y(), self.position().z());
        let walk = self.explore_move(c, roles);
        let owner = if let Some(e) = actions.list.explore_mut() {
            let walked = e.state.move_to().is_some();
            let owner = e.take_point(at, &walk, &c.w.model.tuning.move_to)?;
            if walked {
                stop_mover(c.w.parts.mover_mut(self.id))?;
            }
            owner
        } else {
            if actions.top_action().is_some() || c.w.parts.unread.contains(self.id) {
                return Err(GenieError::unsupported(
                    "an Explore replacing another action",
                ));
            }
            let (e, owner) = ExploreAction::new(at, &walk, &c.w.model.tuning.move_to)?;
            actions.list.set_explore(Some(e));
            owner
        };
        self.explore_owner(owner, c)
    }

    /// What an Explore's `MoveTo` reads of this unit ([`ExploreMove`]); a unit whose master the world
    /// lacks reads no sprite defaults and gives none.
    fn explore_move<'g>(&self, c: &'g Step<'_>, roles: &Roles) -> ExploreMove<'g> {
        let master = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.model.masters.of(c.player),
        );
        master.map_or_else(
            |_| ExploreMove {
                reads: MoveToReads::with_sprites(
                    SpriteReads {
                        defaults: MoveDefaults::default(),
                        graphics: &c.w.model.graphics,
                    },
                    (self, c.w.parts.mover_of(self.id)),
                ),
                given: None,
            },
            |master| {
                ExploreMove::of(
                    (self, c.w.parts.mover_of(self.id)),
                    master,
                    (
                        &c.w.model.graphics,
                        c.w.parts.combat_of(self.id).abilities.look.as_ref(),
                    ),
                    roles,
                )
            },
        )
    }

    /// What an Explore's `set_state` asks of its unit ([`crate::movement::explore::Owner`]): its notify 0x202
    /// with the explore's action value to the unit AI, and the standing sprite (slot 0x68 with
    /// the master's `+0x470`) where the slice holds the unit's sprite. `Set_state` 2's slots are
    /// not modelled.
    pub(crate) fn explore_owner(
        &mut self,
        owner: Owner,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        use crate::movement::explore::Owner;
        match owner {
            Owner::Nothing => return Ok(()),
            Owner::Stopped => {
                return Err(GenieError::unsupported(
                    "an Explore's set_state 2 (slots 0x68)",
                ));
            }
            Owner::WantsPoint => self.post_own(
                &mut c.w.parts.unit_ai,
                NotifyKind::REQUIRES_SEARCH,
                [AiAction::Explore.value(), 0, 0],
            )?,
            Owner::Standing => {}
        }
        let standing = self
            .master(c.w.master_tables(), c.player)?
            .standing_sprite
            .and_then(|s| c.w.model.graphics.get(s))
            .copied();
        if let Some(m) = c.w.parts.mover_mut(self.id) {
            m.shows(standing.as_ref());
        }
        if self.sprite.is_some() {
            let standing = standing.ok_or(GenieError::unsupported(
                "an explorer without its standing sprite",
            ))?;
            new_sprite(
                &mut self.sprite,
                c.w.parts.move_stats_mut(self.id),
                standing,
                c.w.streams.get(Stream::Sprite),
                c.w.model.tuning.sprite,
            );
        }
        Ok(())
    }
}

/// The sprites' table of a world without sprites.
#[cfg(test)]
static NO_GRAPHICS: Graphics = Graphics::new();

/// What an Explore's `MoveTo` reads of a unit in state 2 with speed 1 in a world without sprites,
/// for tests that make an Explore.
#[cfg(test)]
pub(crate) fn bare_walk() -> ExploreMove<'static> {
    ExploreMove {
        reads: MoveToReads {
            sprites: SpriteReads {
                defaults: MoveDefaults::default(),
                graphics: &NO_GRAPHICS,
            },
            state: ObjectState::Alive,
            speed: Speed::new(1.0),
            at: Position::ORIGIN,
            ground: Footprint::ZERO,
        },
        given: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Footprint;
    use crate::map::spatial::TileSight;
    use crate::movement::Speed;
    #[cfg(test)]
    use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait, TargetMoved};
    use crate::objects::Graphics;
    use crate::objects::ObjectState;
    use crate::objects::SpeedMultiplier;
    use crate::objects::SpriteFlags;
    use crate::objects::Voice;
    use crate::objects::sprite::Graphic;
    #[cfg(test)]
    use crate::objects::{FacetCount, Frame};
    use crate::pathing::zones::ZoneId;
    use crate::testing::scene::{Scene, scout_area};
    use crate::testing::tuning;
    use crate::world::Seconds;
    use crate::world::World;

    const WALKING: Graphic = Graphic {
        id: SpriteId::named(7),
        frames: Frame::of_data(10),
        facets: FacetCount::of_data(16),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };

    /// What an Explore's `MoveTo` reads of a unit in state 2 with speed 1: the sprites in
    /// `graphics` with the point default 20, and the walking sprite `given`.
    fn walk_of(graphics: &Graphics, given: Option<SpriteId>) -> ExploreMove<'_> {
        let defaults = MoveDefaults {
            point: Some(SpriteId::named(20)),
            object: Some(SpriteId::named(21)),
        };
        ExploreMove {
            reads: MoveToReads {
                sprites: SpriteReads { defaults, graphics },
                state: ObjectState::Alive,
                speed: Speed::new(1.0),
                at: Position::ORIGIN,
                ground: Footprint::ZERO,
            },
            given,
        }
    }

    /// The sprite its `MoveTo` shows: the master's walking sprite, for none `FUN_140aa0d30`'s
    /// default.
    #[test]
    fn an_explore_move_to_shows_the_master_walking_sprite_else_the_default() {
        let other = Graphic {
            id: SpriteId::named(20),
            ..WALKING
        };
        let table = Graphics::from([(SpriteId::named(7), WALKING), (SpriteId::named(20), other)]);
        let sprite = |given| {
            let walk = walk_of(&table, given);
            let (a, _) =
                ExploreAction::new(Position::new(5.0, 5.0, 0.0), &walk, &tuning().move_to).unwrap();
            a.state.move_to().and_then(|m| m.sprite).map(|g| g.id)
        };
        assert_eq!(sprite(Some(SpriteId::named(7))), Some(SpriteId::named(7)));
        assert_eq!(sprite(None), Some(SpriteId::named(20)));
    }

    /// The scout map (`FUN_140f4a240`, `FUN_140f4d5e0`): 480 by 480 from tile (0, 0), each
    /// byte the fill; a set off the map is let be, a read off it is none.
    #[test]
    fn an_influence_map_holds_byte_per_tile_from_its_reference_point() {
        let mut m = InfluenceMap::new(GridSize::new(SCOUT_MAP_SIZE, SCOUT_MAP_SIZE), 0);
        assert_eq!(
            (
                m.get(Tile::new(0, 0)),
                m.get(Tile::new(479, 479)),
                m.get(Tile::new(480, 0))
            ),
            (Some(0), Some(0), None)
        );
        m.set(Tile::new(12, 34), 1);
        m.set(Tile::new(-1, 3), 1);
        m.set(Tile::new(3, 480), 1);
        assert_eq!(
            (m.get(Tile::new(12, 34)), m.get(Tile::new(34, 12))),
            (Some(1), Some(0))
        );
        assert_eq!(m.cells.iter().map(|&c| u32::from(c)).sum::<u32>(), 1);
        // a reference point shifts the grid
        let shifted = InfluenceMap {
            origin: Tile::new(10, 20),
            ..InfluenceMap::new(GridSize::new(4, 4), 7)
        };
        assert_eq!(
            (
                shifted.get(Tile::new(10, 20)),
                shifted.get(Tile::new(9, 20)),
                shifted.get(Tile::new(13, 23))
            ),
            (Some(7), None, Some(7))
        );
    }

    /// An Explore without a point waits for one; a point makes the quiet `MoveTo` to it (range
    /// 0, the sprite its maker resolved, a new wait), which a code of 1 drops again.
    #[test]
    fn an_explore_without_point_waits_for_one_then_walks() {
        let table = Graphics::from([(SpriteId::named(7), WALKING)]);
        let walk = walk_of(&table, Some(SpriteId::named(7)));
        let (mut a, owner) =
            ExploreAction::new(Position::new(-1.0, -1.0, 0.0), &walk, &tuning().move_to).unwrap();
        assert_eq!((a.state, owner), (ExploreState::Wait, Owner::WantsPoint));
        let walking = WALKING;
        assert_eq!(
            a.take_point(Position::new(19.0, 84.0, 0.0), &walk, &tuning().move_to),
            Ok(Owner::Nothing)
        );
        let made = MoveToAction {
            state: MoveTo::Start,
            point: Position::new(19.0, 84.0, 0.0),
            target: None,
            range: Distance::ZERO,
            sprite: Some(walking),
            voice: Voice::Quiet,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::NEW,
        };
        assert_eq!(
            a.state,
            ExploreState::Walk {
                move_to: Some(made)
            }
        );
        assert_eq!(
            a.after_walk(UpdateCode::UnderWay, &tuning().move_to),
            Ok(Owner::Nothing)
        );
        assert_eq!(
            a.after_walk(UpdateCode::Done, &tuning().move_to),
            Ok(Owner::WantsPoint)
        );
        assert_eq!(a.state, ExploreState::Wait);
        assert_eq!(
            a.take_point(Position::new(-1.0, 5.0, 0.0), &walk, &tuning().move_to),
            Ok(Owner::WantsPoint)
        );
        // a unit without speed fails its MoveTo (13), one in state 3 or later adds none
        let mut stuck = walk;
        stuck.reads.speed = Speed::ZERO;
        let (b, _) =
            ExploreAction::new(Position::new(5.0, 5.0, 0.0), &stuck, &tuning().move_to).unwrap();
        assert_eq!(b.state.move_to().map(|m| m.state), Some(MoveTo::Failed));
        stuck.reads.state = ObjectState::Dying;
        let (b, _) =
            ExploreAction::new(Position::new(5.0, 5.0, 0.0), &stuck, &tuning().move_to).unwrap();
        assert_eq!(b.state, ExploreState::Walk { move_to: None });
    }

    fn rng() -> Well512 {
        let mut r = Well512::default();
        r.set_seed(7);
        r
    }

    /// MarvinBlue/426048731's scout near its town centre: at (20.5, 92.5) on a map of 120 by
    /// 120 tiles of land, line of sight `sight`, on `scene`'s tiles (the scene's size is
    /// replaced).
    fn scouting(scene: Scene, sight: f32) -> World {
        scene
            .scout(Position::new(20.5, 92.5, 0.0), Distance::new(sight))
            .build()
    }

    /// 120 by 120 tiles of land, [`PLAYER`] seeing `sight` of each.
    fn map(sight: TileSight) -> Scene {
        Scene::open(GridSize::new(120, 120)).sight(sight)
    }

    /// A closed ring of rocks about the tiles from `low` to `high` (each corner inside), which no
    /// path crosses.
    fn walled(scene: Scene, low: Tile, high: Tile) -> Scene {
        let (x0, y0, x1, y1) = (low.x() - 1, low.y() - 1, high.x() + 1, high.y() + 1);
        let ring = (x0..=x1)
            .flat_map(|x| [Tile::new(x, y0), Tile::new(x, y1)])
            .chain((y0 + 1..y1).flat_map(|y| [Tile::new(x0, y), Tile::new(x1, y)]));
        scene.rocks(ring)
    }

    fn tiles(list: &[[i32; 2]]) -> Vec<Tile> {
        list.iter().map(|&[x, y]| Tile::new(x, y)).collect()
    }

    /// The last search's square: line of sight 2 makes it 4 tiles out. With every edge tile but
    /// one explored, that one wins whatever the shuffle; with the square all explored, the flood
    /// walks the explored tiles to the first unexplored one it meets.
    #[test]
    fn the_last_search_takes_the_square_then_floods_the_seen_tiles() {
        let square: Vec<Tile> = (16..=24)
            .flat_map(|x| (88..=96).map(move |y| Tile::new(x, y)))
            .collect();
        let edge_but_one = square.iter().copied().filter(|&t| t != Tile::new(24, 90));
        let mut w = scouting(
            map(TileSight::Unknown).sight_on(edge_but_one, TileSight::Explored),
            2.0,
        );
        let (mut area, map_size, _) = scout_area(&mut w);
        let found = beyond_sight(
            &mut area,
            Position::new(20.5, 92.5, 0.0),
            Distance::new(2.0),
            map_size,
            &mut rng(),
        );
        assert_eq!(found, Ok(Some(Tile::new(24, 90))));
        // all of the square explored: the flood leaves it left first, at (15, 92)
        let mut w = scouting(
            map(TileSight::Unknown).sight_on(square, TileSight::Explored),
            2.0,
        );
        let (mut area, map_size, _) = scout_area(&mut w);
        let found = beyond_sight(
            &mut area,
            Position::new(20.5, 92.5, 0.0),
            Distance::new(2.0),
            map_size,
            &mut rng(),
        );
        assert_eq!(found, Ok(Some(Tile::new(15, 92))));
    }

    /// MarvinBlue/426048731's first pick: the town centre at (29, 94), the scout at (20.5,
    /// 92.5); the nearest ring point by the gap from the scout's footprint wins, an explored one
    /// does not count, nor one walled in by rocks beyond the scout's reach (its line of sight 4
    /// less a tile).
    #[test]
    fn the_closest_open_point_of_the_rings_wins() {
        let tc = Some(Tile::new(29, 94));
        let size = GridSize::new(120, 120);
        let mut w = scouting(map(TileSight::Unknown), 4.0);
        let (mut area, _, _) = scout_area(&mut w);
        assert_eq!(
            unexplored_area(tc, size, &mut area),
            Ok(Some(Tile::new(19, 94)))
        );
        let explored = tiles(&[[19, 94], [19, 104]]);
        let mut w = scouting(
            map(TileSight::Unknown).sight_on(explored.clone(), TileSight::Explored),
            4.0,
        );
        let (mut area, _, _) = scout_area(&mut w);
        assert_eq!(
            unexplored_area(tc, size, &mut area),
            Ok(Some(Tile::new(14, 94)))
        );
        // (19, 84) walled in 4 tiles out: the reach of 3 cannot get to it
        let scene = map(TileSight::Unknown)
            .sight_on(explored, TileSight::Explored)
            .sight_on([Tile::new(14, 94)], TileSight::Explored);
        let mut w = scouting(walled(scene, Tile::new(15, 80), Tile::new(23, 88)), 4.0);
        let (mut area, _, _) = scout_area(&mut w);
        assert_eq!(
            unexplored_area(tc, size, &mut area),
            Ok(Some(Tile::new(9, 94)))
        );
        assert_eq!(unexplored_area(None, size, &mut area), Ok(None));
    }

    /// A map 40 wide and 60 high: the labelling starts patches only in its 40 by 40 square, and
    /// its fill (`FUN_141413190`, each row's `height` tiles from `row * width`) reaches the land
    /// below the water row 39 up to (19, 40): that land keeps the unreached zone 512 there. A
    /// pond's islet inside is numbered. The scout stands at (15.5, 35.5), line of sight 8, the
    /// town centre at (15, 30); every tile is explored but `open`.
    fn below_the_water(open: Tile) -> World {
        let size = GridSize::new(40, 60);
        let row = (0..40).map(|x| Tile::new(x, 39));
        let pond = (22..=28)
            .flat_map(|x| (27..=33).map(move |y| Tile::new(x, y)))
            .filter(|t| t.x() == 22 || t.x() == 28 || t.y() == 27 || t.y() == 33);
        Scene::open(size)
            .water(row.chain(pond))
            .sight(TileSight::Explored)
            .sight_on([open], TileSight::Unknown)
            .scout(Position::new(15.5, 35.5, 0.0), Distance::new(8.0))
            .build()
    }

    /// The rings compare the zones' low bytes (`CMP byte ptr [RSP+0x48],AL` at 0x140c65812):
    /// the point (15, 40) below the water, zone 512, counts as the scout's zone 0; the islet's
    /// (25, 30) does not. The unit AI's search compares whole zones: (15, 40) does not count
    /// there.
    #[test]
    fn the_rings_compare_the_zones_low_bytes() {
        let tc = Some(Tile::new(15, 30));
        let mut w = below_the_water(Tile::new(15, 40));
        let (mut area, map_size, _) = scout_area(&mut w);
        assert_eq!(area.own_zone(), ZoneId::new(0));
        assert_eq!(area.zone(Tile::new(15, 40)), ZoneId::new(0x200));
        assert_ne!(area.zone(Tile::new(25, 30)).byte(), 0);
        assert_eq!(
            unexplored_area(tc, map_size, &mut area),
            Ok(Some(Tile::new(15, 40)))
        );
        let seeker = Seeker {
            at: Position::new(15.5, 35.5, 0.0),
            line_of_sight: Distance::new(8.0),
            order: None,
            target_point: Point::new(NO_POINT, NO_POINT),
            player_kind: PlayerKind::Human,
        };
        assert_eq!(
            closest_undiscovered(&mut area, seeker, map_size, &mut rng()),
            Ok(None)
        );
        let mut w = below_the_water(Tile::new(25, 30));
        let (mut area, map_size, _) = scout_area(&mut w);
        assert_eq!(unexplored_area(tc, map_size, &mut area), Ok(None));
    }

    /// The unit AI's search (`FUN_140c4bd40`) on MarvinBlue/426048731's scout (line of sight 2,
    /// its square 5 tiles out): of the unexplored tiles in the square the nearest wins after
    /// one draw of the AI stream; walled in, it is the only one tested for reach, and none wins;
    /// a computer player's unit searches nothing and draws nothing.
    #[test]
    fn the_unit_ai_search_takes_the_nearest_unexplored_tile_it_reaches() {
        let seeker = Seeker {
            at: Position::new(20.5, 92.5, 0.0),
            line_of_sight: Distance::new(2.0),
            order: None,
            target_point: Point::new(NO_POINT, NO_POINT),
            player_kind: PlayerKind::Human,
        };
        // within the square: 16 and 25 tiles squared away, and one beyond
        let unexplored = tiles(&[[16, 92], [25, 92], [20, 97], [40, 92]]);
        let scene = map(TileSight::Explored).sight_on(unexplored, TileSight::Unknown);
        let mut w = scouting(scene.clone(), 2.0);
        let (mut area, size, _) = scout_area(&mut w);
        let mut ai = rng();
        let found = closest_undiscovered(&mut area, seeker, size, &mut ai);
        assert_eq!(found, Ok(Some(Tile::new(16, 92))));
        let mut once = rng();
        let _ = once.rand15();
        assert_eq!(ai.rand15(), once.rand15());
        // (16, 92) walled in beyond the reach of 1
        let mut w = scouting(
            walled(scene.clone(), Tile::new(15, 91), Tile::new(16, 92)),
            2.0,
        );
        let (mut area, size, _) = scout_area(&mut w);
        assert_eq!(
            closest_undiscovered(&mut area, seeker, size, &mut rng()),
            Ok(None)
        );
        let computer = Seeker {
            player_kind: PlayerKind::Computer,
            ..seeker
        };
        let mut w = scouting(scene, 2.0);
        let (mut area, size, _) = scout_area(&mut w);
        let mut untouched = rng();
        assert_eq!(
            closest_undiscovered(&mut area, computer, size, &mut untouched),
            Ok(None)
        );
        assert_eq!(untouched.rand15(), rng().rand15());
    }
}
