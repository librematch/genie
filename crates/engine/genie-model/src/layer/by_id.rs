//! A role table's serde form (FD-046-D08): a record from each id with roles to its role names,
//! `{ "39" = ['GATE] }`, the ids without roles left out.

use bitflags::Flags;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serializer;
use serde::de::Error;
use serde::ser::SerializeMap;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The table of `entries`, by id: an id's later entry adds to its roles.
pub fn table<F: Flags + Copy>(entries: &[(usize, F)]) -> Vec<F> {
    let mut sets = vec![F::empty(); entries.iter().map(|e| e.0 + 1).max().unwrap_or(0)];
    for &(id, set) in entries {
        sets[id].insert(set);
    }
    sets
}

/// The table as a map from each id with roles to their names.
pub fn serialize<S: Serializer, F: Flags>(
    sets: &Arc<Vec<F>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let held: Vec<(usize, Vec<&str>)> = sets
        .iter()
        .enumerate()
        .filter(|(_, set)| !set.is_empty())
        .map(|(id, set)| (id, set.iter_names().map(|(name, _)| name).collect()))
        .collect();
    let mut map = serializer.serialize_map(Some(held.len()))?;
    for (id, names) in held {
        map.serialize_entry(&id.to_string(), &names)?;
    }
    map.end()
}

/// The table from a map from ids to role names; an id that is no number or a name no role of
/// the set fails.
pub fn deserialize<'de, D: Deserializer<'de>, F: Flags + Copy>(
    deserializer: D,
) -> Result<Arc<Vec<F>>, D::Error> {
    let by_id = BTreeMap::<String, Vec<String>>::deserialize(deserializer)?;
    let mut entries = Vec::with_capacity(by_id.len());
    for (id, names) in by_id {
        let id: usize = id
            .parse()
            .map_err(|_| D::Error::custom(format!("the id {id} is no number")))?;
        let mut set = F::empty();
        for name in names {
            set.insert(
                F::from_name(&name)
                    .ok_or_else(|| D::Error::custom(format!("{name} is no role")))?,
            );
        }
        entries.push((id, set));
    }
    Ok(Arc::new(table(&entries)))
}

/// The schema of a role table: a record from ids to lists of role names.
#[cfg(feature = "schema")]
pub type Schema<F> = BTreeMap<String, Vec<F>>;
