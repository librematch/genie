//! A unit's strikes back and its volleys: the missiles it fires and launches.

use crate::GenieError;
use crate::combat::MissileCount;
use crate::combat::damage::Victim;
use crate::combat::missile::AimAt;
use crate::combat::missile::ShooterId;
use crate::map::Distance;
use crate::map::Radii;
use crate::map::spatial::Placed;
use crate::map::{GroundOffset, Offset, Position};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::math::rng::Stream;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::actions::ActionList;
use crate::objects::lifecycle::static_storage;
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::VolleySpread;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::{Master, player_master};
use crate::objects::release::Releaser;
use crate::objects::sprite::ActiveSprite;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::PlayerId;
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::vision::visibility::{Seer, capture_kind};
use crate::world::ARMED_FIREPOWER;
use crate::world::Seconds;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
    /// The unit's own part of its hit, written back: what a retaliation or reflected damage left
    /// of its hit points and powerups, its charge and `+0x8a8`, the damage notify (500) a
    /// retaliation posts to its unit AI; a retaliation that killed it is the caller's (Gaia
    /// taking a killed attacker is not modelled).
    pub(crate) fn hit_back(
        &mut self,
        own: Victim,
    ) -> Result<Option<crate::combat::damage::Kill>, GenieError> {
        self.life.hp = own.hp;
        if let Some(a) = own.armored.as_ref() {
            self.combat.hp_powerup = a.hp_powerup;
        }
        self.combat.charge = own.source.charge;
        self.combat.charge_hit = own.source.charge_hit;
        if let Some(ai) = self.combat.ai.as_mut() {
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
    pub(crate) fn volley(&mut self, c: &mut Step<'_>, aimed: Aimed) -> Result<(), GenieError> {
        let master = object_master(
            self.own_master.as_deref(),
            (self.id, self.life.master_id),
            c.w.masters.of(c.player),
        )?;
        if (matches!(
            master.id,
            ids::ID_527 | ids::ID_528 | ids::ID_1104 | ids::ID_2133 | ids::ID_2134 | ids::ID_2135
        ) || matches!(master.class, class::CLASS_35 | class::CLASS_64))
            && self.life.state == ObjectState::Alive
        {
            return Err(GenieError::unsupported(
                "a self-destructing attacker's hit (its object 112 and its death)",
            ));
        }
        if master.charge_type.ranged() {
            return Err(GenieError::unsupported(
                "a charged hit (slot 0x2f8 with charge types 6 and 7)",
            ));
        }
        let missile = master.missile.master().is_some();
        // what the rest reads of the master, before the notify changes the unit
        let notifies = matches!(master.id, ids::ID_73 | ids::ID_559);
        let (
            trebuchet,
            VolleySpread {
                x: x_spread,
                y: y_spread,
                ..
            },
        ) = (master.class == class::CLASS_54, master.volley_spread);
        let (first, extra) = if master.max_volley == MissileCount::ZERO {
            (true, 0)
        } else {
            let count =
                crate::combat::missile::volley_count(self.combat.firepower, master.max_volley);
            let old = self.combat.next_volley;
            let mut n = count - old.counted();
            let VolleySpread {
                x: x_spread,
                y: y_spread,
                ..
            } = master.volley_spread;
            let mut all = true;
            if !master.combat_ability.contains(CombatAbility::FULL_VOLLEY) {
                let k = if Distance::ZERO < x_spread || Distance::ZERO < y_spread {
                    let draw = c.w.streams.get(Stream::Object).rand15() as i32;
                    (draw * 2) / 0x7fff + 1
                } else {
                    1
                };
                if k < n {
                    self.combat.next_volley = self.combat.next_volley.plus(k as u8);
                    // an immediate 0.05
                    self.combat.attack_timer = Seconds::new(0.05);
                    n = k;
                    all = false;
                }
            }
            if all {
                self.combat.next_volley = MissileCount::ZERO;
                if notifies {
                    self.post_own(NotifyKind::ACTION_EXECUTED, [600, -1, 0])?;
                }
            }
            (
                old == MissileCount::ZERO,
                if old == MissileCount::ZERO { n - 1 } else { n },
            )
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
                let scatter = GroundOffset::new(
                    crate::combat::missile::scatter(x, x_spread),
                    crate::combat::missile::scatter(y, y_spread),
                );
                self.volley_missile(c, &aimed, scatter)?;
            }
        }
        if (extra != 0 || missile)
            && c.w
                .players
                .get(aimed.player.index())
                .is_some_and(|q| q.computer)
        {
            return Err(GenieError::unsupported(
                "a hit on a computer player's object (its notify 0x214)",
            ));
        }
        if first && missile {
            self.fire(c, &aimed)?;
        }
        Ok(())
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
    pub(crate) fn fire(&mut self, c: &mut Step<'_>, aimed: &Aimed) -> Result<(), GenieError> {
        self.combat.attack_count = self.combat.attack_count.next();
        let master = self.master(&c.w.masters, c.player)?;
        if master.charge_type.ranged() {
            return Err(GenieError::unsupported(
                "a charge projectile (charge types 6 and 7, slot 0x2f8)",
            ));
        }
        let facing = self
            .body
            .mover()
            .map(|m| m.angle)
            .ok_or(GenieError::unsupported("a ranged unit that does not move"))?;
        let shot = Shot {
            made: master.missile.master().ok_or(GenieError::missing_data(
                "a ranged unit whose master has no missile",
            ))?,
            at: crate::combat::missile::spawn_point(self.position(), facing, master.weapon_offset),
            point: aimed.position,
            reach: crate::combat::missile::reach(master.radius, master.weapon_range),
            shooter: true,
            aimer: crate::combat::missile::Aimer {
                hit_chance: master.hit_chance,
                spread: master.missed_spread,
                class: master.class,
            },
            attack_count: self.combat.attack_count,
        };
        let (weapon_range, blast_level) = (master.weapon_range, master.blast_level);
        self.launch(c, aimed, &shot)?;
        if !matches!(self.life.master_id, ids::ID_73 | ids::ID_559) {
            self.post_own(
                NotifyKind::ACTION_EXECUTED,
                [600, ObjectId::field(aimed.target), 0],
            )?;
        }
        let spares_friends = blast_level.spares_friends(weapon_range);
        if !spares_friends
            && c.w
                .players
                .get(aimed.player.index())
                .is_some_and(|q| q.computer)
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
    ) -> Result<(), GenieError> {
        let masters =
            c.w.masters
                .get(c.player.index())
                .ok_or(GenieError::unsupported("a volley without the masters"))?;
        let master = self.master_in(Some(masters))?;
        let facing = self
            .body
            .mover()
            .map(|m| m.angle)
            .ok_or(GenieError::unsupported("a ranged unit that does not move"))?;
        let building = master.class == class::BUILDING;
        let at = crate::combat::missile::volley_spawn_point(
            self.position(),
            facing,
            master.weapon_offset,
            master.volley_spread.start,
            scatter,
            building.then_some(aimed.position.ground()),
        );
        let volley = master.volley_missile.and_then(|id| masters.get(id));
        let made = match volley {
            Some(v) => v,
            None if master.missile.master().is_some() => {
                master.missile.master().and_then(|m| masters.get(m)).ok_or(
                    GenieError::unsupported("a missile its player's masters do not have"),
                )?
            }
            None => {
                return Err(GenieError::unsupported(
                    "a volley's extra missile of a master without a missile",
                ));
            }
        };
        if matches!(made.id, ids::ID_527 | ids::ID_528) {
            return Err(GenieError::unsupported(
                "a volley's extra missile of master 527 or 528 (its attack count)",
            ));
        }
        let shooter = volley.is_none();
        let aimer = if shooter { master } else { made };
        let shot = Shot {
            made: made.id,
            at,
            point: crate::combat::missile::spawn_point(
                aimed.position,
                facing,
                scatter.with_height(0.0),
            ),
            reach: crate::combat::missile::volley_reach(master.radius, master.weapon_range),
            shooter,
            aimer: crate::combat::missile::Aimer {
                hit_chance: aimer.hit_chance,
                spread: aimer.missed_spread,
                class: aimer.class,
            },
            attack_count: crate::combat::AttackCount::ZERO,
        };
        self.launch(c, aimed, &shot)?;
        if !matches!(self.life.master_id, ids::ID_73 | ids::ID_559) {
            self.post_own(NotifyKind::ACTION_EXECUTED, [600, -1, 0])?;
        }
        Ok(())
    }

    /// A missile's making and start, which [`Unit::fire`] and [`Unit::volley_missile`] share.
    /// The missile master comes from the player's master's slot 0x40 (`FUN_1410d5920`): from the
    /// pool of type 60 when it holds one (keeping its id), else new with the next reusable id
    /// ([`Recycling`]). The setups draw as a new object's do (its standing sprite,
    /// [`ActiveSprite::start`] on the Sprite stream; its facet for a sprite with a random first
    /// frame and several facets, then its angle for a turning one: Object stream) and put it on
    /// the map (slot 0x58: held inside it; a pooled missile that last flew no farther than 0.1
    /// takes the ground by its fly mode); it joins the end of its owner's list. Then its action
    /// (`FUN_140a9a040`): the shot's point, the master's hit mode, the target taken (slot 0x178)
    /// and, with the shooter, the shooter taken too and its place, master and owner with the
    /// missile's radii as its origin (else none); the list's add starts it (`FUN_140a9c5b0`):
    /// `set_state` 3 aims ([`crate::combat::missile::aim`], Action stream) and launches
    /// ([`crate::combat::missile::MissileAction::launch`], its angle). The missile keeps the object's id
    /// as what it was fired from, its reach and the object's attack count. Not held: the
    /// missile's sprite and pitch facet, and a shooter's own master copy (its origin's master
    /// −1).
    pub(crate) fn launch(
        &mut self,
        c: &mut Step<'_>,
        aimed: &Aimed,
        shot: &Shot,
    ) -> Result<(), GenieError> {
        let made = player_master(&c.w.masters, c.player, shot.made).ok_or(
            GenieError::unsupported("a missile its player's masters do not have"),
        )?;
        let stats = made.missile_stats.ok_or(GenieError::unsupported(
            "a missile master without its flight",
        ))?;
        if !made.recyclable {
            return Err(GenieError::unsupported(
                "a missile of a master that is not recyclable",
            ));
        }
        let moving = made.moving.ok_or(GenieError::unsupported(
            "a missile master that does not move",
        ))?;
        let (id, flying) = if let Some(pooled) = c.w.recycling.missiles.pop() {
            pooled
        } else {
            let id = c.w.recycling.next_id;
            c.w.recycling.next_id = id.predecessor();
            (id, true)
        };
        // the setups' draws: static (its sprite, its facet), then moving (its angle)
        let standing = made
            .standing_sprite
            .and_then(|s| c.w.graphics.get(s).copied());
        if let Some(g) = standing {
            let _ = ActiveSprite::start(g, c.w.streams.get(Stream::Sprite));
            if g.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME) && g.facets.several() {
                let _ = c.w.streams.get(Stream::Object).rand15();
            }
        }
        let facets = standing.map(|g| crate::objects::facing::SpriteFacets {
            count: g.facets,
            turns: g.flags.contains(SpriteFlags::TURNS),
        });
        let mut angle = Angle::ZERO;
        if let Some(f) = facets.filter(|_| crate::objects::facing::draws_angle(facets)) {
            let r = c.w.streams.get(Stream::Object).rand15();
            angle = crate::objects::facing::drawn_angle(r, f);
        }
        let mut mover = Mover {
            animated: crate::objects::layers::Animated::default(),
            forward: BVector::new(1.0, 0.0, 0.0),
            right: BVector::new(0.0, 0.0, crate::production::spawn::START_RIGHT_Z),
            velocity: None,
            paths: Vec::new(),
            factor: 1.0,
            stats: moving,
            waiting: 0,
            wait_delays: crate::movement::WaitDelays::NONE,
            attack_moving: false,
            last_move: c.w.world_time,
            moved_at: Position::ORIGIN,
            substitute_count: crate::movement::SubstituteCount::NONE,
            ignore_units: Vec::new(),
            user_waypoints: Vec::new(),
            last_turn: Seconds::ZERO,
            angle,
            pauses: Vec::new(),
            in_the_way: Vec::new(),
            sprite_facets: facets,
        };
        let mut position = mover.set_position(shot.at, c.w.spatial.map);
        if !flying {
            let ground =
                c.w.spatial
                    .terrain
                    .as_ref()
                    .ok_or(GenieError::unsupported(
                        "a missile without the map's terrain",
                    ))?
                    .ground(position.ground())?;
            position = position.with_z(crate::combat::missile::grounded(
                position.z(),
                ground,
                false,
                stats.fly_mode,
            ));
        }
        mover.mark_move(position, c.w.world_time);
        let solid = 0.0 < made.radius.x() && 0.0 < made.radius.y() && made.tall;
        let placed = Placed {
            position,
            radius: made.radius,
            class: made.class,
            player: c.player,
            master: made.id,
            obstruction_type: made.obstruction_type,
            buildable_on: made.buildable_on,
            tall: made.tall,
            moves: made.moves(),
            unknown: None,
            obstruction: (made.obstruction_type != ObstructionType::NONE && solid).then_some(
                crate::map::spatial::Obstruction {
                    kind: made.obstruction_type,
                    class: made.obstruction_class,
                },
            ),
        };
        c.w.spatial.insert(id, placed);
        c.w.pathing.moved(id, &placed, false);
        if let Some(v) = c.w.spatial.sight.as_mut() {
            v.add_seer(
                id,
                Seer {
                    player: c.player,
                    master: made.id,
                    los: made.line_of_sight,
                    state: ObjectState::Alive,
                    explored: false,
                    building: None,
                    umv: 0,
                    capture: capture_kind(c.player, made.class, made.id),
                    kind: made.kind,
                },
            );
            let _ = v.sight_on(id, position, made.radius, false, false)?;
        }
        let held = static_storage(c.w.players.player_mut(c.player)?, &made.storage)?;
        // its action: the target and the shooter taken (slot 0x178), then the start
        let mut action = crate::combat::missile::MissileAction {
            state: crate::combat::missile::MissileState::New,
            target: aimed.target,
            fired_from: shot.shooter.then_some(ShooterId::new(self.id)),
            point: shot.point,
            timer: Distance::ZERO,
            direction: Offset::ZERO,
            ballistic_velocity: Speed::ZERO,
            ballistic_acceleration: crate::movement::Acceleration::ZERO,
            distance: Distance::ZERO,
            hit_mode: stats.hit_mode,
            result: crate::combat::missile::StrikeResult::NONE,
            origin: if shot.shooter {
                crate::combat::missile::Origin {
                    position: self.position(),
                    radius: made.radius,
                    master: Some(self.life.master_id),
                    owner: Some(c.player),
                }
            } else {
                crate::combat::missile::Origin::NONE
            },
            met: Vec::new(),
        };
        let point = {
            let draws = c.w.streams.get(Stream::Action);
            crate::combat::missile::aim(
                AimAt {
                    point: action.point,
                    from: position,
                    target: Some((aimed.radius, aimed.class)),
                    aimer: shot.aimer,
                    master_id: made.id,
                    attack_count: shot.attack_count,
                    hit_mode: &mut action.hit_mode,
                },
                &mut || draws.rand15(),
            )?
        };
        if let Some(a) = action.launch(position, point, &stats)? {
            mover.angle = a;
        }
        let missile = Self {
            id,
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Alive,
                hp: HitPoints::of_master(made.hit_points),
                attribute_type: held.kind,
                amount: held.amount,
                dying_sprite: made.death_sprite,
                death_spawn: made.death_spawns[0].is_some(),
                death_facets: None,
                inside: None,
                goto_sleep: false,
                master_id: made.id,
                master: made.life,
            },
            body: crate::objects::layers::Body::new(position, None, Some(mover)),
            // its +0x900, the firing object's attack count when fired
            combat: crate::objects::layers::Combat {
                attack_count: self.combat.attack_count,
                ..Default::default()
            },
            actions: crate::objects::layers::ActionLayer {
                list: ActionList::default().with_missile(Some(action)),
                ..Default::default()
            },
            attackable: true,
            pasture: None,
            my_pasture: None,
            ..Self::default()
        };
        // it joins the end of its owner's list (slot 0x2d0, which files it in the tracker);
        // then its action takes the target and the shooter
        c.w.players.player_mut(c.player)?.tracker.add(
            id,
            crate::players::tracker::key(c.w.masters.of(c.player), &c.w.lines, made.id),
        );
        let class = made.class;
        let own = crate::combat::missile::Missile {
            max_range: shot.reach,
            fired_from: Some(ShooterId::new(self.id)),
            charge: false,
            flying,
            step: Offset::ZERO,
        };
        let missile = crate::world::parts::Object {
            unit: missile,
            parts: crate::world::parts::ObjectParts {
                missile: Some(own),
                ..Default::default()
            },
        };
        let _ = c.w.push_live(c.player, missile)?;
        let missile = Releaser {
            id,
            class: Some(class),
            player: c.player,
        };
        crate::world::lets::take(c, self, aimed.target, missile)?;
        if shot.shooter {
            // the shooter, of the missile's own player: never listed
            self.taken(false, id);
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
pub(crate) fn armed(master: &Master, firepower: crate::combat::Firepower) -> bool {
    master.kind != BUILDING
        || matches!(master.class, class::CLASS_51 | class::CLASS_54)
        || master.missile.master().is_some()
        || ARMED_FIREPOWER < firepower
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
    pub(crate) aimer: crate::combat::missile::Aimer,
    pub(crate) attack_count: crate::combat::AttackCount,
}
