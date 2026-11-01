//! `RGE_Action_Object` (types 40 and up, FD-045-D06): the action layer, the save's action list
//! (`RgeActionObject`'s `actions`), then what genie keeps for the default task its update starts
//! (`RGE_Action_Object::update`, `0x14106ac90`). Every object holds it for now (T4 step 3).

use crate::GenieError;
use crate::objects::actions::ActionList;
use genie_diff::Diff;

/// The action layer of an object: its action list and its master's default task.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct ActionLayer {
    /// Its action list (`RGE_Action_List`, [`ActionList`]): the actions it holds, its current one
    /// first.
    pub list: ActionList,
    /// A default task other than the Artifact action. `RGE_Action_Object::update` (0x14106ac90)
    /// starts it in an object's first update if the object is still in state 2 after its static
    /// step; the slice does not model it, so that update stops the player.
    #[diff(unsaved, eq)]
    pub unmodelled_action: Option<GenieError>,
    /// What its master's default task would start once its action list is empty, when the
    /// slice does not model it (None: nothing, as a wild animal's Explore).
    #[diff(unsaved, eq)]
    pub default_task: Option<GenieError>,
    /// Its master's default task is a Fly, which starts once its action list is empty (a bird at
    /// a game's start, before its first update) with the task its master holds then. Not
    /// saved: a load from the header sets it, a load of a later save (the Fly saved) does not.
    #[diff(unsaved)]
    pub default_fly: bool,
    /// Its master's default task is a Make, which starts once its action list is empty in
    /// state 2 (a farm or fish trap before its first update as a built building), with the task
    /// its master holds then.
    pub default_make: bool,
    /// Its master's default task is the Artifact action (livestock), which starts again once its
    /// action list is empty in state 2 (an animal out of a building whose Artifact went), its
    /// first update in the next tick, as a Fly's.
    #[diff(gap = "the import reads the shared master's default task, not an own master's")]
    pub default_artifact: bool,
}
