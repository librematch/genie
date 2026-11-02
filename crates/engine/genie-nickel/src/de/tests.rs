use crate::de::from_expr;
use crate::evaluate;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

/// A model type with a field of each kind the deserializer reads.
#[derive(Debug, Deserialize, PartialEq)]
struct Sample {
    touch: f32,
    arrived: f32,
    window: f32,
    class: i16,
    roles: Vec<String>,
    kind: Kind,
    share: Option<f64>,
}

/// An enum of unit variants and a newtype variant.
#[derive(Debug, Deserialize, PartialEq)]
enum Kind {
    Gate,
    Wall,
    Tower(u8),
}

/// A record deserializes with each f32 rounded once from its rational: 0.010000001 and 0.01 are
/// neighbouring f32s, a negative number keeps its sign, an enum tag names a unit variant and a
/// variant with a value a newtype variant.
#[test]
fn records_deserialize_with_each_f32_rounded_once() {
    let expr = evaluate(
        "{ touch = 0.010000001, arrived = 0.01, window = -60000, class = 39, roles = [\"gate\"], kind = 'Gate, share = null }",
        "sample.ncl",
    )
    .unwrap();
    let sample: Sample = from_expr(&expr).unwrap();
    assert_eq!(sample.touch.to_bits(), 0x3c23_d70b);
    assert_eq!(sample.arrived.to_bits(), 0x3c23_d70a);
    assert_eq!(sample.window.to_bits(), (-60000.0f32).to_bits());
    assert_eq!(
        (sample.class, &sample.roles[..], &sample.kind),
        (39, &["gate".to_owned()][..], &Kind::Gate)
    );
    assert_eq!(sample.share, None);
    let tower: Kind = from_expr(&evaluate("'Tower 3", "t.ncl").unwrap()).unwrap();
    assert_eq!(tower, Kind::Tower(3));
}

/// An integer beyond its field, a fraction in an integer field and a source that does not
/// evaluate fail, the field's path named.
#[test]
fn values_that_do_not_fit_fail_with_the_field_path() {
    let expr = evaluate(
        "{ touch = 0, arrived = 0, window = 0, class = 40000, roles = [], kind = 'Wall, share = 0.5 }",
        "s.ncl",
    )
    .unwrap();
    let error = from_expr::<Sample>(&expr).unwrap_err();
    assert!(error.to_string().contains("class"), "{error}");
    let expr = evaluate(
        "{ touch = 0, arrived = 0, window = 0, class = 1.5, roles = [], kind = 'Wall, share = 0.5 }",
        "s.ncl",
    )
    .unwrap();
    assert!(
        from_expr::<Sample>(&expr)
            .unwrap_err()
            .to_string()
            .contains("class")
    );
    let expr = evaluate("{ roles = [1] }", "s.ncl").unwrap();
    assert!(
        from_expr::<BTreeMap<String, Vec<String>>>(&expr)
            .unwrap_err()
            .to_string()
            .contains("roles.0")
    );
    let Err(error) = evaluate("{ a = }", "broken.ncl") else {
        panic!("a broken source evaluates");
    };
    assert!(error.to_string().contains("broken.ncl"), "{error}");
}

/// A field of any type takes each kind of value: null, a bool, an integer, a fraction, a
/// string, an enum tag, an array and a record; an enum variant with a value has no such form.
#[test]
fn any_value_deserializes_into_fields_of_any_type() {
    let expr = evaluate(
        "{ a = null, b = true, c = -3, d = 1.5, e = \"x\", f = 'Gate, g = [1, 2], h = { i = 0 } }",
        "any.ncl",
    )
    .unwrap();
    let value: Value = from_expr(&expr).unwrap();
    let expected = json!({ "a": null, "b": true, "c": -3, "d": 1.5, "e": "x", "f": "Gate", "g": [1, 2], "h": { "i": 0 } });
    assert_eq!(value, expected);
    let variant = evaluate("{ v = 'Tower 3 }", "any.ncl").unwrap();
    assert!(
        from_expr::<Value>(&variant)
            .unwrap_err()
            .to_string()
            .contains("no serde shape")
    );
}
