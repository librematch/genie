use crate::errors::no_such_player;
use crate::test_ids::object_id;
use std::num::NonZeroUsize;

use genie_core::objects::unit::Unit;
use genie_core::world::parts::Object;
use genie_diff::Scope;
use genie_rec::de::Op;

use crate::fixtures::{combat_object, gaia, header, human, recording};
use crate::survey::{field_key, header_survey};
use crate::{ONLY_IN_THE_GAME, object_field_differences_in};
use genie_core::combat::AttackCount;
use genie_core::objects::layers::Combat;
use genie_core::world::parts::ObjectParts;

fn every(count: usize) -> NonZeroUsize {
    NonZeroUsize::new(count).unwrap()
}

/// A field's path without its indices and keys, nested ones too.
#[test]
fn field_key_drops_its_indices() {
    assert_eq!(field_key("life.hp"), "life.hp");
    assert_eq!(field_key("actions[0].to[12]"), "actions[].to[]");
    assert_eq!(field_key("costs[[1, 2]].amount"), "costs[].amount");
    assert_eq!(field_key(ONLY_IN_THE_GAME), ONLY_IN_THE_GAME);
}

/// Every field that differs, counted over every compared save of the window: the saves of
/// `first` and each `every`-th update after it, a missing one skipped.
#[test]
fn survey_counts_each_field_over_the_window_saves() {
    let quiet = header(vec![gaia(Vec::new()), human(Vec::new())]);
    let game = header(vec![gaia(Vec::new()), human(vec![combat_object(7, 83)])]);
    let rec = recording(
        &quiet,
        &(0..5).map(|_| Op::Sync { dt: 50 }).collect::<Vec<_>>(),
    );
    let mut games = |update: usize| (update != 4).then(|| recording(&game, &[]));
    let survey =
        header_survey(&rec, None, &[1], (2, 5), every(1), Scope::Saved, &mut games).unwrap();
    assert_eq!(survey.saves, 3);
    assert_eq!(survey.stops, []);
    let [row] = survey.rows.as_slice() else {
        panic!("one field differs: {:?}", survey.rows);
    };
    assert_eq!(
        (row.field.as_str(), row.count, row.first),
        (ONLY_IN_THE_GAME, 3, 2)
    );
    assert_eq!((row.player, row.example.id.get()), (1, 7));
    let mut games = |_: usize| Some(recording(&game, &[]));
    let sparse =
        header_survey(&rec, None, &[1], (1, 5), every(2), Scope::Saved, &mut games).unwrap();
    assert_eq!(
        (sparse.saves, sparse.rows[0].count, sparse.rows[0].first),
        (3, 3, 1)
    );
    let missing = header_survey(&rec, None, &[2], (1, 5), every(1), Scope::Saved, &mut games);
    assert!(missing.is_err_and(|error| error == no_such_player(2)));
}

/// A player that stops is listed with its stop and compared no more; the survey ends once
/// every player has.
#[test]
fn stopped_player_is_listed_and_compared_no_more() {
    let save = header(vec![gaia(Vec::new()), human(vec![combat_object(7, 83)])]);
    let rec = recording(&save, &[Op::Sync { dt: 50 }, Op::Sync { dt: 50 }]);
    let mut never = |_: usize| -> Option<Vec<u8>> { panic!("no save is read after the stop") };
    let survey =
        header_survey(&rec, None, &[1], (1, 2), every(1), Scope::Saved, &mut never).unwrap();
    let [(1, stop)] = survey.stops.as_slice() else {
        panic!("player 1 stops: {:?}", survey.stops);
    };
    assert_eq!(stop.update, 0);
    assert_eq!((survey.saves, survey.rows.len()), (0, 0));
}

/// The saved fields alone; with the gaps the fields genie models apart from the game; all adds
/// the unsaved ones.
#[test]
fn the_scope_widens_the_compared_fields() {
    let object = |attack_count, idle_variant| {
        // the layer's crate-private columns leave no struct update to another crate
        let mut combat = Combat::default();
        combat.attack_count = AttackCount::of_data(attack_count);
        Object {
            unit: Unit {
                id: object_id(1),
                idle_variant,
                ..Unit::default()
            },
            parts: ObjectParts {
                combat,
                ..Default::default()
            },
        }
    };
    let (slice, game) = ([object(1, 1)], [object(0, 0)]);
    let fields = |scope| -> Vec<String> {
        object_field_differences_in(&slice, &game, scope)
            .into_iter()
            .map(|difference| difference.field)
            .collect()
    };
    assert_eq!(fields(Scope::Saved), Vec::<String>::new());
    assert_eq!(fields(Scope::SavedWithGaps), ["combat.attack_count"]);
    // the row's fields first, then its parts'
    assert_eq!(fields(Scope::All), ["idle_variant", "combat.attack_count"]);
}
