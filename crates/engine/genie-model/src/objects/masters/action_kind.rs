//! The action types (`TRIBE_Action_*`'s `+0x8`, a task's `action_type`): the action a task makes
//! a unit do, and the type of an action a unit runs. The types more than one routine names are
//! here; the ones only one routine names stay beside it (the Gather's 5, the Attack's 9, the
//! Make's 21).

use crate::{GenieError, GenieResult};
use derive_more::Display;
use genie_diff::{Diff, Differ};
use serde::Deserialize;
use serde::Serialize;

/// An action type: a task's `action_type` (`+0x10`, the `.dat`'s) and an action's type (`+0x8`),
/// one numbering. The `.dat` holds types the simulation does not run (the AI's, the scenario's),
/// so the type is a newtype with named constants, not an enum: any value 0 or more the data
/// holds is a type, and the named ones are values of the type. A field or a filter that holds −1
/// for none or any is `Option<ActionKind>`.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Display,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ActionKind(
    /// Its value, as the `.dat`, the save and a checkpoint hold it.
    i16,
);

impl ActionKind {
    /// `TRIBE_Action_Build`'s type (101, `0x65`): the task a builder works by.
    pub const BUILD: Self = Self::named(101);

    /// `TRIBE_Action_Trade`'s type (111, `0x6f`): the task of the trade carts and ships, the task whose carrying sprite a trade cart with goods shows
    /// (`FUN_1412028a0`, `object_id` 0x54), and whose load keeps a stopped unit's AI going
    /// (`FUN_14106a940`).
    pub const TRADE: Self = Self::named(0x6f);

    /// `TRIBE_Action_Pack`'s type (124, `0x7c`): the Gather's drop-site searches pass over it with [`Self::UNPACK`]
    /// (`FUN_1411671e0`).
    pub const PACK: Self = Self::named(0x7c);

    /// `TRIBE_Action_Unpack`'s type (125, `0x7d`): the task of the packed siege units (master type
    /// 80, class 51: the trebuchet and the mangonel), whose target a gather drops
    /// (`FUN_140a903a0`). A building whose top action is an Unpack takes a hunt sent to it to
    /// `set_state` 15 (a farm's top action is `TRIBE_Action_Farm`, 121).
    pub const UNPACK: Self = Self::named(0x7d);

    /// `RGE_Action_Combat`'s task type (7): a task of attacking, whose factory makes an Attack (9).
    pub const COMBAT: Self = Self::named(7);

    /// `RGE_Action_Guard`'s type (13, `0xd`): the task a list for an own, allied or artifact target
    /// takes under that filter alone.
    pub const GUARD: Self = Self::named(0xd);

    /// `TRIBE_Action_Convert`'s type (104, `0x68`): a monk's conversion.
    pub const CONVERT: Self = Self::named(0x68);

    /// `TRIBE_Action_Heal`'s type (105, `0x69`): a monk's healing, whose range is the target's
    /// owner's attribute 90.
    pub const HEAL: Self = Self::named(0x69);

    /// The type 123 (`0x7b`) a building's queue counts as busy with [`Self::PACK`] and [`Self::UNPACK`].
    pub const ACTION_123: Self = Self::named(0x7b);

    /// The type 126 (`0x7e`): its object is never interruptible (slot 0x2d0).
    pub const ACTION_126: Self = Self::named(0x7e);

    /// The type 132 (`0x84`): picking an object up (a monk's relic); the group commands use it too.
    pub const PICK_UP: Self = Self::named(0x84);

    /// The type 135 (`0x87`): the task a monk carrying a relic takes at its own town centre.
    pub const ACTION_135: Self = Self::named(0x87);

    /// The type 136 (`0x88`): depositing what an object carries (a relic); the group commands use it
    /// too.
    pub const DEPOSIT: Self = Self::named(0x88);

    /// The type 156 (`0x9c`): a task of a new unit that spawns more units (`FUN_140ace760`).
    pub const ACTION_156: Self = Self::named(0x9c);

    /// The type 154 (`0x9a`) of the task a kill's bonus reads (`kill_bonus_tasks`); the villager's
    /// wait for a foundation (`genie_core::construction::wait_solidify::WAIT_SOLIDIFY`) has the same
    /// number as its action's type.
    pub const LOOT: Self = Self::named(0x9a);

    /// `TRIBE_Action_Make_Obj`'s type (102, `0x66`): a building's training of a unit.
    pub const MAKE_OBJECT: Self = Self::named(0x66);

    /// `TRIBE_Action_Make_Tech`'s type (103, `0x67`): a building's research.
    pub const MAKE_TECH: Self = Self::named(0x67);

    /// The type 6 of the task whose work range a new object keeps at `+0xb74` (`FUN_141206d50`): a
    /// hunted animal's leash.
    pub const HUNT_RANGE: Self = Self::named(6);

    /// Its value, for a packet, a dump and the checkpoint's words (const, for the tables the
    /// game holds by number).
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    /// A type the code names, in a constant (an action's module names its own).
    ///
    /// # Panics
    ///
    /// For a negative value, which no type is (at compile time in a constant).
    #[must_use]
    pub const fn named(value: i16) -> Self {
        assert!(0 <= value, "a negative action type");
        Self(value)
    }

    /// The type a data field names, none for a negative value (−1 in the `.dat` and the save).
    #[must_use]
    pub const fn of_data(value: i16) -> Option<Self> {
        if value < 0 { None } else { Some(Self(value)) }
    }

    /// The value of a field or filter that holds −1 for none.
    #[must_use]
    pub const fn field(kind: Option<Self>) -> i16 {
        match kind {
            Some(action_kind) => action_kind.0,
            None => -1,
        }
    }
}

/// The types of `values`, in their order: a table the game holds by number.
#[must_use]
pub const fn kinds<const N: usize>(values: [i16; N]) -> [ActionKind; N] {
    let mut out = [ActionKind(0); N];
    let mut i = 0;
    while i < N {
        out[i] = ActionKind::named(values[i]);
        i += 1;
    }
    out
}

impl TryFrom<i16> for ActionKind {
    type Error = GenieError;

    fn try_from(value: i16) -> GenieResult<Self> {
        Self::of_data(value)
            .ok_or_else(|| GenieError::invalid_input("a negative action type").with("value", value))
    }
}

impl Diff for ActionKind {
    fn diff(&self, other: &Self, differ: &mut Differ) {
        differ.equal(self, other);
    }
}
