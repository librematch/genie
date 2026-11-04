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
        .filter_map(|linked_tech_group| {
            Linked::of(&linked_tech_group.kind).map(|linked| (linked_tech_group, linked))
        })
        .flat_map(|(linked_tech_group, linked)| {
            linked_tech_group.techs.iter().filter_map(move |&tech| {
                i16::try_from(tech)
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
    let int = |index: usize, key: &'static str, value: &serde_json::Value| {
        value
            .as_i64()
            .and_then(|number| i32::try_from(number).ok())
            .ok_or_else(|| unit_line_value(index, key))
    };
    let text = |index: usize, value: &serde_json::Value| {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| unit_line_value(index, "Identifier"))
    };
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let line = entry
                .get("LineID")
                .map(|value| int(index, "LineID", value))
                .transpose()?;
            let chain = entry
                .get("IDChain")
                .map(|value| {
                    value
                        .as_array()
                        .ok_or_else(|| unit_line_value(index, "IDChain"))?
                        .iter()
                        .map(|id| int(index, "IDChain", id))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let building = match entry.get("Building") {
                None => false,
                Some(value) => value
                    .as_bool()
                    .ok_or_else(|| unit_line_value(index, "Building"))?,
            };
            let identifiers = match entry.get("Identifier") {
                None => Vec::new(),
                Some(serde_json::Value::Array(names)) => names
                    .iter()
                    .map(|value| text(index, value))
                    .collect::<Result<Vec<_>, _>>()?,
                Some(name) => vec![text(index, name)?],
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

/// A drop-site target's `object_state` (of entry `index`): none for a negative value, which the
/// task finder reads as any state (`0 <= state`), else the state.
fn object_state(index: usize, number: i64) -> GenieResult<Option<ObjectState>> {
    if number < 0 {
        return Ok(None);
    }
    u8::try_from(number)
        .ok()
        .and_then(|byte| ObjectState::of_byte(byte).ok())
        .map(Some)
        .ok_or_else(|| drop_site_value("a drop site's object state past 8", index, "object_state"))
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
    let num = |value: &serde_json::Value| value.as_i64().unwrap_or(-1);
    let list = |entry: &serde_json::Value, key: &str| -> Vec<i64> {
        entry
            .get(key)
            .and_then(|value| value.as_array())
            .map_or_else(Vec::new, |values| values.iter().map(num).collect())
    };
    let class = |index: usize, key: &'static str, number: i64| {
        i16::try_from(number)
            .ok()
            .and_then(|short| ClassId::try_from(short).ok())
            .ok_or_else(|| {
                drop_site_value("a drop site's class that is no master class", index, key)
            })
    };
    let Some(entries) = json_value["drop_site_list"].as_array() else {
        return Ok(Vec::new());
    };
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let worker_class = match entry.get("worker_object_group").map_or(-1, num) {
                -1 => None,
                number => Some(class(index, "worker_object_group", number)?),
            };
            let targets = entry["target_list"]
                .as_array()
                .map_or_else(Vec::new, Vec::clone)
                .iter()
                .map(|value| {
                    let field = |key: &str| value.get(key).map_or(-1, num);
                    Ok(DropTarget {
                        class: class(index, "object_group", field("object_group"))?,
                        workers: WorkerCount::of_data(field("worker_num") as i32),
                        state: object_state(index, field("object_state"))?,
                        attribute: AttributeId::of_field(field("attribute_type") as i16).map_err(
                            |_| {
                                drop_site_value(
                                    "a drop site's attribute that is no attribute",
                                    index,
                                    "attribute_type",
                                )
                            },
                        )?,
                    })
                })
                .collect::<GenieResult<Vec<_>>>()?;
            Ok(DropSite {
                building: MasterId::try_from(num(&entry["building_id"]) as i16).map_err(|_| {
                    drop_site_value(
                        "a drop site's building that is no master",
                        index,
                        "building_id",
                    )
                })?,
                worker_class,
                workers: list(entry, "worker_object_list")
                    .into_iter()
                    .filter_map(|value| MasterId::of_data(value as i16))
                    .collect(),
                civs: list(entry, "target_civs")
                    .into_iter()
                    .map(|civ| CivId::new(civ as u8))
                    .collect(),
                auto_work: entry.get("auto_work").and_then(serde_json::Value::as_bool)
                    != Some(false),
                targets,
                livestock: entry.get("accepts_livestock").is_some(),
                resource_kinds: entry["update_ai_resource_types"]
                    .as_array()
                    .map_or_else(Vec::new, Vec::clone)
                    .iter()
                    .map(|value| {
                        AttributeId::try_from(value.get("resource_type").map_or(-1, num) as i16)
                            .map_err(|_| {
                                drop_site_value(
                                    "a drop site's attribute that is no attribute",
                                    index,
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
        let reason_and_context = |error: GenieError| {
            let context: Vec<_> = error
                .context()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect();
            (error.reason(), context.join(", "))
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
        let state = state.map_or_else(String::new, |value| format!(r#", "object_state": {value}"#));
        format!(
            r#"{{"drop_site_list": [{{"building_id": 562, "target_list": [{{"object_group": 15{state}}}]}}]}}"#
        )
    }

    /// A target's `object_state` is its state; a missing key or a negative value any state
    /// (none); a value past 8 an error.
    #[test]
    fn target_object_state_reads_as_state_or_none() {
        let state = |value| drop_sites(&table(value)).map(|sites| sites[0].targets[0].state);
        assert_eq!(state(Some(5)).unwrap(), Some(ObjectState::Dead));
        assert_eq!(state(None).unwrap(), None);
        assert_eq!(state(Some(-2)).unwrap(), None);
        assert_eq!(
            state(Some(9)).map_err(|error| error.reason()),
            Err("a drop site's object state past 8")
        );
    }
}
