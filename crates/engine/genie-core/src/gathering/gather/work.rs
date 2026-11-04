//! The face (state 6), the work (7), the drop-off's update (8) and a random wait.

use crate::combat::damage::Victim;
use crate::gathering::both_food;
use crate::gathering::gather::GatherState;
use crate::gathering::gather::{GatherAction, GatherReads, LAST_BIT, Next, REMAINDER};
use crate::gathering::hunt::Dropped;
use crate::gathering::hunt::{DropOff, resource_kind};
use crate::math::rng::Stream;
use crate::movement::Me;
use crate::movement::Speed;
use crate::movement::move_to::SubCode;
use crate::objects::ObjectState;
use crate::objects::sprite::{Graphic, new_sprite};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::world::Seconds;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::settings::FarmMode;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};
use std::mem::take as mem_take;

impl GatherAction {
    /// `set_state(6)` (case 6) on a target: a farm, masters 357 and 1188 and a target not yet
    /// built (state below 2, the unit's slot 0x380) are not modelled; the timer clears and the
    /// unit takes the walking sprite (`FUN_140a8b660`; slot 0x68 takes the standing one for
    /// none).
    pub(super) fn set_face(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        target: &Victim,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        self.state = GatherState::Face;
        // another player's farm is the gatherer's to take (FUN_141185360, FUN_1411858f0)
        if roles.of_class(target.class).contains(ClassRoles::FARM) && target.player != step.player {
            return Err(GenieError::unsupported(
                "a gather's set_state 6 on another player's farm (FUN_141185360)",
            ));
        }
        if roles
            .of_master(target.master_id)
            .contains(MasterRoles::DEAD_FARM_SITE)
        {
            return Err(GenieError::unsupported(
                "a gather's set_state 6 on master 357 or 1188",
            ));
        }
        if target.state < ObjectState::Alive {
            return Ok(Next::Build);
        }
        if roles
            .of_master(target.master_id)
            .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE)
        {
            return Err(GenieError::unsupported(
                "a gather's set_state 6 on master 1893 or 1897",
            ));
        }
        self.timer = Seconds::ZERO;
        Self::show(
            step,
            unit,
            self.walk_sprite(unit, gather_reads)
                .or(gather_reads.sprites.standing),
        );
        Ok(Next::Stay)
    }

    /// Slot 0x68 with `sprite` (`FUN_141161280`, [`new_sprite`]) on the unit at once: its new
    /// sprite draws from the Sprite stream; none shows nothing new.
    pub(super) fn show(step: &mut Step<'_>, unit: &mut Unit, sprite: Option<Graphic>) {
        if let Some(graphic) = sprite {
            let rng = step.w.streams.get(Stream::Sprite);
            new_sprite(
                &mut unit.sprite,
                step.w.parts.move_stats_mut(unit.id),
                graphic,
                rng,
                step.w.model.tuning.sprite,
            );
        }
    }

    /// `set_state(7)` (case 7): the gathering starts (`+0x80`), the timer clears; without the
    /// game's farm setting the unit's place is `mGatherX`, `mGatherY`. A load of another resource
    /// is dropped unless both are food (`FUN_14119d310` with 0; then the unit keeps the kind it
    /// carries). The unit takes the working sprite (slot 0x68; the standing one for none); the
    /// gathering sound (slot 0x4c0) is not held.
    pub(super) fn set_work(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        gather_reads: &GatherReads,
    ) {
        self.state = GatherState::Work;
        self.worked = true;
        self.timer = Seconds::ZERO;
        if gather_reads.farm_mode == FarmMode::Fixed {
            let at = unit.place.position();
            self.gather_at = at.ground();
        }
        let kind = self.task.resource;
        let held = unit.life.attribute_kind;
        if held != kind && !both_food(kind, held) {
            unit.life.attribute_kind = kind;
            unit.life.amount = Amount::ZERO;
        }
        Self::show(
            step,
            unit,
            gather_reads
                .sprites
                .working
                .or(gather_reads.sprites.standing),
        );
    }

    /// State 7 (case 6) with one stop: the work, `(work rate) × work_val1 × dt`, at most what the
    /// target holds, times the productivity. A load that would pass the capacity ends the
    /// gathering ([`Next::Drop`]); else the target loses the work, its last bit (above 0, below
    /// 0.1) going with a thousandth of its master's first storage amount, the unit gains, and a
    /// remainder below 0.1 of the capacity rounds the load up. A farm's food under the player's
    /// attribute 96 is sold at once: the player gains it as food (slot 0xf8 with 1,
    /// [`Step::give_load`]); else a full load, or an empty target that cannot be gathered empty
    /// (the combat object's slot 0x2b8), ends it. The game's `+0xc14` (`FUN_140c884c0`) is 0,
    /// as the tick oracle holds it.
    ///
    /// # Errors
    ///
    /// The sale's.
    pub(super) fn gather(
        &self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        target: &mut Victim,
        gather_reads: &GatherReads,
        roles: &Roles,
    ) -> GenieResult<Next> {
        let held = &mut unit.life.amount;
        let work = Amount::new((gather_reads.work_rate * self.task.work_value) * step.dt.seconds());
        let take = if work <= target.amount {
            work
        } else {
            target.amount
        };
        let capacity = gather_reads.capacity;
        let mut gain = gather_reads.productivity.get() * take;
        if capacity < gain + *held {
            return Ok(Next::Drop);
        }
        target.amount += -take;
        // the last bit and the storage's share summed first (`0x140a8f751`)
        if target.amount < REMAINDER && Amount::ZERO < target.amount {
            gain += target.storage * LAST_BIT + target.amount;
            target.amount = Amount::ZERO;
        }
        *held += gain;
        let over = (*held - capacity).abs();
        if *held < capacity && over < REMAINDER && over <= target.amount {
            target.amount += -over;
            *held += over;
        }
        // a farm's food sold at once (attribute 96): the load goes to the player as food (slot
        // 0xf8 with 1), the unit holds none, and the gathering goes on
        if Amount::ZERO < gather_reads.food_sold
            && roles.of_class(target.class).contains(ClassRoles::FARM)
            // The food kind a farm holds (`+0x308` 16): the food sold at once.
            && roles.of_some_attribute(unit.life.attribute_kind).contains(AttributeRoles::FARM_FOOD)
        {
            let sold = mem_take(&mut unit.life.amount);
            step.give_load(step.w.model.slots.attribute.food, sold)?;
            return Ok(Next::Stay);
        }
        let empty = target.amount <= Amount::ZERO
            && !roles
                .of_master(target.master_id)
                .contains(MasterRoles::REGATHERED);
        Ok(if over < REMAINDER || empty {
            Next::Drop
        } else {
            Next::Stay
        })
    }

    /// State 8's update (case 7): the `MoveTo` to the drop site walks ([`GatherAction::update_list`]);
    /// once it ended (its update in state 1 returns 1: the list deletes it and its destructor
    /// stops the unit) the load goes
    /// to the player at a drop site that takes it (`site`) within `GatherTuning::drop_reach` (squared): the
    /// resource of what the unit carries ([`crate::gathering::hunt::resource_kind`]), which the caller adds;
    /// the unit carries nothing after (`FUN_14119d310` with −1). Returns the resource and the
    /// amount; the caller lets the drop site go and walks back to the target (`set_state(4)`).
    /// A `MoveTo` that failed or let its target go (codes 3 to 5) waits at random
    /// ([`GatherAction::random_wait`]). A site that does not take the load idles (`set_state` 2),
    /// one out of reach waits at random.
    ///
    /// # Errors
    ///
    /// What the drop-off does not model.
    pub(crate) fn update_drop(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        gather_reads: &GatherReads,
        site: Option<DropOff>,
        roles: &Roles,
    ) -> GenieResult<Dropped> {
        if self.timer < Seconds::ZERO {
            self.timer += step.dt.seconds();
            if self.timer < Seconds::ZERO {
                return Ok(Dropped::Going);
            }
            self.timer = Seconds::ZERO;
            return Ok(Dropped::Again);
        }
        // the list's code: under way walks or waits, done ends it; failed or its target gone
        // waits at random
        match self.update_list(step, unit, me, gather_reads, roles)? {
            SubCode::UnderWay => return Ok(Dropped::Going),
            SubCode::Failed | SubCode::TargetGone => {
                self.random_wait(step, unit);
                return Ok(Dropped::Going);
            }
            SubCode::Done => self.tear_down(step.w.parts.mover_mut(unit.id))?,
        }
        // the drop site gone: set_state(8) again
        let Some(site) = site else {
            return Ok(Dropped::Again);
        };
        if !site.takes {
            self.set_idle(step, unit, gather_reads);
            return Ok(Dropped::Idle);
        }
        if step.w.model.tuning.gather.drop_reach < site.gap_squared.get() {
            self.random_wait(step, unit);
            return Ok(Dropped::Going);
        }
        let attributes = step.w.players.player(step.player)?.attributes.len();
        let kind = resource_kind(unit.life.attribute_kind, attributes, step.w.model.slots);
        let amount = unit.life.amount;
        unit.life.attribute_kind = None;
        unit.life.amount = Amount::ZERO;
        Ok(Dropped::Load(kind, amount))
    }

    /// `LAB_140a8ef80`: the timer takes a draw of the Action stream (`GetRandFloat0to1`) less a
    /// half, and the unit's animation stops at once (`FUN_141161850`: its `+0x7b8` 0).
    pub(super) fn random_wait(&mut self, step: &mut Step<'_>, unit: &Unit) {
        self.timer = Seconds::new(
            step.w.streams.get(Stream::Action).float01() - step.w.model.tuning.gather.wait_shift,
        );
        step.w.parts.set_anim_speed(unit.id, Speed::ZERO);
    }
}
