//! The hunt's search for another target when its class is one the unit AI does not attack
//! (trees): the closest tree from the hunt's drop site, which the hunt then takes.

use crate::combat::attack::BeginLayer;
use crate::combat::attack::{arms, engagement, show_arrived};
use crate::gathering::gather::unit::WalkSprites;
use crate::gathering::hunt::HuntRetarget;
use crate::gathering::hunt::TreeSearch;
use crate::gathering::hunt::{HUNT_OBJECT_UNPLACED, HuntAction, Quarry};
use crate::map::Distance;
use crate::map::Position;
use crate::math::fmath::sqrt;
use crate::math::rng::Stream;
use crate::movement::MoveStart;
use crate::objects::ObjectId;
use crate::objects::WorkerCount;
use crate::objects::layers::Action;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::{GatherGroup, MasterTables};
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::AllyTest;
use crate::unit_ai::task_finder::allied;
use crate::world::HUNTER_UNMOVING;
use crate::world::layer::Roles;
use crate::world::lets::take;
use crate::world::others::OthersRef;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};
use std::cmp::Ordering;

impl Unit {
    /// The hunt's search for another target (`FUN_140ac4670`) when the class its unit AI targets
    /// (else the hunt's) is one the AI's slot 0x1b0 gives 0 (the civilian's `FUN_140c1c4f0`: trees,
    /// 15): with `within` (its first argument above 0; 8 when the target went on the walk to it)
    /// only a hunt within it of its point searches (the 3D distance, its components summed x,
    /// height, y); the unit AI's `ClosestResourceObject` with gather group 0, the hunt's second
    /// target as the drop site, at most 2 workers ([`Unit::resource_search_from`]) names the next
    /// tree; with `near` (its second argument above 0; 5 when the Attack's `MoveTo` failed) a tree
    /// farther than it from the unit (the same 3D distance) does not count. The hunt takes it (slot
    /// 0x48: [`crate::gathering::hunt::HuntAction::retarget`], the new target taken).
    /// [`TreeSearch::Found`] when it found one.
    #[expect(
        clippy::too_many_lines,
        reason = "the hunt's search for another target (`FUN_140ac4670`) ported as one function, its branches in the game's order"
    )]
    pub(crate) fn hunt_tree_search(
        &mut self,
        (hunt, actions): (&mut HuntAction, &mut Action),
        step: &mut Step<'_>,
        within: Option<Distance>,
        near: Option<Distance>,
        roles: &Roles,
    ) -> GenieResult<TreeSearch> {
        // NaN compares false as the game's `comiss` does
        let beyond = |from: Position, to: Position, limit: Distance| {
            let (dx, dz, dy) = (from.x() - to.x(), from.z() - to.z(), from.y() - to.y());
            let distance = sqrt(dx * dx + dz * dz + dy * dy);
            distance.partial_cmp(&limit.get()) != Some(Ordering::Less)
        };
        if let Some(within) = within
            && beyond(hunt.point, self.position(), within)
        {
            return Ok(TreeSearch::NoneFound);
        }
        let site = hunt.carry_to;
        let Some(found) = self.resource_search_from(
            actions,
            step,
            GatherGroup::WOOD,
            WorkerCount::of_data(2),
            site,
        )?
        else {
            return Ok(TreeSearch::NoneFound);
        };
        let (tp, unit) = OthersRef::of(&step.world.players, &step.world.index, step.player)?
            .find(found)
            .ok_or(GenieError::unsupported(
                "a hunt's next tree the slice does not hold",
            ))?;
        if let Some(near) = near {
            let (dx, dz, dy) = (
                self.position().x() - unit.position().x(),
                self.position().z() - unit.position().z(),
                self.position().y() - unit.position().y(),
            );
            if near.get() < sqrt(dx * dx + dz * dz + dy * dy) {
                return Ok(TreeSearch::NoneFound);
            }
        }
        let target_master = unit.master(step.world.master_tables(), tp)?;
        let own = step
            .world
            .spatial
            .relations
            .get(step.player)
            .ok_or(GenieError::unsupported(
                "a hunter's player without relations",
            ))?;
        let quarry = Quarry {
            id: found,
            position: unit.position(),
            class: target_master.class,
            kind: target_master.kind,
            state: unit.life.state,
            player: tp,
            amount: unit.life.amount,
            allied: allied(
                own,
                step.world.treaty.as_ref(),
                step.player,
                tp,
                AllyTest::WithRelation4,
            ),
            top_action: step.world.parts.actions_of(unit.id).top_action_kind(),
            drop_site: false,
        };
        let master = object_master(
            step.world.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            step.world.model.masters.of(step.player),
        )?;
        let drops_here = master
            .action_part()
            .drop_sites
            .contains(&unit.life.master_id);
        let at = |id: ObjectId| {
            step.world
                .spatial
                .get_known(id)?
                .ok_or(HUNT_OBJECT_UNPLACED)
        };
        let (me_at, them) = (at(self.id)?, at(found)?);
        let speed = step.world.top_speed(step.player, self)?;
        let engagement = engagement(
            step.world.parts.unit_ai.get(self.id),
            speed,
            master,
            me_at,
            them,
            step.world.world_time,
        )?;
        let walks = self.move_walks(
            step.world.parts.mover_of(self.id),
            step.world.top_speed_terms(step.player, self)?,
            WalkSprites {
                every: MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
                player: step.player,
                graphics: &step.world.model.graphics,
                look: step.world.parts.combat_of(self.id).abilities.look.as_ref(),
            },
            roles,
        )?;
        let old = hunt.target;
        let moving = hunt.task.walk_sprite_id(self.life.held());
        let arms = arms(
            master,
            &step.world.model.graphics,
            hunt.task.sprites,
            moving,
            roles,
        );
        let mover = step.world.parts.mover_mut(self.id).ok_or(HUNTER_UNMOVING)?;
        let retargeted = hunt.retarget(
            HuntRetarget {
                target: &quarry,
                player: step.player,
                walks,
                drops_here,
            },
            &engagement,
            arms,
            mover,
            BeginLayer {
                roles,
                attack_tuning: &step.world.model.tuning.attack,
                move_to_tuning: &step.world.model.tuning.move_to,
            },
            &step.world.model.tuning.hunt,
        );
        actions.list.set_hunt(Some(*hunt));
        if let Some(move_start) = retargeted? {
            if move_start == MoveStart::Arrived {
                show_arrived(
                    &arms,
                    &mut self.sprite,
                    step.world.parts.motion_mut(self.id),
                    step.world.streams.get_mut(Stream::Sprite),
                    step.world.model.tuning.sprite,
                )?;
            }
            // slot 0x68 (FUN_140a7a230): the old target goes, the new one is taken
            self.hunt_release(step, [old])?;
            let by = self.releaser(step)?;
            take(step, self, found, by, roles)?;
        }
        Ok(TreeSearch::Found)
    }
}
