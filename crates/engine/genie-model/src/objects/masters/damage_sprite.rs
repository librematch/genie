//! A master's damage sprite ([`DamageGraphic`]): its sprite, the hit point share it shows from and
//! how it is applied.

use crate::objects::SpriteId;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// One damage sprite of a master (`+0x638`'s entries; the save's `damage_sprite`). A new entry
/// (the default) has no sprite and zero bytes (the game zeroes its 16 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DamageGraphic {
    /// `+0x0`, the sprite (`tempId`, −1 for none).
    pub sprite: Option<SpriteId>,
    /// `+0x8`, `damage_percent`.
    pub percent: HpPercent,
    /// `+0x9`, the `flag` (the `.dat`'s `apply_mode`).
    pub apply_mode: DamageSpriteMode,
}

/// A hit point share in whole per cent (a byte): a damage sprite's `damage_percent` (`+0x8`),
/// below which share it shows, and an object's damage share (`+0xd`) the sprites follow.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HpPercent(u8);

impl HpPercent {
    /// The whole share (100).
    pub const WHOLE: Self = Self(100);

    /// The share of the byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The share an add effect of `n` makes of it: the byte's sum, wrapping.
    #[must_use]
    pub const fn plus(self, n: u8) -> Self {
        Self(self.0.wrapping_add(n))
    }
}

/// How a damage sprite is applied (its `flag`, `+0x9`, the `.dat`'s `apply_mode`): placed,
/// scattered (two Object-stream draws) or shown as the object's sprite.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash,
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct DamageSpriteMode(u8);

impl DamageSpriteMode {
    /// Placed on the object (0).
    pub const PLACED: Self = Self(0);
    /// Scattered over it (1).
    pub const SCATTERED: Self = Self(1);
    /// Shown as its sprite (2).
    pub const SHOWN: Self = Self(2);

    /// The mode of the data's byte `value`: every byte is one.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The mode an add effect of `n` makes of it: the byte's sum, wrapping.
    #[must_use]
    pub const fn plus(self, n: u8) -> Self {
        Self(self.0.wrapping_add(n))
    }
}

impl Diff for HpPercent {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

impl Diff for DamageSpriteMode {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::{DamageSpriteMode, HpPercent};

    /// The shares order as bytes and add wrapping; the modes are the data's bytes.
    #[test]
    fn the_damage_sprite_values_are_the_game_bytes() {
        assert!(HpPercent::of_data(25) < HpPercent::WHOLE);
        assert_eq!(HpPercent::of_data(250).plus(10), HpPercent::of_data(4));
        assert_eq!(DamageSpriteMode::default(), DamageSpriteMode::PLACED);
        assert_eq!(DamageSpriteMode::SCATTERED.plus(1), DamageSpriteMode::SHOWN);
        assert_eq!(DamageSpriteMode::of_data(7).get(), 7);
    }
}
