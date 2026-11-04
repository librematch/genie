//! An object as one value (FD-045-D51): its row and its parts, as the world takes it in and
//! gives it back whole, and the view a comparison reads ([`ObjectView`]).

use crate::combat::missile::Missile;
use crate::commands::GatherPoint;
use crate::movement::Mover;
use crate::objects::layers::Action;
use crate::objects::layers::Building;
use crate::objects::layers::Combat;
use crate::objects::layers::Motion;
use crate::objects::masters::Master;
use crate::objects::unit::Unit;
use crate::orders::town_bell::TownBell;
use crate::production::Production;
use crate::unit_ai::UnitAi;
use crate::world::Unread;
#[cfg(doc)]
use crate::world::{World, parts::Parts};
use genie_diff::{Diff, Differ};
use std::borrow::Cow;

/// One object's parts as a value, outside the world's tables: what it enters the world with and
/// what a removal for good returns. Each part compares as the field it was on the object.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct ObjectParts {
    /// Its dead master ([`Parts::dead_master`]).
    #[diff(gap = "the import does not read a saved dead master")]
    pub dead_master: Option<Master>,
    /// Its gather point ([`Parts::gather_point`]).
    #[diff(path = "building.gather_point")]
    pub gather_point: Option<GatherPoint>,
    /// Its production lists ([`Parts::production`]).
    #[diff(path = "building.production")]
    pub production: Option<Production>,
    /// Its missile's own fields ([`Parts::missile`]).
    #[diff(path = "missile")]
    pub missile: Option<Missile>,
    /// Its own master ([`Parts::own_master`]).
    #[diff(
        gap = "the import does not read a saved own master; genie makes none for a foundation or a dying annex"
    )]
    pub own_master: Option<Master>,
    /// Its town bell ([`Parts::town_bell`]), a new unit's where the table holds none.
    #[diff(path = "combat.town_bell")]
    pub town_bell: TownBell,
    /// Its unit AI ([`Parts::unit_ai`]).
    #[diff(path = "combat.ai")]
    pub unit_ai: Option<UnitAi>,
    /// Its building layer ([`Parts::building`]), [`Building::NONE`] where the table holds none.
    #[diff(path = "building")]
    pub building: Building,
    /// Its combat layer ([`Parts::combat`]), a new object's where the table holds none.
    #[diff(path = "combat")]
    pub combat: Combat,
    /// Its animated and moving layers ([`Parts::motion`]), none for a static object. They
    /// compare at their old step, `body`, beside the row's place.
    #[diff(path = "body")]
    pub motion: Option<Motion>,
    /// Its action layer ([`Parts::actions`]): its action list and its master's default task, an
    /// empty one's where the table holds none.
    #[diff(path = "actions")]
    pub actions: Action,
    /// Why the load could not read it whole ([`Parts::unread`]): not game data, the import's
    /// gap, compared only between two runs.
    #[diff(unsaved, eq)]
    pub unread: Option<Unread>,
}

impl ObjectParts {
    /// Its mover (the motion part's moving layer), none for an object that does not move.
    #[must_use]
    pub fn mover(&self) -> Option<&Mover> {
        self.motion.as_ref().and_then(Motion::mover)
    }
}

/// An object as one value: its row ([`Unit`], what the lists hold) and its parts. The world takes
/// an object in and gives it back whole; a [`Unit`] alone is an object without parts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Object {
    /// Its row.
    pub unit: Unit,
    /// Its parts.
    pub parts: ObjectParts,
}

impl Diff for Object {
    /// As its view ([`ObjectView`]).
    fn diff(&self, other: &Self, differ: &mut Differ) {
        self.view().diff(&other.view(), differ);
    }
}

impl Object {
    /// The object as a comparison reads it, its parts its own.
    #[must_use]
    pub fn view(&self) -> ObjectView<'_> {
        self.into()
    }
}

/// An object as a comparison reads it: its row and its parts, the parts lent by an [`Object`]
/// (the game's save, imported) or copied out of the world's tables ([`World::object_view`]).
#[derive(Clone, Debug)]
pub struct ObjectView<'a> {
    /// Its row.
    pub unit: &'a Unit,
    /// Its parts.
    pub parts: Cow<'a, ObjectParts>,
}

impl Diff for ObjectView<'_> {
    /// The row's fields, then the parts', each at the path it had on the object.
    fn diff(&self, other: &Self, differ: &mut Differ) {
        self.unit.diff(other.unit, differ);
        self.parts.as_ref().diff(other.parts.as_ref(), differ);
    }
}

impl<'a> From<&'a Object> for ObjectView<'a> {
    fn from(object: &'a Object) -> Self {
        ObjectView {
            unit: &object.unit,
            parts: Cow::Borrowed(&object.parts),
        }
    }
}

impl From<Unit> for Object {
    fn from(unit: Unit) -> Self {
        Self {
            unit,
            parts: ObjectParts::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    use crate::gathering::hunt::tests::walker;
    use crate::objects::layers::{Animated, Motion};
    use crate::world::parts::ObjectParts;

    #[test]
    fn only_moving_object_parts_hold_mover() {
        let walker = walker();
        let moving = ObjectParts {
            motion: Some(Motion::Moving(walker.clone())),
            ..ObjectParts::default()
        };
        assert_eq!(moving.mover(), Some(&walker));
        let animated = ObjectParts {
            motion: Some(Motion::Animated(Animated::default())),
            ..ObjectParts::default()
        };
        assert_eq!(animated.mover(), None);
        assert_eq!(ObjectParts::default().mover(), None, "a static object");
    }
}
