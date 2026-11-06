//! The states the update and the unit's stop set: the prelude, the work's end and what
//! follows it, the restart, the idle, a retarget, the halt and the end; the builder count and
//! the load the work's end drops.

use crate::construction::build::BuildState;
use crate::construction::build::{BuildAction, BuildReads, drop_done_orders};
use crate::gathering::DropSiteId;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::move_to::MoveToAction;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerKind;
use crate::players::attribute::AttributeId;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::{AiAction, OrderKind};
use crate::{GenieError, GenieResult};

use crate::construction::build::BUILDING_UNPLACED;
use crate::construction::{BuilderCount, FoundationId};
use crate::movement::Mover;
use crate::objects::layers::Action;
use crate::objects::masters::OwnerMasters;
use crate::orders::Work;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::lets::release;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use std::mem::replace;

impl BuildAction {
    /// The foundation as the update reads it ([`BuildReads::site`]): the builder's player's
    /// object, None once the player no longer holds it.
    pub(super) fn site_unit<'world>(
        step: &'world Step<'_>,
        reads: &BuildReads,
    ) -> GenieResult<Option<&'world Unit>> {
        let _ = step.world.players.player(step.player)?;
        Ok(reads
            .site
            .and_then(|id| step.world.live_object(step.player, id)))
    }

    /// The builder count (`+0x9e8`) of the building `FUN_140aaddb0` gives a Build on `site`
    /// ([`crate::world::World::counted_building`]: an annex's main building) changes by `by`
    /// at once; nothing once the world no longer holds it.
    ///
    /// # Errors
    ///
    /// An object of a master its owner lacks.
    pub(super) fn count(
        step: &mut Step<'_>,
        site: Option<FoundationId>,
        by: i8,
    ) -> GenieResult<()> {
        if let Some(id) = step.world.counted_building(site)?
            && step.world.object(id).is_some()
        {
            let builder_count = step.world.parts.repairers_mut(id)?;
            *builder_count = builder_count.changed_by(by);
        }
        Ok(())
    }

    /// The builders the work's share reads (`+0x9e8` of `FUN_140aaddb0`'s building for the
    /// foundation `site`), 0 once the world no longer holds it.
    ///
    /// # Errors
    ///
    /// An object of a master its owner lacks.
    pub(super) fn builders(
        step: &Step<'_>,
        site: Option<FoundationId>,
    ) -> GenieResult<BuilderCount> {
        Ok(step
            .world
            .counted_building(site)?
            .and_then(|id| step.world.object(id))
            .map_or(BuilderCount::NONE, |(_, unit)| {
                step.world.parts.repairers_of(unit.id)
            }))
    }

    /// A counted builder leaves the builders of its foundation ([`BuildAction::count`]) and
    /// is no longer counted.
    ///
    /// # Errors
    ///
    /// [`BuildAction::count`]'s.
    pub(super) fn uncount(&mut self, step: &mut Step<'_>) -> GenieResult<()> {
        if self.added {
            Self::count(step, self.target, -1)?;
            self.added = false;
        }
        Ok(())
    }

    /// The prelude of every `set_state` (`0x140aae200`): a counted builder leaves its
    /// foundation's builders ([`BuildAction::uncount`]), and the sub-actions go
    /// (`FUN_140a98620`: a `MoveTo`'s teardown stops the unit, slot 0x5d0).
    ///
    /// # Errors
    ///
    /// The builder count's, and the unit's stop that the motion step does not model.
    pub(super) fn leave(&mut self, step: &mut Step<'_>, builder: &Unit) -> GenieResult<()> {
        self.uncount(step)?;
        if self.state.take_move_to().is_some()
            && let Some(mover) = step.world.parts.mover_mut(builder.id)
        {
            mover.stop()?;
        }
        Ok(())
    }

    /// The load a builder drops (`FUN_140aadc50`): its player's slot 0xf8 with 1 and 1 on
    /// `kind` and `amount` ([`Step::give_load`]), then it carries nothing (`FUN_14119d310` with
    /// −1 and 0).
    ///
    /// # Errors
    ///
    /// [`Step::give_load`]'s.
    pub(super) fn drop_load(
        step: &mut Step<'_>,
        builder: &mut Unit,
        kind: AttributeId,
        amount: Amount,
    ) -> GenieResult<()> {
        step.give_load(kind, amount)?;
        builder.life.attribute_kind = None;
        builder.life.amount = Amount::ZERO;
        Ok(())
    }

    /// `set_state(224)` (`0x140aae200` case 0xe0): the work ended. A builder carrying something
    /// (`+0x310` above 0, its kind not −1) on a drop site of its civ (`FUN_140fde180`,
    /// [`BuildReads::drop_site`]) drops it ([`BuildAction::drop_load`], the food kinds as food).
    ///
    /// # Errors
    ///
    /// The prelude's and the drop's.
    pub(super) fn finish(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
    ) -> GenieResult<()> {
        self.leave(step, builder)?;
        self.state = BuildState::Finished;
        let (amount, kind) = (builder.life.amount, builder.life.attribute_kind);
        let Some(kind) = kind.filter(|_| reads.drop_site && Amount::ZERO < amount) else {
            return Ok(());
        };
        // the foods a unit carries (15 to 17) as food
        let kind = if kind.is_food_load() {
            step.world.model.slots.attribute.food
        } else {
            kind
        };
        Self::drop_load(step, builder, kind, amount)
    }

    /// Case 0xdf of the update, the work ended (224): a villager works the farm it built (class
    /// 49 but the fish trap, `FUN_141194b20`: a pasture too), a fishing ship its fish trap (slot
    /// 0x7e8 on it, at its place: [`BuildAction::works_it`]); another builder of a pasture (1893,
    /// 1897, slot 0x780 against its owner's attribute 290) is not modelled; the builder's unit
    /// AI drops its build orders on foundations gone or built (`FUN_140c5dcd0`); with orders
    /// left in its queue (`FUN_140c564a0`) it has no order and no action (`FUN_140c64b60`,
    /// `FUN_140c64b50` with −1) and the Build ends (`set_state(1)`, then the update returns 1).
    /// A builder without queued orders looks for more work around the building
    /// (`FUN_1415cc4e0`, [`BuildAction::search`]). A computer player's builder is not modelled.
    ///
    /// # Errors
    ///
    /// What it does not model.
    pub(super) fn after_work(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        actions: &mut Action,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        if step.world.players.player(step.player)?.kind.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's builder after its work",
            ));
        }
        let site = Self::site_unit(step, reads)?.ok_or(GenieError::unsupported(
            "the end of a Build on a foundation the slice does not read",
        ))?;
        let master = site.master(step.world.master_tables(), step.player)?;
        // LAB_140ab09af: a villager works the farm it built (FUN_141194b20, FUN_141195750), a
        // fishing ship its fish trap (FUN_141194640, FUN_1411945a0): its unit AI drops its build
        // orders (FUN_140c5e020 with 0x2be), then slot 0x7e8 on the building
        let farm = roles.of_class(master.class).contains(ClassRoles::FARM)
            && !roles.of_master(master.id).contains(MasterRoles::FISH_TRAP);
        let works_it = (farm
            && roles
                .of_some_class(reads.class)
                .contains(ClassRoles::VILLAGER))
            || (roles.of_master(master.id).contains(MasterRoles::FISH_TRAP)
                && roles
                    .of_master(builder.life.master_id)
                    .contains(MasterRoles::TRAPPER));
        // a pasture is a farm to the test, so a villager works it as any farm; another builder
        // works it when its livestock's workers (slot 0x780, FUN_1411c5dd0) are its owner's
        // attribute 290
        if !works_it
            && roles
                .of_master(master.id)
                .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE)
        {
            return Err(GenieError::unsupported(
                "the end of a Build on a pasture (its animals, slot 0x780)",
            ));
        }
        // the search reads the building of its target (`+0x20`) and its master (`+0xa0`)
        let (built, found) = (
            Some((site.id, site.life.state)),
            (FoundationId::new(site.id), site.life.master_id),
        );
        let ai = step
            .world
            .parts
            .unit_ai
            .get_mut(builder.id)
            .ok_or(GenieError::unsupported(
                "a builder without a unit AI after its work",
            ))?;
        if works_it {
            ai.orders
                .retain(|queued_order| queued_order.order != OrderKind::Build);
            self.works_it(actions, step, builder)?;
            return Ok(HeadCode::Keeps);
        }
        // FUN_140c5dcd0 (the building just completed is in state 2 now)
        drop_done_orders(ai, built, &reads.build_targets);
        if ai.orders.is_empty() {
            self.search(step, builder, actions, (found, reads), roles)?;
            return Ok(HeadCode::Keeps);
        }
        ai.end_order_and_action()?;
        self.end(step, builder, BuildState::Done)?;
        Ok(HeadCode::Deletes)
    }

    /// Slot 0x7e8 on the building the builder built, at its place, inside the update (the
    /// builder lent back to its slot: [`World::order_work`], which replaces the Build); the
    /// update then answers 0.
    ///
    /// [`World::order_work`]: crate::world::World::order_work
    fn works_it(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        builder: &mut Unit,
    ) -> GenieResult<()> {
        let point = step
            .world
            .spatial
            .get(self.target)
            .ok_or(BUILDING_UNPLACED)?
            .position;
        let work = Work::on(self.target.map(FoundationId::id), point);
        self.lend(step, builder, actions, |world, player_id, list_index| {
            world.order_work(player_id, list_index, &work)
        })
    }

    /// The Build's case 0xdf after the work: the builder's search for more work around the building
    /// `site` of master `built` it finished ([`Unit::more_work`]: its slot 0x7e8 inside the update,
    /// the builder lent back to its slot, [`World::order_work`]); without one the Build lets the
    /// building go and waits ([`BuildAction::no_more_work`]; `set_state(3)` without orders: notify
    /// 0x202, then the standing sprite).
    ///
    /// # Errors
    ///
    /// What the search and the work command do not model.
    ///
    /// [`World::order_work`]: crate::world::World::order_work
    fn search(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        actions: &mut Action,
        ((site, built), reads): ((FoundationId, MasterId), &BuildReads),
        roles: &Roles,
    ) -> GenieResult<()> {
        if let Some(work) = builder.more_work(actions, step, site.id(), built, roles)? {
            return self.lend(step, builder, actions, |world, player_id, list_index| {
                world.order_work(player_id, list_index, &work)
            });
        }
        self.no_more_work()?;
        release(step, builder, site, reads.by)?;
        builder.post_own(
            &mut step.world.parts.unit_ai,
            NotifyKind::REQUIRES_SEARCH,
            [AiAction::Build.value(), 0, 0],
        )?;
        Self::stand(step, builder)
    }

    /// The unit's slot 0x68 with its master's standing sprite (`+0x470`,
    /// [`Unit::show_standing`]) at once.
    ///
    /// # Errors
    ///
    /// [`Unit::show_standing`]'s.
    pub(super) fn stand(step: &mut Step<'_>, builder: &mut Unit) -> GenieResult<()> {
        builder.show_standing(
            step.world.parts.motion.layer_mut(builder.id),
            OwnerMasters::new(
                &step.world.parts.own_master,
                step.world.model.masters.of(step.player),
            ),
            &step.world.model.graphics,
            step.world.streams.get_mut(Stream::Sprite),
            step.world.model.tuning.sprite,
        )
    }

    /// `set_state(4)` (`0x140aae200` case 4) toward its foundation `self.target`: the prelude
    /// (it leaves the foundation's builders, its `MoveTo` goes: the unit stops), then a new
    /// `MoveTo` to the foundation (`FUN_140aa0df0`, [`BuildAction::walk_to_foundation`]), arrived
    /// when the footprints touch; the foundation's place becomes the point. A foundation in a
    /// state above 6 the `MoveTo` refuses (`FUN_140a7a230`) and walks to its place,
    /// far from it. Without a foundation the point stays and the `MoveTo` has no target: it
    /// walks to its zeroed point, far from it (`FUN_14118f680` without a target: `FLT_MAX`), in
    /// state 13 for a unit without speed (`FUN_140aa2100`).
    ///
    /// # Errors
    ///
    /// The prelude's, a builder without movement state, a foundation without a place.
    pub(super) fn restart(
        &mut self,
        step: &mut Step<'_>,
        builder: &Unit,
        roles: &Roles,
    ) -> GenieResult<()> {
        self.leave(step, builder)?;
        let (to, walk) = self.walk_to_foundation(step, builder, roles)?;
        *self = self.restarted(
            self.target,
            to.map_or(self.point, |walk_to| walk_to.at),
            walk,
        );
        Ok(())
    }
}

impl BuildAction {
    /// `set_state(3)` (`0x140aae200` case 3): the prelude (out of the foundation's builders, the
    /// `MoveTo`'s teardown), then with a unit AI its build orders on foundations gone or no
    /// longer in state 0 leave its queue (`FUN_140c5dcd0`, the foundation's state as it is now);
    /// with orders left (`FUN_140c564a0`) it has no order and no action (`FUN_140c64b60`,
    /// `FUN_140c64b50` with −1) and the Build ends (`set_state(1)`: the update after removes
    /// it); with none it posts notify 0x202 to its unit AI and takes its standing sprite (slot
    /// 0x68).
    ///
    /// # Errors
    ///
    /// A unit AI on a patrol order, and what the prelude and the sprite do not model.
    pub(super) fn idle(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
    ) -> GenieResult<()> {
        let built = Self::site_unit(step, reads)?.map(|unit| (unit.id, unit.life.state));
        self.leave(step, builder)?;
        self.state = BuildState::Idle;
        let mut left = 0;
        if let Some(ai) = step.world.parts.unit_ai.get_mut(builder.id) {
            drop_done_orders(ai, built, &reads.build_targets);
            left = ai.orders.len();
            if left != 0 {
                ai.end_order_and_action()?;
                self.end(step, builder, BuildState::Done)?;
            }
        }
        if left == 0 {
            builder.post_own(
                &mut step.world.parts.unit_ai,
                NotifyKind::REQUIRES_SEARCH,
                [AiAction::Build.value(), 0, 0],
            )?;
            Self::stand(step, builder)?;
        }
        Ok(())
    }

    /// Slot 0x48 (`FUN_140ab1b30`), the work command's target for the Build's own task, here its
    /// own foundation again (slot 0x68 with the same target does nothing): working (state 6) it
    /// stays, as does a computer player's builder walking to it (state 4); else `set_state(4)`:
    /// it leaves the builders of `FUN_140aaddb0`'s building (`builders`, an annex's main
    /// building's) and its `MoveTo` (the unit stops, slot 0x5d0), and the new `MoveTo` to the
    /// foundation at `at` starts (`walk`, [`BuildTask::walk`]).
    ///
    /// [`BuildTask::walk`]: crate::construction::build::BuildTask::walk
    ///
    /// # Errors
    ///
    /// A builder counted among the builders of a foundation the caller does not hold.
    pub fn retake(
        &mut self,
        at: Position,
        walk: Option<MoveToAction>,
        mover: Option<&mut Mover>,
        builders: Option<&mut BuilderCount>,
        player_kind: PlayerKind,
    ) -> GenieResult<()> {
        if self.state == BuildState::Work
            || (matches!(self.state, BuildState::Move { .. }) && player_kind.is_computer())
        {
            return Ok(());
        }
        if self.added {
            let builder_count = builders.ok_or(GenieError::unsupported(
                "a builder retaking a foundation the slice does not hold",
            ))?;
            *builder_count = builder_count.left();
        }
        if self.state.move_to().is_some()
            && let Some(mover) = mover
        {
            mover.stop()?;
        }
        *self = self.restarted(self.target, at, walk);
        Ok(())
    }

    /// Slot 0x48 (`FUN_140ab1b30`) on another foundation of the builder's player: slot 0x68
    /// (`FUN_140ab0300`) takes it (the caller's builder count, release and take; the Build is no
    /// longer among the builders) and `set_state(4)` starts over: its `MoveTo` goes (the unit
    /// stops, slot 0x5d0) and the new one to the foundation `target` at `at` starts (`walk`,
    /// [`BuildTask::walk`]).
    ///
    /// [`BuildTask::walk`]: crate::construction::build::BuildTask::walk
    ///
    /// # Errors
    ///
    /// The unit's stop that the motion step does not model.
    pub fn retarget(
        &mut self,
        target: FoundationId,
        at: Position,
        walk: Option<MoveToAction>,
        mover: Option<&mut Mover>,
    ) -> GenieResult<()> {
        if self.state.move_to().is_some()
            && let Some(mover) = mover
        {
            mover.stop()?;
        }
        *self = self.restarted(Some(target), at, walk);
        Ok(())
    }

    /// Slot 0x38 (`FUN_140ab0350`) as the unit's stop calls it: `set_state(2)` (`0x140aae200`):
    /// a counted builder leaves its foundation's builders first (the caller's,
    /// [`crate::world::World::leave_builders`]: true when it counted), the sub-actions go (a
    /// `MoveTo`'s teardown stops the unit), the unit takes its standing sprite (slot 0x68 with the
    /// master's `+0x470`, the caller's), the foundation and the second target go (slot 0x68 and
    /// `FUN_140a7a190` with none: the caller releases both) and the point is −1. The answer is
    /// true, so the stop keeps the list; returns whether it counted, the foundation and the
    /// second target (its drop site).
    ///
    /// # Errors
    ///
    /// The unit's stop that the motion step does not model.
    pub(crate) fn halt(
        &mut self,
        mover: Option<&mut Mover>,
    ) -> GenieResult<(bool, Option<FoundationId>, Option<DropSiteId>)> {
        let counted = replace(&mut self.added, false);
        if self.state.take_move_to().is_some()
            && let Some(mover) = mover
        {
            mover.stop()?;
        }
        self.point = Position::from([-1.0; 3]);
        self.state = BuildState::Halted;
        Ok((counted, self.target.take(), self.site.take()))
    }

    /// `set_state(2)` from the update ([`BuildAction::halt`]): a counted builder leaves its
    /// foundation's builders ([`BuildAction::count`]), the unit takes its standing sprite, and
    /// the foundation and the second target are let go (their slot 0x180), all at once.
    ///
    /// # Errors
    ///
    /// The builder count's, the sprite's and the releases'.
    pub(super) fn halt_in(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
    ) -> GenieResult<()> {
        let (counted, target, second) = self.halt(step.world.parts.mover_mut(builder.id))?;
        if counted {
            Self::count(step, target, -1)?;
        }
        Self::stand(step, builder)?;
        release(step, builder, target, reads.by)?;
        release(step, builder, second, reads.by)?;
        Ok(())
    }

    /// The search for more work found nothing (`0x140ab0f00`): slot 0x68 with none lets the
    /// foundation go (the caller's) and `set_state(3)` waits (the caller's notify 0x202 and
    /// standing sprite: its unit AI has no orders, or the search would not have run).
    ///
    /// # Errors
    ///
    /// A Build with its `MoveTo` (`set_state`'s teardown is not modelled for it).
    pub const fn no_more_work(&mut self) -> GenieResult<()> {
        if self.state.move_to().is_some() {
            return Err(GenieError::unsupported("a Build waiting with its MoveTo"));
        }
        self.target = None;
        self.state = BuildState::Idle;
        Ok(())
    }

    /// `set_state(1)` and `set_state(13)` (`0x140aae200` cases 1 and 0xd, one case): the prelude
    /// ([`BuildAction::leave`]), then the builder takes its standing sprite (slot 0x68 with its
    /// master's `+0x470`) and, with a target, its owner's slot 0x380 hears of it
    /// (`FUN_141618d30`: only a computer player's AI listens).
    ///
    /// # Errors
    ///
    /// The prelude's and the sprite's.
    pub(super) fn end(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        state: BuildState,
    ) -> GenieResult<()> {
        self.leave(step, builder)?;
        self.state = state;
        Self::stand(step, builder)
    }
}
