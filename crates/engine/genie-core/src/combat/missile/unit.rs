//! A missile's flight: where it ends, what it hits and where it lands.

use crate::GenieError;
use crate::combat::missile::MissileState;
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
use crate::objects::lifecycle::death_parts::forget_resource;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::MasterKind;
use crate::objects::masters::player_master;
use crate::objects::unit::{Unit, missed_passes};
use crate::players::PlayerId;
use crate::unit_ai::NotifyKind;

use crate::world::World;
use crate::world::others::OthersRef;
use crate::world::player::Player;
use crate::world::slots::PlayerSlots;
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
    pub(crate) fn fly(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let Some(mut missile) = self.missile.take() else {
            return Ok(());
        };
        let mut action = self.actions.take_missile();
        let flown = self.fly_missile(&mut missile, &mut action, c);
        self.actions.set_missile(action);
        self.missile = Some(missile);
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
        action: &mut crate::combat::missile::MissileAction,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let dead = |state: ObjectState| ObjectState::Alive < state && state != ObjectState::Undead;
        let target_state = readable(c, action.target)?.map(|(_, u, _)| u.life.state);
        let shooter_state = OthersRef::of(&c.w.players, c.player)?
            .own(action.fired_from)
            .map(|u| u.life.state);
        if action.target.is_some() && target_state.is_none() {
            if c.w.spatial.get(action.target).is_some() {
                return Err(GenieError::unsupported(
                    "a missile's target the slice cannot read",
                ));
            }
            action.target = None;
        }
        if action.fired_from.is_some() && shooter_state.is_none() {
            if c.w.spatial.get(action.fired_from).is_some() {
                return Err(GenieError::unsupported(
                    "a missile's shooter the slice cannot read",
                ));
            }
            action.fired_from = None;
        }
        if let Some(target) = action.target
            && target_state.is_some_and(dead)
        {
            let by = self.releaser(c)?;
            crate::world::lets::release(c, self, target, by)?;
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
        missile: &mut crate::combat::missile::Missile,
        slot: &mut Option<crate::combat::missile::MissileAction>,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let master = self.master(&c.w.masters, c.player)?;
        let stats = master.missile_stats.ok_or(GenieError::unsupported(
            "a missile whose master has no flight",
        ))?;
        let Some(action) = slot.as_mut() else {
            return Ok(());
        };
        self.missile_ends(action, c)?;
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
        let (step, flying) = action.fly(c.dt.seconds(), missile.max_range, &stats)?;
        missile.step = step;
        missile.flying = flying;
        let old = self.position();
        let to = old.moved(step);
        let (w, h) = (c.w.spatial.map.width(), c.w.spatial.map.height());
        if to.x() < 0.0 || w as f32 <= to.x() || to.y() < 0.0 || h as f32 <= to.y() {
            self.life.hp = HitPoints::ZERO;
        }
        let mover = self
            .body
            .mover_mut()
            .ok_or(GenieError::unsupported("a missile without its moving part"))?;
        let mut at = mover.set_position(to, c.w.spatial.map);
        let ground =
            c.w.spatial
                .terrain
                .as_ref()
                .ok_or(GenieError::unsupported(
                    "a missile without the map's terrain",
                ))?
                .ground(at.ground())?;
        at = at.with_z(crate::combat::missile::grounded(
            at.z(),
            ground,
            flying,
            stats.fly_mode,
        ));
        mover.mark_move(at, c.w.world_time);
        mover.new_position(self.id, at, (&mut c.w.spatial, Some(&mut c.w.pathing)))?;
        if stats.projectile_type.meets_on_the_way() {
            let fired = Fired {
                charge: missile.charge,
                by: missile.fired_from,
            };
            self.missile_hits(action, &stats, fired, old, c)?;
        }
        if ground == at.z() {
            self.missile_lands(missile, slot, &stats, c)?;
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
        action: &mut crate::combat::missile::MissileAction,
        stats: &crate::combat::missile::MissileStats,
        fired: Fired,
        old: Position,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        if !(0.0 < stats.radius.x() && 0.0 < stats.radius.y()) {
            return Ok(());
        }
        let blast_level = self.master(&c.w.masters, c.player)?.blast_level;
        let met = self.missile_meets(stats, old, c)?;
        for &(id, player, kind, level) in &met {
            if action.state == MissileState::Done {
                break;
            }
            if action.met.contains(&id) {
                continue;
            }
            match action.hit_mode {
                crate::combat::missile::HitMode::TARGET_ALONE => {
                    if Some(id) != action.target {
                        continue;
                    }
                    let result = self.missile_strike(action, fired, c)?;
                    action.result = result;
                    if !stats.vanish_mode.contains(VanishMode::PASSES_THROUGH)
                        && result != StrikeResult::SHIELDED
                    {
                        self.end_missile(action);
                    }
                }
                crate::combat::missile::HitMode::MISSED => {
                    if missed_passes(c, (id, player, kind, level), action.target, blast_level) {
                        continue;
                    }
                    if Some(id) != action.target {
                        return Err(GenieError::unsupported(
                            "a missed missile meeting another object than its target",
                        ));
                    }
                    let result = self.missile_strike(action, fired, c)?;
                    action.result = result;
                    // the master's dying sprite plays there (a sprite of its own, not held)
                    if (stats.vanish_mode.contains(VanishMode::PASSES_THROUGH)
                        || result == StrikeResult::SHIELDED)
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
            action.met = met.iter().map(|m| m.0).collect();
        }
        Ok(())
    }

    /// [`Unit::missile_hits`]'s objects the missile meets on its step from `old`: the step held
    /// on the map ([`crate::combat::missile::QUERY_EDGE`] short of its far edges), the records its sweep
    /// grown by the missile's radius crosses (the manager's walk query with the missile's player,
    /// the missile itself ignored), of those each whose height the missile's meets
    /// ([`crate::combat::missile::meets_height`]): its id, player, master type and blast defence.
    ///
    /// # Errors
    ///
    /// A frozen object the sweep may cross, and an object of a master the slice does not have.
    pub(crate) fn missile_meets(
        &self,
        stats: &crate::combat::missile::MissileStats,
        old: Position,
        c: &mut Step<'_>,
    ) -> Result<Vec<(ObjectId, PlayerId, MasterKind, BlastDefense)>, GenieError> {
        let (w, h) = (c.w.spatial.map.width(), c.w.spatial.map.height());
        let hold = |v: f32, size: i32| {
            if v < 0.0 {
                0.0
            } else if size as f32 <= v {
                size as f32 - crate::combat::missile::QUERY_EDGE
            } else {
                v
            }
        };
        let at = self.position();
        let from = Point::new(hold(old.x(), w), hold(old.y(), h));
        let to = Point::new(hold(at.x(), w), hold(at.y(), h));
        let rect = Rect::spanning(from, to);
        let grow = stats.radius.x();
        c.w.spatial
            .check_known_records(rect.grown(Distance::new(grow)))?;
        let crossed = c.w.spatial.walk_hits(WalkQuery {
            start: from,
            end: to,
            rect,
            grow: Distance::new(grow),
            ally_factor: 1.0,
            player: Some(c.player),
            ignore: &[RecordId::from(self.id)],
        })?;
        let mut met = Vec::new();
        for record in crossed {
            // the game's table reads the hit's integer (an edge's may name an object)
            let Some(id) = record.key() else {
                continue;
            };
            let Some(p) = c.w.spatial.get(id) else {
                continue;
            };
            let m =
                player_master(&c.w.masters, p.player, p.master).ok_or(GenieError::unsupported(
                    "an object a missile meets of a master the slice does not have",
                ))?;
            if crate::combat::missile::meets_height(
                p.position.z(),
                m.height.get(),
                at.z(),
                stats.radius.z(),
            ) {
                met.push((id, p.player, m.kind, m.blast_defense));
            }
        }
        Ok(met)
    }

    /// The missile action's `set_state(1)` (`FUN_140a9de70`): the missile's slot 0xd8
    /// (`FUN_14118f580`), its hit points to 0.
    pub(crate) const fn end_missile(&mut self, action: &mut crate::combat::missile::MissileAction) {
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
        action: &crate::combat::missile::MissileAction,
        fired: Fired,
        c: &mut Step<'_>,
    ) -> Result<StrikeResult, GenieError> {
        if fired.charge {
            return Err(GenieError::unsupported("a charge projectile's own hit"));
        }
        if action.fired_from.is_none() {
            return self.own_strike(action, fired.by, c);
        }
        let mut victim = victim(c, action.target)?;
        let (p, mode_680) = (c.player, c.w.mode_680);
        let World {
            players,
            masters,
            spatial,
            streams,
            ..
        } = &mut *c.w;
        let Player {
            objects,
            attributes,
            ..
        } = &mut players[p.index()];
        let shooter = objects
            .by_id_mut(action.fired_from)
            .ok_or(GenieError::unsupported(
                "a missile's shooter outside its list",
            ))?;
        let master = shooter.master(masters, p)?;
        if Distance::ZERO < master.area_effect {
            return Err(GenieError::unsupported(
                "a missile of a shooter with an area effect",
            ));
        }
        let terrain = spatial.terrain.as_ref();
        let mut own = victim_of(shooter, master, attributes, p, terrain, mode_680);
        let victim = victim.as_mut().ok_or(GenieError::unsupported(
            "a missile's target the slice does not hold live",
        ))?;
        let hit = crate::combat::attack::Hit {
            at: self.position(),
            from_z: Some(action.origin.position.z()),
            missile: true,
            chance: master.hit_chance,
        };
        let object = streams.get(Stream::Object);
        let struck = crate::combat::attack::strike(&mut own, victim, &mut c.blasted, hit, object)?;
        if let Some(target) = struck.executed {
            shooter.post_own(NotifyKind::ACTION_EXECUTED, [600, target.get(), 0])?;
        }
        if shooter.hit_back(own)?.is_some() {
            return Err(GenieError::unsupported(
                "a retaliation killing a missile's shooter",
            ));
        }
        write_strike_back(c, self, victim)?;
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
        action: &crate::combat::missile::MissileAction,
        fired_from: Option<ObjectId>,
        c: &mut Step<'_>,
    ) -> Result<StrikeResult, GenieError> {
        if action.origin.owner.is_some() {
            return Err(GenieError::unsupported(
                "a shooterless missile with an origin (its elevation, FUN_14113e550)",
            ));
        }
        let master = self.master(&c.w.masters, c.player)?;
        if Distance::ZERO < master.area_effect {
            return Err(GenieError::unsupported(
                "a shooterless missile with an area effect",
            ));
        }
        let terrain = c.w.spatial.terrain.as_ref();
        let mut own = victim_of(
            self,
            master,
            &c.w.players.player(c.player)?.attributes,
            c.player,
            terrain,
            c.w.mode_680,
        );
        // the missile's own part is out of the unit while it flies
        own.source.fired_from = fired_from;
        let mut victim = victim(c, action.target)?.ok_or(GenieError::unsupported(
            "a missile's target the slice does not hold live",
        ))?;
        let victim = &mut victim;
        let hit = crate::combat::attack::Hit {
            at: self.position(),
            from_z: None,
            missile: true,
            chance: master.hit_chance,
        };
        let object = c.w.streams.get(Stream::Object);
        let struck = crate::combat::attack::strike(&mut own, victim, &mut c.blasted, hit, object)?;
        if self.hit_back(own)?.is_some() {
            return Err(GenieError::unsupported("a retaliation killing a missile"));
        }
        write_strike_back(c, self, victim)?;
        Ok(struck.result)
    }

    /// `FUN_140a9e6e0`'s landing: a missile still flying toward a target it did not meet lands on
    /// it when it lies on the target's box ([`crate::combat::missile::on_target`]) and its master's hit
    /// mode is 0 or 2, or 1 against another player's target: the shooter's hit
    /// ([`Unit::missile_strike`]), which ends it unless a shield blocked it. A missile that still
    /// flies after that hits the ground: unless its last hit was blocked, the blast of the object
    /// it was fired from there (slot 0x848, `FUN_14107f690`: nothing without an area effect; one
    /// is not modelled); it ends (`set_state` 1) and, unless its master's vanish mode has
    /// [`crate::combat::missile::VanishMode::STAYS`], is
    /// destroyed (slot 0xd0, `FUN_14106c420`: its action list goes, which lets its target and
    /// shooter go; `FUN_14118f1e0`: its hit points 0 and state 7).
    pub(crate) fn missile_lands(
        &mut self,
        missile: &crate::combat::missile::Missile,
        slot: &mut Option<crate::combat::missile::MissileAction>,
        stats: &crate::combat::missile::MissileStats,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let fired = Fired {
            charge: missile.charge,
            by: missile.fired_from,
        };
        let charge = fired.charge;
        let Some(action) = slot.as_mut() else {
            return Ok(());
        };
        self.missile_lands_on_target(action, fired, stats, c)?;
        if action.state == MissileState::Done {
            return Ok(());
        }
        if action.result != StrikeResult::SHIELDED {
            let blaster = if !charge && action.fired_from.is_some() {
                OthersRef::of(&c.w.players, c.player)?
                    .own(action.fired_from)
                    .map(|u| u.life.master_id)
            } else {
                Some(self.life.master_id)
            };
            let area = blaster
                .and_then(|id| player_master(&c.w.masters, c.player, id))
                .map(|m| m.area_effect)
                .ok_or(GenieError::unsupported(
                    "a landed missile's blast of a master the slice does not have",
                ))?;
            if Distance::ZERO < area {
                return Err(GenieError::unsupported("a landed missile's blast"));
            }
        }
        self.end_missile(action);
        if !stats.vanish_mode.contains(VanishMode::STAYS) {
            self.missile_destroyed(slot, c)?;
        }
        Ok(())
    }

    /// [`Unit::missile_lands`] on a target it did not meet yet: it lands when it lies on the
    /// target's box and its master's hit mode is 0 or 2, or 1 against another player's target;
    /// the shooter's hit ([`Unit::missile_strike`]) ends it unless a shield blocked it.
    fn missile_lands_on_target(
        &mut self,
        action: &mut crate::combat::missile::MissileAction,
        fired: Fired,
        stats: &crate::combat::missile::MissileStats,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let Some(target) = action.target else {
            return Ok(());
        };
        if action.state == MissileState::Done || action.met.contains(&target) {
            return Ok(());
        }
        let v = victim(c, action.target)?.ok_or(GenieError::unsupported(
            "a missile's target the slice does not hold live",
        ))?;
        if stats.hit_mode.strikes_on_landing(c.player != v.player)
            && crate::combat::missile::on_target(
                self.position(),
                stats.radius,
                v.position,
                v.radius,
            )
        {
            let result = self.missile_strike(action, fired, c)?;
            action.result = result;
            if result != StrikeResult::SHIELDED {
                self.end_missile(action);
            }
        }
        Ok(())
    }

    /// A landed missile's destruction (slot 0xd0, `FUN_14106c420`): its action list goes,
    /// which lets its target and shooter go ([`crate::world::lets::release`]); then
    /// `FUN_14118f1e0`: its hit points 0 and state 7, out of the visible resources below state
    /// 4. One inside another is not modelled.
    fn missile_destroyed(
        &mut self,
        slot: &mut Option<crate::combat::missile::MissileAction>,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        if let Some(a) = slot.take() {
            let by = self.releaser(c)?;
            for t in [a.target, a.fired_from].into_iter().flatten() {
                crate::world::lets::release(c, self, t, by)?;
            }
        }
        if self.life.state < ObjectState::Removed {
            if self.life.inside.is_some() {
                return Err(GenieError::unsupported(
                    "a destroyed object inside another (slot 0x1a8)",
                ));
            }
            let before = self.life.state;
            self.life.hp = HitPoints::ZERO;
            self.life.goto_sleep = false;
            self.life.state = ObjectState::Removed;
            c.w.spatial.sight_state(self.id, ObjectState::Removed)?;
            if before < ObjectState::DyingSprite {
                let master = self.master(&c.w.masters, c.player)?;
                forget_resource(&mut c.w.spatial, Some(master), self.id, self.position())?;
            }
        }
        Ok(())
    }
}
