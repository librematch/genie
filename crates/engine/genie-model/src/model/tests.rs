use crate::model::Model;
use std::sync::Arc;

/// A cloned model shares each table with its stock until it writes one: the written table is
/// its own, the others stay shared.
#[test]
fn cloned_models_share_their_tables_until_one_is_written() {
    let stock = Model::default();
    let mut game = stock.clone();
    assert!(Arc::ptr_eq(&stock.techs, &game.techs));
    Arc::make_mut(&mut game.techs).clear();
    assert!(!Arc::ptr_eq(&stock.techs, &game.techs));
    assert!(Arc::ptr_eq(&stock.graphics, &game.graphics));
}

/// A model reads back from its serde form (FD-046-D18): its masters with each type's layers
/// (the class −1 of a master without one too), its technologies and effects.
#[test]
fn models_read_back_from_their_serde_form() {
    use crate::objects::masters::{ClassId, Master, MasterId, MasterKind, Masters};
    use crate::technology::{Effect, EffectCommand, Effects};
    let master = |kind: MasterKind, class: i16| Master {
        id: MasterId::named(kind.get().into()),
        class: ClassId::of_master(class),
        ..Master::of_kind(kind)
    };
    let model = Model {
        masters: vec![Arc::new(Masters::new([
            master(MasterKind::STATIC, -1),
            master(MasterKind::MISSILE, 6),
            master(MasterKind::BUILDING, 3),
        ]))],
        effects: Arc::new(Effects::new(vec![Effect {
            commands: vec![EffectCommand::of_data(1, 3, 1, -1, 20.0).unwrap()],
        }])),
        ..Model::default()
    };
    let text = serde_json::to_string(&model).unwrap();
    assert_eq!(serde_json::from_str::<Model>(&text).unwrap(), model);
}
