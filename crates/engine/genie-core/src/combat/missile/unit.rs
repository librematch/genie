//! A missile's flight: where it ends, what it hits and where it lands.

use crate::combat::damage::HitKind;
use crate::combat::damage::HitLayer;
use crate::combat::missile::MissileState;
use crate::combat::missile::ShooterId;
use crate::combat::missile::StrikeResult;
use crate::combat::missile::VanishMode;
use crate::combat::victims::{readable, victim, victim_of, write_strike_back};
use crate::combat::volley::Fired;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::Rect;
use crate::map::spatial::{RecordId, WalkQuery};
use crate::math::rng::Stream;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::ObjectState;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::MasterKind;
use crate::objects::masters::player_master;
use crate::objects::unit::{Unit, missed_passes};
use crate::players::PlayerId;
use crate::unit_ai::NotifyKind;
use crate::{GenieError, GenieResult};
use genie_model::model::Model;

use crate::combat::attack::{Hit, strike};
use crate::combat::missile::{
    HitMode, Missile, MissileAction, MissileStats, MissileTargetId, grounded, meets_height,
};
use crate::objects::layers::Action;
use crate::objects::masters::MasterTables;
use crate::world::World;
use crate::world::index::ObjectsRef;
use crate::world::layer::Roles;
use crate::world::lets::release;
use crate::world::others::OthersRef;
use crate::world::player::Player;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

impl Unit {
    /// The missile action's update (slot 6, `FUN_140a9e6e0`). First its target and the object it
    /// was fired from: one gone from the world's table is dropped; a dead target (above state 2
    /// but 6, `FUN_1411944f0`) is let go (slot 0x68 with none, `FUN_140a7a230`: slot 0x180).
    /// Then state 4 flies ([`crate::combat::missile::MissileAction::fly`]): the step (`+0x7dc`) moves the
    /// missile (`FUN_14113fb60`: off the map its hit points go, slot 0xd8; the position setter,
    /// slot 0x58, holds it on the map and puts it on the ground by its fly mode once it is no
    /// farther than 0.1 from its point, [`crate::combat::missile::grounded`]); the objects its step
    /// crossed meet it ([`Unit::missile_hits`]); a missile whose height is the ground's lands
    /// ([`Unit::missile_lands`]). A dead shooter's weapons going to the missile
    /// (`FUN_14113e170`), the sprite's pitch facet and the projectile types 1 and 2 are not
    /// modelled.
    pub(crate) fn fly(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let Some(mut missile) = step.world.parts.missile.get(self.id).copied() else {
            return Ok(());
        };
        let mut action = actions.list.take_missile();
        let flown = self.fly_missile(actions, &mut missile, &mut action, step, roles);
        actions.list.set_missile(action);
        if let Some(own) = step.world.parts.missile.get_mut(self.id) {
            *own = missile;
        }
        flown
    }

    /// [`Unit::fly`]'s first step: the missile's target and the object it was fired from, one
    /// gone from the world's table dropped, a dead target (above state 2 but 6) let go.
    ///
    /// # Errors
    ///
    /// A target or a shooter in the world the slice cannot read, and a dead shooter (its
    /// weapons going to the missile, `FUN_14113e170`).
    pub(crate) fn missile_ends(
        &mut self,
        action: &mut MissileAction,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let dead = |state: ObjectState| ObjectState::Alive < state && state != ObjectState::Undead;
        let target_state = readable(step, action.target)?.map(|(_, unit, _)| unit.life.state);
        let shooter_state = OthersRef::of(&step.world.players, &step.world.index, step.player)?
            .own(action.fired_from)
            .map(|unit| unit.life.state);
        if action.target.is_some() && target_state.is_none() {
            if step.world.spatial.get(action.target).is_some() {
                return Err(GenieError::unsupported(
                    "a missile's target the slice cannot read",
                ));
            }
            action.target = None;
        }
        if action.fired_from.is_some() && shooter_state.is_none() {
            if step.world.spatial.get(action.fired_from).is_some() {
                return Err(GenieError::unsupported(
                    "a missile's shooter the slice cannot read",
                ));
            }
            action.fired_from = None;
        }
        if let Some(target) = action.target
            && target_state.is_some_and(dead)
        {
            let by = self.releaser(step)?;
            release(step, self, target, by)?;
            action.target = None;
        }
        if action.fired_from.is_some() && shooter_state.is_some_and(dead) {
            return Err(GenieError::unsupported(
                "a missile whose shooter died (its weapons, FUN_14113e170)",
            ));
        }
        Ok(())
    }

    pub(crate) fn fly_missile(
        &mut self,
        actions: &mut Action,
        missile: &mut Missile,
        slot: &mut Option<MissileAction>,
        context: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let master = self.master(context.world.master_tables(), context.player)?;
        let stats = master.missile_stats().ok_or(GenieError::unsupported(
            "a missile whose master has no flight",
        ))?;
        let Some(action) = slot.as_mut() else {
            return Ok(());
        };
        self.missile_ends(action, context)?;
        match action.state {
            MissileState::Flying => {}
            MissileState::Done => {
                return Err(GenieError::unsupported(
                    "a missile action's update in state 1",
                ));
            }
            _ => {
                return Err(GenieError::unsupported(
                    "a missile action in a state but 1 and 4",
                ));
            }
        }
        let (step, flying) = action.fly(
            context.dt.seconds(),
            missile.max_range,
            &stats,
            &context.world.model.tuning.missile,
        )?;
        missile.step = step;
        missile.flying = flying;
        let old = self.position();
        let to = old + step;
        let (width, height) = (
            context.world.spatial.map.width(),
            context.world.spatial.map.height(),
        );
        if to.x() < 0.0 || width as f32 <= to.x() || to.y() < 0.0 || height as f32 <= to.y() {
            self.life.hp = HitPoints::ZERO;
        }
        let mover = context
            .world
            .parts
            .mover_mut(self.id)
            .ok_or(GenieError::unsupported("a missile without its moving part"))?;
        let mut at = mover.set_position(
            to,
            context.world.spatial.map,
            &context.world.model.tuning.movement,
        );
        let ground = context
            .world
            .spatial
            .terrain
            .as_ref()
            .ok_or(GenieError::unsupported(
                "a missile without the map's terrain",
            ))?
            .ground(at.ground())?;
        // a missile still flying (`+0x994`) keeps its height
        if !flying {
            at = at.with_z(grounded(at.z(), ground, stats.fly_mode));
        }
        mover.mark_move(at, context.world.world_time);
        mover.new_position(
            &mut self.place,
            self.id,
            at,
            (&mut context.world.spatial, Some(&mut context.world.pathing)),
            roles,
            context.world.model.tuning.visibility,
        )?;
        if stats.projectile_kind.meets_on_the_way() {
            let fired = Fired {
                charge: missile.charge,
                by: missile.fired_from,
            };
            self.missile_hits(actions, action, &stats, fired, old, context)?;
        }
        if ground == at.z() {
            self.missile_lands(actions, missile, slot, &stats, context, roles)?;
        }
        Ok(())
    }

    /// `FUN_140a9e6e0`'s hits (projectile types 0 to 2): the objects whose obstruction boxes, grown
    /// by the missile's radius on x, its step from `old` crossed (`FUN_14113f740` ->
    /// `FUN_1411570c0`: the query `FUN_141051900` with flags 0x10, the missile itself ignored, the
    /// ends held inside the map) and that lie at its height
    /// ([`crate::combat::missile::meets_height`]). Each one the last step did not meet, while the
    /// missile still flies: hit mode 0 hits its target alone; hit mode 1 (a miss) an object of
    /// another player that is no ally's and no missile, but not one of Gaia's above type 15 or one
    /// below the missile's blast level unless it is the target. A hit is the shooter's
    /// ([`Unit::missile_strike`]); unless the master's vanish mode has
    /// [`crate::combat::missile::VanishMode::PASSES_THROUGH`] or a shield blocked it, the missile
    /// ends (`set_state` 1: slot 0xd8, its hit points 0). The step's objects become the met ones
    /// (`+0x88`) when there are any. A missed missile's hit on another object than its target, hit
    /// mode 2 and the Hussite wagon's case (`+0x90`) are not modelled.
    pub(crate) fn missile_hits(
        &mut self,
        actions: &mut Action,
        action: &mut MissileAction,
        stats: &MissileStats,
        fired: Fired,
        old: Position,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let roles = &step.world.model.roles.clone();
        if !(0.0 < stats.radius.x() && 0.0 < stats.radius.y()) {
            return Ok(());
        }
        let blast_level = self
            .master(step.world.master_tables(), step.player)?
            .combat_part()
            .blast_level;
        let met = self.missile_meets(stats, old, step)?;
        for &(id, player, kind, level) in &met {
            if action.state == MissileState::Done {
                break;
            }
            if action.met.contains(&id) {
                continue;
            }
            match action.hit_mode {
                HitMode::TARGET_ALONE => {
                    if Some(id) != action.target.map(MissileTargetId::id) {
                        continue;
                    }
                    let result = self.missile_strike(actions, action, fired, step, roles)?;
                    action.result = result;
                    if !stats.vanish_mode.contains(VanishMode::PASSES_THROUGH)
                        && result != StrikeResult::Shielded
                    {
                        self.end_missile(action);
                    }
                }
                HitMode::MISSED => {
                    let target = action.target.map(MissileTargetId::id);
                    if missed_passes(step, (id, player, kind, level), target, blast_level) {
                        continue;
                    }
                    if Some(id) != action.target.map(MissileTargetId::id) {
                        return Err(GenieError::unsupported(
                            "a missed missile meeting another object than its target",
                        ));
                    }
                    let result = self.missile_strike(actions, action, fired, step, roles)?;
                    action.result = result;
                    // the master's dying sprite plays there (a sprite of its own, not held)
                    if (stats.vanish_mode.contains(VanishMode::PASSES_THROUGH)
                        || result == StrikeResult::Shielded)
                        && blast_level.reaches(level)
                    {
                        continue;
                    }
                    self.end_missile(action);
                }
                _ => return Err(GenieError::unsupported("a missile of hit mode 2")),
            }
        }
        if !met.is_empty() {
            action.met = met.iter().map(|struck| struck.0).collect();
        }
        Ok(())
    }

    /// [`Unit::missile_hits`]'s objects the missile meets on its step from `old`: the step held on
    /// the map
    /// ([`MissileTuning::query_edge`](crate::world::layer::generated::MissileTuning::query_edge)
    /// short of its far edges), the records its sweep grown by the missile's radius crosses (the
    /// manager's walk query with the missile's player, the missile itself ignored), of those each
    /// whose height the missile's meets ([`crate::combat::missile::meets_height`]): its id, player,
    /// master type and blast defence.
    ///
    /// # Errors
    ///
    /// A frozen object the sweep may cross, and an object of a master the slice does not have.
    pub(crate) fn missile_meets(
        &self,
        stats: &MissileStats,
        old: Position,
        step: &mut Step<'_>,
    ) -> GenieResult<Vec<(ObjectId, PlayerId, MasterKind, BlastDefense)>> {
        let (width, height) = (
            step.world.spatial.map.width(),
            step.world.spatial.map.height(),
        );
        let hold = |coordinate: f32, size: i32| {
            if coordinate < 0.0 {
                0.0
            } else if size as f32 <= coordinate {
                size as f32 - step.world.model.tuning.missile.query_edge
            } else {
                coordinate
            }
        };
        let at = self.position();
        let from = Point::new(hold(old.x(), width), hold(old.y(), height));
        let to = Point::new(hold(at.x(), width), hold(at.y(), height));
        let rect = Rect::spanning(from, to);
        let grow = stats.radius.x();
        step.world
            .spatial
            .check_known_records(rect.grown(Distance::new(grow)))?;
        let crossed = step.world.spatial.walk_hits(WalkQuery {
            start: from,
            end: to,
            rect,
            grow: Distance::new(grow),
            ally_factor: 1.0,
            player: Some(step.player),
            ignore: &[RecordId::from(self.id)],
            objects: ObjectsRef::new(
                &step.world.players,
                &step.world.index,
                &step.world.model.masters,
                &step.world.parts,
                &step.world.model.roles,
                &step.world.model.tuning,
            ),
        })?;
        let mut met = Vec::new();
        for record in crossed {
            // the game's table reads the hit's integer (an edge's may name an object)
            let Some(id) = record.key() else {
                continue;
            };
            let Some(placed) = step.world.spatial.get(id) else {
                continue;
            };
            let master = player_master(&step.world.model.masters, placed.player, placed.master)
                .ok_or(GenieError::unsupported(
                    "an object a missile meets of a master the slice does not have",
                ))?;
            if meets_height(
                placed.position.z(),
                master.height.get(),
                at.z(),
                stats.radius.z(),
            ) {
                met.push((id, placed.player, master.kind, master.blast_defense));
            }
        }
        Ok(met)
    }

    /// The missile action's `set_state(1)` (`FUN_140a9de70`): the missile's slot 0xd8
    /// (`FUN_14118f580`), its hit points to 0.
    pub(crate) const fn end_missile(&mut self, action: &mut MissileAction) {
        action.state = MissileState::Done;
        self.life.hp = HitPoints::ZERO;
    }

    /// The shooter's hit for its missile (`FUN_140a9e6e0`: slot 0x860 of the object it was fired
    /// from, `FUN_1412115e0`, at the missile's place with 1 and the missile flag, the action's
    /// origin as the hit's, whose height the elevation factor reads): [`crate::combat::attack::strike`] on
    /// the target, the executed action posted to the shooter's unit AI, the shooter's own part
    /// written back; its result. A shooterless missile hits on its own ([`Unit::own_strike`]). A
    /// charge projectile's own hit, a shooter with an area effect (its trample or blast) and a
    /// retaliation killing the shooter are not modelled; a building shooter's elevation factor
    /// is its own (`FUN_1411cc890`, its relics: [`crate::combat::attack::strike`]).
    pub(crate) fn missile_strike(
        &mut self,
        actions: &mut Action,
        action: &MissileAction,
        fired: Fired,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<StrikeResult> {
        let damage_tuning = step.world.model.tuning.damage;
        if fired.charge {
            return Err(GenieError::unsupported("a charge projectile's own hit"));
        }
        if action.fired_from.is_none() {
            return self.own_strike(actions, action, fired.by, step, roles);
        }
        let mut victim = victim(step, action.target, roles)?;
        let (player_id, mode_680) = (step.player, step.world.mode_680);
        let shooter_slot = step.world.live_index(player_id, action.fired_from);
        let World {
            players,
            spatial,
            streams,
            parts,
            model: Model { masters, slots, .. },
            ..
        } = &mut *step.world;
        let Player {
            objects,
            attributes,
            ..
        } = players.player_mut(player_id)?;
        let shooter = shooter_slot
            .and_then(|list_index| objects.get_mut(list_index))
            .ok_or(GenieError::unsupported(
                "a missile's shooter outside its list",
            ))?;
        let master = shooter.master(MasterTables::new(masters, &parts.own_master), player_id)?;
        if Distance::ZERO < master.combat_part().area_effect {
            return Err(GenieError::unsupported(
                "a missile of a shooter with an area effect",
            ));
        }
        let terrain = spatial.terrain.as_ref();
        let mut own = victim_of(
            (
                shooter,
                parts.unit_ai.contains(shooter.id),
                parts.building_of(shooter.id),
                parts.combat_of(shooter.id),
                parts.actions.of(shooter.id),
                parts.unread.get(shooter.id),
            ),
            master,
            attributes,
            player_id,
            terrain,
            mode_680,
            roles,
        );
        let victim = victim.as_mut().ok_or(GenieError::unsupported(
            "a missile's target the slice does not hold live",
        ))?;
        let hit = Hit {
            at: self.position(),
            from_z: Some(action.origin.position.z()),
            kind: HitKind::Missile,
            chance: master.combat_part().hit_chance,
        };
        let object = streams.get_mut(Stream::Object);
        let struck = strike(
            &mut own,
            victim,
            &mut step.blasted,
            hit,
            object,
            HitLayer {
                roles,
                slots: *slots,
                damage_tuning: &damage_tuning,
            },
        )?;
        if let Some(target) = struck.executed {
            shooter.post_own(
                &mut parts.unit_ai,
                NotifyKind::ACTION_EXECUTED,
                [600, target.get(), 0],
            )?;
        }
        if shooter
            .hit_back(own, parts.ai_and_combat_mut(shooter.id)?)?
            .is_some()
        {
            return Err(GenieError::unsupported(
                "a retaliation killing a missile's shooter",
            ));
        }
        write_strike_back(step, self, actions, victim, roles)?;
        Ok(struck.result)
    }

    /// A shooterless missile's own hit (`FUN_140a9e6e0`: slot 0x860 of the missile,
    /// `FUN_141084230`, at its place): [`crate::combat::attack::strike`] with the missile as the attacker,
    /// its own master's weapons and hit chance (its origin names no master), and no elevation
    /// factor (the missile's slot 0x1f0, `FUN_14113e550`: 1 for an origin without an owner); the
    /// damage's notify comes from what it was fired from (`+0x998`, [`crate::combat::damage`]). The
    /// executed action goes to the missile's unit AI, which it has none of; what a retaliation
    /// does to it is written back. A missile with an area effect (its blast) or whose origin
    /// names an owner is not modelled.
    pub(crate) fn own_strike(
        &mut self,
        actions: &mut Action,
        action: &MissileAction,
        fired_from: Option<ShooterId>,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<StrikeResult> {
        if action.origin.owner.is_some() {
            return Err(GenieError::unsupported(
                "a shooterless missile with an origin (its elevation, FUN_14113e550)",
            ));
        }
        let master = self.master(step.world.master_tables(), step.player)?;
        if Distance::ZERO < master.combat_part().area_effect {
            return Err(GenieError::unsupported(
                "a shooterless missile with an area effect",
            ));
        }
        let terrain = step.world.spatial.terrain.as_ref();
        let mut own = victim_of(
            (
                self,
                step.world.parts.unit_ai.contains(self.id),
                step.world.parts.building_of(self.id),
                step.world.parts.combat_of(self.id),
                actions,
                step.world.parts.unread.get(self.id),
            ),
            master,
            &step.world.players.player(step.player)?.attributes,
            step.player,
            terrain,
            step.world.mode_680,
            roles,
        );
        // the missile's own part is out of the unit while it flies
        own.source.fired_from = fired_from;
        let mut victim = victim(step, action.target, roles)?.ok_or(GenieError::unsupported(
            "a missile's target the slice does not hold live",
        ))?;
        let victim = &mut victim;
        let hit = Hit {
            at: self.position(),
            from_z: None,
            kind: HitKind::Missile,
            chance: master.combat_part().hit_chance,
        };
        let object = step.world.streams.get_mut(Stream::Object);
        let struck = strike(
            &mut own,
            victim,
            &mut step.blasted,
            hit,
            object,
            HitLayer {
                roles,
                slots: step.world.model.slots,
                damage_tuning: &step.world.model.tuning.damage,
            },
        )?;
        if self
            .hit_back(own, step.world.parts.ai_and_combat_mut(self.id)?)?
            .is_some()
        {
            return Err(GenieError::unsupported("a retaliation killing a missile"));
        }
        write_strike_back(step, self, actions, victim, roles)?;
        Ok(struck.result)
    }
}
