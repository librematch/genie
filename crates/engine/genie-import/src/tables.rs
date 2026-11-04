//! The game's own tables beside the `.dat`: the linked technologies, the unit lines and the
//! drop sites.

use genie_model::objects::ObjectState;
use genie_model::objects::WorkerCount;
use genie_model::objects::lines::{LineId, UnitLine};
use genie_model::players::CivId;
use genie_model::players::attribute::AttributeId;
use genie_model::technology::Linked;
use genie_model::technology::TechId;
use genie_model::unit_ai::task_finder::{DropSite, DropTarget};
use genie_model::{GenieError, GenieResult};
use std::collections::BTreeMap;

/// The game's linked technologies (`resources/_common/dat/linkedTechs.json`): each group's
/// technologies with the group's kind. A technology of a kind the game does not know, or out of
/// the ids' range, is left out.
///
/// # Errors
///
/// A file that is not JSON, or not the table's shape.
pub fn linked_techs(json: &str) -> GenieResult<BTreeMap<TechId, Linked>> {
    let file: genie_json::LinkedTechs = serde_json::from_str(json).map_err(not_json)?;
    Ok(file
        .linked_techs
        .iter()
        .filter_map(|g| Linked::of(&g.kind).map(|k| (g, k)))
        .flat_map(|(linked_tech_group, linked)| {
            linked_tech_group.techs.iter().filter_map(move |&t| {
                i16::try_from(t)
                    .ok()
                    .and_then(TechId::of_data)
                    .map(|tech_id| (tech_id, linked))
            })
        })
        .collect())
}

/// A table that is not JSON.
fn not_json(error: serde_json::Error) -> GenieError {
    GenieError::invalid_input("a table that is not JSON").with_source(error)
}

/// A unit line's value of another kind than the game reads: the entry's index and the key.
#[track_caller]
fn unit_line_value(entry: usize, key: &'static str) -> GenieError {
    GenieError::invalid_input("a unit line value of another kind than the game reads")
        .with("entry", entry)
        .with("key", key)
}

/// The game's unit lines (`resources/_common/dat/unitlines.json`) as `FUN_140fde680` reads each
/// entry of `UnitLines`: `LineID` (an int), `IDChain` (an array of ints), `Building` (a bool,
/// false when absent) and `Identifier` (a string or an array of strings). A key an entry leaves
/// out stays out ([`genie_model::objects::lines::UnitLines::build`] skips the entry); a file without
/// `UnitLines` holds none.
///
/// # Errors
///
/// A file that is not JSON, or a value of another kind than the game reads.
pub fn unit_lines(json: &str) -> GenieResult<Vec<UnitLine>> {
    let json_value: serde_json::Value = serde_json::from_str(json).map_err(not_json)?;
    let Some(entries) = json_value
        .get("UnitLines")
        .and_then(serde_json::Value::as_array)
    else {
        return Ok(Vec::new());
    };
    let int = |i: usize, key: &'static str, v: &serde_json::Value| {
        v.as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| unit_line_value(i, key))
    };
    let text = |i: usize, v: &serde_json::Value| {
        v.as_str()
            .map(str::to_owned)
            .ok_or_else(|| unit_line_value(i, "Identifier"))
    };
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let line = e.get("LineID").map(|v| int(i, "LineID", v)).transpose()?;
            let chain = e
                .get("IDChain")
                .map(|v| {
                    v.as_array()
                        .ok_or_else(|| unit_line_value(i, "IDChain"))?
                        .iter()
                        .map(|id| int(i, "IDChain", id))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let building = match e.get("Building") {
                None => false,
                Some(b) => b.as_bool().ok_or_else(|| unit_line_value(i, "Building"))?,
            };
            let identifiers = match e.get("Identifier") {
                None => Vec::new(),
                Some(serde_json::Value::Array(names)) => names
                    .iter()
                    .map(|value| text(i, value))
                    .collect::<Result<Vec<_>, _>>()?,
                Some(name) => vec![text(i, name)?],
            };
            Ok(UnitLine {
                line: line.map(LineId::of_data),
                chain,
                building,
                identifiers,
            })
        })
        .collect()
}

/// A drop site's value that is not what its key names (a class, a master, an attribute, an
/// object state): `reason`, the entry's index and the key.
#[track_caller]
fn drop_site_value(reason: &'static str, entry: usize, key: &'static str) -> GenieError {
    GenieError::invalid_input(reason)
        .with("entry", entry)
        .with("key", key)
}

/// A drop-site target's `object_state` (of entry `i`): none for a negative value, which the task
/// finder reads as any state (`0 <= state`), else the state.
fn object_state(i: usize, n: i64) -> GenieResult<Option<ObjectState>> {
    if n < 0 {
        return Ok(None);
    }
    u8::try_from(n)
        .ok()
        .and_then(|n| ObjectState::of_byte(n).ok())
        .map(Some)
        .ok_or_else(|| drop_site_value("a drop site's object state past 8", i, "object_state"))
}

/// The game's drop-site table (`resources/_common/dat/dropsites.json`): each entry's
/// `building_id`, `worker_object_group` (−1 when absent), `worker_object_list`, `target_civs`,
/// its targets' `attribute_type`s and its `update_ai_resource_types`, in file order.
///
/// # Errors
///
/// A file that is not JSON, a target's `object_group` or an entry's `worker_object_group`
/// that is neither a class nor (the worker group only) -1, or a target's `object_state` past 8.
pub fn drop_sites(json: &str) -> GenieResult<Vec<DropSite>> {
    use genie_model::objects::masters::{ClassId, MasterId};
    let json_value: serde_json::Value = serde_json::from_str(json).map_err(not_json)?;
    let num = |v: &serde_json::Value| v.as_i64().unwrap_or(-1);
    let list = |e: &serde_json::Value, key: &str| -> Vec<i64> {
        e.get(key)
            .and_then(|l| l.as_array())
            .map_or_else(Vec::new, |values| values.iter().map(num).collect())
    };
    let class = |i: usize, key: &'static str, n: i64| {
        i16::try_from(n)
            .ok()
            .and_then(|n| ClassId::try_from(n).ok())
            .ok_or_else(|| drop_site_value("a drop site's class that is no master class", i, key))
    };
    let Some(entries) = json_value["drop_site_list"].as_array() else {
        return Ok(Vec::new());
    };
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let worker_class = match e.get("worker_object_group").map_or(-1, num) {
                -1 => None,
                n => Some(class(i, "worker_object_group", n)?),
            };
            let targets = e["target_list"]
                .as_array()
                .map_or_else(Vec::new, Vec::clone)
                .iter()
                .map(|t| {
                    let field = |key: &str| t.get(key).map_or(-1, num);
                    Ok(DropTarget {
                        class: class(i, "object_group", field("object_group"))?,
                        workers: WorkerCount::of_data(field("worker_num") as i32),
                        state: object_state(i, field("object_state"))?,
                        attribute: AttributeId::of_field(field("attribute_type") as i16).map_err(
                            |_| {
                                drop_site_value(
                                    "a drop site's attribute that is no attribute",
                                    i,
                                    "attribute_type",
                                )
                            },
                        )?,
                    })
                })
                .collect::<GenieResult<Vec<_>>>()?;
            Ok(DropSite {
                building: MasterId::try_from(num(&e["building_id"]) as i16).map_err(|_| {
                    drop_site_value("a drop site's building that is no master", i, "building_id")
                })?,
                worker_class,
                workers: list(e, "worker_object_list")
                    .into_iter()
                    .filter_map(|w| MasterId::of_data(w as i16))
                    .collect(),
                civs: list(e, "target_civs")
                    .into_iter()
                    .map(|c| CivId::new(c as u8))
                    .collect(),
                auto_work: e.get("auto_work").and_then(serde_json::Value::as_bool) != Some(false),
                targets,
                livestock: e.get("accepts_livestock").is_some(),
                resource_kinds: e["update_ai_resource_types"]
                    .as_array()
                    .map_or_else(Vec::new, Vec::clone)
                    .iter()
                    .map(|value| {
                        AttributeId::try_from(value.get("resource_type").map_or(-1, num) as i16)
                            .map_err(|_| {
                                drop_site_value(
                                    "a drop site's attribute that is no attribute",
                                    i,
                                    "resource_type",
                                )
                            })
                    })
                    .collect::<GenieResult<Vec<_>>>()?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::tables::{drop_sites, unit_lines};
    use genie_model::GenieError;
    use genie_model::objects::ObjectState;
    use std::error::Error as StdError;

    /// Each error names its reason, the entry and the key it found out of shape; a file that
    /// is not JSON keeps the parser's error as its source.
    #[test]
    fn table_errors_name_their_entry_and_key() {
        let reason_and_context = |e: GenieError| {
            let context: Vec<_> = e
                .context()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect();
            (e.reason(), context.join(", "))
        };
        let error = drop_sites("[").unwrap_err();
        assert_eq!(error.reason(), "a table that is not JSON");
        assert!(StdError::source(&error).is_some());
        let site = |target: &str| {
            format!(r#"{{"drop_site_list": [{{"building_id": 562, "target_list": [{target}]}}]}}"#)
        };
        assert_eq!(
            reason_and_context(drop_sites(&site(r#"{"object_group": -5}"#)).unwrap_err()),
            (
                "a drop site's class that is no master class",
                "entry: 0, key: object_group".to_owned()
            )
        );
        assert_eq!(
            reason_and_context(
                drop_sites(&site(r#"{"object_group": 15, "attribute_type": -5}"#)).unwrap_err()
            ),
            (
                "a drop site's attribute that is no attribute",
                "entry: 0, key: attribute_type".to_owned()
            )
        );
        assert_eq!(
            reason_and_context(unit_lines(r#"{"UnitLines": [{}, {"LineID": "x"}]}"#).unwrap_err()),
            (
                "a unit line value of another kind than the game reads",
                "entry: 1, key: LineID".to_owned()
            )
        );
    }

    /// A table of one entry whose target has `object_state` `state` (absent for `None`).
    fn table(state: Option<i64>) -> String {
        let state = state.map_or_else(String::new, |s| format!(r#", "object_state": {s}"#));
        format!(
            r#"{{"drop_site_list": [{{"building_id": 562, "target_list": [{{"object_group": 15{state}}}]}}]}}"#
        )
    }

    /// A target's `object_state` is its state; a missing key or a negative value any state
    /// (none); a value past 8 an error.
    #[test]
    fn target_object_state_reads_as_state_or_none() {
        let state = |s| drop_sites(&table(s)).map(|sites| sites[0].targets[0].state);
        assert_eq!(state(Some(5)).unwrap(), Some(ObjectState::Dead));
        assert_eq!(state(None).unwrap(), None);
        assert_eq!(state(Some(-2)).unwrap(), None);
        assert_eq!(
            state(Some(9)).map_err(|error| error.reason()),
            Err("a drop site's object state past 8")
        );
    }
}
