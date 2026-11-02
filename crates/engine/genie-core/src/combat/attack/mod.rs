//! The Attack action (`RGE_Action_Attack`, type 9, vtable `0x142ed47a8`) of a melee unit: a
//! hunting villager's sub-action on a tree or on its livestock ([`crate::gathering::hunt`]).
//! Built away from its target, it walks there with a `MoveTo` sub-action ([`crate::movement::move_to`]); in
//! reach it swings (state 7): the hit lands on the attack sprite's frame ([`crate::objects::sprite`]), and
//! the unit waits out its reload between swings (state 6). A target that dies ends the swing
//! (state 2) and the action (`set_state` 1, code 5 to the hunt).
//!
//! 158041: constructor `FUN_140a7c210`, start (slot 5) `0x140a7f8c0`, update (slot 6)
//! `0x140a82ae0`, `set_state` (slot 16) `0x140a820c0`; the range test `FUN_140a7f9d0`
//! (`readyToAttack`); the hit (object slot 0x858, `FUN_14120c0a0` -> `FUN_141081020` -> slot 0x860
//! `FUN_1412115e0` -> `FUN_141084230` -> the target's slot 0xe8, whose damage is
//! [`crate::combat::damage`]'s).

use crate::GenieError;
use crate::combat::damage::Victim;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::rng::Well512;
use crate::movement::Speed;
use crate::movement::move_to::{MoveDefaults, TargetMoved};
use crate::objects::ObjectId;
use crate::objects::layers::MotionMut;
use crate::objects::masters::ActionType;
use crate::objects::masters::ClassId;
use crate::objects::masters::HitChance;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::class;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::UnitAiType;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{OrderKind, UnitAi};
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::parts::PartTable;
use genie_diff::Diff;

mod attack_state;
mod constructor;
mod hits;
mod ready_to_attack;
mod sprites;
mod states;
mod unit;
mod unit_ai_side;
mod update;
mod walk_and_swing;
mod walks;
mod work_slots;

pub use self::attack_state::AttackState;
pub use self::constructor::{arms, arms_looking, engagement, gaps};
use self::hits::{AfterHit, after_hit, blast_so_far};
pub(crate) use self::hits::{Hit, strike};
#[cfg(test)]
use self::ready_to_attack::in_range;
use self::sprites::{NO_OBJECT_STREAM, set_attack_timer};
pub use self::sprites::{SwingSprites, swing_sprites};
pub(crate) use self::sprites::{face_target, reset_sprite, show_arrived, volleys};
pub(crate) use self::unit::run_down_attack_timers;
use self::update::squared_reach;
use self::walks::move_reads;
pub(crate) use self::walks::speed_of;
pub use self::work_slots::{Mark, Wielder, Worked};
#[cfg(test)]
use crate::combat::damage;
#[cfg(test)]
use crate::movement::Mover;
#[cfg(test)]
use crate::objects::sprite::ActiveSprite;

/// The Attack action's type, which word 9 adds.
pub const ATTACK: ActionType = ActionType::named(9);

/// An Attack whose target the map does not hold (none, or one gone).
pub(super) const TARGET_GONE: GenieError =
    GenieError::unsupported("an Attack whose target is gone");

/// A range at most this counts as none (`DAT_142eb5690`): the range test widens the gap by
/// `-0.5` (`DAT_142eb6e58`) and the reach by `0.5` (`DAT_142e7f384`).
const NO_RANGE: f32 = 0.1;
const HALF: f32 = 0.5;
/// The squared reach of an attack without a range (`DAT_142ed48e4`): `0x3c23d70b`, which is
/// `0.1f * 0.1f`, one ULP above `0.01f`.
const TOUCH: DistanceSquared = DistanceSquared::new(crate::math::image::attack::TOUCH);
/// The gap below which the game takes a distance as zero (`DAT_142eb0c60`).
const EPSILON: f32 = crate::math::image::attack::EPSILON;
/// The point an attack on an object has none of (`DAT_14330ec20`).
const NONE: f32 = crate::math::image::attack::NONE;
/// The milliseconds of a second the reload world time counts (`FUN_141089820`).
const RELOAD_MS: f32 = crate::math::image::attack::RELOAD_MS;
/// The hit chance's scale (`0x7fffffff`, the low word of a 64-bit Object-stream draw).
const DRAW_SCALE: i64 = 0x7fff_ffff;
/// The least range of a unit that walks onto the point where it saw its target
/// (`DAT_14330eb8c`, [`AttackState::GotoPoint`]).
const SEEN_RANGE: f32 = crate::math::image::attack::SEEN_RANGE;

/// The sprites and frames the Attack takes from its constructor.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Arms {
    /// `+0x68`: the sprite its `MoveTo` shows (its constructor's argument: the hunt's walking
    /// sprite, the task's moving one; else the master's running one, slot 0x7d0, else its
    /// walking one, slot 0x7c8).
    pub move_sprite: Option<Graphic>,
    /// `+0x70`: the attack sprite (the hunt's task's proceeding sprite); `+0x78`, a second one
    /// the unit picks from; `+0x80`, the sprite it stands with when done; `+0x88`, the sprite it
    /// waits with between swings (the master's idle attack sprite).
    pub sprite: Option<Graphic>,
    /// `+0x78`: a second attack sprite the unit picks from.
    pub second_sprite: Option<Graphic>,
    /// `+0x80`: the sprite it stands with when done.
    pub idle_sprite: Option<Graphic>,
    /// `+0x88`: the sprite it waits with between swings.
    pub idle_attack_sprite: Option<Graphic>,
    /// `+0x98`, the master's missile (`missile_id`, -1 for a melee unit); `+0x9a`, the frame that
    /// fires (`fire_missile_at_frame`); `+0x9c`, the frame that may interrupt
    /// (`interrupt_frame`), -1 for none.
    pub missile: Option<MasterId>,
    /// `+0x98`: the frame it fires at (`fire_missile_at_frame`).
    pub fire_frame: crate::objects::Frame,
    /// `+0x9c`: the frame that may interrupt (`interrupt_frame`), −1 for none.
    pub interrupt_frame: crate::objects::Frame,
}

/// The Attack action (`RGE_Action_Attack`, type 9) as the hunt's sub-action.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Attack {
    /// The target object (`+0x20`, its id at `+0x30`).
    pub target: Option<ObjectId>,
    /// The second target (`+0x28`, its id at `+0x34`): the work command's Attack takes its
    /// target as this too (`FUN_140a7a190`, slot 0x178), −1 for none.
    pub target2: Option<ObjectId>,
    /// The action state (`+0x18`), with its walk's quiet `MoveTo` ([`AttackState`]).
    pub state: AttackState,
    /// `+0x44`: seconds in the state, the update adds dt.
    pub timer: Seconds,
    /// `+0x90`: the range it attacks from; `+0x94`, the least.
    pub range: Distance,
    /// `+0x94`: the least range it attacks from.
    pub min_range: Distance,
    /// `+0x38`..`+0x40`: the point it attacks; -1 for none.
    pub point: Position,
    /// `+0xa4`..`+0xac`: where it last saw its target.
    pub seen: Position,
    /// `+0x9e`: a hit is still to land in this swing.
    pub pending: bool,
    /// `+0xb0` and `+0xb4`: the time of a swing past its sprite's length; a melee swing counts
    /// `+0xb0` down until the hit.
    #[diff(
        gap = "differs from the save (MarvinBlue/426048731's 9181 at update 1025: genie -0.34, game 0)"
    )]
    pub swing: Seconds,
    /// `+0xb4`: the rest after the swing's sprite.
    #[diff(
        gap = "differs from the save (MarvinBlue/426048731's 10029 at update 1150: genie 0.5, game 0)"
    )]
    pub rest: Seconds,
    /// The sprites and frames from its constructor.
    pub arms: Arms,
    /// `+0x60`, `subActionValue`: the action is another's sub-action (the Hunt's) and posts no
    /// notifies; a unit's own Attack posts them to its unit AI (slot 0x328).
    pub sub_action: bool,
    /// `+0x61`, `mTargetMovedState`, which its `MoveTo` takes ([`TargetMoved`]): a unit's own
    /// Attack's follows, the Hunt's asks for a path at once (its `set_state` 12 sets it).
    pub on_target_moved: TargetMoved,
    /// `+0x9f`, `was_same_owner` (saved): its target was its unit's player's when the
    /// constructor (`FUN_140a7c210`) took it; the update's ally test spares such a livestock or
    /// farm target ([`Attack::update`]).
    pub same_owner: bool,
}

/// A notify an Attack's start posts to its unit's AI: its kind and three parameters.
pub(crate) type Posted = (crate::unit_ai::NotifyKind, [i32; 3]);

/// A unit's own Attack command ([`Attack::command`]): its target, what it engages and with
/// which arms, the target's sight and owner, and what the start's `set_state(6)` reads.
#[derive(Clone, Debug)]
pub(crate) struct AttackCommand<'a> {
    /// The target.
    pub(crate) target: ObjectId,
    /// What the Attack reads of its unit and its target as it starts.
    pub(crate) engagement: &'a Engagement,
    /// Its sprites, missile and frames.
    pub(crate) arms: Arms,
    /// The target is in the unit's player's sight (its slot 0x1e0 0xf, or 0x80 with its
    /// master's fog flag).
    pub(crate) visible: bool,
    /// The target's owner.
    pub(crate) target_owner: PlayerId,
    /// What the start's `set_state(6)` reads ([`ReadyReads`]); None when the world does not hold
    /// the target live.
    pub(crate) ready: Option<ReadyReads>,
}

/// What the start's `set_state(6)` reads for its range test and swing
/// ([`Attack::enter_ready`]), read as the command starts.
#[derive(Clone, Debug)]
pub(crate) struct ReadyReads {
    /// The swing's reads of the attacker.
    pub(crate) reads: AttackReads,
    /// The target and the attacker as combat objects.
    pub(crate) fight: Fight,
}

/// readyToAttack's answer to the start ([`Attack::begin`]), which the start's end follows
/// ([`Attack::command`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Begun {
    /// Its last `MoveTo` started arrived.
    arrived: bool,
    /// The start ends in `set_state(6)`.
    ready: bool,
    /// The start ends in `set_state(13)`.
    failed: bool,
    /// readyToAttack's `set_state(13)` came before the start's `set_state(4)`.
    failed_first: bool,
}

/// What the Attack reads of its unit and its target when it starts.
#[derive(Clone, Copy, Debug)]
pub struct Engagement {
    /// The gap between the two footprints squared (`FUN_14118f8c0`) and its root (`FUN_14118f680`).
    pub gap_squared: DistanceSquared,
    /// The gap's root (`FUN_14118f680`).
    pub gap: Distance,
    /// The unit's and the target's positions.
    pub position: Position,
    /// The target's place.
    pub target_position: Position,
    /// The unit's top speed (slot 0x208), its master's weapon range with any bonus against the
    /// target (slot 0x238) and its least range.
    pub speed: Speed,
    /// Its master's weapon range with any bonus against the target (slot 0x238).
    pub range: Distance,
    /// Its least range.
    pub min_range: Distance,
    /// The unit AI's `canMoveToAttack` (`FUN_140c48420`): it has an order, or no order but a stance
    /// below 2.
    pub can_move: bool,
    /// The unit AI's stance (`+0x1a9`), whose 1 adds a leash test.
    pub stance: UnitStance,
    /// The unit AI's `state_position` (`+0x1ac`, `+0x1b0`), the leash's centre.
    pub state_position: Point,
    /// The world's time (`FUN_1415300d0`), which a facing over time reads.
    pub world_time: WorldTime,
    /// The unit's master, of which 1704 and 1706 do not turn to face their target.
    pub master: MasterId,
    /// The target's master's class, whose 39 (a gate) readyToAttack measures by its parts.
    pub target_class: ClassId,
    /// The target is the unit's player's (the constructor's `+0x9f`, [`Attack::same_owner`]).
    pub same_owner: bool,
}

/// What the Attack's update reads of its unit, its master, its unit AI and its owner, copied as
/// the update starts ([`AttackReads::of`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct AttackReads {
    /// Its master's standing sprite (`+0x470`), which slot 0x68 shows for no sprite
    /// (`FUN_141158620`).
    pub(crate) standing: Option<Graphic>,
    /// Its master has a volley missile (`+0x8d4`) or a charge projectile (`+0x928`) its player
    /// has: a hit without a missile is a volley too (`FUN_14120c0a0`), not the melee hit.
    pub(crate) volleys: bool,
    /// Slot 0x218 (`FUN_141086c90`): the master's reload time (`speed_of_attack`) with the
    /// unit's reload stinger and powerup.
    pub(crate) reload: Seconds,
    /// The master's hit chance (`+0x83e`).
    pub(crate) hit_chance: HitChance,
    /// `FUN_14120ea30` for a building: it can attack (class 51 or 54, a missile, or a firepower
    /// above 1); true for the others.
    pub(crate) armed: bool,
    /// The swing's own sprites ([`Attack::set_swing`]): its terrain look's attack sprite and its
    /// master's special graphic.
    pub(crate) swing_sprites: SwingSprites,
    /// The unit's player is a computer (`player+0x10 == 3`, `FUN_141659cf0`).
    pub(crate) computer: bool,
    /// The world's time (`FUN_1415300d0`), which a facing over time and the reload read.
    pub(crate) world_time: WorldTime,
    /// The unit AI's canMoveToAttack (slot 0x1d0, `FUN_140c48420`), which readyToAttack reads
    /// of a target out of reach.
    pub(crate) can_move: bool,
    /// The unit AI's stance (`+0x1a9`).
    pub(crate) stance: UnitStance,
    /// The unit AI's `state_position` (`+0x1ac`, `+0x1b0`), the centre of stance 1's leash.
    pub(crate) state_position: Point,
    /// The unit has a unit AI whose order queue is empty (`FUN_140c564a0` is 0).
    pub(crate) queue_empty: bool,
    /// The unit AI's order (`+0x3c`, `FUN_140c4cc20`).
    pub(crate) ai_order: AiOrder,
    /// The unit AI's factory type, None without a unit AI.
    pub(crate) ai_type: Option<UnitAiType>,
    /// `FUN_1412161a0`: a volley is under way (`next_volley`, `+0xa04`, not 0).
    pub(crate) volley_under_way: bool,
    /// The sprites a `MoveTo` of the Attack takes when the Attack gives none: the defaults of the
    /// unit's master and terrain look.
    pub(crate) move_defaults: MoveDefaults,
}

impl AttackReads {
    /// What the Attack of a unit of `master` with the combat layer `combat` reads: its player's masters and the sprites' table
    /// (`tables`), its unit AI `ai` (the unit's own, or the one running outside it), whether its
    /// owner is a computer player and the world's time (`now`).
    pub(crate) fn of(
        combat: &crate::objects::layers::Combat,
        master: &Master,
        (masters, graphics): (
            crate::objects::masters::OwnerMasters<'_>,
            &crate::objects::Graphics,
        ),
        ai: Option<&UnitAi>,
        (computer, now): (bool, WorldTime),
    ) -> Self {
        Self {
            standing: master
                .standing_sprite
                .and_then(|s| graphics.get(s))
                .copied(),
            volleys: masters.table.is_some_and(|m| volleys(master, m)),
            reload: combat.reload_stinger + (master.reload - combat.reload_powerup),
            hit_chance: master.hit_chance,
            armed: crate::combat::volley::armed(master, combat.firepower),
            swing_sprites: swing_sprites(combat.abilities.look.as_ref(), master, graphics),
            computer,
            world_time: now,
            can_move: ai
                .is_some_and(|ai| ai.current_order.is_some() || ai.stance <= UnitStance::DEFENSIVE),
            stance: ai.map_or(UnitStance::AGGRESSIVE, |ai| ai.stance),
            state_position: ai.map_or(Point::ORIGIN, |ai| ai.state_position),
            queue_empty: ai.is_some_and(|ai| ai.orders.is_empty()),
            ai_order: AiOrder::of(ai),
            ai_type: ai.map(|ai| ai.ai_type),
            volley_under_way: combat.next_volley != crate::combat::MissileCount::ZERO,
            move_defaults: MoveDefaults::of(master, combat.abilities.look.as_ref()),
        }
    }
}

/// The unit AI's order as the Attack reads it (`+0x3c`, `FUN_140c4cc20`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AiOrder {
    /// The unit has no unit AI.
    Absent,
    /// Its unit AI has no order (the field's −1).
    Idle,
    /// Its unit AI's order.
    Order(OrderKind),
}

impl AiOrder {
    /// The order of the unit AI `ai`.
    pub(crate) fn of(ai: Option<&UnitAi>) -> Self {
        ai.map_or(Self::Absent, |ai| {
            ai.current_order.map_or(Self::Idle, Self::Order)
        })
    }

    /// The unit has a unit AI.
    pub(crate) const fn has_ai(self) -> bool {
        !matches!(self, Self::Absent)
    }
}

/// The strike's combat objects, copies its update reads as it starts: its target (`victim`) and
/// the unit as a combat object (`own`). A hit changes both: the target goes back to the world
/// where the hit lands ([`crate::combat::victims::write_strike_back`]), the unit after its
/// update ([`Unit::hit_back`]).
#[derive(Clone, Debug)]
pub(crate) struct Fight {
    /// The target, which the hit changes.
    pub(crate) victim: Victim,
    /// The unit as a combat object: what its hits read of it (its attack, its master's weapons,
    /// its place and its player's elevation attributes) and what a retaliation, reflected damage
    /// or its charge changes.
    pub(crate) own: Victim,
}

/// The world's parts readyToAttack and the swing's start read and draw from, built where they
/// are needed ([`World::attack_ground`] in an update): the map, the Sprite stream, the Object
/// stream (none where no update runs, as at the attack command's start: a draw there is not
/// modelled) and the list the notifies go to while the unit AI is out of the unit (the attack
/// command's start as the AI runs: the AI posts them itself); else each goes to the unit AI at
/// once (slot 0x328).
pub(crate) struct Ground<'a> {
    /// The map.
    pub(crate) map: &'a Spatial,
    /// The Sprite stream, which a new sprite draws from.
    pub(crate) sprite_rng: &'a mut Well512,
    /// The Object stream, which a swing's second sprite draws from.
    pub(crate) object_rng: Option<&'a mut Well512>,
    /// Where the unit's notifies go.
    pub(crate) posts: Posts<'a>,
    /// The sprites' table, which a `MoveTo` the Attack makes takes its sprite from.
    pub(crate) graphics: &'a crate::objects::Graphics,
    /// The objects by id: a gate's annexes, which its reach measures.
    pub(crate) by_id: crate::world::index::ObjectsRef<'a>,
    /// The unit's own motion layer (its mover and animation speed), lent out of the world's
    /// table while `by_id` reads the others' (FD-045-D58).
    pub(crate) motion: MotionMut<'a>,
}

/// Where [`Ground::post`] sends the unit's notifies.
pub(crate) enum Posts<'a> {
    /// The list the unit AI posts itself while it is out of the world's table.
    Held(&'a mut Vec<(crate::unit_ai::NotifyKind, [i32; 3])>),
    /// The world's unit AIs: each notify goes to the unit's at once.
    Ais(&'a mut PartTable<UnitAi>),
}

impl Ground<'_> {
    /// The unit's notify of `kind` with `p` (slot 0x328): to the list while its unit AI is out
    /// of the world's table, else to its unit AI at once.
    ///
    /// # Errors
    ///
    /// The notify's.
    pub(crate) fn post(
        &mut self,
        unit: &Unit,
        kind: crate::unit_ai::NotifyKind,
        p: [i32; 3],
    ) -> Result<(), GenieError> {
        match &mut self.posts {
            Posts::Held(list) => {
                list.push((kind, p));
                Ok(())
            }
            Posts::Ais(ais) => unit.post_own(ais, kind, p),
        }
    }
}

impl World {
    /// The Attack's [`Ground`] in an update of unit `id`: the map, the Sprite and Object
    /// streams, its motion layer; the notifies go to the unit AI at once.
    pub(crate) fn attack_ground(&mut self, id: ObjectId) -> Ground<'_> {
        let Self {
            spatial,
            streams,
            graphics,
            players,
            index,
            masters,
            parts,
            ..
        } = self;
        let (object, sprite) = streams.object_and_sprite();
        let (motion, motions) = parts.motion.lend(id);
        Ground {
            map: spatial,
            sprite_rng: sprite,
            object_rng: Some(object),
            posts: Posts::Ais(&mut parts.unit_ai),
            graphics,
            by_id: crate::world::index::ObjectsRef::of_tables(
                players,
                index,
                crate::objects::masters::MasterTables::new(masters, &parts.own_master),
                (
                    &parts.building,
                    &parts.combat,
                    &parts.actions,
                    &parts.unread,
                    motions,
                ),
            ),
            motion: MotionMut::new(motion),
        }
    }
}

/// The unit's animation speed (`+0x7b8`) as the Attack sets it: a mover's; a building's stays
/// (MISSING.md, "The Attack's animation speed of a building").
pub(crate) fn set_walker_anim(mut motion: MotionMut<'_>, speed: Speed) {
    if let Some(m) = motion.mover_mut() {
        m.animated.speed = speed;
    }
}

/// readyToAttack's gap for a range up to [`NO_RANGE`]: half a tile less (`DAT_142eb6e58`).
const SHORT_RANGE_GAP: Distance = Distance::new(crate::math::image::attack::SHORT_RANGE_GAP);

/// readyToAttack's leash of stance 1 lets a target this much past the range from the unit AI's
/// `state_position` (`DAT_14330ebb8`, read by `ADDSS` at `0x140a80051`).
const LEASH_SLACK: Distance = Distance::new(crate::math::image::attack::LEASH_SLACK);

/// The masters readyToAttack does not turn to face their target (`0x6744`, `0x6746` raw).
const MASTER_1704: MasterId = MasterId::named(1704);
const MASTER_1706: MasterId = MasterId::named(1706);

/// The masters whose swings post no executed action (`0x60e5`, `0x62cb` raw).
const QUIET_SWINGS: [MasterId; 2] = [MasterId::named(73), MasterId::named(559)];

/// The class an Attack does not take as its target (`-0xe70` raw, [`Attack::set_target`]).
const CLASS_60: ClassId = class::CLASS_60;

/// An Attack in a state that walks to its target object, without one.
const ATTACK_WITHOUT_TARGET: GenieError =
    GenieError::unreachable("an Attack's set_state(4) without its target");

/// What readyToAttack answers: ready to swing, waiting for the attack timer, or the target out of
/// reach at this gap (the unit follows it).
enum Readiness {
    Ready,
    Waiting,
    Away(Distance),
}

#[cfg(test)]
pub(crate) mod tests;
