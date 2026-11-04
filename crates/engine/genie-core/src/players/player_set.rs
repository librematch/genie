//! A set of players, one bit each ([`PlayerSet`]).

use crate::players::PlayerId;

bitflags::bitflags! {
    /// A set of players, one bit each (bit p: player p), as the game's 32-bit sets hold them:
    /// the players who cannot see a ghost (`+0x7f0`, the save's `CantSeeBits`), the players a
    /// unit's player counts as allies at a gate. Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct PlayerSet: u32 {
        /// Any player's bit.
        const _ = !0;
    }
}

impl PlayerSet {
    /// No player.
    pub const NONE: Self = Self::empty();

    /// The set of player `player_id` alone (`1 << player_id`, the shift's count taken mod 32 as the
    /// game's).
    #[must_use]
    pub fn of(player_id: PlayerId) -> Self {
        Self::from_bits_retain(1u32.wrapping_shl(u32::from(player_id.get())))
    }

    /// The set holds player `player_id` (its bit, the count mod 32).
    #[must_use]
    pub fn holds(self, player_id: PlayerId) -> bool {
        self.intersects(Self::of(player_id))
    }
}

impl FromIterator<PlayerId> for PlayerSet {
    /// The set of the players given, each its bit ([`PlayerSet::of`]).
    fn from_iter<I: IntoIterator<Item = PlayerId>>(players: I) -> Self {
        players
            .into_iter()
            .fold(Self::NONE, |set, player| set | Self::of(player))
    }
}

impl genie_diff::Diff for PlayerSet {
    /// By equality: a set is one value.
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::players::{PlayerId, PlayerSet};
    use std::iter::empty;

    /// Player p is bit p; a set holds the players it was given, and keeps every bit.
    #[test]
    fn player_is_its_bit() {
        let (one, four) = (PlayerId::new(1), PlayerId::new(4));
        assert_eq!(PlayerSet::of(four).bits(), 1 << 4);
        let mut set = PlayerSet::NONE;
        set |= PlayerSet::of(one);
        assert!(set.holds(one) && !set.holds(four));
        assert_eq!(PlayerSet::from_bits_retain(0x8000_0002).bits(), 0x8000_0002);
        assert_eq!(format!("{:b}", set | PlayerSet::of(four)), "10010");
        assert_eq!(PlayerSet::default(), PlayerSet::NONE);
    }

    /// A set collected from players holds each one's bit.
    #[test]
    fn players_collect_into_their_set() {
        let set: PlayerSet = [1, 4, 1].map(PlayerId::new).into_iter().collect();
        assert_eq!(set.bits(), 0b1_0010);
        assert_eq!(empty::<PlayerId>().collect::<PlayerSet>(), PlayerSet::NONE);
    }
}
