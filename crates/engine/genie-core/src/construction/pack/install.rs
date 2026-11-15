//! A Pack's making and its start: the constructor with its unit AI's order, the install through
//! slot 0x808, the start (slot 0x28) and `set_state` with its construction sprite and the
//! annexes it destroys, and the target it takes.

use crate::GenieError;
use crate::GenieResult;
use crate::commands::QueueId;
use crate::construction::pack::{PackAction, PackState, pack_time};
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::Voice;
use crate::objects::layers::Action;
use crate::objects::masters::MasterKind;
use crate::objects::teardown::HeadStart;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

/// A Pack's slot 0x68 or `set_state` on a unit's list that holds no Pack: the install puts the
/// Pack it starts at the head, and its start destroys only other objects.
const PACK_OFF_LIST: GenieError =
    GenieError::unreachable("a Pack's start or take of its target without the Pack on its list");
/// A Pack that the list's add deleted unstarted, whose slot 0x68 the order would call through a
/// freed block (`0x14122736c`): the add deletes it only for a unit in state 3 or later, which
/// slot 0x7e8 turns away first (`CMP byte [RBX+0x640], 2`, `JA`).
const PACK_DELETED_UNSTARTED: GenieError =
    GenieError::unreachable("a Pack's take of its target after the add deleted it");

impl World {
    /// The Pack's constructor (`FUN_140ad4ff0`) for player `player`'s unit at `idx` with the
    /// order's queue id `unique_id` (−1 at slot 0x7e8): its own part ([`PackAction::new`]) with
    /// the share its unit kept (`+0x88a`, [`Action::pack_progress`]) of its master's pack time;
    /// then a unit AI (`+0x6b0`) takes the order on a target (`FUN_140c64b60` with `0x2db` and
    /// 1, [`UnitAi::set_order`]) and the action on a target (`FUN_140c64b50` with `0x277`,
    /// [`UnitAi::set_action`]). Every site makes it before slot 0x808's deletion, so the old
    /// actions' destructors and restarts see that order and action.
    ///
    /// [`UnitAi::set_order`]: crate::unit_ai::UnitAi::set_order
    /// [`UnitAi::set_action`]: crate::unit_ai::UnitAi::set_action
    ///
    /// # Errors
    ///
    /// A unit the world does not hold live, or of a master it lacks.
    pub(crate) fn construct_pack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        unique_id: QueueId,
    ) -> GenieResult<PackAction> {
        let unit = self.players.live(player, idx)?;
        let id = unit.id;
        let time = pack_time(unit.master(self.master_tables(), player)?);
        let pack = PackAction::new(unique_id, time, self.parts.actions_of(id).pack_progress);
        let rng = self.streams.get_mut(Stream::Ai);
        if let Some(ai) = self.parts.unit_ai.get_mut(id) {
            ai.set_order(Some(OrderKind::OnTarget), rng);
            ai.set_action(Some(AiAction::OnTarget));
        }
        Ok(pack)
    }

    /// A new Pack on player `player`'s unit at `idx` with the queue id `unique_id`, as slot
    /// 0x7e8 (`FUN_141227190`) installs it: the constructor ([`World::construct_pack`]), then
    /// slot 0x808 ([`World::set_only_action`]); the list holds the Pack it hands back at its head
    /// and starts it (slot 0x28 with 1, [`Unit::start_pack`], on the unit's step). With a
    /// `target` the Pack then takes it (its slot 0x68, [`World::pack_takes`]). A Pack that the
    /// add deleted unstarted starts nothing; the game calls slot 0x808 with what the allocation
    /// gave, which genie's allocation does not fail.
    ///
    /// # Errors
    ///
    /// The constructor's, the install's, the start's and the take's; a target after the add
    /// deleted the Pack ([`PACK_DELETED_UNSTARTED`], unreachable).
    pub(crate) fn install_pack(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        unique_id: QueueId,
        target: Option<ObjectId>,
    ) -> GenieResult<()> {
        let pack = self.construct_pack(player, idx, unique_id)?;
        let Some(pack) = self.set_only_action(player, idx, Some(pack))? else {
            return target.map_or(Ok(()), |target| {
                Err(PACK_DELETED_UNSTARTED.with("target", target))
            });
        };
        let id = self.players.live(player, idx)?.id;
        self.parts.actions_mut(id)?.list.set_pack(Some(pack));
        self.run_on_step(player, idx, |unit, actions, step| {
            unit.start_pack(HeadStart::Added, actions, step)
        })?;
        if let Some(target) = target {
            self.pack_takes(player, idx, target)?;
        }
        Ok(())
    }

    /// Slot 0x340's Pack of an unpacked trebuchet (`FUN_141216c50`,
    /// [`MoveWith::Packs`]) on player `player`'s unit at `idx`: the constructor with the queue id
    /// −1 (`MOV R8D, -1` at `0x141216e87`), slot 0x808 (`0x141216eaa`) and the start, with no
    /// target ([`World::install_pack`]).
    ///
    /// [`MoveWith::Packs`]: crate::movement::ai_moves::MoveWith::Packs
    ///
    /// # Errors
    ///
    /// The install's.
    pub(crate) fn install_pack_to_move(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
    ) -> GenieResult<()> {
        self.install_pack(player, idx, QueueId::NONE, None)
    }

    /// The Pack's slot 0x68 (`RGE_Action`'s `FUN_140a7a230`) with `target` once slot 0x808
    /// added it (`0x14122736c`) on player `player`'s unit at `idx`: its target becomes `target`
    /// ([`World::retarget_action`]: none past state 6, else taken by slot 0x178, as the Pack is
    /// loud).
    ///
    /// # Errors
    ///
    /// A unit the world does not hold live, a list without its Pack ([`PACK_OFF_LIST`],
    /// unreachable), and the change's.
    fn pack_takes(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> GenieResult<()> {
        let id = self.players.live(player, idx)?.id;
        let old = self
            .parts
            .actions_of(id)
            .list
            .pack()
            .ok_or(PACK_OFF_LIST)?
            .target;
        let held = self.retarget_action(player, idx, old, target, Voice::Loud)?;
        self.parts
            .actions_mut(id)?
            .list
            .pack_mut()
            .ok_or(PACK_OFF_LIST)?
            .target = held;
        Ok(())
    }
}

impl Unit {
    /// The Pack's slot 0x28 (`0x140ad6150`, `TEST DL, DL`): with 1, the list's add
    /// ([`HeadStart::Added`]), it goes to work (`set_state(6)`, [`Unit::set_pack_state`]); with
    /// 0, a restart under a deleted head ([`HeadStart::Restarted`]), it does nothing.
    ///
    /// # Errors
    ///
    /// The `set_state`'s.
    pub(crate) fn start_pack(
        &mut self,
        start: HeadStart,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        match start {
            HeadStart::Added => self.set_pack_state(PackState::Working, actions, step),
            HeadStart::Restarted => Ok(()),
        }
    }

    /// `set_state` (slot 0x80, `0x140ad68e0`) of the Pack on the unit's list: its state (`+0x18`)
    /// becomes `state`; with 6 it goes to work ([`PackAction::go_to_work`]: its status 1, its
    /// timer from the share the unit kept, `+0x88a`, of its master's pack time), the building
    /// shows its master's construction sprite (the unit's slot 0x68 with `+0x950`,
    /// [`Unit::show_construction`]) and its annexes that are buildings go
    /// ([`Unit::destroy_building_annexes`]). The game writes the status before the sprite and the
    /// timer after it, which reads neither. Any other state is the state alone, as
    /// [`PackAction::finish`] writes it outside the unit's step.
    ///
    /// # Errors
    ///
    /// A list without a Pack ([`PACK_OFF_LIST`], unreachable), a unit of a master the world
    /// lacks, and the sprite's and the annexes'.
    pub(crate) fn set_pack_state(
        &mut self,
        state: PackState,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let pack = actions.list.pack_mut().ok_or(PACK_OFF_LIST)?;
        if state != PackState::Working {
            pack.state = state;
            return Ok(());
        }
        let time = pack_time(self.master(step.world.master_tables(), step.player)?);
        pack.go_to_work(time, actions.pack_progress);
        self.show_construction(step)?;
        self.destroy_building_annexes(actions, step)
    }

    /// `set_state(6)`'s annexes (`0x140ad696c` to `0x140ad6998`): each of the building's four
    /// slots (`+0xbc0` to `+0xbd8`, [`Building::annexes`]) whose object is a building (`CMP byte
    /// [RCX+0xf], 0x50`) takes slot 0xd0 (`TRIBE_Building_Object`'s `FUN_1411cfb30`,
    /// [`World::destroy`]), the building lent back to its slot; an annex of another type stays.
    /// A destroy writes no slot of its main building (its update lets an annex past state 6 go),
    /// so the slots are read once. An annex the world no longer holds is passed over, as the
    /// building's other walks of its annexes do ([`Unit::annexes_follow`]).
    ///
    /// [`Building::annexes`]: crate::objects::layers::Building::annexes
    ///
    /// # Errors
    ///
    /// An annex of a master the world lacks, and the destroys'.
    fn destroy_building_annexes(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let annexes = step.world.parts.building_of(self.id).annexes;
        for annex in annexes.ids() {
            let Some((owner, unit)) = step.world.object(annex) else {
                continue;
            };
            if unit.master(step.world.master_tables(), owner)?.kind != MasterKind::BUILDING {
                continue;
            }
            step.lend((&mut *self, &mut *actions), |world| {
                world.destroy(owner, annex)
            })?;
        }
        Ok(())
    }
}
