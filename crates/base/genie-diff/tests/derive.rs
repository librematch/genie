//! The derive: a struct by its fields' names, a tuple struct of one field as its field, an enum
//! by its variant and then the same variant's fields; `#[diff(unsaved)]` and `#[diff(eq)]`.

use genie_diff::{Diff, Difference, Scope, differences};

fn found<T: Diff>(left: &T, right: &T, scope: Scope) -> Vec<(String, String, String)> {
    differences(left, right, scope)
        .into_iter()
        .map(|Difference { path, left, right }| (path, left, right))
        .collect()
}

fn row(path: &str, left: &str, right: &str) -> (String, String, String) {
    (path.to_owned(), left.to_owned(), right.to_owned())
}

/// An id that names an object: a newtype, compared as its value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
struct Id(i32);

/// Something no derive reaches, compared as one value.
#[derive(Clone, Debug, Default, PartialEq)]
struct Opaque {
    why: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Diff)]
struct Life {
    hp: f32,
    state: u8,
}

#[derive(Clone, Debug, PartialEq, Diff)]
enum Action {
    Idle,
    Move { to: [f32; 2], waited: u8 },
    Attack(Id, f32),
    Wrapped(Life),
}

#[derive(Clone, Debug, PartialEq, Diff)]
struct Pair(u8, i16);

#[derive(Clone, Debug, PartialEq, Diff)]
struct Unit {
    id: Id,
    life: Life,
    actions: Vec<Action>,
    /// Not in the game's save: a run keeps it, a load starts it at 0.
    #[diff(unsaved)]
    waits: u8,
    #[diff(eq)]
    why: Option<Opaque>,
    #[diff(unsaved, eq)]
    cache: Opaque,
    /// A known difference genie does not model yet.
    #[diff(gap = "not modelled yet")]
    sprite: Option<u16>,
    pair: Pair,
    r#type: i16,
}

fn unit() -> Unit {
    Unit {
        id: Id(7),
        life: Life { hp: 25.0, state: 2 },
        actions: vec![Action::Move {
            to: [1.5, 2.0],
            waited: 0,
        }],
        waits: 0,
        why: None,
        cache: Opaque::default(),
        sprite: None,
        pair: Pair(1, 2),
        r#type: 70,
    }
}

/// Equal values have no difference, in either scope.
#[test]
fn equal_values_do_not_differ() {
    assert_eq!(found(&unit(), &unit(), Scope::Saved), []);
    assert_eq!(found(&unit(), &unit(), Scope::All), []);
}

/// A field is named by its path from the value compared, each float in full.
#[test]
fn nested_field_is_named_by_its_path() {
    let mut other = unit();
    other.life.hp = 24.999_998;
    other.actions[0] = Action::Move {
        to: [1.5, 2.25],
        waited: 3,
    };
    other.r#type = 71;
    assert_eq!(
        found(&unit(), &other, Scope::Saved),
        [
            row("life.hp", "25.0", "24.999998"),
            row("actions[0].to[1]", "2.0", "2.25"),
            row("actions[0].waited", "0", "3"),
            row("type", "70", "71"),
        ]
    );
}

/// A newtype is its value; a tuple struct of more fields names them by position.
#[test]
fn newtype_is_its_value_and_tuple_struct_fields_are_numbered() {
    let mut other = unit();
    other.id = Id(8);
    other.pair = Pair(1, 3);
    assert_eq!(
        found(&unit(), &other, Scope::Saved),
        [row("id", "7", "8"), row("pair.1", "2", "3")]
    );
}

/// Two variants differ by their names; the same variant compares its fields, a variant of one
/// field as that field.
#[test]
fn an_enum_differs_by_its_variant_then_by_the_variant_fields() {
    let mut other = unit();
    other.actions[0] = Action::Idle;
    assert_eq!(
        found(&unit(), &other, Scope::Saved),
        [row("actions[0]", "Move", "Idle")]
    );
    let attack = |id, at| Unit {
        actions: vec![Action::Attack(Id(id), at), Action::Wrapped(Life::default())],
        ..unit()
    };
    let mut wrapped = attack(5, 0.5);
    wrapped.actions[1] = Action::Wrapped(Life { hp: 1.0, state: 0 });
    assert_eq!(
        found(&attack(4, 0.5), &wrapped, Scope::Saved),
        [
            row("actions[0].0", "4", "5"),
            row("actions[1].hp", "0.0", "1.0"),
        ]
    );
}

/// A gap counts only where the gaps do; an unsaved gap only in every field's scope.
#[test]
fn gap_counts_only_with_the_gaps() {
    let mut other = unit();
    other.sprite = Some(7);
    assert_eq!(found(&unit(), &other, Scope::Saved), []);
    let gap = [row("sprite", "None", "Some(7)")];
    assert_eq!(found(&unit(), &other, Scope::SavedWithGaps), gap);
    assert_eq!(found(&unit(), &other, Scope::All), gap);
}

/// An unsaved field counts only where every field does.
#[test]
fn an_unsaved_field_counts_only_in_scope_all() {
    let mut other = unit();
    other.waits = 4;
    other.cache = Opaque { why: "kept" };
    assert_eq!(found(&unit(), &other, Scope::Saved), []);
    assert_eq!(found(&unit(), &other, Scope::SavedWithGaps), []);
    assert_eq!(
        found(&unit(), &other, Scope::All),
        [
            row("waits", "0", "4"),
            row("cache", "Opaque { why: \"\" }", "Opaque { why: \"kept\" }"),
        ]
    );
}

/// A field compared by its equality is one value: its whole text where it differs.
#[test]
fn an_eq_field_is_one_value() {
    let mut other = unit();
    other.why = Some(Opaque { why: "no" });
    assert_eq!(
        found(&unit(), &other, Scope::Saved),
        [row("why", "None", "Some(Opaque { why: \"no\" })")]
    );
}

/// A generic type takes its parameters' differences.
#[test]
fn generic_kind_compares_its_parameter() {
    #[derive(Debug, Diff)]
    struct Held<T> {
        value: T,
    }
    let a = Held { value: Id(1) };
    let b = Held { value: Id(2) };
    assert_eq!(found(&a, &b, Scope::Saved), [row("value", "1", "2")]);
}

/// An object's parts that moved off its row, each at the place it had.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
struct Parts {
    #[diff(path = "building.gather_point")]
    gather_point: Option<u8>,
    #[diff(path = "combat.ai", gap = "a test's gap")]
    ai: u8,
    #[diff(path = "building.queue", eq)]
    queue: Vec<u8>,
}

/// A field with a path takes that place in the path, whatever else it is marked.
#[test]
fn field_with_path_takes_that_place() {
    let other = Parts {
        gather_point: Some(2),
        ai: 1,
        queue: vec![3],
    };
    let saved = [
        row("building.gather_point", "None", "Some(2)"),
        row("building.queue", "[]", "[3]"),
    ];
    assert_eq!(found(&Parts::default(), &other, Scope::Saved), saved);
    assert_eq!(
        found(&Parts::default(), &other, Scope::SavedWithGaps),
        [
            saved[0].clone(),
            row("combat.ai", "0", "1"),
            saved[1].clone()
        ]
    );
}
