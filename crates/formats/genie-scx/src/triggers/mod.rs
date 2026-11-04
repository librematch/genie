use byteorder::{LE, WriteBytesExt};
use genie_error::GenieResult;
use genie_support::StringKey;
use std::convert::TryInto;
use std::io::Write;

mod condition;
mod effect;
mod trigger;

pub use self::condition::TriggerCondition;
pub use self::effect::TriggerEffect;
pub use self::trigger::{Trigger, TriggerSystem};

fn write_opt_string_key(mut output: impl Write, opt_key: Option<&StringKey>) -> GenieResult<()> {
    output.write_u32::<LE>(if let Some(key) = opt_key {
        key.try_into()?
    } else {
        0xFFFF_FFFF
    })?;
    Ok(())
}
