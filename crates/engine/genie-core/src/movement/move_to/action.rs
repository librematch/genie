//! The `MoveTo` as a unit's own top action ([`MoveToAction`]): its constructors, its start and
//! its update.

use crate::GenieError;
use crate::combat::abilities::Look;
use crate::map::Distance;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::rng::Well512;
use crate::movement::Speed;
use crate::movement::move_to::Shown;
use crate::movement::move_to::{
    Follow, Goal, MoveTo, MoveWait, TargetMoved, Walk, ask_walk, target_left, walk_or_give_up,
};
use crate::movement::{Mover, PathAsk, footprint_gap};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::ids::TOWN_CENTRE;
use crate::objects::masters::master_type;
use crate::objects::sprite::{ActiveSprite, Graphic, new_sprite, standing, walking};
use crate::unit_ai::NotifyKind;
use crate::unit_ai::{ActionKind, Inbox};
use crate::world::Seconds;
use genie_diff::Diff;

/// A `MoveTo` (`RGE_Action_Move_To`): a unit's own top action (a wandering animal's, a moved or
/// following unit's; its `+0x60` and `+0x61` are 0: it is not quiet), or the sub-action of an
/// action that walks ([`MoveToAction::sub_to_point`]).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct MoveToAction {
    /// Its state (`+0x18`).
    pub state: MoveTo,
    /// `+0x38`..`+0x40`: the point it walks to without a target object; with one, the target's
    /// place when it was made or last asked for a path (its state 3), which a path's end that
    /// follows the target moves ([`TargetMoved::Follows`]). A target the world removed leaves the
    /// `MoveTo` walking to it.
    pub point: Position,
    /// The target object it walks to (`+0x20`, its id at `+0x30`), None for a point (−1).
    pub target: Option<ObjectId>,
    /// `+0x68`.
    pub range: Distance,
    /// The sprite it shows when it turns to state 4 (`+0x58`, the save's second `short_val`):
    /// the one its maker gave, else its constructor's default ([`point_sprite`],
    /// [`object_sprite`]); None for none (null).
    pub sprite: Option<Graphic>,
    /// `+0x60`: quiet. An action makes its sub-action `MoveTo` quiet (`FUN_140a7a170` with 1):
    /// its `set_state` 1, 13 and 14 show no standing sprite (`0x140aa29e0`), and its update
    /// posts no notify 0x1fa, 0x1f9 or 0x1fb (`0x140aa2a70`). A `MoveTo` of its own is not quiet.
    /// The save does not hold it: the import takes its maker's.
    pub quiet: bool,
    /// `+0x61`: what it does when its target object left its point ([`TargetMoved`]); 0
    /// ([`TargetMoved::Follows`]) from its constructors (`FUN_140a73e90`), which an Enter sets
    /// to 2 and a drop-off's, a hunt's and its Attack's maker to 1.
    pub on_target_moved: TargetMoved,
    /// Its wait after the motion step gave up.
    pub wait: MoveWait,
}

/// A `MoveTo`'s start counts the unit at its target within this gap (`0x140aa2100`,
/// `DAT_142eb99cc`).
pub(super) const ARRIVED: f32 = crate::math::image::move_to::ARRIVED;
/// A `MoveTo` to an object the world does not hold.
pub(crate) const TARGET_LACKING: GenieError =
    GenieError::unsupported("a MoveTo to an object the world lacks");
/// Livestock's `MoveTo` to a town centre walks to its place less this on both axes
/// (`FUN_140aa0df0`, `DAT_142eae8d0`).
const TOWN_CENTRE_OFFSET: f32 = crate::math::image::artifact::TOWN_CENTRE_OFFSET;

/// `FUN_140aa0df0`'s test of a `MoveTo`'s target object of master `master` in `state`:
/// livestock (`livestock`, the unit's class 58) moving to a town centre (master 109) holds none,
/// and no `MoveTo` holds a target in state 7 or later (`FUN_140a7a230`); any other target is
/// held, and a `MoveTo` of its own (not quiet) takes it (slot 0x178).
#[must_use]
pub fn holds_object(livestock: bool, master: MasterId, state: ObjectState) -> bool {
    // The town centre's master (`FUN_140aa0df0`'s `0x6109` raw), to which livestock walks beside.
    !(livestock && matches!(master, TOWN_CENTRE)) && state < ObjectState::Removed
}

/// What one update of a [`MoveToAction`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveStep {
    /// It is done (its update in state 1 returns 1): the action list removes it.
    pub done: bool,
    /// Its target left its point: it told the unit (notify 0x1fc) and turned to state 3.
    pub target_moved: bool,
    /// The motion step could not go on (its result 1): the caller sets state 13, or for a
    /// human's villager the wait of state 2 (`LAB_140aa320c`).
    pub gave_up: bool,
    /// It had failed (state 13): its update returns 3 after the notify 0x1f9 (an own `MoveTo` is
    /// never quiet); the action list removes it.
    pub failed: bool,
}

/// The range a `MoveTo` to an object (`FUN_140aa0df0`) takes for `range`: one made with none
/// (0.0; `UCOMISS`, so −0.0 too and never a NaN) takes the unit's slot 0x268. A building's
/// (`FUN_1411c5d80`, master type 80) is `image::order::CART_MOVE_RANGE` (0.75) when its slot
/// 0x4f0 holds (`FUN_1411d4980` -> `FUN_1415c60b0`: its master has a task of action type 0x99, a
/// mule cart's); any other's (`FUN_14106a6d0`) is 0. A `MoveTo` to a point (`FUN_140aa0d30`)
/// keeps its range.
#[must_use]
pub fn object_range(range: Distance, master: Option<&Master>) -> f32 {
    if range != Distance::ZERO {
        return range.get();
    }
    let cart = master.is_some_and(|m| {
        m.kind == master_type::BUILDING
            && m.tasks
                .iter()
                .any(|t| t.action_type == crate::construction::build::CART_TASK)
    });
    if cart {
        crate::math::image::order::CART_MOVE_RANGE
    } else {
        0.0
    }
}

/// The sprite (`+0x58`) of a `MoveTo` to a point whose maker gives none (`FUN_140aa0d30` with
/// 0): slot 0x7c8's, the walking sprite ([`walking`]), else slot 0x798's, the standing one
/// ([`standing`]); `look` and `goods` as those read them.
#[must_use]
pub fn point_sprite(master: &Master, look: Option<&Look>, goods: bool) -> Option<SpriteId> {
    walking(master, look, goods).or_else(|| standing(master, look, goods))
}

/// The sprite (`+0x58`) of a `MoveTo` to an object whose maker gives none (`FUN_140aa0df0` with
/// 0): the master's walking sprite (`+0x748`), else its standing one (`+0x470`); no terrain look.
#[must_use]
pub fn object_sprite(master: &Master) -> Option<SpriteId> {
    master.move_sprite.or(master.standing_sprite)
}

/// The sprite the unit AI's moves give their `MoveTo` (slot 0x340, `FUN_1410866e0`, with
/// `walk` its seventh argument; slot 0x348, `FUN_141086440`, never walks): the master's running
/// sprite (`+0x750`) unless the move walks, else (or without one) its walking sprite (`+0x748`);
/// no terrain look. None leaves the constructor's default ([`point_sprite`], [`object_sprite`]).
#[must_use]
pub fn ai_sprite(master: &Master, walk: bool) -> Option<SpriteId> {
    let run = master.run_sprite.filter(|_| !walk);
    run.or(master.move_sprite)
}

/// What a `MoveTo`'s constructors take for the sprite (`+0x58`) when their maker gives none,
/// read from the unit's master and terrain look as the `MoveTo` is made: `FUN_140aa0d30`'s
/// ([`point_sprite`]) and `FUN_140aa0df0`'s ([`object_sprite`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveDefaults {
    /// `FUN_140aa0d30`'s, a `MoveTo` to a point.
    pub point: Option<SpriteId>,
    /// `FUN_140aa0df0`'s, a `MoveTo` to an object.
    pub object: Option<SpriteId>,
}

impl MoveDefaults {
    /// The defaults of a unit of `master` with the terrain look `look` (a trade cart's goods are
    /// not held: none).
    #[must_use]
    pub fn of(master: &Master, look: Option<&Look>) -> Self {
        Self {
            point: point_sprite(master, look, false),
            object: object_sprite(master),
        }
    }

    /// The sprite of a `MoveTo` to a point (`FUN_140aa0d30`) whose maker gives `given`.
    #[must_use]
    pub const fn to_point(self, given: Option<SpriteId>) -> Option<SpriteId> {
        match given {
            Some(id) => Some(id),
            None => self.point,
        }
    }

    /// The sprite of a `MoveTo` to an object (`FUN_140aa0df0`) whose maker gives `given`.
    #[must_use]
    pub const fn to_object(self, given: Option<SpriteId>) -> Option<SpriteId> {
        match given {
            Some(id) => Some(id),
            None => self.object,
        }
    }
}

/// The unit's sprite as its own `MoveTo`'s `set_state` (`0x140aa29e0`) shows it with slot 0x68
/// ([`new_sprite`]): state 4 the `MoveTo`'s sprite ([`MoveToAction::sprite`]); states 1 and 13
/// (an own `MoveTo` is not quiet) slot 0x798's ([`standing`]). Slot 0x68 shows the master's
/// standing sprite (`+0x470`) for a null one.
#[derive(Debug)]
pub struct Shows<'a> {
    /// The unit's sprite.
    pub sprite: &'a mut Option<ActiveSprite>,
    /// The Sprite stream.
    pub rng: &'a mut Well512,
    /// Slot 0x798's sprite, the unit's standing one.
    pub standing: Option<Graphic>,
    /// The master's standing sprite (`+0x470`), which slot 0x68 shows for a null one.
    pub master_standing: Option<Graphic>,
}

impl MoveToAction {
    /// A new `MoveTo` of its own to the object `target` within `range` (`FUN_140aa0df0`; its point
    /// `+0x38` the target's place, which a path request renews), with
    /// its `sprite` ([`MoveToAction::sprite`]), as its start (slot 0x808 ->
    /// `0x140aa2100`) leaves it: a unit more than [`ARRIVED`] from the target's footprint
    /// (`FUN_14118f680`) in state 3 with speed (slot 0x208), 13 without; a unit at the target
    /// already in state 1, which its next update ends. States 1 and 13 (`0x140aa29e0`, not quiet)
    /// show the master's standing sprite ([`Mover::shows_standing`]; the sprite itself and
    /// `+0x7b8`, cleared, are the caller's, [`MoveToAction::shows_standing`]). `me` is the unit's
    /// place.
    pub fn to_target(
        world: &Spatial,
        me: Position,
        mover: &mut Mover,
        target: ObjectId,
        range: Distance,
        sprite: Option<Graphic>,
    ) -> Result<Self, GenieError> {
        let t = world.get_known(target)?.ok_or(TARGET_LACKING)?;
        let ground = mover.stats.radius.ground();
        let state = if footprint_gap(me, ground, t.position, t.radius) <= Distance::new(ARRIVED) {
            MoveTo::Arrived
        } else if mover.stats.speed == Speed::ZERO {
            MoveTo::Failed
        } else {
            MoveTo::Start
        };
        Ok(Self {
            state,
            point: t.position,
            target: Some(target),
            range,
            sprite,
            quiet: false,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        })
    }

    /// A new `MoveTo` of its own (quiet, `+0x60`, 0) to the object `target` within `range`
    /// (`FUN_140aa0df0`), then its start (slot 0x808 -> `0x140aa2100`). Livestock (`livestock`,
    /// the unit's class 58) moving to a town centre (master 109) holds no target and walks to
    /// the centre's place less [`TOWN_CENTRE_OFFSET`] on both axes; a target in a `state` of 7
    /// or later is not held either (`FUN_140a7a230`: its id stays −1) and the `MoveTo` walks to
    /// its place, as a `MoveTo` to that point starts ([`MoveToAction::to_point`]). Any other
    /// target is held ([`MoveToAction::to_target`]), and the caller takes it (slot
    /// 0x178, [`MoveToAction::target`] not −1).
    ///
    /// # Errors
    ///
    /// A target the world lacks.
    pub fn to_object(
        world: &Spatial,
        me: Position,
        mover: &mut Mover,
        (target, state): (ObjectId, ObjectState),
        range: Distance,
        (sprite, livestock): (Option<Graphic>, bool),
    ) -> Result<Self, GenieError> {
        let t = world.get_known(target)?.ok_or(TARGET_LACKING)?;
        let at = t.position;
        let goal = if livestock && t.master == TOWN_CENTRE {
            Position::new(
                at.x() - TOWN_CENTRE_OFFSET,
                at.y() - TOWN_CENTRE_OFFSET,
                at.z(),
            )
        } else if holds_object(livestock, t.master, state) {
            return Self::to_target(world, me, mover, target, range, sprite);
        } else {
            at
        };
        Ok(Self::to_point(goal, range, sprite, Some(mover)))
    }

    /// A new `MoveTo` of its own (not quiet) to `point` within `range` (`FUN_140aa0d30`), with its
    /// `sprite` ([`MoveToAction::sprite`]), as its start (slot 0x808 ->
    /// `0x140aa2100`) leaves it. Without a target object the start's gap (`FUN_14118f680` with
    /// none) is `f32::MAX`, never within 0.01: a unit with speed (slot 0x208, `UCOMISS`: a NaN
    /// speed has some) is in state 3, one without in 13 (`0x140aa29e0`, not quiet: the master's
    /// standing sprite, [`Mover::shows_standing`]; the sprite itself and `+0x7b8`, cleared, are the
    /// caller's, [`MoveToAction::shows_standing`]). A unit without movement state has no speed.
    #[must_use]
    pub fn to_point(
        point: Position,
        range: Distance,
        sprite: Option<Graphic>,
        mover: Option<&mut Mover>,
    ) -> Self {
        let state = match mover {
            Some(m) if m.stats.speed != Speed::ZERO => MoveTo::Start,
            Some(_) | None => MoveTo::Failed,
        };
        Self {
            state,
            point,
            target: None,
            range,
            sprite,
            quiet: false,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait::default(),
        }
    }

    /// Where it walks (`+0x30`, else `+0x38`..`+0x40`): its target object, else its point.
    #[must_use]
    pub const fn walks_to(&self) -> Goal {
        match self.target {
            None => Goal::Point(self.point),
            Some(id) => Goal::Target(id),
        }
    }

    /// The point it walks to when it has no target object, None for one to a target: what the
    /// tests compare.
    #[cfg(test)]
    pub(crate) const fn goal(&self) -> Option<Position> {
        if self.target.is_none() {
            Some(self.point)
        } else {
            None
        }
    }

    /// It ended at its start, the unit at its target (state 1).
    #[must_use]
    pub fn arrived(&self) -> bool {
        self.state == MoveTo::Arrived
    }

    /// Its start ended it (state 1 at its target, or 13 for a unit without speed), whose
    /// `set_state` (`0x140aa29e0`, not quiet) shows the master's standing sprite and clears the
    /// unit's `+0x7b8`: the caller's ([`crate::objects::unit::Unit::shows_standing_sprite`]).
    #[must_use]
    pub const fn shows_standing(&self) -> bool {
        matches!(self.state, MoveTo::Arrived | MoveTo::Failed)
    }

    /// A `MoveTo` of its own is not quiet: its `set_state` 1 or 13 shows slot 0x798's sprite
    /// ([`Shows::standing`], slot 0x68 with it, [`new_sprite`]) and clears the unit's `+0x7b8`.
    fn ended(&self, walk: &mut Walk<'_>, shows: &mut Shows<'_>) {
        if matches!(self.state, MoveTo::Arrived | MoveTo::Failed) {
            if let Some(g) = shows.standing.or(shows.master_standing) {
                new_sprite(shows.sprite, walk.body.move_stats_mut(), g, shows.rng);
            }
            walk.set_anim_speed(0.0);
        }
    }

    /// One update (`0x140aa2a70`) of a `MoveTo` of its own, of the unit whose sprite `shows`
    /// holds, after its first check of its target ([`MoveToAction::check_held`], the caller's,
    /// which holds the world): a walk goes on ([`MoveToAction::update_moving`]), a start asks for
    /// its path with what the unit's AI says (`ask`, [`MoveToAction::update_start`]), a wait runs
    /// by dt and at its end the `MoveTo` asks again (`set_state` 3).
    pub fn update(
        &mut self,
        mut walk: Walk<'_>,
        ask: PathAsk,
        mut shows: Shows<'_>,
    ) -> Result<MoveStep, GenieError> {
        let mut step = MoveStep::default();
        match self.state {
            MoveTo::Moving => self.update_moving(&mut walk, &mut step, &mut shows)?,
            MoveTo::Start => self.update_start(&mut walk, ask, &mut step, &mut shows)?,
            MoveTo::Arrived => step.done = true,
            // case 2: the timer runs by dt; at its end the MoveTo asks again (set_state 3)
            MoveTo::Waiting => {
                self.wait.timer += walk.dt;
                if self.wait.delay <= self.wait.timer {
                    self.wait.timer = Seconds::ZERO;
                    self.state = MoveTo::Start;
                }
            }
            // 0x140aa2a70's state 13: the notify (the caller's), then code 3
            MoveTo::Failed => step.failed = true,
        }
        Ok(step)
    }

    /// The update's walk (state 4): the check of its target ([`MoveToAction::check_target`]),
    /// then the motion step (`LAB_140aa320c`), which moving on leaves the `MoveTo` in the state the
    /// check left. A `MoveTo` that asks again ends its update at the check; one whose path's end
    /// could not follow tells the unit (notify 0x1fc, the caller's [`MoveStep::target_moved`]). A
    /// motion step that gives up leaves the wait or the failure to the caller.
    fn update_moving(
        &mut self,
        walk: &mut Walk<'_>,
        step: &mut MoveStep,
        shows: &mut Shows<'_>,
    ) -> Result<(), GenieError> {
        match self.check_target(walk)? {
            Follow::Again => return Ok(()),
            Follow::Blocked => step.target_moved = true,
            Follow::Stays | Follow::PathEnd(_) => {}
        }
        match walk_or_give_up(walk.reborrow())? {
            Some(MoveTo::Moving) => {}
            Some(other) => self.state = other,
            None => step.gave_up = true,
        }
        self.ended(walk, shows);
        Ok(())
    }

    /// The update's first check of its target object (`0x140aa2a70`, before its state's case),
    /// with the target's object state (`state`, `+0x640`; None when the world holds no object of
    /// its id). A target the world no longer holds is cleared (`FUN_140a78450` finds none: slot
    /// 0x68 with none releases nothing, as its lookup fails) and the `MoveTo` walks on to its
    /// point. A held one in a state above 6 is let go (slot 0x68 with none, `FUN_140a7a230`; a
    /// `MoveTo` that is not quiet owes it the release, slot 0x180, its caller's) and the update
    /// ends there: 5 when quiet, else 4 after notify 0x1fb through `inbox` (the move action
    /// value, the id let go, −1, and 0). Returns that code.
    ///
    /// # Errors
    ///
    /// The notify's.
    pub fn check_held(
        &mut self,
        state: Option<ObjectState>,
        inbox: &mut Inbox<'_>,
    ) -> Result<Option<u8>, GenieError> {
        if self.target.is_none() {
            return Ok(None);
        }
        match state {
            None => {
                self.target = None;
                Ok(None)
            }
            Some(s) if ObjectState::Undead < s => {
                self.target = None;
                if self.quiet {
                    return Ok(Some(5));
                }
                inbox.post(
                    NotifyKind::ACTION_INVALIDATED,
                    [ActionKind::Move.value(), -1, 0],
                )?;
                Ok(Some(4))
            }
            Some(_) => Ok(None),
        }
    }

    /// A moving `MoveTo`'s check of its target object (`0x140aa2a70` in state 4, before the
    /// motion step; none for a `MoveTo` to a point) by its `+0x61` ([`target_moved`]): the path's
    /// end follows the target ([`Mover::follow_path_end`]), or the `MoveTo` turns to state 3 (the
    /// slice's [`MoveTo::Start`]) when it asks again or its path's end could not follow.
    ///
    /// # Errors
    ///
    /// [`target_moved`]'s.
    pub(crate) fn check_target(&mut self, walk: &mut Walk<'_>) -> Result<Follow, GenieError> {
        let Some(target) = self.target else {
            return Ok(Follow::Stays);
        };
        let follow = target_left(walk, target, self.point, self.on_target_moved)?;
        match follow {
            Follow::PathEnd(p) => {
                let mover = walk.body.mover_mut().ok_or(crate::world::WALKER_UNMOVING)?;
                mover.follow_path_end(p);
                self.point = p;
            }
            Follow::Again | Follow::Blocked => self.state = MoveTo::Start,
            Follow::Stays => {}
        }
        Ok(follow)
    }

    /// The update's start (state 3): after a wait `+0x6c` goes from 1 to 2 as it asks again; the
    /// goal is its point or its target; it asks for its path and moves (state 4), whose
    /// `set_state` shows its sprite ([`MoveToAction::sprite`], [`start_walk_shown`]) before the
    /// motion step.
    fn update_start(
        &mut self,
        walk: &mut Walk<'_>,
        ask: PathAsk,
        step: &mut MoveStep,
        shows: &mut Shows<'_>,
    ) -> Result<(), GenieError> {
        self.wait.ask_again();
        let goal = self.walks_to();
        self.state = MoveTo::Moving;
        let shown = Shown {
            sprite: &mut *shows.sprite,
            rng: &mut *shows.rng,
            graphic: self.sprite.or(shows.master_standing),
        };
        let (state, asked) = ask_walk(walk, (goal, self.range.get()), ask, Some(shown))?;
        if let Some(point) = asked {
            self.point = point;
        }
        match state {
            Some(state) => self.state = state,
            None => step.gave_up = true,
        }
        self.ended(walk, shows);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::spatial::Placed;
    use crate::movement::move_to::MoveWaitState;
    use crate::movement::move_to::tests::sheep;
    use crate::objects::SpriteFlags;
    use crate::test_ids::object_id;

    /// A sprite `id` whose speed factor is `factor`, looped.
    const fn graphic(id: i16, factor: f32) -> Graphic {
        Graphic {
            id: SpriteId::named(id),
            frames: 10,
            facets: 16,
            speed_multiplier: factor,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED,
        }
    }

    /// The running sprite (factor 1.5), the walking one (1) and the standing one (1.25).
    const RUNNING: Graphic = graphic(13154, 1.5);
    const WALKING: Graphic = graphic(13153, 1.0);
    const STANDING: Graphic = graphic(13151, 1.25);

    /// A `MoveTo` of its own to the house 10028's point, in `state`, with the sprite `sprite`.
    fn own_move_to(state: MoveTo, sprite: Option<Graphic>, target: i32) -> MoveToAction {
        MoveToAction {
            sprite,
            state,
            point: Position::new(30.0, 88.0, 0.0),
            target: ObjectId::of_field(target),
            range: Distance::new(0.1),
            quiet: false,
            on_target_moved: TargetMoved::Follows,
            wait: MoveWait {
                state: MoveWaitState::Waiting,
                timer: Seconds::ZERO,
                delay: Seconds::new(0.15),
            },
        }
    }

    /// The unit's sprite and the Sprite stream, which a `MoveTo` of its own shows on.
    #[derive(Default)]
    struct Looks {
        sprite: Option<ActiveSprite>,
        rng: Well512,
    }

    impl Looks {
        /// The view of [`Shows`], with [`STANDING`] as slot 0x798's and the master's.
        fn shows(&mut self) -> Shows<'_> {
            Shows {
                sprite: &mut self.sprite,
                rng: &mut self.rng,
                standing: Some(STANDING),
                master_standing: Some(STANDING),
            }
        }

        /// The id of the sprite the unit shows.
        fn shown(&self) -> Option<SpriteId> {
            self.sprite.map(|s| s.graphic.id)
        }
    }

    /// A `MoveTo` of its own (`0x140aa2a70`) with [`WalkRig`]'s walker (its master's speed
    /// 0.8): a start counts a wait's ask (`+0x6c` 1 to 2), asks for its path and turns to state
    /// 4, whose `set_state` (`0x140aa29e0`) shows its sprite (the running one: top speed 0.8 ×
    /// 1.5) before the motion step; a move runs the motion step and keeps moving; one at its
    /// target is done, a failed one reports it; a wait runs by the walk's dt and asks again at
    /// its delay. A target that is gone fails its check (the caller clears it first, [`MoveToAction::check_held`]).
    ///
    /// [`WalkRig`]: crate::movement::tests::WalkRig
    #[test]
    fn a_move_to_of_its_own_starts_moves_and_reports_its_end() {
        use crate::movement::tests::WalkRig;
        let mut rig = WalkRig::on_the_path();
        let mut looks = Looks::default();
        let mut m = own_move_to(MoveTo::Start, Some(RUNNING), -1);
        let step = m
            .update(rig.walk(0), PathAsk::default(), looks.shows())
            .unwrap();
        assert_eq!(step, MoveStep::default());
        assert_eq!(
            (m.state, m.wait.state),
            (MoveTo::Moving, MoveWaitState::AskedAgain)
        );
        assert_eq!(looks.shown(), Some(SpriteId::named(13154)));
        assert_eq!(rig.mover_mut().stats.speed, Speed::new(0.8 * 1.5));
        let step = m
            .update(rig.walk(0), PathAsk::default(), looks.shows())
            .unwrap();
        assert_eq!((m.state, step), (MoveTo::Moving, MoveStep::default()));
        let mut on = own_move_to(MoveTo::Start, Some(WALKING), 10028);
        let _ = on
            .update(rig.walk(0), PathAsk::default(), looks.shows())
            .unwrap();
        assert_eq!(
            (on.state, looks.shown(), rig.mover_mut().stats.speed),
            (
                MoveTo::Moving,
                Some(SpriteId::named(13153)),
                Speed::new(0.8)
            )
        );
        let ended = |state| {
            let mut rig = WalkRig::on_the_path();
            let mut m = own_move_to(state, Some(WALKING), -1);
            let step = m
                .update(rig.walk(0), PathAsk::default(), Looks::default().shows())
                .unwrap();
            (step, m.state)
        };
        let done = MoveStep {
            done: true,
            ..MoveStep::default()
        };
        let failed = MoveStep {
            failed: true,
            ..MoveStep::default()
        };
        assert_eq!(ended(MoveTo::Arrived), (done, MoveTo::Arrived));
        assert_eq!(ended(MoveTo::Failed), (failed, MoveTo::Failed));
        // the walk is a tenth of a second: the wait of 0.15 asks again on its second update
        let mut w = own_move_to(MoveTo::Waiting, Some(WALKING), -1);
        let _ = w
            .update(rig.walk(0), PathAsk::default(), looks.shows())
            .unwrap();
        assert_eq!(
            (w.state, w.wait.timer),
            (MoveTo::Waiting, Seconds::new(0.1))
        );
        let _ = w
            .update(rig.walk(0), PathAsk::default(), looks.shows())
            .unwrap();
        assert_eq!((w.state, w.wait.timer), (MoveTo::Start, Seconds::ZERO));
        let reason = |mut m: MoveToAction| {
            let mut rig = WalkRig::on_the_path();
            m.update(rig.walk(0), PathAsk::default(), Looks::default().shows())
                .unwrap_err()
                .reason()
        };
        assert_eq!(
            reason(own_move_to(MoveTo::Moving, Some(WALKING), 77)),
            "a MoveTo whose target is gone"
        );
    }

    /// The `set_state` of a `MoveTo` of its own (`0x140aa29e0`, not quiet): turning to state 4
    /// shows its sprite and a null one the master's standing sprite (slot 0x68 with none); the
    /// end in range at its start (state 1) shows slot 0x798's sprite and clears `+0x7b8`, and so
    /// does a walk that arrives; the top speed follows each sprite (the master's 0.8 times its
    /// factor).
    #[test]
    fn a_move_to_of_its_own_shows_its_sprite_as_it_moves_and_the_standing_one_at_its_end() {
        use crate::movement::tests::WalkRig;
        let started = |sprite: Option<Graphic>| {
            let mut rig = WalkRig::on_the_path();
            let mut looks = Looks::default();
            let mut m = own_move_to(MoveTo::Start, sprite, -1);
            let _ = m
                .update(rig.walk(0), PathAsk::default(), looks.shows())
                .unwrap();
            (m.state, looks.shown(), rig.mover_mut().stats.speed)
        };
        assert_eq!(
            started(Some(RUNNING)),
            (
                MoveTo::Moving,
                Some(SpriteId::named(13154)),
                Speed::new(0.8 * 1.5)
            )
        );
        assert_eq!(
            started(None),
            (
                MoveTo::Moving,
                Some(SpriteId::named(13151)),
                Speed::new(0.8 * 1.25)
            )
        );
        // at the point already: state 1, which shows the standing sprite
        let mut rig = WalkRig::on_the_path();
        rig.body.set_anim_speed(1.0);
        let mut looks = Looks::default();
        let mut at = own_move_to(MoveTo::Start, Some(RUNNING), -1);
        at.point = Position::new(28.0, 89.0, 0.0);
        let ask = PathAsk {
            ends_in_range: true,
            ..PathAsk::default()
        };
        let _ = at.update(rig.walk(0), ask, looks.shows()).unwrap();
        assert_eq!(at.state, MoveTo::Arrived);
        assert_eq!(looks.shown(), Some(SpriteId::named(13151)));
        assert_eq!(rig.mover_mut().stats.speed, Speed::new(0.8 * 1.25));
        assert_eq!(rig.mover_mut().animated.speed, 0.0);
    }

    /// The sprite a `MoveTo` takes when its maker gives none: `FUN_140aa0d30`'s slot 0x7c8 (a
    /// terrain look's walking sprite first), else slot 0x798; `FUN_140aa0df0`'s master's walking
    /// sprite, else its standing one, never a look's. The unit AI's moves give the running sprite
    /// unless they walk, else the walking one, and none without either.
    #[test]
    fn a_move_to_without_a_sprite_takes_its_constructor_s() {
        let master = Master {
            run_sprite: Some(SpriteId::named(13154)),
            move_sprite: Some(SpriteId::named(13153)),
            standing_sprite: Some(SpriteId::named(13151)),
            ..Master::default()
        };
        assert_eq!(
            point_sprite(&master, None, false),
            Some(SpriteId::named(13153))
        );
        assert_eq!(object_sprite(&master), Some(SpriteId::named(13153)));
        assert_eq!(ai_sprite(&master, false), Some(SpriteId::named(13154)));
        assert_eq!(ai_sprite(&master, true), Some(SpriteId::named(13153)));
        let look = Look {
            standing: Some(SpriteId::named(501)),
            walking: Some(SpriteId::named(500)),
            attacking: None,
            running: None,
            dying: None,
            undead: None,
        };
        assert_eq!(
            point_sprite(&master, Some(&look), false),
            Some(SpriteId::named(500))
        );
        let still = Master {
            run_sprite: None,
            move_sprite: None,
            ..master
        };
        assert_eq!(
            point_sprite(&still, None, false),
            Some(SpriteId::named(13151))
        );
        assert_eq!(object_sprite(&still), Some(SpriteId::named(13151)));
        assert_eq!(ai_sprite(&still, false), None);
        let standing_look = Look {
            walking: None,
            ..look
        };
        assert_eq!(
            point_sprite(&still, Some(&standing_look), false),
            Some(SpriteId::named(501))
        );
    }

    /// The walk of a `MoveTo` of its own on the house 10028 (`0x140aa2a70`, state 4): a target
    /// that stays walks on; one that left the `MoveTo`'s point (by more than the tolerance) has
    /// a walker on its path's last stretch follow it there, and any other tell the unit and turn
    /// the `MoveTo` to state 3 before the motion step.
    #[test]
    fn a_move_to_of_its_own_turns_to_its_start_when_its_target_moved() {
        use crate::movement::tests::WalkRig;
        let walked = |point: Position, waypoint| {
            let mut rig = WalkRig::on_the_path();
            rig.mover_mut().paths[0].waypoint = waypoint;
            let mut m = own_move_to(MoveTo::Moving, Some(WALKING), 10028);
            m.point = point;
            let step = m
                .update(rig.walk(0), PathAsk::default(), Looks::default().shows())
                .unwrap();
            (m.state, step.target_moved)
        };
        assert_eq!(
            walked(Position::new(30.0, 88.0, 0.0), 1),
            (MoveTo::Moving, false)
        );
        assert_eq!(
            walked(Position::new(36.0, 88.0, 0.0), 1),
            (MoveTo::Moving, false)
        );
        assert_eq!(
            walked(Position::new(36.0, 88.0, 0.0), 0),
            (MoveTo::Start, true)
        );
    }

    /// `FUN_140aa0df0` with no range takes the unit's slot 0x268: 0.75 for a building whose
    /// master has a task of action type 0x99 (431096717's mule carts of player 2 at update 1),
    /// 0 for a building without one, for a unit with one, and without a master; a range given
    /// stays.
    #[test]
    fn a_move_to_an_object_without_a_range_takes_a_mule_cart_s() {
        use crate::objects::masters::{Master, Task, master_type::BUILDING};
        let cart_task = Task {
            action_type: crate::construction::build::CART_TASK,
            ..Task::default()
        };
        let cart = Master {
            kind: BUILDING,
            tasks: vec![Task::default(), cart_task],
            ..Master::default()
        };
        let building = Master {
            kind: BUILDING,
            tasks: vec![Task::default()],
            ..Master::default()
        };
        let unit = Master {
            kind: master_type::COMBAT,
            ..cart.clone()
        };
        assert_eq!(object_range(Distance::ZERO, Some(&cart)), 0.75);
        assert_eq!(object_range(Distance::new(-0.0), Some(&cart)), 0.75);
        assert_eq!(object_range(Distance::ZERO, Some(&building)), 0.0);
        assert_eq!(object_range(Distance::ZERO, Some(&unit)), 0.0);
        assert_eq!(object_range(Distance::ZERO, None), 0.0);
        assert_eq!(object_range(Distance::new(0.5), Some(&cart)), 0.5);
        assert!(object_range(Distance::new(f32::NAN), Some(&cart)).is_nan());
    }

    #[test]
    fn a_move_to_an_object_at_hand_starts_arrived_and_shows_the_standing_sprite() {
        let mut world = Spatial::new(GridSize::new(120, 120));
        world.insert(object_id(9185), sheep(30.4, 40.0));
        world.insert(object_id(9186), sheep(35.0, 40.0));
        let mut walker = crate::gathering::hunt::tests::walker();
        // footprints 0 apart: within ARRIVED, state 1 (its owner shows the standing sprite)
        let me = Position::new(30.0, 40.0, 0.0);
        let at_hand = MoveToAction::to_target(
            &world,
            me,
            &mut walker,
            object_id(9185),
            Distance::ZERO,
            None,
        )
        .unwrap();
        assert!(at_hand.arrived());
        // 4.6 tiles on it starts walking (state 3)
        let mut walker = crate::gathering::hunt::tests::walker();
        let far = MoveToAction::to_target(
            &world,
            me,
            &mut walker,
            object_id(9186),
            Distance::ZERO,
            None,
        )
        .unwrap();
        assert!(!far.arrived());
        assert_eq!(walker.stats.speed, Speed::new(0.8));
    }

    /// `FUN_140aa0df0` on an object: a live target is held (the caller takes it); a target in
    /// state 7 is not (`FUN_140a7a230`) and the `MoveTo` walks to its place; livestock moving to
    /// a town centre walks to its place less 0.2 on both axes without a target. Without a target
    /// the start is state 3, or 13 for a unit without speed.
    #[test]
    fn a_move_to_an_object_holds_it_unless_it_is_gone_or_a_town_centre_for_livestock() {
        let mut world = Spatial::new(GridSize::new(120, 120));
        world.insert(object_id(9186), sheep(35.0, 40.0));
        world.insert(
            object_id(9149),
            Placed {
                master: TOWN_CENTRE,
                radius: Footprint::new(2.0, 2.0),
                ..sheep(50.0, 40.0)
            },
        );
        let me = Position::new(30.0, 40.0, 0.0);
        let to = |target, livestock, speed: f32| {
            let mut walker = crate::gathering::hunt::tests::walker();
            walker.stats.speed = Speed::new(speed);
            MoveToAction::to_object(
                &world,
                me,
                &mut walker,
                target,
                Distance::ZERO,
                (None, livestock),
            )
            .unwrap()
        };
        let held = to((object_id(9186), ObjectState::Alive), false, 0.8);
        assert_eq!(
            (ObjectId::field(held.target), held.goal(), held.state),
            (9186, None, MoveTo::Start)
        );
        let gone = to((object_id(9186), ObjectState::Removed), false, 0.8);
        assert_eq!(
            (ObjectId::field(gone.target), gone.goal(), gone.state),
            (-1, Some(Position::new(35.0, 40.0, 0.0)), MoveTo::Start)
        );
        assert_eq!(
            to((object_id(9186), ObjectState::Removed), false, 0.0).state,
            MoveTo::Failed
        );
        // a villager to the town centre holds it; livestock walks beside it
        assert_eq!(
            to((object_id(9149), ObjectState::Alive), false, 0.8).target,
            Some(object_id(9149))
        );
        let beside = to((object_id(9149), ObjectState::Alive), true, 0.8);
        assert_eq!(
            (ObjectId::field(beside.target), beside.goal()),
            (-1, Some(Position::new(50.0 - 0.2, 40.0 - 0.2, 0.0)))
        );
    }

    /// `FUN_140aa0d30` and its start `0x140aa2100`: a point is never within 0.01 (no target, so
    /// `f32::MAX`): state 3 for a unit with speed (a NaN speed has some), 13 for one without,
    /// whose not quiet `set_state` shows the standing sprite (its speed); without movement state
    /// a unit has no speed. Its point is `+0x38`.
    #[test]
    fn a_move_to_a_point_starts_unless_its_unit_has_no_speed() {
        let point = Position::new(12.0, 34.0, 1.0);
        let start = |speed: f32| {
            let mut walker = crate::gathering::hunt::tests::walker();
            walker.stats.speed = Speed::new(speed);
            let m =
                MoveToAction::to_point(point, Distance::new(0.5), Some(RUNNING), Some(&mut walker));
            (m, walker.stats.speed)
        };
        let (moving, speed) = start(0.8);
        assert_eq!(
            (
                moving.state,
                moving.goal(),
                ObjectId::field(moving.target),
                moving.range,
                moving.sprite
            ),
            (
                MoveTo::Start,
                Some(point),
                -1,
                Distance::new(0.5),
                Some(RUNNING)
            )
        );
        assert_eq!(speed, Speed::new(0.8));
        assert!(!moving.shows_standing());
        assert_eq!(moving.point, point);
        assert_eq!(start(f32::NAN).0.state, MoveTo::Start);
        let (stuck, _) = start(0.0);
        assert_eq!(stuck.state, MoveTo::Failed);
        assert!(stuck.shows_standing());
        let none = MoveToAction::to_point(point, Distance::ZERO, None, None);
        assert_eq!(none.state, MoveTo::Failed);
    }

    /// `FUN_140aa0df0` puts the target's place in `+0x38` (its point); a unit
    /// without speed away from the target fails at the start (state 13), which shows the
    /// standing sprite as an arrival does.
    #[test]
    fn a_move_to_an_object_holds_its_place_and_a_unit_without_speed_fails_standing() {
        let mut world = Spatial::new(GridSize::new(120, 120));
        world.insert(object_id(9186), sheep(35.0, 40.0));
        let me = Position::new(30.0, 40.0, 0.0);
        let mut walker = crate::gathering::hunt::tests::walker();
        let far = MoveToAction::to_target(
            &world,
            me,
            &mut walker,
            object_id(9186),
            Distance::ZERO,
            None,
        )
        .unwrap();
        assert_eq!(far.point, Position::new(35.0, 40.0, 0.0));
        assert_eq!(walker.stats.speed, Speed::new(0.8));
        walker.stats.speed = Speed::ZERO;
        let stuck = MoveToAction::to_target(
            &world,
            me,
            &mut walker,
            object_id(9186),
            Distance::ZERO,
            None,
        )
        .unwrap();
        assert_eq!(stuck.state, MoveTo::Failed);
        assert!(stuck.shows_standing());
    }
}
