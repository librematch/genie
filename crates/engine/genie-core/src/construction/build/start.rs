//! The Build's constructor on a foundation, its install through slot 0x808 and its start
//! (slot 0x28): a walk to the foundation it holds, else the wait for work.

use crate::GenieResult;
use crate::construction::FoundationId;
use crate::construction::build::unit::{build_order_states, construction_site};
use crate::construction::build::{BuildAction, BuildState, BuildTask};
use crate::map::Position;
use crate::objects::ListIndex;
use crate::objects::actions::LAST_TAKEN;
use crate::objects::teardown::HeadStart;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::world::Seconds;
use crate::world::World;
use crate::world::others::OthersRef;
use crate::world::step::Step;

impl BuildAction {
    /// The constructor on a foundation (`FUN_140aad4e0`: the factory's, `FUN_140aca4b0` case 0x65
    /// with a target, and the build command's, `FUN_14120d410`): the base constructor
    /// (`FUN_140a73e90` with 1: no second target, no sub-action, the point (0, 0, 0)), the task
    /// `task` (`+0x48`) and not counted among the builders (`+0x68` 0). A foundation whose owner
    /// (`owner`) is the builder's player (`builder`: the two players' `+0x718`) gives the point its
    /// place, and becomes the target (`FUN_140a7a230`) unless it is past [`LAST_TAKEN`]; another
    /// player's leaves the target none and the point at (0, 0, 0). The take of a held target (slot
    /// 0x178) is the caller's. The routine also tests the foundation for null (`0x140aad52c`),
    /// which no caller passes, so `foundation` is a [`Unit`]. The factory hands a call without a
    /// target to the point constructor (`FUN_140aad470`, `0x140aca517`). The build command returns
    /// first on an object the world lacks (`0x14120d438`). The game's constructor leaves the state
    /// 0, which nothing reads before the install's start ([`Unit::start_build`]); genie holds 3
    /// until then.
    #[must_use]
    pub fn new(task: BuildTask, builder: PlayerId, owner: PlayerId, foundation: &Unit) -> Self {
        let (target, point) = if owner == builder {
            let held = foundation.life.state <= LAST_TAKEN;
            (
                held.then_some(FoundationId::new(foundation.id)),
                foundation.position(),
            )
        } else {
            (None, Position::ORIGIN)
        };
        Self {
            target,
            state: BuildState::Idle,
            point,
            task,
            added: false,
            timer: Seconds::ZERO,
            site: None,
        }
    }
}

impl World {
    /// Slot 0x808 with the new `build` on player `player`'s unit at `idx`
    /// ([`World::set_only_action`]): the list's deletion, then its add, which holds the Build at
    /// its head and starts it (slot 0x28 with 1, [`Unit::start_build`] with [`HeadStart::Added`])
    /// on the unit's step. A Build that the add deleted unstarted (a unit in state 3 or later)
    /// starts nothing.
    ///
    /// # Errors
    ///
    /// The install's and the start's.
    pub(crate) fn install_build(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        build: BuildAction,
    ) -> GenieResult<()> {
        let Some(mut build) = self.set_only_action(player, idx, Some(build))? else {
            return Ok(());
        };
        self.run_on_step(player, idx, |unit, actions, step| {
            let started = unit.start_build(&mut build, HeadStart::Added, step);
            actions.list.set_build(Some(build));
            started
        })
    }
}

impl Unit {
    /// The Build's slot 0x28 (`0x140aadd80`, `TEST DL, DL`): with 1, the list's add
    /// ([`HeadStart::Added`]), a Build that holds its foundation (`+0x20`) walks to it
    /// (`set_state(4)`, [`BuildAction::restart`]); one whose constructor took none (another
    /// player's foundation, one past state 6: [`BuildAction::new`]) and, with 0, one that a
    /// deletion left at the head ([`HeadStart::Restarted`]) wait for work (`set_state(3)`,
    /// [`BuildAction::idle`]) on their foundation and their unit AI's build orders as the player
    /// holds them now.
    ///
    /// # Errors
    ///
    /// A player the world lacks, and the walk's and the wait's.
    pub(crate) fn start_build(
        &mut self,
        build_action: &mut BuildAction,
        start: HeadStart,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        if start == HeadStart::Added && build_action.target.is_some() {
            let roles = &step.world.model.roles.clone();
            return build_action.restart(step, self, roles);
        }
        let others = OthersRef::of(&step.world.players, &step.world.index, step.player)?;
        let site = construction_site(&step.world.parts.building, others.own(build_action.target));
        let build_targets = build_order_states(step.world.parts.unit_ai.get(self.id), others);
        build_action.idle(step, self, site, &build_targets)
    }
}
