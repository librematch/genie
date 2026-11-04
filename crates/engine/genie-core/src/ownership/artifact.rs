//! The Artifact action (type 107): the default task of livestock, which keeps checking whether a
//! unit of another player is close enough to take the animal over (the search is the caller's,
//! [`crate::map::spatial::Spatial::artifact_owner`]).
//!
//! 158041: `TRIBE_Action_Artifact`, vtable `0x142ed6520`; update `0x140aad250` (slot 6), `set_state`
//! `0x140aace30` (slot 16). The .dat gives Goat, Goose, Cow and Sheep the default task 0, a task with
//! action type 107. An idle animal holds this action as its top action, so checkpoint word 9 counts
//! `107 + state` for it.

use crate::GenieResult;
use crate::garrison::ContainerId;
use crate::map::Distance;
use crate::map::Position;
use crate::movement::Me;
use crate::movement::move_to::TargetMoved;
use crate::movement::move_to::{ListUpdate, SubCode, update_sub_actions};
use crate::movement::move_to::{MoveTo, MoveToAction, MoveToReads, WalkTo};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::Voice;
use crate::objects::actions::Action;
use crate::objects::masters::ActionKind;
use crate::objects::release::Releaser;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::ownership::CaptureKind;
use crate::ownership::capture::Captured;
use crate::ownership::capture::OwnerChange;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::step::Step;
use genie_diff::Diff;

/// What the Artifact's slot 0x68 (the base's `FUN_140a7a230`) did to its targets, for the
/// caller: the action is not quiet (`+0x60` 0, its constructors' `FUN_140a73e90` with 1), so the
/// old target, when the world holds it, lets the unit go (its slot 0x180, `World::release`) and
/// a new one in a state below 7 takes it (its slot 0x178, `World::take`), in that order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Targets {
    /// The old target it let go.
    pub(crate) released: Option<ObjectId>,
    /// The new target it took.
    pub(crate) taken: Option<ObjectId>,
}

/// What the Artifact's update reads of its unit ([`Artifact::update`]).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ArtifactReads {
    /// The animal may change hands: it is not of the combat type with `+0x924` 0
    /// (`FUN_14107cc90`).
    pub(crate) searches: bool,
    /// The unit as the targets it lets go count it ([`Unit::releaser`]).
    pub(crate) by: Releaser,
}

/// What the Artifact's state step asks of its unit ([`Artifact::step`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Handover {
    /// The check found player `new` to take the animal over (the caller's conversion,
    /// `FUN_140aab570`), its sub-actions gone ([`Artifact::drop_sub_actions`]).
    Taken {
        /// The player that takes it.
        new: PlayerId,
        /// The sub-actions that went.
        gone: SubActionsGone,
    },
    /// State 100: the unit's stop (slot 0x148), then its ownership change to Gaia (slot 0x70),
    /// the caller's; the action checks again (`set_state(6)`).
    ToGaia,
}

/// What the deletion of the Artifact's sub-actions (`FUN_140a98620`) leaves to the caller
/// ([`Artifact::drop_sub_actions`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SubActionsGone {
    /// A walking `MoveTo` went, the Artifact's own or the Enter's: its teardown (`0x140aa1f30`)
    /// stops the unit (slot 0x5d0).
    pub(crate) stops: bool,
    /// The building or transport the Enter's teardown let go (slot 0x180).
    pub(crate) released: Option<ObjectId>,
}

/// One animal's Artifact action, as the checkpoint sees it.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct Artifact {
    /// 6 checks for a new owner; 10 waits for the timer.
    pub state: ArtifactState,
    /// `+0x44`, in seconds.
    pub timer: Seconds,
    /// Gaia waits half as long between checks.
    pub gaia: bool,
    /// Its sub-action, one at most: a move's `MoveTo` or an enter command's Enter.
    pub sub: Option<ArtifactSub>,
    /// The action's point (`+0x38`..`+0x40`): where its `MoveTo` walks, or its target's place.
    pub goal: Option<Position>,
    /// The target of a move to an object (`+0x20`, its id at `+0x30`).
    pub target: Option<ObjectId>,
    /// Its task's moving sprite (`+0x40`), which its `MoveTo`s take; None for none.
    pub moving: Option<SpriteId>,
}

/// The action type the checkpoint adds for an Artifact action.
pub const ARTIFACT_KIND: ActionKind = ActionKind::named(107);

use crate::garrison::EnterAction;
use crate::objects::layers::Action as LayersAction;
use crate::objects::masters::MasterKind;
pub use crate::ownership::artifact_state::ArtifactState;
pub use crate::ownership::artifact_sub::ArtifactSub;
use crate::unit_ai::NotifyKind;
use crate::world::Tick;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::layer::generated::ArtifactTuning;
use crate::world::layer::generated::MoveToTuning;
use crate::world::lets::release;
use std::mem::take;

impl Artifact {
    /// A new action of an object of player `owner` and a task whose moving sprite is `moving`,
    /// about to check for a new owner.
    #[must_use]
    pub fn new(owner: PlayerId, moving: Option<SpriteId>) -> Self {
        Self {
            state: ArtifactState::Check,
            timer: Seconds::ZERO,
            gaia: owner == PlayerId::GAIA,
            sub: None,
            goal: None,
            target: None,
            moving,
        }
    }

    /// Its `MoveTo`, none without a walk.
    #[must_use]
    pub const fn move_to(&self) -> Option<&MoveToAction> {
        match &self.sub {
            Some(ArtifactSub::Walk { move_to }) => Some(move_to),
            _ => None,
        }
    }

    /// Its Enter, none without one.
    #[must_use]
    pub const fn enter(&self) -> Option<&EnterAction> {
        match &self.sub {
            Some(ArtifactSub::Enter { enter }) => Some(enter),
            _ => None,
        }
    }

    /// Its `MoveTo` taken out, none without a walk (an Enter stays).
    pub const fn take_move_to(&mut self) -> Option<MoveToAction> {
        match self.sub {
            Some(ArtifactSub::Walk { move_to }) => {
                self.sub = None;
                Some(move_to)
            }
            _ => None,
        }
    }

    /// Its Enter taken out, none without one (a walk stays).
    pub const fn take_enter(&mut self) -> Option<EnterAction> {
        match self.sub {
            Some(ArtifactSub::Enter { enter }) => {
                self.sub = None;
                Some(enter)
            }
            _ => None,
        }
    }

    /// Slot 0x68 with none: the target goes; the one let go, if any.
    pub fn drop_target(&mut self) -> Option<ObjectId> {
        self.set_target(None).released
    }

    /// `FUN_140a98620` on the sub-actions (one at most): a `MoveTo` goes with its goal, whose
    /// teardown stops a walking unit; an Enter goes with its teardown (`FUN_140a85bf0` ->
    /// `FUN_140a73fc0`): its building or transport is let go (slot 0x180), and its own `MoveTo`,
    /// if it walks, goes too. The stop and the release are the caller's.
    pub fn drop_sub_actions(&mut self) -> SubActionsGone {
        match self.sub.take() {
            None => SubActionsGone::default(),
            Some(ArtifactSub::Walk { .. }) => SubActionsGone {
                stops: true,
                released: None,
            },
            Some(ArtifactSub::Enter { enter }) => SubActionsGone {
                stops: enter.state.move_to().is_some(),
                released: enter.target.map(ContainerId::id),
            },
        }
    }

    /// The enter command's Enter as the sub-action (`FUN_1410847e0` with an Artifact on top:
    /// `FUN_140a98e20` pops the sub-action it held, [`Artifact::drop_sub_actions`], then
    /// `FUN_140a977e0` takes the Enter). What went, the caller's.
    pub fn hold_enter(&mut self, enter: EnterAction) -> SubActionsGone {
        let gone = self.drop_sub_actions();
        self.sub = Some(ArtifactSub::Enter { enter });
        gone
    }

    /// Slot 0x190 and slot 0x288 on the animal with this as its top action (`FUN_14106d090`,
    /// `FUN_141082dc0`): no target (`+0x30` −1) and no point (`+0x38` −1). A herder waits for this
    /// before it slaughters.
    #[must_use]
    pub const fn idle(&self) -> bool {
        self.target.is_none() && self.goal.is_none()
    }

    /// Slot 0x68, the base's (`FUN_140a7a230`, `+0x60` 0): a target other than `new` goes (let
    /// go), then `new` comes (taken); the caller passes none for a dead one (a state of 7 or
    /// more), which leaves none.
    fn set_target(&mut self, new: Option<ObjectId>) -> Targets {
        let mut targets = Targets::default();
        if self.target == new {
            return targets;
        }
        targets.released = self.target;
        match new {
            Some(new) => {
                self.target = Some(new);
                targets.taken = Some(new);
            }
            None => self.target = None,
        }
        targets
    }

    /// Slot 8 (`FUN_140aaca30`) on a move to the object `to` (its place, the unit's gap to its
    /// footprint and its state, which is below 7 as the commands give no other): the old target
    /// goes, then the new one comes ([`Artifact::set_target`] with none, then with it), then
    /// `set_state(5)` (`0x140aace30`): its sub-actions go ([`Artifact::drop_sub_actions`]), the
    /// point takes the target's place, and a quiet `MoveTo` (`FUN_140aa0df0` with `+0x60` 1) to
    /// the target starts ([`MoveToReads::to_object`]), within 1 for livestock going to a building
    /// (`to_building`), else 0 (the unit's slot 0x268, 0); one of livestock to a town centre
    /// (master 109, `to_town_centre`) walks to the point 0.2 below the centre's on both axes
    /// instead, without the target ([`WalkTo::unheld`]). The `MoveTo` takes the task's moving
    /// sprite, else `FUN_140aa0df0`'s default. Then the action checks again (`set_state(6)`).
    /// Returns the targets let go and taken, then the sub-actions that went.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn take_object(
        &mut self,
        to: WalkTo,
        (to_building, to_town_centre): (bool, bool),
        reads: &MoveToReads<'_>,
        artifact_tuning: ArtifactTuning,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<(Targets, SubActionsGone)> {
        let range = if to_building { 1.0 } else { 0.0 };
        // its point (`+0x38`): the target's place, or the point below a town centre
        let held = if to_town_centre {
            WalkTo::unheld(Position::new(
                to.at.x() - artifact_tuning.town_centre_offset,
                to.at.y() - artifact_tuning.town_centre_offset,
                to.at.z(),
            ))
        } else {
            to
        };
        let walk = reads.to_object(
            held,
            Distance::new(range),
            self.moving,
            TargetMoved::Follows,
            move_to_tuning,
        )?;
        let alive = to
            .state
            .is_none_or(|object_state| object_state < ObjectState::Removed);
        let released = self.set_target(None).released;
        let taken = self.set_target(to.id.filter(|_| alive)).taken;
        let gone = self.drop_sub_actions();
        self.goal = Some(to.at);
        self.sub = walk.map(|move_to| ArtifactSub::Walk { move_to });
        self.state = ArtifactState::Check;
        Ok((Targets { released, taken }, gone))
    }

    /// Slot 8 (`FUN_140aaca30`) on a move to a point, which the action takes: the point, then
    /// `set_state(4)` (`0x140aace30`): its sub-actions go ([`Artifact::drop_sub_actions`]), a
    /// `MoveTo` to the point within 0 (`FUN_140aa0d30` with its task's sprite, else its default;
    /// not quiet) is added and starts ([`MoveToReads::to_point`]: state 3 for an animal), and
    /// the action checks again (`set_state` 6). Returns the sub-actions that went.
    ///
    /// # Errors
    ///
    /// A sprite the table lacks.
    pub fn take_point(
        &mut self,
        point: Position,
        reads: &MoveToReads<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> GenieResult<SubActionsGone> {
        let walk = reads.to_point(point, Distance::ZERO, self.moving, move_to_tuning)?;
        self.goal = Some(point);
        let gone = self.drop_sub_actions();
        self.sub = walk.map(|move_to_action| ArtifactSub::Walk {
            move_to: MoveToAction {
                voice: Voice::Loud,
                ..move_to_action
            },
        });
        self.state = ArtifactState::Check;
        Ok(gone)
    }

    /// Slot 0x38 (`FUN_140aad1a0`), as the unit's stop (`FUN_14106fa60`) calls it (also before a
    /// new owner takes the animal): with a sub-action (`FUN_140a98730`) its sub-actions go
    /// ([`Artifact::drop_sub_actions`]), with the point (−1) and the target (slot 0x68 with none:
    /// let go); without one both stay. Then it checks again (`set_state` 6, which sets nothing
    /// else). Returns the sub-actions that went, then the target let go (the caller's release,
    /// after the Enter's); the unit's standing sprite (slot 0x68 with the master's `+0x470`) is
    /// the caller's too.
    pub fn stop(&mut self) -> (SubActionsGone, Option<ObjectId>) {
        let mut gone = SubActionsGone::default();
        let mut released = None;
        if self.sub.is_some() {
            gone = self.drop_sub_actions();
            self.goal = None;
            released = self.set_target(None).released;
        }
        self.state = ArtifactState::Check;
        (gone, released)
    }

    /// The action's teardown (its destructor `FUN_140aab0d0` -> `FUN_140a73fc0`, as the action
    /// list's clearing or a dead object's runs it): the target goes (let go, `+0x60` 0); it holds
    /// no second target (`+0x28`); its sub-actions go ([`Artifact::drop_sub_actions`]). Returns
    /// the target let go, then the sub-actions that went, the caller's.
    pub fn tear_down(&mut self) -> (Option<ObjectId>, SubActionsGone) {
        let released = self.target.take();
        (released, self.drop_sub_actions())
    }

    /// One update (`0x140aad250`) of `unit`'s Artifact on the step, the action copied out of
    /// its list. Its `MoveTo` goes first ([`Artifact::walk`]). Then the check (state 6): an
    /// animal of the combat type whose `+0x924` is 0 (`FUN_14107cc90`, `searches` false) never
    /// changes hands; any other asks for its new owner (`FUN_140aabc40`,
    /// [`crate::map::spatial::Spatial::artifact_owner`]) and returns the player that takes it,
    /// after its sub-actions went (`FUN_140a98620`: an Enter's teardown lets its building go, a
    /// walking `MoveTo`'s stops the unit, at once), for the caller's conversion
    /// (`FUN_140aab570`). With no new owner it waits (`set_state` 10). In the other states the
    /// step ([`Artifact::step`]).
    ///
    /// # Errors
    ///
    /// What the walk, the owner search and the releases give.
    pub(crate) fn update(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        artifact_reads: &ArtifactReads,
        roles: &Roles,
    ) -> GenieResult<Option<Handover>> {
        self.walk(step, unit, me, artifact_reads)?;
        let owner = if self.state == ArtifactState::Check && artifact_reads.searches {
            step.world
                .spatial
                .artifact_owner(unit.id, &step.world.players, roles)?
        } else {
            None
        };
        let new = self.step(step.dt, owner)?;
        if let Some(Handover::Taken { gone, .. }) = new {
            let_go_gone(step, unit, gone, artifact_reads.by)?;
        }
        Ok(new)
    }

    /// The update's first part on the step: its `MoveTo` sub-action (`FUN_140a992b0`,
    /// [`MoveToAction::sub_update`]), whose start shows its sprite before the motion step; after
    /// a start or a move the units in its way go to the unit AI at once (notify 0x215). A
    /// `MoveTo` whose motion step gave up fails (`set_state` 13: an animal never waits as a
    /// villager may); the failed one's update returns 3. Once it ended (codes 1 to 5) the
    /// Artifact drops it (`FUN_140a98620`: its teardown stops the unit, slot 0x5d0), its point
    /// (−1) and its target (slot 0x68 with none: let go), and the unit AI hears it, with the
    /// move action value, unless it was quiet (to an object): its completion (0x1fa) from its
    /// update in state 1, or its failure (0x1f9) from state 13; each at once.
    ///
    /// # Errors
    ///
    /// What the motion step does not model, and the release's and the notify's.
    pub(crate) fn walk(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        artifact_reads: &ArtifactReads,
    ) -> GenieResult<()> {
        let Some(state) = self.move_to().map(|move_to_action| move_to_action.state) else {
            return Ok(());
        };
        let notify = match state {
            MoveTo::Arrived => Some(NotifyKind::ACTION_COMPLETED),
            MoveTo::Failed => Some(NotifyKind::ACTION_FAILED),
            state => {
                // a motion step that gives up in the start's update fails the MoveTo, as in
                // its moving state
                let mut walk = self.take_move_to();
                let code = update_sub_actions(&mut walk, step, unit, me, ListUpdate::default())?;
                self.sub = walk.map(|move_to| ArtifactSub::Walk { move_to });
                if state != MoveTo::Waiting {
                    unit.post_in_the_way(
                        step.world.parts.motion.mover_mut(unit.id),
                        &mut step.world.parts.unit_ai,
                    )?;
                }
                // its target let go (5; the game's 4 too, which only a loud MoveTo gives): it
                // ends now, as at 1 and 3, its notify posted by the check
                if code != SubCode::TargetGone {
                    return Ok(());
                }
                None
            }
        };
        let (voice, released) = self.end_walk();
        if let Some(mover) = step.world.parts.mover_mut(unit.id) {
            mover.stop()?;
        }
        if let Some(object_id) = released {
            release(step, unit, object_id, artifact_reads.by)?;
        }
        if let Some(notify) = notify.filter(|_| voice.is_loud()) {
            unit.post_own(
                &mut step.world.parts.unit_ai,
                notify,
                [AiAction::Move.value(), 0, 0],
            )?;
        }
        Ok(())
    }

    /// The `MoveTo` ended (codes 1 to 5): it goes with the point and the target (slot 0x68 with
    /// none); its voice (loud without one), and the target let go.
    fn end_walk(&mut self) -> (Voice, Option<ObjectId>) {
        let voice = self
            .take_move_to()
            .map_or(Voice::Loud, |move_to_action| move_to_action.voice);
        self.goal = None;
        let released = self.set_target(None).released;
        (voice, released)
    }

    /// The update's second part after `dt_ms`: the state's step ([`Handover`]): the check's new
    /// `owner` (the caller's search, none without one), with what went of its sub-actions (the
    /// caller's), or state 100's handover to Gaia (the action checks again; the stop and the
    /// change are the caller's).
    pub fn step(
        &mut self,
        dt_ms: Millis,
        owner: Option<PlayerId>,
    ) -> GenieResult<Option<Handover>> {
        match self.state {
            ArtifactState::Check => {
                if let Some(new) = owner {
                    // its sub-actions go (FUN_140a98620) before the caller's conversion: the
                    // point and the target stay, so the conversion's stop ([`Artifact::stop`])
                    // finds no sub-action
                    let gone = self.drop_sub_actions();
                    return Ok(Some(Handover::Taken { new, gone }));
                }
                self.state = ArtifactState::Wait;
                self.timer = if self.gaia {
                    Seconds::new(0.5)
                } else {
                    Seconds::new(1.0)
                };
            }
            ArtifactState::Wait => {
                // world+0x164 is `dt / 1000.0f`; the subtraction is f32
                self.timer -= dt_ms.seconds();
                if self.timer <= Seconds::ZERO {
                    self.state = ArtifactState::Check;
                }
            }
            ArtifactState::ToGaia => {
                self.state = ArtifactState::Check;
                return Ok(Some(Handover::ToGaia));
            }
        }
        Ok(None)
    }
}

/// What went of the Artifact's sub-actions in its update (`gone`), at once: the building or
/// transport an Enter's teardown let go is released by `by` (slot 0x180), and a walking
/// `MoveTo`'s teardown stops `unit` (slot 0x5d0).
///
/// # Errors
///
/// The release's and the stop's.
fn let_go_gone(
    step: &mut Step<'_>,
    unit: &mut Unit,
    gone: SubActionsGone,
    by: Releaser,
) -> GenieResult<()> {
    if let Some(object_id) = gone.released {
        release(step, unit, object_id, by)?;
    }
    if gone.stops
        && let Some(mover) = step.world.parts.mover_mut(unit.id)
    {
        mover.stop()?;
    }
    Ok(())
}

impl Unit {
    /// The Artifact's part of the update (`0x140aad250`): its Enter sub-action first
    /// ([`Unit::artifact_enter_part`]), then its walk and the check for a new owner
    /// ([`Unit::artifact_walk_and_check`]), then the handover it found
    /// ([`Unit::artifact_handover`]). In the first tick it waits.
    pub(crate) fn artifact_part(
        &mut self,
        actions: &mut LayersAction,
        step: &mut Step<'_>,
        me: Me,
        roles: &Roles,
        slots: Slots,
    ) -> GenieResult<Option<Captured>> {
        let by = self.releaser(step)?;
        if step.world.ticks <= Tick::new(1) {
            return Ok(None);
        }
        self.artifact_enter_part(actions, step, me, by, roles)?;
        let new = self.artifact_walk_and_check(actions, step, me, by, roles)?;
        self.artifact_handover(actions, step, new, roles, slots)
    }

    /// The Artifact's Enter sub-action (an enter command's, `FUN_1410847e0`) updates first
    /// (`FUN_140a992b0`) as the unit's own Enter does, taken as the unit's meanwhile. Once it
    /// ended (1 to 5: the failed one's 3, the list removing it) the sub-actions go, the point is
    /// −1 and the target goes (slot 0x68 with none, released by `by`); an Enter that got the
    /// unit in stays, and the inside update of the next update deletes the Artifact.
    pub(crate) fn artifact_enter_part(
        &mut self,
        actions: &mut LayersAction,
        step: &mut Step<'_>,
        me: Me,
        by: Releaser,
        roles: &Roles,
    ) -> GenieResult<()> {
        let Some(enter_action) = actions.list.artifact_mut().and_then(Artifact::take_enter) else {
            return Ok(());
        };
        let own = actions.list.take_enter();
        actions.list.set_enter(Some(enter_action));
        let updated = self.enter_part(actions, step, me, roles);
        let mut after = actions.list.take_enter();
        actions.list.set_enter(own);
        if updated? == HeadCode::Deletes
            && let Some(enter_action) = after.take()
        {
            // its sub-action list deletes it (FUN_140a98e20): the Enter's destructor
            Action::Enter(enter_action)
                .tear_down(step.world.parts.motion_mut(self.id))?
                .let_go(step, self, by)?;
        }
        if let Some(art) = actions.list.artifact_mut() {
            if let Some(enter) = after {
                art.sub = Some(ArtifactSub::Enter { enter });
            } else {
                art.goal = None;
                if let Some(object_id) = art.drop_target() {
                    release(step, self, object_id, by)?;
                }
            }
        }
        Ok(())
    }

    /// The Artifact's walk and its check for a new owner ([`Artifact::update`] on the step, the
    /// action copied out of its list and written back after): what it reads of the unit
    /// ([`ArtifactReads`]: whether it may change hands, the combat objects whose sight's capture
    /// flag is 0 never do). The handover found, if any;
    /// none without an Artifact.
    pub(crate) fn artifact_walk_and_check(
        &mut self,
        actions: &mut LayersAction,
        step: &mut Step<'_>,
        me: Me,
        by: Releaser,
        roles: &Roles,
    ) -> GenieResult<Option<Handover>> {
        let Some(mut artifact) = actions.list.artifact().copied() else {
            return Ok(None);
        };
        let capture = step
            .world
            .spatial
            .sight
            .as_ref()
            .and_then(|visibility| visibility.seer(self.id))
            .map(|seer| seer.capture);
        let combat =
            MasterKind::COMBAT <= self.master(step.world.master_tables(), step.player)?.kind;
        let artifact_reads = ArtifactReads {
            searches: !(combat && capture == Some(CaptureKind::None)),
            by,
        };
        // written back before an error too: the stop's context reads the list
        let new = artifact.update(step, self, me, &artifact_reads, roles);
        if let Some(own) = actions.list.artifact_mut() {
            *own = artifact;
        }
        new
    }

    /// The handover the Artifact's check found: to a new owner its capture
    /// ([`Unit::capture`]); state 100's to Gaia the unit's stop (slot 0x148, [`Unit::capture_stop`])
    /// and slot 0x70 to Gaia, reported when the unit or a building changed hands. None changes
    /// nothing.
    pub(crate) fn artifact_handover(
        &mut self,
        actions: &mut LayersAction,
        step: &mut Step<'_>,
        new: Option<Handover>,
        roles: &Roles,
        slots: Slots,
    ) -> GenieResult<Option<Captured>> {
        match new {
            Some(Handover::Taken { new, .. }) => {
                let (main, also) = self.capture(actions, step, new, roles, slots)?;
                Ok(Some(Captured {
                    new,
                    main,
                    also,
                    building: take(&mut step.building_handover),
                }))
            }
            Some(Handover::ToGaia) => {
                let class = self.placed_class(step)?;
                self.capture_stop(actions, step, class, roles)?;
                let main = self.change_owner(actions, step, PlayerId::GAIA, roles)?
                    == OwnerChange::Changed;
                let building = take(&mut step.building_handover);
                Ok((main || building).then(|| Captured {
                    new: PlayerId::new(0),
                    main,
                    also: Vec::new(),
                    building,
                }))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::attack::tests::Rig;
    #[cfg(test)]
    use crate::combat::damage::Victim;
    #[cfg(test)]
    use crate::garrison::tests::enter_task;
    use crate::map::Footprint;
    use crate::movement::Speed;
    use crate::movement::move_to::MoveWaitState;
    use crate::movement::move_to::{MoveDefaults, MoveToReads, MoveWait, SpriteReads, WalkTo};
    use crate::objects::ActionState;
    use crate::objects::Graphics;
    use crate::objects::ObjectId;
    use crate::objects::SpeedMultiplier;
    use crate::objects::SpriteFlags;
    use crate::objects::sprite::Graphic;
    #[cfg(test)]
    use crate::objects::{FacetCount, Frame};
    use crate::test_ids::object_id;
    use crate::testing::class;
    use crate::testing::roles;
    use crate::testing::tuning;
    #[cfg(test)]
    use crate::world::WorldTime;
    use std::sync::Arc;

    #[test]
    fn check_finds_no_owner_and_waits_one_second() {
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        let _ = artifact.step(Millis::new(50), None).unwrap();
        assert_eq!(
            (artifact.state, artifact.timer),
            (ArtifactState::Wait, Seconds::new(1.0))
        );
    }

    #[test]
    fn move_point_starts_moveto_and_the_check_runs_again() {
        // MarvinBlue/426048731's cow 9517 in 1118: MOVE to (28.5, 93.5); slot 8 takes the point
        let mut artifact = Artifact {
            state: ArtifactState::Wait,
            timer: Seconds::new(0.4),
            ..Artifact::new(PlayerId::new(1), None)
        };
        assert_eq!(
            artifact.take_point(
                Position::new(28.5, 93.5, 0.0),
                &move_reads(&sprites()),
                &tuning().move_to
            ),
            Ok(SubActionsGone::default())
        );
        assert_eq!(
            (artifact.state, artifact.timer, artifact.goal),
            (
                ArtifactState::Check,
                Seconds::new(0.4),
                Some(Position::new(28.5, 93.5, 0.0))
            )
        );
        // a MoveTo to the point within 0 that is not quiet: the action's AI hears its end
        let to_point = MoveToAction {
            voice: Voice::Loud,
            ..MoveToAction::sub_to_point(Position::new(28.5, 93.5, 0.0), Distance::ZERO, None)
        };
        assert_eq!(artifact.move_to().copied(), Some(to_point));
        // a second move while the first walks: its MoveTo goes, which stops the unit (the
        // sheep 9520 in 768)
        let mut walking = Artifact {
            sub: Some(ArtifactSub::Walk {
                move_to: MoveToAction::sub_in(MoveTo::Moving),
            }),
            ..artifact
        };
        assert_eq!(
            walking.take_point(
                Position::new(28.5, 94.5, 0.0),
                &move_reads(&sprites()),
                &tuning().move_to
            ),
            Ok(SubActionsGone {
                stops: true,
                released: None
            })
        );
        assert_eq!(
            walking
                .move_to()
                .copied()
                .map(|move_to_action| move_to_action.state),
            Some(MoveTo::Start)
        );
        // a MoveTo that ended goes with the point, then the check waits again
        artifact.sub = Some(ArtifactSub::Walk {
            move_to: MoveToAction {
                state: MoveTo::Arrived,
                ..to_point
            },
        });
        let mut rig = Rig::on_the_path(Victim::default());
        let found = rig.run(50, |step, unit, _, _, _| {
            artifact.update(step, unit, me(), &reads(), &roles())
        });
        assert_eq!(found, Ok(None));
        assert_eq!(
            (
                artifact.state,
                artifact.timer,
                artifact.move_to().copied(),
                artifact.goal
            ),
            (ArtifactState::Wait, Seconds::new(1.0), None, None)
        );
    }

    /// The one sub-action (FD-045-D63): a walk gives its `MoveTo` and no Enter, an Enter its
    /// Enter and no `MoveTo`; each take takes only its own kind and leaves the other.
    #[test]
    fn artifact_holds_one_sub_action_walk_or_enter() {
        let walk = MoveToAction::sub_in(MoveTo::Moving);
        let mut artifact = Artifact {
            sub: Some(ArtifactSub::Walk { move_to: walk }),
            ..Artifact::new(PlayerId::new(1), None)
        };
        assert_eq!((artifact.move_to(), artifact.enter()), (Some(&walk), None));
        assert_eq!(artifact.take_enter(), None);
        assert_eq!(artifact.take_move_to(), Some(walk));
        assert_eq!(artifact.sub, None);
        let enter = EnterAction::new(
            ContainerId::new(object_id(11970)),
            Position::new(34.125, 123.333, 0.0),
            &enter_task(Distance::ZERO, class::TRANSPORT),
        );
        artifact.sub = Some(ArtifactSub::Enter { enter });
        assert_eq!((artifact.move_to(), artifact.enter()), (None, Some(&enter)));
        assert_eq!(artifact.take_move_to(), None);
        assert_eq!(artifact.take_enter(), Some(enter));
        assert_eq!(artifact.sub, None);
    }

    /// `set_state(4)` and `set_state(5)` (`0x140aace30`) delete every sub-action
    /// (`FUN_140a98620`), the Enter too: its transport is let go and its walking `MoveTo` stops
    /// the unit (429750109's livestock 11962 walking into the transport 11970, then MOVE to
    /// (32.854168, 120.729164) in update 44: the new `MoveTo` alone holds it).
    #[test]
    fn move_deletes_the_held_enter_letting_its_transport_go() {
        let walking_enter = || {
            EnterAction::new(
                ContainerId::new(object_id(11970)),
                Position::new(34.125, 123.333, 0.0),
                &enter_task(Distance::ZERO, class::TRANSPORT),
            )
            .walking(MoveTo::Moving)
        };
        let by_enter = SubActionsGone {
            stops: true,
            released: Some(object_id(11970)),
        };
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        artifact.sub = Some(ArtifactSub::Enter {
            enter: walking_enter(),
        });
        assert_eq!(
            artifact.take_point(
                Position::new(32.854_168, 120.729_164, 0.0),
                &move_reads(&sprites()),
                &tuning().move_to
            ),
            Ok(by_enter)
        );
        assert_eq!(
            (
                artifact.enter().copied(),
                artifact
                    .move_to()
                    .copied()
                    .map(|move_to_action| move_to_action.state),
                artifact.goal
            ),
            (
                None,
                Some(MoveTo::Start),
                Some(Position::new(32.854_168, 120.729_164, 0.0))
            )
        );
        // to an object: the old target goes and the new one comes first, then the Enter
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        artifact.sub = Some(ArtifactSub::Enter {
            enter: walking_enter(),
        });
        let (targets, gone) = artifact
            .take_object(
                object(9149, Position::new(5.0, 6.0, 0.0), true),
                (true, false),
                &move_reads(&sprites()),
                tuning().artifact,
                &tuning().move_to,
            )
            .unwrap();
        assert_eq!(
            (targets.released, targets.taken, gone),
            (None, Some(object_id(9149)), by_enter)
        );
        assert_eq!(
            (
                artifact.enter().copied(),
                artifact
                    .move_to()
                    .copied()
                    .map(|move_to_action| move_to_action.state)
            ),
            (None, Some(MoveTo::Start))
        );
        // an Enter that does not walk stops nothing
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        artifact.sub = Some(ArtifactSub::Enter {
            enter: EnterAction::new(
                ContainerId::new(object_id(11970)),
                Position::new(1.0, 2.0, 0.0),
                &enter_task(Distance::ZERO, class::TRANSPORT),
            ),
        });
        assert_eq!(
            artifact.take_point(
                Position::new(3.0, 4.0, 0.0),
                &move_reads(&sprites()),
                &tuning().move_to
            ),
            Ok(SubActionsGone {
                stops: false,
                released: Some(object_id(11970))
            })
        );
    }

    /// Slot 0x38 (`FUN_140aad1a0`) with an Enter as the sub-action (`FUN_140a98730` true): the
    /// Enter goes (its transport let go, its walk stopped), then the point and the target; the
    /// teardown (`FUN_140a73fc0`) lets the target go before the Enter's.
    #[test]
    fn the_stop_and_the_teardown_delete_the_held_enter() {
        let mut enter_action = EnterAction::new(
            ContainerId::new(object_id(11970)),
            Position::new(1.0, 2.0, 0.0),
            &enter_task(Distance::ZERO, class::TRANSPORT),
        );
        enter_action = enter_action.walking(MoveTo::Moving);
        let by_enter = SubActionsGone {
            stops: true,
            released: Some(object_id(11970)),
        };
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        (artifact.sub, artifact.goal, artifact.target, artifact.state) = (
            Some(ArtifactSub::Enter {
                enter: enter_action,
            }),
            Some(Position::new(1.0, 2.0, 0.0)),
            Some(object_id(77)),
            ArtifactState::Wait,
        );
        assert_eq!(artifact.stop(), (by_enter, Some(object_id(77))));
        assert_eq!(
            (
                artifact.enter().copied(),
                artifact.goal,
                artifact.target,
                artifact.state
            ),
            (None, None, None, ArtifactState::Check)
        );
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        (artifact.sub, artifact.target) = (
            Some(ArtifactSub::Enter {
                enter: enter_action,
            }),
            Some(object_id(77)),
        );
        assert_eq!(artifact.tear_down(), (Some(object_id(77)), by_enter));
        assert_eq!((artifact.enter().copied(), artifact.target), (None, None));
        // the check that finds an owner deletes it too
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        artifact.sub = Some(ArtifactSub::Enter {
            enter: enter_action,
        });
        let found = artifact
            .step(Millis::new(50), Some(PlayerId::new(2)))
            .unwrap();
        assert_eq!(
            found,
            Some(Handover::Taken {
                new: PlayerId::new(2),
                gone: by_enter
            })
        );
        assert_eq!(artifact.enter().copied(), None);
    }

    /// `FUN_1410847e0` under an Artifact: `FUN_140a98e20` pops the sub-action the enter command
    /// replaces. An Enter goes with its teardown (`FUN_140a85bf0` -> `FUN_140a73fc0`): its
    /// building or transport is let go (slot 0x180), and its walking `MoveTo`'s teardown stops
    /// the unit (430061417's pig 7920 in update 32: INTERACT on the transport 7934 again, the
    /// unit's speed and acceleration 0, so the next velocity step does not move it).
    #[test]
    fn an_enter_command_replaces_the_held_enter_letting_its_target_go_and_stopping_its_walk() {
        let enter = |target| {
            EnterAction::new(
                target,
                Position::new(1.0, 2.0, 0.0),
                &enter_task(Distance::ZERO, class::TRANSPORT),
            )
        };
        let gone = |stops, released| SubActionsGone { stops, released };
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        // nothing held: nothing to let go, nothing walking
        assert_eq!(
            artifact.hold_enter(enter(ContainerId::new(object_id(7934)))),
            gone(false, None)
        );
        // the held Enter walks: its MoveTo's teardown stops the unit
        let held = artifact.take_enter().unwrap();
        artifact.sub = Some(ArtifactSub::Enter {
            enter: held.walking(MoveTo::Moving),
        });
        assert_eq!(
            artifact.hold_enter(enter(ContainerId::new(object_id(7934)))),
            gone(true, Some(object_id(7934)))
        );
        assert_eq!(
            artifact
                .enter()
                .copied()
                .map(|enter_action| (enter_action.target, enter_action.state.move_to().copied())),
            Some((Some(ContainerId::new(object_id(7934))), None))
        );
        // an Enter not walking only lets its target go
        assert_eq!(
            artifact.hold_enter(enter(ContainerId::new(object_id(7935)))),
            gone(false, Some(object_id(7934)))
        );
        // the Artifact's own MoveTo goes with its goal and stops the unit
        artifact.sub = None;
        artifact.sub = Some(ArtifactSub::Walk {
            move_to: MoveToAction::sub_in(MoveTo::Moving),
        });
        assert_eq!(
            artifact.hold_enter(enter(ContainerId::new(object_id(7934)))),
            gone(true, None)
        );
        assert_eq!(artifact.move_to().copied(), None);
    }

    #[test]
    fn gaia_waits_half_second() {
        let mut artifact = Artifact::new(PlayerId::GAIA, None);
        let _ = artifact.step(Millis::new(50), None).unwrap();
        assert_eq!(artifact.timer, Seconds::new(0.5));
    }

    #[test]
    fn the_wait_ends_when_the_timer_reaches_zero_then_one_check_follows() {
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        let _ = artifact.step(Millis::new(50), None).unwrap();
        // 1.0 - 20 × 0.05 in f32 is -1.56e-7, so the 20th update ends the wait
        let waits = (0..30).take_while(|_| {
            let _ = artifact.step(Millis::new(50), None).unwrap();
            artifact.state == ArtifactState::Wait
        });
        assert_eq!(waits.count(), 19);
        assert_eq!(artifact.state, ArtifactState::Check);
        let _ = artifact.step(Millis::new(50), None).unwrap();
        assert_eq!(
            (artifact.state, artifact.timer),
            (ArtifactState::Wait, Seconds::new(1.0))
        );
    }

    #[test]
    fn check_that_finds_new_owner_hands_the_animal_over_and_stays_check() {
        let mut artifact = Artifact::new(PlayerId::GAIA, None);
        let found = artifact
            .step(Millis::new(50), Some(PlayerId::new(2)))
            .unwrap();
        assert_eq!(
            (found, artifact.state),
            (
                Some(Handover::Taken {
                    new: PlayerId::new(2),
                    gone: SubActionsGone::default()
                }),
                ArtifactState::Check
            )
        );
        // no owner found (a combat-type animal whose +0x924 is 0 never asks): it waits
        let mut artifact = Artifact::new(PlayerId::GAIA, None);
        let found = artifact.step(Millis::new(50), None).unwrap();
        assert_eq!((found, artifact.state), (None, ArtifactState::Wait));
        // the conversion's stop: the check again, no MoveTo went
        assert_eq!(artifact.stop(), (SubActionsGone::default(), None));
        assert_eq!(artifact.state, ArtifactState::Check);
        // a walking animal's MoveTo goes (its teardown stops the unit), and its target is let go
        let mut walking = Artifact::new(PlayerId::new(1), None);
        let _ = walking
            .take_point(
                Position::new(28.5, 94.5, 0.0),
                &move_reads(&sprites()),
                &tuning().move_to,
            )
            .unwrap();
        walking.target = Some(object_id(9149));
        assert_eq!(
            walking.stop(),
            (
                SubActionsGone {
                    stops: true,
                    released: None
                },
                Some(object_id(9149))
            )
        );
        assert_eq!(
            (walking.move_to().copied(), walking.goal, walking.target),
            (None, None, None)
        );
        // without a sub-action the point and the target stay (FUN_140a98730 false)
        let mut kept = Artifact::new(PlayerId::new(1), None);
        (kept.goal, kept.target, kept.state) = (
            Some(Position::new(3.0, 4.0, 0.0)),
            Some(object_id(77)),
            ArtifactState::Wait,
        );
        assert_eq!(kept.stop(), (SubActionsGone::default(), None));
        assert_eq!(
            (kept.goal, kept.target, kept.state),
            (
                Some(Position::new(3.0, 4.0, 0.0)),
                Some(object_id(77)),
                ArtifactState::Check
            )
        );
    }

    /// The check that finds a new owner drops its walking `MoveTo` (`FUN_140a98620`, the unit
    /// stopped by the caller) and keeps its point and target, which the conversion's stop then
    /// leaves (no sub-action).
    #[test]
    fn walking_animal_taken_over_keeps_its_point_and_target() {
        let mut artifact = Artifact::new(PlayerId::GAIA, None);
        artifact.goal = Some(Position::new(5.0, 6.0, 0.0));
        artifact.target = Some(object_id(9149));
        artifact.sub = Some(ArtifactSub::Walk {
            move_to: to_object(9149, MoveTo::Moving),
        });
        let found = artifact
            .step(Millis::new(50), Some(PlayerId::new(1)))
            .unwrap();
        assert_eq!(
            found,
            Some(Handover::Taken {
                new: PlayerId::new(1),
                gone: SubActionsGone {
                    stops: true,
                    released: None
                }
            })
        );
        assert_eq!(
            (artifact.move_to().copied(), artifact.goal, artifact.target),
            (
                None,
                Some(Position::new(5.0, 6.0, 0.0)),
                Some(object_id(9149))
            )
        );
        assert_eq!(artifact.stop(), (SubActionsGone::default(), None));
        assert_eq!(artifact.target, Some(object_id(9149)));
    }

    /// State 100 (`0x140aad250`'s case `'d'`): the animal goes to Gaia, the caller's, and the
    /// action checks again; it asks for no owner.
    #[test]
    fn state_100_hands_the_animal_to_gaia() {
        let mut artifact = Artifact {
            state: ArtifactState::ToGaia,
            ..Artifact::new(PlayerId::new(1), None)
        };
        let found = artifact.step(Millis::new(50), None).unwrap();
        assert_eq!(
            (found, artifact.state),
            (Some(Handover::ToGaia), ArtifactState::Check)
        );
    }

    /// Slot 8 on an object (`FUN_140aaca30`): the old target is let go, then the new one taken
    /// (slot 0x68 twice); the same object is let go and taken again; a dead one is not taken.
    /// The teardown lets the target go and stops a walk.
    #[test]
    fn move_to_an_object_lets_the_old_target_go_and_takes_the_new() {
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        let (targets, gone) = artifact
            .take_object(
                object(9149, Position::new(10.0, 12.0, 0.0), true),
                (true, false),
                &move_reads(&sprites()),
                tuning().artifact,
                &tuning().move_to,
            )
            .unwrap();
        assert_eq!(
            (targets, gone),
            (
                Targets {
                    released: None,
                    taken: Some(object_id(9149))
                },
                SubActionsGone::default()
            )
        );
        // a quiet MoveTo to the target within 1 (a building)
        assert_eq!(
            (artifact.target, artifact.move_to().copied()),
            (Some(object_id(9149)), Some(to_object(9149, MoveTo::Start)))
        );
        let (targets, gone) = artifact
            .take_object(
                object(9149, Position::new(10.0, 12.0, 0.0), true),
                (true, false),
                &move_reads(&sprites()),
                tuning().artifact,
                &tuning().move_to,
            )
            .unwrap();
        assert!(gone.stops);
        assert_eq!(
            targets,
            Targets {
                released: Some(object_id(9149)),
                taken: Some(object_id(9149))
            }
        );
        let (targets, _) = artifact
            .take_object(
                object(300, Position::new(1.0, 2.0, 0.0), false),
                (false, false),
                &move_reads(&sprites()),
                tuning().artifact,
                &tuning().move_to,
            )
            .unwrap();
        assert_eq!(
            targets,
            Targets {
                released: Some(object_id(9149)),
                taken: None
            }
        );
        // not a building: within 0; a target in state 7 is not held (`FUN_140a7a230`)
        assert_eq!(
            (
                artifact.target,
                artifact.move_to().copied().map(|move_to_action| (
                    ObjectId::field(move_to_action.target),
                    move_to_action.range
                ))
            ),
            (None, Some((-1, Distance::ZERO)))
        );
        artifact.target = Some(object_id(9149));
        assert_eq!(
            artifact.tear_down(),
            (
                Some(object_id(9149)),
                SubActionsGone {
                    stops: true,
                    released: None
                }
            )
        );
        assert_eq!((artifact.target, artifact.move_to().copied()), (None, None));
        assert_eq!(artifact.tear_down(), (None, SubActionsGone::default()));
    }

    /// The motion step gives up in the update the start's `set_state(4)` runs (426321525's
    /// livestock in update 2656; a unit without speed here): the `MoveTo` fails, as in its moving
    /// state, and the next update ends it with its failure: the unit stops and its AI hears it
    /// (0x1f9 with the move action value).
    #[test]
    fn walk_the_motion_step_gives_up_at_its_start_fails() {
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        let _ = artifact
            .take_point(
                Position::new(35.5, 88.5, 0.0),
                &move_reads(&sprites()),
                &tuning().move_to,
            )
            .unwrap();
        let mut rig = Rig::on_the_path(Victim::default());
        rig.mover_mut().stats.speed = Speed::ZERO;
        rig.run(50, |step, unit, _, _, _| {
            artifact.walk(step, unit, me(), &reads())
        })
        .unwrap();
        assert_eq!(
            (
                artifact
                    .move_to()
                    .copied()
                    .map(|move_to_action| move_to_action.state),
                rig.posted()
            ),
            (Some(MoveTo::Failed), vec![])
        );
        rig.mover_mut().animated.speed = Speed::new(1.0);
        rig.run(50, |step, unit, _, _, _| {
            artifact.walk(step, unit, me(), &reads())
        })
        .unwrap();
        let failed = (NotifyKind::ACTION_FAILED, [AiAction::Move.value(), 0, 0]);
        assert_eq!(
            (artifact.move_to().copied(), rig.posted()),
            (None, vec![failed])
        );
        assert_eq!(rig.mover().animated.speed, Speed::new(0.0));
    }

    /// The Artifact's reads in [`Rig`]'s world: a searching animal, released by itself.
    fn reads() -> ArtifactReads {
        ArtifactReads {
            searches: true,
            by: by(),
        }
    }

    /// The unit 9177 as the targets it lets go count it.
    const fn by() -> Releaser {
        Releaser {
            id: object_id(9177),
            class: Some(class::VILLAGER),
            player: PlayerId::new(1),
        }
    }

    /// [`Rig::on_the_path`]'s unit 9177 as the walker.
    const fn me() -> Me {
        Me {
            human_order: WorldTime::ZERO,
            id: object_id(9177),
            group: None,
            action_state: ActionState::NO_ACTION,
        }
    }

    /// The `MoveTo`s' sprites without defaults or a table: a `MoveTo` shows none.
    fn sprites() -> SpriteReads<'static> {
        static NONE: Graphics = Graphics::new();
        SpriteReads {
            defaults: MoveDefaults::default(),
            graphics: &NONE,
        }
    }

    /// The quiet `MoveTo` in `state` to the building `target` at (10, 12) within 1, without a
    /// sprite.
    fn to_object(target: i32, state: MoveTo) -> MoveToAction {
        MoveToAction {
            point: Position::new(10.0, 12.0, 0.0),
            target: ObjectId::of_field(target),
            range: Distance::new(1.0),
            ..MoveToAction::sub_in(state)
        }
    }

    /// What a new `MoveTo` reads of an animal in state 2 with speed 1, with the sprites
    /// `sprites`.
    fn move_reads<'graphics>(sprites: &SpriteReads<'graphics>) -> MoveToReads<'graphics> {
        MoveToReads {
            sprites: *sprites,
            state: ObjectState::Alive,
            speed: Speed::new(1.0),
            at: Position::ORIGIN,
            ground: Footprint::ZERO,
        }
    }

    /// The object `id` at `at`, far from the animal, in state 2 (alive) or 7.
    fn object(id: i32, at: Position, alive: bool) -> WalkTo {
        WalkTo {
            id: ObjectId::of_field(id),
            at,
            gap: Distance::new(5.0),
            state: Some(if alive {
                ObjectState::Alive
            } else {
                ObjectState::Removed
            }),
        }
    }

    /// A sprite `id` of 10 frames in 16 facets.
    const fn graphic(id: i16) -> Graphic {
        Graphic {
            id: SpriteId::named(id),
            frames: Frame::of_data(10),
            facets: FacetCount::of_data(16),
            speed_multiplier: SpeedMultiplier::ONE,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED,
        }
    }

    /// The sprite its `MoveTo` shows: the task's moving sprite, else the default of its
    /// constructor, toward an object (`FUN_140aa0df0`) or the point (`FUN_140aa0d30`).
    #[test]
    fn an_artifact_move_to_shows_its_task_moving_sprite_else_its_default() {
        let graphics = [3, 20, 21]
            .map(|id| (SpriteId::named(id), graphic(id)))
            .into();
        let sprites = SpriteReads {
            defaults: MoveDefaults {
                point: Some(SpriteId::named(20)),
                object: Some(SpriteId::named(21)),
            },
            graphics: &graphics,
        };
        let shown = |artifact: &Artifact| {
            artifact
                .move_to()
                .copied()
                .and_then(|move_to_action| move_to_action.sprite)
                .map(|graphic| graphic.id)
        };
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        let _ = artifact
            .take_point(
                Position::new(28.5, 93.5, 0.0),
                &move_reads(&sprites),
                &tuning().move_to,
            )
            .unwrap();
        assert_eq!(shown(&artifact), Some(SpriteId::named(20)));
        let _ = artifact
            .take_object(
                object(9149, Position::new(1.0, 1.0, 0.0), true),
                (false, false),
                &move_reads(&sprites),
                tuning().artifact,
                &tuning().move_to,
            )
            .unwrap();
        assert_eq!(shown(&artifact), Some(SpriteId::named(21)));
        artifact.moving = Some(SpriteId::named(3));
        let _ = artifact
            .take_point(
                Position::new(28.5, 93.5, 0.0),
                &move_reads(&sprites),
                &tuning().move_to,
            )
            .unwrap();
        assert_eq!(shown(&artifact), Some(SpriteId::named(3)));
        // a sprite the table lacks fails the move before it changes anything
        let before = Artifact {
            moving: Some(SpriteId::named(4)),
            ..artifact
        };
        let mut lacking = before;
        assert!(
            lacking
                .take_point(
                    Position::new(1.0, 1.0, 0.0),
                    &move_reads(&sprites),
                    &tuning().move_to
                )
                .is_err()
        );
        assert!(
            lacking
                .take_object(
                    object(9149, Position::new(1.0, 1.0, 0.0), true),
                    (false, false),
                    &move_reads(&sprites),
                    tuning().artifact,
                    &tuning().move_to
                )
                .is_err()
        );
        assert_eq!(lacking, before);
    }

    /// A move to a town centre's object by livestock (`FUN_140aa0df0` with master 109) walks to
    /// the point 0.2 below the centre's on both axes, without the target.
    #[test]
    fn livestock_moved_to_town_centre_walks_to_the_point_below_it() {
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        let _ = artifact
            .take_object(
                object(9149, Position::new(10.0, 12.0, 0.0), true),
                (true, true),
                &move_reads(&sprites()),
                tuning().artifact,
                &tuning().move_to,
            )
            .unwrap();
        assert_eq!(
            artifact.move_to().copied().map(|move_to_action| (
                move_to_action.goal(),
                ObjectId::field(move_to_action.target),
                move_to_action.range,
                move_to_action.voice
            )),
            Some((
                Some(Position::new(9.8, 11.8, 0.0)),
                -1,
                Distance::new(1.0),
                Voice::Quiet
            ))
        );
        assert_eq!(artifact.target, Some(object_id(9149)));
    }

    /// The walk's start shows its sprite (`0x140aa29e0`'s `set_state(4)`) before the motion
    /// step, here one that gives up (a unit without speed); the units in the way of its walk go
    /// to the unit AI at once, starting or walking on.
    #[test]
    fn an_artifact_walk_shows_its_sprite_as_it_starts() {
        let mut artifact = Artifact::new(PlayerId::new(1), Some(SpriteId::named(3)));
        let graphics = Graphics::from([(SpriteId::named(3), graphic(3))]);
        let with_3 = SpriteReads {
            defaults: MoveDefaults::default(),
            graphics: &graphics,
        };
        let _ = artifact
            .take_point(
                Position::new(35.5, 88.5, 0.0),
                &move_reads(&with_3),
                &tuning().move_to,
            )
            .unwrap();
        let mut rig = Rig::on_the_path(Victim::default());
        // the top speed derives from the base and the sprite shown
        (rig.mover_mut().stats.speed, rig.mover_mut().stats.base) = (Speed::ZERO, Speed::ZERO);
        assert_eq!(
            Arc::make_mut(&mut rig.world.model.graphics).insert(SpriteId::named(3), graphic(3)),
            None
        );
        rig.mover_mut().in_the_way = vec![object_id(77)];
        rig.run(50, |step, unit, _, _, _| {
            artifact.walk(step, unit, me(), &reads())
        })
        .unwrap();
        let in_the_way = (NotifyKind::IN_THE_WAY, [77, -1, -1]);
        assert_eq!(
            (
                artifact
                    .move_to()
                    .copied()
                    .map(|move_to_action| move_to_action.state),
                rig.sprite()
            ),
            (Some(MoveTo::Failed), Some(SpriteId::named(3)))
        );
        assert_eq!(rig.posted(), vec![in_the_way]);
        let mut walking = Artifact::new(PlayerId::new(1), None);
        let _ = walking
            .take_point(
                Position::new(35.5, 88.5, 0.0),
                &move_reads(&sprites()),
                &tuning().move_to,
            )
            .unwrap();
        let start = walking.move_to().copied().unwrap();
        walking.sub = Some(ArtifactSub::Walk {
            move_to: MoveToAction {
                state: MoveTo::Moving,
                ..start
            },
        });
        let mut rig = Rig::on_the_path(Victim::default());
        rig.mover_mut().in_the_way = vec![object_id(77)];
        rig.run(50, |step, unit, _, _, _| {
            walking.walk(step, unit, me(), &reads())
        })
        .unwrap();
        assert_eq!(
            (
                walking
                    .move_to()
                    .copied()
                    .map(|move_to_action| move_to_action.state),
                rig.posted()
            ),
            (Some(MoveTo::Moving), vec![in_the_way])
        );
        // a waiting MoveTo counts the update's time and posts no unit in the way; at its delay
        // it asks again
        let mut waiting = Artifact {
            sub: Some(ArtifactSub::Walk {
                move_to: MoveToAction {
                    state: MoveTo::Waiting,
                    wait: MoveWait {
                        state: MoveWaitState::Waiting,
                        timer: Seconds::new(0.92),
                        delay: Seconds::new(1.0),
                    },
                    ..start
                },
            }),
            ..Artifact::new(PlayerId::new(1), None)
        };
        let mut rig = Rig::on_the_path(Victim::default());
        rig.mover_mut().in_the_way = vec![object_id(77)];
        rig.run(50, |step, unit, _, _, _| {
            waiting.walk(step, unit, me(), &reads())
        })
        .unwrap();
        assert_eq!(
            (
                waiting
                    .move_to()
                    .copied()
                    .map(|move_to_action| (move_to_action.state, move_to_action.wait.timer)),
                rig.posted()
            ),
            (Some((MoveTo::Waiting, Seconds::new(0.92 + 0.05))), vec![])
        );
        rig.run(50, |step, unit, _, _, _| {
            waiting.walk(step, unit, me(), &reads())
        })
        .unwrap();
        assert_eq!(
            waiting
                .move_to()
                .copied()
                .map(|move_to_action| (move_to_action.state, move_to_action.wait.timer)),
            Some((MoveTo::Start, Seconds::ZERO))
        );
        // a target the world lost is cleared: the MoveTo walks on to its point
        let mut lost = Artifact {
            sub: Some(ArtifactSub::Walk {
                move_to: MoveToAction {
                    target: Some(object_id(4242)),
                    ..MoveToAction::sub_in(MoveTo::Start)
                },
            }),
            ..Artifact::new(PlayerId::new(1), None)
        };
        rig.run(50, |step, unit, _, _, _| {
            lost.walk(step, unit, me(), &reads())
        })
        .unwrap();
        assert_eq!(
            lost.move_to()
                .copied()
                .map(|move_to_action| ObjectId::field(move_to_action.target)),
            Some(-1)
        );
        // a target that died (state 7) is let go, and the walk ends at once (code 5)
        let mut rig = Rig::on_the_path(Victim::default()).holding(600, PlayerId::new(1));
        if let Some(unit) = rig.world.players[1].objects.last_mut() {
            unit.life.state = ObjectState::Removed;
        }
        let mut dying = Artifact {
            sub: Some(ArtifactSub::Walk {
                move_to: MoveToAction {
                    target: Some(object_id(600)),
                    ..MoveToAction::sub_in(MoveTo::Moving)
                },
            }),
            target: Some(object_id(600)),
            ..Artifact::new(PlayerId::new(1), None)
        };
        rig.run(50, |step, unit, _, _, _| {
            dying.walk(step, unit, me(), &reads())
        })
        .unwrap();
        assert_eq!((dying.move_to().copied(), dying.target), (None, None));
        assert_eq!(rig.posted(), vec![]);
    }

    /// A `MoveTo` that ended goes with the point and lets the target go at once (slot 0x68 with
    /// none); a quiet one (to an object) posts nothing, one to the point posts its end.
    #[test]
    fn an_ended_walk_lets_the_target_go() {
        let mut rig = Rig::on_the_path(Victim::default())
            .holding(9149, PlayerId::new(1))
            .holding(77, PlayerId::new(1));
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        artifact.target = Some(object_id(9149));
        artifact.goal = Some(Position::new(1.0, 1.0, 0.0));
        artifact.sub = Some(ArtifactSub::Walk {
            move_to: to_object(9149, MoveTo::Arrived),
        });
        rig.run(50, |step, unit, _, _, _| {
            artifact.walk(step, unit, me(), &reads())
        })
        .unwrap();
        assert_eq!(
            (
                artifact.target,
                artifact.goal,
                rig.workers(9149, PlayerId::new(1))
            ),
            (None, None, Some(0))
        );
        assert_eq!(rig.posted(), vec![]);
        artifact.target = Some(object_id(77));
        artifact.sub = Some(ArtifactSub::Walk {
            move_to: MoveToAction {
                voice: Voice::Loud,
                ..MoveToAction::sub_in(MoveTo::Failed)
            },
        });
        rig.run(50, |step, unit, _, _, _| {
            artifact.walk(step, unit, me(), &reads())
        })
        .unwrap();
        let failed = (NotifyKind::ACTION_FAILED, [AiAction::Move.value(), 0, 0]);
        assert_eq!(
            (rig.workers(77, PlayerId::new(1)), rig.posted()),
            (Some(0), vec![failed])
        );
        // nothing walking: nothing more
        rig.run(50, |step, unit, _, _, _| {
            artifact.walk(step, unit, me(), &reads())
        })
        .unwrap();
        assert_eq!(rig.posted().len(), 1);
    }

    /// The update on the step: the check that finds no owner (no sight here) waits. A check
    /// that finds one drops the sub-actions, whose effects follow at once ([`let_go_gone`]): the
    /// held Enter's transport is let go and the walking unit stops.
    #[test]
    fn an_update_checks_after_its_walk_and_lets_go_what_handover_drops() {
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        let mut rig = Rig::on_the_path(Victim::default());
        let found = rig.run(50, |step, unit, _, _, _| {
            artifact.update(step, unit, me(), &reads(), &roles())
        });
        assert_eq!((found, artifact.state), (Ok(None), ArtifactState::Wait));
        // the step's effects on a handover: the Enter's transport let go, the unit stopped
        let mut enter_action = EnterAction::new(
            ContainerId::new(object_id(11970)),
            Position::new(1.0, 2.0, 0.0),
            &enter_task(Distance::ZERO, class::TRANSPORT),
        );
        enter_action = enter_action.walking(MoveTo::Moving);
        let mut rig = Rig::on_the_path(Victim::default()).holding(11970, PlayerId::new(1));
        rig.mover_mut().animated.speed = Speed::new(1.0);
        let mut artifact = Artifact::new(PlayerId::new(1), None);
        artifact.sub = Some(ArtifactSub::Enter {
            enter: enter_action,
        });
        let found = artifact
            .step(Millis::new(50), Some(PlayerId::new(2)))
            .unwrap();
        let Some(Handover::Taken { new, gone }) = found else {
            panic!("a handover to player 2: {found:?}");
        };
        assert_eq!(new, PlayerId::new(2));
        rig.run(50, |step, unit, _, _, _| {
            let_go_gone(step, unit, gone, by())
        })
        .unwrap();
        assert_eq!(rig.mover().animated.speed, Speed::new(0.0));
        assert_eq!(rig.workers(11970, PlayerId::new(1)), Some(0));
    }
}
