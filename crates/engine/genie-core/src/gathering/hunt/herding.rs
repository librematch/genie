//! Leading own livestock to its drop site (state 10), facing an animal and the decay check.

use crate::combat::attack::Aim;
use crate::combat::attack::Fight;
use crate::combat::attack::face_target;
use crate::combat::damage::Victim;
use crate::gathering::hunt::DecayRule;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::update::write_target_back;
use crate::gathering::hunt::{Herd, HuntAction, HuntNext, HuntReads, LEAD_REACH, LEAD_WAIT};
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::Mover;
use crate::movement::footprint_gap_squared;
use crate::objects::layers::Action;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::players::AmountRate;
use crate::world::HUNTER_UNMOVING;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::AttributeRoles;
use crate::world::layer::Roles;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl HuntAction {
    /// `set_state(10)` (`0x140ac60e0` case 10): `sheep_still_time` clears, the animal's place
    /// becomes the start (`+0x78`, `+0x7c`), the hunter faces it (`FUN_141162db0`, at once; one
    /// that turns over time is not modelled), a hunter of type 70 with a special graphic shows
    /// it (slot 0x68 with master `+0x8e8`) and the timer starts at −1 s.
    pub(super) fn set_lead(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        fight: &Fight,
        hunt_reads: &HuntReads,
        herd: &Herd,
    ) -> GenieResult<()> {
        self.tear_down(step.world.parts.mover_mut(unit.id))?;
        self.state = HuntState::Lead;
        self.lead.still_time = 0;
        let at = fight.victim.position;
        self.lead.start = at.ground();
        self.face_animal(
            (unit.position(), step.world.parts.mover_mut(unit.id)),
            at,
            hunt_reads.attack.world_time,
        )?;
        if let Some(graphic) = herd.special_graphic {
            new_sprite(
                &mut unit.sprite,
                step.world.parts.move_stats_mut(unit.id),
                graphic,
                step.world.streams.get_mut(Stream::Sprite),
                step.world.model.tuning.sprite,
            );
        }
        self.timer = LEAD_WAIT;
        Ok(())
    }

    /// `FUN_141162db0` on the animal at `to` at once (a unit without a turn time turns by the
    /// facets between) at the world's time `now`, of the hunter at `at` with its mover.
    pub(super) fn face_animal(
        &self,
        (at, mover): (Position, Option<&mut Mover>),
        to: Position,
        now: WorldTime,
    ) -> GenieResult<()> {
        let mover = mover.ok_or(HUNTER_UNMOVING)?;
        if face_target(mover, at, to, now)? == Aim::Turning {
            return Err(GenieError::unsupported(
                "a hunter that turns over time to face its target",
            ));
        }
        Ok(())
    }

    /// State 10's update (`0x140ac7a00` case 9): without its target the hunt searches
    /// (`set_state(3)`, the caller's). An animal that stands where it was sent (`Herd::idle`) is
    /// slaughtered (`set_state(12)`); else the timer runs by dt and at its end an animal on the
    /// start is slaughtered too; one that moved but stays within 1.5 tiles (`LEAD_REACH`
    /// squared, `FUN_14118f8c0`) becomes the start again, the hunter faces it and waits another
    /// second; one farther is walked to (`set_state(4)`).
    pub(super) fn update_lead(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        fight: &Fight,
        hunt_reads: &HuntReads,
        roles: &Roles,
    ) -> GenieResult<HuntNext> {
        if self.target.is_none() {
            self.state = HuntState::Search;
            return Ok(HuntNext::Search);
        }
        let herd = hunt_reads.herd.ok_or(GenieError::unsupported(
            "a hunt leading livestock unprepared",
        ))?;
        if herd.idle {
            self.start_attack(step, unit, &herd, roles)?;
            return Ok(HuntNext::Stay);
        }
        self.timer += step.dt.seconds();
        if self.timer < Seconds::ZERO {
            return Ok(HuntNext::Stay);
        }
        let position = fight.victim.position;
        if position.x() == self.lead.start.x() && position.y() == self.lead.start.y() {
            self.start_attack(step, unit, &herd, roles)?;
            return Ok(HuntNext::Stay);
        }
        let ground = step
            .world
            .parts
            .mover_of(unit.id)
            .map(|mover| mover.stats.radius.ground())
            .ok_or(HUNTER_UNMOVING)?;
        if footprint_gap_squared(unit.place.position(), ground, position, fight.victim.radius)
            <= LEAD_REACH
        {
            self.lead.start = position.ground();
            self.face_animal(
                (unit.position(), step.world.parts.mover_mut(unit.id)),
                position,
                hunt_reads.attack.world_time,
            )?;
            self.timer = LEAD_WAIT;
            return Ok(HuntNext::Stay);
        }
        let walks = hunt_reads.walks(
            &step.world.model.graphics,
            unit,
            step.world.parts.mover_of(unit.id),
        );
        let player = fight.own.player;
        self.set_walk(
            (&step.world.spatial, unit.position()),
            step.world.parts.motion_mut(unit.id),
            &fight.victim,
            player,
            &walks,
            &step.world.model,
        )?;
        Ok(HuntNext::Stay)
    }

    /// `FUN_140ac44b0`: a task whose `attribute_type4` is 0x124, with its player's attribute 292
    /// at 1, stops its target's decay: a target whose master decays (`+0x620` above 0) gets a
    /// master of its own unless it has one (`FUN_141188bc0`, `FUN_141186d10`) and that master's
    /// decay takes the other sign (its bits XOR `0x80000000`), and the target goes back to the
    /// world at once ([`Victim::decay`]). Otherwise it does nothing.
    pub(super) fn decay_check(
        &self,
        step: &mut Step<'_>,
        hunter: &mut Unit,
        actions: &mut Action,
        decay: DecayRule,
        fight: &mut Fight,
        roles: &Roles,
    ) -> GenieResult<()> {
        if self.stops_decay_of(decay, &mut fight.victim, roles) {
            write_target_back(step, hunter, actions, fight, roles)?;
        }
        Ok(())
    }

    /// [`HuntAction::decay_check`]'s test and its sign change on the target's copy: true when it
    /// stopped the decay.
    pub(super) fn stops_decay_of(
        &self,
        decay: DecayRule,
        victim: &mut Victim,
        roles: &Roles,
    ) -> bool {
        let stops = roles
            .of_some_attribute(self.task.kind4)
            .contains(AttributeRoles::DECAY_TASK)
            && decay == DecayRule::Stops
            && AmountRate::ZERO < victim.decay;
        if stops {
            victim.decay = -victim.decay;
        }
        stops
    }
}
