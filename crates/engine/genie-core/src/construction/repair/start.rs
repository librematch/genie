//! The Repair's constructor on a target, its install through slot 0x808 and its start
//! (slot 0x28): a walk to the target it holds, else the wait for work.

use crate::GenieResult;
use crate::construction::RepairTargetId;
use crate::construction::repair::unit::counted_object_of;
use crate::construction::repair::{REPAIR_TARGET_UNHELD, RepairAction, RepairState, RepairTask};
use crate::map::Distance;
use crate::objects::ListIndex;
use crate::objects::Voice;
use crate::objects::actions::LAST_TAKEN;
use crate::objects::teardown::HeadStart;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::others::OthersRef;
use crate::world::step::Step;

impl RepairAction {
    /// The constructor on a target (`FUN_140ad7fa0`: the factory's, `FUN_140aca4b0` case 0x6a
    /// with a target, and the repair command's, `FUN_141217c00`): the base constructor
    /// (`FUN_140a73e90` with 1), the task `task` (`+0x48`), not counted among the repairers
    /// (`+0x6c` 0), loud, its `MoveTo`'s range `range` (the unit's master's weapon range); the
    /// target (`FUN_140a7a230`) with its place as the point. The take (slot 0x178) is the
    /// caller's, and so is a ship's meeting point (`FUN_140ad9120`,
    /// [`crate::construction::repair::SHIP_REPAIR`]). The game's constructor leaves the state 0,
    /// which nothing reads before the install's start ([`Unit::start_repair`]); genie holds 3 until
    /// then.
    ///
    /// # Errors
    ///
    /// A target past [`LAST_TAKEN`], whose master `FUN_140ad9120` reads through the null target
    /// that `FUN_140a7a230` left ([`REPAIR_TARGET_UNHELD`]: the game faults there).
    pub fn new(task: RepairTask, range: Distance, target: &Unit) -> GenieResult<Self> {
        if LAST_TAKEN < target.life.state {
            return Err(REPAIR_TARGET_UNHELD
                .with("target", target.id)
                .with("state", target.life.state));
        }
        Ok(Self {
            target: Some(RepairTargetId::new(target.id)),
            state: RepairState::Idle,
            point: target.position(),
            timer: Seconds::ZERO,
            task,
            range,
            counted: false,
            voice: Voice::Loud,
        })
    }
}

impl World {
    /// Slot 0x808 with the new `repair` on player `player`'s unit at `idx`
    /// ([`World::set_only_action`]): the list's deletion, then its add, which holds the Repair at
    /// its head and starts it (slot 0x28 with 1, [`Unit::start_repair`] with
    /// [`HeadStart::Added`]) on the unit's step. A Repair that the add deleted unstarted (a unit in
    /// state 3 or later) starts nothing.
    ///
    /// # Errors
    ///
    /// The install's and the start's.
    pub(crate) fn install_repair(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        repair: RepairAction,
    ) -> GenieResult<()> {
        let Some(mut repair) = self.set_only_action(player, idx, Some(repair))? else {
            return Ok(());
        };
        self.run_on_step(player, idx, |unit, actions, step| {
            let started = unit.start_repair(&mut repair, HeadStart::Added, step);
            actions.list.set_repair(Some(repair));
            started
        })
    }
}

impl Unit {
    /// The Repair's slot 0x28 (`FUN_140ad9030`, `TEST DL, DL`): with 1, the list's add
    /// ([`HeadStart::Added`]), a Repair that holds its target (`+0x20`) walks to it
    /// (`set_state(4)`, [`RepairAction::set_state`] with what its update reads as it starts,
    /// [`Unit::repair_reads`]); one without a target and, with 0, one that a deletion left at the
    /// head ([`HeadStart::Restarted`]) wait for work (`set_state(3)`, [`RepairAction::idle`]) with
    /// the one value that case reads where the unit stands, the object its repairers count in
    /// ([`counted_object_of`]), none for a target the world does not hold.
    ///
    /// # Errors
    ///
    /// An owner the world lacks, a target without its master, and the walk's and the wait's.
    pub(crate) fn start_repair(
        &mut self,
        repair_action: &mut RepairAction,
        start: HeadStart,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        if start == HeadStart::Added && repair_action.target.is_some() {
            let roles = &step.world.model.roles.clone();
            let reads = self.repair_reads(step, repair_action.target, roles)?;
            let walk = RepairState::Move { move_to: None };
            return repair_action.set_state(step, self, walk, &reads, roles);
        }
        let others = OthersRef::of(&step.world.players, &step.world.index, step.player)?;
        let target = repair_action.target.map(RepairTargetId::id);
        let counted = match target.and_then(|id| Some((id, others.find(id)?))) {
            Some((target, (player_id, target_unit))) => {
                let kind = target_unit
                    .master(step.world.master_tables(), player_id)?
                    .kind;
                counted_object_of(&step.world.parts.building, others, target, kind)
            }
            None => None,
        };
        repair_action.idle(step, self, counted)
    }
}
