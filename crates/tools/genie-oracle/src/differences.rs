//! Where two object lists differ, object by object and field by field ([`Difference`]): every
//! field the game's save holds, each named by its path in genie's object (`life.hp`,
//! `body.paths[0].points[2]`), compared by genie-diff (FD-045-D14). Each object compares
//! whole, its row and its parts ([`ObjectView`]): the world's tables lend the slice's parts, the
//! import's objects their own (FD-045-D51).

use genie_core::objects::ObjectId;
use genie_core::objects::masters::MasterId;
use genie_core::world::parts::ObjectView;
use genie_diff::Scope;
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};

/// One field of one object where the slice and the game's save differ
/// ([`object_field_differences`]); its text is [`object_differences`]' line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// The object's id.
    pub id: ObjectId,
    /// Its master: the game's, or the slice's for an object only the slice holds.
    pub master: MasterId,
    /// The field's path in the object, or [`ONLY_IN_THE_GAME`], [`ONLY_IN_THE_SLICE`].
    pub field: String,
    /// The slice's value (its summary for an object only the slice holds).
    pub slice: String,
    /// The game's value (its summary for an object only the game holds).
    pub game: String,
}

/// [`Difference::field`] of an object the game's save holds and the slice does not.
pub const ONLY_IN_THE_GAME: &str = "only in the game";
/// [`Difference::field`] of an object the slice holds and the game's save does not.
pub const ONLY_IN_THE_SLICE: &str = "only in the slice";

impl Display for Difference {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), Error> {
        match self.field.as_str() {
            ONLY_IN_THE_GAME => write!(formatter, "{}: {} ({})", self.id, self.field, self.game),
            ONLY_IN_THE_SLICE => write!(formatter, "{}: {} ({})", self.id, self.field, self.slice),
            _ => write!(
                formatter,
                "{}: {} slice {} game {}",
                self.id, self.field, self.slice, self.game
            ),
        }
    }
}

/// Where two object lists differ, one line per object and field ([`object_field_differences`]).
#[must_use]
pub fn object_differences<'world>(
    slice: impl IntoIterator<Item = impl Into<ObjectView<'world>>>,
    game: impl IntoIterator<Item = impl Into<ObjectView<'world>>>,
) -> Vec<String> {
    object_field_differences(slice, game)
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Where two object lists differ, one [`Difference`] per object and field the game's save holds:
/// `slice` from the slice ([`crate::object_rows`]), `game` from the game's save imported as
/// genie's objects ([`crate::saved_objects`]), each whole, matched by id, in the game's order,
/// then the objects only the slice holds. Equal lists give none.
#[must_use]
pub fn object_field_differences<'world>(
    slice: impl IntoIterator<Item = impl Into<ObjectView<'world>>>,
    game: impl IntoIterator<Item = impl Into<ObjectView<'world>>>,
) -> Vec<Difference> {
    object_field_differences_in(slice, game, Scope::Saved)
}

/// [`object_field_differences`] over the fields of `scope`: [`Scope::SavedWithGaps`] adds the
/// fields genie models apart from the game (each one an entry in the workspace's `MISSING.md`),
/// [`Scope::All`] adds the unsaved ones too.
#[must_use]
pub fn object_field_differences_in<'world>(
    slice: impl IntoIterator<Item = impl Into<ObjectView<'world>>>,
    game: impl IntoIterator<Item = impl Into<ObjectView<'world>>>,
    scope: Scope,
) -> Vec<Difference> {
    let slice: Vec<ObjectView<'world>> = slice.into_iter().map(Into::into).collect();
    let game: Vec<ObjectView<'world>> = game.into_iter().map(Into::into).collect();
    let mut out = Vec::new();
    // the first slice object of each id, as a search from the front finds it
    let mut by_id = HashMap::with_capacity(slice.len());
    for object_view in &slice {
        let _ = by_id.entry(object_view.unit.id).or_insert(object_view);
    }
    for game_view in &game {
        let (id, master) = (game_view.unit.id, game_view.unit.life.master_id);
        let Some(&object_view) = by_id.get(&id) else {
            out.push(Difference {
                id,
                master,
                field: ONLY_IN_THE_GAME.to_owned(),
                slice: String::new(),
                game: summary(game_view),
            });
            continue;
        };
        out.extend(
            genie_diff::differences(object_view, game_view, scope)
                .into_iter()
                .map(|difference| Difference {
                    id,
                    master,
                    field: difference.path,
                    slice: difference.left,
                    game: difference.right,
                }),
        );
    }
    for object_view in slice.iter().filter(|slice_view| {
        !game
            .iter()
            .any(|game_view| game_view.unit.id == slice_view.unit.id)
    }) {
        out.push(Difference {
            id: object_view.unit.id,
            master: object_view.unit.life.master_id,
            field: ONLY_IN_THE_SLICE.to_owned(),
            slice: summary(object_view),
            game: String::new(),
        });
    }
    out
}

/// What names an object one side lacks: its master, state, hit points, place and top action.
fn summary(object_view: &ObjectView<'_>) -> String {
    let unit = object_view.unit;
    format!(
        "master {} state {} hp {:?} at {:?} action {:?}",
        unit.life.master_id,
        unit.life.state,
        unit.life.hp.get(),
        unit.position(),
        crate::top_action(&object_view.parts.actions)
            .map(|top_action| (top_action.kind, top_action.state))
    )
}
