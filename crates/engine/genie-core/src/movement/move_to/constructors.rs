//! The quiet sub-action `MoveTo`s an action makes and adds to its sub-action list: what their
//! constructors read of the unit ([`MoveToReads`]) and the object one walks to ([`WalkTo`]); to an
//! object (`FUN_140aa0df0`), to a point (`FUN_140aa0d30`) and to none.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::footprint_gap;
use crate::movement::move_to::{
    MoveTo, MoveToAction, MoveWait, NO_TARGET_GAP, SpriteReads, TargetMoved,
};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::masters::Master;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::World;

/// The last object state a `MoveTo` takes as its target (`FUN_140a7a230`: one above is refused,
/// its id −1).
const LAST_TAKEN: ObjectState = ObjectState::Undead;

/// The point a `MoveTo` without a target walks to (`FUN_140aa0df0` leaves its zeroed `+0x38`).
const NO_TARGET_POINT: Position = Position::ORIGIN;

/// What a new sub-action `MoveTo` reads of its unit: the sprites its constructor takes for none
/// (the defaults of the unit's master and terrain look, and the sprites' table), the unit's
/// object state (`+0x640`: its action list adds a sub-action only below 3, `FUN_140a977e0`), its
/// speed (slot 0x208: without one the `MoveTo` starts failed, `0x140aa2100`), and its place and
/// its master's radii, from which the gap to its target is measured (`FUN_14118f680`).
#[derive(Clone, Copy, Debug)]
pub struct MoveToReads<'a> {
    /// The sprites a `MoveTo` takes when its maker gives none.
    pub sprites: SpriteReads<'a>,
    /// The unit's object state.
    pub state: ObjectState,
    /// The unit's speed.
    pub speed: Speed,
    /// The unit's place.
    pub at: Position,
    /// Its master's x and y radii (`+0x5d8`, `+0x5dc`).
    pub ground: Footprint,
}

/// An object a `MoveTo` walks to: its id, its place, the unit's footprint gap to it
/// (`FUN_14118f680`) and its object state (`+0x640`), None where the world does not hold it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkTo {
    /// Its id, none for a `MoveTo` that holds no object ([`WalkTo::unheld`]).
    pub id: Option<ObjectId>,
    /// Its place, the `MoveTo`'s point (`+0x38`).
    pub at: Position,
    /// The unit's footprint gap to it.
    pub gap: Distance,
    /// Its object state.
    pub state: Option<ObjectState>,
}

impl WalkTo {
    /// A `MoveTo` that holds no object but walks to `at` as to a point that is far (a livestock
    /// `MoveTo` to a town centre, `FUN_140aa0df0`): no target, [`NO_TARGET_GAP`].
    #[must_use]
    pub const fn unheld(at: Position) -> Self {
        Self {
            id: None,
            at,
            gap: Distance::new(NO_TARGET_GAP),
            state: None,
        }
    }
}

impl WalkTo {
    /// The target the `MoveTo` takes (slot 0x68, `FUN_140a7a230`) and the gap its start measures:
    /// an object in a state above [`LAST_TAKEN`] is refused (the id -1) and far
    /// ([`NO_TARGET_GAP`]).
    fn held(&self) -> (Option<ObjectId>, f32) {
        match self.state {
            Some(s) if LAST_TAKEN < s => (None, NO_TARGET_GAP),
            _ => (self.id, self.gap.get()),
        }
    }
}

impl<'g> MoveToReads<'g> {
    /// What a new `MoveTo` reads of `unit` with its mover `mover` (its motion layer's) of
    /// `master` (its own, or the one its task switches it to), with the sprites' table
    /// `graphics` and the unit's terrain look `look` (its combat layer's): an object without
    /// movement has no speed (a static object's slot 0x208).
    #[must_use]
    pub fn of(
        (unit, mover): (&Unit, Option<&Mover>),
        master: &Master,
        (graphics, look): (
            &'g crate::objects::Graphics,
            Option<&crate::combat::abilities::Look>,
        ),
    ) -> Self {
        Self {
            sprites: SpriteReads::of(master, look, graphics),
            state: unit.life.state,
            speed: mover.map_or(Speed::ZERO, |m| m.stats.speed),
            at: unit.position(),
            ground: master.radius,
        }
    }

    /// What a new `MoveTo` reads of `unit` with its mover `mover` and the sprites `sprites`:
    /// its state and speed, its place and the radii of its movement state (none for a unit that
    /// does not move).
    #[must_use]
    pub fn with_sprites(sprites: SpriteReads<'g>, (unit, mover): (&Unit, Option<&Mover>)) -> Self {
        Self {
            sprites,
            state: unit.life.state,
            speed: mover.map_or(Speed::ZERO, |m| m.stats.speed),
            at: unit.position(),
            ground: mover.map_or(Footprint::ZERO, |m| m.stats.radius.ground()),
        }
    }

    /// The object `id`, live or sleeping, as a `MoveTo` of this unit walks to it ([`WalkTo`]: its
    /// place, its state and the gap between their footprints by their masters' radii,
    /// `FUN_14118f680`), None where the world does not hold it.
    ///
    /// # Errors
    ///
    /// An object whose master its player's table lacks.
    pub fn walk_to(&self, w: &World, id: Option<ObjectId>) -> Result<Option<WalkTo>, GenieError> {
        let Some((player, t)) = w.object(id) else {
            return Ok(None);
        };
        let at = t.position();
        let radius = t.master(w.master_tables(), player)?.radius;
        Ok(Some(WalkTo {
            id,
            at,
            gap: footprint_gap(self.at, self.ground, at, radius),
            state: Some(t.life.state),
        }))
    }

    /// A quiet `MoveTo` to the object `to` within `range` (`FUN_140aa0df0` with 1 and 1, then
    /// `FUN_140a7a170` with 1): its point the object's place, the sprite `given` (else the
    /// constructor's default, [`SpriteReads::to_object`]) and `on_target_moved` (`+0x61`), as
    /// the list adds it ([`MoveToAction::added`]). An object in a state above [`LAST_TAKEN`] is
    /// not its target (`FUN_140a7a230`: the id −1, the point stays) and is far
    /// ([`NO_TARGET_GAP`]).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn to_object(
        &self,
        to: WalkTo,
        range: Distance,
        given: Option<SpriteId>,
        on_target_moved: TargetMoved,
    ) -> Result<Option<MoveToAction>, GenieError> {
        let (target, gap) = to.held();
        let walk = MoveToAction {
            state: MoveTo::Start,
            point: to.at,
            target,
            range,
            sprite: self.sprites.to_object(given)?,
            quiet: true,
            on_target_moved,
            wait: MoveWait::NEW,
        };
        Ok(walk.added(self.state, Distance::new(gap), self.speed))
    }

    /// A quiet `MoveTo` to `point` within `range` (`FUN_140aa0d30`, `FUN_140a7a170` with 1) that
    /// then takes the object `to` as its target (slot 0x68, which leaves the point) with
    /// `on_target_moved`, as the list adds it: the gap its start measures is the object's
    /// ([`WalkTo::gap`]).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn to_point_of(
        &self,
        to: WalkTo,
        (point, range): (Position, f32),
        given: Option<SpriteId>,
        on_target_moved: TargetMoved,
    ) -> Result<Option<MoveToAction>, GenieError> {
        let (target, gap) = to.held();
        let walk = MoveToAction {
            target,
            on_target_moved,
            ..MoveToAction::sub_to_point(point, Distance::new(range), self.sprites.to_point(given)?)
        };
        Ok(walk.added(self.state, Distance::new(gap), self.speed))
    }

    /// `FUN_140aa0df0` without a target: no target, its zeroed point ([`NO_TARGET_POINT`]), far
    /// ([`NO_TARGET_GAP`]); else as [`MoveToReads::to_object`].
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn to_nothing(
        &self,
        range: Distance,
        given: Option<SpriteId>,
    ) -> Result<Option<MoveToAction>, GenieError> {
        let walk = MoveToAction {
            state: MoveTo::Start,
            point: NO_TARGET_POINT,
            target: None,
            range,
            sprite: self.sprites.to_object(given)?,
            quiet: true,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::NEW,
        };
        Ok(walk.added(self.state, Distance::new(NO_TARGET_GAP), self.speed))
    }

    /// A quiet `MoveTo` to `point` within `range` (`FUN_140aa0d30`, then `FUN_140a7a170` with 1)
    /// with the sprite `given` (else the constructor's default, [`SpriteReads::to_point`]), as
    /// the list adds it: far ([`NO_TARGET_GAP`]).
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn to_point(
        &self,
        point: Position,
        range: Distance,
        given: Option<SpriteId>,
    ) -> Result<Option<MoveToAction>, GenieError> {
        let walk = MoveToAction::sub_to_point(point, range, self.sprites.to_point(given)?);
        Ok(walk.added(self.state, Distance::new(NO_TARGET_GAP), self.speed))
    }
}

impl Unit {
    /// What a new `MoveTo` reads of this unit with its mover `mover` ([`MoveToReads::of`] with
    /// its master), `player`'s, whose masters are `every`, with the sprites' table `graphics`
    /// and its terrain look `look` (its combat layer's).
    ///
    /// # Errors
    ///
    /// A unit whose master the table lacks.
    pub(crate) fn move_to_reads<'g>(
        &self,
        mover: Option<&Mover>,
        every: crate::objects::masters::MasterTables<'_>,
        player: PlayerId,
        graphics: (
            &'g crate::objects::Graphics,
            Option<&crate::combat::abilities::Look>,
        ),
    ) -> Result<MoveToReads<'g>, GenieError> {
        Ok(MoveToReads::of(
            (self, mover),
            self.master(every, player)?,
            graphics,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::move_to::MoveDefaults;
    use crate::test_ids::object_id;

    /// The sub-action `MoveTo`s (`FUN_140aa0df0`, `FUN_140aa0d30`, then `FUN_140a977e0`): one to an
    /// object takes its maker's sprite, else the object default; one to a point the point default;
    /// each is quiet and new. Within 0.01 of its target it arrived, else it starts (13 without
    /// speed); a target above state 6 is refused (far, the point stays its place); one without a
    /// target walks to (0, 0, 0); a unit in state 3 or later adds none; a sprite the table lacks
    /// is an error.
    #[test]
    fn a_sub_move_to_takes_its_sprite_target_and_start() {
        let graphics = crate::construction::build::tests::graphics();
        let move_reads = MoveToReads {
            sprites: SpriteReads {
                defaults: MoveDefaults {
                    point: Some(SpriteId::named(20)),
                    object: Some(SpriteId::named(21)),
                },
                graphics: &graphics,
            },
            state: ObjectState::Alive,
            speed: Speed::new(1.0),
            at: Position::new(10.0, 10.0, 0.0),
            ground: Footprint::new(0.2, 0.2),
        };
        let house = WalkTo {
            id: Some(object_id(10028)),
            at: Position::new(10.5, 10.5, 0.0),
            gap: Distance::new(5.0),
            state: Some(ObjectState::Foundation),
        };
        let to = |w: &MoveToReads<'_>, to, given| {
            w.to_object(to, Distance::new(0.1), given, TargetMoved::Follows)
                .unwrap()
                .unwrap()
        };
        let m = to(&move_reads, house, None);
        assert_eq!(
            (m.state, ObjectId::field(m.target), m.point, m.range),
            (
                MoveTo::Start,
                10028,
                Position::new(10.5, 10.5, 0.0),
                Distance::new(0.1)
            )
        );
        assert_eq!(
            (m.sprite.map(|g| g.id), m.quiet, m.wait),
            (Some(SpriteId::named(21)), true, MoveWait::NEW)
        );
        assert_eq!(
            to(&move_reads, house, Some(SpriteId::named(3)))
                .sprite
                .map(|g| g.id),
            Some(SpriteId::named(3))
        );
        let near = WalkTo {
            gap: Distance::new(0.01),
            ..house
        };
        assert_eq!(to(&move_reads, near, None).state, MoveTo::Arrived);
        let gone = WalkTo {
            gap: Distance::ZERO,
            state: Some(ObjectState::Removed),
            ..house
        };
        let refused = to(&move_reads, gone, None);
        assert_eq!(
            (
                refused.state,
                ObjectId::field(refused.target),
                refused.point
            ),
            (MoveTo::Start, -1, house.at)
        );
        let stuck = MoveToReads {
            speed: Speed::ZERO,
            ..move_reads
        };
        assert_eq!(to(&stuck, house, None).state, MoveTo::Failed);
        let nothing = move_reads
            .to_nothing(Distance::new(0.1), None)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                nothing.state,
                ObjectId::field(nothing.target),
                nothing.point
            ),
            (MoveTo::Start, -1, Position::ORIGIN)
        );
        let point = move_reads
            .to_point(Position::new(4.0, 5.0, 0.0), Distance::ZERO, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                point.state,
                ObjectId::field(point.target),
                point.range,
                point.sprite.map(|g| g.id)
            ),
            (MoveTo::Start, -1, Distance::ZERO, Some(SpriteId::named(20)))
        );
        let dying = MoveToReads {
            state: ObjectState::Dying,
            ..move_reads
        };
        assert_eq!(
            dying.to_object(house, Distance::new(0.1), None, TargetMoved::Follows),
            Ok(None)
        );
        assert_eq!(
            dying.to_point(Position::new(4.0, 5.0, 0.0), Distance::ZERO, None),
            Ok(None)
        );
        assert!(
            move_reads
                .to_point(
                    Position::new(4.0, 5.0, 0.0),
                    Distance::ZERO,
                    Some(SpriteId::named(99))
                )
                .is_err()
        );
    }
}
