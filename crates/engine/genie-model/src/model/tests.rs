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
