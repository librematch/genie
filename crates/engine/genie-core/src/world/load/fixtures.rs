//! Save parts for the load's tests, built from the generated layouts' defaults: genie holds no
//! game saves (FD-044), so a test builds the few fields its routine reads and leaves the rest
//! zero.

use crate::objects::masters::ActionKind;
use crate::testing::tuning;
use crate::world::layer::generated::MissileTuning;
use genie_save::generated::world::{
    Action, ActionList, ActionListEntry1, Master, PlayerMaster, PlayerObjects,
    RgeMasterMovingObject, RgeMasterStaticObject, TribeBuildingObject,
};
use genie_save::generated::world::{Object, RgeMovingObject, RgeStaticObject, TribeCombatObject};
use std::iter::once;

/// A combat object (type 70), its id `id` and master `master`; its town bell holds no
/// back-to-work action (−1, as the game's saves do).
pub(crate) fn combat_object(id: i32, master: i16) -> Object {
    let mut object = Object {
        tribe_combat_object: Some(Box::new(TribeCombatObject {
            back_to_work1_town_bell_action: -1,
            back_to_work2_town_bell_action: -1,
            ..TribeCombatObject::default()
        })),
        ..Object::default()
    };
    let s = static_mut(&mut object);
    s.id = id;
    s.master_obj = master;
    object
}

/// The combat part of an object of [`combat_object`].
///
/// # Panics
///
/// For an object without one.
pub(crate) fn combat_mut(object: &mut Object) -> &mut TribeCombatObject {
    object
        .tribe_combat_object
        .as_deref_mut()
        .expect("a combat object")
}

/// The moving part of an object of [`combat_object`].
pub(crate) fn moving_mut(object: &mut Object) -> &mut RgeMovingObject {
    &mut combat_mut(object)
        .rge_combat_object
        .rge_action_object
        .rge_moving_object
}

/// The static part of an object of [`combat_object`].
pub(crate) fn static_mut(object: &mut Object) -> &mut RgeStaticObject {
    &mut moving_mut(object).rge_animated_object.rge_static_object
}

/// A combat master (type 70) of id `id`.
pub(crate) fn combat_master(id: i16) -> Master {
    let mut master = Master {
        tribe_master_combat_object: Some(Box::default()),
        ..Master::default()
    };
    // the master's id and its copy's, as the save writes them first
    let s = master_static_mut(&mut master);
    s.read_temp = id;
    s.read_temp_2 = id;
    master
}

/// The moving part of a master of [`combat_master`].
///
/// # Panics
///
/// For a master without a combat part.
pub(crate) fn master_moving_mut(master: &mut Master) -> &mut RgeMasterMovingObject {
    &mut master
        .tribe_master_combat_object
        .as_deref_mut()
        .expect("a combat master")
        .rge_master_combat_object
        .rge_master_action_object
        .rge_master_moving_object
}

/// The static part of a master of [`combat_master`].
pub(crate) fn master_static_mut(master: &mut Master) -> &mut RgeMasterStaticObject {
    &mut master_moving_mut(master)
        .rge_master_animated_object
        .rge_master_static_object
}

/// A building (type 80), its id `id` and master `master`.
pub(crate) fn building_object(id: i32, master: i16) -> Object {
    let mut object = Object {
        tribe_building_object: Some(Box::default()),
        ..Object::default()
    };
    let s = &mut building_mut(&mut object)
        .tribe_combat_object
        .rge_combat_object
        .rge_action_object
        .rge_moving_object
        .rge_animated_object
        .rge_static_object;
    s.id = id;
    s.master_obj = master;
    object
}

/// The building part of an object of [`building_object`].
///
/// # Panics
///
/// For an object without one.
pub(crate) fn building_mut(object: &mut Object) -> &mut TribeBuildingObject {
    object
        .tribe_building_object
        .as_deref_mut()
        .expect("a building")
}

/// An action list of `actions` (type, action), its end after them.
pub(crate) fn action_list(actions: Vec<(ActionKind, Action)>) -> ActionList {
    ActionList {
        action_kind_list: actions
            .into_iter()
            .map(|(action_kind, action)| ActionListEntry1 {
                action_kind: action_kind.get(),
                action: Some(Box::new(action)),
            })
            .chain(once(ActionListEntry1::default()))
            .collect(),
    }
}

/// A player's saved objects with the masters `masters` (each of type 70).
pub(crate) fn with_masters(masters: Vec<Master>) -> PlayerObjects {
    let n = masters.len();
    PlayerObjects {
        // each master enabled, so the save's list holds them all
        master_object_num: i32::try_from(n).expect("a test's few masters"),
        enabled: vec![1; n],
        marker: (n > 0).then_some(()),
        masters: (n > 0).then(|| {
            masters
                .into_iter()
                .map(|master| PlayerMaster {
                    master_kind: 70,
                    master,
                    end_marker: (),
                })
                .collect()
        }),
        ..PlayerObjects::default()
    }
}

/// Build 158041's missile tuning, which the import reads from the build layer.
pub(crate) fn missile_tuning() -> MissileTuning {
    tuning().missile
}
