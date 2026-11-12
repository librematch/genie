//! The action list's deletions (`FUN_140a98620`, `FUN_140a98e20`), its install (slot 0x808,
//! `FUN_14106e990`) and each action's destructor (its slot 0): what they leave the unit's world
//! to apply.

use crate::combat::missile::ShooterId;
use crate::construction::FoundationId;
use crate::garrison::ContainerId;
use crate::gathering::DropSiteId;
use crate::movement::Speed;
use crate::movement::move_to::MoveToAction;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::UpdateCode;
use crate::objects::actions::{Action, ActionList};
use crate::objects::layers::MotionMut;
use crate::objects::release::Releaser;
use crate::objects::unit::Unit;
use crate::world::lets::torn_down;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

/// What an action's destructor leaves its unit's world: the targets it lets go (slot 0x180),
/// in order, and the buildings a counted builder or repairer leaves (`FUN_140aaddb0`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TornDown {
    /// The objects let go, in the order the destructors let them go.
    pub(crate) releases: Vec<ObjectId>,
    /// The buildings whose builders or repairers count one less.
    pub(crate) builder_leaves: Vec<ObjectId>,
}

impl TornDown {
    /// `target` let go, unless it is none (−1).
    fn release(&mut self, target: impl ObjectKey) {
        if let Some(target) = target.key() {
            self.releases.push(target);
        }
    }

    /// What a later destructor left, `later`, after this one's.
    fn append(&mut self, later: Self) {
        self.releases.extend(later.releases);
        self.builder_leaves.extend(later.builder_leaves);
    }

    /// What the destructors left, applied at once from the update
    /// ([`crate::world::lets::torn_down`]), `by` the unit letting go.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::torn_down`]'s.
    pub(crate) fn let_go(
        self,
        step: &mut Step<'_>,
        me: &mut Unit,
        by: Releaser,
    ) -> GenieResult<()> {
        let releases = self
            .releases
            .into_iter()
            .map(|object_id| (object_id, by))
            .collect();
        torn_down(step, me, (self.builder_leaves, releases))
    }
}

/// What a torn-down action's teardown does to its unit's walk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum TornWalk {
    /// A `MoveTo` went with the action (its own, its sub-action's or the Artifact's Enter's):
    /// its teardown (`0x140aa1f30`) stops the unit (slot 0x5d0).
    Stops,
    /// No `MoveTo` went: the unit's motion stays as it is.
    #[default]
    Untouched,
}

impl From<Option<&MoveToAction>> for TornWalk {
    /// The walk of an action whose state holds `move_to`, if any.
    fn from(move_to: Option<&MoveToAction>) -> Self {
        if move_to.is_some() {
            Self::Stops
        } else {
            Self::Untouched
        }
    }
}

/// The unit's stop (slot 0x5d0) for a torn-down action whose `walk` stops, when the unit moves
/// at all.
fn stop_if_moving(mut motion: MotionMut<'_>, walk: TornWalk) -> GenieResult<()> {
    if walk == TornWalk::Stops
        && let Some(mover) = motion.mover_mut()
    {
        mover.stop()?;
    }
    Ok(())
}

impl Action {
    /// The action's destructor (slot 0) on its unit's motion layer, its top speed `top` (slot
    /// 0x208 as the deletion starts: a teardown changes none of what it reads). A `MoveTo`'s (`FUN_140aa1f30`, an
    /// action's sub-action's too) stops the unit (slot 0x5d0). The base teardown
    /// (`FUN_140a73fc0`) lets the target go (slot 0x180) unless the action is quiet (`+0x60`;
    /// the Hunt's Attack is, so only the Hunt lets its target go), then the second target. The
    /// Build's (`FUN_140aadbf0`) first takes a counted builder (`+0x68`) out of its building's
    /// builders (`FUN_140aaddb0`, `+0x9e8`), as the Repair's (`FUN_140ad8fd0`) a counted
    /// repairer (`+0x6c`) its target's; the Artifact's (`FUN_140aab0d0`) lets its target go and
    /// its `MoveTo`'s stops the unit; the Fly's (`FUN_140a84750`) gives the unit its top speed
    /// `top` as its animation speed (slot 0x208 into `+0x7b8`); the missile's lets its target and the
    /// object it was fired from go. The Make, the wait for a foundation (`0x140ae8780`) and the
    /// Transport hold no target.
    ///
    /// # Errors
    ///
    /// The unit's stop that the motion step does not model.
    pub(crate) fn tear_down(self, mut motion: MotionMut<'_>, top: Speed) -> GenieResult<TornDown> {
        let mut torn = TornDown::default();
        match self {
            Self::WaitSolidify(_) | Self::Make(_) => {}
            Self::Artifact(mut artifact) => {
                let (released, gone) = artifact.tear_down();
                torn.release(released);
                torn.release(gone.released);
                stop_if_moving(motion, gone.walk)?;
            }
            Self::Fly(fly) => {
                // RGE_Action_Bird's destructor: `+0x7b8` takes the bird's slot 0x208
                motion.set_anim_speed(top);
                stop_if_moving(motion, TornWalk::from(fly.state.move_to()))?;
            }
            Self::Enter(enter_action) => {
                stop_if_moving(motion, TornWalk::from(enter_action.state.move_to()))?;
                torn.release(enter_action.target.map(ContainerId::id));
            }
            Self::Transport(transport_action) => {
                stop_if_moving(motion, TornWalk::from(transport_action.state.move_to()))?;
            }
            Self::Build(build_action) => {
                if build_action.added
                    && let Some(target) = build_action.target
                {
                    torn.builder_leaves.push(target.id());
                }
                torn.release(build_action.target.map(FoundationId::id));
                torn.release(build_action.site.map(DropSiteId::id));
                stop_if_moving(motion, TornWalk::from(build_action.state.move_to()))?;
            }
            Self::Repair(repair_action) => {
                if repair_action.counted
                    && let Some(target) = repair_action.target
                {
                    torn.builder_leaves.push(target.id());
                }
                torn.release(repair_action.target);
                stop_if_moving(motion, TornWalk::from(repair_action.state.move_to()))?;
            }
            Self::Explore(explore_action) => {
                stop_if_moving(motion, TornWalk::from(explore_action.state.move_to()))?;
            }
            Self::Hunt(mut hunt_action) => {
                hunt_action.tear_down(motion.mover_mut())?;
                torn.release(hunt_action.target);
                torn.release(hunt_action.carry_to.map(DropSiteId::id));
            }
            Self::Gather(mut gather_action) => {
                gather_action.tear_down(motion.mover_mut())?;
                torn.release(gather_action.target);
                torn.release(gather_action.carry_to.map(DropSiteId::id));
            }
            Self::Attack(mut attack) => {
                attack.tear_down(motion.mover_mut())?;
                torn.release(attack.target);
                torn.release(attack.target2);
            }
            Self::MoveTo(move_to_action) => {
                stop_if_moving(motion, TornWalk::Stops)?;
                torn.release(move_to_action.target);
            }
            Self::Missile(missile_action) => {
                torn.release(missile_action.target);
                torn.release(missile_action.fired_from.map(ShooterId::id));
            }
        }
        Ok(torn)
    }
}

/// The head's update code as `RGE_Action_Object::update` (`0x14106ac90`) reads it from the list's
/// update (`FUN_140a992b0`): 1 (done) and 3 (failed) delete the head
/// ([`ActionList::delete_head`]), any other keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(crate) enum HeadCode {
    /// The head stays for the next update.
    Keeps,
    /// The head's update returned 1 or 3: the list deletes it.
    Deletes,
}

impl HeadCode {
    /// The head's update `code` ([`UpdateCode::deletes`]).
    pub(crate) const fn of(code: UpdateCode) -> Self {
        if code.deletes() {
            Self::Deletes
        } else {
            Self::Keeps
        }
    }

    /// [`HeadCode::Deletes`] when the head's update `ended` (done or failed).
    pub(crate) const fn ended(ended: bool) -> Self {
        if ended { Self::Deletes } else { Self::Keeps }
    }
}

/// A deletion that would leave a second action at the head: its slot 0x28 with 0
/// (`first_in_stack`, each kind's restart into a fixed state) is not ported, and no list of the
/// slice holds two (every add starts from an empty list, the import takes the head).
const NEXT_HEAD: &str = "a deleted head with a second action under it (slot 0x28)";

impl ActionList {
    /// `FUN_140a98e20`: the head goes and its destructor runs ([`Action::tear_down`] on the
    /// unit's motion layer and top speed); then, with another action at the head and the unit below state 3
    /// (`state`, `+0x640`), that action's slot 0x28 with 0 ([`NEXT_HEAD`]). Nothing for an empty
    /// list.
    ///
    /// # Errors
    ///
    /// The destructor's, and a second action under the head below state 3.
    pub(crate) fn delete_head(
        &mut self,
        motion: MotionMut<'_>,
        top: Speed,
        state: ObjectState,
    ) -> GenieResult<TornDown> {
        let Some(head) = self.pop_head() else {
            return Ok(TornDown::default());
        };
        let torn = head.tear_down(motion, top)?;
        if !self.is_empty() && state < ObjectState::Dying {
            return Err(GenieError::unreachable(NEXT_HEAD));
        }
        Ok(torn)
    }

    /// `FUN_140a98620`: [`ActionList::delete_head`] until the list is empty, what each
    /// destructor left in order.
    ///
    /// # Errors
    ///
    /// The first deletion's that fails.
    pub(crate) fn delete_all(
        &mut self,
        mut motion: MotionMut<'_>,
        top: Speed,
        state: ObjectState,
    ) -> GenieResult<TornDown> {
        let mut all = TornDown::default();
        while !self.is_empty() {
            all.append(self.delete_head(motion.reborrow(), top, state)?);
        }
        Ok(all)
    }

    /// `set_only_action` (slot 0x808, `FUN_14106e990`; no class overrides it) on the list of a
    /// unit in `state`, on its motion layer and top speed (`motion`, `top`: slot 0x208 as the
    /// deletion starts). The new `action`'s slot 0x70 with the old head (`FUN_140a986f0`) comes
    /// first and does nothing for every kind the list holds: the base's (`FUN_140a78050`); only
    /// `TRIBE_Action_Convert`'s (`0x140ab7860`) copies an old Convert's `+0x70` and `+0x88`, and
    /// genie holds no Convert. Then the list's deletion ([`ActionList::delete_all`]), then its
    /// add (`FUN_140a977e0`): below state 3 the action comes back, and the caller's start (its
    /// slot 0x28 with 1) holds it as the head of the emptied list; in state 3 or later the list
    /// deletes it unstarted, its destructor (slot 0 with 1, [`Action::tear_down`]) after the old
    /// actions'. None (a null action) only deletes the list. With what the destructors left, in
    /// their order. The debug log around it (`DAT_143f09029`) reads the world time and changes
    /// nothing.
    ///
    /// # Errors
    ///
    /// The deletion's and the new action's destructor's.
    pub(crate) fn set_only<A: Into<Action>>(
        &mut self,
        action: Option<A>,
        mut motion: MotionMut<'_>,
        top: Speed,
        state: ObjectState,
    ) -> GenieResult<(Option<A>, TornDown)> {
        let mut torn = self.delete_all(motion.reborrow(), top, state)?;
        let Some(action) = action else {
            return Ok((None, torn));
        };
        if state < ObjectState::Dying {
            return Ok((Some(action), torn));
        }
        torn.append(action.into().tear_down(motion, top)?);
        Ok((None, torn))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::construction::build::BuildState;
    #[cfg(test)]
    use crate::construction::build::tests::started;
    use crate::construction::build::{BuildAction, BuildTask};
    #[cfg(test)]
    use crate::gathering::hunt::tests::walker;
    use crate::map::Distance;
    use crate::map::Position;
    use crate::math::fmath::BVector;
    #[cfg(test)]
    use crate::movement::Velocity;
    #[cfg(test)]
    use crate::movement::move_to::MoveTo;
    use crate::movement::move_to::MoveToAction;
    use crate::objects::UpdateCode;
    use crate::objects::WorkRate;
    use crate::objects::layers::Motion;
    use crate::objects::masters::ActionKind;
    use crate::objects::masters::TaskId;
    use crate::objects::masters::TaskSprites;
    #[cfg(test)]
    use crate::objects::masters::WorkValue;
    use crate::test_ids::object_id;
    use crate::test_ids::task_key;

    /// A moving object's motion layer, moving (its velocity and animation speed set).
    fn moving() -> Motion {
        let mut mover = walker();
        mover.velocity = Some(Velocity {
            velocity: BVector::new(0.0, 0.0, 0.7),
            acceleration: BVector::new(0.0, 0.0, 1.0),
        });
        let mut motion = Motion::Moving(mover);
        motion.lent().set_anim_speed(Speed::new(0.8));
        motion
    }

    /// A Build of foundation 10028 at its edge (gap 0).
    fn build() -> BuildAction {
        build_on(object_id(10028))
    }

    /// A Build of foundation `foundation` at its edge (gap 0).
    fn build_on(foundation: ObjectId) -> BuildAction {
        let task = BuildTask {
            range: Distance::ZERO,
            work_value: WorkValue::new(1.0),
            work_rate: WorkRate::new(1.0),
            resource: None,
            id: TaskId::of_task(-1),
            key: task_key(ActionKind::BUILD),
            sprites: TaskSprites::NONE,
        };
        started(
            foundation,
            Position::new(30.0, 88.0, 0.0),
            Distance::ZERO,
            task,
        )
    }

    /// `RGE_Action_Object::update` deletes the head on codes 1 and 3 alone.
    #[test]
    fn codes_1_and_3_delete_the_head() {
        let deleting: Vec<u8> = (0..=u8::MAX)
            .filter(|&code| HeadCode::of(UpdateCode::of_data(code)) == HeadCode::Deletes)
            .collect();
        assert_eq!(deleting, [1, 3]);
        assert_eq!(HeadCode::ended(true), HeadCode::Deletes);
        assert_eq!(HeadCode::ended(false), HeadCode::Keeps);
    }

    /// Whether `motion` still moves.
    fn moves(motion: &Motion) -> bool {
        motion.mover().is_some_and(|mover| {
            mover
                .velocity
                .is_some_and(|velocity| velocity.velocity.y() != 0.0)
        })
    }

    /// A torn-down action stops its unit only when a `MoveTo` went with it, one its state held;
    /// the stop leaves a unit without one moving.
    #[test]
    fn only_move_to_that_went_stops_the_unit() {
        let walking = MoveToAction::sub_in(MoveTo::Moving);
        assert_eq!(TornWalk::from(Some(&walking)), TornWalk::Stops);
        assert_eq!(TornWalk::from(None), TornWalk::Untouched);
        let mut motion = moving();
        stop_if_moving(motion.lent(), TornWalk::Untouched).unwrap();
        assert!(moves(&motion));
        stop_if_moving(motion.lent(), TornWalk::Stops).unwrap();
        assert!(!moves(&motion));
    }

    /// A Build's destructor (`FUN_140aadbf0`): a counted builder leaves its building's builders,
    /// the base teardown (`FUN_140a73fc0`) lets the building, then the drop site (the second
    /// target) go, and its `MoveTo` stops the unit; an uncounted one only lets go.
    #[test]
    fn build_destructor_leaves_its_building_and_stops_its_walk() {
        let mut motion = moving();
        let mut build_action = build();
        (build_action.added, build_action.state) = (
            true,
            BuildState::Move {
                move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
            },
        );
        let torn = Action::Build(build_action)
            .tear_down(motion.lent(), Speed::new(1.0))
            .unwrap();
        assert_eq!(
            torn,
            TornDown {
                releases: vec![object_id(10028)],
                builder_leaves: vec![object_id(10028)],
            }
        );
        assert!(!moves(&motion));
        (build_action.added, build_action.state, build_action.site) = (
            false,
            BuildState::Move { move_to: None },
            Some(DropSiteId::new(object_id(10050))),
        );
        let torn = Action::Build(build_action)
            .tear_down(moving().lent(), Speed::new(1.0))
            .unwrap();
        assert_eq!(
            (torn.releases, torn.builder_leaves),
            (vec![object_id(10028), object_id(10050)], vec![])
        );
    }

    /// `FUN_140a78430` reads any action's second target (`+0x34`): a Build's is its drop site.
    #[test]
    fn build_second_target_is_its_drop_site() {
        let mut build_action = build();
        assert_eq!(Action::Build(build_action).second_target(), None);
        build_action.site = Some(DropSiteId::new(object_id(10050)));
        assert_eq!(
            Action::Build(build_action).second_target(),
            Some(object_id(10050))
        );
    }

    /// A `MoveTo`'s destructor (`FUN_140aa1f30`) stops the unit and lets its target go; one
    /// without a target lets nothing go.
    #[test]
    fn move_to_destructor_stops_the_unit() {
        let mut motion = moving();
        let to = MoveToAction::to_point(
            Position::new(5.0, 5.0, 0.0),
            Distance::ZERO,
            None,
            Speed::new(1.0),
        );
        let torn = Action::MoveTo(to)
            .tear_down(motion.lent(), Speed::new(1.0))
            .unwrap();
        assert_eq!(torn, TornDown::default());
        assert!(!moves(&motion));
    }

    /// `FUN_140a98e20` and `FUN_140a98620` on a list of one action: the head goes with its
    /// destructor, the list is empty after; an empty list deletes nothing. A second action under
    /// the head below state 3 (its slot 0x28) is not ported; at state 3 or more it stays.
    #[test]
    fn the_list_deletions_tear_down_the_head_and_stop_at_second_action() {
        let mut motion = moving();
        let mut list = ActionList::default().with_build(Some(build()));
        let torn = list
            .delete_all(motion.lent(), Speed::new(1.0), ObjectState::Alive)
            .unwrap();
        assert_eq!(torn.releases, [object_id(10028)]);
        assert!(list.is_empty());
        assert_eq!(
            list.delete_head(motion.lent(), Speed::new(1.0), ObjectState::Alive),
            Ok(TornDown::default())
        );
        let to = MoveToAction::to_point(
            Position::new(5.0, 5.0, 0.0),
            Distance::ZERO,
            None,
            Speed::new(1.0),
        );
        let two = ActionList::default()
            .with_build(Some(build()))
            .with_move_to(Some(to));
        let err = two
            .clone()
            .delete_head(motion.lent(), Speed::new(1.0), ObjectState::Alive)
            .unwrap_err();
        assert_eq!(err.reason(), NEXT_HEAD);
        let mut dying = two;
        assert_eq!(
            dying
                .delete_head(motion.lent(), Speed::new(1.0), ObjectState::Dying)
                .map(|torn_down| torn_down.releases),
            Ok(vec![])
        );
        assert_eq!(dying.iter().count(), 1);
    }

    /// A `MoveTo` to (5, 5) without a target.
    fn walk_to_point() -> MoveToAction {
        MoveToAction::to_point(
            Position::new(5.0, 5.0, 0.0),
            Distance::ZERO,
            None,
            Speed::new(1.0),
        )
    }

    /// Slot 0x808 with a null action (`FUN_14106e990`) only deletes the list
    /// (`FUN_140a98620`): the Build lets its foundation go, and nothing comes back to start.
    #[test]
    fn set_only_without_action_only_deletes_the_list() {
        let mut motion = moving();
        let mut list = ActionList::default().with_build(Some(build()));
        let (action, torn) = list
            .set_only(
                None::<Action>,
                motion.lent(),
                Speed::new(1.0),
                ObjectState::Alive,
            )
            .unwrap();
        assert_eq!(action, None);
        assert_eq!(torn.releases, [object_id(10028)]);
        assert!(list.is_empty());
    }

    /// Slot 0x808 on a unit below state 3: the old Build goes first (its foundation let go), and
    /// the new Build comes back untouched for its start (`FUN_140a977e0`'s slot 0x28 with 1),
    /// the list empty until the start holds it; its foundation stays taken.
    #[test]
    fn set_only_below_state_3_hands_back_the_new_action_after_the_deletion() {
        for state in [ObjectState::Foundation, ObjectState::Alive] {
            let mut motion = moving();
            let mut list = ActionList::default().with_build(Some(build()));
            let (action, torn) = list
                .set_only(
                    Some(build_on(object_id(10030))),
                    motion.lent(),
                    Speed::new(1.0),
                    state,
                )
                .unwrap();
            assert_eq!(action, Some(build_on(object_id(10030))), "{state:?}");
            assert_eq!(torn.releases, [object_id(10028)], "{state:?}");
            assert!(list.is_empty(), "{state:?}");
        }
    }

    /// Slot 0x808 on a unit in state 3 or later: the list deletes the new action unstarted
    /// (`FUN_140a977e0`: its destructor, slot 0 with 1), after the old actions; its foundation is
    /// let go after the old one's, and nothing comes back to start.
    #[test]
    fn set_only_in_state_3_or_later_deletes_the_new_action_after_the_old_ones() {
        for state in [ObjectState::Dying, ObjectState::Dead, ObjectState::Gone] {
            let mut motion = moving();
            let mut list = ActionList::default().with_build(Some(build()));
            let (action, torn) = list
                .set_only(
                    Some(build_on(object_id(10030))),
                    motion.lent(),
                    Speed::new(1.0),
                    state,
                )
                .unwrap();
            assert_eq!(action, None, "{state:?}");
            assert_eq!(
                torn.releases,
                [object_id(10028), object_id(10030)],
                "{state:?}"
            );
            assert!(list.is_empty(), "{state:?}");
        }
    }

    /// The new action's destructor runs on the unit's motion: a new `MoveTo` that a unit in state
    /// 3 cannot take stops it (`FUN_140aa1f30`, slot 0x5d0), where a unit below state 3 keeps
    /// moving with an empty list deleted.
    #[test]
    fn set_only_deleting_new_move_to_stops_the_unit() {
        let mut motion = moving();
        let (action, _) = ActionList::default()
            .set_only(
                Some(walk_to_point()),
                motion.lent(),
                Speed::new(1.0),
                ObjectState::Alive,
            )
            .unwrap();
        assert_eq!(action, Some(walk_to_point()));
        assert!(moves(&motion));
        let (action, torn) = ActionList::default()
            .set_only(
                Some(walk_to_point()),
                motion.lent(),
                Speed::new(1.0),
                ObjectState::Dying,
            )
            .unwrap();
        assert_eq!((action, torn), (None, TornDown::default()));
        assert!(!moves(&motion));
    }
}
