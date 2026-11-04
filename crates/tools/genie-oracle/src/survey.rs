//! The survey (FD-045-D14): a recording run from its header against the game's saves over a
//! whole window, every differing field counted, not only the first divergence. It shows which
//! fields the port gets wrong and the update where each first does.

use crate::errors::no_such_player;
use genie_core::GenieResult;
use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use genie_diff::Scope;
use genie_rec::de::chapters;

use crate::census::{CensusStop, stop_of};
use crate::{
    Decoder, Difference, MasterTable, frames, header_objects, header_slice,
    object_field_differences_in, object_rows, run,
};
use std::slice::from_ref;

/// One field of a survey ([`header_survey`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurveyRow {
    /// The field's path without its indices (`body.paths[].points[]`), or
    /// [`crate::ONLY_IN_THE_GAME`], [`crate::ONLY_IN_THE_SLICE`].
    pub field: String,
    /// The comparisons where it differs, one per object and save.
    pub count: usize,
    /// The first update where it differs.
    pub first: usize,
    /// The player of its first difference.
    pub player: u32,
    /// Its first difference.
    pub example: Difference,
}

/// What a survey found ([`header_survey`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Survey {
    /// Each differing field, by its first update, then by its path.
    pub rows: Vec<SurveyRow>,
    /// Each player that stopped, with its stop; its later saves are not compared.
    pub stops: Vec<(u32, CensusStop)>,
    /// The game's saves compared.
    pub saves: usize,
}

/// The recording run from its header as the census runs it, the objects of each of `players`
/// compared in `scope` with the game's save after update `first` and every `every`-th update
/// after it, to `last` (`save(u)` as for [`crate::census::header_divergences`], a missing save
/// skipped): each differing field with its count and first difference. A player's comparisons
/// end at its stop; the run ends when every player has stopped.
///
/// # Errors
///
/// When the recording, its header or a save cannot be read, or a player is not in it.
pub fn header_survey(
    recording: &[u8],
    masters: Option<&MasterTable>,
    players: &[u32],
    (first, last): (usize, usize),
    every: NonZeroUsize,
    scope: Scope,
    save: &mut dyn FnMut(usize) -> Option<Vec<u8>>,
) -> GenieResult<Survey> {
    let chapter = chapters(recording)?[0];
    let header = chapter.header()?;
    let mut turns = header_slice(&header, masters)?;
    let mut decoder = Decoder::default();
    if let Some(&p) = players.iter().find(|&&p| turns.players.len() <= p as usize) {
        return Err(no_such_player(p));
    }
    let mut survey = Survey::default();
    let mut rows: BTreeMap<String, SurveyRow> = BTreeMap::new();
    let mut stopped = vec![false; players.len()];
    for (i, frame) in frames(&chapter, last)?.iter().enumerate() {
        run(&mut turns, &mut decoder, from_ref(frame));
        let update = i + 1;
        for (k, &p) in players.iter().enumerate() {
            if let (false, Some(stop)) = (stopped[k], stop_of(&turns, p)) {
                survey.stops.push((p, stop));
                stopped[k] = true;
            }
        }
        if stopped.iter().all(|&s| s) {
            break;
        }
        if update < first || (update - first) % every.get() != 0 {
            continue;
        }
        let Some(game) = save(update) else {
            continue;
        };
        let saved = chapters(&game)?[0];
        let saved = saved.header()?;
        survey.saves += 1;
        for (k, &p) in players.iter().enumerate() {
            if stopped[k] {
                continue;
            }
            let game = header_objects(&saved, p, masters)?;
            for d in object_field_differences_in(object_rows(&turns, p as usize), &game, scope) {
                let field = field_key(&d.field);
                let _ = rows
                    .entry(field.clone())
                    .and_modify(|r| r.count += 1)
                    .or_insert(SurveyRow {
                        field,
                        count: 1,
                        first: update,
                        player: p,
                        example: d,
                    });
            }
        }
    }
    survey.rows = rows.into_values().collect();
    // stable: the map's order by path stays within an update
    survey.rows.sort_by_key(|r| r.first);
    Ok(survey)
}

/// A field's path without its indices and keys: `actions[0].to[1]` is `actions[].to[]`.
pub(crate) fn field_key(path: &str) -> String {
    let mut key = String::with_capacity(path.len());
    let mut depth = 0usize;
    for c in path.chars() {
        match c {
            '[' => {
                if depth == 0 {
                    key.push_str("[]");
                }
                depth += 1;
            }
            ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => key.push(c),
            _ => {}
        }
    }
    key
}

#[cfg(test)]
mod tests;
