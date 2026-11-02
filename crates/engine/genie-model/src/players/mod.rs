//! The players' game data: their attributes (resources and the values technologies change),
//! amounts, civilisations and the world player's tech tree.

pub mod amount;
pub mod amount_rate;
pub mod attribute;
pub mod civ_id;
pub mod history;
pub mod world_player;
pub use self::amount::Amount;
pub use self::amount_rate::AmountRate;
pub use self::civ_id::CivId;
