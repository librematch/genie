//! The standard types: numbers by value, floats by bits, options, sequences, maps and tuples.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{Diff, Difference, Scope, differences};

fn found<T: Diff + ?Sized>(left: &T, right: &T) -> Vec<(String, String, String)> {
    differences(left, right, Scope::Saved)
        .into_iter()
        .map(|Difference { path, left, right }| (path, left, right))
        .collect()
}

fn row(path: &str, left: &str, right: &str) -> (String, String, String) {
    (path.to_owned(), left.to_owned(), right.to_owned())
}

/// A value compared at the top has the empty path.
#[test]
fn number_differs_by_its_value() {
    assert_eq!(found(&3_i32, &3), []);
    assert_eq!(found(&3_i32, &4), [row("", "3", "4")]);
    assert_eq!(found(&true, &false), [row("", "true", "false")]);
    assert_eq!(found("a", "b"), [row("", "\"a\"", "\"b\"")]);
}

/// A float compares by its bits: the zeros differ, the same NaN does not, and two NaNs show
/// their bits.
#[test]
fn float_differs_by_its_bits() {
    assert_eq!(found(&0.0_f32, &-0.0), [row("", "0.0", "-0.0")]);
    assert_eq!(found(&f32::NAN, &f32::NAN), []);
    let other = f32::from_bits(f32::NAN.to_bits() | 1);
    assert_eq!(
        found(&f32::NAN, &other),
        [row("", "NaN (0x7fc00000)", "NaN (0x7fc00001)")]
    );
    assert_eq!(found(&1.0_f64, &1.5), [row("", "1.0", "1.5")]);
}

/// Some against None is one difference; two Somes compare their values at the same path.
#[test]
fn an_option_compares_its_value() {
    assert_eq!(found(&Some(1_u8), &Some(2)), [row("", "1", "2")]);
    assert_eq!(found(&Some(1_u8), &None), [row("", "Some(1)", "None")]);
    assert_eq!(found::<Option<u8>>(&None, &None), []);
}

/// A sequence names its length and each element by its index; an element one side lacks is
/// absent there.
#[test]
fn sequence_compares_its_length_and_elements() {
    assert_eq!(
        found(&vec![1_i32, 2, 3], &vec![1, 5]),
        [
            row("len", "3", "2"),
            row("[1]", "2", "5"),
            row("[2]", "3", "absent"),
        ]
    );
    assert_eq!(found(&[1_u8, 2], &[1, 3]), [row("[1]", "2", "3")]);
    let deque = |v: &[u8]| v.iter().copied().collect::<VecDeque<u8>>();
    assert_eq!(
        found(&deque(&[1]), &deque(&[1, 2])),
        [row("len", "1", "2"), row("[1]", "absent", "2")]
    );
    assert_eq!(found(&Box::new(1_u8), &Box::new(2)), [row("", "1", "2")]);
}

/// A map compares by key; a key one side lacks is absent there. A set names the members only
/// one side holds.
#[test]
fn map_compares_by_key() {
    let left = BTreeMap::from([(1_i16, 0.5_f32), (2, 1.0)]);
    let right = BTreeMap::from([(2_i16, 1.5_f32), (3, 0.0)]);
    assert_eq!(
        found(&left, &right),
        [
            row("[1]", "0.5", "absent"),
            row("[2]", "1.0", "1.5"),
            row("[3]", "absent", "0.0"),
        ]
    );
    let left = BTreeSet::from([1_u8, 2]);
    let right = BTreeSet::from([2_u8, 3]);
    assert_eq!(
        found(&left, &right),
        [row("[1]", "1", "absent"), row("[3]", "absent", "3")]
    );
}

/// A tuple's fields are numbered.
#[test]
fn tuple_numbers_its_fields() {
    assert_eq!(
        found(&(1_u8, 2.0_f32, 'a'), &(1, 2.5, 'b')),
        [row("1", "2.0", "2.5"), row("2", "'a'", "'b'")]
    );
}
