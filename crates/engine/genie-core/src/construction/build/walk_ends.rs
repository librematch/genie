//! The ends of the builder's walk: at its end, failed, its foundation lost; the walk to a
//! point (state 11).

use crate::construction::build::BuildState;
use crate::construction::build::{
    BUILDER_UNMOVING, BuildAction, BuildReads, FOREIGN_SITE, SITE_GONE, UNSEEN_SITE,
};
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::movement::footprint_gap_squared;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::pathing::tile_path::Found;
use crate::pathing::tile_path::{Path, Request};
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::lets::release;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl BuildAction {
    /// The update's cases 3 and 0xde with the sub-action list's 1 or 2 (its `MoveTo` at its
    /// end, or no sub-action): off the footprint (223) it walks back to the foundation
    /// (`set_state(4)`, [`BuildAction::restart`]: in reach or not). In state 4, without a
    /// foundation, a villager, or within the task's range (the footprints' squared gap,
    /// `FUN_14118f8c0`, at most the range squared) or `BuildTuning::reach` of it, it works (`set_state(6)`;
    /// one without a foundation waits instead, `set_state(3)`); else it walks to the foundation
    /// again (`set_state(4)`).
    ///
    /// # Errors
    ///
    /// What the restart and the wait do not model.
    pub(super) fn walk_ended(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        if matches!(self.state, BuildState::Clear { .. }) {
            return self.restart(step, builder, roles);
        }
        if self.target.is_none() {
            return self.idle(step, builder, reads);
        }
        if !roles
            .of_some_class(reads.class)
            .contains(ClassRoles::VILLAGER)
        {
            let at = builder.position();
            let ground = step
                .world
                .parts
                .mover_of(builder.id)
                .map(|mover| mover.stats.radius.ground())
                .ok_or(BUILDER_UNMOVING)?;
            let placed = *step
                .world
                .spatial
                .get_known(self.target)?
                .ok_or(SITE_GONE)?;
            let gap = footprint_gap_squared(at, ground, placed.position, placed.radius);
            let range = self.task.range;
            if !(gap <= range.squared()
                || gap <= DistanceSquared::new(step.world.model.tuning.build.reach))
            {
                return self.restart(step, builder, roles);
            }
        }
        self.leave(step, builder)?;
        self.state = BuildState::Work;
        Ok(())
    }

    /// The update's case 3 or 0xde with its `MoveTo` failed (3): a foundation no builder started
    /// (`+0xc60`) with another player's marks on its footprint ([`Spatial::foreign_marks`]) while
    /// its owner sees it (`FUN_1411d4b80` with 1: its slot 0x1e0, [`Spatial::building_sight`])
    /// goes at once when its owner is human (`FUN_1411ccbb0`,
    /// [`crate::world::World::remove_building`]), the interface hears of it (`FUN_140c898a0`
    /// 0x88, not held) and `set_state(2)` halts the Build. Else the tile planner of the
    /// builder's terrain restriction (`FUN_140f7cb30`) asks for a path from where the builder
    /// last moved to (slot 0x520) to the foundation's place within their radii
    /// (`FUN_140f7c530`): a result below 2 walks again (`set_state(4)`), any other, no planner or
    /// no foundation waits (`set_state(3)`).
    ///
    /// # Errors
    ///
    /// Another player's foundation, one its owner sees only through its ghosts, and what the
    /// restart and the wait do not model.
    ///
    /// [`Spatial::building_sight`]: crate::map::spatial::Spatial::building_sight
    /// [`Spatial::foreign_marks`]: crate::map::spatial::Spatial::foreign_marks
    pub(super) fn walk_failed(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        if let Some(target) = self.target {
            let unit = Self::site_unit(step, reads)?.ok_or(FOREIGN_SITE)?;
            let desolid = step
                .world
                .parts
                .building_of(unit.id)
                .construction
                .is_some_and(|construction| construction.desolid);
            let placed = *step.world.spatial.get_known(target)?.ok_or(SITE_GONE)?;
            if desolid && step.world.spatial.foreign_marks(&placed) {
                let seen = step
                    .world
                    .spatial
                    .building_sight(target.id(), placed.position, placed.player)
                    .ok_or(UNSEEN_SITE)?
                    .seen();
                if seen {
                    self.remove_marked(step)?;
                    return self.halt_in(step, builder, reads);
                }
            }
        }
        let mover = step
            .world
            .parts
            .mover_of(builder.id)
            .ok_or(BUILDER_UNMOVING)?;
        let (restriction, ground, moved_at) = (
            mover.stats.restriction,
            mover.stats.radius.x(),
            mover.moved_at,
        );
        let mut found = Found::Nothing;
        // the foundation's place and its master's radius, read before the planner's borrow
        let known = step
            .world
            .spatial
            .get_known(self.target)
            .map(Option::<&_>::copied);
        let master_radius = match Self::site_unit(step, reads)? {
            Some(unit) => Some(
                unit.master(step.world.master_tables(), step.player)?
                    .radius
                    .x(),
            ),
            None => None,
        };
        if self.target.is_some()
            && let Some(planner) = step
                .world
                .pathing
                .planner(restriction, step.world.spatial.tiles())
        {
            let placed = known?.ok_or(SITE_GONE)?;
            let radius = master_radius.unwrap_or_else(|| placed.radius.x());
            let req = Request {
                from: moved_at.ground(),
                to: placed.position.ground(),
                radii: Distance::new(radius + ground),
                range: Distance::ZERO,
                closed: Vec::new(),
                full: false,
            };
            let mut path = Path::default();
            found = planner.plan::<GenieError>(
                &req,
                false,
                &mut path,
                None,
                &step.world.model.tuning.tile_path,
            )?;
        }
        if found.reaches() {
            self.restart(step, builder, roles)
        } else {
            self.idle(step, builder, reads)
        }
    }

    /// `FUN_1411d4b80`'s end on the foundation another player's marks lie on: a human owner's
    /// foundation goes at once (`FUN_1411ccbb0`, [`crate::world::World::remove_building`]).
    ///
    /// # Errors
    ///
    /// What the removal does not model.
    pub(super) fn remove_marked(&self, step: &mut Step<'_>) -> GenieResult<()> {
        if let Some(target) = self.target
            && let Some((player_id, _)) = step.world.object(target)
            && !step.world.players.player(player_id)?.kind.is_computer()
        {
            step.world.remove_building(player_id, target.id())?;
        }
        Ok(())
    }

    /// Its `MoveTo` returned 4 or 5 in state 4 or 223, or 4 in state 11 (`0x140ab0370` cases 3,
    /// 0xde and 10): slot 0x68 with none (`FUN_140ab0300`: a counted builder leaves the builders
    /// of `FUN_140aaddb0`'s building, the foundation is let go at once) and `set_state(11)`
    /// ([`BuildAction::walk_to_point`]).
    ///
    /// # Errors
    ///
    /// The prelude's and the release's.
    pub(super) fn lose_foundation(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        self.leave(step, builder)?;
        if self.target.is_some() {
            let target = self.target.take();
            release(step, builder, target, reads.by)?;
        }
        self.walk_to_point(step, builder, roles)
    }

    /// `set_state(11)` (`0x140aae200` case 0xb): the prelude ([`BuildAction::leave`]), then a
    /// quiet `MoveTo` to the action's point (`FUN_140aa0d30`: range 0, the task's moving sprite;
    /// `FUN_140a7a170` with 1, [`MoveToReads::to_point`]), which starts in state 3 (a `MoveTo`
    /// without a target is far from it), in 13 for a unit without speed.
    ///
    /// # Errors
    ///
    /// The prelude's, a builder without movement state or its master, a sprite the table lacks.
    ///
    /// [`MoveToReads::to_point`]: crate::movement::move_to::MoveToReads::to_point
    pub(super) fn walk_to_point(
        &mut self,
        step: &mut Step<'_>,
        builder: &Unit,
        roles: &Roles,
    ) -> GenieResult<()> {
        self.leave(step, builder)?;
        self.state = BuildState::WalkPoint { move_to: None };
        let move_reads = builder.move_to_reads(
            step.world.parts.motion.mover(builder.id),
            MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
            step.player,
            (
                &step.world.model.graphics,
                step.world
                    .parts
                    .combat_of(builder.id)
                    .abilities
                    .look
                    .as_ref(),
            ),
            roles,
        )?;
        self.state = BuildState::WalkPoint {
            move_to: move_reads.to_point(
                self.point,
                Distance::ZERO,
                self.task.moving,
                &step.world.model.tuning.move_to,
            )?,
        };
        Ok(())
    }
}
