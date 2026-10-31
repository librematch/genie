//! The vision's player bits ([`VisionBits`]) and a player's two masks ([`VisionMasks`]).

use crate::players::PlayerId;
use genie_diff::Diff;

bitflags::bitflags! {
    /// A word of the vision's player bits: the unified map's word of a tile (`+0x5e30`), a
    /// player's mask (`+0x2a8`, `+0x2a0`) or an object's (`+0x8f8`). Bit p: player p sees the
    /// tile (the low half, [`VisionBits::visible`]); bit 32 + p: player p explored it (the high
    /// half, [`VisionBits::explored`]). The named bits are the revealed and the ended ones no
    /// player has. Every bit is kept (`_ = !0`): the complement and the save's words change none.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct VisionBits: u64 {
        /// The visible bit no tile has that every player's mask carries (29).
        const NO_TILE = 1 << 29;
        /// The visible bit of a player who won or lost (`+0x89` 1 or 2; 30).
        const ENDED = 1 << 30;
        /// The explored bit no tile has that every player's explored mask carries (61).
        const NO_TILE_EXPLORED = 1 << 61;
        /// The explored bit of a player who won or lost (62).
        const ENDED_EXPLORED = 1 << 62;
        /// Every other bit: a player's.
        const _ = !0;
    }
}

impl VisionBits {
    /// No bit.
    pub const NONE: Self = Self::empty();
    /// The visible half (bits 0 to 31): the players that see.
    pub const VISIBLE_HALF: Self = Self::from_bits_retain(0xffff_ffff);
    /// The bits an object's mask keeps (`0x1ffffff01ffffff`): 25 visible and 25 explored.
    pub const OBJECT_MASK: Self = Self::from_bits_retain(0x1ff_ffff_01ff_ffff);

    /// Player `p`'s visible bit (`1 << p`, the shift's count taken mod 64 as the game's).
    #[must_use]
    pub fn visible(p: PlayerId) -> Self {
        Self::from_bits_retain(1u64.wrapping_shl(u32::from(p.get())))
    }

    /// Player `p`'s explored bit (`1 << (32 + p)`, the count mod 64).
    #[must_use]
    pub fn explored(p: PlayerId) -> Self {
        Self::from_bits_retain(1u64.wrapping_shl(32 + u32::from(p.get())))
    }
}

impl Diff for VisionBits {
    /// By equality: a word is one value.
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

/// A player's two vision masks (`+0x2a8`, `+0x2a0`): the bits whose tiles it sees and those
/// whose tiles it explored, its own and those it shares ([`crate::vision`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct VisionMasks {
    /// The visible mask (`+0x2a8`).
    pub visible: VisionBits,
    /// The explored mask (`+0x2a0`).
    pub explored: VisionBits,
}

#[cfg(test)]
mod tests {
    use crate::players::PlayerId;
    use crate::vision::VisionBits;

    /// Player p's visible bit is p, its explored bit 32 + p; words meet on a shared bit; no bit
    /// is ever dropped.
    #[test]
    fn a_player_s_bits_are_p_and_32_plus_p() {
        let p = PlayerId::new(3);
        assert_eq!(VisionBits::visible(p).bits(), 1 << 3);
        assert_eq!(VisionBits::explored(p).bits(), 1 << 35);
        let word = VisionBits::visible(p) | VisionBits::NO_TILE;
        assert!(word.intersects(VisionBits::visible(p)));
        assert!(!word.intersects(VisionBits::explored(p)));
        assert_eq!(word & !VisionBits::visible(p), VisionBits::NO_TILE);
        assert_eq!((!VisionBits::NONE).bits(), u64::MAX);
        assert!(VisionBits::NONE.is_empty() && !word.is_empty());
        assert_eq!(VisionBits::from_bits_retain(0x20).bits(), 0x20);
        assert_eq!(format!("{:?}", VisionBits::ENDED), "VisionBits(ENDED)");
        assert_eq!(format!("{word:?}"), "VisionBits(NO_TILE | 0x8)");
        assert_eq!(format!("{:x}", VisionBits::NO_TILE), "20000000");
        assert_eq!(format!("{:b}", VisionBits::visible(p)), "1000");
        let mut m = VisionBits::NONE;
        m |= VisionBits::ENDED;
        m &= VisionBits::OBJECT_MASK;
        assert!(m.is_empty());
    }
}
