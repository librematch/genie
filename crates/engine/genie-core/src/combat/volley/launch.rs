//! A unit's missile launch: the missile's making and start, which its fire and its volley share.

use crate::GenieError;
use crate::combat::missile::AimAt;
use crate::combat::missile::ShooterId;
use crate::combat::missile::{
    Missile, MissileAction, MissileState, MissileTargetId, Origin, StrikeResult, aim, grounded,
};
use crate::combat::volley::Aimed;
use crate::combat::volley::Shot;
use crate::map::Distance;
use crate::map::spatial::Obstruction;
use crate::map::spatial::Placed;
use crate::map::{Offset, Position};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::math::rng::Stream;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::{Acceleration, SubstituteCount, WaitDelays};
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::actions::ActionList;
use crate::objects::facing::{SpriteFacets, drawn_angle, draws_angle};
use crate::objects::layers::{Action, Animated, Combat, Motion, Static};
use crate::objects::lifecycle::static_storage;
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::player_master;
use crate::objects::release::Releaser;
use crate::objects::sprite::ActiveSprite;
use crate::objects::unit::Unit;
use crate::players::tracker::key;
use crate::production::spawn::START_RIGHT_Z;
use crate::vision::SightCentre;
use crate::vision::SightShape;
use crate::vision::visibility::{Seer, capture_kind};
use crate::world::Seconds;
use crate::world::layer::Roles;
use crate::world::lets::take;
use crate::world::parts::{Object, ObjectParts};
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
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
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let made = player_master(&c.w.model.masters, c.player, shot.made).ok_or(
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
            .and_then(|s| c.w.model.graphics.get(s).copied());
        if let Some(g) = standing {
            let _ = ActiveSprite::start(g, c.w.streams.get(Stream::Sprite));
            if g.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME) && g.facets.several() {
                let _ = c.w.streams.get(Stream::Object).rand15();
            }
        }
        let facets = standing.map(|g| SpriteFacets {
            count: g.facets,
            turns: g.flags.contains(SpriteFlags::TURNS),
        });
        let mut angle = Angle::ZERO;
        if let Some(f) = facets.filter(|_| draws_angle(facets)) {
            let r = c.w.streams.get(Stream::Object).rand15();
            angle = drawn_angle(r, f);
        }
        let mut mover = Mover {
            animated: Animated::default(),
            forward: BVector::new(1.0, 0.0, 0.0),
            right: BVector::new(0.0, 0.0, START_RIGHT_Z),
            velocity: None,
            paths: Vec::new(),
            factor: 1.0,
            stats: moving,
            waiting: 0,
            wait_delays: WaitDelays::NONE,
            attack_moving: false,
            last_move: c.w.world_time,
            moved_at: Position::ORIGIN,
            substitute_count: SubstituteCount::NONE,
            ignore_units: Vec::new(),
            user_waypoints: Vec::new(),
            last_turn: Seconds::ZERO,
            angle,
            pauses: Vec::new(),
            in_the_way: Vec::new(),
            sprite_facets: facets,
        };
        let mut position = mover.set_position(shot.at, c.w.spatial.map, &c.w.model.tuning.movement);
        if !flying {
            let ground =
                c.w.spatial
                    .terrain
                    .as_ref()
                    .ok_or(GenieError::unsupported(
                        "a missile without the map's terrain",
                    ))?
                    .ground(position.ground())?;
            position = position.with_z(grounded(position.z(), ground, false, stats.fly_mode));
        }
        mover.mark_move(position, c.w.world_time);
        let solid = 0.0 < made.radius.x() && 0.0 < made.radius.y() && made.tall;
        let placed = Placed {
            position,
            radius: made.radius,
            class: made.class,
            player: c.player,
            master: made.id,
            obstruction_kind: made.obstruction_kind,
            buildable_on: made.buildable_on,
            tall: made.tall,
            moves: made.moves(),
            unknown: None,
            obstruction: (made.obstruction_kind != ObstructionKind::NONE && solid).then_some(
                Obstruction {
                    kind: made.obstruction_kind,
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
                    capture: capture_kind(c.player, made.class, made.id, roles),
                    kind: made.kind,
                },
            );
            let _ = v.sight_on(
                id,
                position,
                made.radius,
                SightShape::Circle,
                SightCentre::Tile,
                roles,
            )?;
        }
        let held = static_storage(c.w.players.player_mut(c.player)?, &made.storage)?;
        // its action: the target and the shooter taken (slot 0x178), then the start
        let mut action = MissileAction {
            state: MissileState::New,
            // the volley gives the missile its aim as its target
            target: aimed.target.map(MissileTargetId::new),
            fired_from: shot.shooter.then_some(ShooterId::new(self.id)),
            point: shot.point,
            timer: Distance::ZERO,
            direction: Offset::ZERO,
            ballistic_velocity: Speed::ZERO,
            ballistic_acceleration: Acceleration::ZERO,
            distance: Distance::ZERO,
            hit_mode: stats.hit_mode,
            result: StrikeResult::NONE,
            origin: if shot.shooter {
                Origin {
                    position: self.position(),
                    radius: made.radius,
                    master: Some(self.life.master_id),
                    owner: Some(c.player),
                }
            } else {
                Origin::NONE
            },
            met: Vec::new(),
        };
        let point = {
            let draws = c.w.streams.get(Stream::Action);
            aim(
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
                roles,
                &c.w.model.tuning.missile,
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
            place: Static::at(position),
            attackable: true,
            pasture: None,
            my_pasture: None,
            ..Self::default()
        };
        // it joins the end of its owner's list (slot 0x2d0, which files it in the tracker);
        // then its action takes the target and the shooter
        c.w.players.player_mut(c.player)?.tracker.add(
            id,
            key(
                c.w.model.masters.of(c.player),
                &c.w.model.lines,
                made.id,
                roles,
            ),
        );
        let class = made.class;
        let own = Missile {
            max_range: shot.reach,
            fired_from: Some(ShooterId::new(self.id)),
            charge: false,
            flying,
            step: Offset::ZERO,
        };
        let missile = Object {
            unit: missile,
            parts: ObjectParts {
                actions: Action {
                    list: ActionList::default().with_missile(Some(action)),
                    ..Default::default()
                },
                missile: Some(own),
                motion: Some(Motion::Moving(mover)),
                // its +0x900, the firing object's attack count when fired
                combat: Combat {
                    attack_count: c.w.parts.combat_of(self.id).attack_count,
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let _ = c.w.push_live(c.player, missile)?;
        let missile = Releaser {
            id,
            class: Some(class),
            player: c.player,
        };
        take(c, self, aimed.target, missile, roles)?;
        if shot.shooter {
            // the shooter, of the missile's own player: never listed
            self.taken(None, id);
        }
        Ok(())
    }
}
