//! The `MoveTo` action (`RGE_Action_Move_To`, type 1, vtable `0x142ed6030`): a unit walks to a target
//! object or a point. Builders, birds and wandering animals run it, as a sub-action or on its own.
//!
//! 158041: start (slot 5) `0x140aa2100`, update (slot 6) `0x140aa2a70`, `set_state` (slot 16)
//! `0x140aa29e0`. A `MoveTo` in state 3 asks for a path (`FUN_14114e410`) and turns to state 4; in
//! state 4 each update runs the motion step (object slot 193, [`Mover::motion`]).

use crate::GenieError;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::Rect;
use crate::map::spatial::Spatial;
use crate::math::rng::Well512;
use crate::movement::{Me, Motion, Planning, footprint_gap, footprint_gap_squared};
use crate::movement::{MoveStats, PathAsk};
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::layers::Body;
use crate::objects::masters::ActionType;
use crate::objects::sprite::{ActiveSprite, Graphic, new_sprite};
use crate::unit_ai::UnitStance;
use crate::world::Seconds;
use crate::world::WorldTime;
use genie_diff::Diff;

mod action;
mod constructors;
mod list;
mod move_wait_state;
mod sub_action;

pub(crate) use crate::movement::move_to::action::TARGET_LACKING;
pub use crate::movement::move_to::action::{
    MoveDefaults, MoveStep, MoveToAction, Shows, ai_sprite, holds_object, object_range,
    object_sprite, point_sprite,
};
pub use crate::movement::move_to::constructors::{MoveToReads, WalkTo};
pub use crate::movement::move_to::list::ListUpdate;
pub(crate) use crate::movement::move_to::list::update_sub_actions;
pub use crate::movement::move_to::move_wait_state::MoveWaitState;
pub use crate::movement::move_to::sub_action::{GiveUp, NO_TARGET_GAP};

/// The `MoveTo` action's type, which word 9 adds when it is a unit's top action.
pub const MOVE_TO: ActionType = ActionType::named(1);

/// The `MoveTo`'s state (`+0x18`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Diff)]
pub enum MoveTo {
    /// 1: at the target when it started.
    Arrived,
    /// 3: it starts to move (`0x140aa2100`).
    Start,
    /// 4: its first update asked for a path; the unit moves from the next update on.
    Moving,
    /// 2: the motion step gave up and the `MoveTo` waits before it asks again (its timer `+0x70`
    /// up to `+0x74`, the owner's [`MoveWait`]).
    Waiting,
    /// 13: it failed; its update returns 3 (with notify 0x1f9 unless it is quiet).
    Failed,
}

/// A `MoveTo`'s wait after the motion step gave up (`+0x6c` `movement_wait_state`, the timer
/// `+0x70` and its end `+0x74`), held by the action that owns
/// the `MoveTo`; a new `MoveTo` starts without one ([`MoveWait::default`]).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct MoveWait {
    /// `+0x6c`, `movement_wait_state`.
    pub state: MoveWaitState,
    /// `+0x70`: the time waited.
    pub timer: Seconds,
    /// `+0x74`: how long it waits.
    pub delay: Seconds,
}

/// `kDefaultWaitTime`, the wait a `MoveTo`'s constructors set (`+0x74`, `FUN_140aa0d30`,
/// `FUN_140aa0df0`): one second.
const DEFAULT_WAIT: Seconds = Seconds::new(1.0);

impl Default for MoveWait {
    fn default() -> Self {
        Self::NEW
    }
}

impl MoveWait {
    /// A new `MoveTo`'s: never waited, no time waited, [`DEFAULT_WAIT`].
    pub const NEW: Self = Self {
        state: MoveWaitState::Never,
        timer: Seconds::ZERO,
        delay: DEFAULT_WAIT,
    };

    /// The `MoveTo`'s answer to the motion step giving up (it returns 1, `0x140aa2a70` in state
    /// 4): a human player's villager (`waits`) whose `MoveTo` has not waited yet (`+0x6c` 0) waits
    /// (`set_state` 2, `+0x6c` 1) a second with a stance (`+0x1a9`) below 2, else two, then asks
    /// again; any other `MoveTo` fails (`set_state` 13). Returns the `MoveTo`'s state.
    pub const fn give_up(&mut self, waits: bool, stance: UnitStance) -> MoveTo {
        if !matches!(self.state, MoveWaitState::Never) || !waits {
            return MoveTo::Failed;
        }
        self.delay = if stance.below(UnitStance::STAND_GROUND) {
            Seconds::new(1.0)
        } else {
            Seconds::new(2.0)
        };
        self.state = MoveWaitState::Waiting;
        MoveTo::Waiting
    }

    /// The `MoveTo`'s start (state 3) asks for its path: one that waited has asked again, and
    /// never waits again.
    pub const fn ask_again(&mut self) {
        if matches!(self.state, MoveWaitState::Waiting) {
            self.state = MoveWaitState::AskedAgain;
        }
    }
}

impl MoveTo {
    /// The state's number, which word 9 adds.
    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Arrived => 1,
            Self::Start => 3,
            Self::Moving => 4,
            Self::Waiting => 2,
            Self::Failed => 13,
        }
    }
}

/// `+0x61` (`mTargetMovedState`, the save's `target_moved_state`): what a moving `MoveTo` does
/// when its target object left its point (`0x140aa2a70` in state 4, [`target_moved`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Diff)]
pub enum TargetMoved {
    /// 0, the base constructor's (`FUN_140a73e90`): the path's end follows the target when the
    /// unit walks to it and the line to the target is clear; else the `MoveTo` tells the unit
    /// (notify 0x1fc), asks again (state 3) and the motion step runs on.
    Follows,
    /// 1: it asks again (state 3), and its update ends there.
    AsksAgain,
    /// 2: it walks on.
    Ignores,
}

impl TryFrom<u8> for TargetMoved {
    type Error = GenieError;

    /// `+0x61` as the save holds it.
    fn try_from(value: u8) -> Result<Self, GenieError> {
        match value {
            0 => Ok(Self::Follows),
            1 => Ok(Self::AsksAgain),
            2 => Ok(Self::Ignores),
            other => Err(GenieError::invalid_input(
                "a MoveTo's target_moved_state outside 0, 1 and 2",
            )
            .with("target_moved_state", other)),
        }
    }
}

/// What a walking unit's update needs: its layers below the action layer (its place and, if it
/// moves, its movement state), who it is, and the world; the planners and the world's time for
/// a path request (None where the caller has none).
#[derive(Debug)]
pub struct Walk<'a> {
    /// The unit's layers: its place ([`Body::position`]) and its mover, if it moves.
    pub body: &'a mut Body,
    /// The unit.
    pub me: Me,
    /// The world's dt in seconds.
    pub dt: Seconds,
    /// The objects' places; the motion step moves the unit onto a waypoint it reached.
    pub world: &'a mut Spatial,
    /// The planners and the world's time for a path request, None where the caller has none.
    pub plan: Option<Planning<'a>>,
}

impl Walk<'_> {
    /// x, y, z on the map: the unit's place as it is now, after any step of its walk.
    #[must_use]
    pub const fn position(&self) -> Position {
        self.body.position()
    }

    /// Sets the walking unit's animation speed (`+0x7b8`, the animated layer in its mover).
    pub const fn set_anim_speed(&mut self, speed: f32) {
        if let Some(m) = self.body.mover_mut() {
            m.animated.speed = speed;
        }
    }

    /// The same walk for a call that ends before this one is used again.
    pub fn reborrow(&mut self) -> Walk<'_> {
        Walk {
            body: &mut *self.body,
            me: self.me,
            dt: self.dt,
            world: &mut *self.world,
            plan: self.plan.as_mut().map(|p| p.reborrow()),
        }
    }
}

/// Where a `MoveTo` goes: a target object or a point (`x, y, z` on the map).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub enum Goal {
    /// A target object.
    Target(ObjectId),
    /// A point.
    Point(Position),
}

/// A starting `MoveTo`'s update (state 3, `0x140aa2a70`): unless the unit is within `range` of its
/// goal already, it asks for a path (`FUN_14114e410`,
/// [`crate::movement::Mover::request_path_asked`] with what the unit's AI says, `ask`) and turns to
/// state 4, whose `set_state` (`0x140aa29e0`) shows the `MoveTo`'s sprite (`shown`, `+0x58`, with
/// slot 0x68: [`Shown::show`]) and runs the update again at once ([`walk_or_give_up`]), so the
/// motion step reads the top speed with that sprite. Returns the `MoveTo`'s state after it, None
/// when the motion step gave up in that update (the `MoveTo` is in state 4, its sprite shown; the
/// owner's answer decides what follows). In range is the footprints' squared gap (`FUN_14118f8c0`)
/// to a target object, the gap from the unit's footprint to a point, at most `range` squared. A
/// unit already in range ends the `MoveTo` (`set_state` 1, code 0, none of its sprite shown) when
/// [`PathAsk::ends_in_range`] (a unit AI not on order 711 or 733); a `MoveTo` that is not `quiet`
/// (its `+0x60` clear) then shows the unit's idle sprite (slot 0x68 with slot 0x798's: the master's
/// standing sprite, [`crate::movement::Mover::shows_standing`]; the sprite itself and `+0x7b8`,
/// cleared, are the caller's).
///
/// Returns the `MoveTo`'s state after it, and the place it asked a path toward (None when it
/// ended in range): its point (`+0x38`..`+0x40`), which for a target object state 3 takes from
/// the target before the path request.
///
/// # Errors
///
/// A target that is gone, a unit that does not move, and what the motion step does not model.
pub fn ask_walk(
    walk: &mut Walk<'_>,
    (goal, range): (Goal, f32),
    ask: PathAsk,
    shown: Option<Shown<'_>>,
) -> Result<(Option<MoveTo>, Option<Position>), GenieError> {
    let (point, target, radius) = match goal {
        Goal::Target(id) => {
            let t = walk
                .world
                .get_known(id)?
                .ok_or(GenieError::unsupported("a MoveTo whose target is gone"))?;
            (t.position, Some(id), t.radius)
        }
        Goal::Point(p) => (p, None, Footprint::ZERO),
    };
    let now = walk.plan.as_ref().map_or(WorldTime::ZERO, |p| p.now);
    let mover = walk.body.mover_mut().ok_or(crate::world::WALKER_UNMOVING)?;
    let here = mover.position();
    let ground = mover.stats.radius.ground();
    let squared = if target.is_none() {
        let gap = footprint_gap(here, ground, point, radius);
        gap.squared().get()
    } else {
        footprint_gap_squared(here, ground, point, radius).get()
    };
    if ask.ends_in_range && squared <= range * range {
        return Ok((Some(MoveTo::Arrived), None));
    }
    mover.request_path_asked(here, point, Distance::new(range), target, now, ask);
    if let Some(mut shown) = shown {
        shown.show(walk.body.move_stats_mut());
    }
    // the MoveTo's point (`+0x38`): the target's place when it asks for the path
    Ok((walk_or_give_up(walk.reborrow())?, Some(point)))
}

/// A `MoveTo`'s sprite outside the sprites' table.
const SPRITE_LACKING: GenieError =
    GenieError::missing_data("a MoveTo's sprite outside the sprites' table");

/// The sprite `id` (None for none) from the sprites' table `graphics`.
///
/// # Errors
///
/// A sprite the table lacks.
pub fn graphic_of(
    graphics: &std::collections::BTreeMap<SpriteId, Graphic>,
    id: Option<SpriteId>,
) -> Result<Option<Graphic>, GenieError> {
    id.map(|id| {
        graphics
            .get(&id)
            .copied()
            .ok_or_else(|| SPRITE_LACKING.with("sprite", id))
    })
    .transpose()
}

/// What a `MoveTo`'s constructor reads for its sprite (`+0x58`) when its maker gives none: the
/// defaults of the unit's master and terrain look, and the sprites' table that holds them.
#[derive(Clone, Copy, Debug)]
pub struct SpriteReads<'a> {
    /// The constructors' defaults ([`MoveDefaults::of`]).
    pub defaults: MoveDefaults,
    /// The sprites' table.
    pub graphics: &'a std::collections::BTreeMap<SpriteId, Graphic>,
}

impl<'a> SpriteReads<'a> {
    /// The defaults of a unit of `master` with the terrain look `look` ([`MoveDefaults::of`]),
    /// and the sprites' table.
    #[must_use]
    pub fn of(
        master: &crate::objects::masters::Master,
        look: Option<&crate::combat::abilities::Look>,
        graphics: &'a std::collections::BTreeMap<SpriteId, Graphic>,
    ) -> Self {
        Self {
            defaults: MoveDefaults::of(master, look),
            graphics,
        }
    }

    /// The sprite of a `MoveTo` to a point (`FUN_140aa0d30`) whose maker gives `given`.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn to_point(&self, given: Option<SpriteId>) -> Result<Option<Graphic>, GenieError> {
        graphic_of(self.graphics, self.defaults.to_point(given))
    }

    /// The sprite of a `MoveTo` to an object (`FUN_140aa0df0`) whose maker gives `given`.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn to_object(&self, given: Option<SpriteId>) -> Result<Option<Graphic>, GenieError> {
        graphic_of(self.graphics, self.defaults.to_object(given))
    }
}

/// The unit's sprite, the Sprite stream and the sprites' table: an action that holds its
/// `MoveTo`'s sprite as an id shows it through them ([`Sprites::shown`]).
#[derive(Debug)]
pub struct Sprites<'a> {
    /// The unit's sprite.
    pub sprite: &'a mut Option<ActiveSprite>,
    /// The Sprite stream.
    pub rng: &'a mut Well512,
    /// The sprites' table.
    pub graphics: &'a std::collections::BTreeMap<SpriteId, Graphic>,
}

impl Sprites<'_> {
    /// The view that shows the sprite `id` (None for none) at a `MoveTo`'s `set_state(4)`.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn shown(&mut self, id: Option<SpriteId>) -> Result<Shown<'_>, GenieError> {
        let graphic = graphic_of(self.graphics, id)?;
        Ok(Shown {
            sprite: &mut *self.sprite,
            rng: &mut *self.rng,
            graphic,
        })
    }
}

/// The unit's sprite, the Sprite stream and the sprite a walk shows: a `MoveTo` that turns to
/// state 4 shows its sprite (`0x140aa29e0`, slot 0x68 with `+0x58`).
#[derive(Debug)]
pub struct Shown<'a> {
    /// The unit's sprite.
    pub sprite: &'a mut Option<ActiveSprite>,
    /// The Sprite stream.
    pub rng: &'a mut Well512,
    /// The `MoveTo`'s sprite.
    pub graphic: Option<Graphic>,
}

impl Shown<'_> {
    /// The `MoveTo` turned to state 4: the unit shows its sprite ([`new_sprite`]) and reads its
    /// top speed (`stats`) with it. A `MoveTo` without one (`+0x58` null) is not modelled: slot
    /// 0x68 would show the master's standing sprite (`+0x470`); genie shows nothing.
    pub fn show(&mut self, stats: Option<&mut MoveStats>) {
        if let Some(g) = self.graphic {
            new_sprite(self.sprite, stats, g, self.rng);
        }
    }
}

/// A moving `MoveTo`'s update (state 4, `0x140aa2a70` from `LAB_140aa320c`): the motion step
/// (slot 193); returns the `MoveTo`'s state after it, None when the motion step gave up (it
/// returns 1; the owner decides: `set_state` 2 or 13). A unit that arrived ends the `MoveTo`
/// (`set_state` 1, `0x140aa29e0`, which also sets the unit's idle sprite and clears its `+0x7b8`,
/// and `FUN_14114d810` its facet; the slice holds neither). A `MoveTo` without its path is not
/// modelled.
pub fn walk_or_give_up(walk: Walk<'_>) -> Result<Option<MoveTo>, GenieError> {
    let Walk {
        body,
        me,
        dt,
        world,
        mut plan,
    } = walk;
    let mover = body.mover_mut().ok_or(crate::world::WALKER_UNMOVING)?;
    if mover.paths.is_empty() {
        return Err(GenieError::unsupported("the path request (FUN_14114e410)"));
    }
    Ok(match mover.motion(&me, dt, world, plan.as_mut())? {
        Motion::Moving => Some(MoveTo::Moving),
        Motion::Arrived => Some(MoveTo::Arrived),
        Motion::Done => None,
    })
}

/// A target leaves the `MoveTo`'s point when it is more than this off it on either axis
/// (`DAT_142eb6e58`, `DAT_142e7f384`).
const OFF_LOW: f32 = crate::math::image::move_to::OFF_LOW;
const OFF_HIGH: f32 = crate::math::image::move_to::OFF_HIGH;

/// What [`target_moved`] found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Follow {
    /// The target stayed on the `MoveTo`'s point, or the `MoveTo` ignores its moves
    /// ([`TargetMoved::Ignores`]).
    Stays,
    /// The `MoveTo` turns to state 3: it asks for a path again, and its update ends there
    /// ([`TargetMoved::AsksAgain`]).
    Again,
    /// The path's end did not follow ([`TargetMoved::Follows`]): the `MoveTo` tells the unit
    /// (notify 0x1fc), turns to state 3 and the motion step runs on.
    Blocked,
    /// The path's end follows the target to this point ([`Mover::follow_path_end`]); the walk goes on.
    PathEnd(Position),
}

impl Follow {
    /// The `MoveTo` turns to state 3 and its update ends there.
    #[must_use]
    pub const fn again(self) -> bool {
        matches!(self, Self::Again)
    }
}

/// The moving `MoveTo`'s check of its target object (`0x140aa2a70` in state 4, before the motion
/// step) against its point (`point`, `+0x38`..`+0x40`), by its `+0x61` (`mode`). A `MoveTo` that ignores its target's moves
/// ([`TargetMoved::Ignores`]) walks on. Otherwise a target that left the `MoveTo`'s point (more
/// than half a tile off on either axis) makes a `MoveTo` that asks again
/// ([`TargetMoved::AsksAgain`]: a hunt's and its Attack's, a drop-off's) turn to state 3 (the
/// slice's [`MoveTo::Start`]), and its update ends there ([`Follow::Again`]). One that follows
/// ([`TargetMoved::Follows`]: a `MoveTo` of its own, FOLLOW's, a unit's own Attack's) moves its
/// path's end to the target ([`Mover::follow_path_end`], its point the target's place) when the
/// unit walks to that end ([`Mover::walks_to_path_end`]) and the straight line from the unit to
/// the target is clear (`FUN_141052b90`, then `FUN_141052350` with flags `0x100`,
/// [`Spatial::segment_blocked`]: the obstruction manager's buffer and ignore list as the last
/// `FUN_1410562d0` and `FUN_1410562e0` left them, [`Spatial::manager_grow`],
/// [`Spatial::manager_ignore`]), and walks on; else it tells the unit (notify 0x1fc), turns to
/// state 3 and the motion step runs on ([`Follow::Blocked`]).
///
/// # Errors
///
/// A target the world lacks, a walker without movement state.
pub fn target_left(
    walk: &mut Walk<'_>,
    target: ObjectId,
    point: Position,
    mode: TargetMoved,
) -> Result<Follow, GenieError> {
    if mode == TargetMoved::Ignores {
        return Ok(Follow::Stays);
    }
    let t = walk
        .world
        .get_known(target)?
        .ok_or(GenieError::unsupported("a MoveTo whose target is gone"))?;
    let at = t.position;
    let (dx, dy) = (point.x() - at.x(), point.y() - at.y());
    if !(dx < OFF_LOW || OFF_HIGH < dx || dy < OFF_LOW || OFF_HIGH < dy) {
        return Ok(Follow::Stays);
    }
    if mode == TargetMoved::AsksAgain {
        return Ok(Follow::Again);
    }
    let mover = walk.body.mover().ok_or(crate::world::WALKER_UNMOVING)?;
    if mover.walks_to_path_end()? {
        let me = walk.position();
        let (a, b) = (me.ground(), at.ground());
        let grow = walk.world.manager_grow();
        walk.world.check_known_records(Rect::new(
            a.x().min(b.x()) - grow.get(),
            a.y().min(b.y()) - grow.get(),
            a.x().max(b.x()) + grow.get(),
            a.y().max(b.y()) + grow.get(),
        ))?;
        let ignore = walk.world.manager_ignore();
        if !walk
            .world
            .segment_blocked(a, b, grow, mover.stats.restriction_mask, &ignore)
        {
            return Ok(Follow::PathEnd(at));
        }
    }
    Ok(Follow::Blocked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::spatial::Placed;
    use crate::math::Angle;
    use crate::movement::Speed;
    use crate::objects::ActionState;
    use crate::objects::ObjectState;
    use crate::objects::masters::ObstructionType;
    use crate::objects::masters::class;
    use crate::players::PlayerId;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;

    /// A `MoveTo` sub-action ([`MoveToAction::sub_update`]) on house 10028 within 0.1 (the
    /// walker 0.8 off it) with [`WalkRig`]'s walker: a start asks for its path (a `MoveTo` that
    /// waited once, `+0x6c` 1, counts as having asked again, 2) and moves; a move runs the motion
    /// step and keeps moving; both give code 0. A wait runs by the walk's dt and asks again at
    /// its delay (a NaN timer waits on). A `MoveTo` that ended gives 1, a failed one 3; one to a
    /// target the world lacks is an error.
    ///
    /// [`WalkRig`]: crate::movement::tests::WalkRig
    #[test]
    fn a_sub_move_to_starts_moves_waits_and_reports_its_code() {
        use crate::movement::tests::WalkRig;
        let mut rig = WalkRig::on_the_path();
        let mut m = MoveToAction {
            point: Position::new(30.0, 88.0, 0.0),
            target: Some(object_id(10028)),
            range: Distance::new(0.1),
            wait: MoveWait {
                state: MoveWaitState::Waiting,
                timer: Seconds::ZERO,
                delay: Seconds::new(1.0),
            },
            ..MoveToAction::sub_in(MoveTo::Start)
        };
        let gives_up = GiveUp {
            waits: true,
            stance: UnitStance::AGGRESSIVE,
        };
        let mut ai = None;
        let mut update = |m: &mut MoveToAction| {
            let mut inbox = crate::unit_ai::Inbox::of(&mut ai, object_id(9177), ObjectState::Alive);
            // the house 10028 stands (state 2)
            m.sub_update(
                &mut rig.walk(4),
                PathAsk::default(),
                gives_up,
                Some(ObjectState::Alive),
                None,
                &mut inbox,
            )
        };
        assert_eq!(update(&mut m), Ok(0));
        assert_eq!(
            (m.state, m.wait.state),
            (MoveTo::Moving, MoveWaitState::AskedAgain)
        );
        assert_eq!(update(&mut m), Ok(0));
        assert_eq!(m.state, MoveTo::Moving);
        // the walk is a tenth of a second: the wait of 0.15 asks again on its second update
        m.state = MoveTo::Waiting;
        m.wait = MoveWait {
            state: MoveWaitState::Waiting,
            timer: Seconds::ZERO,
            delay: Seconds::new(0.15),
        };
        assert_eq!(update(&mut m), Ok(0));
        assert_eq!(
            (m.state, m.wait.timer),
            (MoveTo::Waiting, Seconds::new(0.1))
        );
        assert_eq!(update(&mut m), Ok(0));
        assert_eq!((m.state, m.wait.timer), (MoveTo::Start, Seconds::ZERO));
        m.wait.timer = Seconds::new(f32::NAN);
        m.state = MoveTo::Waiting;
        assert_eq!(update(&mut m), Ok(0));
        assert_eq!(m.state, MoveTo::Waiting);
        for (state, code) in [(MoveTo::Arrived, 1), (MoveTo::Failed, 3)] {
            m.state = state;
            assert_eq!(update(&mut m), Ok(code));
        }
        let mut lost = MoveToAction {
            target: Some(object_id(4242)),
            ..MoveToAction::sub_in(MoveTo::Start)
        };
        assert!(update(&mut lost).is_err());
    }

    /// The 4v4's woodcutter 31101 (a human player's villager, stance 0) whose Attack's `MoveTo`
    /// the motion step gave up on in update 5101: it waits a second, `+0x6c` 1; once it waited it
    /// fails, as a computer player's or another class's at once. A stance of 2 waits two.
    #[test]
    fn a_human_villager_s_moveto_waits_once_before_it_fails() {
        let mut wait = MoveWait::default();
        assert_eq!(wait.give_up(true, UnitStance::AGGRESSIVE), MoveTo::Waiting);
        assert_eq!(
            (wait.state, wait.delay),
            (MoveWaitState::Waiting, Seconds::new(1.0))
        );
        assert_eq!(wait.give_up(true, UnitStance::AGGRESSIVE), MoveTo::Failed);
        assert_eq!(
            MoveWait::default().give_up(false, UnitStance::AGGRESSIVE),
            MoveTo::Failed
        );
        let mut guarded = MoveWait::default();
        assert_eq!(
            guarded.give_up(true, UnitStance::STAND_GROUND),
            MoveTo::Waiting
        );
        assert_eq!(guarded.delay, Seconds::new(2.0));
    }

    /// The start's ask for the path (state 3) turns a wait into an ask again; a `MoveTo` that
    /// never waited, or asked again already, keeps its wait state, and one that asked again
    /// never waits again.
    #[test]
    fn a_moveto_that_waited_asks_again_once() {
        let mut wait = MoveWait::default();
        wait.ask_again();
        assert_eq!(wait.state, MoveWaitState::Never);
        assert_eq!(wait.give_up(true, UnitStance::AGGRESSIVE), MoveTo::Waiting);
        wait.ask_again();
        assert_eq!(wait.state, MoveWaitState::AskedAgain);
        wait.ask_again();
        assert_eq!(wait.state, MoveWaitState::AskedAgain);
        assert_eq!(wait.give_up(true, UnitStance::AGGRESSIVE), MoveTo::Failed);
    }

    pub(super) fn sheep(x: f32, y: f32) -> Placed {
        Placed {
            position: Position::new(x, y, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::PREDATOR,
            player: PlayerId::new(1),
            master: master_id(594),
            obstruction_type: ObstructionType::NONE,
            buildable_on: 0,
            tall: true,
            speed: Speed::ZERO,
            unknown: None,
            obstruction: None,
        }
    }

    /// A moving sub-action `MoveTo` on house 10028 (at (30, 88)) whose point is 0.6 off it
    /// ([`MoveToAction::sub_update`] in state 4, by `+0x61`): one that asks again turns to state 3
    /// and its update ends before the motion step; one that ignores the move walks on; one that
    /// follows moves its path's end to the house over open tiles and walks on, and past a wall
    /// tells the unit (notify 0x1fc, though quiet), turns to state 3 and walks on. Each gives
    /// code 0. The motion step's first update turns the unit toward its path (its angle).
    #[test]
    fn a_moving_sub_move_to_s_target_that_left_its_point_is_followed_asked_for_or_ignored() {
        use crate::movement::tests::{WalkRig, tiled_world};
        let walk_on = |mode, walled: bool| {
            let mut rig = WalkRig::on_the_path();
            if walled {
                let mut classes = vec![4; 120 * 120];
                for y in 85..92 {
                    classes[y * 120 + 29] = 1;
                }
                rig.world = tiled_world(classes);
            }
            let mut m = MoveToAction {
                point: Position::new(30.6, 88.0, 0.0),
                target: Some(object_id(10028)),
                range: Distance::new(0.1),
                on_target_moved: mode,
                ..MoveToAction::sub_in(MoveTo::Moving)
            };
            let mut ai = Some(crate::unit_ai::UnitAi::default());
            let mut inbox = crate::unit_ai::Inbox::of(&mut ai, object_id(9177), ObjectState::Alive);
            let code = m.sub_update(
                &mut rig.walk(4),
                PathAsk::default(),
                GiveUp::default(),
                Some(ObjectState::Alive),
                None,
                &mut inbox,
            );
            let posted: Vec<crate::unit_ai::NotifyKind> =
                ai.map_or(vec![], |ai| ai.notifies.iter().map(|n| n.kind).collect());
            (
                code,
                m.state,
                posted,
                rig.mover_mut().angle != Angle::ZERO,
                m.point,
            )
        };
        let off = Position::new(30.6, 88.0, 0.0);
        assert_eq!(
            walk_on(TargetMoved::AsksAgain, false),
            (Ok(0), MoveTo::Start, vec![], false, off)
        );
        assert_eq!(
            walk_on(TargetMoved::Ignores, false),
            (Ok(0), MoveTo::Moving, vec![], true, off)
        );
        assert_eq!(
            walk_on(TargetMoved::Follows, false),
            (
                Ok(0),
                MoveTo::Moving,
                vec![],
                true,
                Position::new(30.0, 88.0, 0.0)
            )
        );
        assert_eq!(
            walk_on(TargetMoved::Follows, true),
            (
                Ok(0),
                MoveTo::Start,
                vec![crate::unit_ai::NotifyKind::TARGET_MOVED],
                true,
                off
            )
        );
    }

    /// `0x140aa2a70`'s first check of its target by the target's state: a `MoveTo` to a point
    /// checks nothing; a target the world no longer holds is cleared without a code or a
    /// notify (`FUN_140a78450`); one in state 6 or below stays; one above 6 is let go, a quiet
    /// `MoveTo`'s update ending with 5, another's with 4 after notify 0x1fb (the move action
    /// value, the id let go, −1, and 0). A sub-action `MoveTo` runs it first and gives its 5.
    #[test]
    fn a_move_to_lets_a_dying_target_go_and_walks_on_from_one_the_world_lost() {
        let checked = |quiet: bool, target: i32, state: Option<ObjectState>| {
            let mut m = MoveToAction {
                target: ObjectId::of_field(target),
                quiet,
                ..MoveToAction::sub_in(MoveTo::Moving)
            };
            // a moving unit's AI: an invalidation of its move joins its queue
            let mut ai = Some(crate::unit_ai::UnitAi {
                current_action: Some(crate::unit_ai::ActionKind::Move),
                ..crate::unit_ai::UnitAi::default()
            });
            let mut inbox = crate::unit_ai::Inbox::of(&mut ai, object_id(9177), ObjectState::Alive);
            let code = m.check_held(state, &mut inbox).unwrap();
            let posted: Vec<(crate::unit_ai::NotifyKind, [i32; 3])> = ai.map_or(vec![], |ai| {
                ai.notifies.iter().map(|n| (n.kind, n.p)).collect()
            });
            (code, ObjectId::field(m.target), posted)
        };
        assert_eq!(
            checked(true, -1, Some(ObjectState::Gone)),
            (None, -1, vec![])
        );
        assert_eq!(checked(false, 10028, None), (None, -1, vec![]));
        assert_eq!(
            checked(false, 10028, Some(ObjectState::Undead)),
            (None, 10028, vec![])
        );
        assert_eq!(
            checked(true, 10028, Some(ObjectState::Removed)),
            (Some(5), -1, vec![])
        );
        let invalidated = (
            crate::unit_ai::NotifyKind::ACTION_INVALIDATED,
            [0x262, -1, 0],
        );
        assert_eq!(
            checked(false, 10028, Some(ObjectState::Removed)),
            (Some(4), -1, vec![invalidated])
        );
        // through the sub-action's update, before its state's case
        let mut rig = crate::movement::tests::WalkRig::on_the_path();
        let mut m = MoveToAction {
            target: Some(object_id(10028)),
            ..MoveToAction::sub_in(MoveTo::Moving)
        };
        let mut ai = None;
        let mut inbox = crate::unit_ai::Inbox::of(&mut ai, object_id(9177), ObjectState::Alive);
        let ask = PathAsk::default();
        let code = m.sub_update(
            &mut rig.walk(4),
            ask,
            GiveUp::default(),
            Some(ObjectState::Gone),
            None,
            &mut inbox,
        );
        assert_eq!(
            (code, ObjectId::field(m.target), m.state),
            (Ok(5), -1, MoveTo::Moving)
        );
        let mut loud = MoveToAction {
            target: Some(object_id(10028)),
            quiet: false,
            ..MoveToAction::sub_in(MoveTo::Moving)
        };
        let code = loud.sub_update(
            &mut rig.walk(4),
            ask,
            GiveUp::default(),
            Some(ObjectState::Gone),
            None,
            &mut inbox,
        );
        assert!(code.is_err());
    }

    /// `0x140aa2a70` in state 1 posts notify 0x1fa and in state 13 notify 0x1f9, each with the
    /// move action value (0x262) and 0, 0, unless the `MoveTo` is quiet; the codes stay 1 and 3.
    #[test]
    fn a_not_quiet_move_to_posts_its_end_and_a_quiet_one_does_not() {
        let mut rig = crate::movement::tests::WalkRig::on_the_path();
        let mut update = |quiet: bool, state: MoveTo| {
            let mut m = MoveToAction {
                quiet,
                ..MoveToAction::sub_in(state)
            };
            let mut ai = Some(crate::unit_ai::UnitAi {
                current_action: Some(crate::unit_ai::ActionKind::Move),
                ..crate::unit_ai::UnitAi::default()
            });
            let mut inbox = crate::unit_ai::Inbox::of(&mut ai, object_id(9177), ObjectState::Alive);
            let code = m
                .sub_update(
                    &mut rig.walk(4),
                    PathAsk::default(),
                    GiveUp::default(),
                    None,
                    None,
                    &mut inbox,
                )
                .unwrap();
            let posted: Vec<(crate::unit_ai::NotifyKind, [i32; 3])> = ai.map_or(vec![], |ai| {
                ai.notifies.iter().map(|n| (n.kind, n.p)).collect()
            });
            (code, posted)
        };
        assert_eq!(update(true, MoveTo::Arrived), (1, vec![]));
        assert_eq!(update(true, MoveTo::Failed), (3, vec![]));
        assert_eq!(
            update(false, MoveTo::Arrived),
            (
                1,
                vec![(crate::unit_ai::NotifyKind::ACTION_COMPLETED, [0x262, 0, 0])]
            )
        );
        assert_eq!(
            update(false, MoveTo::Failed),
            (
                3,
                vec![(crate::unit_ai::NotifyKind::ACTION_FAILED, [0x262, 0, 0])]
            )
        );
    }

    /// A sub-action `MoveTo` as its owner adds it (`FUN_140a977e0`, then `0x140aa2100`): a unit
    /// in state 3 or later adds none; within 0.01 of its target it arrived (a NaN gap is far);
    /// farther it starts with speed (a NaN speed too) and fails without; one to a point is far.
    #[test]
    fn an_added_sub_move_to_arrives_starts_or_fails_and_a_dying_unit_adds_none() {
        let m = MoveToAction::sub_to_point(Position::new(1.0, 2.0, 0.0), Distance::new(0.5), None);
        let state = |unit_state, gap, speed| m.added(unit_state, gap, speed).map(|a| a.state);
        assert_eq!(
            state(ObjectState::Alive, Distance::new(0.01), Speed::ZERO),
            Some(MoveTo::Arrived)
        );
        assert_eq!(
            state(ObjectState::Alive, Distance::ZERO, Speed::new(1.0)),
            Some(MoveTo::Arrived)
        );
        assert_eq!(
            state(
                ObjectState::Alive,
                Distance::new(0.010_001),
                Speed::new(1.0)
            ),
            Some(MoveTo::Start)
        );
        assert_eq!(
            state(ObjectState::Alive, Distance::new(0.5), Speed::ZERO),
            Some(MoveTo::Failed)
        );
        assert_eq!(
            state(ObjectState::Alive, Distance::new(f32::NAN), Speed::new(1.0)),
            Some(MoveTo::Start)
        );
        assert_eq!(
            state(ObjectState::Alive, Distance::new(0.5), Speed::new(f32::NAN)),
            Some(MoveTo::Start)
        );
        assert_eq!(
            state(
                ObjectState::Foundation,
                Distance::new(NO_TARGET_GAP),
                Speed::new(1.0)
            ),
            Some(MoveTo::Start)
        );
        assert_eq!(
            state(
                ObjectState::Alive,
                Distance::new(NO_TARGET_GAP),
                Speed::ZERO
            ),
            Some(MoveTo::Failed)
        );
        assert_eq!(
            state(ObjectState::Dying, Distance::ZERO, Speed::new(1.0)),
            None
        );
        assert_eq!(
            state(ObjectState::Removed, Distance::new(0.5), Speed::new(1.0)),
            None
        );
        let added = m.added(ObjectState::Alive, Distance::new(0.5), Speed::new(1.0));
        assert_eq!(
            added.map(|a| (a.point, a.range, a.quiet)),
            Some((m.point, Distance::new(0.5), true))
        );
    }

    /// `+0x61` from the save: 0 follows, 1 asks again, 2 ignores; any other value is the save's
    /// error.
    #[test]
    fn a_saved_target_moved_state_is_one_of_three() {
        assert_eq!(TargetMoved::try_from(0), Ok(TargetMoved::Follows));
        assert_eq!(TargetMoved::try_from(1), Ok(TargetMoved::AsksAgain));
        assert_eq!(TargetMoved::try_from(2), Ok(TargetMoved::Ignores));
        assert!(TargetMoved::try_from(3).is_err());
    }

    /// `0x140aa2a70`'s check of its target in state 4 by `+0x61`: a target within half a tile of
    /// the `MoveTo`'s point stays; off it, a `MoveTo` that asks again asks for a path again, one
    /// that follows moves its path's end to the target while the line to it is clear, else it is
    /// blocked (notify 0x1fc, state 3, the motion step on), and one that ignores its target walks
    /// on without reading it. A target gone and a point not held are errors.
    #[test]
    fn a_target_off_the_move_to_s_point_asks_again_or_moves_the_path_s_end() {
        let open = vec![4; 120 * 120];
        let mut walled = open.clone();
        for y in 85..92 {
            walled[y * 120 + 29] = 1;
        }
        let moved = |classes: &[u8], point: Position, target, mode| {
            let mut world = crate::movement::tests::tiled_world(classes.to_vec());
            let mover = crate::movement::tests::builder_9177();
            let mut mover = Body::new(Position::new(28.0, 89.0, 0.0), None, Some(mover));
            let mut walk = Walk {
                body: &mut mover,
                me: Me {
                    id: object_id(9177),
                    group: None,
                    action_state: ActionState::new(4),
                },
                dt: Seconds::new(0.1),
                world: &mut world,
                plan: None,
            };
            target_left(&mut walk, object_id(target), point, mode)
        };
        let (again, follows) = (TargetMoved::AsksAgain, TargetMoved::Follows);
        let on = Position::new(30.4, 88.0, 0.0);
        let off = Position::new(30.6, 88.0, 0.0);
        assert_eq!(moved(&open, on, 10028, again), Ok(Follow::Stays));
        assert_eq!(moved(&open, on, 10028, follows), Ok(Follow::Stays));
        assert_eq!(moved(&open, off, 10028, again), Ok(Follow::Again));
        assert_eq!(
            moved(&open, off, 10028, follows),
            Ok(Follow::PathEnd(Position::new(30.0, 88.0, 0.0)))
        );
        assert_eq!(moved(&walled, off, 10028, follows), Ok(Follow::Blocked));
        assert!(moved(&open, on, 4242, again).is_err());
        let ignores = TargetMoved::Ignores;
        assert_eq!(moved(&open, off, 10028, ignores), Ok(Follow::Stays));
    }
}
