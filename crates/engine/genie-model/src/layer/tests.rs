use crate::layer::BuildLayer;
use crate::layer::Roles;
use crate::layer::by_id;
use crate::model::Model;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use bitflags::bitflags;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

bitflags! {
    /// Two roles of a space the tests stand in for.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct Sample: u8 {
        /// The first.
        const FIRST = 1;
        /// The second.
        const SECOND = 2;
    }
}

/// A role table in its serde form.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
struct Table {
    #[serde(with = "by_id")]
    sets: Arc<Vec<Sample>>,
}

/// A role table holds each id's roles, a later entry adding to an id's, and its serde form
/// names the ids with roles only.
#[test]
fn role_tables_hold_each_ids_roles_and_name_only_ids_with_roles() {
    let sets = by_id::table(&[(3, Sample::FIRST), (1, Sample::SECOND), (3, Sample::SECOND)]);
    assert_eq!(
        sets,
        [
            Sample::empty(),
            Sample::SECOND,
            Sample::empty(),
            Sample::all()
        ]
    );
    let table = Table {
        sets: Arc::new(sets),
    };
    let value = serde_json::to_value(&table).unwrap();
    assert_eq!(
        value,
        json!({ "sets": { "1": ["SECOND"], "3": ["FIRST", "SECOND"] } })
    );
    assert_eq!(serde_json::from_value::<Table>(value).unwrap(), table);
}

/// A role table's id that is no number, or a name no role of the set, fails.
#[test]
fn role_tables_reject_ids_that_are_no_numbers_and_names_of_no_role() {
    assert!(serde_json::from_value::<Table>(json!({ "sets": { "x": ["FIRST"] } })).is_err());
    assert!(serde_json::from_value::<Table>(json!({ "sets": { "1": ["THIRD"] } })).is_err());
}

/// An id beyond a table, and no class, hold no role.
#[test]
fn ids_beyond_the_tables_hold_no_role() {
    let roles = Roles::from_ids(&[], &[], &[], &[]);
    assert!(roles.of_class(ClassId::of_master(99)).is_empty());
    assert!(roles.of_class(ClassId::of_master(-1)).is_empty());
    assert!(roles.of_master(MasterId::named(99)).is_empty());
}

/// A model takes the layer's roles, slots and tuning values; the rest stays.
#[test]
fn models_take_the_layers_roles_slots_and_tuning() {
    let layer = BuildLayer {
        build: 158_041,
        ..BuildLayer::default()
    };
    let model = Model::default().with_layer(&layer);
    assert_eq!(model.roles, layer.roles);
    assert_eq!(model.slots, layer.slots);
    assert_eq!(*model.tuning, layer.tuning);
    assert_eq!(model.masters, Model::default().masters);
}
