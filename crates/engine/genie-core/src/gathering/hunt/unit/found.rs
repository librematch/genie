//! A hunter's object found: it becomes the hunt's target, and the sprite of a hunt that fails at once.

use crate::combat::attack::BeginLayer;
use crate::combat::attack::{arms, engagement, show_arrived};
use crate::gathering::gather::unit::WalkSprites;
use crate::gathering::hunt::HUNTER_STATE_LACKING;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::Quarry;
use crate::gathering::hunt::{HUNT_TARGET_UNPLACED, HUNTER_UNPLACED, HuntAction, HuntRetarget};
use crate::math::rng::Stream;
use crate::movement::MoveStart;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::MasterTables;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::players::AllyTest;
use crate::players::NO_RELATIONS;
use crate::unit_ai::task_finder::allied;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::lets::take;
use crate::world::others::OthersRef;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Unit {
    /// The object `found` becomes the hunt's target (slot 0x48,
    /// [`crate::gathering::hunt::HuntAction::retarget`] with the hunter's engagement and arms,
    /// the arrived sprite when it starts there), which takes it (its workers count one more; one
    /// of its own player's directly, never listed). A farm found is not modelled. A hunt that
    /// fails at once takes the standing sprite (slot 0x68 with `FUN_140ac5210`'s: the unit
    /// carries nothing now).
    pub(super) fn hunt_take_found(
        &mut self,
        (hunt, actions): (&mut HuntAction, &mut Action),
        step: &mut Step<'_>,
        found: ObjectId,
        roles: &Roles,
    ) -> GenieResult<()> {
        let master = self.master(step.world.master_tables(), step.player)?;
        let (owner, unit) = OthersRef::of(&step.world.players, &step.world.index, step.player)?
            .find(found)
            .ok_or(GenieError::unsupported(
                "a hunt's new target the slice does not hold",
            ))?;
        let target_master = unit.master(step.world.master_tables(), owner)?;
        let relations = step
            .world
            .spatial
            .relations
            .get(step.player)
            .unwrap_or(&NO_RELATIONS);
        if roles
            .of_master(unit.life.master_id)
            .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE)
        {
            return Err(GenieError::unsupported(
                "a hunt finding a farm (FUN_1411c7670)",
            ));
        }
        let quarry = Quarry {
            id: found,
            position: unit.position(),
            class: target_master.class,
            kind: target_master.kind,
            state: unit.life.state,
            player: owner,
            amount: unit.life.amount,
            allied: allied(relations, step.player, owner, AllyTest::WithRelation4),
            top_action: step.world.parts.actions_of(unit.id).top_action_kind(),
            drop_site: false,
        };
        let (me_at, them) = (
            step.world
                .spatial
                .get_known(self.id)?
                .ok_or(HUNTER_UNPLACED)?,
            step.world
                .spatial
                .get_known(found)?
                .ok_or(HUNT_TARGET_UNPLACED)?,
        );
        let speed = step.world.top_speed(step.player, self)?;
        let engagement = engagement(
            step.world.parts.unit_ai.get(self.id),
            speed,
            master,
            me_at,
            them,
            step.world.world_time,
        )?;
        let moving = hunt.task.walk_sprite_id(self.life.held());
        let arms = arms(
            master,
            &step.world.model.graphics,
            hunt.task.sprites,
            moving,
            roles,
        );
        let drops_here = master
            .action_part()
            .drop_sites
            .contains(&unit.life.master_id);
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
        let by = self.releaser(step)?;
        let Some(mover) = step.world.parts.mover_mut(self.id) else {
            return Err(HUNTER_STATE_LACKING);
        };
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
            take(step, self, found, by, roles)?;
        }
        self.hunt_failed_sprite(hunt, step)
    }

    /// A hunt that failed at once after its retarget: the unit shows standing, and a unit with
    /// a sprite takes its master's standing sprite (none in the data file is not modelled).
    fn hunt_failed_sprite(&mut self, hunt: &HuntAction, step: &mut Step<'_>) -> GenieResult<()> {
        if hunt.state != HuntState::Failed {
            return Ok(());
        }
        let standing = self
            .master(step.world.master_tables(), step.player)?
            .standing_sprite
            .and_then(|sprite_id| step.world.model.graphics.get(sprite_id))
            .copied();
        if let Some(mover) = step.world.parts.mover_mut(self.id) {
            mover.shows(standing.as_ref());
        }
        if self.sprite.is_some() {
            let standing = standing.ok_or(GenieError::unsupported(
                "a hunter without its standing sprite",
            ))?;
            new_sprite(
                &mut self.sprite,
                step.world.parts.move_stats_mut(self.id),
                standing,
                step.world.streams.get_mut(Stream::Sprite),
                step.world.model.tuning.sprite,
            );
        }
        Ok(())
    }
}
