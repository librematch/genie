//! Where two object lists differ, object by object and field by field ([`Difference`]): every
//! field the game's save holds, each named by its path in genie's object (`life.hp`,
//! `body.mover.paths[0].points[2]`), compared by genie-diff (FD-045-D14).

use genie_core::objects::ObjectId;
use genie_core::objects::masters::MasterId;
use genie_core::objects::unit::Unit;
use genie_diff::Scope;

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

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.field.as_str() {
            ONLY_IN_THE_GAME => write!(f, "{}: {} ({})", self.id, self.field, self.game),
            ONLY_IN_THE_SLICE => write!(f, "{}: {} ({})", self.id, self.field, self.slice),
            _ => write!(
                f,
                "{}: {} slice {} game {}",
                self.id, self.field, self.slice, self.game
            ),
        }
    }
}

/// Where two object lists differ, one line per object and field ([`object_field_differences`]).
#[must_use]
pub fn object_differences<'a>(
    slice: impl IntoIterator<Item = &'a Unit>,
    game: impl IntoIterator<Item = &'a Unit>,
) -> Vec<String> {
    object_field_differences(slice, game)
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Where two object lists differ, one [`Difference`] per object and field the game's save holds:
/// `slice` from the slice, `game` from the game's save imported as genie's objects, matched by
/// id, in the game's order, then the objects only the slice holds. Equal lists give none.
#[must_use]
pub fn object_field_differences<'a>(
    slice: impl IntoIterator<Item = &'a Unit>,
    game: impl IntoIterator<Item = &'a Unit>,
) -> Vec<Difference> {
    object_field_differences_in(slice, game, Scope::Saved)
}

/// [`object_field_differences`] over the fields of `scope`: [`Scope::SavedWithGaps`] adds the
/// fields genie models apart from the game (each one an entry in the workspace's `MISSING.md`),
/// [`Scope::All`] adds the unsaved ones too.
#[must_use]
pub fn object_field_differences_in<'a>(
    slice: impl IntoIterator<Item = &'a Unit>,
    game: impl IntoIterator<Item = &'a Unit>,
    scope: Scope,
) -> Vec<Difference> {
    let slice: Vec<&Unit> = slice.into_iter().collect();
    let game: Vec<&Unit> = game.into_iter().collect();
    let mut out = Vec::new();
    // the first slice object of each id, as a search from the front finds it
    let mut by_id = std::collections::HashMap::with_capacity(slice.len());
    for &s in &slice {
        let _ = by_id.entry(s.id).or_insert(s);
    }
    for &g in &game {
        let master = g.life.master_id;
        let Some(&s) = by_id.get(&g.id) else {
            out.push(Difference {
                id: g.id,
                master,
                field: ONLY_IN_THE_GAME.to_owned(),
                slice: String::new(),
                game: summary(g),
            });
            continue;
        };
        out.extend(
            genie_diff::differences(s, g, scope)
                .into_iter()
                .map(|d| Difference {
                    id: g.id,
                    master,
                    field: d.path,
                    slice: d.left,
                    game: d.right,
                }),
        );
    }
    for s in slice.iter().filter(|s| !game.iter().any(|g| g.id == s.id)) {
        out.push(Difference {
            id: s.id,
            master: s.life.master_id,
            field: ONLY_IN_THE_SLICE.to_owned(),
            slice: summary(s),
            game: String::new(),
        });
    }
    out
}

/// What names an object one side lacks: its master, state, hit points, place and top action.
fn summary(u: &Unit) -> String {
    format!(
        "master {} state {} hp {:?} at {:?} action {:?}",
        u.life.master_id,
        u.life.state,
        u.life.hp.get(),
        u.position(),
        crate::top_action(u).map(|a| (a.kind, a.state))
    )
}
