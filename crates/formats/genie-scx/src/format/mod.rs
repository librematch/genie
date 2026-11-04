//! This module contains the data format reading/writing.

#![allow(clippy::cognitive_complexity)]

use byteorder::{LE, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::StringKey;
use std::convert::TryInto;
use std::io::Write;

mod object;
mod rge;
mod scx;
mod tribe_read;
mod tribe_write;

pub use self::object::ScenarioObject;
pub(crate) use self::rge::RgeScen;
pub use self::scx::ScxFormat;
pub use self::tribe_read::TribeScen;

impl TribeScen {
    /// Get the data version of the scenario data.
    #[must_use]
    pub const fn version(&self) -> f32 {
        self.base.version
    }

    /// Get the scenario description (the instructions text), if any.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.base.description.as_deref()
    }
}

fn write_opt_string_key(mut output: impl Write, opt_key: Option<&StringKey>) -> GenieResult<()> {
    output.write_u32::<LE>(if let Some(key) = opt_key {
        key.try_into()?
    } else {
        0xFFFF_FFFF
    })?;
    Ok(())
}

#[cfg(test)]
mod tests;
