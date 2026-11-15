//! A unit's missile launch: the missile's making and start, which its fire and its volley share.

use crate::objects::layers::Speedup;

use crate::combat::missile::AimAt;
use crate::combat::missile::MISSILE_TERRAIN_LACKING;
use crate::combat::missile::ShooterId;
use crate::combat::missile::{
    Missile, MissileAction, MissileStart, MissileState, MissileStats, MissileTargetId, Origin,
    StrikeResult, aim, grounded,
};
use crate::combat::volley::Aimed;
use crate::combat::volley::Shot;
use crate::map::Distance;
use crate::map::spatial::Obstruction;
use crate::map::spatial::Placed;
use crate::map::{Offset, Position};
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::math::rng::{Stream, Well512};
use crate::movement::MoveState;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::{Acceleration, SubstituteCount, WaitDelays};
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::SpriteFlags;
use crate::objects::actions::ActionList;
use crate::objects::facing::{SpriteFacets, drawn_angle, draws_angle};
use crate::objects::layers::{Action, Animated, Combat, Motion, Static};
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::release::Releaser;
use crate::objects::sprite::ActiveSprite;
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::players::tracker::key;
use crate::vision::SightCentre;
use crate::vision::SightLayer;
use crate::vision::SightShape;
use crate::vision::visibility::{Seer, capture_kind};
use crate::world::Seconds;
use crate::world::layer::Roles;
use crate::world::layer::generated::MissileTuning;
use crate::world::lets::take;
use crate::world::parts::{Object, ObjectParts};
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};
use std::sync::Arc;

/// A shot whose missile master its player lacks: [`Unit::fire`] and [`Unit::volley_missile`]
/// find the master in the player's table before they launch it.
const SHOT_MASTER_LACKING: GenieError =
    GenieError::unreachable("a shot whose missile master its player lacks");

impl Unit {
    /// A missile's making and start, which [`Unit::fire`] and [`Unit::volley_missile`] share. The
    /// missile master comes from the player's master's slot 0x40 (`FUN_1410d5920`): from the pool
    /// of type 60 when it holds one (keeping its id), else new with the next reusable id
    /// ([`World::recycling`](crate::world::World::recycling)). The setups draw as a new object's do
    /// (its standing sprite, [`ActiveSprite::start`] on the Sprite stream; its facet for a sprite
    /// with a random first frame and several facets, then its angle for a turning one: Object
    /// stream) and put it on the map (slot 0x58: held inside it; a pooled missile that last flew no
    /// farther than 0.1 takes the ground by its fly mode); it joins the end of its owner's list.
    /// Then its action (`FUN_140a9a040`): the shot's point, the master's hit mode, the target taken
    /// (slot 0x178) and, with the shooter, the shooter taken too and its place, master and owner
    /// with the missile's radii as its origin (else none); the list's add starts it
    /// ([`MissileAction::start`], `0x140a9c5b0`): a shot without a target (a fire at a point's,
    /// `FUN_141080650`, which genie does not port) toward a point whose x or y is −1 ends at once
    /// ([`Unit::end_missile`]: no draw, its hit points 0); any other aims and launches, which
    /// gives the missile its angle ([`Unit::start_new_missile`]). The missile keeps
    /// the object's id as what it was fired from, its reach and the object's attack count. Not
    /// held: the missile's sprite and pitch facet, and a shooter's own master copy (its origin's
    /// master −1).
    #[expect(
        clippy::too_many_lines,
        reason = "a missile's making and start (slot 0x40, `FUN_1410d5920`) ported as one function, its setups and draws in the game's order"
    )]
    pub(crate) fn launch(
        &mut self,
        step: &mut Step<'_>,
        aimed: &Aimed,
        shot: &Shot,
        roles: &Roles,
    ) -> GenieResult<()> {
        // the player's masters table (an `Arc`: no table is copied) held beside the world, whose
        // slot 0xf8 the missile's storage calls while its master is read
        let table = step.world.model.masters.shared(step.player).map(Arc::clone);
        let made = table
            .as_deref()
            .and_then(|masters| masters.get(shot.made))
            .ok_or(SHOT_MASTER_LACKING)?;
        let stats = made.missile_stats().ok_or(GenieError::unsupported(
            "a missile master without its flight",
        ))?;
        if !made.recyclable.recycled() {
            return Err(GenieError::unsupported(
                "a missile of a master that is not recyclable",
            ));
        }
        let moving = made.move_stats.ok_or(GenieError::unsupported(
            "a missile master that does not move",
        ))?;
        let (id, flying) = if let Some(pooled) = step.world.recycling.missiles.pop() {
            pooled
        } else {
            let id = step.world.recycling.next_id;
            step.world.recycling.next_id = id.predecessor();
            (id, true)
        };
        // the setups' draws: static (its sprite, its facet), then moving (its angle)
        let standing = made
            .standing_sprite
            .and_then(|sprite_id| step.world.model.graphics.get(sprite_id).copied());
        if let Some(graphic) = standing {
            let _ = ActiveSprite::start(
                graphic,
                step.world.streams.get_mut(Stream::Sprite),
                step.world.model.tuning.sprite,
            );
            if graphic.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME) && graphic.facets.several() {
                let _ = step.world.streams.get_mut(Stream::Object).rand15();
            }
        }
        let facets = standing.map(|graphic| SpriteFacets {
            count: graphic.facets,
            turns: graphic.flags.contains(SpriteFlags::TURNS),
        });
        let mut angle = Angle::ZERO;
        if let Some(sprite_facets) = facets.filter(|_| draws_angle(facets)) {
            let draw = step.world.streams.get_mut(Stream::Object).rand15();
            angle = drawn_angle(draw, sprite_facets);
        }
        let mut mover = Mover {
            animated: Animated::default(),
            forward: BVector::new(1.0, 0.0, 0.0),
            right: BVector::new(0.0, 0.0, step.world.model.tuning.spawn.start_right_z),
            velocity: None,
            paths: Vec::new(),
            factor: 1.0,
            stats: moving,
            waiting: 0,
            wait_delays: WaitDelays::NONE,
            attack_moving: false,
            last_move: step.world.world_time,
            moved_at: Position::ORIGIN,
            substitute_count: SubstituteCount::NONE,
            speedup: Speedup::NONE,
            ignore_units: Vec::new(),
            user_waypoints: Vec::new(),
            last_turn: Seconds::ZERO,
            angle,
            pauses: Vec::new(),
            in_the_way: Vec::new(),
            sprite_facets: facets,
        };
        // a missile's slot 0x208 is RGE's (FUN_14114d1d0)
        let top = mover.own_speed();
        let mut position = mover.set_position(
            shot.at,
            top,
            step.world.spatial.map,
            &step.world.model.tuning.movement,
        );
        if !flying {
            let ground = step
                .world
                .spatial
                .terrain
                .as_ref()
                .ok_or(MISSILE_TERRAIN_LACKING)?
                .ground(position.ground())?;
            position = position.with_z(grounded(position.z(), ground, stats.fly_mode));
        }
        mover.mark_move(position, step.world.world_time);
        let solid = 0.0 < made.radius.x() && 0.0 < made.radius.y() && made.is_tall();
        let placed = Placed {
            position,
            radius: made.radius,
            class: made.class,
            player: step.player,
            master: made.id,
            obstruction_kind: made.obstruction_kind,
            buildable_on: made.buildable_on,
            tall: made.is_tall(),
            moves: made.moves(),
            unknown: None,
            obstruction: (made.obstruction_kind != ObstructionKind::NONE && solid).then_some(
                Obstruction {
                    kind: made.obstruction_kind,
                    class: made.obstruction_class,
                },
            ),
        };
        step.world.spatial.insert(id, placed);
        step.world.pathing.moved(id, &placed, MoveState::Standing);
        if let Some(visibility) = step.world.spatial.sight.as_mut() {
            visibility.add_seer(
                id,
                Seer {
                    player: step.player,
                    master: made.id,
                    los: made.line_of_sight,
                    los_powerup: Distance::ZERO,
                    state: ObjectState::Alive,
                    explored: false,
                    building: None,
                    umv: 0,
                    capture: capture_kind(step.player, made.class, made.id, roles),
                    kind: made.kind,
                },
            );
            let _ = visibility.sight_on(
                id,
                position,
                made.radius,
                SightShape::Circle,
                SightCentre::Tile,
                SightLayer {
                    roles,
                    visibility_tuning: step.world.model.tuning.visibility,
                },
            )?;
        }
        let held = step.world.static_storage(step.player, &made.storage)?;
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
            result: StrikeResult::None,
            origin: if shot.shooter {
                Origin {
                    position: self.position(),
                    radius: made.radius,
                    master: Some(self.life.master_id),
                    owner: Some(step.player),
                }
            } else {
                Origin::NONE
            },
            met: Vec::new(),
        };
        let mut missile = Self {
            id,
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Alive,
                hp: HitPoints::of_master(made.hit_points),
                attribute_kind: held.kind,
                amount: held.amount,
                death_facets: None,
                inside: None,
                goto_sleep: false,
                master_id: made.id,
            },
            place: Static::at(position),
            flags: UnitFlags::ATTACKABLE,
            pasture: None,
            my_pasture: None,
            ..Self::default()
        };
        let new_start = NewMissileStart {
            from: position,
            aimed,
            shot,
            stats: &stats,
        };
        if let Some(angle) = missile.start_new_missile(
            &mut action,
            &new_start,
            step.world.streams.get_mut(Stream::Action),
            roles,
            &step.world.model.tuning.missile,
        )? {
            mover.angle = angle;
        }
        // it joins the end of its owner's list (slot 0x2d0, which files it in the tracker);
        // then its action takes the target and the shooter
        step.world.players.player_mut(step.player)?.tracker.add(
            id,
            key(
                step.world.model.masters.of(step.player),
                &step.world.model.lines,
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
                    attack_count: step.world.parts.combat_of(self.id).attack_count,
                    ..Default::default()
                },
                ..Default::default()
            },
        };
        let _ = step.world.push_live(step.player, missile)?;
        let missile = Releaser {
            id,
            class: Some(class),
            player: step.player,
        };
        take(step, self, aimed.target, missile, roles)?;
        if shot.shooter {
            // the shooter, of the missile's own player: never listed
            self.taken(None, id);
        }
        Ok(())
    }
}

/// What a new missile's start reads besides its action and the Action stream
/// ([`Unit::start_new_missile`]): its place, the shot's aim at its target, the shot and its
/// master's flight.
struct NewMissileStart<'shot> {
    /// The new missile's place.
    from: Position,
    /// The aim at the shot's target: its radii and class, which the aim reads with a target.
    aimed: &'shot Aimed,
    /// The shot: its missile master, its aimer (the firing object's hit chance, spread and
    /// class) and its attack count.
    shot: &'shot Shot,
    /// The missile master's flight.
    stats: &'shot MissileStats,
}

impl Unit {
    /// The list's add of the new missile `self`'s `action`: its slot 5 with 1
    /// ([`MissileAction::start`]) and the `set_state` it chooses (`FUN_140a9de70`). Case 1 ends
    /// it at once ([`Unit::end_missile`]). Case 3 aims from its place (`FUN_140a9c600`, [`aim`]:
    /// the Action stream's draws; a target's height raises the point only with a target,
    /// `+0x20`) and launches ([`MissileAction::launch`]), which gives the missile its angle.
    ///
    /// # Errors
    ///
    /// The aim's and the launch's.
    fn start_new_missile(
        &mut self,
        action: &mut MissileAction,
        new_start: &NewMissileStart<'_>,
        draws: &mut Well512,
        roles: &Roles,
        missile_tuning: &MissileTuning,
    ) -> GenieResult<Option<Angle>> {
        match action.start() {
            MissileStart::Ends => {
                self.end_missile(action);
                Ok(None)
            }
            MissileStart::Launches => {
                let point = aim(
                    AimAt {
                        point: action.point,
                        from: new_start.from,
                        target: action
                            .target
                            .map(|_| (new_start.aimed.radius, new_start.aimed.class)),
                        aimer: new_start.shot.aimer,
                        master_id: new_start.shot.made,
                        attack_count: new_start.shot.attack_count,
                        hit_mode: &mut action.hit_mode,
                    },
                    &mut || draws.rand15(),
                    roles,
                    missile_tuning,
                )?;
                action.launch(new_start.from, point, new_start.stats)
            }
        }
    }
}
