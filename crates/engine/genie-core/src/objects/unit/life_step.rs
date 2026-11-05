//! The life cycle's step in a unit's update: the hit points, the states, and what each new
//! state sets off.

use crate::map::Snow;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::AnimateState;
use crate::objects::HitPoints;
use crate::objects::ListFate;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::facing::facet_of_angle;
use crate::objects::layers::Action;
use crate::objects::lifecycle::MasterLife;
use crate::objects::lifecycle::SpritePlay;
use crate::objects::lifecycle::death_parts::forget_resource;
use crate::objects::masters::GhostMode;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::sprite::dying;
use crate::objects::unit::{Outcome, Unit};
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Unit {
    /// The life cycle's part of the update: the storage that a dying object's player loses
    /// ([`Unit::dying_storage_leaves`]), the life cycle's step and what its new state sets off
    /// (`FUN_14119d710`, [`Unit::state_set_off`]): the death, the ghost, the annexes' states,
    /// the sight, the remains and the death spawn. [`Outcome::Leave`] when the object leaves the
    /// world.
    pub(crate) fn life_step(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<Outcome> {
        self.dying_storage_leaves(step)?;
        let before = self.life.state;
        if before == ObjectState::DyingSprite && self.sprite.is_none() {
            return Err(GenieError::unsupported(
                "a dying object whose sprite the slice does not hold",
            ));
        }
        // the sprite list's slot 0x20 on the current sprite (FUN_1413b38a0)
        let sprite = if self.sprite.is_some_and(|active_sprite| {
            active_sprite.animate != AnimateState::Stopped
                && active_sprite.graphic.flags.contains(SpriteFlags::ANIMATED)
        }) {
            SpritePlay::Animating
        } else {
            SpritePlay::Still
        };
        // what the steps read of its master now: slot 0x7a0's dying sprite (a combat object's
        // terrain look's first) and its decay
        let master = self.master(
            MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
            step.player,
        )?;
        let master_life = MasterLife {
            dying_sprite: dying(
                master,
                step.world.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
            data: master.life,
        };
        let leaves = match self.life.update(
            step.dt.seconds(),
            step.world.streams.get_mut(Stream::Object),
            sprite,
            master_life,
        )? {
            ListFate::Leaves => Outcome::Leave,
            ListFate::Stays => Outcome::Stay,
        };
        self.state_set_off(actions, step, before, roles)?;
        Ok(leaves)
    }

    /// `set_state(3)`'s storage (`FUN_1411a56c0` with 0) for a live object below one hit point:
    /// each of kinds 2 and 16 of an attribute leaves the player (its slot 0xf8 with the amount
    /// negated, at once).
    ///
    /// # Errors
    ///
    /// An attribute the player does not have.
    pub(crate) fn dying_storage_leaves(&self, step: &mut Step<'_>) -> GenieResult<()> {
        // a NaN hit point is not below one
        if !(self.life.is_alive() && self.life.hp < HitPoints::new(1.0)) {
            return Ok(());
        }
        let master = self.master(
            MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
            step.player,
        )?;
        for &(attribute, amount, kind) in &master.storage {
            if let Some(attribute) = attribute
                && matches!(kind, 2 | 16)
            {
                step.world
                    .players
                    .player_mut(step.player)?
                    .add_attribute(attribute, -amount)?;
            }
        }
        Ok(())
    }

    /// The `set_state`s of the life cycle's step from `before` to the state it left
    /// (`FUN_14119d710`): a death goes through state 3 ([`Unit::died`], then the static
    /// `set_state`'s ghosts, `FUN_14118e990`, the caller's), the annexes follow
    /// ([`Unit::annexes_follow`]), the sight takes the states ([`Unit::sight_follows`]), the
    /// dying sprite's end leaves the remains ([`Unit::dying_sprite_ended`]) and a dead object
    /// that holds nothing makes its death spawn ([`Unit::death_spawn_comes`]).
    pub(crate) fn state_set_off(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        before: ObjectState,
        roles: &Roles,
    ) -> GenieResult<()> {
        let after = self.life.state;
        if before <= ObjectState::Alive && ObjectState::Dying <= after {
            self.died(actions, step, before, roles)?;
            if self
                .master(step.world.master_tables(), step.player)?
                .ghost_on_death
                == GhostMode::LEAVES
                && step.world.game_state.leaves_ghosts()
            {
                // the ghosts of the death (FUN_14118e990), the object lent to the world
                let (player_id, id) = (step.player, self.id);
                step.lend((&mut *self, &mut *actions), |world| {
                    world.ghosts_at_death(player_id.index(), id)
                })?;
            }
        }
        self.annexes_follow(actions, step, before, after)?;
        self.sight_follows(step, before, after)?;
        if before == ObjectState::DyingSprite && after == ObjectState::Dead {
            self.dying_sprite_ended(actions, step)?;
        }
        if matches!(before, ObjectState::Dying | ObjectState::Dead)
            && after == ObjectState::Removed
            && self
                .master(step.world.master_tables(), step.player)?
                .death_spawns[0]
                .is_some()
        {
            self.death_spawn_comes(actions, step)?;
        }
        Ok(())
    }

    /// A building's `set_state`s after its death's 3 take its annexes along too, each annex's
    /// slot 0xa0 where it runs with the building lent to the world (`FUN_1411dbb70`,
    /// [`crate::world::World::set_state`], [`Step::lend`]): the animated update's 4
    /// or 5 from 3, the static update's 7 and 8; the dying sprite's end (4 to 5) writes the state
    /// without a `set_state`.
    ///
    /// # Errors
    ///
    /// What an annex's `set_state` does not model.
    pub(crate) fn annexes_follow(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        before: ObjectState,
        after: ObjectState,
    ) -> GenieResult<()> {
        if ObjectState::Dying < after
            && after != before
            && !(before == ObjectState::DyingSprite && after == ObjectState::Dead)
            && self.master(step.world.master_tables(), step.player)?.kind == MasterKind::BUILDING
        {
            let annexes = step.world.parts.building_of(self.id).annexes;
            for annex in annexes.ids() {
                if let Some((player_id, _)) = step.world.object(annex) {
                    step.lend((&mut *self, &mut *actions), |world| {
                        world.set_state(player_id, annex, after)
                    })?;
                }
            }
        }
        Ok(())
    }

    /// The sight follows a changed state: a death through state 3 first, then the new state but
    /// 3; an object that went from below 4 to above 6 leaves the resource records
    /// ([`forget_resource`]).
    pub(crate) fn sight_follows(
        &self,
        step: &mut Step<'_>,
        before: ObjectState,
        after: ObjectState,
    ) -> GenieResult<()> {
        if after == before {
            return Ok(());
        }
        if before <= ObjectState::Alive && ObjectState::Dying <= after {
            step.world.spatial.sight_state(
                self.id,
                ObjectState::Dying,
                &step.world.model.roles,
                step.world.model.tuning.visibility,
            )?;
        }
        if after != ObjectState::Dying {
            step.world.spatial.sight_state(
                self.id,
                after,
                &step.world.model.roles,
                step.world.model.tuning.visibility,
            )?;
        }
        if before < ObjectState::DyingSprite && ObjectState::Undead < after {
            let master = self.master(
                MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
                step.player,
            )?;
            forget_resource(
                &mut step.world.spatial,
                Some(master),
                self.id,
                self.position(),
            )?;
        }
        Ok(())
    }

    /// The dying sprite ended (`RGE_Animated_Object::update`): the animation stops and an object
    /// below type 80 leaves its remains (`FUN_1411a4e90`: its master's `death_spawn_obj_id2`,
    /// `+0x5e6`, the `.dat`'s `blood_unit_id`, −1 for none) with its facet (`+0xe`) as its angle
    /// left it ([`crate::objects::facing::facet_of_angle`]).
    pub(crate) fn dying_sprite_ended(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        step.world.parts.set_anim_speed(self.id, Speed::ZERO);
        let master = self.master(step.world.master_tables(), step.player)?;
        if master.kind < MasterKind::BUILDING
            && let Some(remains) = master.death_spawns[1]
        {
            let facet = step
                .world
                .parts
                .mover_of(self.id)
                .map(|mover| facet_of_angle(mover.angle, mover.stats.facets).0);
            let (player_id, at) = (step.player, self.position());
            let _ = step.lend((&mut *self, &mut *actions), |world| {
                world.remains(player_id, remains, at, facet)
            })?;
        }
        Ok(())
    }

    /// A dead object that holds nothing goes to state 7 and its death spawn comes (slot 0x160,
    /// `FUN_1411a5090`: the master's `death_spawn_obj_id` makes an object at its place; a
    /// moving object's override gives it the dead object's facet and that facet's angle). A
    /// building's override (`FUN_1411e5cd0`) then gives a spawn that is a building its snow
    /// flag of 2 ([`Snow::Kept`], a farm's). The spawn's sprite from slot 0x7a8 (the master's
    /// undead sprite), a building's rubble sprite of an unfinished one (`+0x968`) and its facet
    /// are not held, as no remains hold them; its salvage pile (`+0x9c8`) is not made.
    pub(crate) fn death_spawn_comes(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let spawn = self
            .master(step.world.master_tables(), step.player)?
            .death_spawns[0];
        let spawn = spawn.ok_or(GenieError::missing_data(
            "a death spawn the dead object's master no longer names",
        ))?;
        let facet = step
            .world
            .parts
            .mover_of(self.id)
            .map(|mover| facet_of_angle(mover.angle, mover.stats.facets).0);
        let kept = step
            .world
            .parts
            .building_of(self.id)
            .construction
            .as_ref()
            .is_some_and(|construction| construction.snow == Snow::Kept);
        let (player_id, at) = (step.player, self.position());
        let made = step.lend((&mut *self, &mut *actions), |world| {
            world.remains(player_id, spawn, at, facet)
        })?;
        // the override writes the byte whatever the spawn is; only a building holds it
        let made_construction = step
            .world
            .parts
            .building
            .get_mut(made)
            .and_then(|building| building.construction.as_mut());
        if kept && let Some(construction) = made_construction {
            construction.snow = Snow::Kept;
        }
        Ok(())
    }
}
