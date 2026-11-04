//! The world's market as the save holds it ([`market`]).

use crate::players::market::{Commodity, Market};
use crate::players::{NetTrades, Price, PriceSwing};
use genie_save::generated::world::World;

/// The world's market as the save holds it: the save names the int net trades `*_price_delta`
/// (their bits) and the swings `old_*_price_adj`.
#[must_use]
pub fn market(w: &World) -> Market {
    let commodity = |price: f32, trades: f32, swing: f32| Commodity {
        price: Price::new(price),
        trades: NetTrades::new(trades.to_bits().cast_signed()),
        swing: PriceSwing::new(swing),
    };
    Market {
        min_price: Price::new(w.world_min_price),
        max_price: Price::new(w.world_max_price),
        food: commodity(w.world_food_price, w.food_price_delta, w.old_food_price_adj),
        wood: commodity(w.world_wood_price, w.wood_price_delta, w.old_wood_price_adj),
        stone: commodity(
            w.world_stone_price,
            w.stone_price_delta,
            w.old_stone_price_adj,
        ),
        ore: commodity(w.world_ore_price, w.ore_price_delta, w.old_ore_price_adj),
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
        let m = market(&save);
        assert_eq!(m.min_price, Price::new(20.0));
        assert_eq!(m.max_price, Price::new(9999.0));
        assert_eq!(m.food.price, Price::new(100.0));
        assert_eq!(m.food.trades, NetTrades::new(3));
        assert_eq!(m.food.swing, PriceSwing::new(0.5));
        assert_eq!(m.ore.price, Price::new(70.0));
        assert_eq!(m.wood.trades, NetTrades::new(0));
    }
}
