//! `RGE_Action_Object` (types 40 and up, FD-045-D06): the action layer, the save's action list
//! (`RgeActionObject`'s `actions`), then what genie keeps for the default task its update starts
//! (`RGE_Action_Object::update`, `0x14106ac90`), and the reads of its top action. An object
//! that has one holds it in the world's table (`Parts::actions`, FD-045-D59).

use crate::combat::attack::ATTACK;
use crate::garrison::ENTER;
use crate::gathering::make::MAKE;
use crate::movement::move_to::MOVE_TO;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::actions;
use crate::objects::actions::ActionList;
use crate::objects::layers::DefaultTask;
use crate::objects::masters::ActionKind;
use crate::ownership::artifact::{ARTIFACT_KIND, Artifact};
use crate::world::Unread;
use crate::world::checkpoint::TopAction;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

/// The action layer of an object: its action list and its master's default task.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Action {
    /// Its action list (`RGE_Action_List`, [`ActionList`]): the actions it holds, its current one
    /// first.
    pub list: ActionList,
    /// What its master's default task starts once its action list is empty in state 2
    /// ([`DefaultTask`]).
    pub default_task: DefaultTask,
}

impl Action {
    /// Its slot 0x190 (`FUN_14106d090`, then `FUN_140a78450`): the object its top action targets
    /// (the action's `+0x30` when not −1); none without a top action. `unread`: the load could
    /// not read the object whole ([`Unread`]).
    ///
    /// # Errors
    ///
    /// A top action the slice does not model.
    pub fn top_action_object(&self, unread: Option<&Unread>) -> GenieResult<Option<ObjectId>> {
        if unread.is_some() {
            return Err(GenieError::unsupported(
                "slot 0x190 of an action the slice does not model",
            ));
        }
        Ok(self.top_target())
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
            return Err(GenieError::unsupported(
                "slots 0x190 and 0x288 of an action the slice does not model",
            ));
        }
        match self.top_action_kind() {
            None => Ok(true),
            Some(ARTIFACT_KIND) => Ok(self.list.artifact().is_some_and(Artifact::idle)),
            // a Make has no target, but its point (`+0x38`) is the base constructor's 0.0
            Some(MOVE_TO | MAKE) => Ok(false),
            Some(_) => Err(GenieError::unsupported(
                "slots 0x190 and 0x288 of an action the slice does not model",
            )),
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
        let walking = |m: Option<MoveTo>| {
            m.map(|move_to| TopAction {
                kind: MOVE_TO,
                state: ActionState::new(move_to.number()),
            })
        };
        match self.list.head()? {
            actions::Action::Artifact(artifact) => walking(artifact.move_to().map(|m| m.state))
                .or_else(|| {
                    artifact.enter().map(|e| TopAction {
                        kind: ENTER,
                        state: ActionState::new(e.state.get()),
                    })
                }),
            actions::Action::Hunt(hunt_action) => hunt_action
                .state
                .attack()
                .map(|a| TopAction {
                    kind: ATTACK,
                    state: ActionState::new(a.state.get()),
                })
                .or_else(|| walking(hunt_action.state.move_to().map(|m| m.state))),
            actions::Action::Build(build_action) => {
                walking(build_action.state.move_to().map(|m| m.state))
            }
            actions::Action::Repair(repair_action) => {
                walking(repair_action.state.move_to().map(|m| m.state))
            }
            actions::Action::Fly(fly) => walking(fly.state.move_to().map(|m| m.state)),
            actions::Action::Gather(gather_action) => {
                walking(gather_action.state.move_to().map(|m| m.state))
            }
            actions::Action::Attack(attack) => walking(attack.state.move_to().map(|m| m.state)),
            actions::Action::Explore(explore_action) => {
                walking(explore_action.state.move_to().map(|m| m.state))
            }
            actions::Action::Enter(enter_action) => {
                walking(enter_action.state.move_to().map(|m| m.state))
            }
            actions::Action::Transport(transport_action) => walking(
                transport_action
                    .state
                    .move_to()
                    .map(|move_to_action| move_to_action.state),
            ),
            actions::Action::MoveTo(_)
            | actions::Action::Missile(_)
            | actions::Action::Make(_)
            | actions::Action::WaitSolidify(_) => None,
        }
    }

    /// The target of its top action ([`Action::top_action`], [`actions::Action::target`]); None without an
    /// action the slice holds or without a target.
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
