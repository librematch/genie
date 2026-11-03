use crate::layer::BuildLayer;
use crate::layer::Roles;
use crate::layer::by_id;
use crate::layer::by_id::table;
use crate::layer::generated::AttributeRoles;
use crate::layer::generated::ClassRoles;
use crate::layer::generated::MasterRoles;
use crate::model::Model;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::players::attribute::AttributeId;
use crate::technology::TechId;
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
    let sets = table(&[(3, Sample::FIRST), (1, Sample::SECOND), (3, Sample::SECOND)]);
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

/// An optional class holds its class's roles; no class holds none.
#[test]
fn optional_classes_hold_their_class_roles() {
    let gate = ClassRoles::from_bits_retain(1);
    let roles = Roles {
        classes: Arc::new(vec![ClassRoles::empty(), gate]),
        ..Roles::default()
    };
    assert_eq!(roles.of_some_class(Some(ClassId::named(1))), gate);
    assert!(roles.of_some_class(None).is_empty());
}

/// An optional master holds its master's roles; no master holds none.
#[test]
fn optional_masters_hold_their_master_roles() {
    let dock = MasterRoles::from_bits_retain(1);
    let roles = Roles {
        masters: Arc::new(vec![MasterRoles::empty(), dock]),
        ..Roles::default()
    };
    assert_eq!(roles.of_some_master(Some(MasterId::named(1))), dock);
    assert!(roles.of_some_master(None).is_empty());
}

/// An optional attribute holds its attribute's roles; no attribute holds none.
#[test]
fn optional_attributes_hold_their_attribute_roles() {
    let food = AttributeRoles::from_bits_retain(1);
    let roles = Roles {
        attributes: Arc::new(vec![food]),
        ..Roles::default()
    };
    assert_eq!(roles.of_some_attribute(Some(AttributeId::named(0))), food);
    assert!(
        roles
            .of_some_attribute(Some(AttributeId::named(1)))
            .is_empty()
    );
    assert!(roles.of_some_attribute(None).is_empty());
}

/// A slot's id serializes as its value and parses through the id's check.
#[test]
fn slot_ids_serialize_as_their_values() {
    assert_eq!(serde_json::to_value(ClassId::named(4)).unwrap(), json!(4));
    assert_eq!(
        serde_json::from_value::<ClassId>(json!(49)).unwrap(),
        ClassId::named(49)
    );
    assert!(serde_json::from_value::<ClassId>(json!(-2)).is_err());
    assert_eq!(
        serde_json::to_value(MasterId::named(50)).unwrap(),
        json!(50)
    );
    assert!(serde_json::from_value::<MasterId>(json!(-1)).is_err());
    assert_eq!(
        serde_json::to_value(TechId::try_from(7).unwrap()).unwrap(),
        json!(7)
    );
    assert_eq!(
        serde_json::to_value(AttributeId::try_from(0).unwrap()).unwrap(),
        json!(0)
    );
}
