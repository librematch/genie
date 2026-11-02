//! The market: the world's commodity prices (`+0x868`..`+0x8a0`, saved) and the two commands that
//! trade at them, `TRIBE_COMMAND_SELL_COMMODITY` (0x7a, `FUN_141639310`) and
//! `TRIBE_COMMAND_BUY_COMMODITY` (0x7b, `FUN_141618390`). A trade moves its commodity's price a
//! step (`FUN_1415caf80` down, `FUN_1415cd6a0` up): the step is the growth of the commodity's
//! swing (2 a trade, truncated) in hundredths, and the price resets when the net trades come back
//! to 0. Nothing else changes the prices.

use crate::GenieError;
use crate::commands::TradeLots;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, maxss};
use crate::math::image::market::{
    FLIP as MARKET_FLIP, FOOD_START as MARKET_FOOD_START, LEAST_COST as MARKET_LEAST_COST,
    ORE_START as MARKET_ORE_START, PERCENT as MARKET_PERCENT, ROUND as MARKET_ROUND,
    STONE_START as MARKET_STONE_START, SWING as MARKET_SWING,
    UNKNOWN_PRICE as MARKET_UNKNOWN_PRICE, WOOD_START as MARKET_WOOD_START,
};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::ResourceStat;
use crate::players::attribute::{self, AttributeId};
use crate::players::{NetTrades, Price, PriceSwing};
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// A commodity's price after its net trades came back to 0 (`DAT_142f03164`, `DAT_14330eb8c`,
/// `DAT_142f0703c`).
const FOOD_START: Price = Price::new(MARKET_FOOD_START);
const WOOD_START: Price = Price::new(MARKET_WOOD_START);
const STONE_START: Price = Price::new(MARKET_STONE_START);
const ORE_START: Price = Price::new(MARKET_ORE_START);
/// A trade's growth of the swing (`DAT_142e8c150`), the hundredths of its step
/// (`DAT_14330ebcc`) and the sign flip of a negative growth (`DAT_14330ec20`).
const SWING: f32 = MARKET_SWING;
const PERCENT: f32 = MARKET_PERCENT;
const FLIP: f32 = MARKET_FLIP;
/// The price of an attribute no commodity is (`FUN_1415cd3c0`, `DAT_14330ec20`).
const UNKNOWN_PRICE: Price = Price::new(MARKET_UNKNOWN_PRICE);
/// A purchase costs at least this (`DAT_142f66f34`).
const LEAST_COST: f32 = MARKET_LEAST_COST;
/// A sale's gold rounds half up (`DAT_142e7f384`).
const ROUND: f32 = MARKET_ROUND;

/// One commodity's state in the world.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Commodity {
    /// Its price in gold (wood `+0x874`, food `+0x878`, stone `+0x87c`, ore `+0x880`).
    pub price: Price,
    /// The purchases less the sales (the save's `*_price_delta`: wood `+0x884`, stone `+0x88c`,
    /// food `+0x894`, ore `+0x89c`, an int).
    pub trades: NetTrades,
    /// The swing (the save's `old_*_price_adj`: `+0x888`, `+0x890`, `+0x898`, `+0x8a0`), which
    /// every trade grows by 2.
    pub swing: PriceSwing,
}

/// The world's market.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Market {
    /// `world_min_price` (`+0x868`): a sale lowers no price below it.
    pub min_price: Price,
    /// `world_max_price` (`+0x86c`): a purchase raises no price above it.
    pub max_price: Price,
    /// Food (attribute 0).
    pub food: Commodity,
    /// Wood (attribute 1).
    pub wood: Commodity,
    /// Stone (attribute 2).
    pub stone: Commodity,
    /// The ore (attribute 0x38).
    pub ore: Commodity,
}

impl Market {
    /// The commodity of attribute `commodity` with its starting price, none for another.
    const fn commodity(&mut self, commodity: AttributeId) -> Option<(&mut Commodity, Price)> {
        match commodity {
            attribute::FOOD => Some((&mut self.food, FOOD_START)),
            attribute::WOOD => Some((&mut self.wood, WOOD_START)),
            attribute::STONE => Some((&mut self.stone, STONE_START)),
            attribute::ORE => Some((&mut self.ore, ORE_START)),
            _ => None,
        }
    }

    /// `FUN_1415cd3c0`: the price of attribute `commodity`'s commodity, −1 for another.
    #[must_use]
    pub const fn price(&self, commodity: AttributeId) -> Price {
        match commodity {
            attribute::FOOD => self.food.price,
            attribute::WOOD => self.wood.price,
            attribute::STONE => self.stone.price,
            attribute::ORE => self.ore.price,
            _ => UNKNOWN_PRICE,
        }
    }

    /// One trade of attribute `commodity`'s commodity, a purchase (`FUN_1415cd6a0`) or a sale
    /// (`FUN_1415caf80`); another attribute changes nothing. The net trades move by one; at 0
    /// the swing is 0 and the price its start. Else the swing grows by 2, and its growth (made
    /// positive: `COMISS`, `JBE`, a NaN stays; truncated by `CVTTSS2SI`) in hundredths raises the
    /// price to at most the maximum, or lowers it to at least the minimum (`COMISS`, `JBE`: a NaN
    /// price stays). The game's event 0x80 on a changed price is the interface's.
    pub fn trade(&mut self, commodity: AttributeId, buy: bool) {
        let (min, max) = (self.min_price, self.max_price);
        let Some((c, start)) = self.commodity(commodity) else {
            return;
        };
        c.trades = c.trades.traded(buy);
        if c.trades == NetTrades::ZERO {
            c.swing = PriceSwing::ZERO;
            c.price = start;
            return;
        }
        let next = c.swing.get() + SWING;
        let mut growth = next - c.swing.get();
        if 0.0 > growth {
            growth *= FLIP;
        }
        let step = cvttss2si(growth) as f32 / PERCENT;
        if buy {
            // ADDSS step, price: a NaN price stays itself (the step is never one)
            c.price = c.price.raised(step);
            c.swing = PriceSwing::new(next);
            if c.price > max {
                c.price = max;
            }
        } else {
            c.price = c.price.lowered(step);
            c.swing = PriceSwing::new(next);
            if min > c.price {
                c.price = min;
            }
        }
    }
}

impl World {
    /// The market fee's attribute of player `p` (78, read without a bound check; 0 past the
    /// player's attributes).
    fn fee(&self, p: PlayerId) -> f32 {
        self.players
            .get(p.index())
            .and_then(|pl| pl.attributes.get(attribute::MARKET_FEE.index()).copied())
            .map_or(0.0, Amount::get)
    }

    /// `FUN_141639310`: player `p` sells `times` lots of the commodity `attribute` (the
    /// command's food, wood or stone), each its commodity volume (`+0x140c`). A lot it does not
    /// hold (`COMISS`, `JC`: a NaN does not), or a price at most 0 (`JBE`), ends the sales. The
    /// price falls ([`Market::trade`]), then the lot's worth at the old price less the fee
    /// (attribute 78 times it), plus 0.5, truncated, comes in as gold (the player's slot 0xf8
    /// with 1 and 1: the gold counts as gathered too, [`World::add_resource`]) and the lot goes
    /// (slot 0xf8 with 0 and 1). Not held: the statistics (`FUN_141778560`) and the local
    /// player's telemetry (`FUN_14097daf0`).
    pub(crate) fn sell(
        &mut self,
        p: PlayerId,
        commodity: AttributeId,
        times: TradeLots,
    ) -> Result<(), GenieError> {
        for _ in 0..times.get() {
            let volume = self.players.player(p)?.commodity_volume;
            let held = self
                .players
                .player(p)?
                .attribute(commodity)
                .unwrap_or(Amount::ZERO);
            if below_or_unordered(held.get(), volume.as_f32()) {
                return Ok(());
            }
            let price = self.market.price(commodity).get();
            if at_most_or_unordered(price, 0.0) {
                return Ok(());
            }
            self.market.trade(commodity, false);
            let worth = volume.as_f32() * price;
            let fee = self.fee(p) * worth;
            let gold = cvttss2si(worth - fee + ROUND) as f32;
            // The gold a sale gives and a purchase costs.
            self.add_resource(
                p,
                attribute::GOLD,
                Amount::new(gold),
                ResourceStat::GATHERED,
                true,
            )?;
            let lot = volume.negated_f32();
            self.add_resource(p, commodity, Amount::new(lot), ResourceStat::NONE, true)?;
        }
        Ok(())
    }

    /// `FUN_141618390`: player `p` buys `times` lots of the commodity `attribute`, each its
    /// commodity volume (`+0x140c`, none below 1). A price at most 0 (`COMISS`, `JBE`) ends the
    /// purchases, as does a cost (the lot's worth plus the fee, attribute 78 times it; at least
    /// 21, `MAXSS`) above the player's gold (`JC`: a NaN too). The cost, truncated, goes (slot
    /// 0xf8 with 0 and 1), then the price rises ([`Market::trade`]) and the lot comes in. Not
    /// held: the statistics (`FUN_141778560`) and the local player's telemetry
    /// (`FUN_14097daf0`).
    pub(crate) fn buy(
        &mut self,
        p: PlayerId,
        commodity: AttributeId,
        times: TradeLots,
    ) -> Result<(), GenieError> {
        for _ in 0..times.get() {
            let volume = self.players.player(p)?.commodity_volume;
            if !volume.any() {
                return Ok(());
            }
            let gold = self
                .players
                .player(p)?
                .attribute(attribute::GOLD)
                .unwrap_or(Amount::ZERO);
            let price = self.market.price(commodity).get();
            if at_most_or_unordered(price, 0.0) {
                return Ok(());
            }
            let lot = volume.as_f32();
            let worth = lot * price;
            let cost = maxss(LEAST_COST, self.fee(p) * worth + worth);
            if below_or_unordered(gold.get(), cost) {
                return Ok(());
            }
            let paid = cvttss2si(cost) as f32;
            self.add_resource(
                p,
                attribute::GOLD,
                Amount::new(-paid),
                ResourceStat::NONE,
                true,
            )?;
            self.market.trade(commodity, true);
            self.add_resource(p, commodity, Amount::new(lot), ResourceStat::NONE, true)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::players::LotVolume;
    use crate::test_ids::player_id;
    #[cfg(test)]
    use crate::test_ids::{amounts, attributes};
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;

    fn market() -> Market {
        let at = |price| Commodity {
            price: Price::new(price),
            trades: NetTrades::ZERO,
            swing: PriceSwing::ZERO,
        };
        Market {
            min_price: Price::new(0.2),
            max_price: Price::new(9.99),
            food: at(1.15),
            wood: at(1.0),
            stone: at(1.3),
            ore: at(1.0),
        }
    }

    /// `FUN_1415cd6a0` and `FUN_1415caf80`: each trade moves the price by the swing's growth
    /// (2, truncated) in hundredths and grows the swing; the price stays within the minimum
    /// and the maximum; net trades back at 0 reset the swing and the starting price.
    #[test]
    fn trade_moves_the_price_step_and_back_at_no_net_trades_it_resets() {
        let mut m = market();
        m.trade(attribute::WOOD, true);
        assert_eq!(
            m.wood,
            Commodity {
                price: Price::new(1.0 + 0.02),
                trades: NetTrades::new(1),
                swing: PriceSwing::new(2.0),
            }
        );
        m.trade(attribute::WOOD, true);
        assert_eq!(
            (m.wood.price.get(), m.wood.trades.get(), m.wood.swing.get()),
            (0.02 + (1.0 + 0.02), 2, 4.0)
        );
        m.trade(attribute::WOOD, false);
        assert_eq!(
            (m.wood.price.get(), m.wood.trades.get(), m.wood.swing.get()),
            (0.02 + (1.0 + 0.02) - 0.02, 1, 6.0)
        );
        m.trade(attribute::WOOD, false);
        assert_eq!(
            m.wood,
            Commodity {
                price: Price::new(1.0),
                trades: NetTrades::new(0),
                swing: PriceSwing::new(0.0),
            }
        );
        // the stone's start, the bounds, another attribute
        m.stone.price = Price::new(9.98);
        m.trade(attribute::STONE, true);
        m.trade(attribute::STONE, true);
        assert_eq!(m.stone.price.get(), 9.99);
        m.trade(attribute::STONE, false);
        m.trade(attribute::STONE, false);
        assert_eq!(m.stone.price.get(), 1.3);
        m.food.price = Price::new(0.21);
        m.trade(attribute::FOOD, false);
        m.trade(attribute::FOOD, false);
        assert_eq!(m.food.price.get(), 0.2);
        let before = m;
        m.trade(attribute::HEADROOM, true);
        assert_eq!((m, m.price(attribute::HEADROOM).get()), (before, -1.0));
        // a swing so large that 2 more does not change it: no step
        m.ore.swing = PriceSwing::new(1.0e8);
        m.ore.trades = NetTrades::new(5);
        m.trade(attribute::ORE, true);
        assert_eq!((m.ore.price.get(), m.ore.swing.get()), (1.0, 1.0e8));
    }

    /// Player 1 with 100 of each resource, a commodity volume of 100, a fee of 0.3 and the
    /// market above.
    fn trading() -> World {
        let mut attributes = attributes(250, [100.0; 4]);
        attributes[attribute::MARKET_FEE.index()] = Amount::new(0.3);
        let player = Player {
            attributes,
            commodity_volume: LotVolume::of_data(100),
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 2],
            vec![Player::default(), player],
        );
        t.market = market();
        t
    }

    /// `FUN_141639310`: a sale of 100 wood at 1.0 gives 100 less the fee of 30, plus 0.5,
    /// truncated (70 gold, also counted as gathered gold, 49 and 169), takes the 100 wood and
    /// lowers the price; a second lot the player does not hold sells nothing.
    #[test]
    fn sale_gives_the_lot_worth_less_the_fee_at_the_old_price() {
        let mut t = trading();
        t.sell(player_id(1), attribute::WOOD, TradeLots::of_data(2))
            .unwrap();
        let p = &t.players[1];
        assert_eq!(p.resources(), amounts([100.0, 0.0, 100.0, 170.0]));
        assert_eq!(
            (p.attributes[49], p.attributes[169]),
            (Amount::new(70.0), Amount::new(70.0))
        );
        assert_eq!(
            (t.market.wood.price.get(), t.market.wood.trades.get()),
            (1.0 - 0.02, -1)
        );
    }

    /// `FUN_141618390`: a purchase of 100 food at 1.15 costs 115 plus the fee of 34.5, truncated
    /// (149): the player's 170 gold pays one lot, not the second; the price rises after the
    /// payment. A cheap lot costs at least 21.
    #[test]
    fn purchase_costs_the_lot_worth_and_the_fee_at_least_21() {
        let mut t = trading();
        t.players[1].attributes[attribute::GOLD.index()] = Amount::new(170.0);
        t.buy(player_id(1), attribute::FOOD, TradeLots::of_data(2))
            .unwrap();
        let p = &t.players[1];
        assert_eq!(p.resources(), amounts([200.0, 100.0, 100.0, 21.0]));
        assert_eq!(
            (p.attributes[49], p.attributes[169]),
            (Amount::ZERO, Amount::ZERO)
        );
        assert_eq!(
            (t.market.food.price.get(), t.market.food.trades.get()),
            (0.02 + 1.15, 1)
        );
        let mut t = trading();
        t.players[1].commodity_volume = LotVolume::of_data(10);
        t.buy(player_id(1), attribute::WOOD, TradeLots::of_data(1))
            .unwrap();
        assert_eq!(
            t.players[1].attributes[attribute::GOLD.index()],
            Amount::new(100.0 - 21.0)
        );
        // no volume, no purchase
        t.players[1].commodity_volume = LotVolume::of_data(0);
        t.buy(player_id(1), attribute::WOOD, TradeLots::of_data(1))
            .unwrap();
        assert_eq!(
            t.players[1].attributes[attribute::GOLD.index()],
            Amount::new(79.0)
        );
    }
}
