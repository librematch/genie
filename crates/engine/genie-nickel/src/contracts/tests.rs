use crate::contracts::Outcome;
use crate::contracts::contracts;
use crate::contracts::schema_hash;
use crate::contracts::write_or_check;
use crate::de::from_expr;
use crate::evaluate;
use genie_model::model::Model;
use schemars::JsonSchema;
use schemars::schema_for;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::process;

/// A model type with a field of each kind the contracts write.
#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
struct Gate {
    /// The class.
    class: i16,
    opens: Option<bool>,
    kind: Side,
    speed: f32,
    names: Vec<String>,
    by_name: BTreeMap<String, u8>,
    tower: Tower,
    id: Id,
}

/// A side.
#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
enum Side {
    Left,
    Right,
}

/// An enum with a variant that holds a value.
#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
enum Tower {
    Empty,
    Level(u8),
}

/// A newtype of an id.
#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
struct Id(u16);

/// A value of every field.
const GOOD: &str = "{ class = 39, kind = 'Left, speed = 1.5, names = [\"a\"], by_name = { a = 3 }, tower = 'Level 2, id = 7 }";

/// Whether `value` passes the contract `name` of the contracts written for `Gate`.
fn passes(value: &str, name: &str) -> bool {
    let text = contracts(&schema_for!(Gate)).unwrap();
    evaluate(
        &format!("let C = {text} in ({value} | C.{name})"),
        "gate.ncl",
    )
    .is_ok()
}

#[test]
fn structs_become_record_contracts_and_enums_tag_contracts() {
    let text = contracts(&schema_for!(Gate)).unwrap();
    assert!(text.contains("Gate | doc \"A model type"), "{text}");
    assert!(
        text.contains("class | std.contract.from_predicate"),
        "{text}"
    );
    assert!(text.contains("opens | Bool | optional"), "{text}");
    assert!(
        text.contains("Side | doc \"A side.\" = [| 'Left, 'Right |]"),
        "{text}"
    );
    assert!(
        text.contains("by_name | { _ | std.contract.from_predicate"),
        "{text}"
    );
}

#[test]
fn the_contracts_accept_values_of_the_kind_and_reject_values_that_break_it() {
    assert!(passes(GOOD, "Gate"));
    let other = "{ class = 39, opens = true, kind = 'Right, speed = 0, names = [], by_name = {}, tower = 'Empty, id = 0 }";
    assert!(passes(other, "Gate"));
    // a class beyond i16, a tag no variant names, a fraction in an integer, a missing field and
    // a variant's value beyond its type
    for broken in [
        GOOD.replace("class = 39", "class = 40000"),
        GOOD.replace("'Left", "'Up"),
        GOOD.replace("id = 7", "id = 7.5"),
        GOOD.replace("speed = 1.5, ", ""),
        GOOD.replace("'Level 2", "'Level 300"),
    ] {
        assert!(!passes(&broken, "Gate"), "{broken}");
    }
}

#[test]
fn values_the_contracts_accept_deserialize_into_the_kind() {
    let gate: Gate = from_expr(&evaluate(GOOD, "gate.ncl").unwrap()).unwrap();
    let expected = Gate {
        class: 39,
        opens: None,
        kind: Side::Left,
        speed: 1.5,
        names: vec!["a".to_owned()],
        by_name: BTreeMap::from([("a".to_owned(), 3)]),
        tower: Tower::Level(2),
        id: Id(7),
    };
    assert_eq!(gate, expected);
    let right: Side = from_expr(&evaluate("'Right", "side.ncl").unwrap()).unwrap();
    let empty: Tower = from_expr(&evaluate("'Empty", "tower.ncl").unwrap()).unwrap();
    assert_eq!((right, empty), (Side::Right, Tower::Empty));
}

#[test]
fn the_schema_hash_is_the_texts_sha256() {
    let hash = schema_hash("");
    assert_eq!(hash[..4], [0xe3, 0xb0, 0xc4, 0x42]);
    assert_ne!(schema_hash("a"), hash);
}

/// The model's contracts, as `just contracts` writes them, parse as one record of a contract
/// per definition, and a value passes one of them (a record of three distances).
#[test]
fn the_models_contracts_parse_as_one_record_and_check_values() {
    let text = contracts(&schema_for!(Model)).unwrap();
    let fields = evaluate(
        &format!("let C = {text} in std.record.fields C"),
        "genie.ncl",
    );
    let fields = fields.map(|expr| expr.as_array().map(|array| array.len()));
    assert!(
        matches!(fields, Ok(Some(n)) if n > 100),
        "{:?}",
        fields.err().map(|error| error.to_string())
    );
    let spread = format!("let C = {text} in ({{ x = 1, y = 2.5, start = 0 }} | C.VolleySpread)");
    assert!(evaluate(&spread, "genie.ncl").is_ok());
    let broken =
        format!("let C = {text} in ({{ x = 1, y = \"far\", start = 0 }} | C.VolleySpread)");
    assert!(evaluate(&broken, "genie.ncl").is_err());
}

/// The contracts are written into a new folder; the check finds them current, and stale once
/// the file differs.
#[test]
fn written_contracts_check_current_until_the_file_differs() {
    let folder = env::temp_dir().join(format!("genie-contracts-{}", process::id()));
    let path = folder.join("contracts").join("genie.ncl");
    assert_eq!(
        write_or_check(&path, "{ A = Number }\n", false).unwrap(),
        Outcome::Written
    );
    assert_eq!(
        write_or_check(&path, "{ A = Number }\n", true).unwrap(),
        Outcome::Current
    );
    assert_eq!(
        write_or_check(&path, "{ A = String }\n", true).unwrap(),
        Outcome::Stale
    );
    fs::remove_dir_all(&folder).unwrap();
    assert_eq!(write_or_check(&path, "", true).unwrap(), Outcome::Stale);
}
