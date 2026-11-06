//! The walk to a dead target (state 4), the face (6) and the gathering (7).

use crate::combat::attack::Fight;
use crate::combat::damage::Victim;
use crate::gathering::gather::Walks;
use crate::gathering::hunt::HuntGather;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::{
    HERD_FAR, HUNT_TARGET_UNPLACED, HuntAction, HuntNext, HuntReads, LAST_BIT, REMAINDER,
};
use crate::gathering::{HuntTargetId, both_food};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::rng::{Stream, Well512};
use crate::movement::footprint_gap;
use crate::movement::move_to::WalkTo;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::layers::MotionMut;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::world::HUNTER_UNMOVING;
use crate::world::Seconds;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpriteTuning;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};
use genie_model::model::Model;
use std::cmp::Ordering;

impl HuntAction {
    /// `set_state(4)` (`0x140ac60e0` case 4) after the target died, or for live livestock of the
    /// hunter's `player`: the sub-actions go, the timer clears, the target's place becomes the
    /// point, and a quiet `MoveTo` to the target within the task's range (`HuntTuning::own_livestock_range`
    /// to own livestock) starts (`FUN_140aa0df0`, its start `0x140aa2100`: a unit within 0.01 of
    /// the target's footprint is there already, state 1; else state 3). Another live target is
    /// not modelled.
    pub(super) fn set_walk(
        &mut self,
        (map, at): (&Spatial, Position),
        mut motion: MotionMut<'_>,
        victim: &Victim,
        player: PlayerId,
        walks: &Walks<'_>,
        model: &Model,
    ) -> GenieResult<()> {
        let (roles, hunt_tuning, move_to_tuning) =
            (&model.roles, &model.tuning.hunt, &model.tuning.move_to);
        self.tear_down(motion.mover_mut())?;
        self.state = HuntState::Walk { move_to: None };
        self.timer = Seconds::ZERO;
        // The villagers' class, whose `MoveTo` waits once when the motion step gives up.
        // Classes the hunt treats apart: livestock (58, `-0xe72`) and 49 (`-0xe7b`).
        // The predators' class, whose hunt ends on its own terms (`-0xea2` raw, `0x140ac85fd`).
        let own_livestock =
            roles.of_class(victim.class).contains(ClassRoles::LIVESTOCK) && victim.player == player;
        if victim.state <= ObjectState::Alive && !own_livestock {
            return Err(GenieError::unsupported(
                "a hunt's set_state 4 on a live target after its attack",
            ));
        }
        let placed = map.get_known(self.target)?.ok_or(HUNT_TARGET_UNPLACED)?;
        self.point = placed.position;
        let range = if victim.state <= ObjectState::Alive {
            hunt_tuning.own_livestock_range
        } else {
            self.task.range.get()
        };
        let mover = motion.mover().ok_or(HUNTER_UNMOVING)?;
        let ground = mover.stats.radius.ground();
        let gap = footprint_gap(at, ground, placed.position, placed.radius);
        let to = WalkTo {
            id: self.target.map(HuntTargetId::id),
            at: placed.position,
            gap,
            state: Some(victim.state),
        };
        let move_to =
            self.move_to_object(to, Distance::new(range), mover, walks, move_to_tuning)?;
        self.state = HuntState::Walk { move_to };
        Ok(())
    }

    /// `set_state(6)` (case 6): the hunt faces its target. Live livestock of its own player is led
    /// to its drop site first when it stands more than 3 tiles from it (`HuntTuning::herd_reach`
    /// squared, `set_state` 10, not modelled); then live livestock is attacked (`set_state` 12,
    /// with an Attack of the master's ranges: [`Herd`](crate::gathering::hunt::Herd)). Dead
    /// livestock out of the task's range plus `HuntTuning::gap` is walked to again (`set_state` 4,
    /// [`HuntAction::set_walk`]). Otherwise the unit takes the hunt's sprite (`FUN_140ac51d0`: the
    /// carrying one for a load of the task's resource of 1 or more, else the moving one; slot 0x68
    /// takes the standing sprite for none) and its timer is 0. Another player's target of class 49
    /// (`FUN_141185360`: a farm changes hands, `FUN_1411858f0`, a pasture's livestock ends the
    /// hunt) is not modelled.
    pub(super) fn set_face(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        fight: &Fight,
        hunt_reads: &HuntReads,
        roles: &Roles,
    ) -> GenieResult<HuntNext> {
        // another player's target of class 49 (a farm's or a pasture's livestock): only a building
        // changes hands (FUN_141185360, FUN_1411858f0), any other ends the hunt (set_state 1);
        // the hunter's own goes on to its sprite
        if roles
            .of_class(fight.victim.class)
            .contains(ClassRoles::FARM)
            && fight.victim.player != fight.own.player
        {
            return Err(GenieError::unsupported(
                "a hunt facing a target of class 49 (set_state 6)",
            ));
        }
        if roles
            .of_class(fight.victim.class)
            .contains(ClassRoles::LIVESTOCK)
        {
            if fight.victim.state <= ObjectState::Alive {
                let herd = hunt_reads.herd.ok_or(GenieError::unsupported(
                    "a hunt facing live livestock unprepared",
                ))?;
                // own livestock more than 3 tiles from its drop site is led there: once sent
                // (`moved_sheep`) the hunt waits for it; nearer than 30 tiles an animal that
                // stands is sent (its slot 0x138) with the herd around it (`FUN_140ac4c60`) at
                // once, the hunter in the world; either way `moved_sheep` and `set_state(10)`
                if let (true, Some((site, gap_squared))) = (herd.own, herd.drop_site) {
                    self.drop_site = Some(site);
                    if DistanceSquared::new(step.world.model.tuning.hunt.herd_reach) < gap_squared
                        && (self.lead.moved || gap_squared < HERD_FAR)
                    {
                        if !self.lead.moved && herd.idle {
                            let (player, sheep) = (step.player, self.target);
                            // the hunt faces its livestock target here
                            if let Some(sheep) = sheep {
                                step.lend((&mut *unit, &mut *actions), |world| {
                                    world.lead_herd(player, sheep.id(), site)
                                })?;
                            }
                        }
                        self.lead.moved = true;
                        self.set_lead(step, unit, fight, hunt_reads, &herd)?;
                        return Ok(HuntNext::Stay);
                    }
                }
                self.start_attack(step, unit, &herd, roles)?;
                return Ok(HuntNext::Stay);
            }
            let placed = step
                .world
                .spatial
                .get_known(self.target)?
                .ok_or(HUNT_TARGET_UNPLACED)?;
            let ground = step
                .world
                .parts
                .mover_of(unit.id)
                .map(|mover| mover.stats.radius.ground())
                .ok_or(HUNTER_UNMOVING)?;
            if self.task.range + Distance::new(step.world.model.tuning.hunt.gap)
                < footprint_gap(
                    unit.place.position(),
                    ground,
                    placed.position,
                    placed.radius,
                )
            {
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
                return Ok(HuntNext::Stay);
            }
        }
        self.begin_face(step, unit, hunt_reads);
        Ok(HuntNext::Stay)
    }

    /// `set_state(6)`'s end (`0x140ac60e0` case 6, after its tests on the target): the timer
    /// clears and the unit takes the hunt's walking sprite (`FUN_140ac51d0`: the carrying one
    /// for a load of the task's resource of 1 or more, else the moving one; slot 0x68 takes the
    /// standing sprite for none).
    pub(super) fn begin_face(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        hunt_reads: &HuntReads,
    ) {
        self.state = HuntState::Face;
        self.timer = Seconds::ZERO;
        let carrying = self.task.resource.is_some()
            && unit.life.attribute_kind == self.task.resource
            && 1 <= unit.life.amount.get() as i32;
        let shown = match (carrying, hunt_reads.sprites.carrying) {
            (true, Some(graphic)) => Some(graphic),
            _ => hunt_reads.sprites.moving,
        }
        .or(hunt_reads.sprites.standing);
        if let Some(graphic) = shown {
            new_sprite(
                &mut unit.sprite,
                step.world.parts.move_stats_mut(unit.id),
                graphic,
                step.world.streams.get_mut(Stream::Sprite),
                step.world.model.tuning.sprite,
            );
        }
    }

    /// `set_state(7)` (case 7): a food task keeps a load of food (`FUN_14119d310` keeps the
    /// amount), any other load is dropped; the unit takes the working sprite (slot 0x68; the
    /// standing one for none); the gathering sound is not held.
    pub(super) fn set_gather(
        &mut self,
        unit: &mut Unit,
        mut motion: MotionMut<'_>,
        sprite_rng: &mut Well512,
        hunt_reads: &HuntReads,
        sprite_tuning: SpriteTuning,
    ) {
        self.state = HuntState::Gather;
        let kind = self.task.resource;
        let keeps = both_food(kind, unit.life.attribute_kind);
        if unit.life.attribute_kind != kind {
            unit.life.amount = if keeps {
                unit.life.amount
            } else {
                Amount::ZERO
            };
            unit.life.attribute_kind = kind;
        }
        if let Some(graphic) = hunt_reads.sprites.working.or(hunt_reads.sprites.standing) {
            new_sprite(
                &mut unit.sprite,
                motion.move_stats_mut(),
                graphic,
                sprite_rng,
                sprite_tuning,
            );
        }
    }

    /// State 7 (case 6): the work, `(work rate) × work_val1 × dt`, at most what the target holds,
    /// times the productivity, while the load stays within the capacity: the target loses the work,
    /// its last bit below 0.1 going with a thousandth of its master's first storage amount; the
    /// unit gains, a remainder below 0.1 of the capacity rounding the load up. A full load or an
    /// empty target ends the gathering: [`HuntGather::DropOff`] asks the caller for `set_state(8)`
    /// (its drop site search, [`HuntAction::set_drop`]). The game's `+0xc14` (`FUN_140c884c0`), 0
    /// as the tick oracle holds it, would take the whole target at once.
    pub(super) fn gather(
        &self,
        dt: Seconds,
        unit: &mut Unit,
        victim: &mut Victim,
        hunt_reads: &HuntReads,
        roles: &Roles,
    ) -> GenieResult<HuntGather> {
        let held = &mut unit.life.amount;
        let work = Amount::new((hunt_reads.work_rate * self.task.work_value) * dt);
        let take = if work <= victim.amount {
            work
        } else {
            victim.amount
        };
        let mut gain = hunt_reads.productivity.get() * take;
        let capacity = hunt_reads.capacity;
        // `<=` in the game: a NaN goes to the drop-off too
        let room = capacity - *held;
        if !matches!(
            gain.partial_cmp(&room),
            Some(Ordering::Less | Ordering::Equal)
        ) {
            return Ok(HuntGather::DropOff);
        }
        victim.amount -= take;
        // the last bit and the storage's share summed first (`0x140ac8e04`)
        if victim.amount < REMAINDER {
            gain += victim.storage * LAST_BIT + victim.amount;
            victim.amount = Amount::ZERO;
        }
        *held += gain;
        let over = (*held - capacity).abs();
        if *held < capacity && over < REMAINDER && over <= victim.amount {
            victim.amount -= over;
            *held += over;
        }
        if roles
            .of_class(victim.class)
            .intersects(ClassRoles::PREY | ClassRoles::PREDATOR)
            && roles
                .of_some_attribute(unit.life.attribute_kind)
                .contains(AttributeRoles::GOLD)
            && roles
                .of_some_attribute(self.task.resource)
                .contains(AttributeRoles::GOLD)
        {
            return Err(GenieError::unsupported(
                "a gold load of class 9 or 10 (its player's attribute 3)",
            ));
        }
        Ok(if REMAINDER <= over && Amount::ZERO < victim.amount {
            HuntGather::GoesOn
        } else {
            HuntGather::DropOff
        })
    }
}
