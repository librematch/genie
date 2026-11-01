//! The object's class chain (FD-045-D06): the game's object classes, each layer holding the one
//! below it, as the save reads them (`RgeMovingObject` holds `rge_animated_object`).
//!
//! A part of an object (its mover, an action, its unit AI) reaches its owner's layer through one
//! borrow, and a call the game makes on its own object is a method of that layer: the mover's
//! stop (slot 0x5d0) clears the animated layer's speed at once ([`Mover::stop`]), and its motion
//! step moves the object's place through the static layer's setter ([`Static::new_position`]).
//! The layers below the action layer an object has are its [`Body`].

use crate::GenieError;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::pathing::Pathing;
use genie_diff::Diff;

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
/// unit's velocity sets it as it walks, and a stop clears it. It holds the static layer
/// ([`Static`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct Animated {
    /// `mCurrentSpeed` (`+0x7b8`).
    pub speed: Speed,
    /// The static layer below it: the object's place.
    base: Static,
}

impl Animated {
    /// An animated layer at `speed`, its place the origin until its body places it
    /// ([`Body::new`]).
    #[must_use]
    pub const fn new(speed: Speed) -> Self {
        Self {
            speed,
            base: Static::at(Position::ORIGIN),
        }
    }

    /// An animated layer at `speed` whose object stands at `position`.
    #[must_use]
    pub const fn at(speed: Speed, position: Position) -> Self {
        Self {
            speed,
            base: Static::at(position),
        }
    }

    /// The static layer below it.
    #[must_use]
    pub const fn base(&self) -> &Static {
        &self.base
    }

    /// The static layer below it, to move the object ([`Static::new_position`]).
    pub(crate) const fn base_mut(&mut self) -> &mut Static {
        &mut self.base
    }
}

/// The layers of an object below its action layer, by its type: the static object alone (types
/// below 20), the animated object (20 to 29), or the moving object (30 and up), which holds the
/// animated one. Each holds the static layer with the object's place ([`Static`]).
#[derive(Clone, Debug, PartialEq, Diff)]
#[expect(
    clippy::large_enum_variant,
    reason = "nearly every object moves; a box per unit would cost more than the static few save"
)]
pub enum Body {
    /// A static object (types below 20): its place, no animation speed, no movement.
    Static(Static),
    /// An animated object that does not move (types 20 to 29).
    Animated(Animated),
    /// A moving object (types 30 and up), its animated layer inside its mover.
    Moving(Mover),
}

impl Default for Body {
    fn default() -> Self {
        Self::Static(Static::default())
    }
}

impl Body {
    /// The layers of an object at `position` with `animated` and `mover` (each None where its
    /// type has none): a mover holds the animated layer (`animated`, else the mover's own), else
    /// the animated layer stands alone, else the object is static. The lowest layer takes the
    /// place.
    #[must_use]
    pub fn new(position: Position, animated: Option<Animated>, mover: Option<Mover>) -> Self {
        let base = Static::at(position);
        match (animated, mover) {
            (a, Some(mut m)) => {
                if let Some(a) = a {
                    m.animated = a;
                }
                m.animated.base = base;
                Self::Moving(m)
            }
            (Some(mut a), None) => {
                a.base = base;
                Self::Animated(a)
            }
            (None, None) => Self::Static(base),
        }
    }

    /// A static object's layers at `position`.
    #[must_use]
    pub const fn at(position: Position) -> Self {
        Self::Static(Static::at(position))
    }

    /// The static layer: the object's place.
    #[must_use]
    pub const fn base(&self) -> &Static {
        match self {
            Self::Static(s) | Self::Animated(Animated { base: s, .. }) => s,
            Self::Moving(m) => m.animated.base(),
        }
    }

    /// The static layer, to move the object.
    pub(crate) const fn base_mut(&mut self) -> &mut Static {
        match self {
            Self::Static(s) | Self::Animated(Animated { base: s, .. }) => s,
            Self::Moving(m) => m.animated.base_mut(),
        }
    }

    /// x, y, z on the map.
    #[must_use]
    pub const fn position(&self) -> Position {
        self.base().position()
    }

    /// The object's place setter ([`Static::new_position`]); a moving object's shape goes from
    /// the mesh while it moves (slot 185, [`Mover::is_moving`]).
    pub(crate) fn new_position(
        &mut self,
        id: ObjectId,
        at: Position,
        places: (&mut Spatial, Option<&mut Pathing>),
    ) -> Result<(), GenieError> {
        let moving = self.mover().is_some_and(Mover::is_moving);
        self.base_mut().new_position(id, at, moving, places)?;
        Ok(())
    }

    /// The moving layer, if the object moves.
    #[must_use]
    pub const fn mover(&self) -> Option<&Mover> {
        match self {
            Self::Moving(m) => Some(m),
            Self::Static(_) | Self::Animated(_) => None,
        }
    }

    /// The moving layer to change, if the object moves.
    pub const fn mover_mut(&mut self) -> Option<&mut Mover> {
        match self {
            Self::Moving(m) => Some(m),
            Self::Static(_) | Self::Animated(_) => None,
        }
    }

    /// The movement's values to change (its top speed among them), if the object moves.
    pub fn move_stats_mut(&mut self) -> Option<&mut crate::movement::MoveStats> {
        self.mover_mut().map(|m| &mut m.stats)
    }

    /// The animated layer, if the object has one: its own, or its mover's.
    #[must_use]
    pub const fn animated(&self) -> Option<&Animated> {
        match self {
            Self::Animated(a) | Self::Moving(Mover { animated: a, .. }) => Some(a),
            Self::Static(_) => None,
        }
    }

    /// The animated layer to change, if the object has one.
    pub const fn animated_mut(&mut self) -> Option<&mut Animated> {
        match self {
            Self::Animated(a) | Self::Moving(Mover { animated: a, .. }) => Some(a),
            Self::Static(_) => None,
        }
    }

    /// The animation speed (`+0x7b8`), None for a static object.
    #[must_use]
    pub fn anim_speed(&self) -> Option<Speed> {
        self.animated().map(|a| a.speed)
    }

    /// Sets the animation speed (`+0x7b8`) of an object that has one; a static object has none.
    pub const fn set_anim_speed(&mut self, speed: Speed) {
        if let Some(a) = self.animated_mut() {
            a.speed = speed;
        }
    }

    /// The object's mover goes and its animated layer, with the place, stays (a caller that
    /// works on the mover apart from its object puts it back with [`Body::put_mover`]).
    pub fn take_mover(&mut self) -> Option<Mover> {
        match std::mem::take(self) {
            Self::Moving(m) => {
                *self = Self::Animated(m.animated);
                Some(m)
            }
            other => {
                *self = other;
                None
            }
        }
    }

    /// A mover taken with [`Body::take_mover`] comes back, with the animated layer and the place
    /// as they are now (a static body takes the mover's own animated layer).
    pub fn put_mover(&mut self, mover: Option<Mover>) {
        let animated = self.animated().copied();
        *self = Self::new(self.position(), animated, mover);
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
    fn a_body_holds_the_layers_its_type_has() {
        let at = Position::new(1.5, 2.5, 0.0);
        let a = Animated::new(Speed::new(0.5));
        assert_eq!(Body::new(at, None, None), Body::at(at));
        assert_eq!(
            Body::new(at, Some(a), None).anim_speed(),
            Some(Speed::new(0.5))
        );
        let moving = Body::new(at, Some(a), Some(mover()));
        assert_eq!(
            moving.mover().map(|m| m.animated.speed),
            Some(Speed::new(0.5))
        );
        assert_eq!(moving.anim_speed(), Some(Speed::new(0.5)));
        let mut own = mover();
        own.animated.speed = Speed::new(0.25);
        assert_eq!(
            Body::new(at, None, Some(own)).anim_speed(),
            Some(Speed::new(0.25)),
            "a mover always has an animated layer, its own without another"
        );
    }

    #[test]
    fn every_layer_holds_the_place_at_its_bottom() {
        let at = Position::new(1.5, 2.5, 0.25);
        for body in [
            Body::at(at),
            Body::new(at, Some(Animated::new(Speed::new(1.0))), None),
            Body::new(at, None, Some(mover())),
        ] {
            assert_eq!(body.position(), at, "{body:?}");
            assert_eq!(body.base().position(), at);
        }
        let moving = Body::new(at, None, Some(mover()));
        assert_eq!(moving.mover().map(Mover::position), Some(at));
    }

    #[test]
    fn only_a_static_object_has_no_animation_speed() {
        let mut still = Body::default();
        still.set_anim_speed(Speed::new(1.0));
        assert_eq!((still.anim_speed(), still.mover()), (None, None));
        let mut flag = Body::Animated(Animated::default());
        flag.set_anim_speed(Speed::new(1.0));
        assert_eq!(
            (flag.anim_speed(), flag.mover()),
            (Some(Speed::new(1.0)), None)
        );
        let mut walker = Body::new(Position::ORIGIN, None, Some(mover()));
        walker.set_anim_speed(Speed::new(2.0));
        assert_eq!(
            walker.mover_mut().map(|m| m.animated.speed),
            Some(Speed::new(2.0))
        );
        assert_eq!(
            walker.animated_mut().map(|a| a.speed),
            Some(Speed::new(2.0))
        );
    }

    #[test]
    fn a_taken_mover_leaves_the_animated_layer_and_comes_back_with_it() {
        let at = Position::new(3.0, 4.0, 0.0);
        let mut body = Body::new(at, Some(Animated::new(Speed::new(1.0))), Some(mover()));
        let taken = body.take_mover();
        assert_eq!(
            body,
            Body::new(at, Some(Animated::new(Speed::new(1.0))), None)
        );
        body.set_anim_speed(Speed::ZERO);
        body.put_mover(taken);
        assert_eq!(body.mover().map(|m| m.animated.speed), Some(Speed::ZERO));
        assert_eq!(body.position(), at, "the place stays the body's");
        let mut still = Body::at(at);
        assert_eq!(still.take_mover(), None);
        assert_eq!(still, Body::at(at));
    }

    /// FD-045-01M62GVS08: one place setter writes the body and the spatial index.
    #[test]
    fn the_place_setter_moves_the_object_and_its_index_together() {
        let mut world = indexed(Position::new(1.5, 1.5, 0.0));
        let mut body = Body::new(Position::new(1.5, 1.5, 0.0), None, Some(mover()));
        body.new_position(
            object_id(7),
            Position::new(2.5, 1.25, 0.0),
            (&mut world, None),
        )
        .unwrap();
        assert_eq!(body.position(), Position::new(2.5, 1.25, 0.0));
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
