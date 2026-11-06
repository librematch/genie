//! `SELL_COMMODITY` (0x7a) and `BUY_COMMODITY` (0x7b): the player trades at the market.

use crate::GenieResult;
use crate::commands::ResourceWord;
use crate::commands::TradeLots;
use crate::objects::ObjectId;
use crate::players::MarketTrade;
use crate::players::PlayerId;
use crate::world::World;

/// `SELL_COMMODITY` (0x7a) and `BUY_COMMODITY` (0x7b), `do_command`'s cases: the player trades
/// `times` lots (`+0x6`) of the commodity `attribute` (`+0x4`'s low byte: food, wood or stone;
/// the decoder drops another) at the market ([`World::sell`], [`World::buy`]). The market
/// building (`+0x8`) is not read. Not held: the interface's view (`FUN_140e6de20`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TradeCommand {
    /// The player who trades.
    pub player: PlayerId,
    /// `+0x4`: the commodity (the dispatcher reads its low byte).
    pub attribute: ResourceWord,
    /// `+0x6`: how many lots.
    pub times: TradeLots,
    /// `+0x8`: the market, which the check (`FUN_140e6f3f0`) reads; none for −1, which a
    /// recorded trade can hold.
    pub market: Option<ObjectId>,
    /// A purchase (`BUY_COMMODITY`) or a sale (`SELL_COMMODITY`).
    pub trade: MarketTrade,
}

impl TradeCommand {
    /// The dispatcher trades food, wood and stone only (the attribute's low byte 0 to 2,
    /// `do_command`'s 0x7a and 0x7b): the purchase ([`World::buy`]) or the sale
    /// ([`World::sell`]); a trade cannot fail.
    pub(crate) fn run(&self, world: &mut World) -> GenieResult<()> {
        let Some(commodity) = self.attribute.traded() else {
            return Ok(());
        };
        match self.trade {
            MarketTrade::Buy => world.buy(self.player, commodity, self.times),
            MarketTrade::Sell => world.sell(self.player, commodity, self.times),
        }
    }
}
