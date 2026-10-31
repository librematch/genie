//! What the slice tracks of one object ([`Unit`]) and its update, action by action.

use crate::map::Distance;
use crate::map::Position;
use crate::objects::HpPowerup;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ids::WONDER;
use crate::objects::masters::master_type::MISSILE;
use crate::objects::masters::master_type::TREE;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use crate::world::WorldTime;
use genie_diff::Diff;

use crate::GenieError;
use crate::commands::GatherPoint;
use crate::construction::build::Construction;
use crate::objects::actions::ActionList;
use crate::objects::layers::Body;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::{Master, Masters};
use crate::objects::sprite::ActiveSprite;
use crate::ownership::capture::Captured;
use crate::production::Production;
use crate::unit_ai::UnitAi;
use crate::world::Gate;
use crate::world::step::Step;

mod death;
mod default_task;
mod life_step;
mod stop;
mod top;
mod update;

/// A dying sprite the `.dat` does not have.
const NO_DYING_SPRITE: GenieError =
    GenieError::unsupported("a dying sprite the data file does not have");

/// What the slice tracks of one object.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Unit {
    /// The object's id (`+0x8`), which names it across ticks.
    pub id: ObjectId,
    /// `+0x889`: its waits as a builder for an occupied footprint (not saved; 0 at a load).
    #[diff(unsaved)]
    pub footprint_waits: u8,
    /// Its life cycle: state, hit points, master, what it holds, its dying sprite.
    pub life: Lifecycle,
    /// Its unit AI, when it has one.
    pub ai: Option<UnitAi>,
    /// A default task other than the Artifact action. `RGE_Action_Object::update` (0x14106ac90)
    /// starts it in an object's first update if the object is still in state 2 after its static
    /// step; the slice does not model it, so that update stops the player.
    #[diff(unsaved, eq)]
    pub unmodelled_action: Option<GenieError>,
    /// A building's production queue and make action.
    pub production: Option<Production>,
    /// A building's gather point, which its new units go to.
    pub gather_point: Option<GatherPoint>,
    /// `+0x9e8` (`num_builders`) of a combat object that is no building: the Repair actions
    /// counted among its repairers. A building's is its construction's
    /// ([`Construction::builders`]).
    pub builders: u8,
    /// Its layers below the action layer ([`Body`]): the static layer with its place (every
    /// object), the animated layer (`+0x7b8`, types 20 and up) and the moving one with its
    /// movement state (types 30 and up).
    pub body: Body,
    /// Its action list (`RGE_Action_List`, [`ActionList`]): the actions it holds, its current one
    /// first.
    pub actions: ActionList,
    /// A building's construction.
    pub construction: Option<Construction>,
    /// `+0xa30`, `mWorkRatePowerup`: what the unit adds to its master's work rate.
    pub work_rate_powerup: f32,
    /// `+0x68a`, `unit_attackable_flag`.
    pub attackable: bool,
    /// `+0x689`, `mUnitTargetingFlag`, is 0 (a new object's is 1): a gatherer's search takes the
    /// object only through a pasture (`ClosestResourceObject`).
    pub untargetable: bool,
    /// `+0x685`, `underAttackValue`: its unit AI keeps attackers (`FUN_14119d2d0`).
    pub under_attack: bool,
    /// `+0x790`, `parent_pasture_id`: the pasture it is in.
    pub pasture: Option<ObjectId>,
    /// `+0x794`, `my_pasture_id`: the pasture it tends.
    pub my_pasture: Option<ObjectId>,
    /// `mDeletionEnabled` (`+0x687`, 1 from its constructor) is 0: no delete takes it.
    pub undeletable: bool,
    /// `capture_flag` (`+0x924`).
    pub capture_flag: u8,
    /// An annex's main building (`+0xbb8`, the save's linked owner).
    pub annex_of: Option<ObjectId>,
    /// Its unit group, if any ([`crate::groups::GroupLink`]).
    pub group: crate::groups::GroupLink,
    /// `mHitpointsPowerup` and `mHitpointsMaxPowerup` (`+0xa38`, `+0xa3c`), which the task finder
    /// adds to the hit points.
    pub hp_powerup: HpPowerup,
    /// `berserker_timer` (`+0xa10`): the seconds the regeneration holds back toward its next
    /// whole hit point ([`Unit::regenerate`]).
    pub regen_timer: Seconds,
    /// `mRegenerationRatePowerup` (`+0xa34`), which the regeneration rate adds to its master's
    /// ([`regeneration_rate`]).
    ///
    /// [`regeneration_rate`]: crate::objects::hit_points::regeneration_rate
    pub regen_powerup: f32,
    /// `mRegenerationStinger` (`+0xb3c`), which the regeneration rate adds last
    /// ([`regeneration_rate`]).
    ///
    /// [`regeneration_rate`]: crate::objects::hit_points::regeneration_rate
    pub regen_stinger: f32,
    /// A missile's own part with its action ([`crate::combat::missile`]).
    pub missile: Option<crate::combat::missile::Missile>,
    /// `+0x900`, `attack_count`: the swings that fired a missile (`FUN_141081020`); a missile's
    /// is its firing object's when fired.
    #[diff(
        gap = "differs on 380 of MarvinBlue/426048731's samples (9181 at update 1075: genie 0, game 1): the resets on a new target (FUN_140a7c210, FUN_140a82a10) are not checked"
    )]
    pub attack_count: i32,
    /// `+0xa00` (not saved): a combat object's missiles per volley, its master's volley fire plus
    /// the shares of the units inside it ([`crate::garrison::share`]).
    #[diff(unsaved)]
    pub firepower: f32,
    /// `next_volley` (`+0xa04`): the missiles of the volley under way already fired.
    pub next_volley: u8,
    /// `mReloadWorldtime` (`+0x920`, slots 0x248 and 0x250): the world time its attack timer
    /// runs out at (`FUN_141089820`), 0 for none; once past, the volley under way is over.
    pub reload_worldtime: WorldTime,
    /// The objects inside it (`+0x2f8`), in the order they came in (`FUN_141164af0`); one that
    /// leaves takes the last one's place (`FUN_141166450`).
    /// Not saved as a list: a load lists them by each one's `+0x2f0` in the objects' order.
    #[diff(unsaved)]
    pub garrison: Vec<ObjectId>,
    /// A building's `last_garrison_time` (`+0xbe0`): the world time of its last whole second,
    /// on which its garrison heals ([`Unit::building_seconds`]).
    pub garrison_time: WorldTime,
    /// A building's `relic_count` and `specific_relic_count` (`+0xc20`, `+0xc24`).
    pub relics: [i32; 2],
    /// `damaged_lately_timer` (`+0x30d`, saved): 60 after a hit on a building (`FUN_1411cea90`),
    /// one off in each of its updates that passes whole seconds; the computer AI reads it.
    pub damaged_lately: u8,
    /// A building's `mLivestockSummation` (`+0xc28`, saved): the livestock inside it (their
    /// master's `+0x610`, `FUN_1411cb850`), whose food a mill makes ([`crate::players::world_player`]).
    pub livestock_summation: f32,
    /// A pasture's livestock (`+0xb78`, saved as `livestockId`): the objects its completion put
    /// on its edges ([`crate::world::World::setup_pasture`]), in the order it made them.
    pub livestock: Vec<ObjectId>,
    /// A pasture's workers (`+0xcb8`, saved as `pastureWorkerId`): the units that tend it
    /// ([`crate::world::World::tend_pasture`]), in the order they came.
    pub pasture_workers: Vec<ObjectId>,
    /// A building's `+0xc54`: the frame its BUILD command chose (`FUN_140e7a400`, the packet's
    /// `+0x1c`), which its standing sprite takes once built; 0 from its constructor and the load.
    #[diff(unsaved)]
    pub build_frame: u8,
    /// A combat object's `raider_build_countdown` (`+0x9a0`): the seconds it may stay inside a
    /// building.
    pub raider_countdown: i32,
    /// A building's gate lock ([`Gate`]).
    pub gate: Option<Gate>,
    /// Its active sprite, held from an attack's swing on; None while the slice does not know it.
    /// Its clock runs at the animated layer's speed ([`Body::anim_speed`]).
    #[diff(
        gap = "the actions' sprite changes are ported only in part (the attack's swing, the idle swap)"
    )]
    pub sprite: Option<ActiveSprite>,
    /// `+0x91c`, `attack_timer`: the time until it may attack again; `mReloadStinger` and
    /// `mReloadPowerup` (`+0xb40`, `+0xa28`), which change its reload time.
    pub attack_timer: Seconds,
    /// What its master's passive abilities keep on it ([`crate::combat::abilities`]): the movement damage
    /// timer, the attack timer's rate and its terrain look.
    pub abilities: crate::combat::abilities::Abilities,
    /// `mReloadStinger` (`+0xb40`), added to its master's reload time.
    pub reload_stinger: f32,
    /// `mReloadPowerup` (`+0xa28`), taken off its master's reload time.
    pub reload_powerup: f32,
    /// What a hit reads of its unit and its target besides the masters: `+0xa24`
    /// (`mAttackPowerup`, the attacker's slot 0x6d8), the armor powerups (`+0xa48` melee,
    /// `+0xa4c` pierce, slots 0x708 and 0x710; whole numbers), the armor attacks removed
    /// (`+0x9e4`, `+0x9e6`), the charge (`+0x9e0`) and `mHitSomethingValidForChargeAttack`
    /// (`+0x8a8`: its last hit was its charge's).
    pub attack_powerup: f32,
    /// `mMaxRangePowerup` (`+0xa40`, slot 0x700): what its shown range adds
    /// ([`crate::combat::damage::shown_range`]).
    pub range_powerup: Distance,
    /// Its melee and pierce armor powerups (`+0x9e4`, `+0x9e6`).
    pub armor_powerups: [i16; 2],
    /// The melee and pierce armor taken off it (its charge's hits).
    pub armor_removed: [i16; 2],
    /// Its charge (`+0x9e0`).
    pub charge: f32,
    /// `mHitSomethingValidForChargeAttack` (`+0x8a8`): its last hit was its charge's.
    pub charge_hit: bool,
    /// `+0x78c`: which of its master's standing sprites its idle swap last showed (1 the second,
    /// [`Master::standing_sprite_2`]); not saved, 0 at a load and from the constructor.
    #[diff(unsaved)]
    pub idle_variant: i32,
    /// `+0x644`, `worker_num`: the actions that have the object as their target.
    #[diff(
        gap = "genie counts a target the game does not (MarvinBlue/426048731's town centre 9156 at update 1900)"
    )]
    pub workers: i32,
    /// `+0x650`, `mLastHumanOrderTime`: the world time of the last order a human gave it
    /// (`FUN_140e5e780`, for a unit with a unit AI); an animal's idle check lowers it to 5000.
    pub last_human_order: WorldTime,
    /// A combat object's town-bell flag and back-to-work records ([`crate::orders::town_bell`]).
    pub town_bell: crate::orders::town_bell::TownBell,
    /// The object is a combat object (types 50 and up): its update keeps its sight mask and its
    /// records in the objects-in-sight registry.
    pub combat: bool,
    /// What its master's default task would start once its action list is empty, when the
    /// slice does not model it (None: nothing, as a wild animal's Explore).
    #[diff(unsaved, eq)]
    pub default_task: Option<GenieError>,
    /// The Fly task its master's default task starts once its action list is empty (a bird at
    /// a game's start, before its first update). Not saved: genie's copy of the master's task,
    /// which a load from the header holds on and a load of a later save clears.
    #[diff(unsaved)]
    pub default_fly: Option<crate::objects::masters::Task>,
    /// Its master's default task is a Make, which starts once its action list is empty in
    /// state 2 (a farm or fish trap before its first update as a built building), with the task
    /// its master holds then.
    pub default_make: bool,
    /// Its master's default task is the Artifact action (livestock), which starts again once its
    /// action list is empty in state 2 (an animal out of a building whose Artifact went), its
    /// first update in the next tick, as a Fly's.
    #[diff(gap = "the import reads the shared master's default task, not an own master's")]
    pub default_artifact: bool,
    /// The slice could not model one of its updates: it freezes (its place is known only within
    /// its speed since) while the rest of its player's objects go on.
    pub stopped: bool,
    /// `mROOAlreadyCalled` (`+0x686`, saved): its obstruction record went (its death's
    /// `FUN_14121cc40`, a footprint clearing's `FUN_141189670`, a construction start's clearing
    /// `FUN_1411cd0d0`, a stack unit's completion); later ones leave it alone.
    pub roo_called: bool,
    /// A building's `+0xcd0`: its constructor found mangrove or rice terrain at its place
    /// (`FUN_141188db0`), which master 1889's completion reads.
    pub rice: bool,
    /// A building's `+0xcb0` (`FUN_1411c5f70`): the id of the object its completion's stack unit
    /// made ([`crate::world::World::stack_unit`]); None before (the save does not hold it).
    pub stack_made: Option<ObjectId>,
    /// A building's own store (`+0xae0`, `FUN_1411abad0`'s map by attribute): what its storage
    /// entries of mode 16 put in at its completion and take out at its death
    /// (`FUN_1411e6580`).
    pub store: Vec<(AttributeId, Amount)>,
    /// `+0x6c0` (`FUN_141185010`): the master of its own its master pointer (`+0xa0`) reads, a
    /// copy (`FUN_141186d10`, the master's slot 0x48) of the one it read when it took it; None
    /// while it reads its player's. An ownership change gives it one ([`change_hands`]).
    ///
    /// [`change_hands`]: crate::ownership::change::change_hands
    #[diff(
        gap = "the import does not read a saved own master; genie makes none for a foundation or a dying annex"
    )]
    pub own_master: Option<Box<Master>>,
    /// `+0x6b8`: the copy of its old owner's death-spawn master (`+0x5e4`) an ownership change
    /// took ([`change_hands`]), which its death spawn makes its dead object of
    /// (`FUN_1411a5090`); None for none.
    ///
    /// [`change_hands`]: crate::ownership::change::change_hands
    #[diff(gap = "the import does not read a saved dead master")]
    pub dead_master: Option<Box<Master>>,
    /// `+0xce0`, `attackingUnits` (saved): the units of players its owner is not allied with
    /// that took this foundation (a building below state 2 whose master has a stack unit,
    /// `+0x986`) as their target, once per take, in order ([`Unit::lists_taker`]); its stack
    /// unit's completion tells each of the new object.
    pub attacking_units: Vec<ObjectId>,
}

impl Unit {
    /// The master the object reads through its master pointer (`+0xa0`): its own (`+0x6c0`,
    /// [`Unit::own_master`]) when it has one, else player `player`'s master of its id in
    /// `every`, the world's masters ([`crate::objects::masters::player_master`]).
    ///
    /// # Errors
    ///
    /// No master of its own and none of its id in the player's table, which the game's object
    /// always has ([`crate::world::OBJECT_MASTER`], with the object, its master and the player).
    pub fn master<'a>(
        &'a self,
        every: &'a [Masters],
        player: PlayerId,
    ) -> Result<&'a Master, GenieError> {
        self.master_in(every.get(player.index()))
            .map_err(|e| e.with("player", player))
    }

    /// [`Unit::master`] for a caller that holds its owner's table of masters (`table`, none for
    /// an owner without one) rather than the world's.
    ///
    /// # Errors
    ///
    /// No master of its own and none of its id in `table`.
    pub(crate) fn master_in<'a>(
        &'a self,
        table: Option<&'a Masters>,
    ) -> Result<&'a Master, GenieError> {
        object_master(
            self.own_master.as_deref(),
            (self.id, self.life.master_id),
            table,
        )
    }

    /// x, y, z on the map (`+0x3c8`, `+0x4c8`, `+0x5a8`, obfuscated in the game's memory): its
    /// static layer's ([`crate::objects::layers::Static`]).
    #[must_use]
    pub const fn position(&self) -> Position {
        self.body.position()
    }

    /// The object's place setter (`RGE_Static_Object::new_position`, `0x1411a58b0`): the place
    /// and the world's index of the places together, and its mesh shape where the caller holds
    /// the mesh ([`crate::objects::layers::Static::new_position`]).
    pub(crate) fn new_position(
        &mut self,
        at: Position,
        places: (
            &mut crate::map::spatial::Spatial,
            Option<&mut crate::pathing::Pathing>,
        ),
    ) -> Result<(), GenieError> {
        self.body.new_position(self.id, at, places)?;
        Ok(())
    }

    /// A test's fixture puts the object at `at` without the world's index, which the test sets
    /// itself.
    #[cfg(test)]
    pub(crate) const fn place_for_test(&mut self, at: Position) {
        self.body.base_mut().place_for_test(at);
    }
}

/// A building's annexes among `objects` (`+0xbc0`.., the save's linked object ids), in id order,
/// as the building made them.
pub(crate) fn annexes_of<'a>(
    id: ObjectId,
    objects: impl IntoIterator<Item = &'a Unit>,
) -> Vec<ObjectId> {
    let mut annexes: Vec<ObjectId> = objects
        .into_iter()
        .filter(|u| u.annex_of == Some(id))
        .map(|u| u.id)
        .collect();
    annexes.sort_unstable();
    annexes
}

/// The second target's checks at the start of a Gather's or a Hunt's update (`0x140a8e1d0`,
/// `0x140ac7a00`) on `site`, which the world's table holds (`held`,
/// `FUN_1415417a0`) in `state`: one the table lacks goes (`FUN_140a7a190` with none, with nothing
/// to let go); one past state 6 is let go (slot 0x180) and goes. The game checks the second
/// target's absence before the first target's state; the two touch different objects. Returns
/// the second target after them and the object let go.
pub(crate) fn second_target_checked(
    site: Option<ObjectId>,
    held: bool,
    state: Option<ObjectState>,
) -> (Option<ObjectId>, Option<ObjectId>) {
    let Some(site) = site else {
        return (None, None);
    };
    if !held {
        return (None, None);
    }
    match state {
        Some(s) if ObjectState::Undead < s => (None, Some(site)),
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
pub(crate) fn object_master<'a>(
    own: Option<&'a Master>,
    (id, master_id): (ObjectId, MasterId),
    table: Option<&'a Masters>,
) -> Result<&'a Master, GenieError> {
    own.or_else(|| table?.get(master_id)).ok_or_else(|| {
        crate::world::OBJECT_MASTER
            .with("object", id)
            .with("master", master_id)
    })
}

/// `RGE_Action_Object::update` (`0x14106ac90`): the action list of an object past state 2 goes
/// (`FUN_140a98620`), the Wonder's (master 276, `0x61b0` raw) only past state 5.
pub(crate) fn loses_actions(life: &Lifecycle) -> bool {
    let last = if matches!(life.master_id, WONDER) {
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
    c: &Step<'_>,
    (id, player, kind, level): (ObjectId, PlayerId, MasterKind, u8),
    target: Option<ObjectId>,
    blast_level: u8,
) -> bool {
    if player == c.player || kind == MISSILE {
        return true;
    }
    let other = Some(id) != target;
    c.w.spatial.allied_with(c.player, player)
        || (other && player == PlayerId::new(0) && TREE < kind)
        || (other && level < blast_level)
}

/// What an object's update leaves of it.
#[derive(Clone, Debug, PartialEq, Eq)]
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
