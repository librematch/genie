//! What the slice tracks of one object ([`Unit`]) and its update, action by action.

use crate::map::Position;
use crate::movement::MoveState;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::players::PlayerId;
use crate::world::WorldTime;
use genie_diff::Diff;

use crate::construction::FootprintWaits;
use crate::gathering::{DropSiteId, PastureId};
use crate::groups::GroupLink;
use crate::map::spatial::Spatial;
use crate::objects::layers::Static;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::{Master, MasterTables, Masters, OwnerMasters};
use crate::objects::sprite::ActiveSprite;
use crate::objects::{DamagedTimer, WorkerCount};
use crate::ownership::capture::Captured;
use crate::pathing::Pathing;
use crate::world::OBJECT_MASTER;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::VisibilityTuning;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

mod death;
mod default_task;
mod idle_time;
mod life_step;
mod stop;
mod top;
mod update;

/// A dying sprite the `.dat` does not have.
const NO_DYING_SPRITE: GenieError =
    GenieError::unsupported("a dying sprite the data file does not have");

bitflags::bitflags! {
    /// An object's flag bytes `+0x685` to `+0x68a` ([`Unit`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct UnitFlags: u8 {
        /// `+0x68a`, `unit_attackable_flag`.
        const ATTACKABLE = 1;
        /// `+0x689`, `mUnitTargetingFlag`, is 0 (a new object's is 1): a gatherer's search takes
        /// the object only through a pasture (`ClosestResourceObject`).
        const UNTARGETABLE = 1 << 1;
        /// `+0x685`, `underAttackValue`: its unit AI keeps attackers (`FUN_14119d2d0`).
        const UNDER_ATTACK = 1 << 2;
        /// `mDeletionEnabled` (`+0x687`, 1 from its constructor) is 0: no delete takes it.
        const UNDELETABLE = 1 << 3;
        /// `mROOAlreadyCalled` (`+0x686`, saved): its obstruction record went (its death's
        /// `FUN_14121cc40`, a footprint clearing's `FUN_141189670`, a construction start's clearing
        /// `FUN_1411cd0d0`, a stack unit's completion); later ones leave it alone.
        const ROO_CALLED = 1 << 4;
    }
}

impl Diff for UnitFlags {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

/// What the slice tracks of one object.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Unit {
    /// The object's id (`+0x8`), which names it across ticks.
    pub id: ObjectId,
    /// `+0x889`: its waits as a builder for an occupied footprint (not saved; 0 at a load).
    #[diff(unsaved)]
    pub footprint_waits: FootprintWaits,
    /// Its life cycle: state, hit points, master, what it holds, its dying sprite.
    pub life: Lifecycle,
    /// Its static layer, its place (every object, [`Static`]); the layers above it its type has
    /// are its motion, a part ([`crate::world::parts::Parts::motion`]). It compares at the step
    /// the object's layers had, `body`.
    #[diff(path = "body")]
    pub place: Static,
    /// Its flag bytes `+0x685` to `+0x68a`.
    pub flags: UnitFlags,
    /// `+0x790`, `parent_pasture_id`: the pasture it is in.
    pub pasture: Option<PastureId>,
    /// `+0x794`, `my_pasture_id`: the pasture it tends.
    pub my_pasture: Option<PastureId>,
    /// Its unit group, if any ([`crate::groups::GroupLink`]).
    pub group: GroupLink,
    /// The objects inside it (`+0x2f8`), in the order they came in (`FUN_141164af0`); one that
    /// leaves takes the last one's place (`FUN_141166450`).
    /// Not saved as a list: a load lists them by each one's `+0x2f0` in the objects' order.
    #[diff(unsaved)]
    pub garrison: Vec<ObjectId>,
    /// `damaged_lately_timer` (`+0x30d`, saved): 60 after a hit on a building (`FUN_1411cea90`),
    /// one off in each of its updates that passes whole seconds; the computer AI reads it.
    pub damaged_lately: DamagedTimer,
    /// Its active sprite, held from an attack's swing on; None while the slice does not know it.
    /// Its clock runs at the animated layer's speed ([`crate::world::parts::Parts::anim_speed_of`]).
    #[diff(
        gap = "the actions' sprite changes are ported only in part (the attack's swing, the idle swap)"
    )]
    pub sprite: Option<ActiveSprite>,
    /// `+0x78c`: which of its master's standing sprites its idle swap last showed (1 the second,
    /// [`Master::standing_sprite_2`]); not saved, 0 at a load and from the constructor.
    #[diff(unsaved)]
    pub idle_variant: i32,
    /// `+0x644`, `worker_num`: the actions that have the object as their target.
    #[diff(
        gap = "genie counts a target the game does not (MarvinBlue/426048731's town centre 9156 at update 1900)"
    )]
    pub workers: WorkerCount,
    /// `+0x650`, `mLastHumanOrderTime`: the world time of the last order a human gave it
    /// (`FUN_140e5e780`, for a unit with a unit AI); an animal's idle check lowers it to 5000.
    pub last_human_order: WorldTime,
    /// The slice could not model one of its updates: it freezes (its place is known only within
    /// its speed since) while the rest of its player's objects go on.
    pub stopped: bool,
}

impl Unit {
    /// The master the object reads through its master pointer (`+0xa0`): its own (`+0x6c0`,
    /// [`crate::world::parts::Parts::own_master`]) when it has one, else player `player`'s
    /// master of its id ([`crate::objects::masters::player_master`]), both of `tables`.
    ///
    /// # Errors
    ///
    /// No master of its own and none of its id in the player's table, which the game's object
    /// always has ([`crate::world::OBJECT_MASTER`], with the object, its master and the player).
    pub fn master<'masters>(
        &self,
        tables: MasterTables<'masters>,
        player: PlayerId,
    ) -> GenieResult<&'masters Master> {
        self.master_in(tables.of(player))
            .map_err(|error| error.with("player", player))
    }

    /// [`Unit::master`] for a caller that holds its owner's masters ([`OwnerMasters`]) rather
    /// than the world's.
    ///
    /// # Errors
    ///
    /// No master of its own and none of its id in the owner's table.
    pub(crate) fn master_in<'masters>(
        &self,
        masters: OwnerMasters<'masters>,
    ) -> GenieResult<&'masters Master> {
        object_master(
            masters.own.get(self.id),
            (self.id, self.life.master_id),
            masters.table,
        )
    }

    /// x, y, z on the map (`+0x3c8`, `+0x4c8`, `+0x5a8`, obfuscated in the game's memory): its
    /// static layer's ([`crate::objects::layers::Static`]).
    #[must_use]
    pub const fn position(&self) -> Position {
        self.place.position()
    }

    /// The object's place setter (`RGE_Static_Object::new_position`, `0x1411a58b0`): the place
    /// and the world's index of the places together, and its mesh shape where the caller holds
    /// the mesh, a `moving` object's going ([`crate::objects::layers::Static::new_position`]).
    pub(crate) fn new_position(
        &mut self,
        at: Position,
        moving: MoveState,
        places: (&mut Spatial, Option<&mut Pathing>),
        roles: &Roles,
        visibility_tuning: VisibilityTuning,
    ) -> GenieResult<()> {
        self.place
            .new_position(self.id, at, moving, places, roles, visibility_tuning)
    }

    /// A test's fixture puts the object at `at` without the world's index, which the test sets
    /// itself.
    #[cfg(test)]
    pub(crate) const fn place_for_test(&mut self, at: Position) {
        self.place.place_for_test(at);
    }
}

/// Whether the world's table (`FUN_1415417a0`) holds an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Presence {
    /// It holds it.
    Held,
    /// It does not.
    Gone,
}

impl Presence {
    /// The presence of what a lookup found.
    #[must_use]
    pub const fn of_found<T>(found: Option<&T>) -> Self {
        if found.is_some() {
            Self::Held
        } else {
            Self::Gone
        }
    }
}

/// The second target's checks at the start of a Gather's or a Hunt's update (`0x140a8e1d0`,
/// `0x140ac7a00`) on `site`, which the world's table holds ([`Presence`]) in `state`: one the table
/// lacks goes (`FUN_140a7a190` with none, with nothing to let go); one past state 6 is let go (slot
/// 0x180) and goes. The game checks the second target's absence before the first target's state;
/// the two touch different objects. Returns the second target after them and the object let go.
pub(crate) fn second_target_checked(
    site: Option<DropSiteId>,
    held: Presence,
    state: Option<ObjectState>,
) -> (Option<DropSiteId>, Option<DropSiteId>) {
    let Some(site) = site else {
        return (None, None);
    };
    if held == Presence::Gone {
        return (None, None);
    }
    match state {
        Some(object_state) if ObjectState::Undead < object_state => (None, Some(site)),
        _ => (Some(site), None),
    }
}

/// [`Unit::master_in`] from the fields it reads, its own master `own` and its id and master id
/// (`id`, `master_id`), for a caller that holds other fields of the unit to change: `own`, else
/// master `master_id` of its owner's `table`. The one place of the rule.
///
/// # Errors
///
/// No master of its own and none of `master_id` in `table`.
pub(crate) fn object_master<'masters>(
    own: Option<&'masters Master>,
    (id, master_id): (ObjectId, MasterId),
    table: Option<&'masters Masters>,
) -> GenieResult<&'masters Master> {
    own.or_else(|| table?.get(master_id))
        .ok_or_else(|| OBJECT_MASTER.with("object", id).with("master", master_id))
}

/// `RGE_Action_Object::update` (`0x14106ac90`): the action list of an object past state 2 goes
/// (`FUN_140a98620`), the Wonder's (master 276, `0x61b0` raw) only past state 5.
pub(crate) fn loses_actions(life: &Lifecycle, roles: &Roles) -> bool {
    let last = if roles
        .of_master(life.master_id)
        .contains(MasterRoles::WONDER)
    {
        ObjectState::Dead
    } else {
        ObjectState::Alive
    };
    last < life.state
}

/// The id of the stand-in a unit leaves in its list while it is taken out for its update or an
/// order ([`Unit::taken_out`]): no object has it, so a lookup by id during that time finds the
/// object itself, never the stand-in (the default unit's id 0 is a real object's, 428491175's
/// oak 0, which a petard's blast then read as an empty victim and felled).
pub(crate) const TAKEN_OUT: ObjectId = ObjectId::TAKEN_OUT;

/// [`Unit::missile_hits`]'s hit mode 1 (a missed missile): true when it passes the object it
/// meets (its id, `player`, master type `kind` and blast defence `level`): one of the missile's
/// own player or a missile, an ally's, and, when it is not the `target`, Gaia's of a type above
/// 15 or one of a blast defence below the missile's (`blast_level`).
pub(crate) fn missed_passes(
    step: &Step<'_>,
    (id, player, kind, level): (ObjectId, PlayerId, MasterKind, BlastDefense),
    target: Option<ObjectId>,
    blast_level: BlastLevel,
) -> bool {
    if player == step.player || kind == MasterKind::MISSILE {
        return true;
    }
    let other = Some(id) != target;
    step.world.spatial.allied_with(step.player, player)
        || (other && player == PlayerId::new(0) && MasterKind::TREE < kind)
        || (other && !blast_level.reaches(level))
}

/// What an object's update leaves of it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Outcome {
    Stay,
    /// It leaves its list for good (`RGE_Object_List::update` deletes it).
    Leave,
    /// It moves to its player's sleeping list.
    Sleep,
    /// A player took it or other objects of its player over ([`Captured`]).
    Captured(Captured),
}

#[cfg(test)]
mod tests;
