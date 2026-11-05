//! A saved combat object's town bell ([`town_bell`]) and the parts that hold its unit AI's
//! order and action values ([`ai_parts`]).

use crate::GenieResult;
use crate::map::Point;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::orders::town_bell::{BellShelter, TownBell};
use crate::unit_ai::{AiAction, UnitAi};
use crate::world::load::object_parts::unit_ai;
use genie_save::generated::world::TribeCombatObject;

/// The parts of a saved object that hold its unit AI's order and action values: the unit AI
/// and the town bell (none for an object without a combat part).
///
/// # Errors
///
/// A value no order or action holds.
pub(super) fn ai_parts(
    object: &genie_save::Object,
    own: ObjectId,
    players: usize,
) -> GenieResult<(Option<UnitAi>, TownBell)> {
    let bell = object.combat().map(town_bell).transpose()?;
    Ok((unit_ai(object, own, players)?, bell.unwrap_or_default()))
}

/// A combat object's town-bell flag and back-to-work records.
///
/// # Errors
///
/// A record's action value no action holds ([`AiAction::try_from`]).
pub(super) fn town_bell(tribe_combat_object: &TribeCombatObject) -> GenieResult<TownBell> {
    use crate::orders::town_bell::BackToWork;
    Ok(TownBell {
        flag: BellShelter::of_byte(tribe_combat_object.town_bell_flag),
        work: BackToWork {
            target: ObjectId::of_field(tribe_combat_object.back_to_work1_town_bell_target_id),
            target_class: ClassId::of_param(
                tribe_combat_object.back_to_work1_town_bell_target_kind,
            )?,
            action: AiAction::of_field(tribe_combat_object.back_to_work1_town_bell_action)?,
            target2: ObjectId::of_field(tribe_combat_object.back_to_work1_town_bell_target_id2),
            point: Point::new(
                tribe_combat_object.back_to_work1_town_bell_target_x,
                tribe_combat_object.back_to_work1_town_bell_target_y,
            ),
        },
        before_building: BackToWork {
            target: ObjectId::of_field(tribe_combat_object.back_to_work2_town_bell_target_id),
            target_class: ClassId::of_param(
                tribe_combat_object.back_to_work2_town_bell_target_kind,
            )?,
            action: AiAction::of_field(tribe_combat_object.back_to_work2_town_bell_action)?,
            target2: ObjectId::of_field(tribe_combat_object.back_to_work2_town_bell_target_id2),
            point: Point::new(
                tribe_combat_object.back_to_work2_town_bell_target_x,
                tribe_combat_object.back_to_work2_town_bell_target_y,
            ),
        },
    })
}
