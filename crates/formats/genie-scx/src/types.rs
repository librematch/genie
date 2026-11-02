//! Contains pure types, no IO.
//!
//! Most of these are more descriptive wrappers around integers.
use std::cmp::Ordering;
use std::convert::TryFrom;
use std::fmt::{self, Debug, Display};

/// The SCX Format version string. In practice, this does not really reflect the game version.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ScxVersion(pub(crate) [u8; 4]);

impl ScxVersion {
    /// Get the raw bytes representing this scx format version.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub(crate) const fn to_player_version(self) -> Option<f32> {
        match self.as_bytes() {
            b"1.07" => Some(1.07),
            b"1.09" | b"1.10" | b"1.11" => Some(1.11),
            b"1.12" | b"1.13" | b"1.14" | b"1.15" | b"1.16" => Some(1.12),
            b"1.18" | b"1.19" => Some(1.13),
            b"1.20" | b"1.21" | b"1.32" | b"1.36" | b"1.37" => Some(1.14),
            _ => None,
        }
    }
}

impl Default for ScxVersion {
    fn default() -> Self {
        Self(*b"1.21")
    }
}

impl Debug for ScxVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", String::from_utf8_lossy(&self.0))
    }
}

impl Display for ScxVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}

impl PartialEq<[u8; 4]> for ScxVersion {
    fn eq(&self, other: &[u8; 4]) -> bool {
        other[0] == self.0[0] && other[1] == b'.' && other[2] == self.0[2] && other[3] == self.0[3]
    }
}

impl PartialEq<ScxVersion> for [u8; 4] {
    fn eq(&self, other: &ScxVersion) -> bool {
        other == self
    }
}

impl Ord for ScxVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.0[0].cmp(&other.0[0]) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.0[2].cmp(&other.0[2]) {
            Ordering::Equal => {}
            ord => return ord,
        }
        self.0[3].cmp(&other.0[3])
    }
}

impl PartialOrd for ScxVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Could not parse a diplomatic stance because given number is an unknown stance ID.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid diplomatic stance {} (must be 0/1/3)", .0)]
pub struct ParseDiplomaticStanceError(i32);

/// A player's diplomatic stance toward another player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiplomaticStance {
    /// The other player is an ally.
    Ally = 0,
    /// This player is neutral toward the other player.
    Neutral = 1,
    /// The other player is an enemy.
    Enemy = 3,
}

impl TryFrom<i32> for DiplomaticStance {
    type Error = ParseDiplomaticStanceError;

    fn try_from(n: i32) -> Result<Self, Self::Error> {
        match n {
            0 => Ok(Self::Ally),
            1 => Ok(Self::Neutral),
            3 => Ok(Self::Enemy),
            n => Err(ParseDiplomaticStanceError(n)),
        }
    }
}

impl From<DiplomaticStance> for i32 {
    fn from(stance: DiplomaticStance) -> Self {
        match stance {
            DiplomaticStance::Ally => 0,
            DiplomaticStance::Neutral => 1,
            DiplomaticStance::Enemy => 3,
        }
    }
}

/// Could not parse a data set because given number is an unknown data set ID.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid data set {} (must be 0/1)", .0)]
pub struct ParseDataSetError(i32);

/// The data set used by a scenario, HD Edition only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSet {
    /// The "base" data set, containing Age of Kings and the Age of Conquerors expansion.
    BaseGame,
    /// The "expansions" data set, containing the HD Edition expansions.
    Expansions,
}

impl TryFrom<i32> for DataSet {
    type Error = ParseDataSetError;
    fn try_from(n: i32) -> Result<Self, Self::Error> {
        match n {
            0 => Ok(Self::BaseGame),
            1 => Ok(Self::Expansions),
            n => Err(ParseDataSetError(n)),
        }
    }
}

impl From<DataSet> for i32 {
    fn from(id: DataSet) -> Self {
        match id {
            DataSet::BaseGame => 0,
            DataSet::Expansions => 1,
        }
    }
}

/// Could not parse a DLC package identifier because given number is an unknown DLC ID.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("unknown dlc package {}", .0)]
pub struct ParseDlcPackageError(i32);

/// An HD Edition DLC identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DlcPackage {
    /// The Age of Kings base game.
    AgeOfKings,
    /// The Age of Conquerors expansion.
    AgeOfConquerors,
    /// The Forgotten expansion.
    TheForgotten,
    /// The African Kingdoms expansion.
    AfricanKingdoms,
    /// The Rise of the Rajas expansion.
    RiseOfTheRajas,
    /// The Last Khans expansion.
    LastKhans,
}

impl TryFrom<i32> for DlcPackage {
    type Error = ParseDlcPackageError;
    fn try_from(n: i32) -> Result<Self, Self::Error> {
        match n {
            2 => Ok(Self::AgeOfKings),
            3 => Ok(Self::AgeOfConquerors),
            4 => Ok(Self::TheForgotten),
            5 => Ok(Self::AfricanKingdoms),
            6 => Ok(Self::RiseOfTheRajas),
            7 => Ok(Self::LastKhans),
            n => Err(ParseDlcPackageError(n)),
        }
    }
}

impl From<DlcPackage> for i32 {
    fn from(dlc_id: DlcPackage) -> Self {
        match dlc_id {
            DlcPackage::AgeOfKings => 2,
            DlcPackage::AgeOfConquerors => 3,
            DlcPackage::TheForgotten => 4,
            DlcPackage::AfricanKingdoms => 5,
            DlcPackage::RiseOfTheRajas => 6,
            DlcPackage::LastKhans => 7,
        }
    }
}

fn expected_range(version: f32) -> &'static str {
    if version < 1.25 { "-1-4" } else { "-1-6" }
}

/// Could not parse a starting age because given number refers to an unknown age.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid starting age {} (must be {})", .found, expected_range(*.version))]
pub struct ParseStartingAgeError {
    version: f32,
    found: i32,
}

/// The starting age.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum StartingAge {
    /// Use the game default.
    Default = -1,
    /// Start in the Dark Age with Nomad resources.
    Nomad = -2,
    /// Start in the Dark Age.
    DarkAge = 0,
    /// Start in the Feudal Age.
    FeudalAge = 1,
    /// Start in the Castle Age.
    CastleAge = 2,
    /// Start in the Imperial Age.
    ImperialAge = 3,
    /// Start in the Imperial Age with all technologies researched.
    PostImperialAge = 4,
}

impl StartingAge {
    /// Convert a starting age number to the appropriate enum for a particular
    /// data version.
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is not a starting age in that data version.
    pub fn try_from(n: i32, version: f32) -> Result<Self, ParseStartingAgeError> {
        if version < 1.25 {
            match n {
                -1 => Ok(Self::Default),
                0 => Ok(Self::DarkAge),
                1 => Ok(Self::FeudalAge),
                2 => Ok(Self::CastleAge),
                3 => Ok(Self::ImperialAge),
                4 => Ok(Self::PostImperialAge),
                _ => Err(ParseStartingAgeError { version, found: n }),
            }
        } else {
            match n {
                -1 | 0 => Ok(Self::Default),
                1 => Ok(Self::Nomad),
                2 => Ok(Self::DarkAge),
                3 => Ok(Self::FeudalAge),
                4 => Ok(Self::CastleAge),
                5 => Ok(Self::ImperialAge),
                6 => Ok(Self::PostImperialAge),
                _ => Err(ParseStartingAgeError { version, found: n }),
            }
        }
    }

    /// Serialize the age identifier to an integer that is understood by the given game version.
    #[must_use]
    pub fn to_i32(self, version: f32) -> i32 {
        if version < 1.25 {
            match self {
                Self::Default => -1,
                Self::Nomad | Self::DarkAge => 0,
                Self::FeudalAge => 1,
                Self::CastleAge => 2,
                Self::ImperialAge => 3,
                Self::PostImperialAge => 4,
            }
        } else {
            match self {
                Self::Default => 0,
                Self::Nomad => 1,
                Self::DarkAge => 2,
                Self::FeudalAge => 3,
                Self::CastleAge => 4,
                Self::ImperialAge => 5,
                Self::PostImperialAge => 6,
            }
        }
    }
}

/// Known types of victory conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VictoryCondition {
    /// Capture an object.
    Capture,
    /// Create a number of objects.
    Create,
    /// Destroy an object.
    Destroy,
    /// Destroy a number of objects of a specific type, owned by a specific player.
    DestroyMultiple,
    /// Bring an object into an area.
    BringToArea,
    /// Bring an object close to another object.
    BringToObject,
    /// Collect an amount of a specific resource.
    Attribute,
    /// Explore a certain percentage of the map.
    Explore,
    /// Bring a number of objects into an area.
    CreateInArea,
    /// Destroy all objects of a specific type, owned by a specific player.
    DestroyAll,
    /// Defeat a specific player.
    DestroyPlayer,
    /// Collect an amount of points.
    Points,
    /// Some unknown value, either because of a corrupt scenario file or some new unsupported
    /// game feature.
    Other(u8),
}

impl From<u8> for VictoryCondition {
    fn from(n: u8) -> Self {
        match n {
            0 => Self::Capture,
            1 => Self::Create,
            2 => Self::Destroy,
            3 => Self::DestroyMultiple,
            4 => Self::BringToArea,
            5 => Self::BringToObject,
            6 => Self::Attribute,
            7 => Self::Explore,
            8 => Self::CreateInArea,
            9 => Self::DestroyAll,
            10 => Self::DestroyPlayer,
            11 => Self::Points,
            n => Self::Other(n),
        }
    }
}

impl From<VictoryCondition> for u8 {
    fn from(condition: VictoryCondition) -> Self {
        match condition {
            VictoryCondition::Capture => 0,
            VictoryCondition::Create => 1,
            VictoryCondition::Destroy => 2,
            VictoryCondition::DestroyMultiple => 3,
            VictoryCondition::BringToArea => 4,
            VictoryCondition::BringToObject => 5,
            VictoryCondition::Attribute => 6,
            VictoryCondition::Explore => 7,
            VictoryCondition::CreateInArea => 8,
            VictoryCondition::DestroyAll => 9,
            VictoryCondition::DestroyPlayer => 10,
            VictoryCondition::Points => 11,
            VictoryCondition::Other(n) => n,
        }
    }
}

/// All the versions an SCX file uses in a single struct.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VersionBundle {
    /// The version of the 'container' file format.
    pub format: ScxVersion,
    /// The version of the header.
    pub header: u32,
    /// The version of the HD Edition DLC Options, only if `header` >= 3.
    pub dlc_options: Option<i32>,
    /// The compressed data version.
    pub data: f32,
    /// The version of embedded bitmaps.
    pub picture: u32,
    /// The version of the victory conditions data.
    pub victory: f32,
    /// The version of the trigger system.
    pub triggers: Option<f64>,
    /// The version of the map data.
    pub map: u32,
}

impl VersionBundle {
    /// A version bundle with the parameters `AoE1` uses by default.
    ///
    /// # Errors
    ///
    /// Always: the `AoE1` bundle is not modelled yet.
    pub const fn aoe() -> Result<Self, crate::Error> {
        Err(crate::Error::UnsupportedGameError)
    }

    /// A version bundle with the parameters `AoE1`: Rise of Rome uses by default.
    #[must_use]
    pub const fn ror() -> Self {
        Self {
            format: ScxVersion(*b"1.11"),
            header: 2,
            dlc_options: None,
            data: 1.15,
            picture: 1,
            victory: 2.0,
            triggers: None,
            map: 0,
        }
    }

    /// A version bundle with the parameters `AoK` uses by default.
    #[must_use]
    pub const fn aok() -> Self {
        Self {
            format: ScxVersion(*b"1.18"),
            header: 2,
            dlc_options: None,
            data: 1.2,
            picture: 1,
            victory: 2.0,
            triggers: Some(1.6),
            map: 0,
        }
    }

    /// A version bundle with the parameters `AoC` uses by default
    #[must_use]
    pub const fn aoc() -> Self {
        Self {
            format: ScxVersion(*b"1.21"),
            header: 2,
            dlc_options: None,
            data: 1.22,
            picture: 1,
            victory: 2.0,
            triggers: Some(1.6),
            map: 0,
        }
    }

    /// A version bundle with the parameters `UserPatch` 1.4 uses by default.
    #[must_use]
    pub const fn userpatch_14() -> Self {
        Self::aoc()
    }

    /// A version bundle with the parameters `UserPatch` 1.5 uses by default.
    #[must_use]
    pub const fn userpatch_15() -> Self {
        Self::userpatch_14()
    }

    /// A version bundle with the parameters HD Edition uses by default.
    #[must_use]
    pub const fn hd_edition() -> Self {
        Self {
            format: ScxVersion(*b"1.21"),
            header: 3,
            dlc_options: Some(1000),
            data: 1.26,
            picture: 3,
            victory: 2.0,
            triggers: Some(1.6),
            map: 0,
        }
    }

    /// A version bundle with parameters Age of Empires 2: Definitive Edition uses by default.
    ///
    /// This will be updated along with DE2 patches.
    #[must_use]
    pub const fn aoe2_de() -> Self {
        Self {
            format: ScxVersion(*b"1.37"),
            header: 5,
            dlc_options: Some(1000),
            data: 1.37,
            picture: 3,
            victory: 2.0,
            triggers: Some(2.2),
            map: 2,
        }
    }

    /// Returns whether this version is (likely) for an `AoK` scenario.
    #[must_use]
    pub const fn is_aok(&self) -> bool {
        matches!(self.format.as_bytes(), b"1.18" | b"1.19" | b"1.20")
    }

    /// Returns whether this version is (likely) for an `AoC` scenario.
    #[must_use]
    pub fn is_aoc(&self) -> bool {
        self.format == *b"1.21" && self.data <= 1.22
    }

    /// Returns whether this version is (likely) for an HD Edition scenario.
    #[must_use]
    pub fn is_hd_edition(&self) -> bool {
        self.format == *b"1.21" || self.format == *b"1.22" && self.data > 1.22
    }

    /// Returns whether this version is (likely) for an `AoE2`: Definitive Edition scenario.
    #[must_use]
    pub fn is_age2_de(&self) -> bool {
        self.data >= 1.28
    }
}

impl From<ParseDiplomaticStanceError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ParseDiplomaticStanceError) -> Self {
        Self::invalid_input("a diplomatic stance id this reader does not know").with_source(e)
    }
}

impl From<ParseDataSetError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ParseDataSetError) -> Self {
        Self::invalid_input("a data set id this reader does not know").with_source(e)
    }
}

impl From<ParseDlcPackageError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ParseDlcPackageError) -> Self {
        Self::invalid_input("a DLC package id this reader does not know").with_source(e)
    }
}

impl From<ParseStartingAgeError> for genie_error::GenieError {
    /// The error as a [`genie_error::GenieError`]: its kind and reason, the original as the
    /// source.
    #[track_caller]
    fn from(e: ParseStartingAgeError) -> Self {
        Self::invalid_input("a starting age id this reader does not know").with_source(e)
    }
}

#[cfg(test)]
mod genie_error_tests {
    use super::*;

    /// The errors convert into `GenieError`s with their kinds and reasons, the original as
    /// source.
    #[test]
    fn id_errors_convert_into_genie_errors() {
        let e: genie_error::GenieError = ParseDiplomaticStanceError(9).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a diplomatic stance id this reader does not know"
            )
        );
        assert!(std::error::Error::source(&e).is_some());
        let e: genie_error::GenieError = ParseDataSetError(9).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a data set id this reader does not know"
            )
        );
        assert!(std::error::Error::source(&e).is_some());
        let e: genie_error::GenieError = ParseDlcPackageError(9).into();
        assert_eq!(
            (e.kind(), e.reason()),
            (
                genie_error::ErrorKind::InvalidInput,
                "a DLC package id this reader does not know"
            )
        );
        assert!(std::error::Error::source(&e).is_some());
    }

    /// The victory conditions' ids 0 to 11 and any other kept as it is, both ways.
    #[test]
    fn a_victory_condition_s_id_reads_and_writes_both_ways() {
        let named = [
            VictoryCondition::Capture,
            VictoryCondition::Create,
            VictoryCondition::Destroy,
            VictoryCondition::DestroyMultiple,
            VictoryCondition::BringToArea,
            VictoryCondition::BringToObject,
            VictoryCondition::Attribute,
            VictoryCondition::Explore,
            VictoryCondition::CreateInArea,
            VictoryCondition::DestroyAll,
            VictoryCondition::DestroyPlayer,
            VictoryCondition::Points,
        ];
        for (id, condition) in (0u8..).zip(named) {
            assert_eq!(VictoryCondition::from(id), condition);
            assert_eq!(u8::from(condition), id);
        }
        assert_eq!(VictoryCondition::from(12), VictoryCondition::Other(12));
        assert_eq!(u8::from(VictoryCondition::Other(200)), 200);
    }
}
