//! The Attack action (`RGE_Action_Attack`, type 9, vtable `0x142ed47a8`) of a melee unit: a hunting
//! villager's sub-action on a tree or on its livestock ([`crate::gathering::hunt`]). Built away
//! from its target, it walks there with a `MoveTo` sub-action ([`crate::movement::move_to`]); in
//! reach it swings (state 7): the hit lands on the attack sprite's frame
//! ([`crate::objects::sprite`]), and the unit waits out its reload between swings (state 6). A
//! target that dies ends the swing (state 2) and the action (`set_state` 1, code 5 to the hunt).
//!
//! 158041: constructor `FUN_140a7c210`, start (slot 5) `0x140a7f8c0`, update (slot 6)
//! `0x140a82ae0`, `set_state` (slot 16) `0x140a820c0`; the range test `FUN_140a7f9d0`
//! (`readyToAttack`); the hit (object slot 0x858, `FUN_14120c0a0` -> `FUN_141081020` -> slot 0x860
//! `FUN_1412115e0` -> `FUN_141084230` -> the target's slot 0xe8, whose damage is
//! [`crate::combat::damage`]'s).

use crate::combat::damage::Victim;
use crate::combat::reload::reload_time;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::rng::Well512;
use crate::movement::MoveStart;
use crate::movement::Speed;
use crate::movement::TopSpeed;
use crate::movement::move_to::{MoveDefaults, TargetMoved};
use crate::objects::ObjectId;
use crate::objects::Voice;
use crate::objects::layers::MotionMut;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::HitChance;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::sprite::Graphic;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{OrderKind, UnitAi};
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::parts::PartTable;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;
use genie_model::model::Model;

mod attack_state;
mod constructor;
mod hits;
mod ready_to_attack;
mod restart;
mod sprites;
mod states;
mod unit;
mod unit_ai_side;
mod update;
pub use self::update::Armament;
mod walk_and_swing;
mod walks;
mod work_slots;

pub use self::attack_state::AttackState;
pub(crate) use self::constructor::BeginLayer;
pub use self::constructor::{arms, arms_looking, engagement};
use self::hits::{AfterHit, after_hit, blast_so_far};
pub(crate) use self::hits::{Hit, strike};
pub use self::ready_to_attack::AtOnce;
#[cfg(test)]
use self::ready_to_attack::in_range;
pub(crate) use self::ready_to_attack::target_in_sight;
#[cfg(test)]
use self::sprites::SpriteReset;
pub(crate) use self::sprites::{Aim, face_target, reset_sprite, show_arrived};
use self::sprites::{NO_OBJECT_STREAM, set_attack_timer};
pub use self::sprites::{SwingSprites, swing_sprites};
pub(crate) use self::unit::run_down_attack_timers;
use self::update::squared_reach;
use self::walks::move_reads;
pub use self::work_slots::{Mark, Wielder, Worked};
#[cfg(test)]
use crate::combat::damage;
use crate::combat::volley::armed;
use crate::combat::{AttackTargetId, MissileCount};
use crate::math::image::attack::{EPSILON as ATTACK_EPSILON, NONE as ATTACK_NONE};
#[cfg(test)]
use crate::movement::Mover;
use crate::objects::layers::Combat;
use crate::objects::masters::MasterTables;
#[cfg(test)]
use crate::objects::sprite::ActiveSprite;
use crate::objects::{Frame, Graphics};
use crate::unit_ai::NotifyKind;
use crate::world::layer::Roles;
use crate::world::layer::generated::TurnTuning;
use crate::world::objects_ref::ObjectsRef;

/// The Attack action's type, which word 9 adds.
pub const ATTACK: ActionKind = ActionKind::named(9);

/// An Attack whose target the map does not hold (none, or one gone).
pub(super) const TARGET_GONE: GenieError =
    GenieError::unsupported("an Attack whose target is gone");

/// A range at most this counts as none (`DAT_142eb5690`): the range test widens the gap by
/// `-0.5` (`DAT_142eb6e58`) and the reach by `0.5` (`DAT_142e7f384`).
const NO_RANGE: f32 = 0.1;
const HALF: f32 = 0.5;
/// The gap below which the game takes a distance as zero (`DAT_142eb0c60`).
const EPSILON: f32 = ATTACK_EPSILON;
/// The point an attack on an object has none of (`DAT_14330ec20`).
const NONE: f32 = ATTACK_NONE;
/// The hit chance's scale (`0x7fffffff`, the low word of a 64-bit Object-stream draw).
const DRAW_SCALE: i64 = 0x7fff_ffff;

/// The sprites and frames the Attack takes from its constructor.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Arms {
    /// `+0x68`: the sprite its `MoveTo` shows (its constructor's argument: the hunt's walking
    /// sprite, the task's moving one; else the master's running one, slot 0x7d0, else its
    /// walking one, slot 0x7c8).
    pub(crate) move_sprite: Option<Graphic>,
    /// `+0x70`: the attack sprite (the hunt's task's proceeding sprite); `+0x78`, a second one
    /// the unit picks from; `+0x80`, the sprite it stands with when done; `+0x88`, the sprite it
    /// waits with between swings (the master's idle attack sprite).
    pub(crate) sprite: Option<Graphic>,
    /// `+0x78`: a second attack sprite the unit picks from.
    pub(crate) second_sprite: Option<Graphic>,
    /// `+0x80`: the sprite it stands with when done.
    pub(crate) idle_sprite: Option<Graphic>,
    /// `+0x88`: the sprite it waits with between swings.
    pub(crate) idle_attack_sprite: Option<Graphic>,
    /// `+0x98`, the master's missile (`missile_id`, -1 for a melee unit); `+0x9a`, the frame that
    /// fires (`fire_missile_at_frame`); `+0x9c`, the frame that may interrupt
    /// (`interrupt_frame`), -1 for none.
    pub(crate) missile: Option<MasterId>,
    /// `+0x98`: the frame it fires at (`fire_missile_at_frame`).
    pub(crate) fire_frame: Frame,
    /// `+0x9c`: the frame that may interrupt (`interrupt_frame`), −1 for none.
    pub(crate) interrupt_frame: Frame,
}

/// The Attack action (`RGE_Action_Attack`, type 9) as the hunt's sub-action.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Attack {
    /// The target object (`+0x20`, its id at `+0x30`).
    pub(crate) target: Option<AttackTargetId>,
    /// The second target (`+0x28`, its id at `+0x34`): the work command's Attack takes its
    /// target as this too (`FUN_140a7a190`, slot 0x178), −1 for none.
    pub(crate) target2: Option<AttackTargetId>,
    /// The action state (`+0x18`), with its walk's quiet `MoveTo` ([`AttackState`]).
    pub(crate) state: AttackState,
    /// `+0x44`: seconds in the state, the update adds dt.
    pub(crate) timer: Seconds,
    /// `+0x90`: the range it attacks from; `+0x94`, the least.
    pub(crate) range: Distance,
    /// `+0x94`: the least range it attacks from.
    pub(crate) min_range: Distance,
    /// `+0x38`..`+0x40`: the point it attacks; -1 for none.
    pub(crate) point: Position,
    /// `+0xa4`..`+0xac`: where it last saw its target.
    pub(crate) seen: Position,
    /// `+0x9e`: a hit is still to land in this swing.
    pub(crate) pending: bool,
    /// `+0xb0` and `+0xb4`: the time of a swing past its sprite's length; a melee swing counts
    /// `+0xb0` down until the hit.
    #[diff(
        gap = "differs from the save (MarvinBlue/426048731's 9181 at update 1025: genie -0.34, game 0)"
    )]
    pub(crate) swing: Seconds,
    /// `+0xb4`: the rest after the swing's sprite.
    #[diff(
        gap = "differs from the save (MarvinBlue/426048731's 10029 at update 1150: genie 0.5, game 0)"
    )]
    pub(crate) rest: Seconds,
    /// The sprites and frames from its constructor.
    pub(crate) arms: Arms,
    /// `+0x60`, `subActionValue` ([`Voice`]): quiet as another's sub-action (the Hunt's), which
    /// posts no notifies; a unit's own Attack is loud and posts them to its unit AI (slot 0x328).
    pub(crate) voice: Voice,
    /// `+0x61`, `mTargetMovedState`, which its `MoveTo` takes ([`TargetMoved`]): a unit's own
    /// Attack's follows, the Hunt's asks for a path at once (its `set_state` 12 sets it).
    pub(crate) on_target_moved: TargetMoved,
    /// `+0x9f`, `was_same_owner` (saved): its target was its unit's player's when the
    /// constructor (`FUN_140a7c210`) took it; the update's ally test spares such a livestock or
    /// farm target ([`Attack::update`]).
    pub(crate) same_owner: bool,
}

/// A notify an Attack's start posts to its unit's AI: its kind and three parameters.
pub(crate) type Posted = (NotifyKind, [i32; 3]);

/// What the start of a unit's own Attack reads ([`Attack::start_own`]: the attack command's, the
/// attack task's, a deletion's restart): its target, what it engages, the target's sight and
/// owner, and what the start's `set_state(6)` reads.
#[derive(Clone, Debug)]
pub(crate) struct OwnStart<'world> {
    /// The target.
    pub(crate) target: ObjectId,
    /// What the Attack reads of its unit and its target as it starts.
    pub(crate) engagement: &'world Engagement,
    /// The target is in the unit's player's sight ([`target_in_sight`]).
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

/// What readyToAttack reads at the start ([`Attack::begin`]) besides the unit's mover: what the
/// Attack engages, the gap its reach is to (the target's, a gate's nearest of it and its first two
/// parts, [`crate::movement::gate_gap`]), and the target's sight and owner.
#[derive(Clone, Copy, Debug)]
struct BeginReads<'reads> {
    /// What the Attack reads of its unit and its target as it starts.
    engagement: &'reads Engagement,
    /// The gap the reach is to.
    reach_gap: Distance,
    /// The target is in the unit's player's sight ([`target_in_sight`]).
    visible: bool,
    /// The target's owner.
    target_owner: PlayerId,
}

/// readyToAttack's answer to the start ([`Attack::begin`]), which the start's end follows
/// ([`Attack::start_own`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Begun {
    /// The unit can move: its `MoveTo` started.
    Moves {
        /// How its last `MoveTo` started.
        start: MoveStart,
        /// readyToAttack's `set_state(13)` came before the start's `set_state(4)`.
        failed_first: bool,
    },
    /// The unit cannot move, and the start ends in `set_state(6)`.
    Ready,
    /// The unit cannot move, and the start ends in `set_state(13)`.
    Failed,
}

impl Begun {
    /// How the unit's last `MoveTo` started: under way for a unit that cannot move.
    const fn start(self) -> MoveStart {
        match self {
            Self::Moves { start, .. } => start,
            Self::Ready | Self::Failed => MoveStart::UnderWay,
        }
    }

    /// readyToAttack's `set_state(13)` came before the start's `set_state(4)`.
    const fn failed_first(self) -> bool {
        matches!(
            self,
            Self::Moves {
                failed_first: true,
                ..
            }
        )
    }
}

/// What the Attack reads of its unit and its target when it starts.
#[derive(Clone, Copy, Debug)]
pub struct Engagement {
    /// The gap between the two footprints (`FUN_14118f680`).
    pub(crate) gap: Distance,
    /// The unit's and the target's positions.
    pub(crate) position: Position,
    /// The target's place.
    pub(crate) target_position: Position,
    /// The unit's top speed (slot 0x208), its master's weapon range with any bonus against the
    /// target (slot 0x238) and its least range.
    pub(crate) speed: Speed,
    /// Its master's weapon range with any bonus against the target (slot 0x238).
    pub(crate) range: Distance,
    /// Its least range.
    pub(crate) min_range: Distance,
    /// The unit AI's `canMoveToAttack` (`FUN_140c48420`): it has an order, or no order but a stance
    /// below 2.
    pub(crate) can_move: bool,
    /// The unit AI's stance (`+0x1a9`), whose 1 adds a leash test.
    pub(crate) stance: UnitStance,
    /// The unit AI's `state_position` (`+0x1ac`, `+0x1b0`), the leash's centre.
    pub(crate) state_position: Point,
    /// The world's time (`FUN_1415300d0`), which a facing over time reads.
    pub(crate) world_time: WorldTime,
    /// The unit's master, of which 1704 and 1706 do not turn to face their target.
    pub(crate) master: MasterId,
    /// The target's master's class, whose 39 (a gate) readyToAttack measures by its parts.
    pub(crate) target_class: ClassId,
    /// The target is the unit's player's (the constructor's `+0x9f`, [`Attack::same_owner`]).
    pub(crate) same_owner: bool,
    /// The unit's master moves (`FUN_140fdd080`, [`Master::moves`]), which readyToAttack's
    /// ready-at-once test reads.
    pub(crate) moves: bool,
    /// `next_volley` (`+0xa04`): the missiles of the volley under way already fired, which
    /// readyToAttack's ready-at-once test reads (`FUN_1412161a0`).
    pub(crate) next_volley: MissileCount,
}

bitflags::bitflags! {
    /// What the Attack reads of its unit, its master and its unit AI as yes or no
    /// ([`AttackReads`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct AttackerFlags: u8 {
        /// `FUN_14120ea30` for a building: it can attack (class 51 or 54, a missile, or a firepower
        /// above 1); true for the others.
        const ARMED = 1;
        /// The unit AI's canMoveToAttack (slot 0x1d0, `FUN_140c48420`), which readyToAttack reads
        /// of a target out of reach.
        const CAN_MOVE = 1 << 1;
        /// The unit has a unit AI whose order queue is empty (`FUN_140c564a0` is 0).
        const QUEUE_EMPTY = 1 << 2;
    }
}

/// What the Attack's update reads of its unit, its master, its unit AI and its owner, copied as
/// the update starts ([`AttackReads::of`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct AttackReads {
    /// Its master's standing sprite (`+0x470`), which slot 0x68 shows for no sprite
    /// (`FUN_141158620`).
    pub(crate) standing: Option<Graphic>,
    /// Its yes-or-no reads.
    pub(crate) flags: AttackerFlags,
    /// Slot 0x218 (`FUN_141086c90`): the master's reload time (`speed_of_attack`) with the
    /// unit's reload stinger and powerup.
    pub(crate) reload: Seconds,
    /// The master's hit chance (`+0x83e`).
    pub(crate) hit_chance: HitChance,
    /// The swing's own sprites ([`Attack::set_swing`]): its terrain look's attack sprite and its
    /// master's special graphic.
    pub(crate) swing_sprites: SwingSprites,
    /// The unit's player's type (`player+0x10`; [`PlayerKind::is_computer`] is
    /// `FUN_141659cf0`).
    pub(crate) player_kind: PlayerKind,
    /// The world's time (`FUN_1415300d0`), which a facing over time and the reload read.
    pub(crate) world_time: WorldTime,
    /// The unit AI's stance (`+0x1a9`).
    pub(crate) stance: UnitStance,
    /// The unit AI's `state_position` (`+0x1ac`, `+0x1b0`), the centre of stance 1's leash.
    pub(crate) state_position: Point,
    /// The unit AI's order (`+0x3c`, `FUN_140c4cc20`).
    pub(crate) ai_order: AiOrder,
    /// The unit AI's factory type, None without a unit AI.
    pub(crate) ai_kind: Option<UnitAiKind>,
    /// `next_volley` (`+0xa04`): the missiles of the volley under way already fired, which
    /// `FUN_1412161a0` tests for 0.
    pub(crate) next_volley: MissileCount,
    /// The sprites a `MoveTo` of the Attack takes when the Attack gives none: the defaults of the
    /// unit's master and terrain look.
    pub(crate) move_defaults: MoveDefaults,
    /// What its top speed reads besides its own speed and its place, as the update starts
    /// ([`TopSpeed`]): the `MoveTo`s it makes read whether the unit has a speed (slot 0x208).
    /// Within the unit's own update its lock, powerup and stinger stay, and a ready charge's run
    /// multiplies by a factor that is not 0, which keeps whether the speed is 0.
    pub(crate) top_speed: TopSpeed,
}

/// What the Attack's update reads of its unit as it starts ([`AttackReads::of`]): its combat layer
/// and its top speed's terms, its master and the sprites' table, its unit AI, its owner's type and
/// the world's time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AttackSource<'world> {
    /// The unit's combat layer.
    pub(crate) combat: &'world Combat,
    /// What its top speed reads besides its own speed and its place (slot 0x208).
    pub(crate) top_speed: TopSpeed,
    /// Its master.
    pub(crate) master: &'world Master,
    /// The sprites' table.
    pub(crate) graphics: &'world Graphics,
    /// Its unit AI: its own, or the one running outside it.
    pub(crate) ai: Option<&'world UnitAi>,
    /// Its owner's type (`player+0x10`).
    pub(crate) player_kind: PlayerKind,
    /// The world's time.
    pub(crate) now: WorldTime,
}

impl AttackReads {
    /// What the Attack of a unit of `master` with the combat layer `combat` and its top speed's
    /// terms `top_speed` reads: the sprites' table (`graphics`), its unit AI `ai` (the unit's
    /// own, or the one running outside it), whether its owner is a computer player and the
    /// world's time (`now`).
    pub(crate) fn of(source: AttackSource<'_>, roles: &Roles, turn_tuning: &TurnTuning) -> Self {
        let AttackSource {
            combat,
            top_speed,
            master,
            graphics,
            ai,
            player_kind,
            now,
        } = source;
        Self {
            standing: master
                .standing_sprite
                .and_then(|sprite_id| graphics.get(sprite_id))
                .copied(),
            flags: {
                let mut flags = AttackerFlags::empty();
                flags.set(
                    AttackerFlags::ARMED,
                    armed(master, combat.firepower, roles, turn_tuning),
                );
                flags.set(
                    AttackerFlags::CAN_MOVE,
                    ai.is_some_and(|ai| {
                        ai.current_order.is_some() || ai.stance <= UnitStance::Defensive
                    }),
                );
                flags.set(
                    AttackerFlags::QUEUE_EMPTY,
                    ai.is_some_and(|ai| ai.orders.is_empty()),
                );
                flags
            },
            reload: reload_time(
                master.kind,
                master.combat_part().reload,
                combat.reload_stinger,
                combat.reload_powerup,
            ),
            hit_chance: master.combat_part().hit_chance,
            swing_sprites: swing_sprites(combat.abilities.look.as_ref(), master, graphics),
            player_kind,
            world_time: now,
            stance: ai.map_or(UnitStance::Aggressive, |ai| ai.stance),
            state_position: ai.map_or(Point::ORIGIN, |ai| ai.state_position),
            ai_order: AiOrder::of(ai),
            ai_kind: ai.map(|ai| ai.ai_kind),
            next_volley: combat.next_volley,
            move_defaults: MoveDefaults::of(master, combat.abilities.look.as_ref(), roles),
            top_speed,
        }
    }
}

/// The unit AI's order as the Attack reads it (`+0x3c`, `FUN_140c4cc20`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
pub(crate) struct Ground<'world> {
    /// The map.
    pub(crate) map: &'world Spatial,
    /// The Sprite stream, which a new sprite draws from.
    pub(crate) sprite_rng: &'world mut Well512,
    /// The Object stream, which a swing's second sprite draws from.
    pub(crate) object_rng: Option<&'world mut Well512>,
    /// Where the unit's notifies go.
    pub(crate) posts: Posts<'world>,
    /// The sprites' table, which a `MoveTo` the Attack makes takes its sprite from.
    pub(crate) graphics: &'world Graphics,
    /// The objects by id: a gate's annexes, which its reach measures.
    pub(crate) by_id: ObjectsRef<'world>,
    /// The unit's own motion layer (its mover and animation speed), lent out of the world's
    /// table while `by_id` reads the others' (FD-045-D58).
    pub(crate) motion: MotionMut<'world>,
}

/// Where [`Ground::post`] sends the unit's notifies.
pub(crate) enum Posts<'world> {
    /// The list the unit AI posts itself while it is out of the world's table.
    Held(&'world mut Vec<(NotifyKind, [i32; 3])>),
    /// The world's unit AIs: each notify goes to the unit's at once.
    Ais(&'world mut PartTable<UnitAi>),
}

impl Ground<'_> {
    /// The unit's notify of `kind` with `params` (slot 0x328): to the list while its unit AI is out
    /// of the world's table, else to its unit AI at once.
    ///
    /// # Errors
    ///
    /// The notify's.
    pub(crate) fn post(
        &mut self,
        unit: &Unit,
        kind: NotifyKind,
        params: [i32; 3],
    ) -> GenieResult<()> {
        match &mut self.posts {
            Posts::Held(list) => {
                list.push((kind, params));
                Ok(())
            }
            Posts::Ais(ais) => unit.post_own(ais, kind, params),
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
            players,
            index,
            parts,
            model:
                Model {
                    graphics,
                    masters,
                    roles,
                    tuning,
                    ..
                },
            treaty,
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
            by_id: ObjectsRef::of_tables(
                players,
                index,
                MasterTables::new(masters, &parts.own_master),
                (
                    &parts.building,
                    &parts.combat,
                    &parts.actions,
                    &parts.unread,
                    motions,
                ),
                roles,
                tuning,
                treaty.as_ref(),
            ),
            motion: MotionMut::new(motion),
        }
    }
}

/// The unit's animation speed (`+0x7b8`) as the Attack sets it: a mover's; a building's stays
/// (MISSING.md, "The Attack's animation speed of a building").
pub(crate) fn set_walker_anim(mut motion: MotionMut<'_>, speed: Speed) {
    if let Some(mover) = motion.mover_mut() {
        mover.animated.speed = speed;
    }
}

/// An Attack in a state that walks to its target object, without one: genie enters
/// `set_state(4)` only with its target found ([`Attack::start`]'s engagement, readyToAttack's
/// follow of the victim); the game's walk without one (`0x140a820c0` case 4: to `+0x38`, else
/// `+0xa4`, else notify 0x1f9) is MISSING.md's.
const ATTACK_WITHOUT_TARGET: GenieError =
    GenieError::unreachable("an Attack's set_state(4) without its target");
/// An Attack's unit without a standing sprite.
const STANDING_SPRITE_LACKING: GenieError =
    GenieError::unsupported("an Attack's unit without a standing sprite");
/// An attack's target the slice does not hold among the live objects.
pub(crate) const ATTACK_TARGET_NOT_LIVE: GenieError =
    GenieError::unsupported("an attack's target the slice does not hold live");
/// An attacker whose charge adds to its range (slot 0x2f8).
pub(crate) const CHARGE_RANGE: GenieError =
    GenieError::unsupported("an attacker whose charge adds to its range (slot 0x2f8)");

/// What readyToAttack answers: ready to swing, waiting for the attack timer, or the target out of
/// reach at this gap (the unit follows it).
enum Readiness {
    Ready,
    Waiting,
    Away(Distance),
}

#[cfg(test)]
pub(crate) mod tests;
