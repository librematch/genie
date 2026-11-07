//! State 4's walk to the target and state 7's swing with its hit and its trample.

use crate::combat::Charge;
use crate::combat::attack::AtOnce;
use crate::combat::attack::AttackState;
use crate::combat::attack::{
    AfterHit, Attack, AttackReads, DRAW_SCALE, EPSILON, Fight, HALF, Hit, Readiness, after_hit,
    blast_so_far, reset_sprite, set_attack_timer, set_walker_anim, squared_reach, strike,
};
use crate::combat::damage::HitKind;
use crate::combat::damage::ShieldBlock;
use crate::combat::damage::Victim;
use crate::combat::victims::write_strike_back;
use crate::combat::volley::Aimed;
use crate::combat::volley::VolleyEnd;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::math::rng::{Stream, Well512};
use crate::movement::Speed;
use crate::movement::move_to::GiveUp;
use crate::movement::move_to::Patience;
use crate::movement::move_to::SubCode;
use crate::movement::{Me, footprint_gap_squared};
use crate::objects::Frame;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::SpriteFlags;
use crate::objects::UpdateCode;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::HitChance;
use crate::objects::sprite::{ActiveSprite, new_sprite};
use crate::objects::unit::Unit;
use crate::unit_ai::NotifyKind;
use crate::world::Seconds;

use crate::combat::AttackTargetId;
use crate::combat::damage::HitLayer;
use crate::combat::damage::{blast, charge_hit, elevation_factor, shield, take};
use crate::objects::layers::Action;
use crate::world::ATTACKER_UNMOVING;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Attack {
    /// States 4, 5 and 11 (cases 3, 4, 10): the sub-action list's update (`FUN_140a992b0`,
    /// [`Attack::update_list`]) walks the quiet `MoveTo` to the target at `to` (its place and
    /// radii) or to where it was seen, and its code decides. 0 (it walks or waits): a unit
    /// still farther than its reach goes on, one within it turns ready (`set_state` 6); the
    /// squared reach of a unit without a range is 0.01. 1 and 2 (it ended: the list deletes it,
    /// its destructor, `0x140aa1f30`, stops the unit): ready for state 4, else
    /// [`Attack::arrived_at_seen`]. 3 (it failed): [`Attack::walk_failed`]. 5 (its target
    /// was let go) and the codes above 5 do nothing; 4 is a loud `MoveTo`'s.
    ///
    /// # Errors
    ///
    /// The walk's, and a unit that does not move.
    pub(super) fn walk(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        fight: &Fight,
        attack_reads: &AttackReads,
        (target, radius): (Position, Footprint),
    ) -> GenieResult<UpdateCode> {
        let roles = &step.world.model.roles.clone();
        let gives_up = GiveUp {
            patience: if !attack_reads.player_kind.is_computer()
                && roles
                    .of_class(fight.own.class)
                    .contains(ClassRoles::VILLAGER)
            {
                Patience::WaitsOnce
            } else {
                Patience::Fails
            },
            stance: attack_reads.stance,
        };
        match self.update_list(step, unit, me, attack_reads, gives_up)? {
            SubCode::UnderWay => {
                self.within_reach(step, unit, fight, attack_reads, (target, radius))
            }
            SubCode::Done => {
                self.delete_move_to(step.world.parts.mover_mut(unit.id))?;
                if !matches!(self.state, AttackState::Move { .. }) {
                    return self.arrived_at_seen(step, unit, fight, attack_reads, target);
                }
                self.ready(
                    unit,
                    &mut step.world.attack_ground(unit.id),
                    fight,
                    attack_reads,
                )?;
                Ok(UpdateCode::UnderWay)
            }
            SubCode::Failed => self.walk_failed(step, unit),
            SubCode::TargetGone => Ok(UpdateCode::UnderWay),
        }
    }

    /// The `MoveTo` that walks or waits (code 0): a unit more than its squared reach from the
    /// target at `to` goes on, one within it turns ready (`set_state` 6).
    fn within_reach(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        fight: &Fight,
        attack_reads: &AttackReads,
        (target, radius): (Position, Footprint),
    ) -> GenieResult<UpdateCode> {
        let ground = step
            .world
            .parts
            .mover_of(unit.id)
            .map(|mover| mover.stats.radius.ground())
            .ok_or(ATTACKER_UNMOVING)?;
        let at = unit.place.position();
        let reach = squared_reach(self.range, &step.world.model.tuning.attack);
        if reach.get().abs() <= EPSILON || reach < footprint_gap_squared(at, ground, target, radius)
        {
            return Ok(UpdateCode::UnderWay);
        }
        self.ready(
            unit,
            &mut step.world.attack_ground(unit.id),
            fight,
            attack_reads,
        )?;
        Ok(UpdateCode::UnderWay)
    }

    /// The failed `MoveTo` (its update returns 3 without a notify, it being quiet):
    /// `set_state(13)`; a unit's own Attack (`+0x60` 0) posts 0x1f9 with 600; the update returns
    /// 3 and the list removes the Attack.
    pub(super) fn walk_failed(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
    ) -> GenieResult<UpdateCode> {
        // the list deleted the MoveTo: no stop
        self.state = self.state.bare();
        self.finish_in(
            AttackState::Failed,
            unit,
            step.world.parts.motion_mut(unit.id),
            step.world.streams.get_mut(Stream::Sprite),
            step.world.model.tuning.sprite,
        )?;
        if self.voice.is_loud() {
            unit.post_own(
                &mut step.world.parts.unit_ai,
                NotifyKind::ACTION_FAILED,
                [600, 0, 0],
            )?;
        }
        Ok(UpdateCode::Failed)
    }

    /// State 7 (case 6). A melee swing counts down and lands its hit when its sprite reaches the
    /// firing frame (half its frames without one): the attack timer becomes the reload less the
    /// time to that frame (`FUN_141089820`) and the target takes the hit; a dead target ends the
    /// swing (`set_state` 2). A ranged unit's swing does not count down and fires at its firing
    /// frame (at once with frame 0): its volley at once ([`Unit::volley`]). After the hit, the
    /// swing's sprite looping makes the unit ready again (`set_state` 6).
    pub(super) fn swing(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        fight: &mut Fight,
        attack_reads: &AttackReads,
    ) -> GenieResult<UpdateCode> {
        let Some(sprite) = unit.sprite else {
            return Err(GenieError::unsupported("a swing without the unit's sprite"));
        };
        if self.pending {
            self.swing_to_hit(step, unit, actions, fight, attack_reads, sprite)?;
        } else {
            self.swing_after_hit(step, unit, fight, attack_reads, sprite)?;
        }
        Ok(UpdateCode::UnderWay)
    }

    /// The swing before its hit (state 7 with the hit pending): a melee swing counts down; at the
    /// firing frame (half the sprite's frames for a melee unit without one, at once with frame 0
    /// for a ranged one) the hit lands ([`Attack::land_hit`]). Class 51's swing, a melee swing of
    /// classes 35 and 64, and a ranged one without a firing frame are not modelled.
    pub(super) fn swing_to_hit(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        fight: &mut Fight,
        attack_reads: &AttackReads,
        sprite: ActiveSprite,
    ) -> GenieResult<()> {
        let roles = &step.world.model.roles.clone();
        let ranged = self.arms.missile.is_some();
        if roles
            .of_class(fight.own.class)
            .contains(ClassRoles::PACKED_TREBUCHET)
            || (!ranged
                && roles
                    .of_class(fight.own.class)
                    .intersects(ClassRoles::PETARD | ClassRoles::LAND_MINE))
        {
            return Err(GenieError::unsupported("a swing of class 35, 51 or 64"));
        }
        if ranged && self.arms.fire_frame < Frame::ZERO {
            return Err(GenieError::unsupported(
                "a ranged swing without a firing frame",
            ));
        }
        let fire = if ranged {
            self.arms.fire_frame
        } else if self.arms.fire_frame < Frame::ONE {
            Frame::of_data((sprite.graphic.frames.as_f32() * HALF) as u32 as i16)
        } else {
            self.arms.fire_frame
        };
        if !ranged {
            self.swing -= step.dt.seconds();
        }
        if fire != Frame::ZERO && !sprite.reached(fire) {
            return Ok(());
        }
        self.land_hit(step, unit, actions, fight, attack_reads, sprite)
    }

    /// The swing's hit at its firing frame: the attack timer becomes the reload less the time to
    /// that frame (`FUN_141089820`), then the unit's hit at its target (slot 0x858,
    /// [`Unit::volley`]), which may leave the target the melee hit ([`Attack::melee_hit`]). A dead
    /// target ends the swing (`set_state` 2); else the rest of the reload past the sprite is the
    /// swing's.
    pub(super) fn land_hit(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        fight: &mut Fight,
        attack_reads: &AttackReads,
        sprite: ActiveSprite,
    ) -> GenieResult<()> {
        let roles = &step.world.model.roles.clone();
        let ranged = self.arms.missile.is_some();
        let frame = if self.arms.fire_frame == Frame::ZERO && !ranged {
            sprite.graphic.frames.half()
        } else {
            self.arms.fire_frame
        };
        set_attack_timer(
            step.world.parts.combat_mut(unit.id)?,
            attack_reads.world_time,
            attack_reads.reload
                - Seconds::new(frame.as_f32() * sprite.graphic.frame_duration.get()),
            &step.world.model.tuning.attack,
        );
        // slot 0x858 (FUN_14120c0a0): the volley, whose hit of a master without a missile, a
        // volley missile or a charge projectile is slot 0x860's melee one (FUN_141081020)
        let end = unit.volley(
            step,
            Aimed::at(self.target.map(AttackTargetId::id), &fight.victim),
            roles,
        )?;
        match end {
            VolleyEnd::Done => {}
            VolleyEnd::MeleeHit => {
                Self::melee_hit(step, unit, actions, fight, attack_reads.hit_chance, roles)?;
            }
        }
        self.pending = false;
        if fight.victim.hp <= HitPoints::ZERO {
            return self.enter(AttackState::Killed, step.world.parts.motion_mut(unit.id));
        }
        let rest = attack_reads.reload
            - Seconds::new(sprite.graphic.frames.as_f32() * sprite.graphic.frame_duration.get());
        self.rest = rest;
        self.swing = rest;
        if rest < Seconds::ZERO {
            self.rest = Seconds::ZERO;
        }
        Ok(())
    }

    /// Slot 0x860 (`FUN_1412115e0`) where slot 0x858 leaves the hit to it
    /// ([`VolleyEnd::MeleeHit`]): the fight's target takes the hit of hit chance `chance`
    /// ([`Attack::hit`]), the unit tells its unit AI the action executed (0x200 with 600) and the
    /// target and the blast's objects go back to the world ([`write_strike_back`]).
    fn melee_hit(
        step: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        fight: &mut Fight,
        chance: HitChance,
        roles: &Roles,
    ) -> GenieResult<()> {
        let executed = {
            let object = step.world.streams.get_mut(Stream::Object);
            Self::hit(
                fight,
                &mut step.blasted,
                chance,
                object,
                HitLayer {
                    roles,
                    slots: &step.world.model.slots,
                    damage_tuning: &step.world.model.tuning.damage,
                },
            )?
        };
        if let Some(target) = executed {
            unit.post_own(
                &mut step.world.parts.unit_ai,
                NotifyKind::ACTION_EXECUTED,
                [600, target.get(), 0],
            )?;
        }
        write_strike_back(step, unit, actions, &mut fight.victim, roles)
    }

    /// The swing after its hit: a done swing ends (`set_state` 2), a ready one readies
    /// (`set_state` 6), one past its interrupt frame checks its target (readyToAttack with 0: one
    /// out of reach is followed, `set_state` 4). Once the sprite looped (or does not loop) the unit
    /// is ready again; a reload longer than the sprite waits at its last frame with the idle
    /// attack sprite (else the standing one) at speed 1.
    pub(super) fn swing_after_hit(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        fight: &Fight,
        attack_reads: &AttackReads,
        sprite: ActiveSprite,
    ) -> GenieResult<()> {
        let sprite_tuning = step.world.model.tuning.sprite;
        let timer = step.world.parts.combat_of(unit.id).attack_timer;
        let mut ground = step.world.attack_ground(unit.id);
        match after_hit(
            self.arms.missile,
            timer,
            self.target,
            self.arms.interrupt_frame,
            self.arms.fire_frame,
            sprite.frame,
        ) {
            AfterHit::Done => return self.enter(AttackState::Killed, ground.motion.reborrow()),
            AfterHit::Ready => {
                return self.ready(unit, &mut ground, fight, attack_reads);
            }
            // readyToAttack with its third argument 0: a target out of reach is followed
            // (`set_state` 4); a ready unit only faces it; the swing goes on either way
            AfterHit::Recheck => {
                if let Readiness::Away(gap) =
                    self.ready_to_attack(unit, &mut ground, fight, attack_reads, AtOnce::Tested)?
                {
                    self.follow(unit, &mut ground, attack_reads, fight, gap)?;
                }
            }
            AfterHit::Continue => {}
        }
        // the loop test reads the unit's sprite as it is now (`+0x140`): readyToAttack may have
        // shown the standing one
        let sprite = unit.sprite.unwrap_or(sprite);
        if !sprite.looped && sprite.graphic.flags.contains(SpriteFlags::ANIMATED) {
            return Ok(());
        }
        // a reload longer than the sprite: at its last frame the unit waits with its idle attack
        // sprite (`+0x88`), else its standing one (slot 0x68), from its first frame when it
        // changes (`FUN_1413b9f00`), at speed 1
        if Seconds::ZERO < self.swing && sprite.reached(sprite.graphic.frames) {
            let idle = self
                .arms
                .idle_attack_sprite
                .or(self.arms.idle_sprite)
                .ok_or(GenieError::unsupported(
                    "a swing's end without a sprite to wait with",
                ))?;
            let changed = sprite.graphic.id != idle.id;
            new_sprite(
                &mut unit.sprite,
                ground.motion.move_stats_mut(),
                idle,
                ground.sprite_rng,
                sprite_tuning,
            );
            if changed && let Some(current) = unit.sprite.map(|sp| sp.graphic) {
                let _ = reset_sprite(&mut unit.sprite, current);
            }
            set_walker_anim(ground.motion.reborrow(), Speed::new(1.0));
        }
        self.ready(unit, &mut ground, fight, attack_reads)
    }

    /// The hit of a melee unit (slot 0x858 -> `FUN_141081020` -> slot 0x860,
    /// `FUN_1412115e0`) of hit chance `chance` on the fight's target, the blast's objects
    /// `blasted` about it: a unit with an area effect and a weapon range below 0.99 tramples
    /// ([`Attack::trample`]) unless its charge of type 3 is not its hit's (slot 0x2f0); else
    /// `FUN_141084230` ([`Attack::strike`]). Returns the target of the executed action the unit
    /// tells its unit AI of (0x200 with 600). Not held: the damaged flag of masters 83, 293, 122
    /// and 216 (`+0x683`, `FUN_141081020`'s, [`Unit::fire`]); masters 122 and 123 under the
    /// game's `+0xc14` (`FUN_140c884c0`, 0 as the tick oracle holds it) would take the target at
    /// once.
    pub(super) fn hit(
        fight: &mut Fight,
        blasted: &mut [Victim],
        chance: HitChance,
        object_rng: &mut Well512,
        layer: HitLayer<'_>,
    ) -> GenieResult<Option<ObjectId>> {
        let HitLayer {
            roles,
            damage_tuning,
            ..
        } = layer;
        let attacker = fight.own.source;
        if Distance::ZERO < attacker.area_effect
            && attacker.weapon_range < Distance::new(damage_tuning.blast_range)
            && (attacker.charge_kind != ChargeKind::AREA
                || charge_hit(&attacker, &fight.victim, roles))
        {
            return Self::trample(fight, blasted, chance, object_rng, layer);
        }
        Self::strike(fight, blasted, chance, object_rng, layer)
    }

    /// `FUN_141084230` with the unit's own place as the hit's origin ([`strike`]).
    pub(super) fn strike(
        fight: &mut Fight,
        blasted: &mut [Victim],
        chance: HitChance,
        object_rng: &mut Well512,
        layer: HitLayer<'_>,
    ) -> GenieResult<Option<ObjectId>> {
        let hit = Hit {
            at: fight.own.position,
            from_z: Some(fight.own.position.z()),
            kind: HitKind::Melee,
            chance,
        };
        let struck = strike(
            &mut fight.own,
            &mut fight.victim,
            blasted,
            hit,
            object_rng,
            layer,
        )?;
        Ok(struck.executed)
    }

    /// `FUN_1412115e0`'s trample: the blast about the unit, past its target
    /// ([`damage::blast`](crate::combat::damage::blast); the target stays among the objects on its
    /// tiles, the main building its annexes' hits go to); a building target's blast so far
    /// (`+0xb90`, its main building's for an annex, `0x141211933`) is what its hit leaves out; the
    /// unit tells its unit AI the action executed (0x200, whatever its master); the 64-bit
    /// Object-stream draw against the hit chance; a hit the target's shield does not block
    /// (`FUN_14108d8a0`, on a target of any type) is its slot 0xe8 at the elevation factor times
    /// the hit's 1. A charge of type 1 or 3 the blast or the hit was for (`+0x8a8`) is emptied.
    pub(super) fn trample(
        fight: &mut Fight,
        blasted: &mut [Victim],
        chance: HitChance,
        object_rng: &mut Well512,
        layer: HitLayer<'_>,
    ) -> GenieResult<Option<ObjectId>> {
        let HitLayer { damage_tuning, .. } = layer;
        let (center, target) = (fight.own.position, fight.victim.id);
        let mut targets: Vec<&mut Victim> = blasted.iter_mut().collect();
        targets.push(&mut fight.victim);
        blast(
            &mut fight.own,
            center,
            Some(target),
            None,
            &mut targets,
            1.0,
            layer,
        )?;
        let taken = blast_so_far(&fight.victim, blasted)?;
        let draw = object_rng.raw64();
        let roll = (i64::from(draw as u32 & 0x7fff_ffff) * 100) / DRAW_SCALE;
        if roll <= i64::from(chance.get()) {
            let factor = elevation_factor(
                fight.own.position.z(),
                fight.own.attack_elevation,
                &fight.victim,
                damage_tuning,
            );
            if shield(&mut fight.victim, HitKind::Melee) == ShieldBlock::Passes {
                let weapons = fight.own.weapons.clone();
                let mut others: Vec<&mut Victim> = blasted.iter_mut().collect();
                take(
                    &mut fight.victim,
                    &mut fight.own,
                    &mut others,
                    &weapons,
                    factor,
                    taken,
                    layer,
                )?;
            }
        }
        let attacker = &mut fight.own.source;
        if attacker.charge_hit && attacker.charge_kind.adds_to_attack() {
            attacker.charge = Charge::ZERO;
        }
        Ok(Some(target))
    }
}
