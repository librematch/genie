//! The object's action list (`RGE_Action_List`, FD-045-D12): the actions it holds, its current
//! one (the head) first, each a variant of [`Action`].
//!
//! The game keeps a linked list at the action object's `+0x880` (its head at the list's
//! `+0x10`): only the head runs (slot 0x30, `FUN_140a992b0`), the others get the idle update
//! (`FUN_140a78470`), and a removal (`FUN_140a98e20`) tears the head down and starts the next one
//! (slot 0x28). Each kind has its accessors here (`attack()`, `attack_mut()`, `take_attack()`,
//! `set_attack()`), so a part of the object borrows the list apart from its place and its motion
//! part ([`Motion`]).
//!
//! [`Motion`]: crate::objects::layers::Motion

use crate::combat::attack::ATTACK;
use crate::combat::attack::Attack;
use crate::combat::missile::MISSILE;
use crate::combat::missile::MissileAction;
use crate::construction::build::BuildAction;
use crate::construction::repair::REPAIR;
use crate::construction::repair::RepairAction;
use crate::construction::wait_solidify::WAIT_SOLIDIFY;
use crate::construction::wait_solidify::WaitSolidify;
use crate::garrison::ENTER;
use crate::garrison::EnterAction;
use crate::garrison::transport::TRANSPORT;
use crate::garrison::transport::TransportAction;
use crate::gathering::gather::GATHER;
use crate::gathering::gather::GatherAction;
use crate::gathering::hunt::HUNT;
use crate::gathering::hunt::HuntAction;
use crate::gathering::hunt::HuntState;
use crate::gathering::make::MAKE;
use crate::gathering::make::MakeAction;
use crate::movement::explore::EXPLORE;
use crate::movement::explore::ExploreAction;
use crate::movement::fly::FLY;
use crate::movement::fly::Fly;
use crate::movement::move_to::MOVE_TO;
use crate::movement::move_to::MoveToAction;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::RoleId;
use crate::objects::masters::ActionKind;
use crate::ownership::artifact::ARTIFACT_KIND;
use crate::ownership::artifact::Artifact;
use crate::ownership::artifact::ArtifactSub;
use genie_diff::Diff;
use std::mem::discriminant;
use std::slice::{Iter, IterMut};

/// Declares the action kinds: the [`Action`] variants and, for each, the list's accessors.
macro_rules! action_kinds {
    ($($(#[$doc:meta])* $variant:ident($ty:ty) => $get:ident, $get_mut:ident, $take:ident,
        $set:ident, $with:ident;)*) => {
        /// One action of an object's list, by its kind.
        #[derive(Clone, Debug, PartialEq, Diff)]
        pub enum Action {
            $($(#[$doc])* $variant($ty),)*
        }

        impl ActionList {
            $(
                #[doc = concat!("The list's `", stringify!($variant), "`, if it holds one.")]
                #[must_use]
                pub fn $get(&self) -> Option<&$ty> {
                    self.0.iter().find_map(|action| match action {
                        Action::$variant(held) => Some(held),
                        _ => None,
                    })
                }

                #[doc = concat!("The list's `", stringify!($variant), "` to change, if it holds one.")]
                pub fn $get_mut(&mut self) -> Option<&mut $ty> {
                    self.0.iter_mut().find_map(|action| match action {
                        Action::$variant(held) => Some(held),
                        _ => None,
                    })
                }

                #[doc = concat!("The list's `", stringify!($variant), "` leaves it (no teardown runs).")]
                pub fn $take(&mut self) -> Option<$ty> {
                    let index = self.0.iter().position(|action| matches!(action, Action::$variant(_)))?;
                    match self.0.remove(index) {
                        Action::$variant(held) => Some(held),
                        _ => None,
                    }
                }

                #[doc = concat!("The list's `", stringify!($variant), "` becomes `action`: in its place")]
                #[doc = "if it holds one, else at the head; None takes it out ([`ActionList::put`])."]
                pub fn $set(&mut self, action: Option<$ty>) {
                    match action {
                        Some(held) => self.put(Action::$variant(held)),
                        None => self.0.retain(|listed| !matches!(listed, Action::$variant(_))),
                    }
                }

                #[doc = concat!("The list with its `", stringify!($variant), "` set ([`ActionList::")]
                #[doc = concat!(stringify!($set), "`]).")]
                #[must_use]
                pub fn $with(mut self, action: Option<$ty>) -> Self {
                    self.$set(action);
                    self
                }
            )*
        }
    };
}

action_kinds! {
    /// The livestock default action ([`crate::ownership::artifact`]).
    Artifact(Artifact) => artifact, artifact_mut, take_artifact, set_artifact, with_artifact;
    /// A builder's Build ([`crate::construction::build`]).
    Build(BuildAction) => build, build_mut, take_build, set_build, with_build;
    /// A repairer's Repair ([`crate::construction::repair`]).
    Repair(RepairAction) => repair, repair_mut, take_repair, set_repair, with_repair;
    /// A `MoveTo` as the object's own action (a wandering animal).
    MoveTo(MoveToAction) => move_to, move_to_mut, take_move_to, set_move_to, with_move_to;
    /// A bird's Fly ([`crate::movement::fly`]).
    Fly(Fly) => fly, fly_mut, take_fly, set_fly, with_fly;
    /// A villager's Hunt ([`crate::gathering::hunt`]).
    Hunt(HuntAction) => hunt, hunt_mut, take_hunt, set_hunt, with_hunt;
    /// A forager's Gather ([`crate::gathering::gather`]).
    Gather(GatherAction) => gather, gather_mut, take_gather, set_gather, with_gather;
    /// Its own Attack (type 9, not a Hunt's): a boar's on its attacker.
    Attack(Attack) => attack, attack_mut, take_attack, set_attack, with_attack;
    /// An auto scout's Explore ([`crate::movement::explore`]).
    Explore(ExploreAction) => explore, explore_mut, take_explore, set_explore, with_explore;
    /// A unit's Enter (a garrison, [`crate::garrison`]).
    Enter(EnterAction) => enter, enter_mut, take_enter, set_enter, with_enter;
    /// A unit's Transport (an unload command's, [`crate::garrison::transport`]).
    Transport(TransportAction) => transport, transport_mut, take_transport, set_transport,
        with_transport;
    /// A missile's flight ([`crate::combat::missile`]).
    Missile(MissileAction) => missile, missile_mut, take_missile, set_missile, with_missile;
    /// A farm's or fish trap's Make ([`crate::gathering::make`]).
    Make(MakeAction) => make, make_mut, take_make, set_make, with_make;
    /// A villager's wait for a foundation ([`crate::construction::wait_solidify`]).
    WaitSolidify(WaitSolidify) => wait_solidify, wait_solidify_mut, take_wait_solidify,
        set_wait_solidify, with_wait_solidify;
}

impl Action {
    /// The action's type (`+0x8`, `FUN_140a7be70`), as word 9 of a checkpoint adds it.
    #[must_use]
    pub const fn kind(&self) -> ActionKind {
        match self {
            Self::Artifact(_) => ARTIFACT_KIND,
            Self::Build(_) => ActionKind::BUILD,
            Self::Repair(_) => REPAIR,
            Self::MoveTo(_) => MOVE_TO,
            Self::Fly(_) => FLY,
            Self::Hunt(_) => HUNT,
            Self::Gather(_) => GATHER,
            Self::Attack(_) => ATTACK,
            Self::Explore(_) => EXPLORE,
            Self::Enter(_) => ENTER,
            Self::Transport(_) => TRANSPORT,
            Self::Missile(_) => MISSILE,
            Self::Make(_) => MAKE,
            Self::WaitSolidify(_) => WAIT_SOLIDIFY,
        }
    }

    /// The action's state (`+0x18`); a `MoveTo`'s by its number.
    #[must_use]
    pub const fn state(&self) -> ActionState {
        ActionState::new(match self {
            Self::Artifact(artifact) => artifact.state.get(),
            Self::Build(build_action) => build_action.state.get(),
            Self::Repair(repair_action) => repair_action.state.get(),
            Self::MoveTo(move_to_action) => move_to_action.state.number(),
            Self::Fly(fly) => fly.state.get(),
            Self::Hunt(hunt_action) => hunt_action.state.get(),
            Self::Gather(gather_action) => gather_action.state.get(),
            Self::Attack(attack) => attack.state.get(),
            Self::Explore(explore_action) => explore_action.state.get(),
            Self::Enter(enter_action) => enter_action.state.get(),
            Self::Transport(transport_action) => transport_action.state.get(),
            Self::Missile(missile_action) => missile_action.state.get(),
            Self::Make(make_action) => make_action.state.get(),
            Self::WaitSolidify(wait_solidify) => wait_solidify.state.get(),
        })
    }
}

impl Action {
    /// Slot 0x190's read of the action's target (`+0x30`), none for an action without one (an
    /// Artifact not moving to an object, a Fly, an Explore, a Transport, a Make, a wait for a
    /// foundation, which keeps the foundation at `+0x68`).
    #[must_use]
    pub fn target(&self) -> Option<ObjectId> {
        match self {
            Self::Artifact(artifact) => artifact.target,
            Self::Build(build_action) => build_action.target.map(RoleId::id),
            Self::Repair(repair_action) => repair_action.target.map(RoleId::id),
            Self::MoveTo(move_to_action) => move_to_action.target,
            Self::Hunt(hunt_action) => hunt_action.target.map(RoleId::id),
            Self::Gather(gather_action) => gather_action.target.map(RoleId::id),
            Self::Attack(attack) => attack.target.map(RoleId::id),
            Self::Enter(enter_action) => enter_action.target.map(RoleId::id),
            Self::Missile(missile_action) => missile_action.target.map(RoleId::id),
            Self::Fly(_)
            | Self::Explore(_)
            | Self::Transport(_)
            | Self::Make(_)
            | Self::WaitSolidify(_) => None,
        }
    }

    /// The action's second target (`+0x34`, `FUN_140a78430`), if any: a Hunt's, a Gather's
    /// or a Build's drop site, a missile's shooter, an Attack's second target; the slice's other
    /// actions hold none (an import checks the save's).
    #[must_use]
    pub fn second_target(&self) -> Option<ObjectId> {
        match self {
            Self::Hunt(hunt_action) => hunt_action.carry_to.map(RoleId::id),
            Self::Gather(gather_action) => gather_action.carry_to.map(RoleId::id),
            Self::Build(build_action) => build_action.site.map(RoleId::id),
            Self::Missile(missile_action) => missile_action.fired_from.map(RoleId::id),
            Self::Attack(attack) => attack.target2.map(RoleId::id),
            _ => None,
        }
    }
}

/// An action's sub-action (its list at `+0x50`) whose slot 0x50 a task change runs
/// (`FUN_140a979b0`): a walk's `MoveTo` or a Hunt's Attack.
#[derive(Debug, PartialEq)]
pub(crate) enum SubActionMut<'action> {
    /// A walk's `MoveTo`.
    MoveTo(&'action mut MoveToAction),
    /// A Hunt's Attack (its state 12).
    Attack(&'action mut Attack),
}

impl Action {
    /// Its sub-action (`+0x50`, one at most in every state genie models), none for an action
    /// that holds none: a `MoveTo`, a Missile, a Make and a wait for a foundation make none, and
    /// an Artifact's Enter has no slot 0x50 of its own (`0x140a782d0`, the base's).
    pub(crate) fn sub_action_mut(&mut self) -> Option<SubActionMut<'_>> {
        match self {
            Self::Attack(attack) => walk(attack.state.walk_mut()),
            Self::Build(build_action) => walk(build_action.state.walk_mut()),
            Self::Repair(repair_action) => walk(repair_action.state.walk_mut()),
            Self::Gather(gather_action) => walk(gather_action.state.walk_mut()),
            Self::Enter(enter_action) => walk(enter_action.state.walk_mut()),
            Self::Transport(transport_action) => walk(transport_action.state.walk_mut()),
            Self::Hunt(hunt_action) => match &mut hunt_action.state {
                HuntState::Attack { attack } => attack.as_mut().map(SubActionMut::Attack),
                walking => walk(walking.walk_mut()),
            },
            Self::Fly(fly) => fly.state.move_to_mut().map(SubActionMut::MoveTo),
            Self::Explore(explore_action) => {
                explore_action.state.move_to_mut().map(SubActionMut::MoveTo)
            }
            Self::Artifact(artifact) => match &mut artifact.sub {
                Some(ArtifactSub::Walk { move_to }) => Some(SubActionMut::MoveTo(move_to)),
                Some(ArtifactSub::Enter { .. }) | None => None,
            },
            Self::MoveTo(_) | Self::Missile(_) | Self::Make(_) | Self::WaitSolidify(_) => None,
        }
    }
}

/// The walk's `MoveTo` of a state's sub-action slot, none for an empty one.
fn walk(slot: Option<&mut Option<MoveToAction>>) -> Option<SubActionMut<'_>> {
    slot.and_then(Option::as_mut).map(SubActionMut::MoveTo)
}

/// The object's actions (`RGE_Action_List`), the head first.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct ActionList(Vec<Action>);

impl ActionList {
    /// No action.
    pub const EMPTY: Self = Self(Vec::new());

    /// `action` replaces the list's action of its kind in its place, or else becomes the head.
    pub fn put(&mut self, action: Action) {
        let kind = discriminant(&action);
        match self
            .0
            .iter_mut()
            .find(|action| discriminant(*action) == kind)
        {
            Some(held) => *held = action,
            None => self.0.insert(0, action),
        }
    }

    /// Whether the list is empty (`FUN_140a98730` false: no current action).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `get_action` (`FUN_140a986a0`): the first action of type `kind`, if the list holds one.
    #[must_use]
    pub fn of_kind(&self, kind: ActionKind) -> Option<&Action> {
        self.0.iter().find(|action| action.kind() == kind)
    }

    /// `have_action` (`FUN_140a98740`): whether the list holds an action of type `kind`.
    #[must_use]
    pub fn holds(&self, kind: ActionKind) -> bool {
        self.of_kind(kind).is_some()
    }

    /// The current action (the head), if any (`FUN_140a986f0`: the action of the list's first
    /// node, `+0x10`; null for an empty list).
    #[must_use]
    pub fn head(&self) -> Option<&Action> {
        self.0.first()
    }

    /// The current action (the head) to change, if any ([`ActionList::head`], `FUN_140a986f0`):
    /// a dispatch on its kind hands the variant on.
    pub fn head_mut(&mut self) -> Option<&mut Action> {
        self.0.first_mut()
    }

    /// The head leaves the list (no destructor runs; [`crate::objects::teardown`]'s deletions
    /// run it).
    pub(crate) fn pop_head(&mut self) -> Option<Action> {
        (!self.0.is_empty()).then(|| self.0.remove(0))
    }

    /// The actions, the head first.
    pub fn iter(&self) -> Iter<'_, Action> {
        self.0.iter()
    }

    /// The actions to change, the head first.
    pub(crate) fn iter_mut(&mut self) -> IterMut<'_, Action> {
        self.0.iter_mut()
    }
}

impl<'list> IntoIterator for &'list ActionList {
    type Item = &'list Action;
    type IntoIter = Iter<'list, Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'list> IntoIterator for &'list mut ActionList {
    type Item = &'list mut Action;
    type IntoIter = IterMut<'list, Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership::artifact::ArtifactState;
    use crate::players::PlayerId;

    fn artifact(state: ArtifactState) -> Artifact {
        Artifact {
            state,
            ..Artifact::new(PlayerId::new(1), None)
        }
    }

    fn other() -> Action {
        Action::Artifact(Artifact::new(PlayerId::GAIA, None))
    }

    #[test]
    fn kind_set_twice_keeps_one_entry_in_its_place() {
        let mut list = ActionList::default();
        assert!(list.is_empty());
        list.put(Action::Artifact(artifact(ArtifactState::Check)));
        assert_eq!(
            list.head(),
            Some(&Action::Artifact(artifact(ArtifactState::Check)))
        );
        list.set_artifact(Some(artifact(ArtifactState::Wait)));
        assert_eq!(
            list.iter().cloned().collect::<Vec<_>>(),
            [Action::Artifact(artifact(ArtifactState::Wait))],
            "the held kind changed in its place"
        );
        list.put(other());
        assert_eq!((&list).into_iter().count(), 1);
        assert_eq!(list.head(), Some(&other()));
    }

    #[test]
    fn the_accessors_reach_only_their_kind() {
        let mut list = ActionList::default().with_artifact(Some(artifact(ArtifactState::Wait)));
        assert_eq!(list.artifact(), Some(&artifact(ArtifactState::Wait)));
        assert_eq!(list.make(), None);
        if let Some(artifact) = list.artifact_mut() {
            artifact.state = ArtifactState::ToGaia;
        }
        assert_eq!(list.make_mut(), None);
        assert_eq!(list.take_make(), None);
        assert_eq!(list.take_artifact(), Some(artifact(ArtifactState::ToGaia)));
        assert!(list.is_empty());
    }

    #[test]
    fn an_action_reads_its_kind_and_state() {
        let action = Action::Artifact(artifact(ArtifactState::Check));
        assert_eq!(
            (action.kind(), action.state()),
            (ARTIFACT_KIND, ActionState::new(6))
        );
        let list = ActionList::default().with_artifact(Some(artifact(ArtifactState::Check)));
        assert_eq!(list.of_kind(ARTIFACT_KIND), Some(&action));
        assert!(list.holds(ARTIFACT_KIND));
        assert_eq!(list.of_kind(ATTACK), None);
        assert!(!list.holds(ATTACK));
    }

    #[test]
    fn setting_none_takes_the_kind_out() {
        let mut list = ActionList::default().with_artifact(Some(artifact(ArtifactState::Check)));
        list.set_make(None);
        assert_eq!(list.iter().count(), 1);
        list.set_artifact(None);
        assert_eq!(list.artifact(), None);
        list.set_artifact(None);
        assert!(list.is_empty());
    }
}
