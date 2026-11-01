//! The life cycle's step in a unit's update: the hit points, the states, and what each new
//! state sets off.

use crate::GenieError;
use crate::math::rng::Stream;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::lifecycle::death_parts::forget_resource;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::unit::{Outcome, Unit};
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
    /// The life cycle's part of the update: the storage that a dying object's player loses
    /// ([`Unit::dying_storage_leaves`]), the life cycle's step and what its new state sets off
    /// (`FUN_14119d710`, [`Unit::state_set_off`]): the death, the ghost, the annexes' states,
    /// the sight, the remains and the death spawn. [`Outcome::Leave`] when the object leaves the
    /// world.
    pub(crate) fn life_step(&mut self, c: &mut Step<'_>) -> Result<Outcome, GenieError> {
        self.dying_storage_leaves(c)?;
        let before = self.life.state;
        if before == ObjectState::DyingSprite && self.sprite.is_none() {
            return Err(GenieError::unsupported(
                "a dying object whose sprite the slice does not hold",
            ));
        }
        // the sprite list's slot 0x20 on the current sprite (FUN_1413b38a0)
        let animating = self
            .sprite
            .is_some_and(|s| s.animate != 0 && s.graphic.flags.contains(SpriteFlags::ANIMATED));
        let leaves =
            if self
                .life
                .update(c.dt.seconds(), c.w.streams.get(Stream::Object), animating)?
            {
                Outcome::Leave
            } else {
                Outcome::Stay
            };
        self.state_set_off(c, before)?;
        Ok(leaves)
    }

    /// `set_state(3)`'s storage (`FUN_1411a56c0` with 0) for a live object below one hit point:
    /// each of kinds 2 and 16 of an attribute leaves the player (its slot 0xf8 with the amount
    /// negated, at once).
    ///
    /// # Errors
    ///
    /// An attribute the player does not have.
    pub(crate) fn dying_storage_leaves(&self, c: &mut Step<'_>) -> Result<(), GenieError> {
        // a NaN hit point is not below one
        if !(self.life.is_alive() && self.life.hp < HitPoints::new(1.0)) {
            return Ok(());
        }
        let m = self.master(&c.w.masters, c.player)?;
        for &(attribute, amount, kind) in &m.storage {
            if let Some(attribute) = attribute
                && matches!(kind, 2 | 16)
            {
                c.w.players
                    .player_mut(c.player)?
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
        c: &mut Step<'_>,
        before: ObjectState,
    ) -> Result<(), GenieError> {
        let after = self.life.state;
        if before <= ObjectState::Alive && ObjectState::Dying <= after {
            self.died(c, before)?;
            if self.master(&c.w.masters, c.player)?.ghost_on_death
                == crate::vision::ghost::LEAVES_GHOSTS
                && c.w.game_state != 2
            {
                // the ghosts of the death (FUN_14118e990), the object lent to the world
                let (p, id) = (c.player, self.id);
                c.lend(self, |w| w.ghosts_at_death(p.index(), id))?;
            }
        }
        self.annexes_follow(c, before, after)?;
        self.sight_follows(c, before, after)?;
        if before == ObjectState::DyingSprite && after == ObjectState::Dead {
            self.dying_sprite_ended(c)?;
        }
        if matches!(before, ObjectState::Dying | ObjectState::Dead)
            && after == ObjectState::Removed
            && self.life.death_spawn
        {
            self.death_spawn_comes(c)?;
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
        c: &mut Step<'_>,
        before: ObjectState,
        after: ObjectState,
    ) -> Result<(), GenieError> {
        if ObjectState::Dying < after
            && after != before
            && !(before == ObjectState::DyingSprite && after == ObjectState::Dead)
            && self.master(&c.w.masters, c.player)?.kind == BUILDING
        {
            for annex in c.w.spatial.annexes_of(self.id) {
                if let Some((q, _)) = c.w.object(annex) {
                    c.lend(self, |w| w.set_state(q, annex, after))?;
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
        c: &mut Step<'_>,
        before: ObjectState,
        after: ObjectState,
    ) -> Result<(), GenieError> {
        if after == before {
            return Ok(());
        }
        if before <= ObjectState::Alive && ObjectState::Dying <= after {
            c.w.spatial.sight_state(self.id, ObjectState::Dying)?;
        }
        if after != ObjectState::Dying {
            c.w.spatial.sight_state(self.id, after)?;
        }
        if before < ObjectState::DyingSprite && ObjectState::Undead < after {
            let master = self.master(&c.w.masters, c.player)?;
            forget_resource(&mut c.w.spatial, Some(master), self.id, self.position())?;
        }
        Ok(())
    }

    /// The dying sprite ended (`RGE_Animated_Object::update`): the animation stops and an object
    /// below type 80 leaves its remains (`FUN_1411a4e90`) with its facet (`+0xe`) as its angle
    /// left it ([`crate::objects::facing::facet_of_angle`]).
    pub(crate) fn dying_sprite_ended(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        self.body.set_anim_speed(0.0);
        let kind = self.master(&c.w.masters, c.player)?.kind;
        if kind < BUILDING
            && let Some(remains) = self.life.remains
        {
            let facet = self
                .body
                .mover()
                .map(|m| crate::objects::facing::facet_of_angle(m.angle, m.stats.facets).0);
            let (p, at) = (c.player, self.position());
            c.lend(self, |w| w.remains(p, remains, at, facet))?;
        }
        Ok(())
    }

    /// A dead object that holds nothing goes to state 7 and its death spawn comes (slot 0x160,
    /// `FUN_1411a5090`: the master's `death_spawn_obj_id` makes an object at its place; a
    /// moving object's override gives it the dead object's facet and that facet's angle). The
    /// spawn's sprite from slot 0x7a8 (the master's undead sprite) is not held.
    pub(crate) fn death_spawn_comes(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let spawn = self.master(&c.w.masters, c.player)?.death_spawns[0];
        let spawn = spawn.ok_or(GenieError::missing_data(
            "a death spawn the dead object's master no longer names",
        ))?;
        let facet = self
            .body
            .mover()
            .map(|m| crate::objects::facing::facet_of_angle(m.angle, m.stats.facets).0);
        let (p, at) = (c.player, self.position());
        c.lend(self, |w| w.remains(p, spawn, at, facet))
    }
}
