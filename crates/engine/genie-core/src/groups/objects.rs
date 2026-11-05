//! An object of the world's id table ([`WorldObject`]) as the groups read it, and a unit's
//! sub-formation.

use crate::groups::FormationCategory;
use crate::map::Position;
use crate::math::fmath::BVector;
use crate::movement::Mover;
use crate::objects::ObjectState;
use crate::objects::masters::FormationClass;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::unit::Unit;
use crate::objects::{ObjectId, ObjectKey};
use crate::players::PlayerId;
use crate::vision::ghost::Ghost;
use crate::world::World;
use crate::world::index::ObjectIndex;
use crate::world::layer::Slots;
use crate::world::parts::Parts;
use crate::world::player::Player;

/// An object of the world's id table (`FUN_1415417a0`, `BaseWorld::object`): the objects of
/// every player's live, sleeping and doppelganger lists, which the player's add and removal
/// (`FUN_141668040`) keep in the table through the world's slots 16 and 17 (`FUN_14152cb60`,
/// `FUN_141546a90`). An object of a recyclable master (a missile, a ghost) has a negative id and
/// sits in the table's negative half (`world+0x178`, at the id's negation); one recycled out of
/// the game (`RGE_Static_Object::recycle_out_of_game`) leaves its player's list, so the table.
#[derive(Clone, Copy, Debug)]
pub(crate) enum WorldObject<'world> {
    /// An object of player `.0`'s live or sleeping list, with its mover (none for one that does
    /// not move).
    Unit(PlayerId, &'world Unit, Option<&'world Mover>),
    /// A ghost of player `.0`'s doppelganger list (type 25, its master `243`).
    Ghost(PlayerId, &'world Ghost),
}

impl WorldObject<'_> {
    /// The player whose list holds it (`+0x138`).
    pub(crate) const fn player(&self) -> PlayerId {
        match *self {
            Self::Unit(player_id, _, _) | Self::Ghost(player_id, _) => player_id,
        }
    }

    /// Its master's id (`+0xa0`'s): a ghost's is the build's `doppelganger`.
    pub(crate) const fn master_id(&self, slots: Slots) -> MasterId {
        match *self {
            Self::Unit(_, unit, _) => unit.life.master_id,
            Self::Ghost(..) => slots.master.doppelganger,
        }
    }

    /// Its id (`+0x8`).
    pub(crate) const fn id(&self) -> ObjectId {
        match *self {
            Self::Unit(_, unit, _) => unit.id,
            Self::Ghost(_, ghost) => ghost.id,
        }
    }

    /// Its state (`+0x640`): a ghost's own (2, then 7 and 8 once destroyed).
    pub(crate) const fn state(&self) -> ObjectState {
        match *self {
            Self::Unit(_, unit, _) => unit.life.state,
            Self::Ghost(_, ghost) => ghost.state,
        }
    }

    /// It is inside another object (`+0x2f0`, `FUN_141188da0`); a ghost never is.
    pub(crate) const fn inside(&self) -> bool {
        match *self {
            Self::Unit(_, unit, _) => unit.life.inside.is_some(),
            Self::Ghost(..) => false,
        }
    }

    /// Its place: x, y, z (`+0x3c8`, `+0x4c8`, `+0x5a8`).
    pub(crate) const fn position(&self) -> Position {
        match *self {
            Self::Unit(_, unit, _) => unit.position(),
            Self::Ghost(_, ghost) => ghost.position,
        }
    }

    /// Whether it is a moving object (`RGE_Moving_Object` and its subclasses, types 30 and up,
    /// whose movement state the slice holds): their slot 0x270 is `FUN_141158940`, a static,
    /// animated or doppelganger object's `FUN_141071e30` (0).
    pub(crate) const fn moves(&self) -> bool {
        match *self {
            Self::Unit(_, _, mover) => mover.is_some(),
            Self::Ghost(..) => false,
        }
    }

    /// Slot 0x648, its forward: a moving object's `+0x830` (`FUN_14106d500`), a static,
    /// animated or doppelganger object's (0, 0, 0) (`FUN_141071e10`).
    pub(crate) fn forward(&self) -> BVector {
        match *self {
            Self::Unit(_, _, moves) => moves.map_or(BVector::ZERO, |mover| mover.forward),
            Self::Ghost(..) => BVector::ZERO,
        }
    }

    /// Slot 0x520, where it last moved to as the game's vector (x, height, y): a unit's
    /// ([`crate::objects::unit::Unit::moved_vector`]), a ghost's place (`FUN_141196c10`).
    pub(crate) fn moved_vector(&self) -> BVector {
        match *self {
            Self::Unit(_, unit, mover) => unit.moved_vector(mover),
            Self::Ghost(_, ghost) => ghost.position.vector(),
        }
    }
}

impl World {
    /// `FUN_1415417a0` of `id`: the object the world's table holds for it ([`WorldObject`]),
    /// none for `i32::MIN` (the routine's own test, [`ObjectId::TAKEN_OUT`]'s value) and for an
    /// id no list holds. Only the negative half holds ghosts.
    pub(crate) fn world_object(&self, id: impl ObjectKey) -> Option<WorldObject<'_>> {
        world_object_in(&self.players, &self.index, &self.parts, id)
    }
}

/// [`World::world_object`] of the world's `players`, its table by id `index` and its `parts`
/// (the movers), for a caller that holds other tables of the world.
pub(crate) fn world_object_in<'world>(
    players: &'world [Player],
    index: &'world ObjectIndex,
    parts: &'world Parts,
    id: impl ObjectKey,
) -> Option<WorldObject<'world>> {
    let id = id.key()?;
    if id == ObjectId::TAKEN_OUT {
        return None;
    }
    if let Some((player_id, unit, _)) = index.locate(players, id) {
        return Some(WorldObject::Unit(player_id, unit, parts.mover_of(unit.id)));
    }
    if id.get() < 0 {
        for (player_id, player) in PlayerId::each(players.len()).zip(players) {
            if let Some(ghost) = player
                .ghosts
                .iter()
                .find(|ghost| ghost.id.key() == Some(id))
            {
                return Some(WorldObject::Ghost(player_id, ghost));
            }
        }
    }
    None
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
    formation_category: FormationClass,
) -> Option<FormationCategory> {
    if kind < MasterKind::COMBAT {
        return None;
    }
    if kind == MasterKind::COMBAT && hero_flag.grants(HeroFlags::PROTECTED_FORMATION) {
        return Some(FormationCategory::PROTECTED);
    }
    match formation_category.get() {
        0 | 5 => Some(FormationCategory::PROTECTED),
        category @ 1..=4 => Some(FormationCategory::named(i32::from(category))),
        6 => Some(FormationCategory::named(0)),
        _ => None,
    }
}
