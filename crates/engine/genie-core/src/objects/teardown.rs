//! The action list's deletions (`FUN_140a98620`, `FUN_140a98e20`) and each action's destructor
//! (its slot 0): what they leave the unit's world to apply.

use crate::GenieError;
use crate::combat::missile::ShooterId;
use crate::garrison::ContainerId;
use crate::gathering::DropSiteId;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::UpdateCode;
use crate::objects::actions::{Action, ActionList};
use crate::objects::layers::MotionMut;
use crate::objects::release::Releaser;
use crate::world::step::Step;

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
    fn release(&mut self, target: Option<ObjectId>) {
        if let Some(target) = target {
            self.releases.push(target);
        }
    }

    /// What the destructors left, applied at once from the update
    /// ([`crate::world::lets::torn_down`]), `by` the unit letting go.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::torn_down`]'s.
    pub(crate) fn let_go(
        self,
        c: &mut Step<'_>,
        me: &mut crate::objects::unit::Unit,
        by: Releaser,
    ) -> Result<(), GenieError> {
        let releases = self.releases.into_iter().map(|t| (t, by)).collect();
        crate::world::lets::torn_down(c, me, (self.builder_leaves, releases))
    }
}

/// The unit's stop (slot 0x5d0) for a torn-down action's `MoveTo`, when it had one (`moves`)
/// and the unit moves at all.
fn stop_if_moving(mut motion: MotionMut<'_>, moves: bool) -> Result<(), GenieError> {
    if moves && let Some(m) = motion.mover_mut() {
        m.stop()?;
    }
    Ok(())
}

impl Action {
    /// The action's destructor (slot 0) on its unit's `body`. A `MoveTo`'s (`FUN_140aa1f30`, an
    /// action's sub-action's too) stops the unit (slot 0x5d0). The base teardown
    /// (`FUN_140a73fc0`) lets the target go (slot 0x180) unless the action is quiet (`+0x60`;
    /// the Hunt's Attack is, so only the Hunt lets its target go), then the second target. The
    /// Build's (`FUN_140aadbf0`) first takes a counted builder (`+0x68`) out of its building's
    /// builders (`FUN_140aaddb0`, `+0x9e8`), as the Repair's (`FUN_140ad8fd0`) a counted
    /// repairer (`+0x6c`) its target's; the Artifact's (`FUN_140aab0d0`) lets its target go and
    /// its `MoveTo`'s stops the unit; the Fly's (`FUN_140a84750`) gives the unit its top speed as
    /// its animation speed (slot 0x208 into `+0x7b8`); the missile's lets its target and the
    /// object it was fired from go. The Make, the wait for a foundation (`0x140ae8780`) and the
    /// Transport hold no target.
    ///
    /// # Errors
    ///
    /// The unit's stop that the motion step does not model.
    pub(crate) fn tear_down(self, mut motion: MotionMut<'_>) -> Result<TornDown, GenieError> {
        let mut torn = TornDown::default();
        match self {
            Self::WaitSolidify(_) | Self::Make(_) => {}
            Self::Artifact(mut a) => {
                let (released, gone) = a.tear_down();
                torn.release(released);
                torn.release(gone.released);
                stop_if_moving(motion, gone.stops)?;
            }
            Self::Fly(f) => {
                motion.set_anim_speed(motion.mover().map_or(Speed::ZERO, |m| m.stats.speed));
                stop_if_moving(motion, f.state.move_to().is_some())?;
            }
            Self::Enter(e) => {
                stop_if_moving(motion, e.state.move_to().is_some())?;
                torn.release(e.target.map(ContainerId::id));
            }
            Self::Transport(t) => stop_if_moving(motion, t.state.move_to().is_some())?,
            Self::Build(b) => {
                if b.added
                    && let Some(target) = b.target
                {
                    torn.builder_leaves.push(target);
                }
                torn.release(b.target);
                torn.release(b.site.map(DropSiteId::id));
                stop_if_moving(motion, b.state.move_to().is_some())?;
            }
            Self::Repair(r) => {
                if r.counted
                    && let Some(target) = r.target
                {
                    torn.builder_leaves.push(target);
                }
                torn.release(r.target);
                stop_if_moving(motion, r.state.move_to().is_some())?;
            }
            Self::Explore(e) => stop_if_moving(motion, e.state.move_to().is_some())?,
            Self::Hunt(mut h) => {
                h.tear_down(motion.mover_mut())?;
                torn.release(h.target);
                torn.release(h.carry_to.map(DropSiteId::id));
            }
            Self::Gather(mut g) => {
                g.tear_down(motion.mover_mut())?;
                torn.release(g.target);
                torn.release(g.carry_to.map(DropSiteId::id));
            }
            Self::Attack(mut a) => {
                a.tear_down(motion.mover_mut())?;
                torn.release(a.target);
                torn.release(a.target2);
            }
            Self::MoveTo(m) => {
                stop_if_moving(motion, true)?;
                torn.release(m.target);
            }
            Self::Missile(m) => {
                torn.release(m.target);
                torn.release(m.fired_from.map(ShooterId::id));
            }
        }
        Ok(torn)
    }
}

/// The head's update code as `RGE_Action_Object::update` (`0x14106ac90`) reads it from the list's
/// update (`FUN_140a992b0`): 1 (done) and 3 (failed) delete the head
/// ([`ActionList::delete_head`]), any other keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// unit's `body`); then, with another action at the head and the unit below state 3
    /// (`state`, `+0x640`), that action's slot 0x28 with 0 ([`NEXT_HEAD`]). Nothing for an empty
    /// list.
    ///
    /// # Errors
    ///
    /// The destructor's, and a second action under the head below state 3.
    pub(crate) fn delete_head(
        &mut self,
        motion: MotionMut<'_>,
        state: ObjectState,
    ) -> Result<TornDown, GenieError> {
        let Some(head) = self.pop_head() else {
            return Ok(TornDown::default());
        };
        let torn = head.tear_down(motion)?;
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
        state: ObjectState,
    ) -> Result<TornDown, GenieError> {
        let mut all = TornDown::default();
        while !self.is_empty() {
            let torn = self.delete_head(motion.reborrow(), state)?;
            all.releases.extend(torn.releases);
            all.builder_leaves.extend(torn.builder_leaves);
        }
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::build::{BuildAction, BuildTask};
    use crate::map::Distance;
    use crate::map::Position;
    use crate::math::fmath::BVector;
    use crate::movement::move_to::MoveToAction;
    use crate::objects::UpdateCode;
    use crate::objects::WorkRate;
    use crate::objects::layers::Motion;
    use crate::objects::masters::TaskId;
    use crate::test_ids::object_id;

    /// A moving object's motion layer, moving (its velocity and animation speed set).
    fn moving() -> Motion {
        let mut m = crate::gathering::hunt::tests::walker();
        m.velocity = Some(crate::movement::Velocity {
            velocity: BVector::new(0.0, 0.0, 0.7),
            acceleration: BVector::new(0.0, 0.0, 1.0),
        });
        let mut motion = Motion::Moving(m);
        motion.lent().set_anim_speed(Speed::new(0.8));
        motion
    }

    /// A Build of foundation 10028 at its edge (gap 0).
    fn build() -> BuildAction {
        let task = BuildTask {
            range: Distance::ZERO,
            work_value: crate::objects::masters::WorkValue::new(1.0),
            work_rate: WorkRate::new(1.0),
            resource: None,
            id: TaskId::of_task(-1),
            moving: None,
        };
        crate::construction::build::tests::started(
            object_id(10028),
            Position::new(30.0, 88.0, 0.0),
            Distance::ZERO,
            task,
        )
    }

    /// `RGE_Action_Object::update` deletes the head on codes 1 and 3 alone.
    #[test]
    fn codes_1_and_3_delete_the_head() {
        let deleting: Vec<u8> = (0..=u8::MAX)
            .filter(|&c| HeadCode::of(UpdateCode::of_data(c)) == HeadCode::Deletes)
            .collect();
        assert_eq!(deleting, [1, 3]);
        assert_eq!(HeadCode::ended(true), HeadCode::Deletes);
        assert_eq!(HeadCode::ended(false), HeadCode::Keeps);
    }

    /// Whether `motion` still moves.
    fn moves(motion: &Motion) -> bool {
        motion
            .mover()
            .is_some_and(|m| m.velocity.is_some_and(|v| v.velocity.y() != 0.0))
    }

    /// A Build's destructor (`FUN_140aadbf0`): a counted builder leaves its building's builders,
    /// the base teardown (`FUN_140a73fc0`) lets the building, then the drop site (the second
    /// target) go, and its `MoveTo` stops the unit; an uncounted one only lets go.
    #[test]
    fn build_destructor_leaves_its_building_and_stops_its_walk() {
        let mut motion = moving();
        let mut b = build();
        (b.added, b.state) = (
            true,
            crate::construction::build::BuildState::Move {
                move_to: Some(MoveToAction::sub_in(
                    crate::movement::move_to::MoveTo::Moving,
                )),
            },
        );
        let torn = Action::Build(b).tear_down(motion.lent()).unwrap();
        assert_eq!(
            torn,
            TornDown {
                releases: vec![object_id(10028)],
                builder_leaves: vec![object_id(10028)],
            }
        );
        assert!(!moves(&motion));
        (b.added, b.state, b.site) = (
            false,
            crate::construction::build::BuildState::Move { move_to: None },
            Some(DropSiteId::new(object_id(10050))),
        );
        let torn = Action::Build(b).tear_down(moving().lent()).unwrap();
        assert_eq!(
            (torn.releases, torn.builder_leaves),
            (vec![object_id(10028), object_id(10050)], vec![])
        );
    }

    /// `FUN_140a78430` reads any action's second target (`+0x34`): a Build's is its drop site.
    #[test]
    fn build_second_target_is_its_drop_site() {
        let mut b = build();
        assert_eq!(Action::Build(b).second_target(), None);
        b.site = Some(DropSiteId::new(object_id(10050)));
        assert_eq!(Action::Build(b).second_target(), Some(object_id(10050)));
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
            motion.mover_mut(),
        );
        let torn = Action::MoveTo(to).tear_down(motion.lent()).unwrap();
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
        let torn = list.delete_all(motion.lent(), ObjectState::Alive).unwrap();
        assert_eq!(torn.releases, [object_id(10028)]);
        assert!(list.is_empty());
        assert_eq!(
            list.delete_head(motion.lent(), ObjectState::Alive),
            Ok(TornDown::default())
        );
        let to = MoveToAction::to_point(
            Position::new(5.0, 5.0, 0.0),
            Distance::ZERO,
            None,
            motion.mover_mut(),
        );
        let two = ActionList::default()
            .with_build(Some(build()))
            .with_move_to(Some(to));
        let err = two
            .clone()
            .delete_head(motion.lent(), ObjectState::Alive)
            .unwrap_err();
        assert_eq!(err.reason(), NEXT_HEAD);
        let mut dying = two;
        assert_eq!(
            dying
                .delete_head(motion.lent(), ObjectState::Dying)
                .map(|t| t.releases),
            Ok(vec![])
        );
        assert_eq!(dying.iter().count(), 1);
    }
}
