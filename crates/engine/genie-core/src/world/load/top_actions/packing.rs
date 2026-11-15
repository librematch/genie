//! The siege's own actions of a saved object's top action: the Pack and the Unpack
//! ([`crate::world::load::top_actions::saved_actions`]).

use crate::commands::QueueId;
use crate::construction::pack::{PackAction, PackState, PackStatus};
use crate::construction::unpack::{UnpackAction, UnpackState, UnpackStatus};
use crate::map::Distance;
use crate::map::Point;
use crate::objects::ObjectId;
use crate::objects::actions::Action as ActionsAction;
use crate::world::Seconds;
use crate::world::load::top_actions::{Saved, saved_sub_move_to};
use crate::{GenieError, GenieResult};

/// `TRIBE_Action_Pack`'s load (`FUN_140ad5090`): the base action's state and target (the base's
/// load with 1, `FUN_140a73f00`), the order's queue id (`unique_id`, `+0x6c`), the status
/// (`m_packStatus`, `+0x68`) and the timer (`mTimer`, `+0x44`, which the reader holds from save
/// version `0xa451e` on: every save it reads). Its last writes, the order 0x2db and the action
/// 0x277 to its unit's unit AI (`FUN_140c64b60`, `FUN_140c64b50`), find none: the combat
/// object's load (`FUN_14121ec90`) clears `+0x6b0` (`0x14121ecd5`) before its base's
/// (`FUN_141089880`) reads the action list, and makes the unit AI after it. No slot of the Pack's
/// makes a sub-action, so a save that holds one is no game's.
pub(super) fn import_pack(saved: &Saved<'_>) -> GenieResult<ActionsAction> {
    if saved.base.sub_actions.entries().next().is_some() {
        return Err(GenieError::invalid_input("a saved Pack with a sub-action"));
    }
    let part = saved
        .action
        .tribe_action_pack
        .as_deref()
        .ok_or(GenieError::unreachable(
            "a saved Pack without its own part, which the reader gives every action of type 0x7c",
        ))?;
    Ok(ActionsAction::from(PackAction {
        state: PackState::of_byte(saved.base.state)?,
        target: ObjectId::of_field(saved.base.target_id),
        timer: Seconds::new(part.timer),
        status: PackStatus::of_byte(part.pack_status)?,
        unique_id: QueueId::of_data(part.unique_id),
    }))
}

/// `TRIBE_Action_Unpack`'s load (`FUN_140ae59d0`): the base action's state, target and point
/// (`+0x38`, `+0x3c`) with its sub-action list (the base's load with 1, `FUN_140a73f00`): state
/// 11's quiet `MoveTo` (`FUN_140a7a170` with 1); the order's queue id (`unique_id`, `+0x68`), the
/// status (`m_unpackStatus`, `+0x6c`), the range (`+0x70`, which the reader holds from save
/// version `0x96666` on: every save it reads; `DAT_143cd8948` below it leaves 0) and the timer
/// (`mTimer`, `+0x44`, from `0xa451e` on). It writes nothing to its unit's AI. `set_state` makes
/// no other sub-action, and an Unpack at work (status 1) walks no more: a save that holds either
/// is no game's.
pub(super) fn import_unpack(saved: &Saved<'_>) -> GenieResult<ActionsAction> {
    let sub = saved.move_to_sub(GenieError::invalid_input(
        "a saved Unpack with another sub-action",
    ))?;
    let part = saved
        .action
        .tribe_action_unpack
        .as_deref()
        .ok_or(GenieError::unreachable(
            "a saved Unpack without its own part, which the reader gives every action of type 0x7d",
        ))?;
    let status = UnpackStatus::of_byte(part.unpack_status)?;
    if status == UnpackStatus::Unpacking && sub.is_some() {
        return Err(GenieError::invalid_input("a saved Unpack walking at work"));
    }
    let walk = sub
        .map(|action| saved_sub_move_to(action, saved.graphics))
        .transpose()?;
    Ok(ActionsAction::from(UnpackAction {
        state: UnpackState::of_byte(saved.base.state, walk)?,
        target: ObjectId::of_field(saved.base.target_id),
        point: Point::new(saved.base.target_x, saved.base.target_y),
        timer: Seconds::new(part.timer),
        unique_id: QueueId::of_data(part.unique_id),
        status,
        range: Distance::new(part.range),
    }))
}
