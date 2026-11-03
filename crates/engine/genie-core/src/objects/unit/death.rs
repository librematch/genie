//! An object's death (`set_state(3)` and the static part of it) and a foundation's death.

use crate::GenieError;
use crate::construction::build::NO_CONSTRUCTION;
use crate::construction::build::{Completion, Construction};
use crate::gathering::PastureId;
use crate::math::rng::Stream;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::layers::Action;
use crate::objects::lifecycle::Class;
use crate::objects::lifecycle::death_parts::{built_death, combat_dies, farm_bonus, refund_of};
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::sprite::{building_death, dying, new_sprite};
use crate::objects::unit::{NO_DYING_SPRITE, Unit};
use crate::players::Amount;
use crate::players::tracker::key;
use crate::technology::TechId;
use crate::vision::SightLayer;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
    /// The death of a built building from state 2 ([`Unit::died`]): the storage leaves
    /// (`FUN_1411e63f0`), the owner's watched list lets it go, the tiles unlink, and a
    /// monastery's, wonder's or castle's count leaves. The technology its last one withdraws.
    ///
    /// # Errors
    ///
    /// A player the world does not hold, or what the death does not model.
    fn built_building_died(
        &mut self,
        c: &mut Step<'_>,
        master: &Master,
    ) -> Result<Option<(MasterId, TechId)>, GenieError> {
        // FUN_1411dbb70 to state 3 from 2: the storage leaves (FUN_1411e63f0), the owner's
        // watched list lets it go (FUN_141091310), the tiles unlink (FUN_140f65580), and
        // a monastery's, wonder's or castle's count leaves, at least 0 left
        let player_kind = c.w.players.player(c.player)?.kind;
        let (id, annex_of) = (self.id, c.w.parts.building_of(self.id).annex_of);
        let death = built_death(
            (self, c.w.parts.combat_mut(id)?),
            annex_of,
            master,
            player_kind,
            &c.w.model.roles,
            c.w.model.slots,
        )?;
        c.w.players
            .player_mut(c.player)?
            .add_attributes(death.adds)?;
        c.w.unwatch(c.player, self.id)?;
        if let (Some(v), Some(at)) = (c.w.spatial.sight.as_mut(), death.unlink) {
            v.unlink_building(at, master.radius, c.w.model.tuning.visibility)?;
        }
        if let Some(kind) = death.building.count {
            c.w.players
                .player_mut(c.player)?
                .take_at_least_zero(kind, Amount::new(1.0))?;
        }
        Ok(death.building.tech)
    }

    /// The death's `set_state(3)` (from `before`, the state the object had) and the static
    /// update's dying sprite after it (slot 0x7a0, then slot 0x68, `FUN_141217000`). A building's
    /// (`FUN_1411dbb70`) first lets its annexes go (each takes its hit points at once, in the
    /// world) and sets their states (the caller's after the update), then a
    /// built one dies ([`built_death`]) and a foundation runs the completion's first part and its
    /// construction's start ([`Unit::foundation_died`]); then the combat object's
    /// (`FUN_14121cc40`, [`combat_dies`]), whose refund from state 2 ([`refund_of`]) the caller
    /// takes from its owner's attributes; then the base one's (`FUN_14119d710`, whose storage and
    /// sight are the caller's). An animated or moving object (types 20 to 69) shows its
    /// master's dying sprite. Not held: the particles and the sounds.
    pub(crate) fn died(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        before: ObjectState,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        // its master as the death starts: the world's calls below change other objects
        let master = &self.master(c.w.master_tables(), c.player)?.clone();
        // the last building of a master with a technology withdraws it, after the combat part
        let mut tech = None;
        if master.kind == MasterKind::BUILDING {
            // the static update's death (0x14118ad50) lets the annexes go before its set_state(3):
            // each slot (`+0xbc0`..) clears and its annex's `+0xbb8` with it, and each annex takes
            // the building's hit points (`+0x278`), so it dies in its own update; the set_state
            // (FUN_1411dbb70) then finds no annex to take along
            let annexes = c.w.parts.building_of(self.id).annexes;
            c.w.parts.building_mut(self.id)?.annexes.clear();
            for annex in annexes.ids() {
                if let Some((a, layer)) = c.w.object_and_layer_mut(annex)? {
                    layer.annex_of = None;
                    a.life.hp = self.life.hp;
                }
            }
            if before == ObjectState::Alive {
                tech = self.built_building_died(c, master)?;
            } else if before == ObjectState::Foundation {
                self.foundation_died(actions, c, master, roles)?;
            }
        } else if master.kind != MasterKind::COMBAT {
            // a static object's dying sprite is the lifecycle's ([`Lifecycle::die`]); an
            // animated or moving one shows its master's (slot 0x7a0, `FUN_1411887e0`: no
            // terrain look) through slot 0x68 (a moving one's `FUN_141158620` derives its facet
            // from its angle again)
            if self.life.class == Class::Animated
                && self.life.dying_sprite.is_some()
                && let Some(id) = dying(master, None)
            {
                self.show_dying(c, id)?;
            }
            return Ok(());
        }
        combat_dies(
            self,
            c.w.parts.building_of(self.id),
            master,
            &mut c.w.spatial,
            &mut c.w.pathing,
            roles,
            c.w.model.tuning.visibility,
        )?;
        if before == ObjectState::Alive {
            // FUN_14121cc40: the paid cost leaves attributes 98 and 164 (a building's 98 and
            // 165), each at least 0 (slots 0xf8, then 0xf0 with 0)
            let owner = c.w.players.player_mut(c.player)?;
            for (kind, cost) in refund_of(
                master,
                c.w.parts.building_of(self.id).annex_of,
                roles,
                c.w.model.slots,
            ) {
                owner.take_at_least_zero(kind, cost)?;
            }
        }
        if let Some((m, t)) = tech {
            // FUN_141166ab0 finds no other of the master standing (the building lent to the
            // world for the scan), then FUN_1413d4ec0
            let p = c.player;
            c.lend((&mut *self, &mut *actions), |w| w.withdraw_tech(p, m, t))?;
        }
        // the static update's sprite after the death (0x14118ad50): a building's by its build
        // points (its destruction sprite unbuilt), any other combat object's slot 0x7a0's
        let look = c.w.parts.combat_of(self.id).abilities.look.as_ref();
        let shown = if master.kind == MasterKind::BUILDING {
            let built =
                c.w.parts
                    .building_of(self.id)
                    .construction
                    .ok_or(NO_CONSTRUCTION)?;
            building_death(master, look, built.progress)
        } else {
            dying(master, look)
        };
        if let Some(id) = shown {
            self.show_dying(c, id)?;
        }
        Ok(())
    }

    /// The death's slot 0x68 with the sprite `id`; a moving object's top speed follows it
    /// ([`crate::objects::sprite::top_speed`]: the deer 9655 of MarvinBlue/426048731, running at
    /// 1.4003 when the spears kill it in update 3147, moves at its master's 0.737 with its
    /// dying sprite).
    fn show_dying(&mut self, c: &mut Step<'_>, id: SpriteId) -> Result<(), GenieError> {
        let g = c.w.model.graphics.get(id).copied().ok_or(NO_DYING_SPRITE)?;
        new_sprite(
            &mut self.sprite,
            c.w.parts.move_stats_mut(self.id),
            g,
            c.w.streams.get(Stream::Sprite),
            c.w.model.tuning.sprite,
        );
        Ok(())
    }

    /// A foundation's death (`FUN_1411dbb70` to state 3 from state 0), before the combat object's:
    /// the completion's first part (`FUN_1411c6bd0`,
    /// [`World::completion_part`](crate::world::World::completion_part): a farm's bonus once its
    /// build points reached its build time, a pasture's livestock and a mule cart's search for
    /// resources at once, the foundation lent to the world), then, as the state is still 0 and 3
    /// lies within 1 to 6, its construction's start (`FUN_1411e5670`, [`Construction::begin`]: a
    /// desolid one gets its record and sight, its missiles per volley its master's; its owner
    /// watches it at once, the foundation lent to the world).
    pub(crate) fn foundation_died(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        master: &Master,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let ai = c.w.parts.unit_ai.contains(self.id);
        let construction =
            c.w.parts
                .building_mut(self.id)?
                .construction
                .as_mut()
                .ok_or(NO_CONSTRUCTION)?;
        let completion = Completion {
            master: Some(master),
            farm_bonus: farm_bonus(&c.w.players.player(c.player)?.attributes),
            masters: c.w.model.masters.of(c.player),
            player_kind: c.w.players.player(c.player)?.kind,
            ai,
        };
        let part = completion.first_part(construction, roles)?;
        // its tracker part (FUN_14104eac0): the foundation leaves the pending lists
        let k = key(
            c.w.model.masters.of(c.player),
            &c.w.model.lines,
            self.life.master_id,
            roles,
        );
        c.w.players
            .player_mut(c.player)?
            .tracker
            .complete(self.id, k);
        c.w.players
            .player_mut(c.player)?
            .add_attributes(part.attributes)?;
        // a pasture's livestock and a mule cart's search (from state 0), the foundation lent to
        // the world for what they place and search
        if part.pasture || part.cart {
            let (p, id) = (c.player, self.id);
            c.lend((&mut *self, &mut *actions), |w| {
                if part.pasture {
                    w.setup_pasture(p, PastureId::new(id))?;
                }
                if part.cart {
                    w.cart_search(p, id, ObjectState::Foundation)?;
                }
                Ok(())
            })?;
        }
        let (layer, mover) = c.w.parts.building_and_mover_mut(self.id)?;
        let construction = layer.construction.as_mut().ok_or(NO_CONSTRUCTION)?;
        let started = Construction::begin(
            self.id,
            construction,
            mover,
            &mut c.w.spatial,
            &mut c.w.pathing,
            c.w.world_time,
            SightLayer {
                roles,
                visibility_tuning: c.w.model.tuning.visibility,
            },
        )?;
        if started {
            c.w.parts.combat_mut(self.id)?.firepower = master.creatable_part().volley_fire;
            let (p, id) = (c.player, self.id);
            c.lend((&mut *self, &mut *actions), |w| w.watch_started(p, id))?;
        }
        Ok(())
    }
}
