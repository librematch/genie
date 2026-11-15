//! `RGE_Action_Object` (types 40 and up, FD-045-D06): the action layer, the save's action list
//! (`RgeActionObject`'s `actions`) and the fields beside it, and the reads of its top action. An
//! object that has one holds it in the world's table (`Parts::actions`, FD-045-D59).

use crate::combat::attack::ATTACK;
use crate::construction::pack::PackProgress;
use crate::garrison::ENTER;
use crate::gathering::make::MAKE;
use crate::movement::move_to::MOVE_TO;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::actions;
use crate::objects::actions::ActionList;
use crate::objects::actions::target_object;
use crate::objects::layers::CommandCount;
use crate::objects::layers::{IdleTime, Speedup};
use crate::objects::masters::ActionKind;
use crate::ownership::artifact::{ARTIFACT_KIND, Artifact};
use crate::world::Unread;
use crate::world::checkpoint::TopAction;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

/// Slots 0x190 and 0x288 of an action the slice does not model.
const UNMODELLED_SLOTS: GenieError =
    GenieError::unsupported("slots 0x190 and 0x288 of an action the slice does not model");

/// The action layer of an object: its action list, a villager's idle time, the building that
/// speeds it up, its command count and its pack progress.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Action {
    /// Its action list (`RGE_Action_List`, [`ActionList`]): the actions it holds, its current one
    /// first.
    pub list: ActionList,
    /// A villager's idle time not yet in its owner's statistics (`+0x898` to `+0x8a0`,
    /// [`IdleTime`]).
    #[diff(unsaved)]
    pub idle: IdleTime,
    /// The building that speeds it up (`+0x890`, `+0x894`, [`Speedup`]).
    pub speedup: Speedup,
    /// The works and moves it took (`command_flag`, `+0x888`, [`CommandCount`]).
    pub commands: CommandCount,
    /// `+0x88a` ([`PackProgress`]): the share of its top Pack done that its last stop kept
    /// (`FUN_14106fa60`), which the load (`FUN_14106ec00`, `0x14106f489`) and a stop under any
    /// other action set to 0. Only the Pack's and the Unpack's installs read it.
    #[diff(unsaved)]
    pub(crate) pack_progress: PackProgress,
}

impl Action {
    /// The layer of an object the world's table does not hold (a tree's): no action, no idle
    /// time, no speed-up building and no command.
    pub const NONE: Self = Self {
        list: ActionList::EMPTY,
        idle: IdleTime::NONE,
        speedup: Speedup::NONE,
        commands: CommandCount::ZERO,
        pack_progress: PackProgress::NONE,
    };

    /// Its slot 0x190 (`FUN_14106d090`): the object its top action's target names
    /// ([`target_object`], `FUN_140a78450`: a target of 0 or more), as `find` looks it up in the
    /// world's table; none without a top action.
    pub(crate) fn top_action_object<T>(
        &self,
        find: impl FnOnce(ObjectId) -> Option<T>,
    ) -> Option<T> {
        target_object(self.top_target(), find)
    }

    /// Its top action's slots 0x190 and 0x288 (`FUN_14106d090`, `FUN_141082dc0`): the action has
    /// neither a target nor a point (`+0x30`, `+0x38` −1), or there is none. An Artifact holds
    /// them while it walks a move ([`crate::ownership::artifact::Artifact::idle`]); a `MoveTo` always holds
    /// its point. `unread`: the load could not read the object whole ([`Unread`]).
    ///
    /// # Errors
    ///
    /// A top action the slice does not model.
    pub fn top_action_idle(&self, unread: Option<&Unread>) -> GenieResult<bool> {
        if unread.is_some() {
            return Err(UNMODELLED_SLOTS);
        }
        match self.top_action_kind() {
            None => Ok(true),
            Some(ARTIFACT_KIND) => Ok(self.list.artifact().is_some_and(Artifact::idle)),
            // a Make has no target, but its point (`+0x38`) is the base constructor's 0.0
            Some(MOVE_TO | MAKE) => Ok(false),
            Some(_) => Err(UNMODELLED_SLOTS),
        }
    }

    /// The type of its top action, if it has one.
    #[must_use]
    pub fn top_action_kind(&self) -> Option<ActionKind> {
        self.top_action().map(|top_action| top_action.kind)
    }

    /// Its top action (the list's head) as the slice models it, type and state.
    #[must_use]
    pub fn top_action(&self) -> Option<TopAction> {
        self.list.head().map(|action| TopAction {
            kind: action.kind(),
            state: action.state(),
        })
    }

    /// The sub-action of its top action as the slice models it, type and state: the `MoveTo`
    /// (type 1) an action walks with, a Hunt's Attack (9) and an Artifact's Enter (3). None for
    /// an action without one, or without an action.
    #[must_use]
    pub fn top_sub_action(&self) -> Option<TopAction> {
        use crate::movement::move_to::{MOVE_TO, MoveTo};
        let walking = |walk: Option<MoveTo>| {
            walk.map(|move_to| TopAction {
                kind: MOVE_TO,
                state: ActionState::new(move_to.number()),
            })
        };
        match self.list.head()? {
            actions::Action::Artifact(artifact) => walking(
                artifact
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            )
            .or_else(|| {
                artifact.enter().map(|enter_action| TopAction {
                    kind: ENTER,
                    state: ActionState::new(enter_action.state.get()),
                })
            }),
            actions::Action::Hunt(hunt_action) => hunt_action
                .state
                .attack()
                .map(|attack| TopAction {
                    kind: ATTACK,
                    state: ActionState::new(attack.state.get()),
                })
                .or_else(|| {
                    walking(
                        hunt_action
                            .state
                            .move_to()
                            .map(|move_to_action| move_to_action.state),
                    )
                }),
            actions::Action::Build(build_action) => walking(
                build_action
                    .state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::Repair(repair_action) => walking(
                repair_action
                    .state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::Fly(fly) => walking(
                fly.state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::Gather(gather_action) => walking(
                gather_action
                    .state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::Attack(attack) => walking(
                attack
                    .state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::Explore(explore_action) => walking(
                explore_action
                    .state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::Enter(enter_action) => walking(
                enter_action
                    .state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::Transport(transport_action) => walking(
                transport_action
                    .state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::Unpack(unpack_action) => walking(
                unpack_action
                    .state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::MoveTo(_)
            | actions::Action::Missile(_)
            | actions::Action::Make(_)
            | actions::Action::WaitSolidify(_)
            | actions::Action::Housing(_)
            | actions::Action::Pack(_) => None,
        }
    }

    /// The target id of its top action ([`Action::top_action`], [`actions::Action::target`]), as
    /// the action holds it; None without an action the slice holds or without a target. A reader of
    /// the object it names asks [`Action::top_action_object`], which passes over a recyclable id.
    #[must_use]
    pub fn top_target(&self) -> Option<ObjectId> {
        self.list.head().and_then(actions::Action::target)
    }

    /// The second target of its top action ([`actions::Action::second_target`]); None without an action
    /// the slice holds or without a second target.
    #[must_use]
    pub fn top_second_target(&self) -> Option<ObjectId> {
        self.list.head().and_then(actions::Action::second_target)
    }
}
