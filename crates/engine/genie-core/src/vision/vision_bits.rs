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

    /// Player `player_id`'s visible bit (`1 << player_id`, the shift's count taken mod 64 as the
    /// game's).
    #[must_use]
    pub fn visible(player_id: PlayerId) -> Self {
        Self::from_bits_retain(1u64.wrapping_shl(u32::from(player_id.get())))
    }

    /// Player `player_id`'s explored bit (`1 << (32 + player_id)`, the count mod 64).
    #[must_use]
    pub fn explored(player_id: PlayerId) -> Self {
        Self::from_bits_retain(1u64.wrapping_shl(32 + u32::from(player_id.get())))
    }
}

impl Diff for VisionBits {
    /// By equality: a word is one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

/// A player's two vision masks (`+0x2a8`, `+0x2a0`): the bits whose tiles it sees and those
/// whose tiles it explored, its own and those it shares ([`crate::vision`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Diff)]
pub struct VisionMasks {
    /// The visible mask (`+0x2a8`).
    pub(crate) visible: VisionBits,
    /// The explored mask (`+0x2a0`).
    pub(crate) explored: VisionBits,
}

#[cfg(test)]
mod tests {
    use crate::players::PlayerId;
    use crate::vision::VisionBits;

    /// Player p's visible bit is p, its explored bit 32 + p; words meet on a shared bit; no bit
    /// is ever dropped.
    #[test]
    fn player_bits_are_its_index_and_32_plus_its_index() {
        let player_id = PlayerId::new(3);
        assert_eq!(VisionBits::visible(player_id).bits(), 1 << 3);
        assert_eq!(VisionBits::explored(player_id).bits(), 1 << 35);
        let word = VisionBits::visible(player_id) | VisionBits::NO_TILE;
        assert!(word.intersects(VisionBits::visible(player_id)));
        assert!(!word.intersects(VisionBits::explored(player_id)));
        assert_eq!(word & !VisionBits::visible(player_id), VisionBits::NO_TILE);
        assert_eq!((!VisionBits::NONE).bits(), u64::MAX);
        assert!(VisionBits::NONE.is_empty() && !word.is_empty());
        assert_eq!(VisionBits::from_bits_retain(0x20).bits(), 0x20);
        assert_eq!(format!("{:?}", VisionBits::ENDED), "VisionBits(ENDED)");
        assert_eq!(format!("{word:?}"), "VisionBits(NO_TILE | 0x8)");
        assert_eq!(format!("{:x}", VisionBits::NO_TILE), "20000000");
        assert_eq!(format!("{:b}", VisionBits::visible(player_id)), "1000");
        let mut vision_bits = VisionBits::NONE;
        vision_bits |= VisionBits::ENDED;
        vision_bits &= VisionBits::OBJECT_MASK;
        assert!(vision_bits.is_empty());
    }
}
