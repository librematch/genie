use super::*;
use crate::combat::damage::Allied;
use crate::construction::MainBuildingId;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Radii;
use crate::map::RestrictionId;
use crate::map::TerrainMask;
use crate::math::Angle;
use crate::math::fmath::BVector;
use crate::math::rng::Stream;
use crate::movement::Me;
use crate::movement::MoveAlgorithm;
use crate::movement::TurnRate;
use crate::movement::move_to::MoveWaitState;
use crate::movement::move_to::{GiveUp, MoveDefaults, MoveTo, MoveToReads, MoveWait, SpriteReads};
use crate::objects::ActionState;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::Relics;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::UpdateCode;
use crate::objects::layers::Animated;
use crate::objects::masters::BlastDefense;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::ClassId;
use crate::objects::masters::FlyMode;
use crate::objects::masters::MasterField;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::unit_ai::ActionKind;
use crate::unit_ai::BOAR_AI;
use crate::unit_ai::LION_AI;
use crate::unit_ai::NotifyKind;
use crate::world::Millis;
use crate::world::Player;
use crate::world::step::Step;

/// `FUN_140a82ae0` case 6 after the hit: a ranged unit whose attack timer ran out is done
/// without a target and ready again past its interrupt frame (its firing frame without one);
/// at or past the interrupt frame readyToAttack runs, the swing going on.
#[test]
fn a_swing_after_its_hit_follows_its_interrupt_and_firing_frames() {
    use AfterHit::{Continue, Done, Ready};
    let f = crate::objects::Frame::of_data;
    // ranged, timer run out (NaN counts too)
    assert_eq!(
        after_hit(true, Seconds::ZERO, false, f(5), f(3), f(9)),
        Done
    );
    assert_eq!(
        after_hit(true, Seconds::ZERO, true, f(5), f(3), f(6)),
        Ready
    );
    assert_eq!(
        after_hit(true, Seconds::new(f32::NAN), true, f(5), f(3), f(6)),
        Ready
    );
    assert_eq!(
        after_hit(true, Seconds::new(-1.0), true, f(-1), f(3), f(4)),
        Ready
    );
    assert_eq!(
        after_hit(true, Seconds::ZERO, true, f(-1), f(3), f(3)),
        Continue { check: false }
    );
    assert_eq!(
        after_hit(true, Seconds::ZERO, true, f(5), f(3), f(5)),
        Continue { check: true }
    );
    // ranged with time left, or melee: only the interrupt test
    assert_eq!(
        after_hit(true, Seconds::new(0.5), false, f(5), f(3), f(9)),
        Continue { check: true }
    );
    assert_eq!(
        after_hit(false, Seconds::ZERO, false, f(5), f(3), f(4)),
        Continue { check: false }
    );
    assert_eq!(
        after_hit(false, Seconds::ZERO, true, f(5), f(3), f(5)),
        Continue { check: true }
    );
    assert_eq!(
        after_hit(false, Seconds::ZERO, true, f(-1), f(3), f(9)),
        Continue { check: false }
    );
}

/// The sprites' table of a world without sprites.
pub(crate) static NO_GRAPHICS: Graphics = Graphics::new();

/// What a new `MoveTo` reads of a unit in object state `state` with top speed `speed`: the
/// sprites' table `graphics` with the defaults `defaults`.
fn walking_reads(
    graphics: &Graphics,
    defaults: MoveDefaults,
    state: ObjectState,
    speed: f32,
) -> MoveToReads<'_> {
    MoveToReads {
        sprites: SpriteReads { defaults, graphics },
        state,
        speed: Speed::new(speed),
        at: Position::ORIGIN,
        ground: Footprint::ZERO,
    }
}

/// A quiet `MoveTo` sub-action in `state`.
fn walking(state: MoveTo) -> MoveToAction {
    MoveToAction::sub_in(state)
}

/// A unit's own Attack on `target` in `state` with range `range`.
pub(crate) fn own_attack(target: i32, state: AttackState, range: Distance) -> Attack {
    Attack {
        target: ObjectId::of_field(target),
        target2: None,
        state,
        timer: Seconds::ZERO,
        range,
        min_range: Distance::ZERO,
        point: Position::new(NONE, NONE, NONE),
        seen: Position::new(NONE, NONE, NONE),
        move_to: None,
        pending: false,
        swing: Seconds::ZERO,
        rest: Seconds::ZERO,
        arms: Arms {
            move_sprite: None,
            sprite: None,
            second_sprite: None,
            idle_sprite: None,
            idle_attack_sprite: None,
            missile: None,
            fire_frame: crate::objects::Frame::of_data(0),
            interrupt_frame: crate::objects::Frame::of_data(-1),
        },
        sub_action: false,
        on_target_moved: TargetMoved::Follows,
        same_owner: false,
    }
}

/// Slot 0x38 (`FUN_140a82ac0`): `set_state(1)`: its `MoveTo` goes, stopping the unit and its
/// animation, then the animation runs at speed 1 and the unit stands; the target stays.
#[test]
fn a_stop_ends_the_attack_and_the_unit_stands() {
    let stand = Graphic {
        id: SpriteId::named(1),
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    let mut a = own_attack(77, AttackState::Move, Distance::new(1.0));
    a.move_to = Some(walking(MoveTo::Moving));
    a.arms.idle_sprite = Some(stand);
    let mut body = Body::Moving(crate::gathering::hunt::tests::walker());
    body.set_anim_speed(Speed::new(0.5));
    let (mut sprite, mut rng) = (None, Well512::default());
    a.halt(&mut body, &mut sprite, &mut rng).unwrap();
    // the MoveTo's teardown stops the unit (its speed 0 at once), then set_state(1) sets 1
    assert_eq!(
        (
            a.state,
            a.move_to,
            ObjectId::field(a.target),
            body.anim_speed()
        ),
        (AttackState::Done, None, 77, Some(Speed::new(1.0)))
    );
    assert_eq!(sprite.map(|s| s.graphic.id), Some(SpriteId::named(1)));
    assert_eq!(body.mover().unwrap().velocity.unwrap().velocity.y(), 0.0);
    // a unit without a standing sprite is not modelled
    let mut bare = own_attack(77, AttackState::Ready, Distance::new(1.0));
    assert!(
        bare.halt(&mut Body::default(), &mut sprite, &mut rng)
            .is_err()
    );
}

/// An object of class 9 in state 2 at `x` on the line y = 10, radii 0.2.
fn mark(id: i32, x: f32) -> Mark {
    Mark {
        id: object_id(id),
        kind: master_type::COMBAT,
        class: class::PREY,
        state: ObjectState::Alive,
        position: Position::new(x, 10.0, 0.5),
        radius: Footprint::new(0.2, 0.2),
    }
}

/// A lion (speed 1.2, radii 0.4) at (10, 10) without an area effect.
const LION: Wielder = Wielder {
    area_effect: Distance::ZERO,
    speed: Speed::new(1.2),
    position: Position::new(10.0, 10.0, 0.0),
    radius: Footprint::new(0.4, 0.4),
};

/// `FUN_140a841d0`: the Attack's own target in states 4 to 7 or 11 is kept as it is; in
/// another state a unit that may move takes it again: where it was seen becomes its place,
/// and the same target changes nothing else.
#[test]
fn an_attack_keeps_its_own_target_and_sees_it_again_out_of_its_moving_states() {
    let deer = mark(7, 13.0);
    for state in [
        AttackState::Move,
        AttackState::GotoSeen,
        AttackState::Ready,
        AttackState::Swing,
        AttackState::GotoPoint,
    ] {
        let mut a = own_attack(7, state, Distance::ZERO);
        let w = a.work(Some(deer), None, &LION, |_| panic!("no range test"));
        assert_eq!(
            w,
            Worked {
                kept: true,
                ..Worked::default()
            }
        );
        assert_eq!(a.seen, Position::new(NONE, NONE, NONE), "{state}");
    }
    for state in [
        AttackState::Done,
        AttackState::Killed,
        AttackState::New,
        AttackState::Failed,
    ] {
        let mut a = own_attack(7, state, Distance::ZERO);
        let w = a.work(Some(deer), None, &LION, |id| id == object_id(7));
        assert_eq!(
            w,
            Worked {
                kept: true,
                ..Worked::default()
            }
        );
        assert_eq!(
            (a.target, a.seen),
            (Some(object_id(7)), deer.position),
            "{state}"
        );
    }
    // none answers false
    let mut a = own_attack(7, AttackState::Done, Distance::ZERO);
    assert_eq!(a.work(None, None, &LION, |_| true), Worked::default());
}

/// The range test of slot 9: a unit without speed, or whose unit AI may not move to attack,
/// keeps a target only within its reach (the range, half a tile more up to 0.1; the gap
/// between the footprints); a NaN speed asks the unit AI, a NaN gap is within (`JA`).
#[test]
fn an_attack_that_cannot_follow_keeps_a_target_only_in_reach() {
    // the gap: 13 - 10 - (0.4 + 0.2) = 2.4
    let deer = mark(7, 13.0);
    let reach = |range: Distance, speed: f32, can: bool| {
        let mut a = own_attack(7, AttackState::Done, range);
        let unit = Wielder {
            speed: Speed::new(speed),
            ..LION
        };
        a.work(Some(deer), None, &unit, |_| can).kept
    };
    assert!(reach(Distance::ZERO, 1.2, true));
    assert!(!reach(Distance::new(2.0), 1.2, false));
    assert!(!reach(Distance::new(2.0), 0.0, true));
    assert!(reach(Distance::new(2.4), 0.0, true));
    assert!(reach(Distance::new(2.5), 0.0, false));
    // a range up to 0.1 reaches half a tile more: 0.1 + 0.5 against a gap of 0.6
    let near = mark(7, 11.2);
    let mut a = own_attack(7, AttackState::Done, Distance::new(0.1));
    assert!(
        a.work(
            Some(near),
            None,
            &Wielder {
                speed: Speed::ZERO,
                ..LION
            },
            |_| true
        )
        .kept
    );
    let mut a = own_attack(7, AttackState::Done, Distance::ZERO);
    let farther = mark(7, 11.3);
    assert!(
        !a.work(
            Some(farther),
            None,
            &Wielder {
                speed: Speed::ZERO,
                ..LION
            },
            |_| { true }
        )
        .kept
    );
    // UCOMISS/JP: a NaN speed is not 0, so the unit AI decides
    let mut asked = false;
    let mut a = own_attack(7, AttackState::Done, Distance::ZERO);
    let unit = Wielder {
        speed: Speed::new(f32::NAN),
        ..LION
    };
    assert!(
        a.work(Some(deer), None, &unit, |_| {
            asked = true;
            true
        })
        .kept
    );
    assert!(asked);
    // COMISS/JA: a NaN gap does not lie beyond the reach
    let lost = Mark {
        position: Position::new(f32::NAN, 10.0, 0.0),
        ..deer
    };
    let mut a = own_attack(7, AttackState::Done, Distance::ZERO);
    assert!(
        a.work(
            Some(lost),
            None,
            &Wielder {
                speed: Speed::ZERO,
                ..LION
            },
            |_| false
        )
        .kept
    );
}

/// An area effect's Attack takes a building's main building (`+0xbb8`): another target than
/// its own clears the unit's attack count, lets the old one go and takes the new one (an own
/// Attack; a sub-action does neither); a target in state 7 or more is not taken, and one of
/// class 60 changes nothing but where it was seen.
#[test]
fn an_area_effect_s_attack_takes_a_building_s_main_building() {
    let annex = Mark {
        kind: BUILDING,
        class: class::BUILDING,
        ..mark(30, 11.0)
    };
    let main = Mark {
        id: object_id(31),
        position: Position::new(12.0, 10.0, 0.0),
        radius: Footprint::new(1.5, 1.5),
        ..annex
    };
    let mangonel = Wielder {
        area_effect: Distance::new(1.0),
        ..LION
    };
    let mut a = own_attack(30, AttackState::Ready, Distance::ZERO);
    let w = a.work(Some(annex), Some(main), &mangonel, |id| id == object_id(31));
    assert_eq!(
        w,
        Worked {
            kept: true,
            clears_count: true,
            lets_go: Some(object_id(30)),
            takes: Some(object_id(31)),
        }
    );
    assert_eq!((a.target, a.seen), (Some(object_id(31)), main.position));
    // without an area effect the annex itself is the target, its own in state 6
    let mut a = own_attack(30, AttackState::Ready, Distance::ZERO);
    assert_eq!(
        a.work(Some(annex), Some(main), &LION, |_| true),
        Worked {
            kept: true,
            ..Worked::default()
        }
    );
    // a sub-action lets go and takes nothing
    let mut a = own_attack(30, AttackState::Ready, Distance::ZERO);
    a.sub_action = true;
    let w = a.work(Some(annex), Some(main), &mangonel, |_| true);
    assert_eq!(
        (
            w.lets_go,
            w.takes,
            w.clears_count,
            ObjectId::field(a.target)
        ),
        (None, None, true, 31)
    );
    // a new target in state 7 or more leaves the Attack without one
    let mut a = own_attack(30, AttackState::Ready, Distance::ZERO);
    let dying = Mark {
        state: ObjectState::Removed,
        ..main
    };
    let w = a.work(Some(annex), Some(dying), &mangonel, |_| true);
    assert_eq!(
        (w.lets_go, w.takes, ObjectId::field(a.target)),
        (Some(object_id(30)), None, -1)
    );
    // class 60: slot 0x68 returns at once; the seen point still follows
    let mut a = own_attack(7, AttackState::Done, Distance::ZERO);
    let odd = Mark {
        class: CLASS_60,
        ..mark(8, 10.5)
    };
    let w = a.work(Some(odd), None, &LION, |_| true);
    assert_eq!(
        (w, a.target, a.seen),
        (
            Worked {
                kept: true,
                ..Worked::default()
            },
            Some(object_id(7)),
            odd.position
        )
    );
    // slot 0x68 with none: the count clears and the old target goes
    let mut a = own_attack(7, AttackState::Done, Distance::ZERO);
    assert_eq!(
        a.set_target(None),
        Worked {
            clears_count: true,
            lets_go: Some(object_id(7)),
            ..Worked::default()
        }
    );
    assert_eq!(a.target, None);
}

/// `FUN_1413b9f00`: the sprite of the graphic asked for goes back to its first frame (true);
/// another graphic's, or none, is left (false).
#[test]
fn a_sprite_resets_only_for_its_own_graphic() {
    let swing = Graphic {
        id: SpriteId::named(224),
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let other = Graphic {
        id: SpriteId::named(225),
        ..swing
    };
    let mut sprite = Some(ActiveSprite {
        frame: crate::objects::Frame::of_data(6),
        last_frame: crate::objects::Frame::of_data(5),
        looped: true,
        ..ActiveSprite::start(swing, &mut Well512::default())
    });
    let before = sprite;
    assert!(!reset_sprite(&mut sprite, other));
    assert_eq!(sprite, before);
    assert!(reset_sprite(&mut sprite, swing));
    let s = sprite.unwrap();
    assert_eq!(
        (s.frame, s.last_frame, s.looped),
        (
            crate::objects::Frame::of_data(0),
            crate::objects::Frame::of_data(0),
            false
        )
    );
    assert!(!reset_sprite(&mut None, swing));
}

/// A soldier (class 6, type 70, speed 0.8, radius 0.2) as a mover.
fn soldier_mover(speed: f32) -> Mover {
    use crate::movement::{MoveStats, Velocity};
    Mover {
        forward: BVector::new(1.0, 0.0, 0.0),
        right: BVector::new(0.0, 0.0, 1.0),
        velocity: Some(Velocity {
            velocity: BVector::ZERO,
            acceleration: BVector::ZERO,
        }),
        paths: vec![],
        factor: 1.0,
        stats: MoveStats {
            fly_mode: FlyMode::GROUND,
            speed: Speed::new(speed),
            base: Speed::new(speed),
            turn_rate: TurnRate::MAX,
            turn_radius: Distance::ZERO,
            turn_radius_speed: Speed::MAX,
            radius: Radii::new(0.2, 0.2, 0.5),
            restriction: RestrictionId::named(7),
            restriction_mask: TerrainMask::of_bits(6),
            turn_time: Seconds::ZERO,
            move_algorithm: MoveAlgorithm::NONE,
            min_collision: 1.0,
            fast_turn: false,
            facets: Some(crate::objects::facing::SpriteFacets {
                count: crate::objects::FacetCount::of_data(8),
                turns: true,
            }),
            weapon_range: Distance::ZERO,
        },
        waiting: 0,
        wait_delays: crate::movement::WaitDelays::of_data(0),
        attack_moving: false,
        last_move: WorldTime::ZERO,
        moved_at: Position::ORIGIN,
        substitute_count: crate::movement::SubstituteCount::of_data(0),
        ignore_units: Vec::new(),
        user_waypoints: Vec::new(),
        last_turn: Seconds::ZERO,
        angle: Angle::ZERO,
        animated: Animated::default(),
        pauses: Vec::new(),
        in_the_way: Vec::new(),
        sprite_facets: None,
    }
}

const STANDING: Graphic = Graphic {
    id: SpriteId::named(300),
    frames: crate::objects::Frame::of_data(10),
    facets: crate::objects::FacetCount::of_data(8),
    speed_multiplier: SpeedMultiplier::ONE,
    frame_duration: Seconds::new(0.1),
    replay_delay: Seconds::ZERO,
    flags: SpriteFlags::ANIMATED,
};

/// The reads of a soldier's Attack ([`AttackReads`]): the standing sprite [`STANDING`], a
/// reload of 2 s, a sure hit, a unit AI without an order that may move in stance 0 with an
/// empty queue, a human player at the world's time 0.
pub(crate) fn reads() -> AttackReads {
    AttackReads {
        move_defaults: MoveDefaults::default(),
        standing: Some(STANDING),
        volleys: false,
        reload: Seconds::new(2.0),
        hit_chance: HitChance::of_data(100),
        armed: true,
        swing_sprites: SwingSprites::default(),
        computer: false,
        world_time: WorldTime::ZERO,
        can_move: true,
        stance: UnitStance::AGGRESSIVE,
        state_position: Point::ORIGIN,
        queue_empty: true,
        ai_order: AiOrder::Idle,
        ai_type: None,
        volley_under_way: false,
    }
}

/// The soldier 1 of player 1 as a combat object: type 70, class 6.
pub(crate) fn soldier() -> Victim {
    Victim {
        id: object_id(1),
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        player: PlayerId::new(1),
        ..Victim::default()
    }
}

/// An Attack's update on the step: a world of Gaia and players 1 and 2 (master 4, a soldier of
/// class 6, for each) whose player 1 holds the attacker first (in state 2, with a unit AI on the
/// Attack's action, which takes its notifies); the update's reads ([`reads`]) and its fight
/// (the target and the attacker as combat objects).
pub(crate) struct Rig {
    pub(crate) t: World,
    pub(crate) f: Fight,
    pub(crate) r: AttackReads,
}

impl Rig {
    /// The soldier 1 at (10, 10), a mover of `speed` ([`soldier_mover`]), on `map`, attacking
    /// `victim` as [`soldier`].
    pub(crate) fn on(map: Spatial, speed: f32, victim: Victim) -> Self {
        let players = (0..3).map(|_| Player::default()).collect();
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(0),
            PlayerId::new(0),
            vec![Millis::ZERO; 3],
            players,
        );
        t.spatial = map;
        let _ = t
            .push_live(
                PlayerId::new(1),
                Unit {
                    id: object_id(1),
                    body: Body::new(
                        Position::new(10.0, 10.0, 0.0),
                        None,
                        Some(soldier_mover(speed)),
                    ),
                    ..Unit::default()
                },
            )
            .unwrap();
        Self::with_unit(t, victim, soldier())
    }

    /// The builder 9177 of [`crate::movement::tests::path_world`] on its path toward house
    /// 10028, attacking `victim` as [`soldier`] does.
    pub(crate) fn on_the_path(victim: Victim) -> Self {
        let t = crate::movement::tests::path_world(Player::default());
        let own = Victim {
            id: object_id(9177),
            ..soldier()
        };
        Self::with_unit(t, victim, own)
    }

    fn with_unit(mut t: World, victim: Victim, own: Victim) -> Self {
        let master = Master {
            id: master_id(4),
            kind: master_type::COMBAT,
            class: class::CLASS_6,
            move_sprite: None,
            standing_sprite: None,
            ..Master::default()
        };
        t.masters = vec![Masters::new([master]); t.players.len()];
        let unit = &mut t.players[1].objects[ListIndex::new(0)];
        unit.life.state = ObjectState::Alive;
        unit.life.master_id = master_id(4);
        unit.combat.ai = Some(UnitAi {
            current_action: Some(ActionKind::Attack),
            ..UnitAi::default()
        });
        Self {
            t,
            f: Fight { victim, own },
            r: reads(),
        }
    }

    /// The rig with the object `id` of master 4 in `player`'s list too, one worker at it: a
    /// release counts the worker off ([`Rig::workers`]).
    pub(crate) fn holding(mut self, id: i32, player: PlayerId) -> Self {
        let mut u = Unit {
            id: object_id(id),
            workers: crate::objects::WorkerCount::of_data(1),
            ..Unit::default()
        };
        u.life.state = ObjectState::Alive;
        u.life.master_id = master_id(4);
        let _ = self.t.push_live(player, u).unwrap();
        self
    }

    /// The workers at object `id` of `player` ([`Rig::holding`]).
    pub(crate) fn workers(&self, id: i32, player: PlayerId) -> Option<i32> {
        let own = &self.t.players[player.index()].objects;
        own.iter()
            .find(|u| u.id == object_id(id))
            .map(|u| u.workers.get())
    }

    /// `f` on the attacker's step of `dt` ms with the fight and the reads.
    pub(crate) fn run<R>(
        &mut self,
        dt: u32,
        f: impl FnOnce(&mut Step<'_>, &mut Unit, &mut Fight, &AttackReads) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        let (fight, r) = (&mut self.f, &self.r);
        crate::world::tests::in_update(
            &mut self.t,
            (PlayerId::new(1), ListIndex::new(0)),
            dt,
            |u, c| f(c, u, fight, r),
        )
    }

    /// `f` on the attacker with the world's parts readyToAttack reads (as
    /// [`World::attack_ground`]: its notifies go to the unit AI at once).
    pub(crate) fn ground<R>(
        &mut self,
        f: impl FnOnce(&mut Unit, &mut Ground<'_>, &Fight, &AttackReads) -> R,
    ) -> R {
        let World {
            spatial,
            streams,
            players,
            ..
        } = &mut self.t;
        let (object, sprite) = streams.object_and_sprite();
        let mut g = Ground {
            map: spatial,
            sprite_rng: sprite,
            object_rng: Some(object),
            buffer: None,
            graphics: &NO_GRAPHICS,
        };
        f(
            &mut players[1].objects[ListIndex::new(0)],
            &mut g,
            &self.f,
            &self.r,
        )
    }

    /// The attacker.
    pub(crate) fn unit(&self) -> &Unit {
        &self.t.players[1].objects[ListIndex::new(0)]
    }

    /// The attacker to change.
    pub(crate) fn unit_mut(&mut self) -> &mut Unit {
        &mut self.t.players[1].objects[ListIndex::new(0)]
    }

    /// Its movement state.
    pub(crate) fn mover(&self) -> &Mover {
        self.unit().body.mover().expect("the rig's attacker moves")
    }

    /// Its movement state to change.
    pub(crate) fn mover_mut(&mut self) -> &mut Mover {
        self.unit_mut()
            .body
            .mover_mut()
            .expect("the rig's attacker moves")
    }

    /// The notifies the attacker posted to its unit AI: kind and parameters, in order.
    pub(crate) fn posted(&self) -> Vec<(NotifyKind, [i32; 3])> {
        self.unit().combat.ai.as_ref().map_or_else(Vec::new, |ai| {
            ai.notifies.iter().map(|n| (n.kind, n.p)).collect()
        })
    }

    /// Its sprite's graphic.
    pub(crate) fn sprite(&self) -> Option<SpriteId> {
        self.unit().sprite.map(|sp| sp.graphic.id)
    }
}

/// The soldier's [`Me`] in action state `state`.
const fn me(id: i32, state: AttackState) -> Me {
    Me {
        id: object_id(id),
        group: None,
        action_state: ActionState::new(state.get()),
    }
}

/// A building of player 2 at `at` (radius 0.5, 100 hit points, no armor, reached by blasts
/// of level 2) with its `building` part.
fn struck_building(id: i32, at: Position, building: damage::Building) -> Victim {
    Victim {
        id: object_id(id),
        hp: HitPoints::new(100.0),
        kind: BUILDING,
        class: class::BUILDING,
        player: PlayerId::new(2),
        position: at,
        radius: Footprint::new(0.5, 0.5),
        blast_defense: BlastDefense::new(2),
        master_hp: MasterHitPoints::of_data(100),
        armored: Some(damage::Armored {
            armors: vec![ClassAmount::new(4, 0)],
            terrain_bonus: RestrictionField::NONE,
            friendly_fire: crate::objects::masters::DamageShare::new(1.0),
            ..damage::Armored::default()
        }),
        building: Some(building),
        ..Victim::default()
    }
}

/// `FUN_141084230`'s hit on an annex is its main building's (`FUN_1411cea90`'s forward):
/// the main building among the objects the hit may reach loses it, and the annex takes its
/// hit points.
#[test]
fn a_strike_on_an_annex_hits_its_main_building() {
    let mut annex = struck_building(
        21,
        Position::new(10.5, 10.0, 0.0),
        damage::Building {
            main: Some(MainBuildingId::new(object_id(20))),
            ..damage::Building::default()
        },
    );
    let mut blasted = vec![struck_building(
        20,
        Position::new(11.5, 10.0, 0.0),
        damage::Building {
            annexes: vec![object_id(21)],
            ..damage::Building::default()
        },
    )];
    let mut own = Victim {
        id: object_id(1),
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        player: PlayerId::new(1),
        source: damage::Attacker {
            id: object_id(1),
            kind: master_type::COMBAT,
            class: class::CLASS_6,
            missile: None,
            player: PlayerId::new(1),
            ..damage::Attacker::default()
        },
        weapons: vec![ClassAmount::new(4, 4)],
        ..Victim::default()
    };
    let hit = Hit {
        at: own.position,
        from_z: Some(0.0),
        missile: false,
        chance: HitChance::of_data(100),
    };
    let struck = strike(
        &mut own,
        &mut annex,
        &mut blasted,
        hit,
        &mut Well512::default(),
    )
    .unwrap();
    assert_eq!(
        struck.result,
        crate::combat::missile::StrikeResult::of_data(1)
    );
    assert_eq!(
        (blasted[0].hp, annex.hp),
        (HitPoints::new(96.0), HitPoints::new(96.0))
    );
    assert!(blasted[0].building.as_ref().is_some_and(|b| b.struck));
}

/// A building shooter's missile (429184426's watch tower 25663): its elevation factor is the
/// building's (`FUN_1411cc890`), the combat object's times 1.2 while it holds a specific relic
/// (`+0xc24`), so its hit on a lower target does more than a tower's without one.
#[test]
fn a_building_s_relic_raises_its_missile_s_hit() {
    let hit_by = |relics: Relics| {
        let mut target = struck_building(
            21,
            Position::new(10.5, 10.0, 0.0),
            damage::Building::default(),
        );
        let mut own = Victim {
            id: object_id(1),
            kind: BUILDING,
            class: class::TOWER,
            player: PlayerId::new(1),
            position: Position::new(8.0, 10.0, 2.0),
            source: damage::Attacker {
                id: object_id(1),
                kind: BUILDING,
                class: class::TOWER,
                missile: Some(ids::FARM),
                player: PlayerId::new(1),
                ..damage::Attacker::default()
            },
            weapons: vec![ClassAmount::new(4, 40)],
            building: Some(damage::Building {
                relics,
                ..damage::Building::default()
            }),
            ..Victim::default()
        };
        let hit = Hit {
            at: target.position,
            from_z: Some(2.0),
            missile: true,
            chance: HitChance::of_data(100),
        };
        let _ = strike(
            &mut own,
            &mut target,
            &mut Vec::new(),
            hit,
            &mut Well512::default(),
        )
        .unwrap();
        target.hp
    };
    assert!(
        hit_by(Relics::new(
            crate::objects::RelicCount::of_data(0),
            crate::objects::RelicCount::of_data(1)
        )) < hit_by(Relics::new(
            crate::objects::RelicCount::of_data(0),
            crate::objects::RelicCount::of_data(0)
        ))
    );
    assert!(
        hit_by(Relics::new(
            crate::objects::RelicCount::of_data(1),
            crate::objects::RelicCount::of_data(0)
        )) < hit_by(Relics::new(
            crate::objects::RelicCount::of_data(0),
            crate::objects::RelicCount::of_data(0)
        ))
    );
    assert!(
        hit_by(Relics::new(
            crate::objects::RelicCount::of_data(0),
            crate::objects::RelicCount::of_data(1)
        )) < hit_by(Relics::new(
            crate::objects::RelicCount::of_data(1),
            crate::objects::RelicCount::of_data(0)
        ))
    );
}

/// `FUN_1412115e0`'s trample: its blast about the unit passes its target but reaches the
/// target's main building (5 of 10 at half the damage), whose blast so far (`+0xb90`,
/// `0x141211933`) the hit on the annex then leaves out: 10 less 5.
#[test]
fn a_trample_s_hit_leaves_out_its_building_target_s_blast_so_far() {
    let mut blasted = vec![struck_building(
        20,
        Position::new(10.5, 10.0, 0.0),
        damage::Building {
            annexes: vec![object_id(21)],
            ..damage::Building::default()
        },
    )];
    let mut f = Fight {
        victim: struck_building(
            21,
            Position::new(10.2, 10.0, 0.0),
            damage::Building {
                main: Some(MainBuildingId::new(object_id(20))),
                ..damage::Building::default()
            },
        ),
        own: Victim {
            position: Position::new(10.0, 10.0, 0.0),
            weapons: vec![ClassAmount::new(4, 10)],
            source: damage::Attacker {
                id: object_id(1),
                kind: master_type::COMBAT,
                class: class::CLASS_6,
                missile: None,
                player: PlayerId::new(1),
                area_effect: Distance::new(1.0),
                blast_level: BlastLevel::from_bits_retain(2),
                area_damage: crate::objects::masters::DamageShare::new(0.5),
                ..damage::Attacker::default()
            },
            ..soldier()
        },
    };
    let executed = Attack::hit(
        &mut f,
        &mut blasted,
        HitChance::of_data(100),
        &mut Well512::default(),
    )
    .unwrap();
    assert_eq!(executed, Some(object_id(21)));
    assert_eq!(
        (blasted[0].hp, f.victim.hp),
        (HitPoints::new(90.0), HitPoints::new(90.0))
    );
    assert_eq!(
        blasted[0].building.as_ref().map(|b| b.blast_taken),
        Some(HitPoints::new(5.0))
    );
}

/// The blast so far a trample's hit leaves out: a building's own, an annex's main building's
/// (one the hit does not hold is not modelled), none for another object.
#[test]
fn a_trample_reads_a_building_s_blast_so_far_or_its_main_building_s() {
    let building = |main, blast_taken| {
        struck_building(
            21,
            Position::ORIGIN,
            damage::Building {
                main,
                blast_taken,
                ..damage::Building::default()
            },
        )
    };
    assert_eq!(
        blast_so_far(&building(None, HitPoints::new(3.0)), &[]),
        Ok(HitPoints::new(3.0))
    );
    let main = building(None, HitPoints::new(5.0));
    let main = Victim {
        id: object_id(20),
        ..main
    };
    assert_eq!(
        blast_so_far(
            &building(
                Some(MainBuildingId::new(object_id(20))),
                HitPoints::new(3.0)
            ),
            &[main]
        ),
        Ok(HitPoints::new(5.0))
    );
    assert_eq!(
        blast_so_far(
            &building(
                Some(MainBuildingId::new(object_id(20))),
                HitPoints::new(3.0)
            ),
            &[]
        ),
        Err(crate::construction::build::NOT_HELD)
    );
    let unit = Victim {
        kind: master_type::COMBAT,
        ..Victim::default()
    };
    assert_eq!(blast_so_far(&unit, &[]), Ok(HitPoints::ZERO));
}

/// A world holding the target `id` of `player` and class `class` at `at`, radius 0.2.
fn world_with(id: i32, player: PlayerId, class: ClassId, at: Position) -> Spatial {
    let mut world = Spatial::default();
    world.insert(
        object_id(id),
        crate::map::spatial::Placed {
            position: at,
            radius: Footprint::new(0.2, 0.2),
            class,
            player,
            master: master_id(4),
            obstruction_type: ObstructionType::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    world
}

/// An own Attack of the soldier at (10, 10) on `target` in state 6, range `range` and least
/// range `min`, its point `point`.
fn soldier_attack(target: i32, range: Distance, min: f32, point: Position) -> Attack {
    let mut a = own_attack(target, AttackState::Ready, range);
    a.min_range = Distance::new(min);
    a.point = point;
    a.arms.idle_sprite = Some(STANDING);
    a
}

/// readyToAttack below the least range: a unit's own Attack posts 0x1fe (−1, −1, −1); a unit
/// that may move waits in its state, one without speed fails (`set_state` 13: the standing
/// sprite at speed 1).
#[test]
fn ready_to_attack_under_the_least_range_posts_0x1fe_and_waits_or_fails() {
    let at = Position::new(11.0, 10.0, 0.0);
    let target = || Victim {
        id: object_id(7),
        position: at,
        ..Victim::default()
    };
    for (speed, state) in [(0.8, AttackState::Ready), (0.0, AttackState::Failed)] {
        let mut rig = Rig::on(
            world_with(7, PlayerId::new(0), class::CLASS_6, at),
            speed,
            target(),
        );
        let mut a = soldier_attack(7, Distance::new(4.0), 2.0, Position::from([NONE; 3]));
        let r = rig.ground(|u, g, f, r| a.ready_to_attack(u, g, f, r, false));
        assert!(matches!(r, Ok(Readiness::Waiting)));
        assert_eq!(
            (a.state, rig.posted()),
            (state, vec![(NotifyKind::TOO_CLOSE, [-1; 3])])
        );
        if state == AttackState::Failed {
            assert_eq!(rig.mover().animated.speed, Speed::new(1.0));
            assert_eq!(rig.sprite(), Some(STANDING.id));
        }
    }
    // a sub-action posts nothing
    let mut rig = Rig::on(
        world_with(7, PlayerId::new(0), class::CLASS_6, at),
        0.8,
        target(),
    );
    let mut a = soldier_attack(7, Distance::new(4.0), 2.0, Position::from([NONE; 3]));
    a.sub_action = true;
    let _ = rig
        .ground(|u, g, f, r| a.ready_to_attack(u, g, f, r, false))
        .unwrap();
    assert_eq!(rig.posted(), Vec::new());
}

/// readyToAttack beyond the reach: a unit's own Attack posts 0x1fd; a unit that may move
/// follows (`set_state` 4 is the caller's), one that cannot fails (`set_state` 13).
#[test]
fn ready_to_attack_out_of_reach_follows_or_fails() {
    let at = Position::new(15.0, 10.0, 0.0);
    for (speed, can_move, follows) in [(0.8, true, true), (0.0, true, false), (0.8, false, false)] {
        let victim = Victim {
            id: object_id(7),
            position: at,
            ..Victim::default()
        };
        let mut rig = Rig::on(
            world_with(7, PlayerId::new(0), class::CLASS_6, at),
            speed,
            victim,
        );
        rig.r.can_move = can_move;
        let mut a = soldier_attack(7, Distance::new(1.0), 0.0, Position::new(NONE, NONE, NONE));
        let r = rig
            .ground(|u, g, f, r| a.ready_to_attack(u, g, f, r, false))
            .unwrap();
        assert_eq!(rig.posted(), vec![(NotifyKind::OUT_OF_REACH, [0; 3])]);
        assert_eq!(matches!(r, Readiness::Away(_)), follows);
        assert_eq!(
            a.state,
            if follows {
                AttackState::Ready
            } else {
                AttackState::Failed
            }
        );
    }
}

/// The update's answer to a target out of sight ([`Attack::out_of_sight`]): the target goes,
/// the attack fails (state 13); a unit's own Attack lets the target go and its unit AI forgets
/// it at once; a predator's boar is sent home, a lion's or one without a unit AI not; with its
/// queue empty it posts 0x1f9 with 600, with orders queued not. A sub-action does none of these.
/// Another predator's unit AI is not modelled.
#[test]
fn an_attack_on_a_target_out_of_sight_fails() {
    let run = |a: &mut Attack, class, ai_type, queue_empty| {
        let mut rig = Rig::on_the_path(Victim::default()).holding(77, PlayerId::new(1));
        (rig.f.own.class, rig.r.ai_type, rig.r.queue_empty) = (class, ai_type, queue_empty);
        let ai = rig.unit_mut().combat.ai.as_mut().unwrap();
        ai.current_target = Some(object_id(77));
        ai.home = Some(crate::unit_ai::Home::default());
        let code = rig.run(100, |c, u, f, r| a.out_of_sight(c, u, f, r));
        let ai = rig.unit().combat.ai.as_ref().unwrap();
        let (forgot, home) = (ai.current_target.is_none(), ai.home.unwrap().go_home);
        let let_go = rig.workers(77, PlayerId::new(1)) == Some(0);
        (code, let_go, forgot, home, rig.posted())
    };
    let failed = vec![(NotifyKind::ACTION_FAILED, [600, 0, 0])];
    let own_attack = |target, state, range| {
        let mut a = own_attack(target, state, range);
        a.arms.idle_sprite = Some(STANDING);
        a
    };
    let mut a = own_attack(77, AttackState::Move, Distance::new(1.0));
    let got = run(&mut a, class::CLASS_6, Some(UnitAiType::SOLDIER), true);
    assert_eq!(got, (Ok(UpdateCode::FAILED), true, true, false, failed));
    assert_eq!((a.target, a.state), (None, AttackState::Failed));
    let mut a = own_attack(77, AttackState::Move, Distance::new(1.0));
    assert!(run(&mut a, class::PREDATOR, Some(BOAR_AI), false).3);
    let mut a = own_attack(77, AttackState::Move, Distance::new(1.0));
    let lion = run(&mut a, class::PREDATOR, Some(LION_AI), false);
    assert_eq!((lion.2, lion.3, lion.4), (true, false, vec![]));
    let mut a = own_attack(77, AttackState::Move, Distance::new(1.0));
    let alone = run(&mut a, class::PREDATOR, None, true);
    assert_eq!((alone.0, alone.3), (Ok(UpdateCode::FAILED), false));
    let mut a = Attack {
        sub_action: true,
        ..own_attack(77, AttackState::Move, Distance::new(1.0))
    };
    assert_eq!(
        run(&mut a, class::CLASS_6, Some(UnitAiType::SOLDIER), true),
        (Ok(UpdateCode::FAILED), false, false, false, vec![])
    );
    let mut a = own_attack(77, AttackState::Move, Distance::new(1.0));
    assert!(
        run(&mut a, class::PREDATOR, Some(UnitAiType::SOLDIER), true)
            .0
            .is_err()
    );
}

/// readyToAttack on a player's target out of sight: a unit of stance 0 with speed that may
/// move waits (`set_state` 5: a `MoveTo` to where it saw the target); any other fails
/// (`set_state` 13).
#[test]
fn ready_to_attack_on_a_target_out_of_sight_fails_or_waits() {
    let at = Position::new(11.0, 10.0, 0.0);
    for (stance, fails) in [
        (UnitStance::AGGRESSIVE, false),
        (UnitStance::STAND_GROUND, true),
    ] {
        let mut world = world_with(7, PlayerId::new(2), class::CLASS_6, at);
        world.sight = Some(sight(false));
        let victim = Victim {
            id: object_id(7),
            player: PlayerId::new(2),
            position: at,
            ..Victim::default()
        };
        let mut rig = Rig::on(world, 0.8, victim);
        rig.r.stance = stance;
        let mut a = soldier_attack(7, Distance::new(4.0), 0.0, at);
        a.seen = at;
        let r = rig.ground(|u, g, f, r| a.ready_to_attack(u, g, f, r, false));
        assert!(matches!(r, Ok(Readiness::Waiting)));
        if fails {
            assert_eq!(a.state, AttackState::Failed);
        } else {
            assert_eq!(
                (a.state, a.move_to.map(|m| (m.point, m.state))),
                (AttackState::GotoSeen, Some((at, MoveTo::Start)))
            );
        }
    }
}

/// The players' sight over a 16×16 map: every tile revealed and explored (seen), or none.
fn sight(seen: bool) -> crate::vision::visibility::Visibility {
    let word = if seen {
        crate::vision::VisionBits::NO_TILE | crate::vision::VisionBits::NO_TILE_EXPLORED
    } else {
        crate::vision::VisionBits::NONE
    };
    crate::vision::visibility::Visibility::new(
        GridSize::new(16, 16),
        Vec::new(),
        vec![word; 256],
        Vec::new(),
        0,
        Vec::new(),
    )
}

/// `0x140a82ae0`'s moved target: out of its player's sight, a player's unit whose unit AI
/// has an order and an empty queue waits from state 4 (`set_state` 5: a `MoveTo` to where it
/// saw the target, the range slot 0x238's) and returns 0; seen again, an Attack in state 5 or
/// 11 follows the target (`set_state` 4).
#[test]
fn an_attack_waits_where_it_last_saw_its_target_and_follows_it_seen_again() {
    let (seen_at, now_at) = (
        Position::new(12.0, 10.0, 0.0),
        Position::new(13.0, 10.0, 0.0),
    );
    let run = |state: AttackState, seen: bool| {
        let mut world = world_with(7, PlayerId::new(2), class::CLASS_6, now_at);
        world.sight = Some(sight(seen));
        let victim = Victim {
            id: object_id(7),
            state: ObjectState::Alive,
            player: PlayerId::new(2),
            position: now_at,
            ..Victim::default()
        };
        let mut rig = Rig::on(world, 0.8, victim);
        rig.r.ai_order = AiOrder::Order(OrderKind::Attack);
        rig.f.own.source.weapon_range = Distance::new(3.0);
        let mut a = soldier_attack(7, Distance::new(1.0), 0.0, seen_at);
        (a.state, a.seen, a.move_to) = (state, seen_at, Some(walking(MoveTo::Moving)));
        let code = rig
            .run(20, |c, u, f, r| {
                a.update(c, u, me(1, AttackState::Ready), f, r)
            })
            .unwrap();
        (code, a)
    };
    let (code, a) = run(AttackState::Move, false);
    assert_eq!(
        (
            code,
            a.state,
            a.seen,
            a.move_to.map(|m| (m.point, m.state)),
            a.range
        ),
        (
            UpdateCode::UNDER_WAY,
            AttackState::GotoSeen,
            now_at,
            Some((now_at, MoveTo::Start)),
            Distance::new(3.0)
        )
    );
    for state in [AttackState::GotoSeen, AttackState::GotoPoint] {
        let (code, a) = run(state, true);
        assert_eq!(
            (code, a.state, a.move_to.map(|m| m.state), a.range),
            (
                UpdateCode::UNDER_WAY,
                AttackState::Move,
                Some(MoveTo::Start),
                Distance::new(3.0)
            ),
            "{state}"
        );
    }
}

/// States 5 and 11 once their `MoveTo` arrived: a target in sight makes the unit ready; out
/// of sight, a human player's unit in state 5 with a range of at least 1 walks onto the
/// point (`set_state` 11), any other lets the target go at once and fails (`set_state` 13, its
/// own Attack posting 0x1f9) with 3.
#[test]
fn an_attack_arrived_where_it_saw_its_target_looks_again() {
    let at = Position::new(13.0, 10.0, 0.0);
    let run = |state: AttackState, seen: bool, range: Distance, computer: bool| {
        let mut world = world_with(7, PlayerId::new(2), class::CLASS_6, at);
        world.sight = Some(sight(seen));
        let victim = Victim {
            id: object_id(7),
            state: ObjectState::Alive,
            player: PlayerId::new(2),
            position: at,
            ..Victim::default()
        };
        let mut rig = Rig::on(world, 0.8, victim).holding(7, PlayerId::new(2));
        rig.r.computer = computer;
        rig.f.own.source.weapon_range = Distance::new(4.0);
        let mut a = soldier_attack(7, range, 0.0, at);
        (a.state, a.seen, a.move_to) = (state, at, Some(walking(MoveTo::Arrived)));
        let code = rig
            .run(20, |c, u, f, r| {
                a.update(c, u, me(1, AttackState::Ready), f, r)
            })
            .unwrap();
        let let_go = rig.workers(7, PlayerId::new(2)) == Some(0);
        (code, a, rig.posted(), let_go)
    };
    let (code, a, _, _) = run(AttackState::GotoSeen, true, Distance::new(2.0), false);
    assert_eq!(code, UpdateCode::UNDER_WAY);
    assert!(
        !matches!(a.state, AttackState::GotoSeen | AttackState::Failed),
        "{}",
        a.state
    );
    let (code, a, posted, _) = run(AttackState::GotoSeen, false, Distance::new(1.0), false);
    assert_eq!(
        (
            code,
            a.state,
            a.move_to.map(|m| (m.point, m.state)),
            a.range,
            posted
        ),
        (
            UpdateCode::UNDER_WAY,
            AttackState::GotoPoint,
            Some((at, MoveTo::Start)),
            Distance::new(4.0),
            vec![]
        )
    );
    for (state, range, computer) in [
        (AttackState::GotoSeen, 0.9, false),
        (AttackState::GotoSeen, f32::NAN, false),
        (AttackState::GotoSeen, 2.0, true),
        (AttackState::GotoPoint, 2.0, false),
    ] {
        let (code, a, posted, let_go) = run(state, false, Distance::new(range), computer);
        assert_eq!(
            (code, a.state, a.target, let_go, posted),
            (
                UpdateCode::FAILED,
                AttackState::Failed,
                None,
                true,
                vec![(NotifyKind::ACTION_FAILED, [600, 0, 0])]
            ),
            "{state} {range} {computer}"
        );
    }
}

/// readyToAttack's ready-at-once test: a target on the attack's point is ready at once, but
/// not with the third argument; a NaN difference from the point is not within half a tile
/// (the reach decides and the target's place becomes the point); a NaN attack timer has run
/// out (slot 0x100's `SETBE`).
#[test]
fn ready_to_attack_s_at_once_test_and_its_nan_paths() {
    let at = Position::new(11.0, 10.0, 0.0);
    let run = |point: Position, skip: bool, range: Distance, timer: f32| {
        let victim = Victim {
            id: object_id(7),
            position: at,
            ..Victim::default()
        };
        let mut rig = Rig::on(
            world_with(7, PlayerId::new(0), class::CLASS_6, at),
            0.8,
            victim,
        );
        rig.unit_mut().combat.attack_timer = Seconds::new(timer);
        let mut a = soldier_attack(7, range, 0.0, point);
        let r = rig
            .ground(|u, g, f, r| a.ready_to_attack(u, g, f, r, skip))
            .unwrap();
        (r, a.point)
    };
    assert!(matches!(
        run(at, false, Distance::ZERO, 0.0).0,
        Readiness::Ready
    ));
    // the third argument skips it: the gap 0.6 lies beyond a reach of 0.5
    assert!(matches!(
        run(at, true, Distance::ZERO, 0.0).0,
        Readiness::Away(_)
    ));
    // a NaN point: moved and far, the reach decides (2 > 0.6) and the point follows
    let (r, point) = run(
        Position::new(f32::NAN, 10.0, 0.0),
        false,
        Distance::new(2.0),
        0.0,
    );
    assert!(matches!(r, Readiness::Ready));
    assert_eq!(point, at);
    assert!(matches!(
        run(at, false, Distance::ZERO, f32::NAN).0,
        Readiness::Ready
    ));
    assert!(matches!(
        run(at, false, Distance::ZERO, 0.5).0,
        Readiness::Waiting
    ));
}

/// The ready-at-once range test of a unit that does not move: `COMISS`/`JA` past the range,
/// `JBE` on each least-range test, so NaN passes all three.
#[test]
fn the_ready_at_once_range_test_passes_nan() {
    assert!(in_range(
        Distance::new(1.0),
        Distance::new(2.0),
        Distance::new(0.0)
    ));
    assert!(!in_range(
        Distance::new(3.0),
        Distance::new(2.0),
        Distance::new(0.0)
    ));
    assert!(in_range(
        Distance::new(f32::NAN),
        Distance::new(2.0),
        Distance::new(0.0)
    ));
    assert!(in_range(
        Distance::new(1.0),
        Distance::new(f32::NAN),
        Distance::new(0.0)
    ));
    // a least range above 0.1 needs the gap at least as far
    assert!(!in_range(
        Distance::new(0.5),
        Distance::new(2.0),
        Distance::new(1.0)
    ));
    assert!(in_range(
        Distance::new(1.5),
        Distance::new(2.0),
        Distance::new(1.0)
    ));
    assert!(in_range(
        Distance::new(0.5),
        Distance::new(2.0),
        Distance::new(f32::NAN)
    ));
    assert!(in_range(
        Distance::new(f32::NAN),
        Distance::new(2.0),
        Distance::new(1.0)
    ));
}

/// `FUN_140a82ae0`'s state without a case (of an Attack's, 0) returns 0; its timer runs.
#[test]
fn an_attack_in_a_state_without_a_case_returns_0() {
    let at = Position::new(11.0, 10.0, 0.0);
    let victim = Victim {
        id: object_id(7),
        state: ObjectState::Alive,
        position: at,
        ..Victim::default()
    };
    let mut rig = Rig::on(
        world_with(7, PlayerId::new(0), class::CLASS_6, at),
        0.8,
        victim,
    );
    let mut a = soldier_attack(7, Distance::new(1.0), 0.0, at);
    (a.state, a.seen) = (AttackState::New, at);
    let code = rig.run(20, |c, u, f, r| {
        a.update(c, u, me(1, AttackState::Ready), f, r)
    });
    assert_eq!(code, Ok(UpdateCode::UNDER_WAY));
    assert_eq!((a.state, a.timer), (AttackState::New, Seconds::new(0.02)));
}

/// `0x140a82ae0` at `LAB_140a834ab`, after the target's sight check: a live target (state 0
/// to 2) of a player's unit (`+0x718` not 0) that its player counts as itself or an ally
/// (`FUN_141663dc0` with 1) ends the Attack (`set_state(1)`; its own Attack posts 0x1fb with
/// 600 and the target) and returns 1, unless the target is livestock (58) or a farm (49) and
/// was its player's when the Attack took it (`+0x9f`). 427003795's update 357: the hunters'
/// goat (class 58), Gaia's when their Attacks began, is now their player's.
#[test]
fn an_attack_on_a_target_its_player_counts_as_its_own_ends() {
    let at = Position::new(11.0, 10.0, 0.0);
    let run =
        |class: ClassId, allied: bool, same_owner: bool, player: PlayerId, sub_action: bool| {
            let victim = Victim {
                id: object_id(7),
                state: ObjectState::Alive,
                class,
                player: PlayerId::new(1),
                position: at,
                allied: Allied::new(allied, allied),
                ..Victim::default()
            };
            let mut rig = Rig::on(world_with(7, PlayerId::new(1), class, at), 0.8, victim);
            rig.f.own.player = player;
            let mut a = soldier_attack(7, Distance::new(1.0), 0.0, at);
            // state 0 has no case: an Attack the test spares returns 0
            (a.state, a.seen, a.same_owner, a.sub_action) =
                (AttackState::New, at, same_owner, sub_action);
            let code = rig
                .run(20, |c, u, f, r| {
                    a.update(c, u, me(1, AttackState::Ready), f, r)
                })
                .unwrap();
            (code, a.state, rig.posted())
        };
    // the hunt's goat, Gaia's when its Attack began: the quiet sub-action ends
    assert_eq!(
        run(class::LIVESTOCK, true, false, PlayerId::new(1), true),
        (UpdateCode::DONE, AttackState::Done, vec![])
    );
    // a unit's own Attack posts 0x1fb with 600 and its target
    assert_eq!(
        run(class::LIVESTOCK, true, false, PlayerId::new(1), false),
        (
            UpdateCode::DONE,
            AttackState::Done,
            vec![(NotifyKind::ACTION_INVALIDATED, [600, 7, 0])]
        )
    );
    // its own livestock or farm when it began: spared
    assert_eq!(
        run(class::LIVESTOCK, true, true, PlayerId::new(1), true).0,
        UpdateCode::UNDER_WAY
    );
    assert_eq!(
        run(class::FARM, true, true, PlayerId::new(1), true).0,
        UpdateCode::UNDER_WAY
    );
    // another class whatever it began on; an enemy's or Gaia's unit's go on
    assert_eq!(
        run(class::CLASS_6, true, true, PlayerId::new(1), true).0,
        UpdateCode::DONE
    );
    assert_eq!(
        run(class::CLASS_6, false, false, PlayerId::new(1), true).0,
        UpdateCode::UNDER_WAY
    );
    assert_eq!(
        run(class::CLASS_6, true, false, PlayerId::new(0), true).0,
        UpdateCode::UNDER_WAY
    );
}

/// Case 1 (state 2) once the swing's sprite is done: the Hunt's Attack, a sub-action, lets
/// nothing go (slot 0x68 with none), ends (`set_state` 1) and then posts 0x1fb with 600 and
/// 0x202 before it returns 5.
#[test]
fn a_sub_action_s_attack_after_its_kill_posts_0x1fb_and_returns_5() {
    let at = Position::new(11.0, 10.0, 0.0);
    let victim = Victim {
        id: object_id(7),
        state: ObjectState::Dying,
        position: at,
        ..Victim::default()
    };
    let mut rig = Rig::on(
        world_with(7, PlayerId::new(0), class::PREY, at),
        0.8,
        victim,
    )
    .holding(7, PlayerId::new(0));
    let mut a = soldier_attack(7, Distance::ZERO, 0.0, at);
    (a.state, a.seen, a.sub_action) = (AttackState::Killed, at, true);
    let code = rig.run(20, |c, u, f, r| {
        a.update(c, u, me(1, AttackState::Ready), f, r)
    });
    assert_eq!(code, Ok(UpdateCode::CODE_5));
    assert_eq!((a.state, a.target), (AttackState::Done, None));
    assert_eq!(
        rig.posted(),
        vec![(NotifyKind::ACTION_INVALIDATED, [600, 0, 0x202])]
    );
    assert_eq!(rig.mover().animated.speed, Speed::new(1.0));
    assert_eq!(rig.workers(7, PlayerId::new(0)), Some(1));
}

/// A dead tree (class 15) up to state 5 keeps a non-villager's attack going; ready, the
/// attack on it ends (`set_state` 1) and returns 5. A villager's, or one on a tree past
/// state 5, ends at the update's start, letting the target go at once.
#[test]
fn a_dead_tree_keeps_a_soldier_s_attack_until_it_is_ready() {
    let at = Position::new(11.0, 10.0, 0.0);
    let run = |unit_class: ClassId, tree_state: ObjectState| {
        let victim = Victim {
            id: object_id(7),
            class: class::TREE,
            state: tree_state,
            position: at,
            ..Victim::default()
        };
        let mut rig = Rig::on(
            world_with(7, PlayerId::new(0), class::TREE, at),
            0.8,
            victim,
        )
        .holding(7, PlayerId::new(0));
        rig.f.own.class = unit_class;
        let mut a = soldier_attack(7, Distance::new(1.0), 0.0, at);
        a.seen = at;
        let code = rig
            .run(20, |c, u, f, r| {
                a.update(c, u, me(1, AttackState::Ready), f, r)
            })
            .unwrap();
        (code, a.state, rig.workers(7, PlayerId::new(0)) == Some(0))
    };
    assert_eq!(
        run(class::CLASS_6, ObjectState::Dying),
        (UpdateCode::CODE_5, AttackState::Done, false)
    );
    assert_eq!(
        run(class::CLASS_6, ObjectState::Dead),
        (UpdateCode::CODE_5, AttackState::Done, false)
    );
    assert_eq!(
        run(class::VILLAGER, ObjectState::Dying),
        (UpdateCode::CODE_5, AttackState::Done, true)
    );
    assert_eq!(
        run(class::CLASS_6, ObjectState::Undead),
        (UpdateCode::CODE_5, AttackState::Done, true)
    );
}

/// After a hit, the swing's loop test reads the unit's sprite as readyToAttack left it: one
/// that failed (`set_state` 13, the standing sprite from its first frame) keeps the attack in
/// state 13, though the attack sprite had looped.
#[test]
fn a_swing_that_fails_its_interrupt_check_stays_failed() {
    let at = Position::new(15.0, 10.0, 0.0);
    let victim = Victim {
        id: object_id(7),
        state: ObjectState::Alive,
        position: at,
        ..Victim::default()
    };
    let mut rig = Rig::on(
        world_with(7, PlayerId::new(0), class::CLASS_6, at),
        0.0,
        victim,
    );
    let swing = Graphic {
        id: SpriteId::named(301),
        ..STANDING
    };
    rig.unit_mut().sprite = Some(ActiveSprite {
        looped: true,
        frame: crate::objects::Frame::of_data(9),
        ..ActiveSprite::start(swing, &mut Well512::default())
    });
    let mut a = soldier_attack(7, Distance::new(1.0), 0.0, Position::new(NONE, NONE, NONE));
    (a.state, a.seen, a.arms.interrupt_frame) =
        (AttackState::Swing, at, crate::objects::Frame::of_data(5));
    let code = rig.run(20, |c, u, f, r| {
        a.update(c, u, me(1, AttackState::Ready), f, r)
    });
    assert_eq!(code, Ok(UpdateCode::UNDER_WAY));
    assert_eq!(a.state, AttackState::Failed);
    assert_eq!(rig.posted(), vec![(NotifyKind::OUT_OF_REACH, [0; 3])]);
    assert_eq!(rig.sprite(), Some(STANDING.id));
}

#[test]
fn a_rangeless_attack_reaches_a_tenth_squared_in_f32() {
    // the image holds 0x3c23d70b (0.1f * 0.1f), one ULP above 0.01f
    assert_eq!(squared_reach(Distance::ZERO).get().to_bits(), 0x3c23_d70b);
    assert_eq!(
        squared_reach(Distance::new(NO_RANGE)).get().to_bits(),
        0x3c23_d70b
    );
    assert_eq!(squared_reach(Distance::new(2.0)), DistanceSquared::new(4.0));
}

/// 425906646's town centre 27171 (player 6) in update 6224: its Attack lost the boar 29826
/// and the town centre cannot attack (no missile, a firepower of at most 1): the Attack ends
/// (`set_state(1)`, the standing sprite at speed 1) and posts 0x1fb with 600 and its target,
/// −1.
#[test]
fn a_disarmed_building_s_targetless_attack_ends_with_its_notify() {
    let standing = Graphic {
        id: SpriteId::named(418),
        frames: crate::objects::Frame::of_data(1),
        facets: crate::objects::FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::ZERO,
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::NONE,
    };
    let mut a = Attack {
        target: None,
        target2: None,
        state: AttackState::Ready,
        timer: Seconds::ZERO,
        range: Distance::new(6.0),
        min_range: Distance::ZERO,
        point: Position::new(NONE, NONE, NONE),
        seen: Position::new(NONE, NONE, NONE),
        move_to: None,
        pending: false,
        swing: Seconds::ZERO,
        rest: Seconds::ZERO,
        arms: Arms {
            move_sprite: None,
            sprite: None,
            second_sprite: None,
            idle_sprite: Some(standing),
            idle_attack_sprite: None,
            missile: None,
            fire_frame: crate::objects::Frame::of_data(0),
            interrupt_frame: crate::objects::Frame::of_data(-1),
        },
        sub_action: false,
        on_target_moved: TargetMoved::Follows,
        same_owner: false,
    };
    let mut body = Body::Animated(Animated::new(Speed::new(0.5)));
    assert_eq!(
        a.clone()
            .update_targetless(Seconds::new(0.028), true, &mut body),
        Err(GenieError::unsupported(
            "an Attack without a target in a state but 1"
        ))
    );
    assert_eq!(
        a.update_targetless(Seconds::new(0.028), false, &mut body),
        Ok((UpdateCode::DONE, true))
    );
    assert_eq!(a.state, AttackState::Done);
    let (mut sprite, mut rng) = (None, Well512::default());
    let posted = a.disarmed(&mut sprite, &mut body, &mut rng).unwrap();
    assert_eq!(posted, Some((NotifyKind::ACTION_INVALIDATED, [600, -1, 0])));
    assert_eq!(
        (body.anim_speed(), sprite.map(|s| s.graphic.id)),
        (Some(Speed::new(1.0)), Some(SpriteId::named(418)))
    );
    // the Hunt's Attack, a sub-action, posts nothing
    a.sub_action = true;
    assert_eq!(a.disarmed(&mut sprite, &mut body, &mut rng).unwrap(), None);
}

/// `FUN_140a7c210`: without a `MoveTo` sprite of its own the Attack walks with the unit's
/// running sprite, else its walking one, slots 0x7d0 and 0x7c8: a terrain look's while the
/// unit has one; the attack sprite stays the master's.
#[test]
fn an_attack_walks_with_the_unit_s_running_or_walking_slot() {
    let g = |id| Graphic {
        id,
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let graphics: Graphics = [10, 11, 12, 20, 21]
        .into_iter()
        .map(|id| (SpriteId::named(id), g(SpriteId::named(id))))
        .collect();
    let master = Master {
        move_sprite: Some(SpriteId::named(10)),
        run_sprite: Some(SpriteId::named(11)),
        attack_sprite: Some(SpriteId::named(12)),
        ..Master::default()
    };
    let a = arms(&master, &graphics, TaskSprites::NONE, None);
    assert_eq!(a.move_sprite.map(|s| s.id), Some(SpriteId::named(11)));
    let look = crate::combat::abilities::Look {
        standing: None,
        walking: Some(SpriteId::named(20)),
        attacking: Some(SpriteId::named(21)),
        running: None,
        dying: None,
        undead: None,
    };
    let a = arms_looking(&master, Some(&look), &graphics, TaskSprites::NONE, None);
    assert_eq!(a.move_sprite.map(|s| s.id), Some(SpriteId::named(20)));
    assert_eq!(a.sprite.map(|s| s.id), Some(SpriteId::named(12)));
    // its own MoveTo sprite first
    let a = arms_looking(
        &master,
        Some(&look),
        &graphics,
        TaskSprites::NONE,
        Some(SpriteId::named(10)),
    );
    assert_eq!(a.move_sprite.map(|s| s.id), Some(SpriteId::named(10)));
}

/// The soldier at (10, 10), master 4, of speed `speed` whose unit AI may move to attack
/// (`can_move`) in stance `stance`, its leash centre on itself, and a target of class 6 `gap`
/// east of it; range 1, no least range.
fn engaged(gap: Distance, speed: f32, can_move: bool, stance: UnitStance) -> Engagement {
    Engagement {
        gap_squared: DistanceSquared::new(gap.squared().get()),
        gap,
        position: Position::new(10.0, 10.0, 0.0),
        target_position: Position::new(10.0 + gap.get(), 10.0, 0.0),
        speed: Speed::new(speed),
        range: Distance::new(1.0),
        min_range: Distance::ZERO,
        can_move,
        stance,
        state_position: Point::new(10.0, 10.0),
        world_time: WorldTime::ZERO,
        master: master_id(4),
        target_class: class::CLASS_6,
        same_owner: false,
    }
}

fn standing_arms() -> Arms {
    Arms {
        idle_sprite: Some(STANDING),
        ..own_attack(0, AttackState::New, Distance::ZERO).arms
    }
}

/// What the attack command's start found ([`Attack::own_start`] on target 7 of `owner`,
/// `visible` or not): the Attack, readyToAttack's answer and the notifies it posted.
#[derive(Debug)]
struct Started {
    attack: Attack,
    arrived: bool,
    posted: Vec<(NotifyKind, [i32; 3])>,
    ready: bool,
    failed: bool,
    failed_first: bool,
}

fn own_start(
    world: &Spatial,
    e: &Engagement,
    arms: Arms,
    visible: bool,
    owner: PlayerId,
    mover: &mut Mover,
) -> Result<Started, GenieError> {
    let cmd = AttackCommand {
        target: object_id(7),
        engagement: e,
        arms,
        visible,
        target_owner: owner,
        ready: None,
    };
    let reads = walking_reads(
        &NO_GRAPHICS,
        MoveDefaults::default(),
        ObjectState::Alive,
        e.speed.get(),
    );
    let (attack, begun, posted) = Attack::own_start(&cmd, world, mover, &reads)?;
    Ok(Started {
        attack,
        arrived: begun.arrived,
        posted,
        ready: begun.ready,
        failed: begun.failed,
        failed_first: begun.failed_first,
    })
}

/// The attack command's start of a unit that cannot move (no top speed, or a unit AI in
/// stance 2 without an order, 426049998's scout 1222): readyToAttack's ready end in reach
/// (the target's place the point, a NaN gap within) is `set_state(6)`; out of reach (0x1fd)
/// or below the least range (0x1fe with −1s) it fails (`set_state(13)`).
#[test]
fn the_start_of_a_unit_that_cannot_move_goes_ready_in_reach_and_fails_out_of_it() {
    let start = |e: Engagement| {
        let mut mover = soldier_mover(e.speed.get());
        let world = world_with(7, PlayerId::new(2), class::CLASS_6, e.target_position);
        own_start(
            &world,
            &e,
            standing_arms(),
            true,
            PlayerId::new(2),
            &mut mover,
        )
        .unwrap()
    };
    for e in [
        engaged(Distance::new(0.5), 1.2, false, UnitStance::STAND_GROUND),
        engaged(Distance::new(0.5), 0.0, true, UnitStance::AGGRESSIVE),
        engaged(
            Distance::new(f32::NAN),
            1.2,
            false,
            UnitStance::STAND_GROUND,
        ),
    ] {
        let c = start(e);
        assert_eq!(
            (c.ready, c.failed, c.failed_first, c.arrived, c.posted),
            (true, false, false, false, vec![])
        );
        assert_eq!(
            c.attack.point.get().map(f32::to_bits),
            e.target_position.get().map(f32::to_bits)
        );
        assert_eq!(c.attack.move_to, None);
    }
    let away = start(engaged(
        Distance::new(3.0),
        1.2,
        false,
        UnitStance::STAND_GROUND,
    ));
    assert_eq!(
        (away.ready, away.failed, away.posted),
        (false, true, vec![(NotifyKind::OUT_OF_REACH, [0; 3])])
    );
    assert_eq!(away.attack.point, Position::new(NONE, NONE, NONE));
    let close = start(Engagement {
        min_range: Distance::new(1.0),
        ..engaged(Distance::new(0.5), 1.2, false, UnitStance::STAND_GROUND)
    });
    assert_eq!(
        (close.ready, close.failed, close.posted),
        (false, true, vec![(NotifyKind::TOO_CLOSE, [-1; 3])])
    );
}

/// The start of a unit that may move ends in `set_state(4)` whatever readyToAttack did: below
/// the least range after its 0x1fe; out of reach after its own `set_state(4)` (whose `MoveTo`
/// the second one's teardown stops); on a target out of sight after its `set_state(5)` (stance
/// 0, its `MoveTo` stopped too) or its `set_state(13)` (another stance), which leaves the
/// animation at speed 1; stance 1's leash fails a target 5 past the range from the unit AI's
/// point and holds a nearer one.
#[test]
fn the_start_of_a_unit_that_may_move_turns_to_state_4_whatever_ready_to_attack_found() {
    let start = |e: Engagement, visible: bool| {
        let mut mover = soldier_mover(e.speed.get());
        mover.animated.speed = Speed::new(1.0);
        let world = world_with(7, PlayerId::new(2), class::CLASS_6, e.target_position);
        let c = own_start(
            &world,
            &e,
            standing_arms(),
            visible,
            PlayerId::new(2),
            &mut mover,
        )
        .unwrap();
        assert_eq!(
            (c.attack.state, c.ready, c.failed),
            (AttackState::Move, false, false)
        );
        assert_eq!(c.attack.move_to.map(|m| m.state), Some(MoveTo::Start));
        (
            c.failed_first,
            c.posted,
            mover.animated.speed == Speed::new(0.0),
        )
    };
    assert_eq!(
        start(
            engaged(Distance::new(0.5), 1.2, true, UnitStance::AGGRESSIVE),
            true
        ),
        (false, vec![], false)
    );
    let close = Engagement {
        min_range: Distance::new(1.0),
        ..engaged(Distance::new(0.5), 1.2, true, UnitStance::AGGRESSIVE)
    };
    assert_eq!(
        start(close, true),
        (false, vec![(NotifyKind::TOO_CLOSE, [-1; 3])], false)
    );
    assert_eq!(
        start(
            engaged(Distance::new(3.0), 1.2, true, UnitStance::AGGRESSIVE),
            true
        ),
        (false, vec![(NotifyKind::OUT_OF_REACH, [0; 3])], true)
    );
    assert_eq!(
        start(
            engaged(Distance::new(3.0), 1.2, true, UnitStance::AGGRESSIVE),
            false
        ),
        (false, vec![], true)
    );
    assert_eq!(
        start(
            engaged(Distance::new(3.0), 1.2, true, UnitStance::STAND_GROUND),
            false
        ),
        (true, vec![], false)
    );
    // stance 1: the target 3 from the centre stays within 1 + 5, 7 from it does not
    assert_eq!(
        start(
            engaged(Distance::new(3.0), 1.2, true, UnitStance::DEFENSIVE),
            true
        ),
        (false, vec![(NotifyKind::OUT_OF_REACH, [0; 3])], true)
    );
    let far = Engagement {
        state_position: Point::new(3.0, 10.0),
        ..engaged(Distance::new(3.0), 1.2, true, UnitStance::DEFENSIVE)
    };
    assert_eq!(
        start(far, true),
        (true, vec![(NotifyKind::OUT_OF_REACH, [0; 3])], false)
    );
    // a target out of sight of a unit that cannot follow it fails
    let mut mover = soldier_mover(1.2);
    let e = engaged(Distance::new(3.0), 1.2, false, UnitStance::STAND_GROUND);
    let world = world_with(7, PlayerId::new(2), class::CLASS_6, e.target_position);
    let c = own_start(
        &world,
        &e,
        standing_arms(),
        false,
        PlayerId::new(2),
        &mut mover,
    )
    .unwrap();
    assert_eq!((c.failed, c.ready, c.posted), (true, false, vec![]));
}

/// readyToAttack's ready end turns the unit to its target (`FUN_141162db0`) but for masters
/// 1704 and 1706.
#[test]
fn the_start_faces_the_target_but_for_masters_1704_and_1706() {
    let north = Engagement {
        target_position: Position::new(10.0, 10.5, 0.0),
        ..engaged(Distance::new(0.5), 0.0, false, UnitStance::STAND_GROUND)
    };
    let world = world_with(7, PlayerId::new(2), class::CLASS_6, north.target_position);
    let angle = |master| {
        let mut mover = soldier_mover(0.0);
        mover.angle = Angle::ZERO;
        let e = Engagement { master, ..north };
        let _ = own_start(
            &world,
            &e,
            standing_arms(),
            true,
            PlayerId::new(2),
            &mut mover,
        )
        .unwrap();
        mover.angle
    };
    assert_ne!(angle(master_id(4)), Angle::ZERO);
    assert_eq!(angle(master_id(1704)), Angle::ZERO);
    assert_eq!(angle(master_id(1706)), Angle::ZERO);
}

/// A gate's reach (class 39): the nearest of it and its first two parts (slot 0x680 in id
/// order, `MINSS`), in the start's readyToAttack and an update's; the least range reads the
/// gate's own gap. A gate 3 tiles off with a part 0.5 off is in reach of a range of 1.
#[test]
fn a_gate_s_reach_is_to_its_nearest_part() {
    let gate_at = Position::new(13.4, 10.0, 0.0);
    let gated = || {
        let mut world = world_with(7, PlayerId::new(0), class::GATE, gate_at);
        for (id, x) in [(8, 10.9), (9, 20.0), (10, 10.5)] {
            world.insert(
                object_id(id),
                crate::map::spatial::Placed {
                    position: Position::new(x, 10.0, 0.0),
                    ..*world.get(object_id(7)).unwrap()
                },
            );
            world.set_annex(object_id(id), Some(MainBuildingId::new(object_id(7))));
        }
        world
    };
    let world = gated();
    let e = Engagement {
        target_class: class::GATE,
        ..engaged(Distance::new(3.0), 0.0, false, UnitStance::STAND_GROUND)
    };
    let mut mover_body = Body::new(
        Position::new(10.0, 10.0, 0.0),
        None,
        Some(soldier_mover(0.0)),
    );
    let c = own_start(
        &world,
        &e,
        standing_arms(),
        true,
        PlayerId::new(2),
        mover_body.mover_mut().unwrap(),
    )
    .unwrap();
    // the part 8 is 0.5 off (its footprint 0.1); the part 10, the third, is not read
    assert_eq!((c.ready, c.failed, c.posted), (true, false, vec![]));
    // without its parts the gate is out of reach
    let bare = world_with(7, PlayerId::new(0), class::GATE, gate_at);
    let mut mover = soldier_mover(0.0);
    let c = own_start(
        &bare,
        &e,
        standing_arms(),
        true,
        PlayerId::new(2),
        &mut mover,
    )
    .unwrap();
    assert_eq!((c.ready, c.failed), (false, true));
    // an update's readyToAttack the same way
    let victim = Victim {
        id: object_id(7),
        position: gate_at,
        ..Victim::default()
    };
    let mut rig = Rig::on(gated(), 0.0, victim);
    rig.r.can_move = false;
    let mut a = soldier_attack(7, Distance::new(1.0), 0.0, Position::new(NONE, NONE, NONE));
    let r = rig
        .ground(|u, g, f, r| a.ready_to_attack(u, g, f, r, false))
        .unwrap();
    assert!(matches!(r, Readiness::Ready));
    assert_eq!(a.point, gate_at);
}

/// `set_state(6)` at the start of a unit that cannot move: readyToAttack again finds the
/// target on the point; facing it with its attack timer run out the unit swings
/// (`set_state(7)`: the attack sprite, a hit pending), else it waits in state 6. Without a
/// target or a point a unit's own Attack posts 0x1fa with 600 and ends (`set_state(1)`).
#[test]
fn the_ready_start_swings_at_once_when_its_timer_ran_out() {
    let at = Position::new(10.5, 10.0, 0.0);
    let attack_sprite = Graphic {
        id: SpriteId::named(301),
        ..STANDING
    };
    let ready = |timer: f32| {
        let victim = Victim {
            id: object_id(7),
            position: at,
            ..Victim::default()
        };
        let mut rig = Rig::on(
            world_with(7, PlayerId::new(0), class::CLASS_6, at),
            0.0,
            victim,
        );
        rig.unit_mut().combat.attack_timer = Seconds::new(timer);
        rig.r.can_move = false;
        let mut a = soldier_attack(7, Distance::new(1.0), 0.0, at);
        a.arms.sprite = Some(attack_sprite);
        a.state = AttackState::New;
        rig.ground(|u, g, f, r| a.enter_ready(u, g, f, r)).unwrap();
        assert_eq!(rig.posted(), vec![]);
        (a.state, a.pending, rig.sprite())
    };
    assert_eq!(
        ready(0.0),
        (AttackState::Swing, true, Some(SpriteId::named(301)))
    );
    assert_eq!(ready(0.5), (AttackState::Ready, false, None));
    let mut rig = Rig::on(
        world_with(7, PlayerId::new(0), class::CLASS_6, at),
        0.0,
        Victim::default(),
    );
    let mut a = soldier_attack(-1, Distance::new(1.0), 0.0, Position::from([NONE; 3]));
    rig.ground(|u, g, f, r| a.enter_ready(u, g, f, r)).unwrap();
    assert_eq!(a.state, AttackState::Done);
    assert_eq!(
        rig.posted(),
        vec![(NotifyKind::ACTION_COMPLETED, [600, 0, 0])]
    );
}

/// `set_state(7)`'s sprite: a second attack sprite makes the swing draw the Object stream
/// (`FUN_140810fc0` with 2: 0 the first sprite, 1 the second); the terrain look's attack
/// sprite comes first and draws nothing, and so does a special graphic on a charge that
/// hits.
#[test]
fn a_swing_with_a_second_sprite_draws_which_one_it_shows() {
    let at = Position::new(10.5, 10.0, 0.0);
    let sprite = |id: i16| Graphic {
        id: SpriteId::named(id),
        ..STANDING
    };
    let swing = |object: Well512, sprites: SwingSprites| {
        let victim = Victim {
            id: object_id(7),
            position: at,
            ..Victim::default()
        };
        let mut rig = Rig::on(
            world_with(7, PlayerId::new(0), class::CLASS_6, at),
            0.0,
            victim,
        );
        *rig.t.streams.get(Stream::Object) = object;
        rig.r.can_move = false;
        rig.r.swing_sprites = sprites;
        let mut a = soldier_attack(7, Distance::new(1.0), 0.0, at);
        a.arms.sprite = Some(sprite(301));
        a.arms.second_sprite = Some(sprite(302));
        a.state = AttackState::New;
        rig.ground(|u, g, f, r| a.enter_ready(u, g, f, r)).unwrap();
        (rig.sprite(), rig.t.streams.get(Stream::Object).clone())
    };
    // two seeds whose first draw scales to 0 and to 1
    let seeded = |seed: u32| {
        let mut r = Well512::default();
        r.set_seed(seed);
        r
    };
    let first = |seed: u32| seeded(seed).below_checked(2);
    let (zero, one) = (
        (0..64).find(|&s| first(s) == 0).unwrap(),
        (0..64).find(|&s| first(s) == 1).unwrap(),
    );
    let (shown, left) = swing(seeded(zero), SwingSprites::default());
    let mut drawn = seeded(zero);
    let _ = drawn.below_checked(2);
    assert_eq!((shown, left), (Some(SpriteId::named(301)), drawn));
    assert_eq!(
        swing(seeded(one), SwingSprites::default()).0,
        Some(SpriteId::named(302))
    );
    // the look's sprite draws nothing
    let looked = SwingSprites {
        look: Some(sprite(303)),
        special: None,
    };
    assert_eq!(
        swing(seeded(one), looked),
        (Some(SpriteId::named(303)), seeded(one))
    );
    // a special graphic without a charge that hits draws as before
    let special = SwingSprites {
        look: None,
        special: Some(sprite(304)),
    };
    assert_eq!(swing(seeded(one), special).0, Some(SpriteId::named(302)));
}

/// readyToAttack's leash in an update: a unit in stance 1 whose target stands the range
/// plus 5 or more from its unit AI's `state_position` fails (`set_state(13)`) after its
/// 0x1fd; a nearer one follows. A NaN distance holds (`COMISS`/`JNC`).
#[test]
fn ready_to_attack_s_leash_fails_a_target_too_far_from_the_unit_ai_s_point() {
    let at = Position::new(15.0, 10.0, 0.0);
    for (centre, follows) in [
        (Point::new(10.0, 10.0), true),
        (Point::new(9.0, 10.0), false),
        (Point::new(f32::NAN, 10.0), true),
    ] {
        let victim = Victim {
            id: object_id(7),
            position: at,
            ..Victim::default()
        };
        let mut rig = Rig::on(
            world_with(7, PlayerId::new(0), class::CLASS_6, at),
            0.8,
            victim,
        );
        (rig.r.stance, rig.r.state_position) = (UnitStance::DEFENSIVE, centre);
        let mut a = soldier_attack(7, Distance::new(1.0), 0.0, Position::new(NONE, NONE, NONE));
        let r = rig
            .ground(|u, g, f, r| a.ready_to_attack(u, g, f, r, false))
            .unwrap();
        assert_eq!(rig.posted(), vec![(NotifyKind::OUT_OF_REACH, [0; 3])]);
        assert_eq!(matches!(r, Readiness::Away(_)), follows, "{centre:?}");
        assert_eq!(
            a.state,
            if follows {
                AttackState::Ready
            } else {
                AttackState::Failed
            }
        );
    }
}

/// The unit's own Attack in its update without a target ([`Unit::attack_part`]): a building
/// that cannot attack (`armed` false) ends it with its standing sprite at speed 1, posts 0x1fb
/// with 600 and no target, and the Attack goes (code 1). An Attack whose target the world does
/// not hold live is not modelled.
///
/// [`Unit::attack_part`]: crate::objects::unit::Unit::attack_part
#[test]
fn a_disarmed_building_s_targetless_attack_ends_standing_and_posts_0x1fb() {
    use crate::world::tests::{forage_world, in_update};
    let stand = Graphic {
        id: SpriteId::named(9),
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(1),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let me = |id| Me {
        id,
        group: None,
        action_state: ActionState::NO_ACTION,
    };
    let mut t = forage_world();
    // the town centre's master without a missile cannot attack
    for masters in &mut t.masters {
        masters.get_mut(ids::TOWN_CENTRE).unwrap().missile = MasterField::NONE;
    }
    let centre = &mut t.players[1].objects[ListIndex::new(2)];
    let mut attack = own_attack(-1, AttackState::Done, Distance::ZERO);
    attack.arms.idle_sprite = Some(stand);
    centre.actions.list.set_attack(Some(attack));
    // its unit AI on the attack's action, which takes the Attack's invalidation
    centre.combat.ai = Some(UnitAi {
        current_action: Some(ActionKind::Attack),
        ..UnitAi::default()
    });
    in_update(&mut t, (PlayerId::new(1), ListIndex::new(2)), 50, |u, c| {
        let code = u.attack_part(c, me(object_id(700)), None)?;
        u.end_head_update(c, code)
    })
    .unwrap();
    let centre = &t.players[1].objects[ListIndex::new(2)];
    assert_eq!(centre.actions.list.attack().copied(), None);
    assert_eq!(
        (
            centre.sprite.map(|s| s.graphic.id),
            centre.body.anim_speed()
        ),
        (Some(SpriteId::named(9)), Some(Speed::new(1.0)))
    );
    let posted = &centre.combat.ai.as_ref().unwrap().notifies;
    assert!(
        posted
            .iter()
            .any(|n| n.kind == NotifyKind::ACTION_INVALIDATED && n.p == [600, -1, 0])
    );
    let mut t = forage_world();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .set_attack(Some(own_attack(77, AttackState::Done, Distance::ZERO)));
    let r = in_update(&mut t, (PlayerId::new(1), ListIndex::new(0)), 50, |u, c| {
        let code = u.attack_part(c, me(object_id(9175)), None)?;
        u.end_head_update(c, code)
    });
    assert_eq!(
        r.unwrap_err().reason(),
        "an attack's target the slice does not hold live"
    );
}

/// A sprite of [`STANDING`]'s graphic (ten frames of 0.1 s) that just advanced from frame
/// `last` to `frame`.
fn at_frame(frame: i16, last: i16) -> ActiveSprite {
    ActiveSprite {
        frame: crate::objects::Frame::of_data(frame),
        last_frame: crate::objects::Frame::of_data(last),
        changed: true,
        looped: false,
        ..ActiveSprite::start(STANDING, &mut Well512::default())
    }
}

/// [`own_attack`] on the house 10028 in state `state`, its swing pending, with the standing
/// sprite to end with and a missile (`ranged`) firing at frame `fire`.
fn swinging_attack(state: AttackState, ranged: bool, fire: i16) -> Attack {
    let mut a = own_attack(10028, state, Distance::new(0.1));
    a.pending = true;
    a.arms.idle_sprite = Some(STANDING);
    a.arms.missile = if ranged { Some(master_id(5)) } else { None };
    a.arms.fire_frame = crate::objects::Frame::of_data(fire);
    a
}

/// The swing before its hit ([`Attack::swing_to_hit`]): a ranged swing waits for its firing
/// frame without counting down, then fires its volley at its target at once
/// ([`Attack::land_hit`]: [`archer_world`]'s archer makes its arrow): the attack timer is the
/// reload less the time to that frame, the rest of the reload past the sprite the swing's; a
/// melee swing counts down by the update's dt toward half its frames.
///
/// [`archer_world`]: crate::world::tests::archer_world
#[test]
fn a_ranged_swing_fires_at_its_firing_frame_and_a_melee_one_counts_down() {
    use crate::world::tests::{archer_world, missiles};
    let mut t = archer_world(100, 0, 0.0);
    let mut a = *t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .attack()
        .unwrap();
    a.pending = true;
    a.state = AttackState::Swing;
    a.swing = Seconds::new(1.0);
    let deer = Victim {
        id: object_id(50),
        hp: HitPoints::new(5.0),
        position: Position::new(13.0, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        player: PlayerId::new(0),
        ..Victim::default()
    };
    let mut f = Fight {
        victim: deer,
        own: Victim {
            id: object_id(123),
            ..soldier()
        },
    };
    let r = reads();
    let mut swing_at = |t: &mut World, a: &mut Attack, frame: i16| {
        let sprite = ActiveSprite {
            frame: crate::objects::Frame::of_data(frame),
            last_frame: crate::objects::Frame::of_data(frame - 1),
            changed: true,
            looped: false,
            ..ActiveSprite::start(
                *t.graphics.get(SpriteId::named(301)).unwrap(),
                &mut Well512::default(),
            )
        };
        crate::world::tests::in_update(t, (PlayerId::new(1), ListIndex::new(0)), 100, |u, c| {
            a.swing_to_hit(c, u, &mut f, &r, sprite)
        })
    };
    swing_at(&mut t, &mut a, 4).unwrap();
    assert_eq!(
        (a.pending, a.swing, missiles(&t).len()),
        (true, Seconds::new(1.0), 0)
    );
    swing_at(&mut t, &mut a, 5).unwrap();
    assert_eq!((a.pending, missiles(&t).len()), (false, 1));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].combat.attack_timer,
        Seconds::new(2.0 - 5.0 * 0.1)
    );
    let rest = 2.0 - 10.0 * 0.1;
    assert_eq!((a.rest, a.swing), (Seconds::new(rest), Seconds::new(rest)));
    let mut rig = Rig::on_the_path(Victim {
        hp: HitPoints::new(50.0),
        ..Victim::default()
    });
    let mut melee = swinging_attack(AttackState::Swing, false, 0);
    melee.swing = Seconds::new(1.0);
    rig.run(100, |c, u, f, r| {
        melee.swing_to_hit(c, u, f, r, at_frame(4, 3))
    })
    .unwrap();
    assert_eq!(
        (melee.pending, melee.swing),
        (true, Seconds::new(1.0 - 0.1))
    );
}

/// The swing's unmodelled parts: class 51's swing, a melee swing of class 35, a ranged one
/// without a firing frame and a self-destructing attacker's melee hit; and a ranged hit on a
/// target the strike reads dead ends the swing (`set_state` 2).
#[test]
fn a_swing_s_unmodelled_parts_fail_and_a_killing_hit_ends_it() {
    let reason = |class, master, ranged, fire| {
        let mut rig = Rig::on_the_path(Victim {
            hp: HitPoints::new(50.0),
            ..Victim::default()
        });
        (rig.f.own.class, rig.f.own.master_id) = (class, master);
        let mut a = swinging_attack(AttackState::Swing, ranged, fire);
        rig.run(100, |c, u, f, r| a.swing_to_hit(c, u, f, r, at_frame(5, 4)))
            .unwrap_err()
            .reason()
            .to_owned()
    };
    assert_eq!(
        reason(class::CLASS_51, master_id(4), true, 3),
        "a swing of class 35, 51 or 64"
    );
    assert_eq!(
        reason(class::CLASS_35, master_id(4), false, 0),
        "a swing of class 35, 51 or 64"
    );
    assert_eq!(
        reason(class::CLASS_6, master_id(4), true, -1),
        "a ranged swing without a firing frame"
    );
    assert_eq!(
        reason(class::CLASS_6, ids::ID_527, false, 0),
        "a self-destructing attacker's hit (masters 527, 528, 1104, 2133..2135)"
    );
    let mut t = crate::world::tests::archer_world(100, 0, 0.0);
    let mut a = *t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .attack()
        .unwrap();
    (a.pending, a.state) = (true, AttackState::Swing);
    let mut f = Fight {
        victim: Victim {
            id: object_id(50),
            ..Victim::default()
        },
        own: Victim {
            id: object_id(123),
            ..soldier()
        },
    };
    let r = reads();
    let sprite = ActiveSprite {
        frame: crate::objects::Frame::of_data(5),
        last_frame: crate::objects::Frame::of_data(4),
        changed: true,
        ..ActiveSprite::start(
            *t.graphics.get(SpriteId::named(301)).unwrap(),
            &mut Well512::default(),
        )
    };
    crate::world::tests::in_update(
        &mut t,
        (PlayerId::new(1), ListIndex::new(0)),
        100,
        |u, c| a.land_hit(c, u, &mut f, &r, sprite),
    )
    .unwrap();
    assert_eq!((a.pending, a.state), (false, AttackState::Killed));
    assert_eq!(crate::world::tests::missiles(&t).len(), 1);
}

/// The swing after its hit ([`Attack::swing_after_hit`]): a ranged swing whose timer ran out
/// without a target ends (`set_state` 2), with one past its interrupt frame readies
/// (`set_state` 6, whose readyToAttack finds the house out of sight: the unit goes to where it
/// saw it, state 5); one going on waits for its sprite to loop; a looped sprite with reload
/// left shows the idle sprite at speed 1 and readies.
#[test]
fn a_swing_after_its_hit_ends_readies_or_waits_for_its_sprite() {
    let after = |target, interrupt, sprite: ActiveSprite, swing| {
        let mut rig = Rig::on_the_path(Victim::default());
        let mut a = swinging_attack(AttackState::Swing, true, 3);
        (a.pending, a.target, a.swing) = (false, target, swing);
        a.arms.interrupt_frame = interrupt;
        rig.run(100, |c, u, f, r| a.swing_after_hit(c, u, f, r, sprite))
            .unwrap();
        (a.state, rig.mover().animated.speed)
    };
    assert_eq!(
        after(
            None,
            crate::objects::Frame::of_data(5),
            at_frame(9, 8),
            Seconds::ZERO
        )
        .0,
        AttackState::Killed
    );
    assert_eq!(
        after(
            Some(object_id(10028)),
            crate::objects::Frame::of_data(5),
            at_frame(6, 5),
            Seconds::ZERO
        )
        .0,
        AttackState::GotoSeen
    );
    assert_eq!(
        after(
            Some(object_id(10028)),
            crate::objects::Frame::of_data(-1),
            at_frame(3, 2),
            Seconds::ZERO
        )
        .0,
        AttackState::Swing
    );
    let looped = ActiveSprite {
        looped: true,
        ..at_frame(0, 9)
    };
    assert_eq!(
        after(
            Some(object_id(10028)),
            crate::objects::Frame::of_data(-1),
            looped,
            Seconds::new(0.5)
        ),
        (AttackState::GotoSeen, Speed::new(1.0))
    );
}

/// The house 10028's place, which the builder 9177's path leads to.
const HOUSE: Position = Position::new(30.0, 88.0, 0.0);

/// A quiet `MoveTo` in `state` to the house's place with the wait `wait`.
fn walk_to_house(state: MoveTo, wait: MoveWait) -> MoveToAction {
    MoveToAction {
        point: HOUSE,
        wait,
        ..MoveToAction::sub_in(state)
    }
}

/// The sub-action list's update ([`Attack::update_list`]) on the house 10028: after a wait `+0x6c`
/// goes from 1 to 2, a running `MoveTo` shows its move sprite (the master's running one) as it
/// moves, whose factor sets the top speed (0.8 × 1.5); one in range at once arrives (its code is
/// still 0), the quiet `MoveTo` showing no idle sprite.
#[test]
fn an_attack_s_move_to_starts_running_or_arrives_in_range() {
    let walking = Graphic {
        id: SpriteId::named(301),
        speed_multiplier: SpeedMultiplier::of_data(1.5),
        ..STANDING
    };
    let start = |range| {
        let mut rig = Rig::on_the_path(Victim::default());
        rig.mover_mut().animated.speed = Speed::new(1.0);
        let mut a = own_attack(10028, AttackState::Move, range);
        let wait = MoveWait {
            state: MoveWaitState::Waiting,
            ..MoveWait::NEW
        };
        a.move_to = Some(MoveToAction {
            range,
            sprite: Some(walking),
            ..walk_to_house(MoveTo::Start, wait)
        });
        a.arms.idle_sprite = Some(STANDING);
        let code = rig
            .run(100, |c, u, _, r| {
                a.update_list(c, u, me(9177, AttackState::Move), r, GiveUp::default())
            })
            .unwrap();
        let m = rig.mover();
        (
            code,
            a.move_to.map(|m| (m.state, m.wait.state)),
            rig.sprite(),
            m.animated.speed,
            m.stats.speed,
        )
    };
    assert_eq!(
        start(Distance::new(0.1)),
        (
            UpdateCode::UNDER_WAY,
            Some((MoveTo::Moving, MoveWaitState::AskedAgain)),
            Some(SpriteId::named(301)),
            Speed::new(1.0),
            Speed::new(1.2)
        )
    );
    let (code, move_to, shown, anim, _) = start(Distance::new(5.0));
    assert_eq!(
        (code, move_to, shown, anim),
        (
            UpdateCode::UNDER_WAY,
            Some((MoveTo::Arrived, MoveWaitState::AskedAgain)),
            None,
            Speed::new(1.0)
        )
    );
}

/// `set_state(4)`'s `MoveTo` to the target ([`Attack::walk_to_target`]) takes the Attack's range,
/// walking sprite and `+0x61`, the target's place and its target; it starts arrived within 0.01
/// of the target, in state 3 with speed, 13 without; a unit whose state is 3 or more adds none.
#[test]
fn an_attack_s_move_to_the_target_takes_its_range_sprite_and_target_moved_mode() {
    let at = Position::new(12.0, 10.0, 0.0);
    let table = Graphics::from([(STANDING.id, STANDING)]);
    let made = |gap, speed, state, mode| {
        let mut a = own_attack(7, AttackState::Move, Distance::new(2.0));
        a.arms.move_sprite = Some(STANDING);
        a.on_target_moved = mode;
        a.timer = Seconds::new(5.0);
        let reads = walking_reads(&table, MoveDefaults::default(), state, speed);
        let arrived = a.walk_to_target(at, gap, &reads).unwrap();
        (arrived, a.timer, a.move_to)
    };
    let (arrived, timer, m) = made(
        Distance::new(1.0),
        1.0,
        ObjectState::Alive,
        TargetMoved::AsksAgain,
    );
    let m = m.unwrap();
    assert_eq!((arrived, timer), (false, Seconds::ZERO));
    assert_eq!(
        (
            m.state,
            m.point,
            ObjectId::field(m.target),
            m.range,
            m.sprite,
            m.quiet
        ),
        (
            MoveTo::Start,
            at,
            7,
            Distance::new(2.0),
            Some(STANDING),
            true
        )
    );
    assert_eq!(m.on_target_moved, TargetMoved::AsksAgain);
    let (arrived, _, m) = made(
        Distance::new(0.01),
        1.0,
        ObjectState::Alive,
        TargetMoved::Follows,
    );
    assert_eq!(
        (arrived, m.map(|m| (m.state, m.on_target_moved))),
        (true, Some((MoveTo::Arrived, TargetMoved::Follows)))
    );
    let (arrived, _, m) = made(
        Distance::new(1.0),
        0.0,
        ObjectState::Alive,
        TargetMoved::Follows,
    );
    assert_eq!((arrived, m.map(|m| m.state)), (false, Some(MoveTo::Failed)));
    let (arrived, _, m) = made(
        Distance::ZERO,
        1.0,
        ObjectState::Dying,
        TargetMoved::Follows,
    );
    assert_eq!((arrived, m), (false, None));
    let mut none = own_attack(-1, AttackState::Move, Distance::new(2.0));
    let reads = walking_reads(&table, MoveDefaults::default(), ObjectState::Alive, 1.0);
    assert!(none.walk_to_target(at, Distance::new(1.0), &reads).is_err());
    // without a sprite of its own the constructor's default is taken
    let defaults = MoveDefaults {
        point: Some(STANDING.id),
        object: Some(STANDING.id),
    };
    let reads = walking_reads(&table, defaults, ObjectState::Alive, 1.0);
    let mut bare = own_attack(7, AttackState::Move, Distance::new(2.0));
    let _ = bare.walk_to_target(at, Distance::new(1.0), &reads).unwrap();
    assert_eq!(bare.move_to.and_then(|m| m.sprite), Some(STANDING));
    let mut seen = own_attack(7, AttackState::GotoSeen, Distance::new(2.0));
    seen.walk_to_seen(&reads).unwrap();
    assert_eq!(seen.move_to.and_then(|m| m.sprite), Some(STANDING));
}

/// `set_state(5)` and `(11)`'s `MoveTo` to where the target was seen ([`Attack::walk_to_seen`]):
/// state 5 within the Attack's range, state 11 onto the point (range 0); the point's gap is none,
/// so state 3, or 13 for a unit without speed; none for a unit whose state is 3 or more.
#[test]
fn an_attack_s_move_to_where_it_saw_the_target_walks_within_its_range_or_onto_it() {
    let seen = Position::new(12.0, 10.0, 0.0);
    let made = |state, speed, unit_state| {
        let mut a = own_attack(7, state, Distance::new(3.0));
        a.seen = seen;
        a.timer = Seconds::new(5.0);
        let reads = walking_reads(&NO_GRAPHICS, MoveDefaults::default(), unit_state, speed);
        a.walk_to_seen(&reads).unwrap();
        (
            a.timer,
            a.move_to
                .map(|m| (m.state, m.point, m.range, ObjectId::field(m.target))),
        )
    };
    assert_eq!(
        made(AttackState::GotoSeen, 1.0, ObjectState::Alive),
        (
            Seconds::ZERO,
            Some((MoveTo::Start, seen, Distance::new(3.0), -1))
        )
    );
    assert_eq!(
        made(AttackState::GotoPoint, 1.0, ObjectState::Alive),
        (
            Seconds::ZERO,
            Some((MoveTo::Start, seen, Distance::ZERO, -1))
        )
    );
    assert_eq!(
        made(AttackState::GotoSeen, 0.0, ObjectState::Alive)
            .1
            .map(|m| m.0),
        Some(MoveTo::Failed)
    );
    assert_eq!(made(AttackState::GotoSeen, 1.0, ObjectState::Dying).1, None);
}

/// State 4's update ([`Attack::walk`]) on the house 10028 out of reach: a walking `MoveTo`
/// walks on; a wait runs by the update's dt and asks again at its delay; an arrived `MoveTo`
/// goes and stops the unit, which readies (the house out of sight: it goes to where it saw it);
/// a failed one ends the Attack (code 3), a unit's own posting 0x1f9 with 600. An empty list
/// gives code 1 as an arrived one does.
#[test]
fn an_attack_s_walk_walks_waits_arrives_or_fails() {
    let walked = |move_to: Option<MoveTo>, timer: f32| {
        let mut rig = Rig::on_the_path(Victim::default());
        let mut a = own_attack(10028, AttackState::Move, Distance::new(0.1));
        a.arms.idle_sprite = Some(STANDING);
        let wait = MoveWait {
            state: MoveWaitState::Waiting,
            timer: Seconds::new(timer),
            delay: Seconds::new(0.15),
        };
        a.move_to = move_to.map(|state| walk_to_house(state, wait));
        let code = rig
            .run(100, |c, u, f, r| {
                a.walk(
                    c,
                    u,
                    me(9177, AttackState::Move),
                    f,
                    r,
                    (HOUSE, Footprint::new(1.0, 1.0)),
                )
            })
            .map_err(|e| e.reason().to_owned());
        (
            code,
            a,
            rig.posted(),
            rig.mover().animated.speed == Speed::new(0.0),
        )
    };
    let (code, a, posted, _) = walked(Some(MoveTo::Moving), 0.0);
    assert_eq!(
        (code, a.move_to.map(|m| m.state), a.state, posted),
        (
            Ok(UpdateCode::UNDER_WAY),
            Some(MoveTo::Moving),
            AttackState::Move,
            vec![]
        )
    );
    let (_, a, ..) = walked(Some(MoveTo::Waiting), 0.0);
    assert_eq!(
        a.move_to.map(|m| (m.state, m.wait.timer)),
        Some((MoveTo::Waiting, Seconds::new(0.1)))
    );
    let (_, a, ..) = walked(Some(MoveTo::Waiting), 0.1);
    assert_eq!(
        a.move_to.map(|m| (m.state, m.wait.timer)),
        Some((MoveTo::Start, Seconds::ZERO))
    );
    let (code, a, _, stopped) = walked(Some(MoveTo::Arrived), 0.0);
    assert_eq!(
        (code, a.state, stopped),
        (Ok(UpdateCode::UNDER_WAY), AttackState::GotoSeen, true)
    );
    let (code, a, _, _) = walked(None, 0.0);
    assert_eq!(
        (code, a.state),
        (Ok(UpdateCode::UNDER_WAY), AttackState::GotoSeen)
    );
    let (code, a, posted, _) = walked(Some(MoveTo::Failed), 0.0);
    assert_eq!(
        (code, a.state, posted),
        (
            Ok(UpdateCode::FAILED),
            AttackState::Failed,
            vec![(NotifyKind::ACTION_FAILED, [600, 0, 0])]
        )
    );
}

/// The update of an Attack without a target in state 5 or 11 ([`Attack::update_gone`]): its
/// `MoveTo` walks to where the target was seen (code 0); arrived it ends the Attack (code 1, a
/// unit's own posting 0x1fb with 600 and 0x202 first) and stops the unit; failed it fails the
/// Attack (code 3, a unit's own posting 0x1f9 with 600).
#[test]
fn an_attack_without_a_target_walks_to_where_it_saw_it_and_ends_there() {
    let gone = |state: MoveTo, sub_action: bool| {
        let mut rig = Rig::on_the_path(Victim::default());
        let mut a = own_attack(-1, AttackState::GotoSeen, Distance::new(1.0));
        a.arms.idle_sprite = Some(STANDING);
        a.sub_action = sub_action;
        a.move_to = Some(walk_to_house(state, MoveWait::default()));
        let code = rig
            .run(100, |c, u, _, r| {
                a.update_gone(c, u, me(9177, AttackState::Move), r, class::CLASS_6)
            })
            .unwrap();
        (code, a.state, a.move_to.map(|m| m.state), rig.posted())
    };
    assert_eq!(
        gone(MoveTo::Moving, false),
        (
            UpdateCode::UNDER_WAY,
            AttackState::GotoSeen,
            Some(MoveTo::Moving),
            vec![]
        )
    );
    assert_eq!(
        gone(MoveTo::Arrived, false),
        (
            UpdateCode::DONE,
            AttackState::Done,
            None,
            vec![(NotifyKind::ACTION_INVALIDATED, [600, 0, 0x202])]
        )
    );
    assert_eq!(
        gone(MoveTo::Arrived, true),
        (UpdateCode::DONE, AttackState::Done, None, vec![])
    );
    assert_eq!(
        gone(MoveTo::Failed, false),
        (
            UpdateCode::FAILED,
            AttackState::Failed,
            None,
            vec![(NotifyKind::ACTION_FAILED, [600, 0, 0])]
        )
    );
    assert_eq!(
        gone(MoveTo::Failed, true),
        (UpdateCode::FAILED, AttackState::Failed, None, vec![])
    );
}

/// An Attack of the soldier at (10, 10) on target 7 (a victim of `player` and `class` in
/// `state` at `at`, `seen` or not by the unit's player, held in its player's list with one
/// worker), in `attack_state`, its sub-action `sub` or its own; its unit AI has an order.
fn on_target(
    (player, class, state, at): (PlayerId, ClassId, ObjectState, Position),
    seen: bool,
    (attack_state, sub): (AttackState, bool),
) -> (Rig, Attack) {
    let mut world = world_with(7, player, class, at);
    world.sight = Some(sight(seen));
    let victim = Victim {
        id: object_id(7),
        state,
        class,
        player,
        position: at,
        ..Victim::default()
    };
    let mut rig = Rig::on(world, 0.8, victim).holding(7, player);
    rig.r.ai_order = AiOrder::Order(OrderKind::Attack);
    let mut a = soldier_attack(7, Distance::new(1.0), 0.0, at);
    (a.state, a.seen, a.sub_action) = (attack_state, at, sub);
    (rig, a)
}

/// The update's start on a dead target: a unit's own Attack lets it go at once and ends with
/// 5; a villager's sub-action on Gaia's target forgets it and its point and ends with 5;
/// another unit's sub-action, or a villager's on a player's target, is not modelled.
#[test]
fn an_attack_whose_target_died_lets_it_go_and_ends() {
    let dead = |player: u8| {
        (
            PlayerId::new(player),
            class::CLASS_6,
            ObjectState::Dying,
            Position::new(11.0, 10.0, 0.0),
        )
    };
    let (mut rig, mut a) = on_target(dead(0), true, (AttackState::Ready, false));
    rig.r.ai_order = AiOrder::Idle;
    let code = rig.run(20, |c, u, f, r| a.target_died(c, u, f, r));
    assert_eq!(
        (code, rig.workers(7, PlayerId::new(0)), a.state),
        (Ok(UpdateCode::CODE_5), Some(0), AttackState::Done)
    );
    let (mut rig, mut a) = on_target(dead(0), true, (AttackState::Ready, true));
    rig.f.own.class = class::VILLAGER;
    let code = rig.run(20, |c, u, f, r| a.target_died(c, u, f, r));
    assert_eq!(
        (code, ObjectId::field(a.target), a.point, a.state),
        (
            Ok(UpdateCode::CODE_5),
            -1,
            Position::from([NONE; 3]),
            AttackState::Done
        )
    );
    let (mut rig, mut a) = on_target(dead(0), true, (AttackState::Ready, true));
    let r = rig.run(20, |c, u, f, r| a.target_died(c, u, f, r));
    assert_eq!(
        r.unwrap_err().reason(),
        "an Attack of a unit other than a villager whose target died"
    );
    let (mut rig, mut a) = on_target(dead(2), true, (AttackState::Ready, true));
    rig.f.own.class = class::VILLAGER;
    let r = rig.run(20, |c, u, f, r| a.target_died(c, u, f, r));
    assert_eq!(
        r.unwrap_err().reason(),
        "a villager's Attack whose player-owned target died (its unit AI forgets it)"
    );
}

/// A target that moved: where it was seen follows; seen, an Attack walking to where it saw
/// it follows it (`set_state(4)`); out of sight, Gaia's unit fails, a player's unit with an
/// order and an empty queue waits from state 4 and walks on in state 5; seen in state 4 it
/// goes on.
#[test]
fn an_attack_on_a_moved_target_follows_waits_or_fails() {
    let moved = (
        PlayerId::new(2),
        class::CLASS_6,
        ObjectState::Alive,
        Position::new(13.0, 10.0, 0.0),
    );
    let run = |state: AttackState, seen: bool, own_player: PlayerId| {
        let (mut rig, mut a) = on_target(moved, seen, (state, false));
        a.seen = Position::new(12.0, 10.0, 0.0);
        rig.f.own.player = own_player;
        let t = *rig.t.spatial.get(object_id(7)).unwrap();
        let code = rig
            .run(20, |c, u, f, r| a.target_moved(c, u, f, r, &t))
            .unwrap();
        (code, a)
    };
    let (code, a) = run(AttackState::GotoSeen, true, PlayerId::new(1));
    assert_eq!(
        (code, a.state, a.seen),
        (
            Some(UpdateCode::UNDER_WAY),
            AttackState::Move,
            Position::new(13.0, 10.0, 0.0)
        )
    );
    let (code, a) = run(AttackState::Move, true, PlayerId::new(1));
    assert_eq!(
        (code, a.state, a.seen),
        (None, AttackState::Move, Position::new(13.0, 10.0, 0.0))
    );
    let (code, a) = run(AttackState::Move, false, PlayerId::new(0));
    assert_eq!(
        (code, a.state),
        (Some(UpdateCode::FAILED), AttackState::Failed)
    );
    let (code, a) = run(AttackState::Move, false, PlayerId::new(1));
    assert_eq!(
        (code, a.state),
        (Some(UpdateCode::UNDER_WAY), AttackState::GotoSeen)
    );
    let (code, a) = run(AttackState::GotoSeen, false, PlayerId::new(1));
    assert_eq!((code, a.state), (None, AttackState::GotoSeen));
}

/// The update's ends before the state's step: a live target the unit's player counts as its
/// own ends it with 1 (a unit's own Attack posting 0x1fb); a target inside another ends it
/// with 4; a building that cannot attack ends it with 1; else the state's step follows.
#[test]
fn an_attack_ends_early_on_an_ally_a_hidden_target_or_disarmed() {
    let live = (
        PlayerId::new(2),
        class::CLASS_6,
        ObjectState::Alive,
        Position::new(11.0, 10.0, 0.0),
    );
    let run = |set: fn(&mut Rig)| {
        let (mut rig, mut a) = on_target(live, true, (AttackState::Move, false));
        set(&mut rig);
        let code = rig
            .run(20, |c, u, f, r| a.ends_early(c, u, f, r, true))
            .unwrap();
        ((code, rig.posted()), a)
    };
    let ((code, posted), a) = run(|rig| rig.f.victim.allied.by_attacker = true);
    assert_eq!(
        (code, a.state, posted),
        (
            Some(UpdateCode::DONE),
            AttackState::Done,
            vec![(NotifyKind::ACTION_INVALIDATED, [600, 7, 0])]
        )
    );
    let ((code, posted), a) = run(|rig| rig.f.victim.inside = true);
    assert_eq!(
        (code, a.state, posted),
        (
            Some(UpdateCode::CODE_4),
            AttackState::Done,
            vec![(NotifyKind::ACTION_INVALIDATED, [600, 7, 0])]
        )
    );
    let ((code, _), _) = run(|rig| (rig.f.victim.inside, rig.r.queue_empty) = (true, false));
    assert_eq!(code, Some(UpdateCode::CODE_4));
    let ((code, posted), a) = run(|rig| rig.r.armed = false);
    assert_eq!(
        (code, a.state, posted),
        (
            Some(UpdateCode::DONE),
            AttackState::Done,
            vec![(NotifyKind::ACTION_INVALIDATED, [600, 7, 0])]
        )
    );
    let ((code, posted), a) = run(|_| {});
    assert_eq!((code, a.state, posted), (None, AttackState::Move, vec![]));
}

/// State 6's step: ready in reach swings; ready on a dead tree ends with 5; a target out of
/// reach is followed (`set_state(4)`).
#[test]
fn a_ready_attack_swings_ends_on_a_dead_tree_or_follows() {
    let (mut rig, mut a) = on_target(
        (
            PlayerId::new(2),
            class::CLASS_6,
            ObjectState::Alive,
            Position::new(11.0, 10.0, 0.0),
        ),
        true,
        (AttackState::Ready, false),
    );
    a.arms.sprite = Some(STANDING);
    let code = rig.run(20, |c, u, f, r| a.ready_step(c, u, f, r, true));
    assert_eq!(
        (code, a.state),
        (Ok(UpdateCode::UNDER_WAY), AttackState::Swing)
    );
    let (mut rig, mut a) = on_target(
        (
            PlayerId::new(0),
            class::TREE,
            ObjectState::Dying,
            Position::new(11.0, 10.0, 0.0),
        ),
        true,
        (AttackState::Ready, false),
    );
    let code = rig.run(20, |c, u, f, r| a.ready_step(c, u, f, r, false));
    assert_eq!((code, a.state), (Ok(UpdateCode::CODE_5), AttackState::Done));
    let (mut rig, mut a) = on_target(
        (
            PlayerId::new(0),
            class::CLASS_6,
            ObjectState::Alive,
            Position::new(15.0, 10.0, 0.0),
        ),
        true,
        (AttackState::Ready, false),
    );
    a.point = Position::from([NONE; 3]);
    let code = rig.run(20, |c, u, f, r| a.ready_step(c, u, f, r, true));
    assert_eq!(
        (code, a.state),
        (Ok(UpdateCode::UNDER_WAY), AttackState::Move)
    );
}

/// State 2's step after the kill: an animating swing sprite not yet looped keeps it; done, a
/// unit's own Attack lets its target go at once, posts 0x1fb with 600 and 0x202, ends and
/// returns 2.
#[test]
fn a_killing_attack_ends_once_its_swing_looped() {
    let dead = (
        PlayerId::new(0),
        class::CLASS_6,
        ObjectState::Dying,
        Position::new(11.0, 10.0, 0.0),
    );
    let (mut rig, mut a) = on_target(dead, true, (AttackState::Killed, false));
    let code = rig.run(20, |c, u, _, _| a.killed_step(c, u));
    assert_eq!(
        (
            code,
            rig.workers(7, PlayerId::new(0)),
            rig.posted(),
            ObjectId::field(a.target),
            a.state
        ),
        (
            Ok(UpdateCode::CODE_2),
            Some(0),
            vec![(NotifyKind::ACTION_INVALIDATED, [600, 0, 0x202])],
            -1,
            AttackState::Done
        )
    );
    let (mut rig, mut a) = on_target(dead, true, (AttackState::Killed, false));
    rig.unit_mut().sprite = Some(ActiveSprite::start(STANDING, &mut Well512::default()));
    let code = rig.run(20, |c, u, _, _| a.killed_step(c, u));
    assert_eq!(
        (code, a.state, ObjectId::field(a.target)),
        (Ok(UpdateCode::UNDER_WAY), AttackState::Killed, 7)
    );
}

/// The unit AI's order as the Attack reads it ([`AiOrder::of`]): absent without a unit AI,
/// idle for one without an order.
#[test]
fn the_attack_reads_its_unit_ai_s_order() {
    assert_eq!(AiOrder::of(None), AiOrder::Absent);
    let mut ai = UnitAi::default();
    assert_eq!(AiOrder::of(Some(&ai)), AiOrder::Idle);
    ai.current_order = Some(OrderKind::Attack);
    assert_eq!(AiOrder::of(Some(&ai)), AiOrder::Order(OrderKind::Attack));
    assert!(!AiOrder::Absent.has_ai());
    assert!(AiOrder::Idle.has_ai());
}

/// The update's answer to a dead target ([`Attack::lost`]) by the unit AI: a human player's
/// unit without one, or with an order and its queue empty, waits for another target in stance
/// 0 with speed (not modelled) and else posts 0x1fa with 600; one whose unit AI has no order
/// and an empty queue posts 0x1fb with 600 and 0x202, as a computer player's does; orders
/// queued post nothing. A unit AI forgets the target, but a villager's on Gaia's.
#[test]
fn an_attack_on_a_dead_target_posts_by_its_unit_ai() {
    let dead = (
        PlayerId::new(2),
        class::CLASS_6,
        ObjectState::Dying,
        Position::new(11.0, 10.0, 0.0),
    );
    let lost = |order: AiOrder, set: fn(&mut Rig)| {
        let (mut rig, mut a) = on_target(dead, true, (AttackState::Ready, false));
        rig.r.ai_order = order;
        rig.unit_mut().combat.ai.as_mut().unwrap().current_target = Some(object_id(7));
        set(&mut rig);
        let code = rig
            .run(20, |c, u, f, r| a.lost(c, u, f, r))
            .map_err(|e| e.reason().to_owned());
        let forgot = rig
            .unit()
            .combat
            .ai
            .as_ref()
            .unwrap()
            .current_target
            .is_none();
        (
            code,
            rig.posted(),
            forgot,
            rig.workers(7, PlayerId::new(2)),
            a.state,
            a.move_to.map(|m| (m.state, m.point)),
        )
    };
    let order = AiOrder::Order(OrderKind::Attack);
    // a mover of stance 0 that is not a villager waits where it saw the target (`set_state(5)`):
    // 0x1fb with 600 and 0x202 first, then its `MoveTo` to the target's place
    let seen = Some((MoveTo::Start, Position::new(11.0, 10.0, 0.0)));
    let waits = vec![(NotifyKind::ACTION_INVALIDATED, [600, 0, 0x202])];
    for order in [AiOrder::Absent, order] {
        let waiting = lost(order, |_| {});
        assert_eq!(
            (waiting.0, waiting.1, waiting.3, waiting.4, waiting.5),
            (
                Ok(UpdateCode::CODE_5),
                waits.clone(),
                Some(0),
                AttackState::GotoSeen,
                seen
            )
        );
    }
    let posted_1fa = vec![(NotifyKind::ACTION_COMPLETED, [600, 0, 0])];
    let stance_1 = lost(order, |rig| rig.r.stance = UnitStance::DEFENSIVE);
    assert_eq!(
        (stance_1.0, stance_1.1, stance_1.2, stance_1.3, stance_1.4),
        (
            Ok(UpdateCode::CODE_5),
            posted_1fa,
            true,
            Some(0),
            AttackState::Done
        )
    );
    let villager = lost(AiOrder::Absent, |rig| rig.f.own.class = class::VILLAGER);
    assert_eq!((villager.0, villager.2), (Ok(UpdateCode::CODE_5), false));
    let posted_1fb = vec![(NotifyKind::ACTION_INVALIDATED, [600, 0, 0x202])];
    assert_eq!(lost(AiOrder::Idle, |_| {}).1, posted_1fb);
    assert_eq!(lost(order, |rig| rig.r.computer = true).1, posted_1fb);
    let queued = lost(order, |rig| rig.r.queue_empty = false);
    assert_eq!(
        (queued.0, queued.1, queued.2),
        (Ok(UpdateCode::CODE_5), vec![], true)
    );
    // a villager's unit AI keeps Gaia's target
    let gaia = |rig: &mut Rig| {
        (rig.f.own.class, rig.f.victim.player) = (class::VILLAGER, PlayerId::GAIA);
    };
    assert!(!lost(AiOrder::Idle, gaia).2);
}
