//! Finding a way: the planners of the paths that units walk, from the tile pathfinder over the
//! whole map to the short pathers around the obstructions near a unit.
//!
//! This file holds the world's path planning state besides the objects' places: the tile
//! pathfinder's planners (`world+0x140`, one per terrain restriction), the obstruction manager's
//! shape mesh (`*(DAT_143f0a088 + 8)`) with each object's shape, and the short pather's own state.
//!
//! The mesh's ids and orders follow the game's history and are not in a save, so a slice started
//! from a save gets them from the game ([`Pathing::set_mesh`]); from then on the slice keeps them as
//! the game does: every position change of a tall object with an obstruction record updates its
//! shape (`RGE_Static_Object::new_position` `0x1411a58b0` -> `FUN_14118c540` -> `FUN_141056460`),
//! and every change of a static record's tiles rebuilds the planners' cells over them (the
//! manager's callback `FUN_140f7d4b0` -> `FUN_140f7e4b0`).

use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Restriction;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::map::spatial::{BLOCKED, Obstruction, Placed, Tiles};
use crate::movement::MoveState;
use crate::objects::ObjectId;
use crate::objects::masters::ObstructionKind;
use crate::pathing::mesh::{Mesh, ONE};
use crate::pathing::short_path::Pather;
use crate::pathing::tile_path::PlanRect;
use crate::pathing::tile_path::Planner;
use crate::players::PlayerId;
use std::collections::BTreeMap;

pub use self::fixed_distance::{FixedDistance, FixedDistanceSquared};
pub use self::fixed_point::FixedPoint;
pub use self::fixed_rect::FixedRect;
pub use self::plan_budget_values::{DelayCount, PlanCap, PlanCount};
pub use self::shape_id::ShapeId;
pub use self::shape_side::ShapeSide;
pub use self::short_found::ShortFound;
use crate::map::RestrictionId;
use crate::objects::IdTable;
use std::mem::take;

mod fixed_distance;
mod fixed_point;
mod fixed_rect;
pub mod hull_path;
pub mod long_path;
pub mod mesh;
mod plan_budget_values;
mod shape_id;
mod shape_side;
mod short_found;
pub mod short_path;
pub mod tile_path;
pub mod zones;

/// What a planner is built from and the planners built so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pathing {
    /// The map's size in tiles.
    pub(crate) size: GridSize,
    /// Each tile's terrain, row by row.
    pub(crate) terrain: Vec<TerrainId>,
    /// Each terrain restriction's value per terrain (above 0 passes), by restriction id.
    pub(crate) restrictions: Vec<Restriction>,
    /// The game type (the world's `game_type`): type 12 has the escape the motion step runs for
    /// a unit on an `E` tile (`FUN_141158210` result 1), which the slice does not model.
    pub(crate) game_kind: u8,
    /// The restrictions the world's load gave planners (`FUN_140f7d310`, [`Pathing::register`]),
    /// in their order; None builds one for any restriction (a world made without a load).
    planned: Option<Vec<RestrictionId>>,
    planners: BTreeMap<RestrictionId, Planner>,
    /// Tile rectangles (x0, y0, x1, y1) whose static marks changed since the planners last saw
    /// them.
    pending: Vec<PlanRect>,
    /// The mesh, None until the slice has the game's.
    mesh: Option<Mesh>,
    /// Each object's shape (`+0x64c`).
    shapes: IdTable<ShapeId>,
    /// The temporary shapes of the motion step under way (its `local_1688`).
    temporary: Vec<ShapeId>,
    /// The short pather.
    pub(crate) pather: Pather,
    /// The ignore list the tile planner's smoothing hands the obstruction manager
    /// (`DAT_143f0db50`, six ids, a static the game never clears: zero at the load).
    smoothing_ignore: [ObjectId; 6],
    /// Each player's path budget (by player index): its plans this update and the caps the
    /// save holds ([`PlanBudget`]).
    pub(crate) budgets: Vec<PlanBudget>,
}

/// A player's path budget: `pathingAttemptCapValue` (`+0x674`), `pathingDelayCapValue`
/// (`+0x678`), both saved, and the plans its units and groups made since its last update
/// (`+0x67c`: `FUN_141663c50` counts one, `WorldPlayerBase::update` resets it, not saved).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlanBudget {
    /// `pathingAttemptCapValue`: −1 for no cap.
    pub(crate) attempt_cap: PlanCap,
    /// `pathingDelayCapValue`: −1 for no cap.
    pub(crate) delay_cap: PlanCap,
    /// The plans since the player's last update.
    pub(crate) plans: PlanCount,
}

impl PlanBudget {
    /// `FUN_14165d6f0` for a walker that waited `delays` times: no budget left (false) once the
    /// plans reached a set attempt cap, unless a set delay cap is below the delays.
    #[must_use]
    pub const fn allows(&self, delays: DelayCount) -> bool {
        !(delays.within(self.delay_cap) && self.plans.reached(self.attempt_cap))
    }
}

impl Pathing {
    /// Player `player`'s budget, a default (no caps, no plans) for one the slice has none of.
    #[must_use]
    pub fn budget(&self, player: PlayerId) -> PlanBudget {
        self.budgets
            .get(player.index())
            .copied()
            .unwrap_or(PlanBudget {
                attempt_cap: PlanCap::NONE,
                delay_cap: PlanCap::NONE,
                plans: PlanCount::ZERO,
            })
    }

    /// `FUN_141663c50`: player `player` made a plan.
    pub fn count_plan(&mut self, player: PlayerId) {
        if let Some(b) = self.budgets.get_mut(player.index()) {
            b.plans = b.plans.plus_one();
        }
    }

    /// `WorldPlayerBase::update` (`0x141677450`): the player's plans start over.
    pub fn reset_plans(&mut self, player: PlayerId) {
        if let Some(b) = self.budgets.get_mut(player.index()) {
            b.plans = PlanCount::ZERO;
        }
    }
}

/// A shape's rectangle for an object (`FUN_141056460`): its box grown by half a tile, in fixed
/// point by truncation.
#[must_use]
pub fn shape_rect(p: &Placed) -> FixedRect {
    let (x, y) = (p.position.x(), p.position.y());
    let (rx, ry) = (p.radius.x(), p.radius.y());
    let fixed = |v: f32| (v * ONE as f32) as i64 as i32;
    FixedRect::new(
        fixed((x - rx) - 0.5),
        fixed((y - ry) - 0.5),
        fixed((x + rx) + 0.5),
        fixed((y + ry) + 0.5),
    )
}

/// A shape's rectangle at the end of an action object's update (`FUN_141054fe0`): the place and
/// the radii each in fixed point by truncation, the box grown by half a tile (`0x200000`).
#[must_use]
pub fn refreshed_rect(position: Position, radius: Footprint) -> FixedRect {
    let fixed = |v: f32| (v * ONE as f32) as i64 as i32;
    let (x, y) = (position.x(), position.y());
    let (rx, ry) = (fixed(radius.x()), fixed(radius.y()));
    let half = ONE / 2;
    FixedRect::new(
        (fixed(x) - rx) - half,
        (fixed(y) - ry) - half,
        fixed(x) + half + rx,
        fixed(y) + half + ry,
    )
}

impl Pathing {
    /// The planners and the pather for a map, without a mesh yet.
    #[must_use]
    pub fn new(
        size: GridSize,
        terrain: Vec<TerrainId>,
        restrictions: Vec<Restriction>,
        game_kind: u8,
    ) -> Self {
        Self {
            size,
            terrain,
            restrictions,
            game_kind,
            ..Self::default()
        }
    }

    /// The ignore list of the tile planner's smoothing for `unit` and its level's `target`, as
    /// `FUN_140f7dbb0` fills its static (`DAT_143f0db50`): the unit; the target; a building
    /// target that is not an annex puts its `annexes` (in their slots' order) from the target's
    /// slot on, overwriting it, and counts one more slot than it writes, whose id an earlier
    /// list left there (zero, object 0, until a list writes it). Without a target only the unit.
    /// The list is written only when the smoothing runs ([`Pathing::smoothing_ran`]).
    #[must_use]
    pub fn smoothing_ignore(
        &self,
        unit: ObjectId,
        target: Option<(ObjectId, &[ObjectId])>,
    ) -> ([ObjectId; 6], Vec<ObjectId>) {
        let mut slots = self.smoothing_ignore;
        slots[0] = unit;
        let Some((target, annexes)) = target else {
            return (slots, vec![unit]);
        };
        slots[1] = target;
        let mut count = 2;
        for (i, (cell, &a)) in slots
            .iter_mut()
            .skip(1)
            .zip(annexes.iter().take(4))
            .enumerate()
        {
            *cell = a;
            count = 3 + i;
        }
        (slots, slots.iter().take(count).copied().collect())
    }

    /// The smoothing ran with the ignore list `slots` ([`Pathing::smoothing_ignore`]): the static
    /// keeps it.
    pub const fn smoothing_ran(&mut self, slots: [ObjectId; 6]) {
        self.smoothing_ignore = slots;
    }

    /// An empty mesh for a map `width` tiles wide, as the game's load starts it.
    pub fn start_mesh(&mut self, width: u32) {
        self.mesh = Some(Mesh::new(width));
        self.shapes.clear();
        self.temporary.clear();
    }

    /// The game's mesh and each object's shape in it, at the tick the slice starts from.
    pub fn set_mesh(&mut self, mesh: Mesh, shapes: impl IntoIterator<Item = (ObjectId, ShapeId)>) {
        self.mesh = Some(mesh);
        self.shapes = shapes.into_iter().collect();
    }

    /// The mesh, once set.
    #[must_use]
    pub const fn mesh(&self) -> Option<&Mesh> {
        self.mesh.as_ref()
    }

    /// The mesh and the short pather together, for a search.
    pub fn mesh_and_pather(&mut self) -> Option<(&mut Mesh, &mut Pather)> {
        Some((self.mesh.as_mut()?, &mut self.pather))
    }

    /// An object's shape in the mesh.
    #[must_use]
    pub fn shape(&self, id: ObjectId) -> Option<ShapeId> {
        self.shapes.get(id).copied()
    }

    /// The world's load gives restriction `restriction` a planner (`FUN_140f7d310`), unless the
    /// lookup finds one ([`Pathing::planned_for`]).
    pub fn register(&mut self, restriction: RestrictionId) {
        let _ = self.planned.get_or_insert_with(Vec::new);
        if self.restrictions.get(restriction.index()).is_some()
            && self.planned_for(restriction).is_none()
        {
            self.planned.get_or_insert_with(Vec::new).push(restriction);
        }
    }

    /// `FUN_140f7ca70`, the load's planners searched for restriction `restriction`'s values (the
    /// world's table, `+0x18` each): the first planner whose values (its copy, `+0x3c0`) are
    /// another length than the restriction's, or none, or each equal (`==`, so a NaN never) to
    /// its value. None when none is (`0`), for a restriction the data has not got, and without
    /// the load's planners.
    fn planned_for(&self, restriction: RestrictionId) -> Option<RestrictionId> {
        let values = self.restrictions.get(restriction.index())?;
        self.planned.as_ref()?.iter().copied().find(|&r| {
            let own = self
                .restrictions
                .get(r.index())
                .map_or(&[][..], Restriction::values);
            let values = values.values();
            own.len() != values.len()
                || own.is_empty()
                || own.iter().zip(values).all(|(a, b)| a == b)
        })
    }

    /// The planner of a terrain restriction (`FUN_140f7cb30` -> `FUN_140f7ca70`): the first of
    /// the load's planners whose restriction has the same values, else none (the slice builds it
    /// from the map on first use, `FUN_140f74f90`); the planners built before see the tiles'
    /// changes first. None for a restriction the data has not got.
    pub fn planner(&mut self, restriction: RestrictionId, tiles: &Tiles) -> Option<&mut Planner> {
        let blocked = |tile: Tile| tiles.word(tile).is_some_and(|w| w & BLOCKED != 0);
        for rect in self.pending.drain(..) {
            for p in self.planners.values_mut() {
                p.update(rect, &blocked);
            }
        }
        let _ = self.restrictions.get(restriction.index())?;
        let key = match self.planned.as_ref() {
            None => restriction,
            Some(_) => self.planned_for(restriction)?,
        };
        if !self.planners.contains_key(&key) {
            let passable = self.restrictions.get(key.index())?.clone();
            let planner = Planner::new(self.size, self.terrain.clone(), passable, &blocked);
            let _ = self.planners.insert(key, planner);
        }
        self.planners.get_mut(&key)
    }

    /// Static marks changed over these tile rectangles.
    pub fn tiles_changed(&mut self, rects: impl IntoIterator<Item = PlanRect>) {
        self.pending.extend(rects);
    }

    /// The object's position changed (`FUN_141056460`'s shape part, through `FUN_14118c540`): a
    /// tall object with a record of a kind other than 0 and 12 and a class other than 0 has a shape
    /// while it is not moving (object slot 185, `0x1411583d0`), added or replaced
    /// (`FUN_140fdb8d0`, `FUN_140fdc730`), and none while it moves (`FUN_140fdc250`). Without the
    /// game's mesh nothing is kept.
    pub fn moved(&mut self, id: ObjectId, p: &Placed, moving: MoveState) {
        let Some(mesh) = self.mesh.as_mut() else {
            return;
        };
        let has_shape = p.tall && p.obstruction.is_some_and(Obstruction::recorded);
        if !has_shape {
            return;
        }
        if moving == MoveState::Moving {
            if let Some(s) = self.shapes.remove(id) {
                mesh.remove(s);
            }
            return;
        }
        let rect = shape_rect(p);
        let s = match self.shapes.get(id) {
            None => mesh.add(rect),
            Some(&s) => mesh.replace(s, rect),
        };
        let _ = self.shapes.insert(id, s);
    }

    /// `FUN_141056460`'s shape for an object that does not move, by its master's obstruction
    /// kind and class whether or not it has its record (the load's rehook): added or replaced.
    pub fn place_shape(&mut self, id: ObjectId, p: &Placed) {
        let Some(mesh) = self.mesh.as_mut() else {
            return;
        };
        let rect = shape_rect(p);
        let s = match self.shapes.get(id) {
            None => mesh.add(rect),
            Some(&s) => mesh.replace(s, rect),
        };
        let _ = self.shapes.insert(id, s);
    }

    /// The end of an action object's update (`RGE_Action_Object::update` -> `FUN_141163080` ->
    /// `FUN_141055270` -> `FUN_141054fe0`), every tick: an object with an obstruction record has a
    /// shape while it is not moving (slot 185), its box (`outline`, the master's `+0x688`,
    /// `+0x68c`, for obstruction type 11, else its radii) grown by half a tile
    /// ([`refreshed_rect`]), added or replaced (an odd box always replaces: its shape is removed
    /// and added again); none while it moves. The record's moving flag (byte 3, 0x10) is not
    /// held: the walk reads the gait. Without the game's mesh nothing is kept.
    pub fn refresh(&mut self, id: ObjectId, p: &Placed, moving: MoveState, outline: Footprint) {
        let Some(mesh) = self.mesh.as_mut() else {
            return;
        };
        if p.obstruction.is_none() {
            return;
        }
        if moving == MoveState::Moving {
            if let Some(s) = self.shapes.remove(id) {
                mesh.remove(s);
            }
            return;
        }
        let radius = if p.obstruction_kind == ObstructionKind::KIND_11 {
            outline
        } else {
            p.radius
        };
        let rect = refreshed_rect(p.position, radius);
        let s = match self.shapes.get(id) {
            None => mesh.add(rect),
            Some(&s) => mesh.replace(s, rect),
        };
        let _ = self.shapes.insert(id, s);
    }

    /// A temporary shape for a kept hit of the walk (`FUN_141054fe0` with 1 on an object without a
    /// shape, which the object does not keep): [`refreshed_rect`] of its radii.
    pub fn add_temporary_shape(&mut self, p: &Placed) {
        if let Some(mesh) = self.mesh.as_mut() {
            let s = mesh.add(refreshed_rect(p.position, p.radius));
            self.temporary.push(s);
        }
    }

    /// The motion step's exit (`FUN_141055970` on each): the temporary shapes leave the mesh in
    /// the order they came.
    pub fn drop_temporary_shapes(&mut self) {
        let shapes = take(&mut self.temporary);
        if let Some(mesh) = self.mesh.as_mut() {
            for s in shapes {
                mesh.remove(s);
            }
        }
    }

    /// The object left the world: its shape leaves the mesh.
    pub fn removed(&mut self, id: ObjectId) {
        if let (Some(mesh), Some(s)) = (self.mesh.as_mut(), self.shapes.remove(id)) {
            mesh.remove(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::RestrictionId;
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::ObstructionClass;
    use crate::test_ids::object_id;
    use crate::testing::class;
    use crate::testing::ids;

    /// A short path's result keeps the game's code, and the tests its readers make by code:
    /// `result < 2` reaches, 1 and 2 lead on, 3 and 4 fail.
    #[test]
    fn short_path_result_reads_by_the_game_code() {
        let all = [
            ShortFound::InRange,
            ShortFound::Reached,
            ShortFound::Partial,
            ShortFound::Blocked,
            ShortFound::Invalid,
        ];
        assert_eq!(all.map(ShortFound::code), [0, 1, 2, 3, 4]);
        assert_eq!(
            all.map(ShortFound::reaches),
            [true, true, false, false, false]
        );
        assert_eq!(
            all.map(ShortFound::leads),
            [false, true, true, false, false]
        );
        assert_eq!(
            all.map(ShortFound::fails),
            [false, false, false, true, true]
        );
    }

    /// A fixed-point length adds and subtracts as the game's 32-bit words, wrapping.
    #[test]
    fn fixed_distance_wraps_as_32_bit_word() {
        let a = FixedDistance::new(i32::MAX);
        assert_eq!(
            a.wrapping_add(FixedDistance::TILE).get(),
            i32::MIN + ONE - 1
        );
        assert_eq!(
            FixedDistance::ZERO.wrapping_sub(FixedDistance::TILE).get(),
            -ONE
        );
        assert!(FixedDistance::ZERO < FixedDistance::TILE);
    }

    /// A fixed-point length squares in 64 bits, `>> 22`: a tile's square is a tile, a length
    /// past 32 bits' square root does not wrap, and the sign drops.
    #[test]
    fn fixed_distance_squares_in_64_bits() {
        let tile = FixedDistanceSquared::of(FixedDistance::TILE);
        assert_eq!(tile.get(), i64::from(ONE));
        let far = FixedDistance::new(i32::MAX);
        assert_eq!(
            FixedDistanceSquared::of(far).get(),
            (i64::from(i32::MAX) * i64::from(i32::MAX)) >> 22
        );
        assert_eq!(FixedDistanceSquared::of(FixedDistance::new(-ONE)), tile);
        assert!(FixedDistanceSquared::ZERO < tile);
    }

    /// `FUN_14165d6f0`: no budget once the plans reached a set attempt cap, unless the walker
    /// waited more often than a set delay cap allows; caps of -1 never stop a plan. A plan counts
    /// for its player, and the player's update starts the count over.
    #[test]
    fn players_budget_ends_at_its_attempt_cap() {
        let b = PlanBudget {
            attempt_cap: PlanCap::of_data(6),
            delay_cap: PlanCap::of_data(2),
            plans: PlanCount::of_data(6),
        };
        assert!(!b.allows(DelayCount::ZERO));
        assert!(!b.allows(DelayCount::of_data(2)));
        assert!(b.allows(DelayCount::of_data(3)));
        assert!(
            PlanBudget {
                plans: PlanCount::of_data(5),
                ..b
            }
            .allows(DelayCount::ZERO)
        );
        assert!(
            PlanBudget {
                attempt_cap: PlanCap::NONE,
                ..b
            }
            .allows(DelayCount::ZERO)
        );
        assert!(
            !PlanBudget {
                delay_cap: PlanCap::NONE,
                ..b
            }
            .allows(DelayCount::of_data(9))
        );
        let mut p = Pathing {
            budgets: vec![PlanBudget::default(), b],
            ..Pathing::default()
        };
        p.count_plan(PlayerId::new(1));
        assert_eq!(p.budget(PlayerId::new(1)).plans, PlanCount::of_data(7));
        p.reset_plans(PlayerId::new(1));
        assert_eq!(p.budget(PlayerId::new(1)).plans, PlanCount::ZERO);
        // a player the slice holds no budget of: no caps
        assert!(p.budget(PlayerId::new(5)).allows(DelayCount::ZERO));
        p.count_plan(PlayerId::new(5));
    }

    /// `FUN_140f7ca70`: the first planner whose values are another length, or none, or all
    /// equal (a NaN never equal) to the restriction's; `FUN_140f7d310` adds a planner only when
    /// none is found.
    #[test]
    fn the_planner_lookup_takes_the_first_of_another_length_or_of_equal_values() {
        let mut p = Pathing {
            restrictions: [
                vec![1.0, 0.0],
                vec![1.0, 0.0],
                vec![0.0, 1.0],
                vec![1.0, 0.0, 1.0],
                vec![],
                vec![f32::NAN, 0.0],
            ]
            .map(Restriction::new)
            .into(),
            ..Pathing::default()
        };
        p.register(RestrictionId::named(2));
        p.register(RestrictionId::named(0));
        p.register(RestrictionId::named(1));
        assert_eq!(
            p.planned,
            Some(vec![RestrictionId::named(2), RestrictionId::named(0)])
        );
        assert_eq!(
            p.planned_for(RestrictionId::named(1)),
            Some(RestrictionId::named(0))
        );
        assert_eq!(
            p.planned_for(RestrictionId::named(0)),
            Some(RestrictionId::named(0))
        );
        // another length or none: the first planner
        assert_eq!(
            p.planned_for(RestrictionId::named(3)),
            Some(RestrictionId::named(2))
        );
        assert_eq!(
            p.planned_for(RestrictionId::named(4)),
            Some(RestrictionId::named(2))
        );
        p.register(RestrictionId::named(3));
        p.register(RestrictionId::named(4));
        assert_eq!(
            p.planned,
            Some(vec![RestrictionId::named(2), RestrictionId::named(0)])
        );
        // a NaN equals nothing: a planner of its own
        assert_eq!(p.planned_for(RestrictionId::named(5)), None);
        p.register(RestrictionId::named(5));
        assert_eq!(
            p.planned,
            Some(vec![
                RestrictionId::named(2),
                RestrictionId::named(0),
                RestrictionId::named(5)
            ])
        );
        assert_eq!(p.planned_for(RestrictionId::named(5)), None);
        // a restriction the data has not got
        assert_eq!(p.planned_for(RestrictionId::named(9)), None);
    }

    /// 425906646's villagers walking to player 6's town centre 27171 (annexes 27172, 27173,
    /// 27175) in update 6292: the smoothing ignores the unit and the annexes, the target's slot
    /// overwritten, and one slot more than written, here never written: object 0.
    #[test]
    fn the_smoothing_ignores_main_building_annexes_over_its_own_slot() {
        let mut p = Pathing::default();
        assert_eq!(
            p.smoothing_ignore(object_id(31128), None).1,
            [object_id(31128)]
        );
        assert_eq!(
            p.smoothing_ignore(object_id(31128), Some((object_id(9156), &[])))
                .1,
            [object_id(31128), object_id(9156)]
        );
        let (slots, ignore) = p.smoothing_ignore(
            object_id(31128),
            Some((
                object_id(27171),
                &[object_id(27172), object_id(27173), object_id(27175)],
            )),
        );
        assert_eq!(
            ignore,
            [
                object_id(31128),
                object_id(27172),
                object_id(27173),
                object_id(27175),
                object_id(0)
            ]
        );
        // the static keeps the list once the smoothing ran: a later one-annex list's extra slot
        // holds the second annex
        p.smoothing_ran(slots);
        let (_, ignore) = p.smoothing_ignore(
            object_id(31143),
            Some((object_id(5000), &[object_id(5001)])),
        );
        assert_eq!(
            ignore,
            [object_id(31143), object_id(5001), object_id(27173)]
        );
    }
    use crate::map::spatial::Obstruction;

    fn villager(x: f32, y: f32) -> Placed {
        Placed {
            position: Position::new(x, y, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::VILLAGER,
            player: PlayerId::new(2),
            master: ids::BUILDER,
            obstruction_kind: ObstructionKind::CIRCLE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: true,
            unknown: None,
            obstruction: Some(Obstruction {
                kind: ObstructionKind::CIRCLE,
                class: ObstructionClass::UNIT,
            }),
        }
    }

    #[test]
    fn unit_shape_is_its_box_grown_by_half_tile() {
        let r = shape_rect(&villager(30.0, 89.0));
        assert_eq!(
            r,
            FixedRect::new(
                (29.3f32 * 4_194_304.0) as i32,
                (88.3f32 * 4_194_304.0) as i32,
                (30.7f32 * 4_194_304.0) as i32,
                (89.7f32 * 4_194_304.0) as i32
            )
        );
    }

    #[test]
    /// MarvinBlue/426048731's update 1141: the hunter 10029 at (106.53662, 46.65576), standing,
    /// gets its shape at its update's end, the box of its radii (0.2) grown by half a tile.
    fn standing_unit_gets_its_shape_at_its_update_end() {
        let mut p = Pathing::default();
        p.set_mesh(Mesh::new(120), []);
        let hunter = villager(106.53662, 46.65576);
        p.refresh(
            object_id(10029),
            &hunter,
            MoveState::Standing,
            Footprint::ZERO,
        );
        let s = p.shape(object_id(10029)).unwrap();
        let r = refreshed_rect(hunter.position, hunter.radius);
        assert_eq!(
            r.x0(),
            (106.53662f32 * 4_194_304.0) as i32 - 838_860 - 0x20_0000
        );
        assert_eq!(
            p.mesh().unwrap().shapes[s.index()],
            FixedRect::new(
                r.x0() & !1,
                r.y0() & !1,
                (r.x1() + 1) & !1,
                (r.y1() + 1) & !1
            )
        );
        // moving, it has none; without a record, none either
        p.refresh(
            object_id(10029),
            &hunter,
            MoveState::Moving,
            Footprint::ZERO,
        );
        assert_eq!(p.shape(object_id(10029)), None);
        let mut bare = hunter;
        bare.obstruction = None;
        p.refresh(
            object_id(10029),
            &bare,
            MoveState::Standing,
            Footprint::ZERO,
        );
        assert_eq!(p.shape(object_id(10029)), None);
    }

    #[test]
    fn unit_has_shape_while_it_stands_and_none_while_it_moves() {
        let mut p = Pathing::default();
        p.set_mesh(Mesh::new(120), []);
        p.moved(object_id(9175), &villager(30.0, 89.0), MoveState::Standing);
        let s = p.shape(object_id(9175)).unwrap();
        assert_eq!(p.mesh().unwrap().shapes[s.index()], {
            let r = shape_rect(&villager(30.0, 89.0));
            FixedRect::new(
                r.x0() & !1,
                r.y0() & !1,
                (r.x1() + 1) & !1,
                (r.y1() + 1) & !1,
            )
        });
        p.moved(object_id(9175), &villager(30.1, 89.0), MoveState::Moving);
        assert_eq!(p.shape(object_id(9175)), None);
        // a foundation without its record has none
        let mut site = villager(30.0, 88.0);
        site.obstruction = None;
        p.moved(object_id(10028), &site, MoveState::Standing);
        assert_eq!(p.shape(object_id(10028)), None);
    }
}
