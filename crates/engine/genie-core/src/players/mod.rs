//! The players beside their objects: their game status (playing, won, lost), diplomacy,
//! tribute, treason, the treaty, the market, their history, their object trackers, and the
//! world player's own update.

pub mod ai_module;
use genie_model::players::amount;
use genie_model::players::amount_rate;
pub use genie_model::players::attribute;
mod by_player;
mod chance;
use genie_model::players::civ_id;
mod civ_name;
mod culture;
pub mod diplomacy;
mod escrow_share;
pub mod game_status;
pub mod history;
pub mod market;
mod market_values;
mod player_id;
mod player_kind;
mod player_set;
mod posture;
mod query_relation;
mod relation;
mod resource_stat;
mod stance;
mod team;
pub mod tracker;
pub mod treason;
pub mod treaty;
pub mod tribute;
mod tribute_fee;
mod unit_relation;
pub mod world_player;

pub use self::amount::Amount;
pub use self::amount_rate::AmountRate;
pub use self::by_player::ByPlayer;
pub use self::chance::Chance;
pub use self::civ_id::CivId;
pub use self::civ_name::{CivNameOverride, StringId};
pub use self::culture::Culture;
pub use self::escrow_share::EscrowShare;
pub use self::market_values::{LotVolume, MarketTrade, NetTrades, Price, PriceSwing};
pub use self::player_id::PlayerId;
pub use self::player_kind::PlayerKind;
pub use self::player_set::PlayerSet;
pub use self::posture::Posture;
pub use self::query_relation::QueryRelation;
pub(crate) use self::relation::NO_RELATIONS;
#[cfg(test)]
pub(crate) use self::relation::relation_row;
pub use self::relation::{AllyTest, Relation, Relations};
pub use self::resource_stat::ResourceStat;
pub use self::stance::Stance;
pub use self::team::Team;
pub use self::tribute_fee::TributeFee;
#[cfg(test)]
pub(crate) use self::unit_relation::relations;
pub use self::unit_relation::{
    ByRelation, RelationMask, UnitDiplomacy, UnitRelation, UnitRelations,
};
