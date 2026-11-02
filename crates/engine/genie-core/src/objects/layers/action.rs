//! `RGE_Action_Object` (types 40 and up, FD-045-D06): the action layer, the save's action list
//! (`RgeActionObject`'s `actions`), then what genie keeps for the default task its update starts
//! (`RGE_Action_Object::update`, `0x14106ac90`), and the reads of its top action. An object
//! that has one holds it in the world's table (`Parts::actions`, FD-045-D59).

use crate::GenieError;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::actions;
use crate::objects::actions::ActionList;
use crate::objects::layers::DefaultTask;
use crate::objects::masters::ActionType;
use crate::world::checkpoint::TopAction;
use genie_diff::Diff;

/// The action layer of an object: its action list and its master's default task.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Action {
    /// Its action list (`RGE_Action_List`, [`ActionList`]): the actions it holds, its current one
    /// first.
    pub list: ActionList,
    /// A default task other than the Artifact action. `RGE_Action_Object::update` (0x14106ac90)
    /// starts it in an object's first update if the object is still in state 2 after its static
    /// step; the slice does not model it, so that update stops the player.
    #[diff(unsaved, eq)]
    pub unmodelled_action: Option<GenieError>,
    /// What its master's default task starts once its action list is empty in state 2
    /// ([`DefaultTask`]).
    pub default_task: DefaultTask,
}

impl Action {
    /// Its slot 0x190 (`FUN_14106d090`, then `FUN_140a78450`): the object its top action targets
    /// (the action's `+0x30` when not −1); none without a top action.
    ///
    /// # Errors
    ///
    /// A top action the slice does not model.
    pub fn top_action_object(&self) -> Result<Option<ObjectId>, GenieError> {
        if self.unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "slot 0x190 of an action the slice does not model",
            ));
        }
        Ok(self.top_target())
    }

    /// Its top action's slots 0x190 and 0x288 (`FUN_14106d090`, `FUN_141082dc0`): the action has
    /// neither a target nor a point (`+0x30`, `+0x38` −1), or there is none. An Artifact holds
    /// them while it walks a move ([`crate::ownership::artifact::Artifact::idle`]); a `MoveTo` always holds
    /// its point.
    ///
    /// # Errors
    ///
    /// A top action the slice does not model.
    pub fn top_action_idle(&self) -> Result<bool, GenieError> {
        if self.unmodelled_action.is_some() {
            return Err(GenieError::unsupported(
                "slots 0x190 and 0x288 of an action the slice does not model",
            ));
        }
        match self.top_action_kind() {
            None => Ok(true),
            Some(crate::ownership::artifact::ARTIFACT_TYPE) => Ok(self
                .list
                .artifact()
                .is_some_and(crate::ownership::artifact::Artifact::idle)),
            // a Make has no target, but its point (`+0x38`) is the base constructor's 0.0
            Some(crate::movement::move_to::MOVE_TO | crate::gathering::make::MAKE) => Ok(false),
            Some(_) => Err(GenieError::unsupported(
                "slots 0x190 and 0x288 of an action the slice does not model",
            )),
        }
    }

    /// The type of its top action, if it has one.
    #[must_use]
    pub fn top_action_kind(&self) -> Option<ActionType> {
        self.top_action().map(|a| a.kind)
    }

    /// Its top action (the list's head) as the slice models it, type and state.
    #[must_use]
    pub fn top_action(&self) -> Option<TopAction> {
        self.list.head().map(|a| TopAction {
            kind: a.kind(),
            state: a.state(),
        })
    }

    /// The sub-action of its top action as the slice models it, type and state: the `MoveTo`
    /// (type 1) an action walks with, a Hunt's Attack (9) and an Artifact's Enter (3). None for
    /// an action without one, or without an action.
    #[must_use]
    pub fn top_sub_action(&self) -> Option<TopAction> {
        use crate::movement::move_to::{MOVE_TO, MoveTo};
        let walking = |m: Option<MoveTo>| {
            m.map(|m| TopAction {
                kind: MOVE_TO,
                state: ActionState::new(m.number()),
            })
        };
        match self.list.head()? {
            actions::Action::Artifact(a) => walking(a.move_to.map(|m| m.state)).or_else(|| {
                a.enter.map(|e| TopAction {
                    kind: crate::garrison::ENTER,
                    state: ActionState::new(e.state.get()),
                })
            }),
            actions::Action::Hunt(h) => h
                .attack
                .map(|a| TopAction {
                    kind: crate::combat::attack::ATTACK,
                    state: ActionState::new(a.state.get()),
                })
                .or_else(|| walking(h.move_to.map(|m| m.state))),
            actions::Action::Build(b) => walking(b.move_to.map(|m| m.state)),
            actions::Action::Repair(r) => walking(r.move_to.map(|m| m.state)),
            actions::Action::Fly(f) => walking(f.move_to.map(|m| m.state)),
            actions::Action::Gather(g) => walking(g.move_to.map(|m| m.state)),
            actions::Action::Attack(a) => walking(a.move_to.map(|m| m.state)),
            actions::Action::Explore(e) => walking(e.move_to.map(|m| m.state)),
            actions::Action::Enter(e) => walking(e.move_to.map(|m| m.state)),
            actions::Action::Transport(t) => walking(t.move_to.map(|m| m.state)),
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
