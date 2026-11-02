//! The action part's reads and writes (FD-045-D59): an object's action layer by id in the
//! world's table ([`Parts::actions`]), an empty one where the table holds none, and the
//! updating object's layer out of the table with its row ([`Parts::take_actions`]).

use crate::GenieError;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::world::parts::{PartTable, Parts};
use std::mem::take;
use std::sync::LazyLock;

/// The action layer of an object the table does not hold: an empty one (a tree's).
static NO_ACTIONS: LazyLock<Action> = LazyLock::new(Action::default);

impl PartTable<Action> {
    /// Object `id`'s action layer: an empty one ([`Action::default`]) where the table holds
    /// none.
    #[must_use]
    pub fn of(&self, id: ObjectId) -> &Action {
        self.get(id).unwrap_or(&NO_ACTIONS)
    }

    /// Object `id`'s action layer to change, an empty one put in where the table holds none,
    /// beside borrows of the world's other part tables (its mover's).
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn of_mut(&mut self, id: ObjectId) -> Result<&mut Action, GenieError> {
        self.get_or_insert_with(id, Action::default)
    }
}

impl Parts {
    /// Object `id`'s action layer: an empty one where the table holds none, and while the
    /// object is out for its update (its layer is out with its row).
    #[must_use]
    pub fn actions_of(&self, id: ObjectId) -> &Action {
        self.actions.of(id)
    }

    /// Object `id`'s action layer to change, an empty one put in where the table holds none.
    /// The caller holds the object: its row is borrowed from the world's lists, which this
    /// borrow leaves alone.
    ///
    /// # Errors
    ///
    /// None (an unreachable error where the table lost the layer it took).
    pub(crate) fn actions_mut(&mut self, id: ObjectId) -> Result<&mut Action, GenieError> {
        self.actions.of_mut(id)
    }

    /// Object `id`'s action layer out of the table for its update, as its row is out of its
    /// list: its slot holds an empty one meanwhile ([`Parts::put_actions`] puts it back).
    pub(crate) fn take_actions(&mut self, id: ObjectId) -> Action {
        self.actions.get_mut(id).map(take).unwrap_or_default()
    }

    /// Object `id`'s action layer `layer` in the table: filed where it differs from an empty
    /// one, else the entry goes.
    pub(crate) fn put_actions(&mut self, id: ObjectId, layer: Action) {
        if layer == Action::default() {
            let _ = self.actions.remove(id);
        } else {
            let _ = self.actions.insert(id, layer);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::layers::Action;
    #[cfg(test)]
    use crate::objects::layers::DefaultTask;
    use crate::test_ids::object_id;
    use crate::world::parts::Parts;

    fn flying() -> Action {
        Action {
            default_task: DefaultTask::Fly,
            ..Action::default()
        }
    }

    #[test]
    fn an_object_the_table_does_not_hold_reads_an_empty_layer() {
        let mut parts = Parts::default();
        assert_eq!(parts.actions_of(object_id(1)), &Action::default());
        parts.actions_mut(object_id(1)).unwrap().default_task = DefaultTask::Make;
        assert_eq!(
            parts.actions_of(object_id(1)).default_task,
            DefaultTask::Make
        );
    }

    #[test]
    fn taken_layer_leaves_an_empty_one_and_comes_back() {
        let mut parts = Parts::default();
        let id = object_id(7);
        parts.put_actions(id, flying());
        assert_eq!(parts.take_actions(id), flying());
        assert_eq!(
            parts.actions_of(id),
            &Action::default(),
            "out for its update"
        );
        assert!(parts.actions.contains(id), "its slot kept");
        parts.put_actions(id, flying());
        assert_eq!(parts.actions_of(id), &flying());
        assert_eq!(
            parts.take_actions(object_id(8)),
            Action::default(),
            "none filed"
        );
        parts.put_actions(id, Action::default());
        assert!(
            !parts.actions.contains(id),
            "an empty layer leaves the table"
        );
    }
}
