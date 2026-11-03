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
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Error as FmtError, Formatter};

/// The game's linked technologies (`resources/_common/dat/linkedTechs.json`): each group's
/// technologies with the group's kind. A technology of a kind the game does not know, or out of
/// the ids' range, is left out.
pub fn linked_techs(json: &str) -> Result<BTreeMap<TechId, Linked>, serde_json::Error> {
    let file: genie_json::LinkedTechs = serde_json::from_str(json)?;
    Ok(file
        .linked_techs
        .iter()
        .filter_map(|g| Linked::of(&g.kind).map(|k| (g, k)))
        .flat_map(|(g, k)| {
            g.techs.iter().filter_map(move |&t| {
                i16::try_from(t)
                    .ok()
                    .and_then(TechId::of_data)
                    .map(|t| (t, k))
            })
        })
        .collect())
}

/// What makes `unitlines.json` out of shape for [`unit_lines`].
#[derive(Debug)]
pub enum UnitLinesError {
    /// The file is not JSON.
    Json(serde_json::Error),
    /// A value of a kind the builder does not read: the entry's index and the key.
    Value(usize, &'static str),
}

impl Display for UnitLinesError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        match self {
            Self::Json(e) => write!(f, "{e}"),
            Self::Value(i, key) => write!(f, "UnitLines[{i}]: {key} is not what the game reads"),
        }
    }
}

impl Error for UnitLinesError {}

/// The game's unit lines (`resources/_common/dat/unitlines.json`) as `FUN_140fde680` reads each
/// entry of `UnitLines`: `LineID` (an int), `IDChain` (an array of ints), `Building` (a bool,
/// false when absent) and `Identifier` (a string or an array of strings). A key an entry leaves
/// out stays out ([`genie_model::objects::lines::UnitLines::build`] skips the entry); a file without
/// `UnitLines` holds none.
///
/// # Errors
///
/// A file that is not JSON, or a value of another kind than the game reads.
pub fn unit_lines(json: &str) -> Result<Vec<UnitLine>, UnitLinesError> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(UnitLinesError::Json)?;
    let Some(entries) = v.get("UnitLines").and_then(serde_json::Value::as_array) else {
        return Ok(Vec::new());
    };
    let int = |i: usize, key: &'static str, v: &serde_json::Value| {
        v.as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or(UnitLinesError::Value(i, key))
    };
    let text = |i: usize, v: &serde_json::Value| {
        v.as_str()
            .map(str::to_owned)
            .ok_or(UnitLinesError::Value(i, "Identifier"))
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
                        .ok_or(UnitLinesError::Value(i, "IDChain"))?
                        .iter()
                        .map(|id| int(i, "IDChain", id))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let building = match e.get("Building") {
                None => false,
                Some(b) => b.as_bool().ok_or(UnitLinesError::Value(i, "Building"))?,
            };
            let identifiers = match e.get("Identifier") {
                None => Vec::new(),
                Some(serde_json::Value::Array(names)) => names
                    .iter()
                    .map(|n| text(i, n))
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

/// What makes `dropsites.json` out of shape for [`drop_sites`].
#[derive(Debug)]
pub enum DropSitesError {
    /// The file is not JSON.
    Json(serde_json::Error),
    /// A class value that is no master class: the entry's index and the key.
    Class(usize, &'static str),
    /// A `building_id` that is no master: the entry's index.
    Building(usize),
    /// An attribute value that is no attribute: the entry's index and the key.
    Attribute(usize, &'static str),
    /// An `object_state` past 8: the entry's index.
    State(usize),
}

impl Display for DropSitesError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        match self {
            Self::Json(e) => write!(f, "{e}"),
            Self::Class(i, key) => write!(f, "drop_site_list[{i}]: {key} is no master class"),
            Self::Building(i) => write!(f, "drop_site_list[{i}]: building_id is no master"),
            Self::Attribute(i, key) => write!(f, "drop_site_list[{i}]: {key} is no attribute"),
            Self::State(i) => write!(f, "drop_site_list[{i}]: object_state is no object state"),
        }
    }
}

impl Error for DropSitesError {}

/// A drop-site target's `object_state` (of entry `i`): none for a negative value, which the task
/// finder reads as any state (`0 <= state`), else the state.
fn object_state(i: usize, n: i64) -> Result<Option<ObjectState>, DropSitesError> {
    if n < 0 {
        return Ok(None);
    }
    u8::try_from(n)
        .ok()
        .and_then(|n| ObjectState::of_byte(n).ok())
        .map(Some)
        .ok_or(DropSitesError::State(i))
}

/// The game's drop-site table (`resources/_common/dat/dropsites.json`): each entry's
/// `building_id`, `worker_object_group` (−1 when absent), `worker_object_list`, `target_civs`,
/// its targets' `attribute_type`s and its `update_ai_resource_types`, in file order.
///
/// # Errors
///
/// A file that is not JSON, a target's `object_group` or an entry's `worker_object_group`
/// that is neither a class nor (the worker group only) -1, or a target's `object_state` past 8.
pub fn drop_sites(json: &str) -> Result<Vec<DropSite>, DropSitesError> {
    use genie_model::objects::masters::{ClassId, MasterId};
    let v: serde_json::Value = serde_json::from_str(json).map_err(DropSitesError::Json)?;
    let num = |v: &serde_json::Value| v.as_i64().unwrap_or(-1);
    let list = |e: &serde_json::Value, key: &str| -> Vec<i64> {
        e.get(key)
            .and_then(|l| l.as_array())
            .map_or_else(Vec::new, |l| l.iter().map(num).collect())
    };
    let class = |i: usize, key: &'static str, n: i64| {
        i16::try_from(n)
            .ok()
            .and_then(|n| ClassId::try_from(n).ok())
            .ok_or(DropSitesError::Class(i, key))
    };
    let Some(entries) = v["drop_site_list"].as_array() else {
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
                        attribute: AttributeId::of_field(field("attribute_type") as i16)
                            .map_err(|_| DropSitesError::Attribute(i, "attribute_type"))?,
                    })
                })
                .collect::<Result<Vec<_>, DropSitesError>>()?;
            Ok(DropSite {
                building: MasterId::try_from(num(&e["building_id"]) as i16)
                    .map_err(|_| DropSitesError::Building(i))?,
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
                    .map(|t| {
                        AttributeId::try_from(t.get("resource_type").map_or(-1, num) as i16)
                            .map_err(|_| DropSitesError::Attribute(i, "resource_type"))
                    })
                    .collect::<Result<Vec<_>, DropSitesError>>()?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::tables::{DropSitesError, UnitLinesError, drop_sites};
    use genie_model::objects::ObjectState;

    /// Each error names the entry and the key it found out of shape; a file that is not JSON
    /// gives the parser's message.
    #[test]
    fn table_errors_name_their_entry_and_key() {
        let json = || serde_json::from_str::<serde_json::Value>("[").unwrap_err();
        let drop_sites = [
            (DropSitesError::Json(json()), "EOF while parsing"),
            (
                DropSitesError::Class(2, "object_group"),
                "drop_site_list[2]: object_group is no master class",
            ),
            (
                DropSitesError::Building(3),
                "drop_site_list[3]: building_id is no master",
            ),
            (
                DropSitesError::Attribute(4, "attribute_type"),
                "drop_site_list[4]: attribute_type is no attribute",
            ),
            (
                DropSitesError::State(5),
                "drop_site_list[5]: object_state is no object state",
            ),
        ];
        for (error, text) in drop_sites {
            assert!(error.to_string().starts_with(text), "{error}");
        }
        assert!(
            UnitLinesError::Json(json())
                .to_string()
                .starts_with("EOF while parsing")
        );
        assert_eq!(
            UnitLinesError::Value(1, "LineID").to_string(),
            "UnitLines[1]: LineID is not what the game reads"
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
        assert!(matches!(state(Some(9)), Err(DropSitesError::State(0))));
    }
}
