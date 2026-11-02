//! An object's state code (`ObjectState`) and what an object's life and death read of its master
//! (`MasterData`).

pub mod master_data;
pub mod object_state;
pub use self::master_data::MasterData;
pub use self::object_state::ObjectState;
