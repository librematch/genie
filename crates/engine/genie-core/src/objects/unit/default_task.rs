//! The master's default task, which an idle unit starts in its update: a bird's Fly, a farm's
//! Make or the Artifact of livestock.

use crate::GenieError;
use crate::objects::ObjectState;
use crate::objects::masters::Master;
use crate::objects::unit::Unit;
use crate::ownership::artifact::Artifact;
use crate::players::PlayerId;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
    /// `RGE_Action_Object::update` (`0x14106ac90`) on a unit in state 2 without an action or a
    /// production: the master's default task starts (slot 0x7f0), and its first update comes in
    /// the next update (true). A default task that the slice does not hold is an error.
    pub(crate) fn start_default_task(&mut self, c: &mut Step<'_>) -> Result<bool, GenieError> {
        if self.life.state != ObjectState::Alive
            || self.top_action_kind().is_some()
            || self.building.production.is_some()
        {
            return Ok(false);
        }
        if self.actions.default_fly {
            let task = Master::default_task(self.master(&c.w.masters, c.player)?)
                .copied()
                .ok_or(GenieError::missing_data(
                    "a bird's default Fly whose master has no default task",
                ))?;
            self.start_fly(c, task)?;
            return Ok(true);
        }
        if std::mem::take(&mut self.actions.default_make) {
            let task = Master::default_task(self.master(&c.w.masters, c.player)?)
                .copied()
                .ok_or(GenieError::unsupported("a Make whose master has no task"))?;
            self.actions
                .list
                .set_make(Some(crate::gathering::make::MakeAction::start(
                    task,
                    &c.w.players.player(c.player)?.attributes,
                    &mut self.life,
                )));
            return Ok(true);
        }
        if self.actions.default_artifact {
            // the factory gives the action the default task, whose moving sprite its MoveTos take
            let moving = Master::default_task(self.master(&c.w.masters, c.player)?)
                .and_then(|t| t.sprites.moving);
            self.actions
                .list
                .set_artifact(Some(Artifact::new(c.player == PlayerId::GAIA, moving)));
            return Ok(true);
        }
        if let Some(e) = &self.actions.default_task {
            return Err(e.clone());
        }
        Ok(false)
    }
}
