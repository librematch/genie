//! The Attack's constructor (`FUN_140a7c210`) and its start (slot 5): its sprites and frames
//! and what it reads of its unit and its target.

use crate::GenieError;
use crate::combat::AttackTargetId;
use crate::combat::abilities::Look;
use crate::combat::attack::AttackState;
use crate::combat::attack::TARGET_GONE;
use crate::combat::attack::{
    Arms, Attack, AttackCommand, AttackReads, Begun, Engagement, Fight, Ground, NONE, Posted,
    Posts, face_target,
};
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::spatial::{Placed, Spatial};
use crate::math::fmath::{at_most_or_unordered, sqrt};
use crate::math::rng::{Stream, Well512};
use crate::movement::MoveStart;
use crate::movement::Speed;
use crate::movement::gate_gap;
use crate::movement::move_to::{MoveToReads, TargetMoved};
use crate::movement::{Mover, footprint_gap};
use crate::objects::Graphics;
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::layers::MotionMut;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::{Master, MasterTables, OwnerMasters};
use crate::objects::sprite::ActiveSprite;
use crate::objects::sprite::Cargo;
use crate::objects::sprite::new_sprite;
use crate::objects::sprite::{running, walking};
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::UnitStance;
use crate::unit_ai::{NotifyKind, UnitAi};
use crate::world::ATTACKER_UNMOVING;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::AttackTuning;
use crate::world::layer::generated::MoveToTuning;
use crate::world::layer::generated::SpriteTuning;
use crate::world::step::Step;

/// What an Attack's start reads of the build layer (FD-046-D45, D49): the roles of the unit's
/// master, the attack's tuning values and its `MoveTo`'s, one argument of [`Attack::start`] and
/// [`Attack::begin`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct BeginLayer<'a> {
    /// The build's roles.
    pub(crate) roles: &'a Roles,
    /// The attack's tuning values.
    pub(crate) attack_tuning: &'a AttackTuning,
    /// The `MoveTo`'s tuning values.
    pub(crate) move_to_tuning: &'a MoveToTuning,
}

impl Attack {
    /// The constructor (`FUN_140a7c210`) on `target`: the range and least range of `e`, its
    /// target's place as where it was seen (`+0xa4`..`+0xac`), the point −1 on each axis, the
    /// sprites and frames of `arms`, whether the target is its unit's player's (`+0x9f`); a
    /// unit's own Attack (`+0x60` 0), its `MoveTo` not repathing at once (`+0x61` 0) and running
    /// (slot 0x7d0's sprite).
    pub(crate) fn own(target: ObjectId, e: &Engagement, arms: Arms) -> Self {
        Self {
            target: Some(AttackTargetId::new(target)),
            target2: None,
            state: AttackState::New,
            timer: Seconds::ZERO,
            range: e.range,
            min_range: e.min_range,
            point: Position::from([NONE; 3]),
            seen: e.target_position,
            pending: false,
            swing: Seconds::ZERO,
            rest: Seconds::ZERO,
            arms,
            voice: Voice::Loud,
            on_target_moved: TargetMoved::Follows,
            same_owner: e.same_owner,
        }
    }

    /// The constructor (`FUN_140a7c210`, from Hunt's `set_state` 12 with the master's weapon range,
    /// least range, missile and frame: a quiet sub-action, `+0x61` set, its `MoveTo` showing the
    /// hunt's sprite) and the start (slot 5 with 1, `0x140a7f8c0`, [`Attack::begin`]); the target
    /// is the object itself (the area-effect and gate cases are not reached by a villager) and in
    /// sight (a hunt's targets are, and none is a gate, whose parts' gaps the hunt's callers do
    /// not hand on). The unit's `+0x900` (attack count) is cleared and its sprites
    /// set; the slice holds neither. [`MoveStart::Arrived`] with it: the last `MoveTo` started
    /// arrived, and the caller shows its idle sprite ([`Attack::arrived_sprite`]).
    ///
    /// # Errors
    ///
    /// What the start does not model, and what no hunt's caller takes: a hunter that cannot move
    /// (its Attack's `set_state` 6 or 13 at the start), and readyToAttack's failure before the
    /// start's `set_state(4)` (the unit's `+0x7b8` at 1, as [`Attack::command`] leaves it).
    pub(crate) fn start(
        target: ObjectId,
        e: &Engagement,
        arms: Arms,
        mover: &mut Mover,
        reads: &MoveToReads<'_>,
        layer: BeginLayer<'_>,
    ) -> Result<(Self, MoveStart), GenieError> {
        let roles = layer.roles;
        let mut a = Self {
            voice: Voice::Quiet,
            on_target_moved: TargetMoved::AsksAgain,
            ..Self::own(target, e, arms)
        };
        // The gate class, whose parts' gaps readyToAttack measures.
        // The villagers' class, whose attack ends on any dead target.
        // The trees' class, a dead one of which up to state 5 keeps another unit's attack going.
        if roles.of_class(e.target_class).contains(ClassRoles::GATE) {
            return Err(GenieError::unsupported(
                "a hunt's Attack on a gate (its parts' gaps)",
            ));
        }
        let mut posted = Vec::new();
        let begun = a.begin(
            (e, e.gap),
            (true, PlayerId::GAIA),
            mover,
            &mut posted,
            reads,
            layer,
        )?;
        if begun == Begun::Ready {
            return Err(GenieError::unsupported(
                "an attack that starts in reach and cannot move (set_state 6)",
            ));
        }
        if begun == Begun::Failed {
            return Err(GenieError::unsupported(
                "an attack whose unit cannot move to its target",
            ));
        }
        if begun.failed_first() {
            return Err(GenieError::unsupported(
                "a hunt's Attack readyToAttack failed before its start's state 4 (+0x7b8 at 1)",
            ));
        }
        Ok((a, begun.start()))
    }

    /// A unit's own Attack (the attack command, `FUN_141081a80`): the constructor
    /// (`FUN_140a7c210` with sub-action 0: the master's weapon range and least range, its
    /// sprites, missile and frames, its `MoveTo`'s the running one, slot 0x7d0, or the walking
    /// one: the running speed either way; `+0x61` clear) and the start's readyToAttack with its
    /// `set_state(4)` ([`Attack::begin`]) on `cmd`'s target in `world` (a gate's reach is to its
    /// nearest part too): the Attack, readyToAttack's answer and the notifies it posted.
    ///
    /// # Errors
    ///
    /// What the start does not model, and a target the world does not hold.
    pub(super) fn own_start(
        cmd: &AttackCommand<'_>,
        (world, by_id): (&Spatial, ObjectsRef<'_>),
        mover: &mut Mover,
        reads: &MoveToReads<'_>,
    ) -> Result<(Self, Begun, Vec<Posted>), GenieError> {
        let move_to_tuning = &by_id.tuning().move_to;
        let attack_tuning = &by_id.tuning().attack;
        let roles = by_id.roles();
        let e = cmd.engagement;
        let t = *world.get_known(cmd.target)?.ok_or(TARGET_GONE)?;
        let ground = mover.stats.radius.ground();
        let reach_gap = gate_gap(
            world,
            (e.position, ground),
            (cmd.target, &t),
            e.gap,
            |g| g,
            by_id,
            roles,
        );
        let mut attack = Self::own(cmd.target, e, cmd.arms);
        let mut posted = Vec::new();
        let begun = attack.begin(
            (e, reach_gap),
            (cmd.visible, cmd.target_owner),
            mover,
            &mut posted,
            reads,
            BeginLayer {
                roles,
                attack_tuning,
                move_to_tuning,
            },
        )?;
        Ok((attack, begun, posted))
    }

    /// The attack command's Attack of `unit` (the step's, out of the world): the constructor and
    /// readyToAttack with the start's `set_state(4)` ([`Attack::own_start`]), then the start's
    /// other ends at once (`0x140a7f8c0`): readyToAttack's `set_state(13)` before the
    /// `set_state(4)` that replaced it leaves the unit's animation at speed 1 (`+0x7b8`; the
    /// standing sprite it shows is the start's already); a unit that cannot move fails out of reach
    /// (`set_state(13)`, [`Attack::fail_at_start`]) or goes ready in it (`set_state(6)`,
    /// [`Attack::enter_ready`] with `cmd`'s [`ReadyReads`](crate::combat::attack::ReadyReads)); a
    /// `MoveTo` that starts arrived shows the standing sprite at its speed and clears `+0x7b8`
    /// (`0x140aa29e0`, not quiet). The Attack and the notifies the start posted, in order, for the
    /// unit's AI (the swing's of `set_state(6)` among them).
    ///
    /// # Errors
    ///
    /// What the start does not model, a target the world does not hold, a unit without its
    /// movement state, and a start that goes ready on a target the world does not hold live.
    pub(crate) fn command(
        c: &mut Step<'_>,
        unit: &mut Unit,
        cmd: &AttackCommand<'_>,
        roles: &Roles,
    ) -> Result<(Self, Vec<Posted>), GenieError> {
        let reads = unit.move_to_reads(
            c.w.parts.motion.mover(unit.id),
            MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
            c.player,
            (
                &c.w.model.graphics,
                c.w.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        let (mover, by_id) =
            c.w.parts
                .lend_mover(unit.id, (&c.w.players, &c.w.index, &c.w.model));
        let mover = mover.ok_or(ATTACKER_UNMOVING)?;
        let (mut attack, begun, mut posted) =
            Self::own_start(cmd, (&c.w.spatial, by_id), mover, &reads)?;
        if begun.failed_first() {
            c.w.parts.set_anim_speed(unit.id, Speed::new(1.0));
        }
        if begun == Begun::Failed {
            attack.fail_at_start(
                &mut unit.sprite,
                c.w.parts.motion_mut(unit.id),
                c.w.streams.get(Stream::Sprite),
                c.w.model.tuning.sprite,
            )?;
        }
        if begun == Begun::Ready {
            let ready = cmd.ready.as_ref().ok_or(GenieError::unsupported(
                "an attack's target the slice does not hold live",
            ))?;
            let (motion, by_id) =
                c.w.parts
                    .lend_motion(unit.id, (&c.w.players, &c.w.index, &c.w.model));
            let mut g = Ground {
                map: &c.w.spatial,
                sprite_rng: c.w.streams.get(Stream::Sprite),
                object_rng: None,
                posts: Posts::Held(&mut posted),
                graphics: &c.w.model.graphics,
                by_id,
                motion,
            };
            attack.enter_ready(unit, &mut g, &ready.fight, &ready.reads)?;
        }
        if begun.start() == MoveStart::Arrived {
            unit.shows_standing_sprite(
                c.w.parts.motion.layer_mut(unit.id),
                OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
                &c.w.model.graphics,
                c.w.streams.get(Stream::Sprite),
                c.w.model.tuning.sprite,
            )?;
        }
        Ok((attack, posted))
    }

    /// The start (slot 5 with 1, `0x140a7f8c0`) after its unit AI and place test, its `+0x900`
    /// and its standing sprite (the caller's): readyToAttack (`FUN_140a7f9d0`, third argument 0),
    /// then a unit with a top speed (slot 0x208, `UCOMISS`: a NaN one has some) whose unit AI may
    /// move to attack (slot 0x1d0) turns to state 4 (`set_state(4)`: a `MoveTo` to the target, the
    /// old one's teardown stopping the unit); any other goes ready (`set_state(6)`,
    /// [`Begun::Ready`]) when readyToAttack reached its ready end (its second argument 1), else
    /// fails (`set_state(13)`, [`Begun::Failed`]).
    ///
    /// readyToAttack at the start: a target of a player (`target_owner` above 0) out of the
    /// player's sight (`visible` false: its slot 0x1e0 neither 0xf nor 0x80 with its master's fog
    /// flag) makes a unit of stance 0 with a speed above 0 (`COMISS`/`JBE`: not a NaN one) whose unit AI
    /// may move to attack go where it was seen (`set_state(5)`: a `MoveTo` to that point, which has
    /// no target, so `f32::MAX` from it at its start: state 3), any other fails
    /// ([`Begun::failed_first`]). The constructor's point (−1 on each axis) is never the target's
    /// place, nor within half a tile of it, so no ready-at-once test passes and the reach
    /// decides, by the gap (`FUN_14118f680`): below the least range (`COMISS`/`JA`) a unit's own
    /// Attack posts 0x1fe (−1, −1, −1) and a unit that may not move fails; beyond the reach (the
    /// range, half a tile more up to [`NO_RANGE`]; `COMISS`/`JBE`: a NaN gap is within) it posts
    /// 0x1fd, then stance 1's leash fails a target `AttackTuning::leash_slack` past the range from the unit
    /// AI's `state_position` (`FUN_14099ef70`'s root, `COMISS`/`JNC`: a NaN distance holds), a unit
    /// that may move turns to state 4, any other fails. The reach is to `reach_gap`, a gate's
    /// nearest of it and its first two parts ([`crate::movement::gate_gap`]). Within it the
    /// target's place becomes the point and the unit faces it (`FUN_141162db0`, but masters 1704
    /// and 1706): its ready end.
    ///
    /// [`NO_RANGE`]: crate::combat::attack::NO_RANGE
    pub(super) fn begin(
        &mut self,
        (e, reach_gap): (&Engagement, Distance),
        (visible, target_owner): (bool, PlayerId),
        mover: &mut Mover,
        posted: &mut Vec<(NotifyKind, [i32; 3])>,
        reads: &MoveToReads<'_>,
        layer: BeginLayer<'_>,
    ) -> Result<Begun, GenieError> {
        let BeginLayer {
            roles,
            attack_tuning,
            move_to_tuning,
        } = layer;
        let movable = e.speed != Speed::ZERO && e.can_move;
        let mut failed_first = false;
        let mut ready_end = false;
        if target_owner != PlayerId::GAIA && !visible {
            if e.stance == UnitStance::Aggressive && Speed::ZERO < e.speed && e.can_move {
                self.state = AttackState::GotoSeen { move_to: None };
                self.range = e.range;
                self.walk_to_seen(
                    |move_to| AttackState::GotoSeen { move_to },
                    reads,
                    move_to_tuning,
                )?;
            } else {
                failed_first = true;
            }
        } else if self.min_range > e.gap {
            if self.voice.is_loud() {
                posted.push((NotifyKind::TOO_CLOSE, [-1; 3]));
            }
            failed_first = !movable;
        } else if !at_most_or_unordered(reach_gap.get(), self.reach().get()) {
            if self.voice.is_loud() {
                posted.push((NotifyKind::OUT_OF_REACH, [0; 3]));
            }
            if (e.stance == UnitStance::Defensive
                && self.off_leash(e.target_position, e.state_position, attack_tuning))
                || !movable
            {
                failed_first = true;
            } else {
                let _ = self.set_move(e, mover, reads, move_to_tuning)?;
            }
        } else {
            self.point = e.target_position;
            if !roles
                .of_master(e.master)
                .contains(MasterRoles::UNTURNED_ATTACKER)
            {
                let _ = face_target(mover, e.position, e.target_position, e.world_time)?;
            }
            ready_end = true;
        }
        if movable {
            let start = self.set_move(e, mover, reads, move_to_tuning)?;
            return Ok(Begun::Moves {
                start,
                failed_first,
            });
        }
        Ok(if ready_end {
            Begun::Ready
        } else {
            Begun::Failed
        })
    }

    /// readyToAttack's leash of stance 1 on a target at `at` out of reach: its distance from the
    /// unit AI's `state_position` (`+0x1ac`, `+0x1b0`; `FUN_14099ef70` of y's square plus x's) at
    /// least the range plus `AttackTuning::leash_slack` (`COMISS`/`JNC`: a NaN distance is within).
    pub(super) fn off_leash(
        &self,
        at: Position,
        centre: Point,
        attack_tuning: &AttackTuning,
    ) -> bool {
        let (dx, dy) = (at.x() - centre.x(), at.y() - centre.y());
        sqrt(dy * dy + dx * dx) >= (self.range + Distance::new(attack_tuning.leash_slack)).get()
    }

    /// The start's `set_state(13)` for a unit that cannot move out of reach of its target
    /// ([`Attack::command`]), as [`Attack::finish_in`]: the state, the unit's animation at speed
    /// 1 and its standing sprite (a unit that cannot move has no `MoveTo` to stop). Its next update
    /// posts 0x1f9 and removes it.
    ///
    /// # Errors
    ///
    /// When the unit's master has no standing sprite.
    pub fn fail_at_start(
        &mut self,
        sprite: &mut Option<ActiveSprite>,
        mut motion: MotionMut<'_>,
        rng: &mut Well512,
        sprite_tuning: SpriteTuning,
    ) -> Result<(), GenieError> {
        // State 13 ("Failed"): the target went out of sight.
        self.state = AttackState::Failed;
        motion.set_anim_speed(Speed::new(1.0));
        let idle = self.arms.idle_sprite.ok_or(GenieError::unsupported(
            "an Attack's unit without a standing sprite",
        ))?;
        new_sprite(sprite, motion.move_stats_mut(), idle, rng, sprite_tuning);
        Ok(())
    }

    /// The start's `set_state(6)` for a unit that cannot move ([`Attack::command`]): the range
    /// test again, and the swing when it is ready, on the world's parts `g`.
    ///
    /// # Errors
    ///
    /// What the range test and the swing do not model.
    pub(crate) fn enter_ready(
        &mut self,
        unit: &mut Unit,
        g: &mut Ground<'_>,
        f: &Fight,
        r: &AttackReads,
    ) -> Result<(), GenieError> {
        self.ready(unit, g, f, r)
    }
}

/// The Attack constructor's sprites and frames (`FUN_140a7c210`) for a unit that shows its
/// master's sprites ([`arms_looking`] without a terrain look).
#[must_use]
pub fn arms(
    master: &Master,
    graphics: &Graphics,
    sprites: TaskSprites,
    moving: Option<SpriteId>,
    roles: &Roles,
) -> Arms {
    arms_looking(master, None, graphics, sprites, moving, roles)
}

/// The Attack constructor's sprites and frames (`FUN_140a7c210`): the `MoveTo`'s sprite `moving`
/// (the hunt's walking sprite from Hunt's `set_state` 12, the task's moving one from the task
/// factory, none from the attack command), else the unit's running sprite (slot 0x7d0), else its
/// walking one (slot 0x7c8; both its terrain look's while it has one, `look`,
/// [`crate::objects::sprite::running`]); the task's proceeding sprite (else the master's attack sprite,
/// `+0x7f8`), the master's second attack sprite and standing sprite, missile, firing and
/// interrupting frames.
#[must_use]
pub fn arms_looking(
    master: &Master,
    look: Option<&Look>,
    graphics: &Graphics,
    sprites: TaskSprites,
    moving: Option<SpriteId>,
    roles: &Roles,
) -> Arms {
    let g = |id: Option<SpriteId>| id.and_then(|s| graphics.get(s).copied());
    Arms {
        move_sprite: g(moving)
            .or_else(|| g(running(master, look)))
            .or_else(|| g(walking(master, look, Cargo::Empty, roles))),
        sprite: g(sprites.proceeding).or_else(|| g(master.combat_part().attack_sprite)),
        second_sprite: g(master.combat_part().attack_sprite2),
        idle_sprite: g(master.standing_sprite),
        idle_attack_sprite: g(master.creatable_part().idle_attack_sprite),
        missile: master.combat_part().missile.master(),
        fire_frame: master.combat_part().fire_frame,
        interrupt_frame: master.combat_part().interrupt_frame,
    }
}

/// What an Attack of a unit (its unit AI, top speed and master, placed at `me`) reads of it and
/// its target (placed at `them`).
pub fn engagement(
    ai: Option<&UnitAi>,
    speed: Speed,
    master: &Master,
    me: &Placed,
    them: &Placed,
    world_time: WorldTime,
) -> Result<Engagement, GenieError> {
    if master.creatable_part().charge_kind.ranged() {
        return Err(GenieError::unsupported(
            "an attacker whose charge adds to its range (slot 0x2f8)",
        ));
    }
    Ok(Engagement {
        gap: footprint_gap(me.position, me.radius, them.position, them.radius),
        position: me.position,
        target_position: them.position,
        speed,
        range: master.combat_part().weapon_range,
        min_range: master.combat_part().min_range,
        can_move: ai
            .is_some_and(|ai| ai.current_order.is_some() || ai.stance <= UnitStance::Defensive),
        stance: ai.map_or(UnitStance::Aggressive, |ai| ai.stance),
        state_position: ai.map_or(Point::ORIGIN, |ai| ai.state_position),
        world_time,
        master: master.id,
        target_class: them.class,
        same_owner: me.player == them.player,
    })
}
