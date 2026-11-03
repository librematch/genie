//! The master's default task, which an idle unit starts in its update: a bird's Fly, a farm's
//! Make or the Artifact of livestock.

use crate::GenieError;
use crate::gathering::make::MakeAction;
use crate::objects::DefaultStart;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::layers::DefaultTask;
use crate::objects::masters::Master;
use crate::objects::unit::Unit;
use crate::ownership::artifact::Artifact;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
    /// `RGE_Action_Object::update` (`0x14106ac90`) on a unit in state 2 without an action or a
    /// production: the master's default task starts (slot 0x7f0), and its first update comes in the
    /// next update ([`DefaultStart::Started`]). A default task that the slice does not hold is an
    /// error.
    pub(crate) fn start_default_task(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
    ) -> Result<DefaultStart, GenieError> {
        if self.life.state != ObjectState::Alive
            || actions.top_action_kind().is_some()
            || c.w.parts.production.contains(self.id)
        {
            return Ok(DefaultStart::NotStarted);
        }
        match actions.default_task {
            DefaultTask::Nothing => Ok(DefaultStart::NotStarted),
            DefaultTask::Fly => {
                let task = Master::default_task(self.master(c.w.master_tables(), c.player)?)
                    .copied()
                    .ok_or(GenieError::missing_data(
                        "a bird's default Fly whose master has no default task",
                    ))?;
                self.start_fly(actions, c, task)?;
                Ok(DefaultStart::Started)
            }
            DefaultTask::Make => {
                // it starts once
                actions.default_task = DefaultTask::Nothing;
                let task = Master::default_task(self.master(c.w.master_tables(), c.player)?)
                    .copied()
                    .ok_or(GenieError::unsupported("a Make whose master has no task"))?;
                actions.list.set_make(Some(MakeAction::start(
                    task,
                    &c.w.players.player(c.player)?.attributes,
                    &mut self.life,
                    &c.w.model.roles,
                    c.w.model.slots,
                )));
                Ok(DefaultStart::Started)
            }
            DefaultTask::Artifact => {
                // the factory gives the action the default task, whose moving sprite its
                // MoveTos take
                let moving = Master::default_task(self.master(c.w.master_tables(), c.player)?)
                    .and_then(|t| t.sprites.moving);
                actions
                    .list
                    .set_artifact(Some(Artifact::new(c.player, moving)));
                Ok(DefaultStart::Started)
            }
            DefaultTask::Fails(f) => Err(f.error()),
        }
    }
}
