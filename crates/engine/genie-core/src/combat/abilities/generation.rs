//! The generation tasks (action type 151): their test, what they make and the player's store.

use crate::GenieResult;
use crate::combat::abilities::{ONE, Own, WORKING, WORKING_ACTIONS};
use crate::gathering::gather::unit::give_load;
use crate::math::fmath::below_or_unordered;
use crate::objects::ActionState;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Task;
use crate::objects::masters::TaskFlags;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::vision::visibility::BuildingSight;
use crate::world::AllySharing;
use crate::world::Seconds;
use crate::world::Unread;
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// What a generation task's test reads of the object's top action (`FUN_140a986f0`): its type
/// (`+0x8`), its state (`+0x18`) and its target as the world holds it (`FUN_140a78450`), None
/// when the world holds no object of its id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopWork {
    /// The action's type.
    pub(crate) kind: ActionKind,
    /// The action's state.
    pub(crate) state: ActionState,
    /// Its target.
    pub(crate) target: Option<WorkTarget>,
}

/// What a generation task's test reads of the top action's target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkTarget {
    /// Its hit points (`+0x278`).
    pub(crate) hp: HitPoints,
    /// Its master's first storage amount (`+0x610`).
    pub(crate) storage: Amount,
    /// Its master's class (`+0x410`).
    pub(crate) class: ClassId,
}

/// Whether a generation task generates this update (`0x141207d4d`..`0x141207fbe`): an object
/// of type 70 (`+0xf`) whose task has [`TaskFlags::UNITS_ALWAYS`], or of type 80 with
/// [`TaskFlags::BUILDINGS_ALWAYS`], does;
/// otherwise one whose master is of type 70 does while its top action is one of
/// [`WORKING_ACTIONS`] in state 7 with a target the world holds that has hit points above 0 or
/// whose master stores some (`COMISS`/`JA`, `JBE`: a NaN does not) and whose master's class is
/// the task's `object_group`, and one whose master is of type 80 does while it is built
/// (`+0xca1` 1: its sight, `building`, is [`BuildingSight::Built`]).
#[must_use]
pub fn generates(
    task: &Task,
    kind: MasterKind,
    master_kind: MasterKind,
    top: Option<TopWork>,
    building: Option<BuildingSight>,
) -> bool {
    match kind {
        MasterKind::COMBAT if task.combat_level_flag.contains(TaskFlags::UNITS_ALWAYS) => {
            return true;
        }
        MasterKind::BUILDING if task.combat_level_flag.contains(TaskFlags::BUILDINGS_ALWAYS) => {
            return true;
        }
        _ => {}
    }
    if master_kind == MasterKind::COMBAT
        && let Some(top) = top
        && WORKING_ACTIONS.contains(&top.kind)
        && top.state == WORKING
        && let Some(work_target) = top.target
        && (HitPoints::ZERO < work_target.hp || Amount::ZERO < work_target.storage)
        && task.for_class(work_target.class)
    {
        return true;
    }
    master_kind == MasterKind::BUILDING && building == Some(BuildingSight::Built)
}

/// What a generation task makes in an update (`0x141207fc4`): the world's dt (`world+0x164`)
/// times the task's `work_val1` times the player's attribute the task's `attribute_type2` names,
/// in that order (`MULSS`, `MULSS`).
#[must_use]
pub fn generated(dt: Seconds, task: &Task, multiplier: Amount) -> Amount {
    Amount::new(dt.get() * task.work_val1.get() * multiplier.get())
}

/// The player's store of what its generation tasks made (`player+0x658`): `amount` above 0 (not
/// a NaN) joins the store of attribute `out` (`ADDSS`: the amount plus the store); the store, or
/// the amount alone when it did not join, once at least 1 (`COMISS`/`JC`: a NaN is below) goes to
/// the player (returned, the caller's slot 0xf8) and the store starts again at 0. A store the
/// player does not have (an attribute past its count, which no `.dat` task names) takes nothing;
/// a task without an attribute (−1) has no store, which the caller passes over.
pub fn store(amount: Amount, out: AttributeId, stores: &mut [Amount]) -> Option<Amount> {
    let s = stores.get_mut(out.index())?;
    let mut amount = amount;
    if Amount::ZERO < amount {
        *s += amount;
        amount = *s;
    }
    if below_or_unordered(amount.get(), ONE) {
        return None;
    }
    *s = Amount::ZERO;
    Some(amount)
}

impl World {
    /// A generation task of the object at `(player_id, list_index)` as `owner`'s ([`generates`],
    /// [`generated`], [`store`]): what reaches the store's threshold goes to the player (slot 0xf8
    /// with 1 and 1, as a load's, [`crate::gathering::gather::unit::give_load`]); the statistics
    /// (`FUN_14097daf0`, the local player's, and `FUN_140a4c020`'s telemetry) are not held. The top
    /// action's target is read as the world holds it after the update (its hits wrote their victims
    /// back where they ran).
    ///
    /// # Errors
    ///
    /// The top action of a unit whose action the slice does not model, and a give to a player
    /// who shares with allies ([`crate::gathering::gather::unit::give_load`]).
    pub(super) fn generate(
        &mut self,
        (player_id, list_index): (PlayerId, ListIndex),
        owner: PlayerId,
        master: Own,
        task: &Task,
    ) -> GenieResult<()> {
        let unit = self.players.live(player_id, list_index)?;
        let flags = task.combat_level_flag;
        let short_cut = match master.kind {
            MasterKind::COMBAT => flags.contains(TaskFlags::UNITS_ALWAYS),
            _ => flags.contains(TaskFlags::BUILDINGS_ALWAYS),
        };
        let top = if short_cut || master.kind != MasterKind::COMBAT {
            None
        } else {
            if let Some(error) = self.parts.unread.get(unit.id).map(Unread::error) {
                return Err(error);
            }
            match self.parts.actions_of(unit.id).top_action() {
                Some(top_action) => {
                    // FUN_140a78450: an id below 0 names no object; the target only matters
                    // to an action of the test's at work
                    let works =
                        WORKING_ACTIONS.contains(&top_action.kind) && top_action.state == WORKING;
                    let target = match self
                        .parts
                        .actions_of(unit.id)
                        .top_target()
                        .filter(|t| works && !t.is_recyclable())
                    {
                        Some(object_id) => self.work_target(object_id)?,
                        None => None,
                    };
                    Some(TopWork {
                        kind: top_action.kind,
                        state: top_action.state,
                        target,
                    })
                }
                None => None,
            }
        };
        // the sight of a master not of type 80 is never read (`generates` tests the type first)
        let building = self
            .spatial
            .sight
            .as_ref()
            .and_then(|visibility| visibility.seer(unit.id))
            .and_then(|seer| seer.building);
        if !generates(task, master.kind, master.kind, top, building) {
            return Ok(());
        }
        let player = self.players.player_mut(owner)?;
        let multiplier = (task.attribute_kind2)
            .and_then(|attribute_id| player.attribute(attribute_id))
            .unwrap_or(Amount::ZERO);
        let amount = generated(self.dt_seconds, task, multiplier);
        // no store without the task's attribute: nothing reaches the player
        let Some(out) = task.attribute_kind3 else {
            return Ok(());
        };
        let Some(give) = store(amount, out, &mut player.generated) else {
            return Ok(());
        };
        let sharing = player.ally_sharing.contains(AllySharing::RESOURCES);
        let mut adds = Vec::new();
        give_load(
            (&self.spatial.relations, sharing),
            owner,
            &mut adds,
            out,
            give,
            &self.model.roles,
            self.model.slots,
        )?;
        for (kind, amount) in adds {
            self.players
                .player_mut(owner)?
                .add_attribute(kind, amount)?;
        }
        Ok(())
    }

    /// What a generation task's test reads of the object `id` its top action works on, as the
    /// world holds it; None for an object the world does not hold.
    pub(super) fn work_target(&self, id: ObjectId) -> GenieResult<Option<WorkTarget>> {
        let Some((player_id, unit)) = self.object(id) else {
            return Ok(None);
        };
        let master = unit.master(self.master_tables(), player_id)?;
        Ok(Some(WorkTarget {
            hp: unit.life.hp,
            storage: master.storage.first().map_or(Amount::ZERO, |s| s.1),
            class: master.class,
        }))
    }
}
