//! An object of the world's id table ([`WorldObject`]) as the groups read it, and a unit's
//! sub-formation.

use crate::groups::FormationCategory;
use crate::map::Position;
use crate::math::fmath::BVector;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::{ObjectId, ObjectKey};
use crate::players::PlayerId;

/// An object of the world's id table (`FUN_1415417a0`, `BaseWorld::object`): the objects of
/// every player's live, sleeping and doppelganger lists, which the player's add and removal
/// (`FUN_141668040`) keep in the table through the world's slots 16 and 17 (`FUN_14152cb60`,
/// `FUN_141546a90`). An object of a recyclable master (a missile, a ghost) has a negative id and
/// sits in the table's negative half (`world+0x178`, at the id's negation); one recycled out of
/// the game (`RGE_Static_Object::recycle_out_of_game`) leaves its player's list, so the table.
#[derive(Clone, Copy, Debug)]
pub(crate) enum WorldObject<'a> {
    /// An object of player `.0`'s live or sleeping list.
    Unit(PlayerId, &'a crate::objects::unit::Unit),
    /// A ghost of player `.0`'s doppelganger list (type 25, its master `243`).
    Ghost(PlayerId, &'a crate::vision::ghost::Ghost),
}

impl WorldObject<'_> {
    /// The player whose list holds it (`+0x138`).
    pub(crate) const fn player(&self) -> PlayerId {
        match *self {
            Self::Unit(p, _) | Self::Ghost(p, _) => p,
        }
    }

    /// Its master's id (`+0xa0`'s): a ghost's is its player's [`crate::vision::ghost::GHOST_MASTER`].
    pub(crate) const fn master_id(&self) -> MasterId {
        match *self {
            Self::Unit(_, u) => u.life.master_id,
            Self::Ghost(..) => crate::vision::ghost::GHOST_MASTER,
        }
    }

    /// Its id (`+0x8`).
    pub(crate) const fn id(&self) -> ObjectId {
        match *self {
            Self::Unit(_, u) => u.id,
            Self::Ghost(_, g) => g.id,
        }
    }

    /// Its state (`+0x640`): a ghost's own (2, then 7 and 8 once destroyed).
    pub(crate) const fn state(&self) -> crate::objects::ObjectState {
        match *self {
            Self::Unit(_, u) => u.life.state,
            Self::Ghost(_, g) => g.state,
        }
    }

    /// It is inside another object (`+0x2f0`, `FUN_141188da0`); a ghost never is.
    pub(crate) const fn inside(&self) -> bool {
        match *self {
            Self::Unit(_, u) => u.life.inside.is_some(),
            Self::Ghost(..) => false,
        }
    }

    /// Its place: x, y, z (`+0x3c8`, `+0x4c8`, `+0x5a8`).
    pub(crate) const fn position(&self) -> Position {
        match *self {
            Self::Unit(_, u) => u.position(),
            Self::Ghost(_, g) => g.position,
        }
    }

    /// Whether it is a moving object (`RGE_Moving_Object` and its subclasses, types 30 and up,
    /// whose movement state the slice holds): their slot 0x270 is `FUN_141158940`, a static,
    /// animated or doppelganger object's `FUN_141071e30` (0).
    pub(crate) const fn moves(&self) -> bool {
        match *self {
            Self::Unit(_, u) => u.body.mover().is_some(),
            Self::Ghost(..) => false,
        }
    }

    /// Slot 0x648, its forward: a moving object's `+0x830` (`FUN_14106d500`), a static,
    /// animated or doppelganger object's (0, 0, 0) (`FUN_141071e10`).
    pub(crate) fn forward(&self) -> BVector {
        match *self {
            Self::Unit(_, u) => u.body.mover().map_or(BVector::ZERO, |m| m.forward),
            Self::Ghost(..) => BVector::ZERO,
        }
    }

    /// Slot 0x520, where it last moved to as the game's vector (x, height, y): a unit's
    /// ([`crate::objects::unit::Unit::moved_vector`]), a ghost's place (`FUN_141196c10`).
    pub(crate) fn moved_vector(&self) -> BVector {
        match *self {
            Self::Unit(_, u) => u.moved_vector(),
            Self::Ghost(_, g) => g.position.vector(),
        }
    }
}

impl crate::world::World {
    /// `FUN_1415417a0` of `id`: the object the world's table holds for it ([`WorldObject`]),
    /// none for `i32::MIN` (the routine's own test, [`ObjectId::TAKEN_OUT`]'s value) and for an
    /// id no list holds. Only the negative half holds ghosts.
    pub(crate) fn world_object(&self, id: impl ObjectKey) -> Option<WorldObject<'_>> {
        let id = id.key()?;
        if id == ObjectId::TAKEN_OUT {
            return None;
        }
        if let Some((p, u)) = self.object(id) {
            return Some(WorldObject::Unit(p, u));
        }
        if id.get() < 0 {
            for (p, player) in PlayerId::each(self.players.len()).zip(&self.players) {
                if let Some(g) = player.ghosts.iter().find(|g| g.id.key() == Some(id)) {
                    return Some(WorldObject::Ghost(p, g));
                }
            }
        }
        None
    }
}

/// A unit's sub-formation in its group (`FUN_141011300`, `World::unit_category` for a held object),
/// from its master's type, `hero_flag` (`+0x8d9`) and formation category (`+0x8bc`): none below
/// type 70 (unsigned); a combat master (70) whose hero flag grants
/// [`HeroFlags::PROTECTED_FORMATION`] is 5; categories 1 to 4 are themselves, 0 and 5 are 5, 6 is
/// 0, any other (255) none ("Invalid formation category").
#[must_use]
pub fn sub_formation(
    kind: MasterKind,
    hero_flag: HeroFlags,
    formation_category: u8,
) -> Option<FormationCategory> {
    if kind < COMBAT {
        return None;
    }
    if kind == COMBAT && hero_flag.grants(HeroFlags::PROTECTED_FORMATION) {
        return Some(FormationCategory::PROTECTED);
    }
    match formation_category {
        0 | 5 => Some(FormationCategory::PROTECTED),
        c @ 1..=4 => Some(FormationCategory::named(i32::from(c))),
        6 => Some(FormationCategory::named(0)),
        _ => None,
    }
}
