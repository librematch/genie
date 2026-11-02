//! The Attack's constructor (`FUN_140a7c210`) and its start (slot 5): its sprites and frames
//! and what it reads of its unit and its target.

use crate::GenieError;
use crate::combat::attack::AttackState;
use crate::combat::attack::{
    Arms, Attack, AttackCommand, AttackReads, Begun, Engagement, Fight, Ground, LEASH_SLACK,
    MASTER_1704, MASTER_1706, NONE, Posted, Posts, face_target,
};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::math::rng::{Stream, Well512};
use crate::movement::Speed;
use crate::movement::move_to::{MoveToReads, TargetMoved};
use crate::movement::{Mover, footprint_gap, footprint_gap_squared};
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::layers::Body;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::class;
use crate::objects::sprite::ActiveSprite;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::UnitStance;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::step::Step;

impl Attack {
    /// The constructor (`FUN_140a7c210`) on `target`: the range and least range of `e`, its
    /// target's place as where it was seen (`+0xa4`..`+0xac`), the point −1 on each axis, the
    /// sprites and frames of `arms`, whether the target is its unit's player's (`+0x9f`); a
    /// unit's own Attack (`+0x60` 0), its `MoveTo` not repathing at once (`+0x61` 0) and running
    /// (slot 0x7d0's sprite).
    pub(crate) fn own(target: ObjectId, e: &Engagement, arms: Arms) -> Self {
        Self {
            target: Some(target),
            target2: None,
            state: AttackState::New,
            timer: Seconds::ZERO,
            range: e.range,
            min_range: e.min_range,
            point: Position::from([NONE; 3]),
            seen: e.target_position,
            move_to: None,
            pending: false,
            swing: Seconds::ZERO,
            rest: Seconds::ZERO,
            arms,
            sub_action: false,
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
    /// set; the slice holds neither. True with it: the last `MoveTo` started arrived, and the
    /// caller shows its idle sprite ([`Attack::arrived_sprite`]).
    ///
    /// # Errors
    ///
    /// What the start does not model, and what no hunt's caller takes: a hunter that cannot move
    /// (its Attack's `set_state` 6 or 13 at the start), and readyToAttack's failure before the
    /// start's `set_state(4)` (the unit's `+0x7b8` at 1, as [`Attack::command`] leaves it).
    pub fn start(
        target: ObjectId,
        e: &Engagement,
        arms: Arms,
        mover: &mut Mover,
        reads: &MoveToReads<'_>,
    ) -> Result<(Self, bool), GenieError> {
        let mut a = Self {
            sub_action: true,
            on_target_moved: TargetMoved::AsksAgain,
            ..Self::own(target, e, arms)
        };
        // The gate class, whose parts' gaps readyToAttack measures.
        // The villagers' class, whose attack ends on any dead target.
        // The trees' class, a dead one of which up to state 5 keeps another unit's attack going.
        if e.target_class == class::GATE {
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
        )?;
        if begun.ready {
            return Err(GenieError::unsupported(
                "an attack that starts in reach and cannot move (set_state 6)",
            ));
        }
        if begun.failed {
            return Err(GenieError::unsupported(
                "an attack whose unit cannot move to its target",
            ));
        }
        if begun.failed_first {
            return Err(GenieError::unsupported(
                "a hunt's Attack readyToAttack failed before its start's state 4 (+0x7b8 at 1)",
            ));
        }
        Ok((a, begun.arrived))
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
        (world, by_id): (
            &crate::map::spatial::Spatial,
            crate::world::index::ObjectsRef<'_>,
        ),
        mover: &mut Mover,
        reads: &MoveToReads<'_>,
    ) -> Result<(Self, Begun, Vec<Posted>), GenieError> {
        let e = cmd.engagement;
        let t = *world
            .get_known(cmd.target)?
            .ok_or(crate::combat::attack::TARGET_GONE)?;
        let ground = mover.stats.radius.ground();
        let reach_gap = crate::movement::gate_gap(
            world,
            (e.position, ground),
            (cmd.target, &t),
            e.gap,
            |g| g,
            by_id,
        );
        let mut attack = Self::own(cmd.target, e, cmd.arms);
        let mut posted = Vec::new();
        let begun = attack.begin(
            (e, reach_gap),
            (cmd.visible, cmd.target_owner),
            mover,
            &mut posted,
            reads,
        )?;
        Ok((attack, begun, posted))
    }

    /// The attack command's Attack of `unit` (the step's, out of the world): the constructor and
    /// readyToAttack with the start's `set_state(4)` ([`Attack::own_start`]), then the start's
    /// other ends at once (`0x140a7f8c0`): readyToAttack's `set_state(13)` before the
    /// `set_state(4)` that replaced it leaves the unit's animation at speed 1 (`+0x7b8`; the
    /// standing sprite it shows is the start's already); a unit that cannot move fails out of
    /// reach (`set_state(13)`, [`Attack::fail_at_start`]) or goes ready in it (`set_state(6)`,
    /// [`Attack::enter_ready`] with `cmd`'s [`ReadyReads`]); a `MoveTo` that starts arrived shows
    /// the standing sprite at its speed and clears `+0x7b8` (`0x140aa29e0`, not quiet). The
    /// Attack and the notifies the start posted, in order, for the unit's AI (the swing's of
    /// `set_state(6)` among them).
    ///
    /// # Errors
    ///
    /// What the start does not model, a target the world does not hold, a unit without its
    /// movement state, and a start that goes ready on a target the world does not hold live.
    pub(crate) fn command(
        c: &mut Step<'_>,
        unit: &mut Unit,
        cmd: &AttackCommand<'_>,
    ) -> Result<(Self, Vec<Posted>), GenieError> {
        let reads = unit.move_to_reads(
            crate::objects::masters::MasterTables::new(&c.w.masters, &c.w.parts.own_master),
            c.player,
            (
                &c.w.graphics,
                c.w.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
        )?;
        let mover = unit
            .body
            .mover_mut()
            .ok_or(crate::world::ATTACKER_UNMOVING)?;
        let (mut attack, begun, mut posted) =
            Self::own_start(cmd, (&c.w.spatial, c.w.objects()), mover, &reads)?;
        if begun.failed_first {
            unit.body.set_anim_speed(Speed::new(1.0));
        }
        if begun.failed {
            attack.fail_at_start(
                &mut unit.sprite,
                &mut unit.body,
                c.w.streams.get(Stream::Sprite),
            )?;
        }
        if begun.ready {
            let ready = cmd.ready.as_ref().ok_or(GenieError::unsupported(
                "an attack's target the slice does not hold live",
            ))?;
            let mut g = Ground {
                map: &c.w.spatial,
                sprite_rng: c.w.streams.get(Stream::Sprite),
                object_rng: None,
                posts: Posts::Held(&mut posted),
                graphics: &c.w.graphics,
                by_id: crate::world::index::ObjectsRef::new(
                    &c.w.players,
                    &c.w.index,
                    &c.w.masters,
                    &c.w.parts,
                ),
            };
            attack.enter_ready(unit, &mut g, &ready.fight, &ready.reads)?;
        }
        if begun.arrived {
            unit.shows_standing_sprite(
                crate::objects::masters::OwnerMasters::new(
                    &c.w.parts.own_master,
                    c.w.masters.of(c.player),
                ),
                &c.w.graphics,
                c.w.streams.get(Stream::Sprite),
            )?;
        }
        Ok((attack, posted))
    }

    /// The start (slot 5 with 1, `0x140a7f8c0`) after its unit AI and place test, its `+0x900`
    /// and its standing sprite (the caller's): readyToAttack (`FUN_140a7f9d0`, third argument 0),
    /// then a unit with a top speed (slot 0x208, `UCOMISS`: a NaN one has some) whose unit AI may
    /// move to attack (slot 0x1d0) turns to state 4 (`set_state(4)`: a `MoveTo` to the target, the
    /// old one's teardown stopping the unit); any other goes ready (`set_state(6)`,
    /// [`Begun::ready`]) when readyToAttack reached its ready end (its second argument 1), else
    /// fails (`set_state(13)`, [`Begun::failed`]).
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
    /// 0x1fd, then stance 1's leash fails a target [`LEASH_SLACK`] past the range from the unit
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
        posted: &mut Vec<(crate::unit_ai::NotifyKind, [i32; 3])>,
        reads: &MoveToReads<'_>,
    ) -> Result<Begun, GenieError> {
        let movable = e.speed != Speed::ZERO && e.can_move;
        let mut failed_first = false;
        let mut ready_end = false;
        if target_owner != PlayerId::GAIA && !visible {
            if e.stance == UnitStance::AGGRESSIVE && Speed::ZERO < e.speed && e.can_move {
                self.state = AttackState::GotoSeen;
                self.range = e.range;
                self.walk_to_seen(reads)?;
            } else {
                failed_first = true;
            }
        } else if self.min_range > e.gap {
            if !self.sub_action {
                posted.push((crate::unit_ai::NotifyKind::TOO_CLOSE, [-1; 3]));
            }
            failed_first = !movable;
        } else if !crate::math::fmath::at_most_or_unordered(reach_gap.get(), self.reach().get()) {
            if !self.sub_action {
                posted.push((crate::unit_ai::NotifyKind::OUT_OF_REACH, [0; 3]));
            }
            if (e.stance == UnitStance::DEFENSIVE
                && self.off_leash(e.target_position, e.state_position))
                || !movable
            {
                failed_first = true;
            } else {
                let _ = self.set_move(e, mover, reads)?;
            }
        } else {
            self.point = e.target_position;
            if !matches!(e.master, MASTER_1704 | MASTER_1706) {
                let _ = face_target(mover, e.position, e.target_position, e.world_time)?;
            }
            ready_end = true;
        }
        if movable {
            let arrived = self.set_move(e, mover, reads)?;
            return Ok(Begun {
                arrived,
                ready: false,
                failed: false,
                failed_first,
            });
        }
        Ok(Begun {
            arrived: false,
            ready: ready_end,
            failed: !ready_end,
            failed_first: false,
        })
    }

    /// readyToAttack's leash of stance 1 on a target at `at` out of reach: its distance from the
    /// unit AI's `state_position` (`+0x1ac`, `+0x1b0`; `FUN_14099ef70` of y's square plus x's) at
    /// least the range plus [`LEASH_SLACK`] (`COMISS`/`JNC`: a NaN distance is within).
    pub(super) fn off_leash(&self, at: Position, centre: Point) -> bool {
        let (dx, dy) = (at.x() - centre.x(), at.y() - centre.y());
        crate::math::fmath::sqrt(dy * dy + dx * dx) >= (self.range + LEASH_SLACK).get()
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
        body: &mut Body,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        // State 13 ("Failed"): the target went out of sight.
        self.state = AttackState::Failed;
        body.set_anim_speed(Speed::new(1.0));
        let idle = self.arms.idle_sprite.ok_or(GenieError::unsupported(
            "an Attack's unit without a standing sprite",
        ))?;
        new_sprite(sprite, body.move_stats_mut(), idle, rng);
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
    master: &crate::objects::masters::Master,
    graphics: &crate::objects::Graphics,
    sprites: TaskSprites,
    moving: Option<SpriteId>,
) -> Arms {
    arms_looking(master, None, graphics, sprites, moving)
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
    master: &crate::objects::masters::Master,
    look: Option<&crate::combat::abilities::Look>,
    graphics: &crate::objects::Graphics,
    sprites: TaskSprites,
    moving: Option<SpriteId>,
) -> Arms {
    let g = |id: Option<SpriteId>| id.and_then(|s| graphics.get(s).copied());
    Arms {
        move_sprite: g(moving)
            .or_else(|| g(crate::objects::sprite::running(master, look)))
            .or_else(|| g(crate::objects::sprite::walking(master, look, false))),
        sprite: g(sprites.proceeding).or_else(|| g(master.attack_sprite)),
        second_sprite: g(master.attack_sprite2),
        idle_sprite: g(master.standing_sprite),
        idle_attack_sprite: g(master.idle_attack_sprite),
        missile: master.missile.master(),
        fire_frame: master.fire_frame,
        interrupt_frame: master.interrupt_frame,
    }
}

/// What an Attack of a unit (its unit AI, top speed and master, placed at `me`) reads of it and
/// its target (placed at `them`).
pub fn engagement(
    ai: Option<&crate::unit_ai::UnitAi>,
    speed: Speed,
    master: &crate::objects::masters::Master,
    me: &crate::map::spatial::Placed,
    them: &crate::map::spatial::Placed,
    world_time: WorldTime,
) -> Result<Engagement, GenieError> {
    if master.charge_type.ranged() {
        return Err(GenieError::unsupported(
            "an attacker whose charge adds to its range (slot 0x2f8)",
        ));
    }
    let (gap_squared, gap) = gaps(me.position, me.radius, them.position, them.radius);
    Ok(Engagement {
        gap_squared,
        gap,
        position: me.position,
        target_position: them.position,
        speed,
        range: master.weapon_range,
        min_range: master.min_range,
        can_move: ai
            .is_some_and(|ai| ai.current_order.is_some() || ai.stance <= UnitStance::DEFENSIVE),
        stance: ai.map_or(UnitStance::AGGRESSIVE, |ai| ai.stance),
        state_position: ai.map_or(Point::ORIGIN, |ai| ai.state_position),
        world_time,
        master: master.id,
        target_class: them.class,
        same_owner: me.player == them.player,
    })
}

/// The gap between two footprints, for an [`Engagement`].
#[must_use]
pub fn gaps(
    a: Position,
    a_radius: Footprint,
    b: Position,
    b_radius: Footprint,
) -> (DistanceSquared, Distance) {
    let squared = footprint_gap_squared(a, a_radius, b, b_radius);
    (squared, footprint_gap(a, a_radius, b, b_radius))
}
