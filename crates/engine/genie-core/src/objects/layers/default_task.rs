//! The default task of an object's action layer (FD-045-D48, D60): what its master's default
//! task starts once its action list is empty in state 2 (`RGE_Action_Object::update`,
//! `0x14106ac90`, slot 0x7f0), as the load or the object's constructor found it.

use crate::GenieError;
use crate::gathering::make::MAKE;
use crate::objects::masters::ACTION_FACTORY_KINDS;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use genie_diff::{Diff, Differ};

/// What an object's default task starts once its action list is empty in state 2: one value
/// per object, from its master's default task (the load's, a constructor's).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DefaultTask {
    /// Nothing: no default task, one of a type the action factory makes no action of (a wild
    /// animal's Explore), a saved object's Fly (it never ends) or a Make that started.
    #[default]
    Nothing,
    /// A bird's Fly, which starts with the task its master holds then (before its first
    /// update at a game's start).
    Fly,
    /// A farm's or fish trap's Make, which starts once (its first update as a built building),
    /// with the task its master holds then.
    Make,
    /// Livestock's Artifact, which starts again whenever its action list empties (an animal out
    /// of a building whose Artifact went), its first update in the next tick, as a Fly's.
    Artifact,
    /// A default task the slice cannot start, which fails where the update reaches it.
    Fails(Unstartable),
}

/// A default task the slice cannot start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unstartable {
    /// One of an action type the factory makes that the slice does not start.
    Unported(ActionKind),
    /// One the load could not read.
    Unread(UnreadDefault),
}

/// Why the load could not read an object's default task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnreadDefault {
    /// The object's own master holds a default task: the load reads no own master's tasks.
    OwnMaster,
    /// The load has no master table.
    NoMasterTable,
    /// The object's master is not in the master table.
    NotInTable,
    /// The master's default task id names no task of its list.
    NoSuchTask,
}

impl UnreadDefault {
    /// The reason the start of a default task the load could not read fails with.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::OwnMaster => "the default task of an object's own master",
            Self::NoMasterTable => "no master table: the default task is unknown",
            Self::NotInTable => "a master the table does not have",
            Self::NoSuchTask => "a default task id that names no task",
        }
    }
}

impl Unstartable {
    /// The error its start fails with.
    #[must_use]
    pub fn error(self) -> GenieError {
        let (reason, kind) = match self {
            Self::Unported(kind) => ("a default task the slice does not model", Some(kind)),
            Self::Unread(why) => (why.reason(), None),
        };
        let e = GenieError::unsupported(reason);
        match kind {
            Some(kind) => e.with("action type", kind),
            None => e,
        }
    }
}

/// A comparison of what each value starts, as the save holds it: a Fly only between two runs (a
/// load of a later save holds the Fly itself), the Make always, the Artifact with the known gaps
/// (the load reads the shared master's default task, not an own master's), a start that fails
/// only between two runs.
impl Diff for DefaultTask {
    fn diff(&self, other: &Self, d: &mut Differ) {
        let is = |t: &Self, v: Self| *t == v;
        if d.compares_unsaved() {
            d.field("fly", &is(self, Self::Fly), &is(other, Self::Fly));
        }
        d.field("make", &is(self, Self::Make), &is(other, Self::Make));
        if d.compares_gaps() {
            d.field(
                "artifact",
                &is(self, Self::Artifact),
                &is(other, Self::Artifact),
            );
        }
        if d.compares_unsaved() {
            d.equal_field("fails", &self.fails(), &other.fails());
        }
    }
}

impl DefaultTask {
    /// The start that fails, if it is one.
    #[must_use]
    pub const fn fails(self) -> Option<Unstartable> {
        match self {
            Self::Fails(f) => Some(f),
            Self::Nothing | Self::Fly | Self::Make | Self::Artifact => None,
        }
    }
}

/// What a new building of master `master` starts once its action list is empty in state 2
/// (`RGE_Action_Object::update`, slot 0x7f0): a Make ([`crate::gathering::make`]), or a start
/// the slice cannot make of another type the factory makes; nothing for a type the factory
/// makes no action of.
#[must_use]
pub fn default_start(master: &Master) -> DefaultTask {
    match master.default_task() {
        Some(t) if t.action_kind == MAKE => DefaultTask::Make,
        Some(t) if ACTION_FACTORY_KINDS.contains(&t.action_kind) => {
            DefaultTask::Fails(Unstartable::Unported(t.action_kind))
        }
        _ => DefaultTask::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use genie_diff::{Scope, differences};

    #[test]
    fn start_that_fails_names_why() {
        for t in [
            DefaultTask::Nothing,
            DefaultTask::Fly,
            DefaultTask::Make,
            DefaultTask::Artifact,
        ] {
            assert_eq!(t.fails(), None, "{t:?}");
        }
        let e = Unstartable::Unported(ActionKind::named(6)).error();
        assert_eq!(e.reason(), "a default task the slice does not model");
        let e = Unstartable::Unread(UnreadDefault::OwnMaster).error();
        assert_eq!(e.reason(), "the default task of an object's own master");
    }

    #[test]
    fn comparison_reads_each_start_in_its_scope() {
        let paths = |a, b, scope| {
            differences(&a, &b, scope)
                .into_iter()
                .map(|d| d.path)
                .collect::<Vec<_>>()
        };
        let (fly, none) = (DefaultTask::Fly, DefaultTask::Nothing);
        assert_eq!(paths(fly, none, Scope::Saved), Vec::<String>::new());
        assert_eq!(paths(fly, none, Scope::All), ["fly"]);
        let make = DefaultTask::Make;
        assert_eq!(paths(make, none, Scope::Saved), ["make"]);
        let artifact = DefaultTask::Artifact;
        assert_eq!(paths(artifact, none, Scope::Saved), Vec::<String>::new());
        assert_eq!(paths(artifact, none, Scope::SavedWithGaps), ["artifact"]);
        let read = DefaultTask::Fails(Unstartable::Unread(UnreadDefault::NoSuchTask));
        assert_eq!(
            paths(read, none, Scope::SavedWithGaps),
            Vec::<String>::new()
        );
        assert_eq!(paths(read, none, Scope::All), ["fails"]);
    }
}
