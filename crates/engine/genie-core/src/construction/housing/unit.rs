//! A Housing bound to its building and the world: its install and start, its update with the
//! building's work rate and headroom, and the cost its destructor gives back.

use crate::GenieResult;
use crate::commands::QueueId;
use crate::construction::housing::{HousingAction, HousingState, WorkLeft, give_back};
use crate::math::fmath::maxss;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::UpdateCode;
use crate::objects::layers::Action;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::teardown::{HeadCode, Refund};
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::world::PlayState;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

impl World {
    /// The install of a new Housing on object `idx` of player `player`, `UNIT_ORDER`'s
    /// (`FUN_14161e370` case 3, on a building) and the work command's (the factory
    /// `FUN_140aca4b0` case 0x7b, [`World::start_task`]): the constructor (`FUN_140ac1b80`, with
    /// the order's queue id `queue_id`, the work command's −1), then slot 0x808
    /// ([`World::set_only_action`]), its start after the add ([`HousingAction::start`]) and the
    /// head it holds. The game calls slot 0x808 with what the allocation gave, also a null that
    /// only deletes the list; genie's allocation does not fail.
    ///
    /// # Errors
    ///
    /// The install's and the start's.
    pub(crate) fn install_housing(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        queue_id: QueueId,
    ) -> GenieResult<()> {
        let Some(mut housing) =
            self.set_only_action(player, idx, Some(HousingAction::new(queue_id)))?
        else {
            return Ok(());
        };
        housing.start(
            self.players.player_mut(player)?,
            &self.model.slots.attribute,
            Amount::new(self.model.tuning.housing.cost),
        )?;
        let id = self.players.live(player, idx)?.id;
        self.parts.actions_mut(id)?.list.set_housing(Some(housing));
        Ok(())
    }

    /// The stop's slot 0x38 of the Housing on top (`0x140ac2910`, [`HousingAction::halt`]) of
    /// player `player`'s building at `idx`, the action `housing` as the stop took it: done, its
    /// cost back to the owner's wood, and back on the list, also where the give-back fails.
    ///
    /// # Errors
    ///
    /// A building or an owner the world lacks, and an owner without wood.
    pub(crate) fn halt_housing(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        mut housing: HousingAction,
    ) -> GenieResult<()> {
        let id = self.players.live(player, idx)?.id;
        let halted = housing.halt(
            self.players.player_mut(player)?,
            self.model.slots.attribute.wood,
            Amount::new(self.model.tuning.housing.cost),
        );
        self.parts.actions_mut(id)?.list.set_housing(Some(housing));
        halted
    }

    /// What a destructor gave back (`refund`, [`Refund`]) to its unit's owner `owner`: a
    /// Housing's cost to the owner's wood while the game is played (`world+0x88` 0,
    /// [`PlayState::Playing`]: `0x140ac211f`), else nothing.
    ///
    /// # Errors
    ///
    /// An owner the world lacks, or without wood.
    pub(crate) fn give_back_torn(&mut self, refund: Refund, owner: PlayerId) -> GenieResult<()> {
        if refund == Refund::Nothing || self.game_state != PlayState::Playing {
            return Ok(());
        }
        give_back(
            self.players.player_mut(owner)?,
            self.model.slots.attribute.wood,
            Amount::new(self.model.tuning.housing.cost),
        )
    }
}

impl Unit {
    /// A building's Housing in its update (slot 0x30, `0x140ac2a10`) and its code: done (1) in
    /// state 1; in 13 done (`set_state(1)`) and failed (3); at 6 its work ([`Unit::housing_work`])
    /// comes off its timer, and once that ran out ([`WorkLeft::None`]) the building houses
    /// ([`Unit::house`]) and it is done (`set_state(1)`, 1); else it goes on (0), as in state 0.
    /// Nothing without one.
    ///
    /// # Errors
    ///
    /// The work's and the headroom's.
    pub(crate) fn housing_part(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<HeadCode> {
        let Some(mut housing) = actions.list.housing().copied() else {
            return Ok(HeadCode::Keeps);
        };
        let code = match housing.state {
            HousingState::Done => UpdateCode::Done,
            HousingState::Failed => {
                housing.finish();
                UpdateCode::Failed
            }
            HousingState::Working => {
                if housing.work(self.housing_work(step)?) == WorkLeft::None {
                    self.house(step)?;
                    housing.finish();
                    UpdateCode::Done
                } else {
                    UpdateCode::UnderWay
                }
            }
            HousingState::New => UpdateCode::UnderWay,
        };
        actions.list.set_housing(Some(housing));
        Ok(HeadCode::of(code))
    }

    /// The work the building's Housing does in this update (`0x140ac2a10`): the quick-build
    /// cheat's while the game's `+0xc14` is not 0 (`FUN_140c884c0`, [`World::instant_work`];
    /// [`HousingTuning::instant_work`]); else its work rate, its powerup (slot 0x6e8, `+0xa30`)
    /// plus its master's (`+0x798`, `ADDSS` in that order), over the update's seconds
    /// (`world+0x164`), times its hit points' share of its master's (`+0x530`), at least
    /// [`HousingTuning::least_share`] (`MAXSS`), when they are below them (`COMISS`, `JBE`: a NaN
    /// does the whole).
    ///
    /// [`HousingTuning::instant_work`]: crate::world::layer::generated::HousingTuning::instant_work
    /// [`HousingTuning::least_share`]: crate::world::layer::generated::HousingTuning::least_share
    ///
    /// # Errors
    ///
    /// A building whose master the world lacks.
    pub(crate) fn housing_work(&self, step: &Step<'_>) -> GenieResult<Seconds> {
        let tuning = step.world.model.tuning.housing;
        if step.world.instant_work != 0 {
            return Ok(Seconds::new(tuning.instant_work));
        }
        let master = object_master(
            step.world.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            step.world.model.masters.of(step.player),
        )?;
        let work = Seconds::new(
            (step.world.parts.combat_of(self.id).work_rate_powerup
                + master.action_part().work_rate)
                * step.dt.seconds(),
        );
        let most = HitPoints::of_master(master.hit_points);
        Ok(if most > self.life.hp {
            work * maxss(tuning.least_share, self.life.hp / most)
        } else {
            work
        })
    }

    /// The Housing's work done (`0x140ac2a10`): the building holds headroom (attribute 4,
    /// `FUN_14119d310`), what it held of it (`+0x310` for a held attribute 4, else 0) plus
    /// [`HousingTuning::gain`], and its owner gains as much of attribute 4 (slot 0xf8 with 0 and 1,
    /// [`World::add_player_attribute`]). Between them runs its slot 0x8c0 (`0x1411cc970` for a
    /// building, a bare `JMP` to the combat object's `FUN_14120db40`): it writes a salvage pile's
    /// facet (class 40, `+0xe`) by what it holds, and nothing for another class. genie holds no
    /// facet (the task static-object-facet), and no building of build 158041's `.dat` is of class
    /// 40. The local player's sound (`?pop_capped`) is the interface's.
    ///
    /// [`HousingTuning::gain`]: crate::world::layer::generated::HousingTuning::gain
    ///
    /// # Errors
    ///
    /// The add's.
    pub(crate) fn house(&mut self, step: &mut Step<'_>) -> GenieResult<()> {
        let headroom = step.world.model.slots.attribute.headroom;
        let gain = Amount::new(step.world.model.tuning.housing.gain);
        let held = if self.life.attribute_kind == Some(headroom) {
            self.life.amount
        } else {
            Amount::ZERO
        };
        self.life.attribute_kind = Some(headroom);
        self.life.amount = held + gain;
        step.world.add_player_attribute(step.player, headroom, gain)
    }
}
