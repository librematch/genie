//! Types related to unit types.

use byteorder::{LE, WriteBytesExt};
use genie_support::StringKey;
pub use genie_support::UnitKindId;
use std::cmp::{Ordering, PartialOrd};
use std::convert::{TryFrom, TryInto};
use std::io;
use std::io::{Result, Write};

mod combat;
mod creatables;
mod movers;
mod statics;
mod unit;

pub use self::combat::{BaseCombatUnitKindAttributes, MissileUnitKindAttributes, WeaponInfo};
pub use self::creatables::{
    AttributeCost, BuildingUnitKindAttributes, CombatUnitKindAttributes, LinkedBuilding,
};
pub use self::movers::{
    ActionUnitKindAttributes, AnimatedUnitKindAttributes, MovingUnitKindAttributes,
};
pub use self::statics::{DamageSprite, StaticUnitKindAttributes, UnitAttribute};
pub use self::unit::UnitKind;
use std::result::Result as StdResult;

/// The base class of a unit indicates which data is available for that unit type.
///
/// # Comparison
/// This type implements a comparison operator. A base class value is greater-than-or-equal-to
/// another value if the base class "inherits" from the other value. For example, the
/// `Doppelganger` base class inherits from the `Animated` base class. No other class inherits from
/// `Doppelganger`. Therefore, it compares like this:
///
/// ```rust
/// # use genie_dat::unit_kind::UnitBaseClass;
/// assert!(UnitBaseClass::Doppelganger > UnitBaseClass::Animated);
/// assert!(UnitBaseClass::Doppelganger == UnitBaseClass::Doppelganger);
/// assert_eq!(UnitBaseClass::Doppelganger < UnitBaseClass::Moving, false);
/// assert_eq!(UnitBaseClass::Doppelganger > UnitBaseClass::Moving, false);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[non_exhaustive]
pub enum UnitBaseClass {
    /// The base unit type, for units that do not do anything.
    Static = 10,
    /// Unit type that supports animated sprites.
    Animated = 20,
    /// Unit type for the "fake" units you see in the fog of war, after the actual unit has been
    /// destroyed.
    Doppelganger = 25,
    /// Unit type that supports movement.
    Moving = 30,
    /// Unit type that supports being tasked by a player.
    Action = 40,
    /// Unit type that supports combat.
    BaseCombat = 50,
    /// Unit type for projectiles/missiles/arrows.
    Missile = 60,
    /// Unit type that supports combat (with additional Age of Empires specific data).
    Combat = 70,
    /// Unit type for buildings.
    Building = 80,
    /// The tree unit type.
    Tree = 90,
}

impl PartialOrd for UnitBaseClass {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            return Some(Ordering::Equal);
        }

        let self_n = *self as u8;
        let other_n = *other as u8;

        // handle weird leaves specially
        match self {
            Self::Doppelganger | Self::Missile => {
                if self_n > other_n {
                    Some(Ordering::Greater)
                } else {
                    None
                }
            }
            Self::Tree => match other {
                Self::Static => Some(Ordering::Greater),
                _ => None,
            },
            _ => match other {
                Self::Doppelganger | Self::Missile => {
                    if self_n < other_n {
                        Some(Ordering::Less)
                    } else {
                        None
                    }
                }
                Self::Tree => match self {
                    Self::Static => Some(Ordering::Less),
                    _ => None,
                },
                _ => Some(self_n.cmp(&other_n)),
            },
        }
    }
}

/// An unexpected unit base class was found.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("unknown unit base class: {}", .0)]
pub struct ParseUnitBaseClassError(u8);

impl TryFrom<u8> for UnitBaseClass {
    type Error = ParseUnitBaseClassError;

    fn try_from(n: u8) -> StdResult<Self, Self::Error> {
        match n {
            10 => Ok(Self::Static),
            20 => Ok(Self::Animated),
            25 => Ok(Self::Doppelganger),
            30 => Ok(Self::Moving),
            40 => Ok(Self::Action),
            50 => Ok(Self::BaseCombat),
            60 => Ok(Self::Missile),
            70 => Ok(Self::Combat),
            80 => Ok(Self::Building),
            90 => Ok(Self::Tree),
            n => Err(ParseUnitBaseClassError(n)),
        }
    }
}

impl From<UnitBaseClass> for u8 {
    fn from(class: UnitBaseClass) -> Self {
        class as Self
    }
}

/// A unit class, a group identifier for runtime behaviours.
pub type UnitClass = u16;

fn write_opt_string_key(mut output: impl Write, opt_key: Option<&StringKey>) -> Result<()> {
    output.write_i16::<LE>(if let Some(key) = opt_key {
        key.try_into().map_err(io::Error::other)?
    } else {
        -1
    })?;
    Ok(())
}

impl From<ParseUnitBaseClassError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ParseUnitBaseClassError) -> Self {
        Self::invalid_input("a unit base class this reader does not know").with_source(e)
    }
}

#[cfg(test)]
mod genie_error_tests {
    use super::*;
    #[cfg(test)]
    use std::error::Error as StdError;

    /// The errors convert into `GenieError`s with their kinds and reasons, the original as
    /// source.
    #[test]
    fn unit_base_class_error_converts_into_genie_error() {
        let e: genie_error::GenieError = ParseUnitBaseClassError(9).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a unit base class this reader does not know"
            )
        );
        assert!(StdError::source(&e).is_some());
    }
}
