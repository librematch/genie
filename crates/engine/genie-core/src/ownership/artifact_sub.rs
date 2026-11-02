//! An Artifact's sub-action: one at most, a move's `MoveTo` or an enter command's Enter.

use crate::garrison::EnterAction;
use crate::movement::move_to::MoveToAction;
use genie_diff::Diff;

/// An Artifact's sub-action (FD-045-D63): one at most, as `FUN_140a98620` deletes the one it
/// holds before another comes ([`super::artifact::Artifact::drop_sub_actions`]). No state of
/// the Artifact makes it: a move or an enter command gives it, and the check that follows
/// (`set_state(6)`) keeps it, so it is the action's own field and not a state's.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub enum ArtifactSub {
    /// The `MoveTo` of a move its player gave the animal: to the point
    /// ([`super::artifact::Artifact::take_point`]) or, quiet, to an object
    /// ([`super::artifact::Artifact::take_object`]).
    Walk {
        /// The walk.
        move_to: MoveToAction,
    },
    /// The Enter an enter command made its sub-action (`FUN_1410847e0` under an Artifact on
    /// top), which its update runs first ([`super::artifact::Artifact::hold_enter`]).
    Enter {
        /// The Enter.
        enter: EnterAction,
    },
}
