//! A unit's strikes back and its volleys: the missiles it fires and launches.

use crate::combat::MissileCount;
use crate::combat::damage::Kill;
use crate::combat::damage::Victim;
use crate::combat::missile::ShooterId;
use crate::combat::missile::{
    Aimer, reach, scatter, spawn_point, volley_count, volley_reach, volley_spawn_point,
};
use crate::combat::{AttackCount, Firepower};
use crate::map::Distance;
use crate::map::Radii;
use crate::map::{GroundOffset, Position};
use crate::math::rng::Stream;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Combat;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::VolleySpread;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::PlayerId;
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitAi;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::TurnTuning;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

mod launch;

impl Unit {
    /// The unit's own part of its hit, written back: what a retaliation or reflected damage left
    /// of its hit points and powerups (its combat layer `combat`), its charge and `+0x8a8`, the
    /// damage notify (500) a retaliation posts to its unit AI (`ai`); a retaliation that killed it is the caller's (Gaia
    /// taking a killed attacker is not modelled).
    pub(crate) fn hit_back(
        &mut self,
        own: Victim,
        (ai, combat): (Option<&mut UnitAi>, &mut Combat),
    ) -> GenieResult<Option<Kill>> {
        self.life.hp = own.hp;
        if let Some(a) = own.armored.as_ref() {
            combat.hp_powerup = a.hp_powerup;
        }
        combat.charge = own.source.charge;
        combat.charge_hit = own.source.charge_hit;
        if let Some(ai) = ai {
            for &(attacker, p) in &own.damaged {
                let n = Notify {
                    caller: attacker,
                    recipient: self.id,
                    kind: NotifyKind::DAMAGED,
                    p,
                };
                ai.post(n, self.life.state)?;
            }
        }
        if own.killed.is_some_and(|k| k.to_gaia) {
            return Err(GenieError::unsupported(
                "a killed attacker that goes to Gaia",
            ));
        }
        Ok(own.killed)
    }

    /// Slot 0x858 (`FUN_14120c0a0`), a combat object's hit at its target from the Attack's swing
    /// when it has a missile, a volley missile or a charge projectile
    /// ([`crate::combat::attack::volleys`]; the others' melee hit is the Attack's). A master
    /// without `max_attacks_in_volley` fires its missile ([`Unit::fire`]). One with it fires its
    /// volley (`next_volley`, `+0xa04`, already fired of [`crate::combat::missile::volley_count`]
    /// missiles): unless its `combat_ability` has
    /// [`crate::objects::masters::CombatAbility::FULL_VOLLEY`], a master with a spread fires 1 to 3
    /// at a time (an Object-stream draw, "attack"), the rest of the volley next with its attack
    /// timer at 0.05; the volley's last (masters 73 and 559 tell their unit AI the action executed)
    /// leaves `next_volley` 0. Of the missiles at a time, the first of a volley is its missile
    /// (none without one) and the others its extra missiles ([`Unit::volley_missile`]), each
    /// scattered by two draws. A target of a player but Gaia has its player make a flare at the
    /// attacker ([`crate::world::World::make_flare`]). Not modelled: the self-destructing masters
    /// (527, 528, 1104, 2133..2135, classes 35 and 64 alive), a charge (charge types 6 and 7) and a
    /// target of a computer player (its notify 0x214). Not held: the local player's statistics
    /// (`FUN_140a4ba80`).
    pub(crate) fn volley(
        &mut self,
        c: &mut Step<'_>,
        aimed: Aimed,
        roles: &Roles,
    ) -> GenieResult<()> {
        let master = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.model.masters.of(c.player),
        )?;
        if (roles.of_master(master.id).intersects(
            MasterRoles::DEMOLITION_SHIP
                | MasterRoles::HEAVY_DEMOLITION_SHIP
                | MasterRoles::DEMOLITION_RAFT
                | MasterRoles::INCENDIARY_RAFT
                | MasterRoles::INCENDIARY_SHIP
                | MasterRoles::HEAVY_INCENDIARY_SHIP,
        ) || roles
            .of_class(master.class)
            .intersects(ClassRoles::PETARD | ClassRoles::LAND_MINE))
            && self.life.state == ObjectState::Alive
        {
            return Err(GenieError::unsupported(
                "a self-destructing attacker's hit (its object 112 and its death)",
            ));
        }
        if master.creatable_part().charge_kind.ranged() {
            return Err(GenieError::unsupported(
                "a charged hit (slot 0x2f8 with charge types 6 and 7)",
            ));
        }
        let missile = master.combat_part().missile.master().is_some();
        // what the rest reads of the master, before the notify changes the unit
        let notifies = roles
            .of_master(master.id)
            .intersects(MasterRoles::CHU_KO_NU | MasterRoles::ELITE_CHU_KO_NU);
        let (
            trebuchet,
            VolleySpread {
                x: spread_x,
                y: spread_y,
                ..
            },
        ) = (
            roles
                .of_class(master.class)
                .contains(ClassRoles::UNPACKED_TREBUCHET),
            master.creatable_part().volley_spread,
        );
        let (first, extra) = if master.creatable_part().max_volley == MissileCount::ZERO {
            (true, 0)
        } else {
            let full = master
                .combat_part()
                .combat_ability
                .contains(CombatAbility::FULL_VOLLEY);
            self.volley_shots(
                c,
                (
                    master.creatable_part().max_volley,
                    master.creatable_part().volley_spread,
                    full,
                ),
                notifies,
            )?
        };
        if missile || 0 < extra {
            // a target of a player but Gaia: its player makes a flare (master 112, slot 0x1c0)
            // at the attacker's place, which lasts twice as long from a trebuchet (class 54:
            // its amount, +0x310, doubled)
            if aimed.player != PlayerId::GAIA {
                c.w.make_flare(aimed.player, self.position(), trebuchet)?;
            }
            for _ in 0..extra {
                let object = c.w.streams.get(Stream::Object);
                let (x, y) = (object.rand15(), object.rand15());
                let scatter = GroundOffset::new(scatter(x, spread_x), scatter(y, spread_y));
                self.volley_missile(c, &aimed, scatter, roles)?;
            }
        }
        if (extra != 0 || missile)
            && c.w
                .players
                .get(aimed.player.index())
                .is_some_and(|q| q.kind.is_computer())
        {
            return Err(GenieError::unsupported(
                "a hit on a computer player's object (its notify 0x214)",
            ));
        }
        if first && missile {
            self.fire(c, &aimed, roles)?;
        }
        Ok(())
    }

    /// The volley's shots of a master with a volley (`max_volley`, `spread`, `full`: its full
    /// volley ability): the count its firepower gives less the shots already counted
    /// (`+0x8f0`); without the full volley, a split of one or two (a draw where the volley
    /// spreads) counts on and fires again after 0.05; a whole volley starts the count over and
    /// tells masters 73 and 559's unit AIs the action executed (`notifies`). Returns whether
    /// this is the volley's first shot and its extra missiles.
    fn volley_shots(
        &self,
        c: &mut Step<'_>,
        (max_volley, spread, full): (MissileCount, VolleySpread, bool),
        notifies: bool,
    ) -> GenieResult<(bool, i32)> {
        let count = volley_count(c.w.parts.combat_of(self.id).firepower, max_volley);
        let old = c.w.parts.combat_of(self.id).next_volley;
        let mut n = count - old.counted();
        let VolleySpread {
            x: spread_x,
            y: spread_y,
            ..
        } = spread;
        let mut all = true;
        if !full {
            let k = if Distance::ZERO < spread_x || Distance::ZERO < spread_y {
                let draw = c.w.streams.get(Stream::Object).rand15() as i32;
                (draw * 2) / 0x7fff + 1
            } else {
                1
            };
            if k < n {
                let combat = c.w.parts.combat_mut(self.id)?;
                combat.next_volley = combat.next_volley.plus(k as u8);
                // an immediate 0.05
                combat.attack_timer = Seconds::new(0.05);
                n = k;
                all = false;
            }
        }
        if all {
            c.w.parts.combat_mut(self.id)?.next_volley = MissileCount::ZERO;
            if notifies {
                self.post_own(
                    &mut c.w.parts.unit_ai,
                    NotifyKind::ACTION_EXECUTED,
                    [600, -1, 0],
                )?;
            }
        }
        Ok((
            old == MissileCount::ZERO,
            if old == MissileCount::ZERO { n - 1 } else { n },
        ))
    }

    /// `FUN_141081020`, a ranged unit's swing at its target (from its volley): its attack count
    /// (`+0x900`) rises; its master's missile (`missile_id`; a charge projectile, slot 0x2f8 with
    /// charge types 6 and 7, is not modelled) at its weapon offset turned by its facing
    /// ([`crate::combat::missile::spawn_point`]), its reach [`crate::combat::missile::reach`], aimed at the
    /// target's place, the shooter its origin ([`Unit::launch`], `FUN_14113f520`). Last the
    /// shooter tells its unit AI the action executed (0x200 with 600, but for masters 73 and
    /// 559). Not modelled: a target of a computer player, which a shooter whose blast does not
    /// spare friends notifies (0x20f). Not held: the damaged flag of masters 83, 293, 122 and 216
    /// (`+0x683`).
    pub(crate) fn fire(
        &mut self,
        c: &mut Step<'_>,
        aimed: &Aimed,
        roles: &Roles,
    ) -> GenieResult<()> {
        let combat = c.w.parts.combat_mut(self.id)?;
        combat.attack_count = combat.attack_count.next();
        let master = self.master(c.w.master_tables(), c.player)?;
        if master.creatable_part().charge_kind.ranged() {
            return Err(GenieError::unsupported(
                "a charge projectile (charge types 6 and 7, slot 0x2f8)",
            ));
        }
        let facing =
            c.w.parts
                .mover_of(self.id)
                .map(|m| m.angle)
                .ok_or(GenieError::unsupported("a ranged unit that does not move"))?;
        let shot = Shot {
            made: master
                .combat_part()
                .missile
                .master()
                .ok_or(GenieError::missing_data(
                    "a ranged unit whose master has no missile",
                ))?,
            at: spawn_point(self.position(), facing, master.combat_part().weapon_offset),
            point: aimed.position,
            reach: reach(
                master.radius,
                master.combat_part().weapon_range,
                &c.w.model.tuning.missile,
            ),
            shooter: true,
            aimer: Aimer {
                hit_chance: master.combat_part().hit_chance,
                spread: master.combat_part().missed_spread,
                class: master.class,
            },
            attack_count: c.w.parts.combat_of(self.id).attack_count,
        };
        let (weapon_range, blast_level) = (
            master.combat_part().weapon_range,
            master.combat_part().blast_level,
        );
        self.launch(c, aimed, &shot, roles)?;
        if !roles
            .of_master(self.life.master_id)
            .intersects(MasterRoles::CHU_KO_NU | MasterRoles::ELITE_CHU_KO_NU)
        {
            self.post_own(
                &mut c.w.parts.unit_ai,
                NotifyKind::ACTION_EXECUTED,
                [600, ObjectId::field(aimed.target), 0],
            )?;
        }
        let spares_friends = blast_level.spares_friends(weapon_range);
        if !spares_friends
            && c.w
                .players
                .get(aimed.player.index())
                .is_some_and(|q| q.kind.is_computer())
        {
            return Err(GenieError::unsupported(
                "a missile fired at a computer player's object (its notify 0x20f)",
            ));
        }
        Ok(())
    }

    /// `FUN_141226690`, a volley's extra missile scattered by `scatter`: the master's volley
    /// missile (`FUN_141203760`, a master its player has), shooterless, else its missile with
    /// the shooter; placed by [`crate::combat::missile::volley_spawn_point`], its reach
    /// [`crate::combat::missile::volley_reach`], aimed at the target's place plus the scatter turned by
    /// the facing (`FUN_14113f410`); the missile's own aim when shooterless; it records the
    /// object as what it was fired from (`+0x998`). Last the object tells its unit AI the action
    /// executed (0x200 with 600 and no target, but for masters 73 and 559). Not modelled: a
    /// missile of master 527 or 528 (whose aim reads its attack count, set after it) and a
    /// master without a missile of either kind.
    pub(crate) fn volley_missile(
        &mut self,
        c: &mut Step<'_>,
        aimed: &Aimed,
        scatter: GroundOffset,
        roles: &Roles,
    ) -> GenieResult<()> {
        let masters =
            c.w.model
                .masters
                .get(c.player.index())
                .ok_or(GenieError::unsupported("a volley without the masters"))?;
        let master = self.master_in(OwnerMasters::new(&c.w.parts.own_master, Some(masters)))?;
        let facing =
            c.w.parts
                .mover_of(self.id)
                .map(|m| m.angle)
                .ok_or(GenieError::unsupported("a ranged unit that does not move"))?;
        let building = roles.of_class(master.class).contains(ClassRoles::BUILDING);
        let at = volley_spawn_point(
            self.position(),
            facing,
            master.combat_part().weapon_offset,
            master.creatable_part().volley_spread.start,
            scatter,
            building.then_some(aimed.position.ground()),
        );
        let volley = master
            .creatable_part()
            .volley_missile
            .and_then(|id| masters.get(id));
        let made = match volley {
            Some(v) => v,
            None if master.combat_part().missile.master().is_some() => master
                .combat_part()
                .missile
                .master()
                .and_then(|m| masters.get(m))
                .ok_or(GenieError::unsupported(
                    "a missile its player's masters do not have",
                ))?,
            None => {
                return Err(GenieError::unsupported(
                    "a volley's extra missile of a master without a missile",
                ));
            }
        };
        if roles
            .of_master(made.id)
            .intersects(MasterRoles::DEMOLITION_SHIP | MasterRoles::HEAVY_DEMOLITION_SHIP)
        {
            return Err(GenieError::unsupported(
                "a volley's extra missile of master 527 or 528 (its attack count)",
            ));
        }
        let shooter = volley.is_none();
        let aimer = if shooter { master } else { made };
        let shot = Shot {
            made: made.id,
            at,
            point: spawn_point(aimed.position, facing, scatter.with_height(0.0)),
            reach: volley_reach(
                master.radius,
                master.combat_part().weapon_range,
                &c.w.model.tuning.missile,
            ),
            shooter,
            aimer: Aimer {
                hit_chance: aimer.combat_part().hit_chance,
                spread: aimer.combat_part().missed_spread,
                class: aimer.class,
            },
            attack_count: AttackCount::ZERO,
        };
        self.launch(c, aimed, &shot, roles)?;
        if !roles
            .of_master(self.life.master_id)
            .intersects(MasterRoles::CHU_KO_NU | MasterRoles::ELITE_CHU_KO_NU)
        {
            self.post_own(
                &mut c.w.parts.unit_ai,
                NotifyKind::ACTION_EXECUTED,
                [600, -1, 0],
            )?;
        }
        Ok(())
    }
}

/// What a ranged unit's fire reads of its target ([`Unit::fire`]): its id, place, master's radii
/// and class, and player.
pub(crate) struct Aimed {
    pub(crate) target: Option<ObjectId>,
    pub(crate) position: Position,
    pub(crate) radius: Radii,
    pub(crate) class: ClassId,
    pub(crate) player: PlayerId,
}

impl Aimed {
    /// The fire's aim at `target` as the strike read it (`v`): its place, its master's radii
    /// and height, its class and its player.
    pub(crate) const fn at(target: Option<ObjectId>, v: &Victim) -> Self {
        Self {
            target,
            position: v.position,
            radius: v.radius.with_height(v.height.get()),
            class: v.class,
            player: v.player,
        }
    }
}

/// `FUN_14120ea30` of an attacker of `master` with `firepower` (`+0xa00`): a building can attack
/// when of class 51 or 54, with a missile, or with a firepower above 1; any other attacker can.
pub(crate) fn armed(
    master: &Master,
    firepower: Firepower,
    roles: &Roles,
    turn_tuning: &TurnTuning,
) -> bool {
    master.kind != MasterKind::BUILDING
        || roles
            .of_class(master.class)
            .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET)
        || master.combat_part().missile.master().is_some()
        || Firepower::new(turn_tuning.armed_firepower) < firepower
}

/// Whether two lists hold the same objects, in any order.
pub(crate) fn same_members(a: &[ObjectId], b: &[ObjectId]) -> bool {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

/// What a missile was fired as, which its hits read while its own part is out of the unit: a
/// charge's projectile (`+0x99c`) and what it was fired from (`+0x998`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fired {
    pub(crate) charge: bool,
    pub(crate) by: Option<ShooterId>,
}

/// A missile [`Unit::launch`] makes: its master, where it starts, the point its action aims at,
/// its reach (`+0x990`), whether its action has the object as its shooter (else none), what its
/// aim reads of the firing object (or the missile) and the attack count the aim sees.
pub(crate) struct Shot {
    pub(crate) made: MasterId,
    pub(crate) at: Position,
    pub(crate) point: Position,
    pub(crate) reach: Distance,
    pub(crate) shooter: bool,
    pub(crate) aimer: Aimer,
    pub(crate) attack_count: AttackCount,
}
