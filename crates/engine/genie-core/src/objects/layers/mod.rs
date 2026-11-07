//! The object's class chain (FD-045-D06): the game's object classes, each layer holding the one
//! below it, as the save reads them (`RgeMovingObject` holds `rge_animated_object`).
//!
//! A part of an object (its mover, an action, its unit AI) reaches its owner's layer through one
//! borrow, and a call the game makes on its own object is a method of that layer: the mover's
//! stop (slot 0x5d0) clears the animated layer's speed at once ([`Mover::stop`](crate::movement::Mover::stop)), and its motion
//! step moves the object's place through the static layer's setter ([`Static::new_position`]).
//! The row holds the static layer ([`Static`]); the layers above it an object's type has, up to
//! the action layer, are its [`Motion`], a part of the world's tables.

use crate::GenieResult;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::movement::MoveState;
use crate::objects::ObjectId;
use crate::pathing::Pathing;
use crate::vision::SightLayer;
use genie_diff::Diff;

mod action;
mod building;
mod combat;
mod command_count;
mod default_task;
mod idle_time;
mod motion;
mod speedup;

pub use self::action::Action;
pub use self::building::Building;
pub use self::combat::Combat;
pub use self::command_count::CommandCount;
pub use self::default_task::{DefaultTask, UnreadDefault, Unstartable, default_start};
pub use self::idle_time::{IdleCount, IdleTime};
pub use self::motion::{Animated, Motion, MotionMut};
pub use self::speedup::{Speedup, SpeedupValue};
use crate::map::spatial::Placed;
use crate::world::layer::Roles;
use crate::world::layer::generated::VisibilityTuning;

/// `RGE_Static_Object`'s place (FD-045-D15): x, y, z (`+0x3c8`, `+0x4c8`, `+0x5a8`, obfuscated in
/// the game's memory). Every object has it, below its other layers. Only its setter
/// ([`Static::new_position`]) moves it, and the setter moves the world's index of the places
/// with it, so the two never part.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Static {
    /// x, y, z on the map.
    position: Position,
}

impl Static {
    /// The layer of an object made or loaded at `position` (its constructor, or the load's read
    /// of the three fields): the world's index takes the place when the object goes into it.
    #[must_use]
    pub const fn at(position: Position) -> Self {
        Self { position }
    }

    /// x, y, z on the map.
    #[must_use]
    pub const fn position(&self) -> Position {
        self.position
    }

    /// `RGE_Static_Object::new_position` (`0x1411a58b0`) at `at`, which its callers hold on the
    /// map: the object takes the place, the world's index moves it between the tiles' lists and
    /// refiles its record, its sight follows ([`Spatial::set_position`]), and its mesh shape
    /// follows (`FUN_14118c540` -> `FUN_141056460`, [`Pathing::moved`]; a moving object's
    /// shape goes) where the caller holds the mesh.
    pub(crate) fn new_position(
        &mut self,
        id: ObjectId,
        at: Position,
        moving: MoveState,
        (world, pathing): (&mut Spatial, Option<&mut Pathing>),
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> GenieResult<()> {
        self.position = at;
        world.set_position(id, at, roles, visibility_tuning)?;
        if let (Some(pathing), Some(placed)) = (pathing, world.get(id)) {
            pathing.moved(id, placed, moving);
        }
        Ok(())
    }

    /// `FUN_14119d610` (x, y and z written as they are), then `new_position` at the same place, as
    /// `FUN_1411e7220` moves a unit on the map: [`Static::new_position`] whose every read of the
    /// object's place reads `at` already ([`Spatial::set_written_position`]); the mesh shape
    /// follows, standing.
    ///
    /// # Errors
    ///
    /// The sight's unsupported cases.
    pub(crate) fn new_position_written(
        &mut self,
        id: ObjectId,
        at: Position,
        world: &mut Spatial,
        pathing: &mut Pathing,
        sight: SightLayer<'_>,
    ) -> GenieResult<()> {
        self.position = at;
        world.set_written_position(id, at, sight.roles, sight.visibility_tuning)?;
        if let Some(placed) = world.get(id) {
            pathing.moved(id, placed, MoveState::Standing);
        }
        Ok(())
    }

    /// The object goes inside one at `at` (`FUN_14118c890`): it takes the container's x and y
    /// and keeps its height, and it leaves the world's index ([`Spatial::go_inside`]).
    pub(crate) fn go_inside(&mut self, id: ObjectId, at: Position, world: &mut Spatial) {
        self.position = Position::new(at.x(), at.y(), self.position.z());
        world.go_inside(id, self.position);
    }

    /// The object is put down at `at` with its record `placed` (slot 0x58 on an object that
    /// comes out of another, or whose record its caller took off the map): it takes the place,
    /// the world's index files the record there (its tile's list, its static marks), and the
    /// mesh takes its shape (`FUN_14118c540`, standing). Returns the record as filed.
    pub(crate) fn put_down(
        &mut self,
        id: ObjectId,
        at: Position,
        mut placed: Placed,
        (world, pathing): (&mut Spatial, &mut Pathing),
    ) -> Placed {
        self.position = at;
        placed.position = at;
        world.insert(id, placed);
        pathing.moved(id, &placed, MoveState::Standing);
        placed
    }

    /// A test's fixture puts the object at `at` without the world's index, which the test sets
    /// itself.
    #[cfg(test)]
    pub(crate) const fn place_for_test(&mut self, at: Position) {
        self.position = at;
    }

    /// An object inside another takes the container's place `at` as a copy of its fields
    /// (`FUN_141191b40` before the unload: an object inside takes no `new_position`), and so
    /// does its record inside ([`Spatial::set_inside_position`]).
    pub(crate) fn inside_at(&mut self, id: ObjectId, at: Position, world: &mut Spatial) {
        self.position = at;
        world.set_inside_position(id, at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Tile;
    use crate::map::spatial::Placed;
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::ObstructionKind;
    use crate::players::PlayerId;
    use crate::test_ids::object_id;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::testing::roles;
    use crate::testing::tuning;

    /// A small map with object 7 indexed at `at`.
    fn indexed(at: Position) -> Spatial {
        let mut world = Spatial::new(GridSize::new(10, 10));
        world.insert(
            object_id(7),
            Placed {
                position: at,
                radius: Footprint::new(0.2, 0.2),
                class: class::VILLAGER,
                player: PlayerId::new(1),
                master: ids::VILLAGER_MALE,
                obstruction_kind: ObstructionKind::NONE,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
        world
    }

    /// FD-045-01M62GVS08: one place setter writes the place and the spatial index.
    #[test]
    fn the_place_setter_moves_the_object_and_its_index_together() {
        let mut world = indexed(Position::new(1.5, 1.5, 0.0));
        let mut place = Static::at(Position::new(1.5, 1.5, 0.0));
        place
            .new_position(
                object_id(7),
                Position::new(2.5, 1.25, 0.0),
                MoveState::Moving,
                (&mut world, None),
                &roles(),
                tuning().visibility,
            )
            .unwrap();
        assert_eq!(place.position(), Position::new(2.5, 1.25, 0.0));
        assert_eq!(
            world.get(object_id(7)).map(|placed| placed.position),
            Some(Position::new(2.5, 1.25, 0.0))
        );
        assert_eq!(
            world.tiles().list(Tile::new(2, 1)),
            [object_id(7)],
            "it joins its new tile's list"
        );
        assert!(
            world.tiles().list(Tile::new(1, 1)).is_empty(),
            "and leaves its old one"
        );
    }

    #[test]
    fn an_object_going_inside_takes_its_containers_ground_place() {
        let mut world = indexed(Position::new(1.5, 1.5, 0.5));
        let mut base = Static::at(Position::new(1.5, 1.5, 0.5));
        base.go_inside(object_id(7), Position::new(4.0, 5.0, 0.0), &mut world);
        assert_eq!(
            base.position(),
            Position::new(4.0, 5.0, 0.5),
            "it keeps its height"
        );
        assert_eq!(
            world.get(object_id(7)).map(|placed| placed.position),
            Some(Position::new(4.0, 5.0, 0.5))
        );
        assert!(
            world.tiles().list(Tile::new(1, 1)).is_empty(),
            "it leaves its tile"
        );
        base.inside_at(object_id(7), Position::new(4.0, 5.0, 1.0), &mut world);
        assert_eq!(
            base.position(),
            Position::new(4.0, 5.0, 1.0),
            "a copy of the container's place"
        );
        assert_eq!(
            world.get(object_id(7)).map(|placed| placed.position),
            Some(Position::new(4.0, 5.0, 1.0))
        );
        let record = world.come_out(object_id(7)).expect("its record inside");
        let mut pathing = Pathing::default();
        let filed = base.put_down(
            object_id(7),
            Position::new(2.5, 2.5, 0.0),
            record,
            (&mut world, &mut pathing),
        );
        assert_eq!(base.position(), Position::new(2.5, 2.5, 0.0));
        assert_eq!(filed.position, Position::new(2.5, 2.5, 0.0));
        assert_eq!(
            world.get(object_id(7)).map(|placed| placed.position),
            Some(Position::new(2.5, 2.5, 0.0))
        );
        assert_eq!(
            world.tiles().list(Tile::new(2, 2)),
            [object_id(7)],
            "back on a tile's list"
        );
    }
}
