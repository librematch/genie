//! One update (slot 6, `0x140a82ae0`): a target gone, a disarmed building, the walks to where
//! the target was seen.

use crate::GenieError;
use crate::combat::attack::AttackState;
use crate::combat::attack::walks::QUIET_CODE;
use crate::combat::attack::{
    AiOrder, Attack, AttackReads, Fight, NO_RANGE, NONE, Readiness, SEEN_RANGE, TOUCH, move_reads,
    set_walker_anim,
};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Position;
use crate::map::Tile;
use crate::math::rng::{Stream, Well512};
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::move_to::GiveUp;
use crate::movement::move_to::MoveToAction;
use crate::movement::{Me, footprint_gap};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::UpdateCode;
use crate::objects::layers::MotionMut;
use crate::objects::masters::ClassId;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::class;
use crate::objects::sprite::ActiveSprite;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::NotifyKind;
use crate::world::Seconds;
use crate::world::step::Step;

impl Attack {
    /// The update of an Attack whose target is gone (its target −1 after [`Attack::lost`]): the
    /// timer runs and, past the target's checks, state 1 (case 0) stops the unit's animation and
    /// returns 1: the list deletes the Attack. A building that cannot attack (`armed` false,
    /// `FUN_14120ea30`) ends instead (`set_state(1)`: the caller's [`Attack::disarmed`]) and
    /// returns 1, true. The other states without a target are not modelled.
    pub fn update_targetless(
        &mut self,
        dt: Seconds,
        armed: bool,
        mut motion: MotionMut<'_>,
    ) -> Result<(UpdateCode, bool), GenieError> {
        self.timer += dt;
        if !armed {
            self.state = AttackState::Done;
            return Ok((UpdateCode::DONE, true));
        }
        if self.state != AttackState::Done {
            return Err(GenieError::unsupported(
                "an Attack without a target in a state but 1",
            ));
        }
        motion.set_anim_speed(Speed::ZERO);
        Ok((UpdateCode::DONE, false))
    }

    /// The update (`0x140a82ae0`) of an Attack in state 5 or 11 whose target is gone (its target
    /// null, which [`Attack::lost`] leaves a mover of stance 0 waiting where it saw it): the
    /// timer runs and the checks of the target are skipped; the sub-action list's update
    /// ([`Attack::update_list`]) walks the `MoveTo` to where it was seen. Code 0 goes on; 1
    /// and 2 (it ended, the list deleting it) end the Attack (`set_state` 1, a unit's own Attack
    /// posting 0x1fb with 600 and 0x202 first) and return 1; 3 fails it
    /// ([`Attack::walk_failed`]); 5 and above do nothing. `master_class` is the unit's master's.
    ///
    /// # Errors
    ///
    /// The walk's and the notifies'.
    pub(crate) fn update_gone(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        r: &AttackReads,
        master_class: ClassId,
    ) -> Result<UpdateCode, GenieError> {
        self.timer += c.dt.seconds();
        let gives_up = GiveUp {
            waits: !r.computer && master_class == class::VILLAGER,
            stance: r.stance,
        };
        match self.update_list(c, unit, me, r, gives_up)?.get() {
            1 | 2 => {
                self.delete_move_to(c.w.parts.mover_mut(unit.id))?;
                if !self.sub_action {
                    unit.post_own(
                        &mut c.w.parts.unit_ai,
                        NotifyKind::ACTION_INVALIDATED,
                        [600, 0, 0x202],
                    )?;
                }
                self.finish(
                    unit,
                    c.w.parts.motion_mut(unit.id),
                    c.w.streams.get(Stream::Sprite),
                )?;
                Ok(UpdateCode::DONE)
            }
            3 => self.walk_failed(c, unit),
            4 => Err(QUIET_CODE),
            _ => Ok(UpdateCode::UNDER_WAY),
        }
    }

    /// The end of a building's Attack that cannot attack (`0x140a82ae0` at `LAB_140a83711`):
    /// `set_state(1)` shows the standing sprite at speed 1 (its sub-actions none), then a unit's
    /// own Attack (not quiet, `+0x60` 0) posts 0x1fb with 600 and its target (`+0x30`).
    pub(crate) fn disarmed(
        &self,
        sprite: &mut Option<ActiveSprite>,
        mut motion: MotionMut<'_>,
        rng: &mut Well512,
    ) -> Result<Option<(NotifyKind, [i32; 3])>, GenieError> {
        if self.state.move_to().is_some() {
            return Err(GenieError::unsupported("a building's Attack with a MoveTo"));
        }
        motion.set_anim_speed(Speed::new(1.0));
        let idle = self.arms.idle_sprite.ok_or(GenieError::unsupported(
            "an Attack's unit without a standing sprite",
        ))?;
        new_sprite(sprite, motion.move_stats_mut(), idle, rng);
        Ok((!self.sub_action).then_some((
            NotifyKind::ACTION_INVALIDATED,
            [600, ObjectId::field(self.target), 0],
        )))
    }

    /// One update (slot 6, `0x140a82ae0`) of `unit`'s Attack on the step: the timer runs, the
    /// target is still there and where it was seen; then the state's step. Its notifies go to
    /// the unit AI at once, and what it lets go is let go at once. Returns the action list's
    /// code: 0 under way, 5 done after its target died.
    ///
    /// # Errors
    ///
    /// What the update does not model.
    pub(crate) fn update(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut crate::objects::layers::Action,
        me: Me,
        f: &mut Fight,
        r: &AttackReads,
    ) -> Result<UpdateCode, GenieError> {
        self.timer += c.dt.seconds();
        // a dead tree (class 15) up to state 5 keeps a non-villager's attack going: its update
        // goes on, the target not alive (`0x140a8342e`)
        let alive = f.victim.state <= ObjectState::Alive;
        let gone = !alive
            && (f.victim.class != class::TREE
                || f.own.class == class::VILLAGER
                || ObjectState::Dead < f.victim.state);
        if self.state != AttackState::Killed && gone {
            return self.target_died(c, unit, f, r);
        }
        let (_, t) = self.held_target(&c.w.spatial)?;
        if self.state != AttackState::Killed
            && (t.position.x() != self.seen.x() || t.position.y() != self.seen.y())
            && let Some(code) = self.target_moved(c, unit, f, r, &t)?
        {
            return Ok(code);
        }
        if let Some(code) = self.ends_early(c, unit, f, r, alive)? {
            return Ok(code);
        }
        match self.state {
            // case 0: the unit's animation stops; the list deletes the Attack
            AttackState::Done => {
                set_walker_anim(c.w.parts.motion_mut(unit.id), Speed::new(0.0));
                Ok(UpdateCode::DONE)
            }
            // case 3 (state 4) walks the MoveTo to the target, cases 4 and 10 (states 5 and
            // 11) to where it was seen
            AttackState::Move { .. }
            | AttackState::GotoSeen { .. }
            | AttackState::GotoPoint { .. } => self.walk(c, unit, me, f, r, (t.position, t.radius)),
            AttackState::Ready => self.ready_step(c, unit, f, r, alive),
            AttackState::Swing => self.swing(c, unit, actions, f, r),
            AttackState::Killed => self.killed_step(c, unit),
            // case 0xc: a failed attack posts 0x1f9 with 600 every update (quiet or not), the
            // unit's animation stops and the update returns 3
            AttackState::Failed => {
                unit.post_own(
                    &mut c.w.parts.unit_ai,
                    NotifyKind::ACTION_FAILED,
                    [600, 0, 0],
                )?;
                set_walker_anim(c.w.parts.motion_mut(unit.id), Speed::new(0.0));
                Ok(UpdateCode::FAILED)
            }
            // the states without a case (0; 3, 8 to 10, 12, 14 and above, which no step sets)
            // return 0
            AttackState::New => Ok(UpdateCode::UNDER_WAY),
        }
    }

    /// The update's start on a target that died before the attack ended: the attack lets it go
    /// and ends, returning 5 ([`Attack::lost`] for a unit's own Attack).
    pub(super) fn target_died(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        f: &Fight,
        r: &AttackReads,
    ) -> Result<UpdateCode, GenieError> {
        // the target died before the attack ended (the update's start): one of stance 0 that
        // moves would wait (set_state 5), not modelled; a villager's unit AI keeps a dead
        // target of Gaia's (another's FUN_140c5e1e0 forgets, not modelled). The attack lets
        // the target go (slot 0x68 with none: a sub-action lets nothing go; the unit's attack
        // count clears, the caller's) and its point, posts no notify as the hunt's
        // sub-action (`+0x60`), ends (set_state 1) and returns 5
        if !self.sub_action {
            return self.lost(c, unit, f, r);
        }
        if f.own.class != class::VILLAGER {
            return Err(GenieError::unsupported(
                "an Attack of a unit other than a villager whose target died",
            ));
        }
        if f.victim.player != PlayerId::new(0) {
            return Err(GenieError::unsupported(
                "a villager's Attack whose player-owned target died (its unit AI forgets it)",
            ));
        }
        self.target = None;
        self.point = Position::from([NONE; 3]);
        self.finish(
            unit,
            c.w.parts.motion_mut(unit.id),
            c.w.streams.get(Stream::Sprite),
        )?;
        Ok(UpdateCode::CODE_5)
    }

    /// The update on a target `t` that moved from where it was seen; Some with the update's code
    /// when that ends it.
    pub(super) fn target_moved(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        f: &Fight,
        r: &AttackReads,
        t: &crate::map::spatial::Placed,
    ) -> Result<Option<UpdateCode>, GenieError> {
        // the target moved: where it was seen follows (`+0xa4`..`+0xac`); one the unit's
        // player sees (the target's slot 0x1e0, `FUN_1416635e0` at its tile: 0xf), or only
        // explored (0x80) with its master's fog flag (`+0x605`), keeps the attack going, but
        // an attack in state 5 or 11 follows it again (set_state 4, [`Attack::follow`]). One
        // out of sight keeps an attack in a state but 4 and 5 going; a player's unit without a
        // unit AI, or whose unit AI has an order and an empty queue, waits
        // ([`Attack::wait_at_seen`]; one in state 5 keeps going); any other fails
        // ([`Attack::out_of_sight`])
        self.seen = t.position;
        let seen = sees(&c.w.spatial, f, t.position)?;
        if seen
            && matches!(
                self.state,
                AttackState::GotoSeen { .. } | AttackState::GotoPoint { .. }
            )
        {
            let ground =
                c.w.parts
                    .mover_of(unit.id)
                    .map(|m| m.stats.radius.ground())
                    .ok_or(crate::world::ATTACKER_UNMOVING)?;
            let gap = footprint_gap(unit.place.position(), ground, t.position, t.radius);
            self.follow(unit, &mut c.w.attack_ground(unit.id), r, f, gap)?;
            return Ok(Some(UpdateCode::UNDER_WAY));
        }
        if !seen
            && matches!(
                self.state,
                AttackState::Move { .. } | AttackState::GotoSeen { .. }
            )
        {
            let gives_up = match r.ai_order {
                AiOrder::Absent => false,
                AiOrder::Idle => true,
                AiOrder::Order(_) => !r.queue_empty,
            };
            if f.own.player == PlayerId::new(0) || gives_up {
                return self.out_of_sight(c, unit, f, r).map(Some);
            }
            if !matches!(self.state, AttackState::GotoSeen { .. }) {
                self.wait_at_seen(unit, c.w.parts.motion_mut(unit.id), f, r, &c.w.graphics)?;
                return Ok(Some(UpdateCode::UNDER_WAY));
            }
        }
        Ok(None)
    }

    /// The update's ends before the state's step (Some with the update's code): an allied
    /// target, a target inside another and a building that cannot attack.
    pub(super) fn ends_early(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        f: &Fight,
        r: &AttackReads,
        alive: bool,
    ) -> Result<Option<UpdateCode>, GenieError> {
        // LAB_140a834ab: a live target (state 0 to 2) of a player's unit (+0x718 not 0) that
        // its player counts as itself or an ally (FUN_141663dc0 with 1) ends the attack
        // (set_state 1; its own Attack, not quiet, posts 0x1fb with 600 and the target) and
        // returns 1, unless the target is livestock (58) or a farm (49) that was its player's when
        // the Attack took it (+0x9f)
        let spared = matches!(f.victim.class, class::LIVESTOCK | class::FARM) && self.same_owner;
        let rng = c.w.streams.get(Stream::Sprite);
        if alive && f.own.player != PlayerId::new(0) && !spared && f.victim.allied.by_attacker {
            self.finish(unit, c.w.parts.motion_mut(unit.id), rng)?;
            if !self.sub_action {
                unit.post_own(
                    &mut c.w.parts.unit_ai,
                    NotifyKind::ACTION_INVALIDATED,
                    [600, ObjectId::field(self.target), 0],
                )?;
            }
            return Ok(Some(UpdateCode::DONE));
        }
        // a target inside another (+0x2f0): the attack ends (set_state 1) and returns 4, which
        // keeps it in the list; a unit's own Attack (not quiet, +0x60) of a unit whose unit AI's
        // order queue is empty posts 0x1fb with 600 and the target
        if f.victim.inside {
            self.finish(unit, c.w.parts.motion_mut(unit.id), rng)?;
            if !self.sub_action && r.queue_empty {
                unit.post_own(
                    &mut c.w.parts.unit_ai,
                    NotifyKind::ACTION_INVALIDATED,
                    [600, ObjectId::field(self.target), 0],
                )?;
            }
            return Ok(Some(UpdateCode::CODE_4));
        }
        // a building that cannot attack (FUN_14120ea30) ends it (set_state 1), its own Attack
        // posting 0x1fb with 600 and the target, and returns 1: the list deletes it
        if !r.armed {
            self.finish(unit, c.w.parts.motion_mut(unit.id), rng)?;
            if !self.sub_action {
                unit.post_own(
                    &mut c.w.parts.unit_ai,
                    NotifyKind::ACTION_INVALIDATED,
                    [600, ObjectId::field(self.target), 0],
                )?;
            }
            return Ok(Some(UpdateCode::DONE));
        }
        Ok(None)
    }

    /// State 6's step (case 5): readyToAttack ready swings ([`Attack::set_swing`]), waiting
    /// waits, away follows the target ([`Attack::follow`]); ready on a target no longer alive (a
    /// dead tree) ends the attack with code 5.
    pub(super) fn ready_step(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        f: &Fight,
        r: &AttackReads,
        alive: bool,
    ) -> Result<UpdateCode, GenieError> {
        let mut g = c.w.attack_ground(unit.id);
        match self.ready_to_attack(unit, &mut g, f, r, false)? {
            // a target no longer alive (a dead tree): set_state(1), code 5
            Readiness::Ready if !alive => {
                self.finish(unit, g.motion.reborrow(), g.sprite_rng)?;
                return Ok(UpdateCode::CODE_5);
            }
            Readiness::Ready => self.set_swing(unit, &mut g, f, r)?,
            Readiness::Waiting => {}
            Readiness::Away(gap) => self.follow(unit, &mut g, r, f, gap)?,
        }
        Ok(UpdateCode::UNDER_WAY)
    }

    /// State 2's step (case 1, the target killed): once the swing's sprite looped the attack lets
    /// its target go and ends.
    pub(super) fn killed_step(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
    ) -> Result<UpdateCode, GenieError> {
        // case 1: once the swing's sprite looped (or there is none, or it does not
        // animate) the attack lets its target go and ends; the hunt reads code 5
        let done = unit
            .sprite
            .is_none_or(|sp| sp.looped || !sp.graphic.flags.contains(SpriteFlags::ANIMATED));
        if !done {
            return Ok(UpdateCode::UNDER_WAY);
        }
        // the target goes (slot 0x68 with none: a sub-action lets nothing go; the unit's
        // attack count clears, the caller's) and the attack ends (set_state 1); the Hunt's
        // Attack, a sub-action, then posts 0x1fb with 600 and 0x202 too and returns 5
        if self.sub_action {
            self.target = None;
            self.finish(
                unit,
                c.w.parts.motion_mut(unit.id),
                c.w.streams.get(Stream::Sprite),
            )?;
            unit.post_own(
                &mut c.w.parts.unit_ai,
                NotifyKind::ACTION_INVALIDATED,
                [600, 0, 0x202],
            )?;
            return Ok(UpdateCode::CODE_5);
        }
        // a unit's own Attack (not quiet, `+0x60` 0): the target goes (slot 0x68 with
        // none), it posts 0x1fb with 600 and 0x202, ends (set_state 1) and returns 2,
        // which keeps it in the list
        let target = self.target.take();
        let_go(c, unit, target)?;
        unit.post_own(
            &mut c.w.parts.unit_ai,
            NotifyKind::ACTION_INVALIDATED,
            [600, 0, 0x202],
        )?;
        self.finish(
            unit,
            c.w.parts.motion_mut(unit.id),
            c.w.streams.get(Stream::Sprite),
        )?;
        Ok(UpdateCode::CODE_2)
    }

    /// `set_state(5)` (`0x140a820c0` case 5): the sub-actions go, the range is slot 0x238's
    /// again, and a `MoveTo` to where the target was seen within it starts
    /// ([`Attack::goto_point`]).
    pub(super) fn wait_at_seen(
        &mut self,
        unit: &Unit,
        mut motion: MotionMut<'_>,
        f: &Fight,
        r: &AttackReads,
        graphics: &crate::objects::Graphics,
    ) -> Result<(), GenieError> {
        self.enter(AttackState::GotoSeen { move_to: None }, motion.reborrow())?;
        self.range = f.own.source.weapon_range;
        self.goto_point(
            |move_to| AttackState::GotoSeen { move_to },
            (unit, motion.mover()),
            r,
            graphics,
        )
    }

    /// The `MoveTo` to a point of states 5 and 11 ([`Attack::walk_to_seen`], the walk `walk`
    /// makes) of `unit` with its mover: its start
    /// finds no target object to be near (`FUN_14118f680` answers `FLT_MAX`), so a unit with a
    /// top speed (`UCOMISS`: not 0) asks for a path (state 3), any other fails (state 13).
    ///
    /// # Errors
    ///
    /// A unit that does not move.
    pub(super) fn goto_point(
        &mut self,
        walk: fn(Option<MoveToAction>) -> AttackState,
        unit: (&Unit, Option<&Mover>),
        r: &AttackReads,
        graphics: &crate::objects::Graphics,
    ) -> Result<(), GenieError> {
        self.walk_to_seen(walk, &move_reads(unit, r, graphics)?)
    }

    /// States 5 and 11 once their `MoveTo` arrived (cases 4 and 10, its update 1 or 2): a target
    /// the unit's player sees (the target's slot 0x1e0: 0xf, or 0x80 with its master's fog flag)
    /// makes it ready (`set_state` 6). Out of sight, a human player's unit in state 5 with a range
    /// of at least [`SEEN_RANGE`] (`COMISS`: a NaN is not) walks onto the point (`set_state` 11:
    /// the sub-actions go, the range is slot 0x238's, a `MoveTo` without a range to where the
    /// target was seen); any other lets the target go (slot 0x68 with none) and fails
    /// (`set_state` 13), a unit's own Attack posting 0x1f9 with 600, and returns 3.
    pub(super) fn arrived_at_seen(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        f: &Fight,
        r: &AttackReads,
        target: Position,
    ) -> Result<UpdateCode, GenieError> {
        if sees(&c.w.spatial, f, target)? {
            self.ready(unit, &mut c.w.attack_ground(unit.id), f, r)?;
            return Ok(UpdateCode::UNDER_WAY);
        }
        if matches!(self.state, AttackState::GotoSeen { .. })
            && f.own.player != PlayerId::new(0)
            && SEEN_RANGE <= self.range.get()
            && !r.computer
        {
            self.enter(
                AttackState::GotoPoint { move_to: None },
                c.w.parts.motion_mut(unit.id),
            )?;
            self.range = f.own.source.weapon_range;
            self.goto_point(
                |move_to| AttackState::GotoPoint { move_to },
                (unit, c.w.parts.mover_of(unit.id)),
                r,
                &c.w.graphics,
            )?;
            return Ok(UpdateCode::UNDER_WAY);
        }
        let target = self.target.take();
        if !self.sub_action {
            let_go(c, unit, target)?;
        }
        self.finish_in(
            AttackState::Failed,
            unit,
            c.w.parts.motion_mut(unit.id),
            c.w.streams.get(Stream::Sprite),
        )?;
        if !self.sub_action {
            unit.post_own(
                &mut c.w.parts.unit_ai,
                NotifyKind::ACTION_FAILED,
                [600, 0, 0],
            )?;
        }
        Ok(UpdateCode::FAILED)
    }
}

/// The target's tile at `at` seen by the unit's player (`f.own`), or only explored with its
/// master's fog flag (the target's slot 0x1e0: 0xf, or 0x80 with `+0x605`).
///
/// # Errors
///
/// A tile the map's sight does not hold.
pub(super) fn sees(
    map: &crate::map::spatial::Spatial,
    f: &Fight,
    at: Position,
) -> Result<bool, GenieError> {
    let (tx, ty) = (at.x() as i32, at.y() as i32);
    Ok(map.sees_tile(f.own.player, Tile::new(tx, ty))
        || (f.victim.fog_visibility != FogVisibility::INVISIBLE
            && map.explored(f.own.player, Tile::new(tx, ty))?))
}

/// A unit's own Attack lets its target go (slot 0x68 with none: its slot 0x180) at once
/// ([`crate::world::lets::release`]); none lets nothing go.
///
/// # Errors
///
/// The release's.
pub(super) fn let_go(
    c: &mut Step<'_>,
    unit: &mut Unit,
    target: Option<ObjectId>,
) -> Result<(), GenieError> {
    let Some(target) = target else {
        return Ok(());
    };
    let by = unit.releaser(c)?;
    crate::world::lets::release(c, unit, target, by)
}

/// The squared reach of an attack of `range` (`FUN_140a82ae0` at `0x140a83950`): the range squared,
/// or [`TOUCH`] for a range of at most [`NO_RANGE`].
pub(super) fn squared_reach(range: Distance) -> DistanceSquared {
    if NO_RANGE < range.get() {
        range.squared()
    } else {
        TOUCH
    }
}
