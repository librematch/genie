//! The body's frames, and from the save: a `MoveTo`'s wait, the world's market and a master's
//! combat part.

use crate::Error;
use genie_core::movement::move_to::MoveWait;
use genie_core::players::market::{Commodity, Market};
use genie_core::players::{NetTrades, Price, PriceSwing};
use genie_core::world::Millis;
use genie_core::world::Seconds;
use genie_rec::de;
use genie_rec::de::Op;
use genie_save::generated::world::Master;
use genie_save::generated::world::{Action, RgeMasterCombatObject, World};

/// One frame of the body (FD-007's per-frame record order): the op-2 record that starts a tick, its
/// dt and, every interval, a checkpoint, then the op-1 commands the tick runs after its `SeedAll`.
pub(super) struct Frame<'a> {
    pub(super) dt: Millis,
    pub(super) checkpoint: Option<Box<de::Checkpoint>>,
    pub(super) commands: Vec<&'a [u8]>,
}

/// The chapter's first `limit` frames.
pub(super) fn frames<'a>(chapter: &de::Chapter<'a>, limit: usize) -> Result<Vec<Frame<'a>>, Error> {
    let mut out: Vec<Frame<'a>> = Vec::new();
    for op in chapter.ops() {
        let (dt, checkpoint) = match op? {
            Op::Sync { dt } => (dt, None),
            Op::Checkpoint { dt, checkpoint } => (dt, Some(checkpoint)),
            Op::Command { packet, .. } => {
                out.last_mut()
                    .ok_or(Error::CommandBeforeFirstSync)?
                    .commands
                    .push(packet);
                continue;
            }
            _ => continue,
        };
        if out.len() == limit {
            break;
        }
        out.push(Frame {
            dt: Millis::new(dt),
            checkpoint,
            commands: Vec::new(),
        });
    }
    Ok(out)
}

/// A saved `MoveTo`'s wait (`+0x6c`, `+0x70`, `+0x74`); none for another sub-action.
///
/// # Errors
///
/// A saved wait state that names no state.
pub(super) fn saved_move_wait(sub: &Action) -> Result<MoveWait, genie_core::GenieError> {
    use genie_core::movement::move_to::{MoveWait, MoveWaitState};
    sub.rge_action_by_type
        .as_deref()
        .and_then(|t| t.rge_action_move_to.as_deref())
        .map_or_else(
            || Ok(MoveWait::default()),
            |m| {
                Ok(MoveWait {
                    state: MoveWaitState::of_byte(m.movement_wait_state)?,
                    timer: Seconds::new(m.timer),
                    delay: Seconds::new(f32::from_bits(m.default_wait_time)),
                })
            },
        )
}

/// The world's market as the save holds it: the save names the int net trades `*_price_delta`
/// (their bits) and the swings `old_*_price_adj`.
pub(super) fn market(w: &World) -> Market {
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

pub(super) fn combat_master(m: &Master) -> Option<&RgeMasterCombatObject> {
    None.or_else(|| {
        m.tribe_master_combat_object
            .as_deref()
            .map(|c| &c.rge_master_combat_object)
    })
    .or_else(|| {
        m.tribe_master_building_object
            .as_deref()
            .map(|b| &b.tribe_master_combat_object.rge_master_combat_object)
    })
    .or_else(|| {
        m.rge_master
            .as_deref()
            .and_then(|r| r.rge_master_combat_object.as_deref())
    })
    .or_else(|| {
        m.rge_master.as_deref().and_then(|r| {
            r.rge_master_missile_object
                .as_deref()
                .map(|m| &m.rge_master_combat_object)
        })
    })
}

#[cfg(test)]
mod tests;
