//! A player's tribute: `TRIBE_COMMAND_GIVE_ATTRIBUTES` (0xc4, `TRIBE_Command::do_command`'s case,
//! `FUN_140e7d040`). A player with a market gives up to four of its resources to another, and
//! pays the market's fee on top; what it lacks lowers both alike.

use crate::GenieError;
use crate::commands::ResourceWord;
use crate::math::fmath::maxss;
use crate::objects::masters::MasterId;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::TributeFee;
use crate::players::tracker::ListKey;
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

/// One of a tribute's four entries: the attribute (`+0x24`..), its amount (`+0x04`..) and the
/// fee (`+0x14`..), a share of the amount.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gift {
    /// The attribute given: food, wood, stone or gold (0 to 3); any other entry gives nothing.
    pub attribute: ResourceWord,
    /// The amount the receiver gets.
    pub amount: Amount,
    /// The fee, the share of the amount the giver pays on top.
    pub fee: TributeFee,
}

/// What one entry moves for a giver that has `have` of the resource: the amount the receiver
/// gets and what the giver pays. The amount and the fee are at least 0 (`MAXSS` with 0: a NaN is
/// 0); the fee on a positive amount is the amount times the fee; a giver that has less than the
/// two (`COMISS`, `JBE`: an unordered sum passes) pays all it has, the fee its share,
/// `have - have / (fee + 1)`, and the receiver gets the rest.
#[must_use]
pub fn settle(have: Amount, gift: Gift) -> (Amount, Amount) {
    let amount = Amount::new(maxss(gift.amount.get(), 0.0));
    let fee = maxss(gift.fee.get(), 0.0);
    let mut extra = if Amount::ZERO < amount {
        amount * fee
    } else {
        Amount::ZERO
    };
    let mut given = amount;
    if have < given + extra {
        extra = have - have / (fee + 1.0);
        given = have - extra;
    }
    (given, given + extra)
}

impl World {
    /// `FUN_1416259e0`: player `p` may pay tribute, its tracker counting a market or a 1647 above
    /// 0 (the build's `market` and `market_1647`; `FUN_140b26630`: a master the player has,
    /// `FUN_141659d20`, else none; the count of its done list, `FUN_14104eea0`).
    #[must_use]
    pub fn pays_tribute(&self, p: PlayerId) -> bool {
        let has = |m: MasterId| {
            self.model
                .masters
                .get(p.index())
                .is_some_and(|ms| ms.get(m).is_some())
        };
        let s = self.model.slots.master;
        [s.market, s.market_1647].iter().any(|&m| {
            has(m)
                && self
                    .players
                    .get(p.index())
                    .is_some_and(|pl| !pl.tracker.list(ListKey::master(m)).is_empty())
        })
    }

    /// `FUN_140e7d040`: player `from` gives player `to` its `gifts` ([`settle`]), unless the two
    /// are one or `from` has no market ([`World::pays_tribute`]). For each resource entry the
    /// receiver gains the amount and the giver loses it with the fee (the player's slot 0xf8,
    /// `FUN_141616820`, with 0 and 1: no gathered totals); the giver counts it in attributes 53
    /// and 450 + `to` and with the fee in 175, the receiver in 171 and 475 + `from`; then the
    /// receiver hears notify 0x20a (its slot 0x2e8, `FUN_14162dab0`: no case for it but the old
    /// short pather's hulled area and the shapes of the obstruction manager, neither held; a
    /// computer player's AI hears it, slot 0x3c8, which is not modelled). Not held: the local
    /// player's telemetry (`FUN_14097daf0`), the statistics (`FUN_1416625b0`,
    /// `FUN_1417786d0`) and the game's event 0x60 (`FUN_140c898a0`).
    ///
    /// # Errors
    ///
    /// An attribute a player does not have, and a computer player's receipt.
    pub(crate) fn tribute(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        gifts: &[Gift; 4],
    ) -> Result<(), GenieError> {
        if from == to || !self.pays_tribute(from) {
            return Ok(());
        }
        for &gift in gifts {
            let Some(kind) = gift.attribute.tributed() else {
                continue;
            };
            let have = *self
                .players
                .player(from)?
                .attributes
                .checked(kind.index())?;
            let (given, paid) = settle(have, gift);
            self.players.player_mut(to)?.add_attribute(kind, given)?;
            self.players.player_mut(from)?.add_attribute(kind, -paid)?;
            let giver = self.players.player_mut(from)?;
            giver.add_attribute(self.model.slots.attribute.tribute_given, given)?;
            giver.add_attribute(self.model.slots.attribute.tribute_paid, paid)?;
            giver.add_attribute(
                self.model.slots.attribute.tribute_to.offset(to.index())?,
                given,
            )?;
            let receiver = self.players.player_mut(to)?;
            receiver.add_attribute(self.model.slots.attribute.tribute_received, given)?;
            receiver.add_attribute(
                self.model
                    .slots
                    .attribute
                    .tribute_from
                    .offset(from.index())?,
                given,
            )?;
            if receiver.kind.is_computer() {
                return Err(GenieError::unsupported(
                    "a computer player's AI told of a tribute (0x20a, slot 0x3c8)",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::{Master, Masters};
    #[cfg(test)]
    use crate::players::PlayerType;
    #[cfg(test)]
    use crate::players::tracker::Key;
    use crate::test_ids::object_id;
    use crate::test_ids::player_id;
    #[cfg(test)]
    use crate::test_ids::{amounts, attributes};
    use crate::testing::class;
    use crate::testing::ids;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;
    use std::sync::Arc;

    fn gift(attribute: u16, amount: f32, fee: f32) -> Gift {
        Gift {
            attribute: ResourceWord::of_data(attribute),
            amount: Amount::new(amount),
            fee: TributeFee::of_data(fee),
        }
    }

    /// The fee goes on top of the amount; a giver short of the two pays all it has, the receiver
    /// getting `have / (fee + 1)` as `have - (have - have / (fee + 1))`; a negative or NaN
    /// amount or fee is 0.
    #[test]
    fn tribute_pays_its_fee_on_top_and_what_the_giver_lacks_lowers_both() {
        assert_eq!(
            settle(Amount::new(1000.0), gift(1, 100.0, 0.3)),
            (Amount::new(100.0), Amount::new(100.0 + 100.0 * 0.3))
        );
        let extra = 100.0 - 100.0 / 1.3f32;
        assert_eq!(
            settle(Amount::new(100.0), gift(1, 100.0, 0.3)),
            (
                Amount::new(100.0 - extra),
                Amount::new(100.0 - extra + extra)
            )
        );
        assert_eq!(
            settle(Amount::new(100.0), gift(0, -5.0, 0.3)),
            (Amount::ZERO, Amount::ZERO)
        );
        assert_eq!(
            settle(Amount::new(100.0), gift(0, f32::NAN, 0.3)),
            (Amount::ZERO, Amount::ZERO)
        );
        assert_eq!(
            settle(Amount::new(100.0), gift(0, 50.0, f32::NAN)),
            (Amount::new(50.0), Amount::new(50.0))
        );
        // no fee: exactly what it has
        assert_eq!(
            settle(Amount::new(40.0), gift(3, 50.0, 0.0)),
            (Amount::new(40.0), Amount::new(40.0))
        );
    }

    /// Players 1 and 2, each with 600 attributes and 500 of each resource, player 1 a market
    /// (master 84) when `market`.
    fn world(market: bool) -> World {
        let player = || Player {
            attributes: attributes(600, [500.0; 4]),
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 3],
            vec![Player::default(), player(), player()],
        );
        let masters = Masters::new([Master {
            id: ids::MARKET,
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            ..Master::default()
        }]);
        t.model.masters = vec![
            Arc::new(masters.clone()),
            Arc::new(masters.clone()),
            Arc::new(masters),
        ];
        if market {
            t.players[1].tracker.add(
                object_id(7),
                Key {
                    master: Some(ids::MARKET),
                    class: Some(class::BUILDING),
                    pending: false,
                    military: false,
                },
            );
        }
        t
    }

    /// `FUN_140e7d040`: player 1's tribute of 100 wood and 200 gold at a fee of 0.3 to player 2
    /// (an entry of attribute 4 gives nothing): the resources move with the fee, and both count
    /// it; without a market, or to itself, nothing moves.
    #[test]
    fn tribute_moves_the_resources_and_counts_them_for_both_players() {
        let gifts = [
            gift(1, 100.0, 0.3),
            gift(3, 200.0, 0.3),
            gift(4, 50.0, 0.3),
            gift(0, 0.0, 0.3),
        ];
        let mut t = world(true);
        t.tribute(player_id(1), player_id(2), &gifts).unwrap();
        let (giver, receiver) = (&t.players[1], &t.players[2]);
        assert_eq!(
            giver.resources(),
            amounts([500.0, 500.0 - 130.0, 500.0, 500.0 - 260.0])
        );
        assert_eq!(receiver.resources(), amounts([500.0, 600.0, 500.0, 700.0]));
        assert_eq!(
            (
                giver.attributes[53],
                giver.attributes[175],
                giver.attributes[452]
            ),
            (Amount::new(300.0), Amount::new(390.0), Amount::new(300.0))
        );
        assert_eq!(
            (receiver.attributes[171], receiver.attributes[476]),
            (Amount::new(300.0), Amount::new(300.0))
        );
        assert_eq!(receiver.attributes[4], Amount::ZERO);
        for (from, to, market) in [(1, 2, false), (1, 1, true)] {
            let mut t = world(market);
            let before = t.players.clone();
            t.tribute(player_id(from), player_id(to), &gifts).unwrap();
            assert_eq!(t.players, before);
        }
    }

    /// A computer player's AI would hear the tribute (its slot 0x3c8).
    #[test]
    fn computer_player_receipt_is_not_modelled() {
        let mut t = world(true);
        t.players[2].kind = PlayerType::COMPUTER;
        assert!(
            t.tribute(player_id(1), player_id(2), &[gift(1, 10.0, 0.3); 4])
                .is_err()
        );
    }
}
