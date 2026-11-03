//! The hunt's search for another target when its class is one the unit AI does not attack
//! (trees): the closest tree from the hunt's drop site, which the hunt then takes.

use crate::GenieError;
use crate::combat::attack::BeginLayer;
use crate::combat::attack::{arms, engagement, show_arrived};
use crate::gathering::hunt::HuntRetarget;
use crate::gathering::hunt::{NO_HUNT, Quarry};
use crate::map::Distance;
use crate::map::Position;
use crate::math::fmath::sqrt;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::WorkerCount;
use crate::objects::layers::Action;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::{GatherGroup, MasterTables};
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::unit_ai::task_finder::allied;
use crate::world::HUNTER_UNMOVING;
use crate::world::layer::Roles;
use crate::world::lets::take;
use crate::world::others::OthersRef;
use crate::world::step::Step;
use std::cmp::Ordering;

impl Unit {
    /// The hunt's search for another target (`FUN_140ac4670`) when the class its unit AI targets
    /// (else the hunt's) is one the AI's slot 0x1b0 gives 0 (the civilian's `FUN_140c1c4f0`:
    /// trees, 15): with `within` (its first argument above 0; 8 when the target went on the walk
    /// to it) only a hunt within it of its point searches (the 3D distance, its components summed
    /// x, height, y); the unit AI's `ClosestResourceObject` with gather group 0, the hunt's second
    /// target as the drop site, at most 2 workers ([`Unit::resource_search_from`]) names the next
    /// tree; with `near` (its second argument above 0; 5 when the Attack's `MoveTo` failed) a tree
    /// farther than it from the unit (the same 3D distance) does not count. The hunt takes it
    /// (slot 0x48: [`crate::gathering::hunt::HuntAction::retarget`], the new target taken). True when it
    /// found one.
    pub(crate) fn hunt_tree_search(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        within: Option<Distance>,
        near: Option<Distance>,
        roles: &Roles,
    ) -> Result<bool, GenieError> {
        // NaN compares false as the game's `comiss` does
        let beyond = |a: Position, b: Position, limit: Distance| {
            let (dx, dz, dy) = (a.x() - b.x(), a.z() - b.z(), a.y() - b.y());
            let d = sqrt(dx * dx + dz * dz + dy * dy);
            d.partial_cmp(&limit.get()) != Some(Ordering::Less)
        };
        let hunt = actions.list.hunt().ok_or(NO_HUNT)?;
        if let Some(within) = within
            && beyond(hunt.point, self.position(), within)
        {
            return Ok(false);
        }
        let site = hunt.carry_to;
        let Some(found) = self.resource_search_from(
            actions,
            c,
            GatherGroup::WOOD,
            WorkerCount::of_data(2),
            site,
        )?
        else {
            return Ok(false);
        };
        let (tp, t) = OthersRef::of(&c.w.players, &c.w.index, c.player)?
            .find(found)
            .ok_or(GenieError::unsupported(
                "a hunt's next tree the slice does not hold",
            ))?;
        if let Some(near) = near {
            let (dx, dz, dy) = (
                self.position().x() - t.position().x(),
                self.position().z() - t.position().z(),
                self.position().y() - t.position().y(),
            );
            if near.get() < sqrt(dx * dx + dz * dz + dy * dy) {
                return Ok(false);
            }
        }
        let target_master = t.master(c.w.master_tables(), tp)?;
        let own =
            c.w.spatial
                .relations
                .get(c.player)
                .ok_or(GenieError::unsupported(
                    "a hunter's player without relations",
                ))?;
        let quarry = Quarry {
            id: found,
            position: t.position(),
            class: target_master.class,
            kind: target_master.kind,
            state: t.life.state,
            player: tp,
            amount: t.life.amount,
            allied: allied(own, c.player, tp, true),
            top_action: c.w.parts.actions_of(t.id).top_action_kind(),
            drop_site: false,
        };
        let master = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.model.masters.of(c.player),
        )?;
        let drops_here = master.drop_sites.contains(&t.life.master_id);
        let at = |id: ObjectId| {
            c.w.spatial
                .get_known(id)?
                .ok_or(GenieError::unsupported("a hunt's object without a place"))
        };
        let (me_at, them) = (at(self.id)?, at(found)?);
        let speed =
            c.w.parts
                .mover_of(self.id)
                .map_or(Speed::ZERO, |m| m.stats.speed);
        let engagement = engagement(
            c.w.parts.unit_ai.get(self.id),
            speed,
            master,
            me_at,
            them,
            c.w.world_time,
        )?;
        let walks = self.move_walks(
            c.w.parts.mover_of(self.id),
            MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
            c.player,
            (
                &c.w.model.graphics,
                c.w.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        let hunt = actions.list.hunt_mut().ok_or(NO_HUNT)?;
        let old = hunt.target;
        let moving = hunt.task.walk_sprite_id(self.life.held());
        let arms = arms(
            master,
            &c.w.model.graphics,
            hunt.task.sprites,
            moving,
            roles,
        );
        let mover = c.w.parts.mover_mut(self.id).ok_or(HUNTER_UNMOVING)?;
        if let Some(arrived) = hunt.retarget(
            HuntRetarget {
                target: &quarry,
                player: c.player,
                walks,
                drops_here,
            },
            &engagement,
            arms,
            mover,
            BeginLayer {
                roles,
                attack_tuning: &c.w.model.tuning.attack,
                move_to_tuning: &c.w.model.tuning.move_to,
            },
            &c.w.model.tuning.hunt,
        )? {
            if arrived {
                show_arrived(
                    &arms,
                    &mut self.sprite,
                    c.w.parts.motion_mut(self.id),
                    c.w.streams.get(Stream::Sprite),
                    c.w.model.tuning.sprite,
                )?;
            }
            // slot 0x68 (FUN_140a7a230): the old target goes, the new one is taken
            self.hunt_release(c, [old])?;
            let by = self.releaser(c)?;
            take(c, self, found, by, roles)?;
        }
        Ok(true)
    }
}
