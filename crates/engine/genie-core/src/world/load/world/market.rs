//! The world's market as the save holds it ([`market`]).

use crate::players::market::{Commodity, Market};
use crate::players::{NetTrades, Price, PriceSwing};
use genie_save::generated::world::World;

/// The world's market as the save holds it: the save names the int net trades `*_price_delta`
/// (their bits) and the swings `old_*_price_adj`.
#[must_use]
pub fn market(world: &World) -> Market {
    let commodity = |price: f32, trades: f32, swing: f32| Commodity {
        price: Price::new(price),
        trades: NetTrades::new(trades.to_bits().cast_signed()),
        swing: PriceSwing::new(swing),
    };
    Market {
        min_price: Price::new(world.world_min_price),
        max_price: Price::new(world.world_max_price),
        food: commodity(
            world.world_food_price,
            world.food_price_delta,
            world.old_food_price_adj,
        ),
        wood: commodity(
            world.world_wood_price,
            world.wood_price_delta,
            world.old_wood_price_adj,
        ),
        stone: commodity(
            world.world_stone_price,
            world.stone_price_delta,
            world.old_stone_price_adj,
        ),
        ore: commodity(
            world.world_ore_price,
            world.ore_price_delta,
            world.old_ore_price_adj,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A commodity takes its price, its swing and its net trades as the bits of the saved float.
    #[test]
    fn the_net_trades_are_the_saved_floats_bits() {
        let save = World {
            world_min_price: 20.0,
            world_max_price: 9999.0,
            world_food_price: 100.0,
            food_price_delta: f32::from_bits(3),
            old_food_price_adj: 0.5,
            world_ore_price: 70.0,
            ..World::default()
        };
        let market = market(&save);
        assert_eq!(market.min_price, Price::new(20.0));
        assert_eq!(market.max_price, Price::new(9999.0));
        assert_eq!(market.food.price, Price::new(100.0));
        assert_eq!(market.food.trades, NetTrades::new(3));
        assert_eq!(market.food.swing, PriceSwing::new(0.5));
        assert_eq!(market.ore.price, Price::new(70.0));
        assert_eq!(market.wood.trades, NetTrades::new(0));
    }
}
