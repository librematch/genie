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

use crate::combat::attack::Attack;
use crate::combat::missile::MissileAction;
use crate::construction::build::BuildAction;
use crate::construction::repair::RepairAction;
use crate::construction::wait_solidify::WaitSolidify;
use crate::garrison::EnterAction;
use crate::garrison::transport::TransportAction;
use crate::gathering::gather::GatherAction;
use crate::gathering::hunt::HuntAction;
use crate::gathering::make::MakeAction;
use crate::movement::explore::ExploreAction;
use crate::movement::fly::Fly;
use crate::movement::move_to::MoveToAction;
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::masters::ActionType;
use crate::objects::masters::action_type;
use crate::ownership::artifact::Artifact;
use genie_diff::Diff;

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
                    self.0.iter().find_map(|a| match a {
                        Action::$variant(x) => Some(x),
                        _ => None,
                    })
                }

                #[doc = concat!("The list's `", stringify!($variant), "` to change, if it holds one.")]
                pub fn $get_mut(&mut self) -> Option<&mut $ty> {
                    self.0.iter_mut().find_map(|a| match a {
                        Action::$variant(x) => Some(x),
                        _ => None,
                    })
                }

                #[doc = concat!("The list's `", stringify!($variant), "` leaves it (no teardown runs).")]
                pub fn $take(&mut self) -> Option<$ty> {
                    let i = self.0.iter().position(|a| matches!(a, Action::$variant(_)))?;
                    match self.0.remove(i) {
                        Action::$variant(x) => Some(x),
                        _ => None,
                    }
                }

                #[doc = concat!("The list's `", stringify!($variant), "` becomes `action`: in its place")]
                #[doc = "if it holds one, else at the head; None takes it out ([`ActionList::put`])."]
                pub fn $set(&mut self, action: Option<$ty>) {
                    match action {
                        Some(x) => self.put(Action::$variant(x)),
                        None => self.0.retain(|a| !matches!(a, Action::$variant(_))),
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
    pub const fn kind(&self) -> ActionType {
        match self {
            Self::Artifact(_) => crate::ownership::artifact::ARTIFACT_TYPE,
            Self::Build(_) => action_type::BUILD,
            Self::Repair(_) => crate::construction::repair::REPAIR,
            Self::MoveTo(_) => crate::movement::move_to::MOVE_TO,
            Self::Fly(_) => crate::movement::fly::FLY,
            Self::Hunt(_) => crate::gathering::hunt::HUNT,
            Self::Gather(_) => crate::gathering::gather::GATHER,
            Self::Attack(_) => crate::combat::attack::ATTACK,
            Self::Explore(_) => crate::movement::explore::EXPLORE,
            Self::Enter(_) => crate::garrison::ENTER,
            Self::Transport(_) => crate::garrison::transport::TRANSPORT,
            Self::Missile(_) => crate::combat::missile::MISSILE,
            Self::Make(_) => crate::gathering::make::MAKE,
            Self::WaitSolidify(_) => crate::construction::wait_solidify::WAIT_SOLIDIFY,
        }
    }

    /// The action's state (`+0x18`); a `MoveTo`'s by its number.
    #[must_use]
    pub const fn state(&self) -> ActionState {
        ActionState::new(match self {
            Self::Artifact(a) => a.state.get(),
            Self::Build(a) => a.state.get(),
            Self::Repair(a) => a.state.get(),
            Self::MoveTo(a) => a.state.number(),
            Self::Fly(a) => a.state.get(),
            Self::Hunt(a) => a.state.get(),
            Self::Gather(a) => a.state.get(),
            Self::Attack(a) => a.state.get(),
            Self::Explore(a) => a.state.get(),
            Self::Enter(a) => a.state.get(),
            Self::Transport(a) => a.state.get(),
            Self::Missile(a) => a.state.get(),
            Self::Make(a) => a.state.get(),
            Self::WaitSolidify(a) => a.state.get(),
        })
    }
}

impl Action {
    /// Slot 0x190's read of the action's target (`+0x30`), none for an action without one (an
    /// Artifact not moving to an object, a Fly, an Explore, a Transport, a Make, a wait for a
    /// foundation, which keeps the foundation at `+0x68`).
    #[must_use]
    pub const fn target(&self) -> Option<ObjectId> {
        match self {
            Self::Artifact(a) => a.target,
            Self::Build(a) => a.target,
            Self::Repair(a) => a.target,
            Self::MoveTo(a) => a.target,
            Self::Hunt(a) => a.target,
            Self::Gather(a) => a.target,
            Self::Attack(a) => a.target,
            Self::Enter(a) => match a.target {
                Some(t) => Some(t.id()),
                None => None,
            },
            Self::Missile(a) => a.target,
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
    pub const fn second_target(&self) -> Option<ObjectId> {
        match self {
            Self::Hunt(h) => match h.carry_to {
                Some(s) => Some(s.id()),
                None => None,
            },
            Self::Gather(g) => match g.carry_to {
                Some(s) => Some(s.id()),
                None => None,
            },
            Self::Build(b) => match b.site {
                Some(s) => Some(s.id()),
                None => None,
            },
            Self::Missile(m) => match m.fired_from {
                Some(s) => Some(s.id()),
                None => None,
            },
            Self::Attack(a) => a.target2,
            _ => None,
        }
    }
}

/// The object's actions (`RGE_Action_List`), the head first.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct ActionList(Vec<Action>);

impl ActionList {
    /// `action` replaces the list's action of its kind in its place, or else becomes the head.
    pub fn put(&mut self, action: Action) {
        let kind = std::mem::discriminant(&action);
        match self
            .0
            .iter_mut()
            .find(|a| std::mem::discriminant(*a) == kind)
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
    pub fn of_kind(&self, kind: ActionType) -> Option<&Action> {
        self.0.iter().find(|a| a.kind() == kind)
    }

    /// `have_action` (`FUN_140a98740`): whether the list holds an action of type `kind`.
    #[must_use]
    pub fn holds(&self, kind: ActionType) -> bool {
        self.of_kind(kind).is_some()
    }

    /// The current action (the head), if any.
    #[must_use]
    pub fn head(&self) -> Option<&Action> {
        self.0.first()
    }

    /// The head leaves the list (no destructor runs; [`crate::objects::teardown`]'s deletions
    /// run it).
    pub(crate) fn pop_head(&mut self) -> Option<Action> {
        (!self.0.is_empty()).then(|| self.0.remove(0))
    }

    /// The actions, the head first.
    pub fn iter(&self) -> std::slice::Iter<'_, Action> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a ActionList {
    type Item = &'a Action;
    type IntoIter = std::slice::Iter<'a, Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership::artifact::ArtifactState;

    fn artifact(state: ArtifactState) -> Artifact {
        Artifact {
            state,
            ..Artifact::new(false, None)
        }
    }

    fn other() -> Action {
        Action::Artifact(Artifact::new(true, None))
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
        if let Some(a) = list.artifact_mut() {
            a.state = ArtifactState::ToGaia;
        }
        assert_eq!(list.make_mut(), None);
        assert_eq!(list.take_make(), None);
        assert_eq!(list.take_artifact(), Some(artifact(ArtifactState::ToGaia)));
        assert!(list.is_empty());
    }

    #[test]
    fn an_action_reads_its_type_and_state() {
        let a = Action::Artifact(artifact(ArtifactState::Check));
        assert_eq!(
            (a.kind(), a.state()),
            (
                crate::ownership::artifact::ARTIFACT_TYPE,
                ActionState::new(6)
            )
        );
        let list = ActionList::default().with_artifact(Some(artifact(ArtifactState::Check)));
        assert_eq!(
            list.of_kind(crate::ownership::artifact::ARTIFACT_TYPE),
            Some(&a)
        );
        assert!(list.holds(crate::ownership::artifact::ARTIFACT_TYPE));
        assert_eq!(list.of_kind(crate::combat::attack::ATTACK), None);
        assert!(!list.holds(crate::combat::attack::ATTACK));
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
