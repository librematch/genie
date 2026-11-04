//! The market: the world's commodity prices (`+0x868`..`+0x8a0`, saved) and the two commands that
//! trade at them, `TRIBE_COMMAND_SELL_COMMODITY` (0x7a, `FUN_141639310`) and
//! `TRIBE_COMMAND_BUY_COMMODITY` (0x7b, `FUN_141618390`). A trade moves its commodity's price a
//! step (`FUN_1415caf80` down, `FUN_1415cd6a0` up): the step is the growth of the commodity's
//! swing (2 a trade, truncated) in hundredths, and the price resets when the net trades come back
//! to 0. Nothing else changes the prices.

use crate::GenieResult;
use crate::commands::TradeLots;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, maxss};
use crate::math::image::market::{
    FLIP as MARKET_FLIP, PERCENT as MARKET_PERCENT, ROUND as MARKET_ROUND,
    UNKNOWN_PRICE as MARKET_UNKNOWN_PRICE,
};
use crate::players::Amount;
use crate::players::MarketTrade;
use crate::players::PlayerId;
use crate::players::ResourceStat;
use crate::players::attribute::AttributeId;
use crate::players::world_player::ResourceShare;
use crate::players::{NetTrades, Price, PriceSwing};
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::MarketTuning;
use crate::world::slots::PlayerSlotsExt;

/// The hundredths of a trade's step (`DAT_14330ebcc`) and the sign flip of a negative growth
/// (`DAT_14330ec20`).
const PERCENT: f32 = MARKET_PERCENT;
const FLIP: f32 = MARKET_FLIP;
/// The price of an attribute no commodity is (`FUN_1415cd3c0`, `DAT_14330ec20`).
const UNKNOWN_PRICE: Price = Price::new(MARKET_UNKNOWN_PRICE);
/// A sale's gold rounds half up (`DAT_142e7f384`).
const ROUND: f32 = MARKET_ROUND;

/// One commodity's state in the world.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Commodity {
    /// Its price in gold (wood `+0x874`, food `+0x878`, stone `+0x87c`, ore `+0x880`).
    pub(crate) price: Price,
    /// The purchases less the sales (the save's `*_price_delta`: wood `+0x884`, stone `+0x88c`,
    /// food `+0x894`, ore `+0x89c`, an int).
    pub(crate) trades: NetTrades,
    /// The swing (the save's `old_*_price_adj`: `+0x888`, `+0x890`, `+0x898`, `+0x8a0`), which
    /// every trade grows by 2.
    pub(crate) swing: PriceSwing,
}

/// The world's market.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Market {
    /// `world_min_price` (`+0x868`): a sale lowers no price below it.
    pub(crate) min_price: Price,
    /// `world_max_price` (`+0x86c`): a purchase raises no price above it.
    pub(crate) max_price: Price,
    /// Food (attribute 0).
    pub(crate) food: Commodity,
    /// Wood (attribute 1).
    pub(crate) wood: Commodity,
    /// Stone (attribute 2).
    pub(crate) stone: Commodity,
    /// The ore (attribute 0x38).
    pub(crate) ore: Commodity,
}

impl Market {
    /// The commodity of attribute `commodity` with its starting price, none for another.
    fn commodity(
        &mut self,
        commodity: AttributeId,
        roles: &Roles,
        market_tuning: &MarketTuning,
    ) -> Option<(&mut Commodity, Price)> {
        {
            let held = roles.of_attribute(commodity);
            if held.contains(AttributeRoles::FOOD) {
                Some((&mut self.food, Price::new(market_tuning.food_start)))
            } else if held.contains(AttributeRoles::WOOD) {
                Some((&mut self.wood, Price::new(market_tuning.wood_start)))
            } else if held.contains(AttributeRoles::STONE) {
                Some((&mut self.stone, Price::new(market_tuning.stone_start)))
            } else if held.contains(AttributeRoles::ORE) {
                Some((&mut self.ore, Price::new(market_tuning.ore_start)))
            } else {
                None
            }
        }
    }

    /// `FUN_1415cd3c0`: the price of attribute `commodity`'s commodity, −1 for another.
    #[must_use]
    pub fn price(&self, commodity: AttributeId, roles: &Roles) -> Price {
        {
            let held = roles.of_attribute(commodity);
            if held.contains(AttributeRoles::FOOD) {
                self.food.price
            } else if held.contains(AttributeRoles::WOOD) {
                self.wood.price
            } else if held.contains(AttributeRoles::STONE) {
                self.stone.price
            } else if held.contains(AttributeRoles::ORE) {
                self.ore.price
            } else {
                UNKNOWN_PRICE
            }
        }
    }

    /// One trade of attribute `commodity`'s commodity, a purchase (`FUN_1415cd6a0`) or a sale
    /// (`FUN_1415caf80`); another attribute changes nothing. The net trades move by one; at 0
    /// the swing is 0 and the price its start. Else the swing grows by 2, and its growth (made
    /// positive: `COMISS`, `JBE`, a NaN stays; truncated by `CVTTSS2SI`) in hundredths raises the
    /// price to at most the maximum, or lowers it to at least the minimum (`COMISS`, `JBE`: a NaN
    /// price stays). The game's event 0x80 on a changed price is the interface's.
    pub fn trade(
        &mut self,
        commodity: AttributeId,
        trade: MarketTrade,
        roles: &Roles,
        market_tuning: &MarketTuning,
    ) {
        let (min, max) = (self.min_price, self.max_price);
        let Some((commodity, start)) = self.commodity(commodity, roles, market_tuning) else {
            return;
        };
        commodity.trades = commodity.trades.traded(trade);
        if commodity.trades == NetTrades::ZERO {
            commodity.swing = PriceSwing::ZERO;
            commodity.price = start;
            return;
        }
        let next = commodity.swing.get() + market_tuning.swing;
        let mut growth = next - commodity.swing.get();
        if 0.0 > growth {
            growth *= FLIP;
        }
        let step = cvttss2si(growth) as f32 / PERCENT;
        if trade == MarketTrade::Buy {
            // ADDSS step, price: a NaN price stays itself (the step is never one)
            commodity.price = commodity.price.raised(step);
            commodity.swing = PriceSwing::new(next);
            if commodity.price > max {
                commodity.price = max;
            }
        } else {
            commodity.price = commodity.price.lowered(step);
            commodity.swing = PriceSwing::new(next);
            if min > commodity.price {
                commodity.price = min;
            }
        }
    }
}

impl World {
    /// The market fee's attribute of player `player_id` (78, read without a bound check; 0 past the
    /// player's attributes).
    fn fee(&self, player_id: PlayerId) -> f32 {
        self.players
            .get(player_id.index())
            .and_then(|pl| {
                pl.attributes
                    .get(self.model.slots.attribute.market_fee.index())
                    .copied()
            })
            .map_or(0.0, Amount::get)
    }

    /// `FUN_141639310`: player `player_id` sells `times` lots of the commodity `attribute` (the
    /// command's food, wood or stone), each its commodity volume (`+0x140c`). A lot it does not
    /// hold (`COMISS`, `JC`: a NaN does not), or a price at most 0 (`JBE`), ends the sales. The
    /// price falls ([`Market::trade`]), then the lot's worth at the old price less the fee
    /// (attribute 78 times it), plus 0.5, truncated, comes in as gold (the player's slot 0xf8
    /// with 1 and 1: the gold counts as gathered too, [`World::add_resource`]) and the lot goes
    /// (slot 0xf8 with 0 and 1). Not held: the statistics (`FUN_141778560`) and the local
    /// player's telemetry (`FUN_14097daf0`).
    pub(crate) fn sell(
        &mut self,
        player_id: PlayerId,
        commodity: AttributeId,
        times: TradeLots,
    ) -> GenieResult<()> {
        for _ in 0..times.get() {
            let volume = self.players.player(player_id)?.commodity_volume;
            let held = self
                .players
                .player(player_id)?
                .attribute(commodity)
                .unwrap_or(Amount::ZERO);
            if below_or_unordered(held.get(), volume.as_f32()) {
                return Ok(());
            }
            let price = self.market.price(commodity, &self.model.roles).get();
            if at_most_or_unordered(price, 0.0) {
                return Ok(());
            }
            self.market.trade(
                commodity,
                MarketTrade::Sell,
                &self.model.roles,
                &self.model.tuning.market,
            );
            let worth = volume.as_f32() * price;
            let fee = self.fee(player_id) * worth;
            let gold = cvttss2si(worth - fee + ROUND) as f32;
            // The gold a sale gives and a purchase costs.
            self.add_resource(
                player_id,
                self.model.slots.attribute.gold,
                Amount::new(gold),
                ResourceStat::Gathered,
                ResourceShare::Shared,
            )?;
            let lot = volume.negated_f32();
            self.add_resource(
                player_id,
                commodity,
                Amount::new(lot),
                ResourceStat::None,
                ResourceShare::Shared,
            )?;
        }
        Ok(())
    }

    /// `FUN_141618390`: player `player_id` buys `times` lots of the commodity `attribute`, each its
    /// commodity volume (`+0x140c`, none below 1). A price at most 0 (`COMISS`, `JBE`) ends the
    /// purchases, as does a cost (the lot's worth plus the fee, attribute 78 times it; at least
    /// 21, `MAXSS`) above the player's gold (`JC`: a NaN too). The cost, truncated, goes (slot
    /// 0xf8 with 0 and 1), then the price rises ([`Market::trade`]) and the lot comes in. Not
    /// held: the statistics (`FUN_141778560`) and the local player's telemetry
    /// (`FUN_14097daf0`).
    pub(crate) fn buy(
        &mut self,
        player_id: PlayerId,
        commodity: AttributeId,
        times: TradeLots,
    ) -> GenieResult<()> {
        for _ in 0..times.get() {
            let volume = self.players.player(player_id)?.commodity_volume;
            if !volume.any() {
                return Ok(());
            }
            let gold = self
                .players
                .player(player_id)?
                .attribute(self.model.slots.attribute.gold)
                .unwrap_or(Amount::ZERO);
            let price = self.market.price(commodity, &self.model.roles).get();
            if at_most_or_unordered(price, 0.0) {
                return Ok(());
            }
            let lot = volume.as_f32();
            let worth = lot * price;
            let cost = maxss(
                self.model.tuning.market.least_cost,
                self.fee(player_id) * worth + worth,
            );
            if below_or_unordered(gold.get(), cost) {
                return Ok(());
            }
            let paid = cvttss2si(cost) as f32;
            self.add_resource(
                player_id,
                self.model.slots.attribute.gold,
                Amount::new(-paid),
                ResourceStat::None,
                ResourceShare::Shared,
            )?;
            self.market.trade(
                commodity,
                MarketTrade::Buy,
                &self.model.roles,
                &self.model.tuning.market,
            );
            self.add_resource(
                player_id,
                commodity,
                Amount::new(lot),
                ResourceStat::None,
                ResourceShare::Shared,
            )?;
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
    use crate::testing::attribute;
    use crate::testing::roles;
    use crate::testing::tuning;
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
        let mut market = market();
        market.trade(
            attribute::WOOD,
            MarketTrade::Buy,
            &roles(),
            &tuning().market,
        );
        assert_eq!(
            market.wood,
            Commodity {
                price: Price::new(1.0 + 0.02),
                trades: NetTrades::new(1),
                swing: PriceSwing::new(2.0),
            }
        );
        market.trade(
            attribute::WOOD,
            MarketTrade::Buy,
            &roles(),
            &tuning().market,
        );
        assert_eq!(
            (
                market.wood.price.get(),
                market.wood.trades.get(),
                market.wood.swing.get()
            ),
            (0.02 + (1.0 + 0.02), 2, 4.0)
        );
        market.trade(
            attribute::WOOD,
            MarketTrade::Sell,
            &roles(),
            &tuning().market,
        );
        assert_eq!(
            (
                market.wood.price.get(),
                market.wood.trades.get(),
                market.wood.swing.get()
            ),
            (0.02 + (1.0 + 0.02) - 0.02, 1, 6.0)
        );
        market.trade(
            attribute::WOOD,
            MarketTrade::Sell,
            &roles(),
            &tuning().market,
        );
        assert_eq!(
            market.wood,
            Commodity {
                price: Price::new(1.0),
                trades: NetTrades::new(0),
                swing: PriceSwing::new(0.0),
            }
        );
        // the stone's start, the bounds, another attribute
        market.stone.price = Price::new(9.98);
        market.trade(
            attribute::STONE,
            MarketTrade::Buy,
            &roles(),
            &tuning().market,
        );
        market.trade(
            attribute::STONE,
            MarketTrade::Buy,
            &roles(),
            &tuning().market,
        );
        assert_eq!(market.stone.price.get(), 9.99);
        market.trade(
            attribute::STONE,
            MarketTrade::Sell,
            &roles(),
            &tuning().market,
        );
        market.trade(
            attribute::STONE,
            MarketTrade::Sell,
            &roles(),
            &tuning().market,
        );
        assert_eq!(market.stone.price.get(), 1.3);
        market.food.price = Price::new(0.21);
        market.trade(
            attribute::FOOD,
            MarketTrade::Sell,
            &roles(),
            &tuning().market,
        );
        market.trade(
            attribute::FOOD,
            MarketTrade::Sell,
            &roles(),
            &tuning().market,
        );
        assert_eq!(market.food.price.get(), 0.2);
        let before = market;
        market.trade(
            attribute::HEADROOM,
            MarketTrade::Buy,
            &roles(),
            &tuning().market,
        );
        assert_eq!(
            (market, market.price(attribute::HEADROOM, &roles()).get()),
            (before, -1.0)
        );
        // a swing so large that 2 more does not change it: no step
        market.ore.swing = PriceSwing::new(1.0e8);
        market.ore.trades = NetTrades::new(5);
        market.trade(attribute::ORE, MarketTrade::Buy, &roles(), &tuning().market);
        assert_eq!(
            (market.ore.price.get(), market.ore.swing.get()),
            (1.0, 1.0e8)
        );
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
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 2],
            vec![Player::default(), player],
        );
        world.market = market();
        world
    }

    /// `FUN_141639310`: a sale of 100 wood at 1.0 gives 100 less the fee of 30, plus 0.5,
    /// truncated (70 gold, also counted as gathered gold, 49 and 169), takes the 100 wood and
    /// lowers the price; a second lot the player does not hold sells nothing.
    #[test]
    fn sale_gives_the_lot_worth_less_the_fee_at_the_old_price() {
        let mut world = trading();
        world
            .sell(player_id(1), attribute::WOOD, TradeLots::of_data(2))
            .unwrap();
        let player = &world.players[1];
        assert_eq!(player.resources(), amounts([100.0, 0.0, 100.0, 170.0]));
        assert_eq!(
            (player.attributes[49], player.attributes[169]),
            (Amount::new(70.0), Amount::new(70.0))
        );
        assert_eq!(
            (
                world.market.wood.price.get(),
                world.market.wood.trades.get()
            ),
            (1.0 - 0.02, -1)
        );
    }

    /// `FUN_141618390`: a purchase of 100 food at 1.15 costs 115 plus the fee of 34.5, truncated
    /// (149): the player's 170 gold pays one lot, not the second; the price rises after the
    /// payment. A cheap lot costs at least 21.
    #[test]
    fn purchase_costs_the_lot_worth_and_the_fee_at_least_21() {
        let mut world = trading();
        world.players[1].attributes[attribute::GOLD.index()] = Amount::new(170.0);
        world
            .buy(player_id(1), attribute::FOOD, TradeLots::of_data(2))
            .unwrap();
        let player = &world.players[1];
        assert_eq!(player.resources(), amounts([200.0, 100.0, 100.0, 21.0]));
        assert_eq!(
            (player.attributes[49], player.attributes[169]),
            (Amount::ZERO, Amount::ZERO)
        );
        assert_eq!(
            (
                world.market.food.price.get(),
                world.market.food.trades.get()
            ),
            (0.02 + 1.15, 1)
        );
        let mut world = trading();
        world.players[1].commodity_volume = LotVolume::of_data(10);
        world
            .buy(player_id(1), attribute::WOOD, TradeLots::of_data(1))
            .unwrap();
        assert_eq!(
            world.players[1].attributes[attribute::GOLD.index()],
            Amount::new(100.0 - 21.0)
        );
        // no volume, no purchase
        world.players[1].commodity_volume = LotVolume::of_data(0);
        world
            .buy(player_id(1), attribute::WOOD, TradeLots::of_data(1))
            .unwrap();
        assert_eq!(
            world.players[1].attributes[attribute::GOLD.index()],
            Amount::new(79.0)
        );
    }
}
