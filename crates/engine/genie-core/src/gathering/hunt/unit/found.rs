//! A hunter's object found: it becomes the hunt's target, and the sprite of a hunt that fails at once.

use crate::GenieError;
use crate::combat::attack::{arms, engagement, show_arrived};
use crate::gathering::hunt::HuntRetarget;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::Quarry;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::MasterTables;
use crate::objects::masters::TaskSprites;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::players::NO_RELATIONS;
use crate::unit_ai::task_finder::allied;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::lets::take;
use crate::world::others::OthersRef;
use crate::world::step::Step;

impl Unit {
    /// The object `found` becomes the hunt's target (slot 0x48,
    /// [`crate::gathering::hunt::HuntAction::retarget`] with the hunter's engagement and arms,
    /// the arrived sprite when it starts there), which takes it (its workers count one more; one
    /// of its own player's directly, never listed). A farm found is not modelled. A hunt that
    /// fails at once takes the standing sprite (slot 0x68 with `FUN_140ac5210`'s: the unit
    /// carries nothing now).
    pub(super) fn hunt_take_found(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        found: ObjectId,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let master = self.master(c.w.master_tables(), c.player)?;
        let (owner, t) = OthersRef::of(&c.w.players, &c.w.index, c.player)?
            .find(found)
            .ok_or(GenieError::unsupported(
                "a hunt's new target the slice does not hold",
            ))?;
        let target_master = t.master(c.w.master_tables(), owner)?;
        let relations = c.w.spatial.relations.get(c.player).unwrap_or(&NO_RELATIONS);
        if roles
            .of_master(t.life.master_id)
            .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE)
        {
            return Err(GenieError::unsupported(
                "a hunt finding a farm (FUN_1411c7670)",
            ));
        }
        let quarry = Quarry {
            id: found,
            position: t.position(),
            class: target_master.class,
            kind: target_master.kind,
            state: t.life.state,
            player: owner,
            amount: t.life.amount,
            allied: allied(relations, c.player, owner, true),
            top_action: c.w.parts.actions_of(t.id).top_action_kind(),
            drop_site: false,
        };
        let (me_at, them) = (
            c.w.spatial
                .get_known(self.id)?
                .ok_or(GenieError::unsupported("a hunter without a place"))?,
            c.w.spatial
                .get_known(found)?
                .ok_or(GenieError::unsupported("a hunt's target without a place"))?,
        );
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
        let (sprites, moving) = actions.list.hunt().map_or((TaskSprites::NONE, None), |h| {
            let moving = h.task.walk_sprite_id(self.life.held());
            (h.task.sprites, moving)
        });
        let arms = arms(master, &c.w.model.graphics, sprites, moving, roles);
        let drops_here = master.drop_sites.contains(&t.life.master_id);
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
        let by = self.releaser(c)?;
        let (Some(hunt), Some(mover)) = (actions.list.hunt_mut(), c.w.parts.mover_mut(self.id))
        else {
            return Err(GenieError::unsupported(
                "a hunter without its hunt or movement state",
            ));
        };
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
            roles,
        )? {
            if arrived {
                show_arrived(
                    &arms,
                    &mut self.sprite,
                    c.w.parts.motion_mut(self.id),
                    c.w.streams.get(Stream::Sprite),
                )?;
            }
            take(c, self, found, by, roles)?;
        }
        self.hunt_failed_sprite(actions, c)
    }

    /// A hunt that failed at once after its retarget: the unit shows standing, and a unit with
    /// a sprite takes its master's standing sprite (none in the data file is not modelled).
    fn hunt_failed_sprite(&mut self, actions: &Action, c: &mut Step<'_>) -> Result<(), GenieError> {
        let failed = actions
            .list
            .hunt()
            .is_some_and(|h| h.state == HuntState::Failed);
        if !failed {
            return Ok(());
        }
        let standing = self
            .master(c.w.master_tables(), c.player)?
            .standing_sprite
            .and_then(|s| c.w.model.graphics.get(s))
            .copied();
        if let Some(m) = c.w.parts.mover_mut(self.id) {
            m.shows(standing.as_ref());
        }
        if self.sprite.is_some() {
            let standing = standing.ok_or(GenieError::unsupported(
                "a hunter without its standing sprite",
            ))?;
            new_sprite(
                &mut self.sprite,
                c.w.parts.move_stats_mut(self.id),
                standing,
                c.w.streams.get(Stream::Sprite),
            );
        }
        Ok(())
    }
}
