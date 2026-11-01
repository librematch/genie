//! A computer player's chance setting: the percent a unit AI's draw is held against.

/// A computer player's chance (saved; GAME 0xb's settings 1 and 2): the missile dodge's
/// (`chance_to_dodge_missiles`, `+0xe10`) and the siege's keep-distance
/// (`chance_for_archers_to_maintain_distance`, `+0xe11`), a percent a unit AI's 15-bit draw is
/// held against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Chance(u8);

impl Chance {
    /// No chance: no draw passes (a player the world lacks).
    pub const NONE: Self = Self(0);

    /// The chance of the save's or the command's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// Whether the 15-bit draw `draw` passes: its percent (`IMUL` by 100, then `IDIV` by
    /// 0x7fff, which a 15-bit draw keeps positive) lies below the chance.
    #[must_use]
    pub fn passes(self, draw: u32) -> bool {
        (draw as i32) * 100 / 0x7fff < i32::from(self.0)
    }

    /// The byte, as the save holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A draw passes when its percent lies below the chance: 0x7fff is 100, 0 passes any
    /// chance but none.
    #[test]
    fn a_draw_passes_below_the_chance_s_percent() {
        let chance = Chance::of_data(30);
        // 9830 * 100 / 0x7fff = 29, 9831 * 100 / 0x7fff = 30
        assert!(chance.passes(9830));
        assert!(!chance.passes(9831));
        assert!(Chance::of_data(1).passes(0));
        assert!(!Chance::NONE.passes(0));
        assert!(!Chance::of_data(100).passes(0x7fff));
        assert_eq!(chance.get(), 30);
    }
}
