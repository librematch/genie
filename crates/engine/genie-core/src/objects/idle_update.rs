//! The idle update of the actions the list update does not run (`FUN_140a784a0`): the list
//! update's of each action below the head (`FUN_140a78470`), the inside update's of the head too
//! (`FUN_140a78480`). A target or second target past state 6 goes, and the sub-actions take the
//! same (`FUN_140a98bf0`).

use crate::GenieResult;
use crate::combat::AttackCount;
use crate::combat::attack::Attack;
use crate::garrison::EnterAction;
use crate::movement::move_to::MoveToAction;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::RoleId;
use crate::objects::actions::{Action, LAST_TAKEN, SubActionMut, sub_walk};
use crate::objects::layers::Action as LayersAction;
use crate::objects::teardown::TornDown;
use crate::objects::unit::Unit;
use crate::orders::object_gone;
use crate::ownership::artifact::{Artifact, ArtifactSub};
use crate::world::step::Step;

/// What an idle update reads: the object state (`+0x640`) of an object an action holds, which
/// the game reads through the action's pointer (`+0x20`, `+0x28`).
pub(crate) struct IdleReads<'reads> {
    /// The state of the object of an id the action holds; an error for one the world does not
    /// hold, whose memory the game reads as it was.
    pub(crate) state_of: &'reads dyn Fn(ObjectId) -> GenieResult<ObjectState>,
}

impl IdleReads<'_> {
    /// `held` when its object is past state 6 (`CMP byte [+0x640], 7` and `JC` at
    /// `0x140a784b2`, `0x140a784cc`), none for none or an object in state 6 or below.
    fn gone(&self, held: Option<ObjectId>) -> GenieResult<Option<ObjectId>> {
        let Some(id) = held else {
            return Ok(None);
        };
        Ok((LAST_TAKEN < (self.state_of)(id)?).then_some(id))
    }
}

/// What the idle update of an action leaves its unit's world, in the update's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Idled {
    /// An Attack's target went (its slot 0x68, `FUN_140a82a10`): the unit's attack count
    /// (`+0x900`) clears.
    pub(crate) clears_count: bool,
    /// The buildings a counted builder or repairer leaves and the objects let go (slot 0x180).
    pub(crate) left: TornDown,
}

impl Action {
    /// `FUN_140a784a0` on an action of the list ([`IdledAction::idle_update`]), with what it
    /// leaves the unit's world.
    ///
    /// # Errors
    ///
    /// A target the world does not hold ([`IdleReads::state_of`]).
    pub(crate) fn idle_update(&mut self, reads: &IdleReads<'_>) -> GenieResult<Idled> {
        let mut idled = Idled::default();
        IdledAction::Listed(self).idle_update(reads, &mut idled)?;
        Ok(idled)
    }

    /// The kind's slot 0x68 with none on a held target. The base's (`FUN_140a7a230`) lets the
    /// target go (slot 0x180 on the object the world's table holds of its id `+0x30`) unless the
    /// action is quiet (`+0x60`), and the action holds none (`+0x20` 0, `+0x30` −1). The Attack's
    /// (`FUN_140a82a10`) first clears the unit's attack count ([`Attack::change_target`]); the
    /// Build's (`FUN_140ab0300`) and the Repair's (`FUN_140ada470`) first take a counted builder
    /// (`+0x68`) or repairer (`+0x6c`) out of its building's count (`FUN_140aaddb0`,
    /// `FUN_140ad9050`: `+0x9e8`); the Hunt's (`FUN_140ac7980`) then clears its lead
    /// ([`HuntAction::let_go`]). Of the slice's actions only the `MoveTo`, the Attack and the
    /// Repair hold a `+0x60` of their own: the others are loud.
    ///
    /// [`HuntAction::let_go`]: crate::gathering::hunt::HuntAction::let_go
    fn drop_idle_target(&mut self, idled: &mut Idled) {
        match self {
            Self::MoveTo(move_to_action) => move_to_action.drop_idle_target(idled),
            Self::Attack(attack) => attack.drop_idle_target(idled),
            Self::Enter(enter_action) => enter_action.drop_idle_target(idled),
            Self::Artifact(artifact) => idled.left.release(artifact.set_target(None).released),
            Self::Hunt(hunt_action) => idled.left.release(hunt_action.let_go().map(RoleId::id)),
            Self::Gather(gather_action) => {
                idled
                    .left
                    .release(gather_action.target.take().map(RoleId::id));
            }
            Self::Missile(missile_action) => {
                idled
                    .left
                    .release(missile_action.target.take().map(RoleId::id));
            }
            Self::Pack(pack_action) => idled.left.release(pack_action.target.take()),
            Self::Unpack(unpack_action) => idled.left.release(unpack_action.target.take()),
            Self::Build(build_action) => {
                if build_action.added
                    && let Some(target) = build_action.target
                {
                    idled.left.builder_leaves.push(target.id());
                }
                build_action.added = false;
                idled
                    .left
                    .release(build_action.target.take().map(RoleId::id));
            }
            Self::Repair(repair_action) => {
                if repair_action.counted
                    && let Some(target) = repair_action.target
                {
                    idled.left.builder_leaves.push(target.id());
                }
                repair_action.counted = false;
                let target = repair_action.target.take().map(RoleId::id);
                if repair_action.voice.is_loud() {
                    idled.left.release(target);
                }
            }
            // they hold no target (`Action::target`): no idle update reaches them here
            Self::Fly(_)
            | Self::Explore(_)
            | Self::Transport(_)
            | Self::Make(_)
            | Self::WaitSolidify(_)
            | Self::Housing(_) => {}
        }
    }

    /// The second target (`+0x28`, its id `+0x34`) goes: −1 for the kinds that hold one
    /// ([`Action::second_target`]).
    const fn clear_second_target(&mut self) {
        match self {
            Self::Hunt(hunt_action) => hunt_action.carry_to = None,
            Self::Gather(gather_action) => gather_action.carry_to = None,
            Self::Build(build_action) => build_action.site = None,
            Self::Missile(missile_action) => missile_action.fired_from = None,
            Self::Attack(attack) => attack.target2 = None,
            Self::Artifact(_)
            | Self::Repair(_)
            | Self::MoveTo(_)
            | Self::Fly(_)
            | Self::Explore(_)
            | Self::Enter(_)
            | Self::Transport(_)
            | Self::Make(_)
            | Self::WaitSolidify(_)
            | Self::Housing(_)
            | Self::Pack(_)
            | Self::Unpack(_) => {}
        }
    }
}

/// An action `FUN_140a784a0` runs on: one of the list (`FUN_140a78470`, `FUN_140a78480`) or a
/// sub-action of one (`+0x50`, `FUN_140a98bf0`), which genie's states hold by kind.
#[derive(Debug)]
enum IdledAction<'action> {
    /// An action of the list.
    Listed(&'action mut Action),
    /// A walk's `MoveTo`: quiet where it holds a target (`FUN_140a7a170` with 1); the loud
    /// walks of a Fly and of an Artifact go to a point.
    Walk(&'action mut MoveToAction),
    /// The Hunt's Attack (its state 12), quiet.
    HuntAttack(&'action mut Attack),
    /// The Artifact's Enter.
    ArtifactEnter(&'action mut EnterAction),
}

impl IdledAction<'_> {
    /// `FUN_140a784a0`: a target (`+0x20`) past state 6 goes by the kind's slot 0x68 with none
    /// ([`IdledAction::drop_target`]); a second target (`+0x28`) past state 6 is let go (slot
    /// 0x180 on the object of its id `+0x34`, with no test of the action's `+0x60`) and goes
    /// (`+0x28` 0, `+0x34` −1); then the sub-actions take the same
    /// ([`IdledAction::idle_sub_actions`]).
    ///
    /// # Errors
    ///
    /// A target the world does not hold ([`IdleReads::state_of`]).
    fn idle_update(mut self, reads: &IdleReads<'_>, idled: &mut Idled) -> GenieResult<()> {
        if reads.gone(self.target())?.is_some() {
            self.drop_target(idled);
        }
        if let Some(second) = reads.gone(self.second_target())? {
            idled.left.release(second);
            self.clear_second_target();
        }
        self.idle_sub_actions(reads, idled)
    }

    /// Its target (`+0x20`, its id `+0x30`).
    fn target(&self) -> Option<ObjectId> {
        match self {
            Self::Listed(action) => action.target(),
            Self::Walk(move_to_action) => move_to_action.target,
            Self::HuntAttack(attack) => attack.target.map(RoleId::id),
            Self::ArtifactEnter(enter_action) => enter_action.target.map(RoleId::id),
        }
    }

    /// Its second target (`+0x28`, its id `+0x34`): a `MoveTo` and an Enter hold none, and the
    /// Hunt's Attack none either (its constructor `FUN_140a7c210` leaves it null, and its update
    /// only clears it, `0x140a834d1`).
    fn second_target(&self) -> Option<ObjectId> {
        match self {
            Self::Listed(action) => action.second_target(),
            Self::HuntAttack(attack) => attack.target2.map(RoleId::id),
            Self::Walk(_) | Self::ArtifactEnter(_) => None,
        }
    }

    /// The kind's slot 0x68 with none ([`Action::drop_idle_target`]).
    fn drop_target(&mut self, idled: &mut Idled) {
        match self {
            Self::Listed(action) => action.drop_idle_target(idled),
            Self::Walk(move_to_action) => move_to_action.drop_idle_target(idled),
            Self::HuntAttack(attack) => attack.drop_idle_target(idled),
            Self::ArtifactEnter(enter_action) => enter_action.drop_idle_target(idled),
        }
    }

    /// The second target goes ([`Action::clear_second_target`]).
    const fn clear_second_target(&mut self) {
        match self {
            Self::Listed(action) => action.clear_second_target(),
            Self::HuntAttack(attack) => attack.target2 = None,
            Self::Walk(_) | Self::ArtifactEnter(_) => {}
        }
    }

    /// `FUN_140a98bf0` on the action's sub-actions (`+0x50`, one at most in every state genie
    /// models), each its [`IdledAction::idle_update`].
    ///
    /// # Errors
    ///
    /// A target the world does not hold.
    fn idle_sub_actions(self, reads: &IdleReads<'_>, idled: &mut Idled) -> GenieResult<()> {
        self.into_sub_action()
            .map_or(Ok(()), |sub_action| sub_action.idle_update(reads, idled))
    }

    /// Its sub-action: its walk's `MoveTo`, the Hunt's Attack ([`Action::sub_action_mut`]) or the
    /// Artifact's Enter, which has no slot 0x50 of its own.
    fn into_sub_action(self) -> Option<Self> {
        match self {
            Self::Listed(Action::Artifact(Artifact {
                sub: Some(ArtifactSub::Enter { enter }),
                ..
            })) => Some(Self::ArtifactEnter(enter)),
            Self::Listed(action) => action.sub_action_mut().map(Self::from),
            Self::HuntAttack(attack) => sub_walk(attack.state.walk_mut()).map(Self::from),
            Self::ArtifactEnter(enter_action) => {
                sub_walk(enter_action.state.walk_mut()).map(Self::from)
            }
            Self::Walk(_) => None,
        }
    }
}

impl<'action> From<SubActionMut<'action>> for IdledAction<'action> {
    /// A sub-action whose slot 0x50 a task change runs: a walk's `MoveTo` or the Hunt's Attack.
    fn from(sub_action: SubActionMut<'action>) -> Self {
        match sub_action {
            SubActionMut::MoveTo(move_to_action) => Self::Walk(move_to_action),
            SubActionMut::Attack(attack) => Self::HuntAttack(attack),
        }
    }
}

impl MoveToAction {
    /// Slot 0x68 with none, the base's (`FUN_140a7a230`): the target goes, let go unless the
    /// `MoveTo` is quiet (`+0x60`, as a sub-action that holds a target is).
    fn drop_idle_target(&mut self, idled: &mut Idled) {
        let target = self.target.take();
        if self.voice.is_loud() {
            idled.left.release(target);
        }
    }
}

impl Attack {
    /// Slot 0x68 with none (`FUN_140a82a10`, [`Attack::change_target`]): the unit's attack count
    /// clears, then the target goes, let go unless the Attack is quiet (the Hunt's).
    fn drop_idle_target(&mut self, idled: &mut Idled) {
        let worked = self.change_target(None);
        idled.clears_count |= worked.clears_count;
        idled.left.release(worked.lets_go);
    }
}

impl EnterAction {
    /// Slot 0x68 with none, the base's (`FUN_140a7a230`): the building or transport goes, let go.
    fn drop_idle_target(&mut self, idled: &mut Idled) {
        idled.left.release(self.target.take().map(RoleId::id));
    }
}

/// The actions an idle update of the list reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum IdleFrom {
    /// The list update (`FUN_140a992b0`, after the head's slot 0x30): each action below the
    /// head (`FUN_140a78470`).
    BelowHead,
    /// The inside update (`FUN_140a98790`): the head (`FUN_140a78480`), then each action below
    /// it (`FUN_140a78470`).
    Head,
}

impl IdleFrom {
    /// The actions at the head of the list that the update passes over.
    const fn passed(self) -> usize {
        match self {
            Self::BelowHead => 1,
            Self::Head => 0,
        }
    }
}

impl Unit {
    /// The idle update ([`Action::idle_update`]) of the unit's actions `from` on, in the list's
    /// order, each one's effects applied at once by the unit ([`Unit::apply_idled`]) before the
    /// next one's. A target's state is the updating unit's own for its id, which the world's
    /// table does not find while it updates.
    ///
    /// # Errors
    ///
    /// A target the world does not hold, and what the effects give.
    pub(crate) fn idle_actions(
        &mut self,
        actions: &mut LayersAction,
        step: &mut Step<'_>,
        from: IdleFrom,
    ) -> GenieResult<()> {
        for action in actions.list.iter_mut().skip(from.passed()) {
            let idled = action.idle_update(&IdleReads {
                state_of: &|id| self.held_state(step, id),
            })?;
            self.apply_idled(step, idled)?;
        }
        Ok(())
    }

    /// The object state (`+0x640`) of the object `id` an action of the unit holds: the unit's own,
    /// else the world's.
    ///
    /// # Errors
    ///
    /// An object the world does not hold ([`object_gone`]).
    fn held_state(&self, step: &Step<'_>, id: ObjectId) -> GenieResult<ObjectState> {
        if id == self.id {
            return Ok(self.life.state);
        }
        step.world.object_state(id).ok_or_else(|| object_gone(id))
    }

    /// What an idle update left, applied at once: the unit's attack count clears, then the
    /// buildings are left and the objects let go by the unit
    /// ([`TornDown::let_go`]).
    ///
    /// # Errors
    ///
    /// The unit's combat layer's, the unit's master's and what the leaves and releases give.
    fn apply_idled(&mut self, step: &mut Step<'_>, idled: Idled) -> GenieResult<()> {
        if idled.clears_count {
            step.world.parts.combat_mut(self.id)?.attack_count = AttackCount::ZERO;
        }
        if idled.left.is_empty() {
            return Ok(());
        }
        let by = self.releaser(step)?;
        idled.left.let_go(step, self, by)
    }
}

#[cfg(test)]
mod tests;
