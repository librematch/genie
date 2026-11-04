//! Contains pure types, no IO.
//!
//! Most of these are more descriptive wrappers around integers.
use genie_error::{GenieError, GenieResult};
use std::cmp::Ordering;
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

/// A player's diplomatic stance toward another player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::FromPrimitive, num_enum::IntoPrimitive)]
#[repr(i32)]
#[non_exhaustive]
pub enum DiplomaticStance {
    /// The other player is an ally.
    Ally = 0,
    /// This player is neutral toward the other player.
    Neutral = 1,
    /// The other player is an enemy.
    Enemy = 3,
    /// A value no stance names (a corrupt file or a newer game's), kept to be written back.
    #[num_enum(catch_all)]
    Other(i32),
}

/// The data set used by a scenario, HD Edition only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::FromPrimitive, num_enum::IntoPrimitive)]
#[repr(i32)]
#[non_exhaustive]
pub enum DataSet {
    /// The "base" data set, containing Age of Kings and the Age of Conquerors expansion.
    BaseGame = 0,
    /// The "expansions" data set, containing the HD Edition expansions.
    Expansions = 1,
    /// A value no data set names, kept to be written back.
    #[num_enum(catch_all)]
    Other(i32),
}

/// An HD Edition DLC identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::FromPrimitive, num_enum::IntoPrimitive)]
#[repr(i32)]
#[non_exhaustive]
pub enum DlcPackage {
    /// The Age of Kings base game.
    AgeOfKings = 2,
    /// The Age of Conquerors expansion.
    AgeOfConquerors = 3,
    /// The Forgotten expansion.
    TheForgotten = 4,
    /// The African Kingdoms expansion.
    AfricanKingdoms = 5,
    /// The Rise of the Rajas expansion.
    RiseOfTheRajas = 6,
    /// The Last Khans expansion.
    LastKhans = 7,
    /// A value no DLC names (a newer one), kept to be written back.
    #[num_enum(catch_all)]
    Other(i32),
}

/// The starting age.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum StartingAge {
    /// Use the game default.
    Default,
    /// Start in the Dark Age with Nomad resources.
    Nomad,
    /// Start in the Dark Age.
    DarkAge,
    /// Start in the Feudal Age.
    FeudalAge,
    /// Start in the Castle Age.
    CastleAge,
    /// Start in the Imperial Age.
    ImperialAge,
    /// Start in the Imperial Age with all technologies researched.
    PostImperialAge,
    /// A value no age of the data version names, kept to be written back.
    Other(i32),
}

impl StartingAge {
    /// The starting age that number `n` names in data version `version`: before 1.25 −1 to 4
    /// (the default, then Dark to Post-Imperial), from 1.25 on −1 or 0 (the default), 1 (Nomad)
    /// and 2 to 6; any other number is [`StartingAge::Other`].
    #[must_use]
    pub const fn from_i32(n: i32, version: f32) -> Self {
        if version < 1.25 {
            match n {
                -1 => Self::Default,
                0 => Self::DarkAge,
                1 => Self::FeudalAge,
                2 => Self::CastleAge,
                3 => Self::ImperialAge,
                4 => Self::PostImperialAge,
                n => Self::Other(n),
            }
        } else {
            match n {
                -1 | 0 => Self::Default,
                1 => Self::Nomad,
                2 => Self::DarkAge,
                3 => Self::FeudalAge,
                4 => Self::CastleAge,
                5 => Self::ImperialAge,
                6 => Self::PostImperialAge,
                n => Self::Other(n),
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
                Self::Other(n) => n,
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
                Self::Other(n) => n,
            }
        }
    }
}

/// Known types of victory conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    pub const fn aoe() -> GenieResult<Self> {
        Err(GenieError::missing_data(
            "a version bundle this crate does not hold",
        ))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The victory conditions' ids 0 to 11 and any other kept as it is, both ways.
    #[test]
    fn victory_condition_id_reads_and_writes_both_ways() {
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

    /// A value no variant names reads as `Other` and writes back as itself; a named one keeps
    /// its number.
    #[test]
    fn unnamed_values_are_kept_and_written_back() {
        assert_eq!(DiplomaticStance::from(3), DiplomaticStance::Enemy);
        assert_eq!(DiplomaticStance::from(2), DiplomaticStance::Other(2));
        assert_eq!(i32::from(DiplomaticStance::Other(2)), 2);
        assert_eq!(i32::from(DiplomaticStance::Enemy), 3);
        assert_eq!(DataSet::from(9), DataSet::Other(9));
        assert_eq!(i32::from(DataSet::Other(9)), 9);
        assert_eq!(DlcPackage::from(7), DlcPackage::LastKhans);
        assert_eq!(DlcPackage::from(8), DlcPackage::Other(8));
        assert_eq!(i32::from(DlcPackage::Other(8)), 8);
        assert_eq!(VictoryCondition::from(12), VictoryCondition::Other(12));
    }

    /// A starting age's number depends on the data version: 1 is Feudal before 1.25 and Nomad
    /// from 1.25 on; a number past the version's last age is kept.
    #[test]
    fn starting_ages_read_by_their_data_version() {
        assert_eq!(StartingAge::from_i32(1, 1.2), StartingAge::FeudalAge);
        assert_eq!(StartingAge::from_i32(1, 1.25), StartingAge::Nomad);
        assert_eq!(StartingAge::from_i32(99, 1.2), StartingAge::Other(99));
        assert_eq!(StartingAge::Other(99).to_i32(1.2), 99);
        assert_eq!(StartingAge::from_i32(7, 1.3), StartingAge::Other(7));
        assert_eq!(StartingAge::PostImperialAge.to_i32(1.3), 6);
    }
}
