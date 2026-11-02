//! The views a unit's update and its unit AI read the world through ([`UnitView`],
//! [`Step`], [`Foreign`], [`OwnerView`], the pickers).

use crate::GenieError;
use crate::map::spatial::Spatial;
use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::objects::unit::Unit;
use crate::pathing::Pathing;
use crate::players::PlayerId;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::others::OthersRef;
use crate::world::player::Player;
use crate::world::slots::PlayerSlots;

mod commands;
mod side;
mod unit_view;
mod world_picker;

pub(crate) use self::commands::{town_centre, town_centre_unit};
pub(crate) use self::world_picker::NOT_HELD;
#[cfg(test)]
use self::world_picker::attacks;
pub(crate) use self::world_picker::{WorldPicker, patrol_information};
#[cfg(test)]
use crate::unit_ai::UnitAi;
use crate::world::step::Step;

/// What a tick's update of one object needs from the world.
/// What a hunt's drop-off ([`Unit::drop_off_in`]) reads besides its unit: the owner's masters,
/// the world with its zone maps and planners, the world's time, the Object stream (the drop-site
/// search's plans) and the owner's type.
pub(crate) struct DropWorld<'a> {
    pub masters: crate::objects::masters::OwnerMasters<'a>,
    pub spatial: &'a mut Spatial,
    pub zones: &'a mut crate::pathing::zones::Zones,
    pub pathing: &'a mut Pathing,
    pub object: &'a mut Well512,
    pub world_time: WorldTime,
    pub computer: bool,
    pub groups: &'a crate::groups::UnitGroups,
}

/// What the unit's slot 0x388 (`FUN_141226fc0`) found while its unit AI ran
/// ([`crate::unit_ai::UnitSide::wait_solidify`]): the foundation, the task its slot 0x820 found
/// on it and the master the search switches the unit to (slot 0x90; None when its own master's
/// list gave the task), which the side makes the unit's at once ([`World::wait_for_foundation`],
/// [`UnitView::lend`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Wait {
    pub(crate) target: crate::construction::FoundationId,
    pub(crate) task: crate::objects::masters::Task,
    pub(crate) switch: Option<MasterId>,
}

/// The unit's repair command (slot 0x378, `FUN_141217c00`) as its unit AI gave it
/// ([`UnitView::repair_command`]), which the side makes at once ([`World::repair_order`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RepairOrder {
    /// The Repair on top takes the target (its slot 0x48, `FUN_140adb480`).
    Retake(ObjectId),
    /// The masters the unit switches to (slot 0x90 with 0): the repairer of its task swap group,
    /// then the one the task search's swap group gave; then a Repair on the task found, if any.
    Start {
        target: ObjectId,
        switches: [Option<MasterId>; 2],
        task: Option<crate::objects::masters::Task>,
    },
}

/// The unit's enter command (slot 0x3a0, `FUN_1410847e0`, forced) as its unit AI gave it
/// ([`UnitView::enter_command`]), which the side makes at once ([`World::enter_order`]): the
/// master the task search's swap group gave (slot 0x90 with 0), then the Enter on the task found
/// into the target (an annex: its main building, the Enter's target), under an Artifact on top
/// or not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EnterOrder {
    pub(crate) target: ObjectId,
    pub(crate) task: crate::objects::masters::Task,
    pub(crate) switch: Option<MasterId>,
    pub(crate) under_artifact: bool,
}

/// The players but the owner of the unit that updates, read-only: the players before it and
/// after it.
#[derive(Clone, Copy)]
pub(crate) struct Foreign<'a> {
    pub(crate) before: &'a [Player],
    pub(crate) after: &'a [Player],
    /// The live list of the player whose list is out, once its object changed hands: that
    /// object's AI part reads it as another player's ([`OthersRef::changed_hands`]).
    pub(crate) left: Option<(PlayerId, &'a crate::objects::object_list::ObjectList)>,
}

impl<'a> Foreign<'a> {
    /// Player `q`, unless it is the one whose list is out.
    pub(crate) fn player(&self, q: PlayerId) -> Option<&'a Player> {
        let q = q.index();
        let own = self.before.len();
        match q.cmp(&own) {
            std::cmp::Ordering::Less => self.before.get(q),
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => self.after.get(q - own - 1),
        }
    }
}

/// What a unit's AI part reads of its owner's side ([`Unit::ai_part`]): the owner, its civ and
/// AI settings, and the player that took the unit over in its update
/// (`new_owner`), whose side it reads (`RGE_Combat_Object::update` reads the owner after the
/// change; [`owner_view`] gives its objects). Its masters, diplomacy and attributes are the
/// world's (the side's diplomacy: [`OwnerView::relations`]).
#[derive(Clone, Copy)]
pub(crate) struct OwnerView {
    pub(crate) player: PlayerId,
    pub(crate) new_owner: Option<PlayerId>,
    pub(crate) civ: crate::players::CivId,
    pub(crate) ai_settings: crate::world::player::AiSettings,
}

impl OwnerView {
    /// The owner's side: player `new`'s when the unit changed hands to it in its update, else
    /// the updating player's (`c`).
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub(crate) fn of(c: &Step<'_>, new: Option<PlayerId>) -> Result<Self, GenieError> {
        let taker = new.filter(|&n| n != c.player);
        let Some((new, q)) = taker.and_then(|n| Some((n, c.w.players.get(n.index())?))) else {
            let owner = c.w.players.player(c.player)?;
            return Ok(Self {
                player: c.player,
                new_owner: None,
                civ: owner.civ,
                ai_settings: owner.ai_settings,
            });
        };
        Ok(Self {
            player: new,
            new_owner: Some(new),
            civ: q.civ,
            ai_settings: q.ai_settings,
        })
    }

    /// The side's diplomacy (`player+0x260`).
    ///
    /// # Errors
    ///
    /// A player the world does not hold.
    pub(crate) fn relations(self, w: &World) -> Result<&crate::players::Relations, GenieError> {
        Ok(&w.players.player(self.player)?.relations)
    }
}

/// The objects as the unit AI of player `p`'s updating unit reads them: the update's view of
/// `players`, or for a unit player `new` took over in its update, that player's live list as its
/// own and the old owner's among the others' ([`OthersRef::changed_hands`]).
pub(crate) fn owner_view<'a>(
    players: &'a [Player],
    index: &'a crate::world::index::ObjectIndex,
    p: PlayerId,
    new: Option<PlayerId>,
) -> Result<OthersRef<'a>, GenieError> {
    Ok(changed_view(OthersRef::of(players, index, p)?, new))
}

/// `view` as the unit AI of a unit player `new` took over reads it ([`owner_view`]).
pub(crate) fn changed_view(view: OthersRef<'_>, new: Option<PlayerId>) -> OthersRef<'_> {
    match new.and_then(|new| Some((new, view.players().player(new)?))) {
        Some((new, q)) => view.changed_hands(new, &q.objects),
        None => view,
    }
}

/// A unit and the world as its unit AI asks them while it runs, the AI taken out of the unit
/// for that time ([`crate::unit_ai::UnitSide`]): the searches and the unit's own commands. The
/// world is the update's step's, the unit out of its slot.
pub(crate) struct UnitView<'u, 's> {
    /// The update's step: the world without the unit (its stand-in holds its slot), the
    /// updating player and the unit's id.
    pub(crate) c: &'u mut Step<'s>,
    /// The unit, without its unit AI.
    pub(crate) unit: &'u mut Unit,
    /// The unit's action layer, out of the table with it.
    pub(crate) actions: &'u mut crate::objects::layers::Action,
    /// What the attack check of a search reads of the unit.
    pub(crate) attacker: crate::unit_ai::Attacker,
    /// The owner, as the AI runs (a captured unit's new one): its diplomacy, attributes and
    /// civ are what the task search reads of it.
    pub(crate) player: PlayerId,
    /// The owner is a computer player.
    pub(crate) computer: bool,
    /// The player that took the unit over in its update ([`changed_view`]).
    pub(crate) new_owner: Option<PlayerId>,
    /// The owner's civ.
    pub(crate) civ: crate::players::CivId,
    /// The owner's AI settings.
    pub(crate) ai_settings: crate::world::player::AiSettings,
}

#[cfg(test)]
mod tests;
