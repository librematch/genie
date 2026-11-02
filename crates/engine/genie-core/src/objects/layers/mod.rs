//! The object's class chain (FD-045-D06): the game's object classes, each layer holding the one
//! below it, as the save reads them (`RgeMovingObject` holds `rge_animated_object`).
//!
//! A part of an object (its mover, an action, its unit AI) reaches its owner's layer through one
//! borrow, and a call the game makes on its own object is a method of that layer: the mover's
//! stop (slot 0x5d0) clears the animated layer's speed at once ([`Mover::stop`]), and its motion
//! step moves the object's place through the static layer's setter ([`Static::new_position`]).
//! The row holds the static layer ([`Static`]); the layers above it an object's type has, up to
//! the action layer, are its [`MotionLayer`], a part of the world's tables.

use crate::GenieError;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::pathing::Pathing;
use genie_diff::Diff;

mod action;
mod building;
mod combat;

pub use self::action::ActionLayer;
pub use self::building::Building;
pub use self::combat::Combat;

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
    /// follows (`FUN_14118c540` -> `FUN_141056460`, [`Pathing::moved`]; a `moving` object's
    /// shape goes) where the caller holds the mesh.
    pub(crate) fn new_position(
        &mut self,
        id: ObjectId,
        at: Position,
        moving: bool,
        (world, pathing): (&mut Spatial, Option<&mut Pathing>),
    ) -> Result<(), GenieError> {
        self.position = at;
        world.set_position(id, at)?;
        if let (Some(pathing), Some(p)) = (pathing, world.get(id)) {
            pathing.moved(id, p, moving);
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
        mut placed: crate::map::spatial::Placed,
        (world, pathing): (&mut Spatial, &mut Pathing),
    ) -> crate::map::spatial::Placed {
        self.position = at;
        placed.position = at;
        world.insert(id, placed);
        pathing.moved(id, &placed, false);
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

/// `RGE_Animated_Object`'s own field (types 20 and up): `mCurrentSpeed` (`+0x7b8`), the speed the
/// object's sprite clock runs at. Its constructor (`FUN_141070cf0`) sets it to 0; a moving
/// unit's velocity sets it as it walks, and a stop clears it. The static layer below it is the
/// object's row's ([`Static`], `Unit::place`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Animated {
    /// `mCurrentSpeed` (`+0x7b8`).
    pub speed: Speed,
}

impl Animated {
    /// An animated layer at `speed`.
    #[must_use]
    pub const fn new(speed: Speed) -> Self {
        Self { speed }
    }
}

/// The layers of an object between its static layer (the row's place, [`Static`]) and its action
/// layer, by its type (FD-045-D58): the animated object (types 20 to 29), or the moving object
/// (30 and up), which holds the animated one. A static object (types below 20) has none: the
/// world's table ([`crate::world::parts::Parts::motion`]) holds no entry for it.
#[derive(Clone, Debug, PartialEq, Diff)]
#[expect(
    clippy::large_enum_variant,
    reason = "nearly every object of the table moves; a box per mover would cost more than the few animated ones save"
)]
pub enum MotionLayer {
    /// An animated object that does not move (types 20 to 29).
    Animated(Animated),
    /// A moving object (types 30 and up), its animated layer inside its mover.
    Moving(Mover),
}

impl MotionLayer {
    /// The layers of an object with `animated` and `mover` (each None where its type has none):
    /// a mover holds the animated layer (`animated`, else the mover's own), else the animated
    /// layer stands alone, else the object is static (none).
    #[must_use]
    pub fn of(animated: Option<Animated>, mover: Option<Mover>) -> Option<Self> {
        match (animated, mover) {
            (a, Some(mut m)) => {
                if let Some(a) = a {
                    m.animated = a;
                }
                Some(Self::Moving(m))
            }
            (Some(a), None) => Some(Self::Animated(a)),
            (None, None) => None,
        }
    }

    /// The moving layer, if the object moves.
    #[must_use]
    pub const fn mover(&self) -> Option<&Mover> {
        match self {
            Self::Moving(m) => Some(m),
            Self::Animated(_) => None,
        }
    }

    /// The moving layer to change, if the object moves.
    pub const fn mover_mut(&mut self) -> Option<&mut Mover> {
        match self {
            Self::Moving(m) => Some(m),
            Self::Animated(_) => None,
        }
    }

    /// The animated layer: its own, or its mover's.
    #[must_use]
    pub const fn animated(&self) -> &Animated {
        match self {
            Self::Animated(a) | Self::Moving(Mover { animated: a, .. }) => a,
        }
    }

    /// The animated layer to change.
    pub const fn animated_mut(&mut self) -> &mut Animated {
        match self {
            Self::Animated(a) | Self::Moving(Mover { animated: a, .. }) => a,
        }
    }
}

#[cfg(test)]
impl MotionLayer {
    /// A test's own layer, apart from a world, to change through [`MotionMut`].
    pub(crate) const fn lent(&mut self) -> MotionMut<'_> {
        MotionMut(Some(self))
    }
}

/// An object's motion layer to change beside its row, none for a static object (the world's
/// table lends it, [`crate::world::parts::Parts::motion_mut`]): what a routine that stops,
/// shows or speeds its object reaches of the layers above its place.
#[derive(Debug)]
pub struct MotionMut<'a>(Option<&'a mut MotionLayer>);

impl<'a> MotionMut<'a> {
    /// The layer `motion` to change (none for a static object).
    #[must_use]
    pub const fn new(motion: Option<&'a mut MotionLayer>) -> Self {
        Self(motion)
    }

    /// The moving layer, if the object moves.
    #[must_use]
    pub fn mover(&self) -> Option<&Mover> {
        self.0.as_deref().and_then(MotionLayer::mover)
    }

    /// The moving layer to change, if the object moves.
    pub fn mover_mut(&mut self) -> Option<&mut Mover> {
        self.0.as_deref_mut().and_then(MotionLayer::mover_mut)
    }

    /// The movement's values to change (its top speed among them), if the object moves.
    pub fn move_stats_mut(&mut self) -> Option<&mut crate::movement::MoveStats> {
        self.mover_mut().map(|m| &mut m.stats)
    }

    /// The animation speed (`+0x7b8`), none for a static object.
    #[must_use]
    pub fn anim_speed(&self) -> Option<Speed> {
        self.0.as_deref().map(|m| m.animated().speed)
    }

    /// Sets the animation speed (`+0x7b8`) of an object that has one; a static object has none.
    pub fn set_anim_speed(&mut self, speed: Speed) {
        if let Some(m) = self.0.as_deref_mut() {
            m.animated_mut().speed = speed;
        }
    }

    /// The same layer for a call that ends before this one is used again.
    pub fn reborrow(&mut self) -> MotionMut<'_> {
        MotionMut(self.0.as_deref_mut())
    }

    /// The mover for as long as the layer was lent, none for an object that does not move.
    #[must_use]
    pub fn into_mover(self) -> Option<&'a mut Mover> {
        self.0.and_then(MotionLayer::mover_mut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Tile;
    use crate::map::spatial::Placed;
    use crate::movement::Speed;
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::ObstructionType;
    use crate::objects::masters::class;
    use crate::objects::masters::ids;
    use crate::players::PlayerId;
    use crate::test_ids::object_id;

    fn mover() -> Mover {
        crate::movement::tests::builder_9177()
    }

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
                obstruction_type: ObstructionType::NONE,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
        world
    }

    #[test]
    fn a_motion_holds_the_layers_its_type_has() {
        let a = Animated::new(Speed::new(0.5));
        assert_eq!(
            MotionLayer::of(None, None),
            None,
            "a static object has none"
        );
        assert_eq!(
            MotionLayer::of(Some(a), None).map(|m| m.animated().speed),
            Some(Speed::new(0.5))
        );
        let moving = MotionLayer::of(Some(a), Some(mover())).unwrap();
        assert_eq!(
            moving.mover().map(|m| m.animated.speed),
            Some(Speed::new(0.5))
        );
        assert_eq!(moving.animated().speed, Speed::new(0.5));
        let mut own = mover();
        own.animated.speed = Speed::new(0.25);
        assert_eq!(
            MotionLayer::of(None, Some(own)).map(|m| m.animated().speed),
            Some(Speed::new(0.25)),
            "a mover always has an animated layer, its own without another"
        );
    }

    #[test]
    fn only_a_moving_object_has_a_mover() {
        let mut flag = MotionLayer::Animated(Animated::default());
        flag.animated_mut().speed = Speed::new(1.0);
        assert_eq!(
            (flag.animated().speed, flag.mover()),
            (Speed::new(1.0), None)
        );
        let mut walker = MotionLayer::Moving(mover());
        walker.animated_mut().speed = Speed::new(2.0);
        assert_eq!(
            walker.mover_mut().map(|m| m.animated.speed),
            Some(Speed::new(2.0))
        );
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
                true,
                (&mut world, None),
            )
            .unwrap();
        assert_eq!(place.position(), Position::new(2.5, 1.25, 0.0));
        assert_eq!(
            world.get(object_id(7)).map(|p| p.position),
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
            world.get(object_id(7)).map(|p| p.position),
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
            world.get(object_id(7)).map(|p| p.position),
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
            world.get(object_id(7)).map(|p| p.position),
            Some(Position::new(2.5, 2.5, 0.0))
        );
        assert_eq!(
            world.tiles().list(Tile::new(2, 2)),
            [object_id(7)],
            "back on a tile's list"
        );
    }
}
