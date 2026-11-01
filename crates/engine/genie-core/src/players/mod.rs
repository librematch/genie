//! The players beside their objects: their game status (playing, won, lost), diplomacy,
//! tribute, treason, the market, their history, their object trackers, and the world player's
//! own update.

pub mod ai_module;
mod amount;
mod amount_rate;
pub mod attribute;
mod by_player;
mod chance;
mod civ_id;
pub mod diplomacy;
mod escrow_share;
pub mod game_status;
pub mod history;
pub mod market;
mod market_values;
mod player_id;
mod player_set;
mod player_type;
mod query_relation;
mod relation;
mod stance;
mod team;
pub mod tracker;
pub mod treason;
pub mod tribute;
mod unit_relation;
pub mod world_player;

pub use self::amount::Amount;
pub use self::amount_rate::AmountRate;
pub use self::by_player::ByPlayer;
pub use self::chance::Chance;
pub use self::civ_id::CivId;
pub use self::escrow_share::EscrowShare;
pub use self::market_values::{NetTrades, Price, PriceSwing};
pub use self::player_id::PlayerId;
pub use self::player_set::PlayerSet;
pub use self::player_type::PlayerType;
pub use self::query_relation::QueryRelation;
pub(crate) use self::relation::NO_RELATIONS;
#[cfg(test)]
pub(crate) use self::relation::relation_row;
pub use self::relation::{Relation, Relations};
pub use self::stance::Stance;
pub use self::team::Team;
#[cfg(test)]
pub(crate) use self::unit_relation::relations;
pub use self::unit_relation::{ByRelation, UnitRelation, UnitRelations};
