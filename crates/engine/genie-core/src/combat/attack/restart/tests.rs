//! The Attack's restart under a deleted head: the unit AI and place test, the attack count it
//! keeps, the walk, the ready end and the failure of a unit that cannot move, the target on the
//! point, the sight of a player's target, and a target the world does not hold.

use crate::combat::AttackCount;
use crate::combat::AttackTargetId;
use crate::combat::MissileCount;
use crate::combat::attack::tests::sight;
use crate::combat::attack::{
    Attack, AttackState, NONE, OwnStart, TARGET_GONE, arms, engagement, target_in_sight,
};
use crate::combat::attack::{AttackReads, AttackSource, Fight, ReadyReads};
use crate::combat::victims::victim;
use crate::garrison::ContainerId;
use crate::map::Position;
use crate::math::fmath::BVector;
use crate::movement::Speed;
use crate::movement::Velocity;
use crate::movement::move_to::{MoveTo, MoveToAction};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::SpriteId;
use crate::objects::actions::ActionList;
use crate::objects::layers::Static;
use crate::objects::masters::Masters;
use crate::objects::masters::TaskSprites;
use crate::objects::teardown::HeadStart;
use crate::orders::tests::work_attack;
use crate::players::PlayerId;
use crate::test_ids::{master_id, object_id};
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitStance;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::slots::PlayerSlotsExt;
use crate::world::tests::{delete_head_at, wait_over};
use std::sync::Arc;

/// [`work_attack`]'s soldier 11 (player 1, slot 0).
const SOLDIER: ObjectId = object_id(11);

/// [`work_attack`]'s wolf 50 of Gaia, which the soldier touches at (10.4, 10).
const WOLF: ObjectId = object_id(50);

/// The wolf's place in [`work_attack`].
const WOLF_AT: Position = Position::new(10.4, 10.0, 0.0);

/// [`work_attack`]'s soldier (its unit AI in `stance`) with its own Attack on the wolf as the
/// constructor makes it, `attack` changing it, under a wait for a foundation ([`wait_over`]):
/// its attack count 3, its animation at speed 1 and the soldier moving.
fn soldier_under_wait(stance: UnitStance, attack: impl FnOnce(&mut Attack)) -> World {
    let (mut world, _) = work_attack(stance);
    let soldier = &world.players[1].objects[ListIndex::FIRST];
    let master = soldier
        .master(world.master_tables(), PlayerId::new(1))
        .unwrap();
    let engaged = engagement(
        None,
        Speed::new(1.2),
        master,
        world.spatial.get(SOLDIER).unwrap(),
        world.spatial.get(WOLF).unwrap(),
        WorldTime::ZERO,
        MissileCount::ZERO,
    )
    .unwrap();
    let shown = arms(
        master,
        &world.model.graphics,
        TaskSprites::NONE,
        None,
        &world.model.roles,
    );
    let mut own = Attack::own(WOLF, &engaged, shown);
    attack(&mut own);
    world.parts.actions.of_mut(SOLDIER).unwrap().list =
        wait_over(ActionList::default().with_attack(Some(own)));
    world.parts.combat_mut(SOLDIER).unwrap().attack_count = AttackCount::of_data(3);
    world.parts.set_anim_speed(SOLDIER, Speed::new(1.0));
    world.parts.mover_mut(SOLDIER).unwrap().velocity = Some(Velocity {
        velocity: BVector::new(0.0, 0.0, 0.7),
        acceleration: BVector::new(0.0, 0.0, 1.0),
    });
    world
}

/// The wolf of `world` moves to `x` on the soldier's row.
fn wolf_to(world: &mut World, x: f32) {
    let at = Position::new(x, 10.0, 0.0);
    world.object_mut(WOLF).unwrap().1.place = Static::at(at);
    let mut placed = *world.spatial.get(WOLF).unwrap();
    placed.position = at;
    world.spatial.insert(WOLF, placed);
}

/// What the restart left: the soldier's Attack (the list's only action), its attack count, its
/// sprite, its animation speed, whether it still moves, and the notifies its unit AI heard.
#[derive(Debug, PartialEq)]
struct Restarted {
    attack: Attack,
    count: AttackCount,
    sprite: Option<SpriteId>,
    animation: Option<Speed>,
    moves: bool,
    heard: Vec<(NotifyKind, [i32; 3])>,
}

/// The soldier after the deletion of the wait over its Attack ([`delete_head_at`]).
fn restarted(world: &mut World) -> Restarted {
    delete_head_at(world, PlayerId::new(1), ListIndex::FIRST).unwrap();
    let list = &world.parts.actions_of(SOLDIER).list;
    assert_eq!(list.iter().count(), 1);
    let mover = world.parts.mover_of(SOLDIER).unwrap();
    Restarted {
        attack: *list.attack().unwrap(),
        count: world.parts.combat_of(SOLDIER).attack_count,
        sprite: world.players[1].objects[ListIndex::FIRST]
            .sprite
            .map(|active_sprite| active_sprite.graphic.id),
        animation: world.parts.anim_speed_of(SOLDIER),
        moves: mover
            .velocity
            .is_some_and(|velocity| velocity.velocity.y() != 0.0),
        heard: world.parts.unit_ai.get(SOLDIER).map_or(vec![], |ai| {
            ai.notifies
                .iter()
                .map(|notify| (notify.kind, notify.params))
                .collect()
        }),
    }
}

/// An Attack walking to the wolf: state 4 with its `MoveTo` under way.
fn walking(attack: &mut Attack) {
    attack.state = AttackState::Move {
        move_to: Some(MoveToAction::sub_in(MoveTo::Moving)),
    };
}

/// A deleted head over the Attack of a unit without a unit AI (`+0x6b0`) or inside an object
/// (`+0x2f0`) restarts it to `set_state(13)` and nothing more (`0x140a7f8c0`): its walk's
/// `MoveTo` goes and stops the unit, the animation runs at speed 1 and the unit stands; no
/// readyToAttack (its point stays −1, no notify), and the attack count stays.
#[test]
fn deleted_head_over_attack_without_unit_ai_or_inside_object_fails_it() {
    let mut ailess = soldier_under_wait(UnitStance::Aggressive, walking);
    ailess.parts.set_unit_ai(SOLDIER, None);
    let mut inside = soldier_under_wait(UnitStance::Aggressive, walking);
    inside.object_mut(SOLDIER).unwrap().1.life.inside = Some(ContainerId::new(object_id(60)));
    for mut world in [ailess, inside] {
        let after = restarted(&mut world);
        assert_eq!(
            (after.attack.state, after.attack.point),
            (AttackState::Failed, Position::new(NONE, NONE, NONE))
        );
        assert_eq!(
            (after.count, after.sprite, after.animation, after.moves),
            (
                AttackCount::of_data(3),
                Some(SpriteId::named(300)),
                Some(Speed::new(1.0)),
                false
            )
        );
        assert_eq!(after.heard, vec![]);
    }
}

/// The restart (with 0) keeps the unit's attack count; the start of a new Attack (with 1, as
/// the list adds it) clears it.
#[test]
fn restart_keeps_attack_count_where_start_of_new_attack_clears_it() {
    let mut world = soldier_under_wait(UnitStance::Aggressive, |_| {});
    assert_eq!(restarted(&mut world).count, AttackCount::of_data(3));
    let mut world = soldier_under_wait(UnitStance::Aggressive, |_| {});
    let mut attack = *world.parts.actions_of(SOLDIER).list.attack().unwrap();
    let roles = world.model.roles.clone();
    let posted = world
        .run_on_step(PlayerId::new(1), ListIndex::FIRST, |unit, actions, step| {
            let placed = *step.world.spatial.get(WOLF).unwrap();
            let me = *step.world.spatial.get(SOLDIER).unwrap();
            let master = unit.master(step.world.master_tables(), step.player)?;
            let engaged = engagement(
                step.world.parts.unit_ai.get(SOLDIER),
                step.world.top_speed(step.player, unit)?,
                master,
                &me,
                &placed,
                step.world.world_time,
                MissileCount::ZERO,
            )?;
            let reads = AttackReads::of(
                AttackSource {
                    combat: step.world.parts.combat_of(SOLDIER),
                    top_speed: step.world.top_speed_terms(step.player, unit)?,
                    master,
                    graphics: &step.world.model.graphics,
                    ai: step.world.parts.unit_ai.get(SOLDIER),
                    player_kind: step.world.players.player(step.player)?.kind,
                    now: step.world.world_time,
                },
                &roles,
                &step.world.model.tuning.turn,
            );
            let ready = ReadyReads {
                reads,
                fight: Fight {
                    victim: victim(step, WOLF, &roles)?.unwrap(),
                    own: unit.own_victim(actions, step, &roles)?.unwrap(),
                },
            };
            let start = OwnStart {
                target: WOLF,
                engagement: &engaged,
                visible: target_in_sight(
                    &step.world.spatial,
                    step.player,
                    &placed,
                    master.fog_visibility,
                )?,
                target_owner: placed.player,
                ready: Some(ready),
            };
            attack.start_own(step, unit, HeadStart::Added, &start, &roles)
        })
        .unwrap();
    assert_eq!(
        (world.parts.combat_of(SOLDIER).attack_count, posted),
        (AttackCount::ZERO, vec![])
    );
}

/// A unit that may move (a top speed, a unit AI in stance 0) restarts its Attack to state 4
/// whatever readyToAttack found: the wolf it touches becomes the point, and the `MoveTo` to it
/// starts arrived (within 0.01), showing the standing sprite at speed 0.
#[test]
fn restart_of_unit_that_may_move_turns_to_state_4() {
    let mut world = soldier_under_wait(UnitStance::Aggressive, |attack| {
        attack.state = AttackState::Ready;
    });
    let after = restarted(&mut world);
    assert_eq!(
        (
            after.attack.state.bare(),
            after
                .attack
                .state
                .move_to()
                .map(|move_to_action| move_to_action.state),
            after.attack.point
        ),
        (
            AttackState::Move { move_to: None },
            Some(MoveTo::Arrived),
            WOLF_AT
        )
    );
    assert_eq!(
        (after.count, after.sprite, after.animation),
        (
            AttackCount::of_data(3),
            Some(SpriteId::named(300)),
            Some(Speed::ZERO)
        )
    );
}

/// A unit that cannot move (its unit AI in stance 2 without an order) restarts its Attack in
/// reach of the wolf to `set_state(6)`: its walk's `MoveTo` goes and stops it, and readyToAttack
/// again finds the wolf on the point; with its reload under way it waits ready, with it done it
/// swings at once on its attack sprite.
#[test]
fn restart_of_unit_that_cannot_move_goes_ready_in_reach() {
    for (timer, state, sprite) in [
        (0.5, AttackState::Ready, 300),
        (0.0, AttackState::Swing, 301),
    ] {
        let mut world = soldier_under_wait(UnitStance::StandGround, walking);
        world.parts.combat_mut(SOLDIER).unwrap().attack_timer = Seconds::new(timer);
        let after = restarted(&mut world);
        assert_eq!(
            (after.attack.state, after.attack.point),
            (state, WOLF_AT),
            "reload timer {timer}"
        );
        assert_eq!(
            (after.sprite, after.moves, after.heard),
            (Some(SpriteId::named(sprite)), false, vec![])
        );
    }
}

/// A unit that cannot move restarts its Attack out of reach of the wolf to `set_state(13)`:
/// readyToAttack tells its unit AI that the target is out of reach (0x1fd), its walk's `MoveTo`
/// goes and stops it, and it stands at speed 1.
#[test]
fn restart_of_unit_that_cannot_move_fails_out_of_reach() {
    let mut world = soldier_under_wait(UnitStance::StandGround, walking);
    wolf_to(&mut world, 13.0);
    let after = restarted(&mut world);
    assert_eq!(
        (after.attack.state, after.attack.point),
        (AttackState::Failed, Position::new(NONE, NONE, NONE))
    );
    assert_eq!(
        (after.sprite, after.animation, after.moves, after.heard),
        (
            Some(SpriteId::named(300)),
            Some(Speed::new(1.0)),
            false,
            vec![(NotifyKind::OUT_OF_REACH, [0; 3])]
        )
    );
}

/// An Attack whose point still holds the wolf's place restarts ready at once, out of reach of a
/// unit that cannot move: `set_state(6)` without 0x1fd, the point kept (readyToAttack's
/// ready-at-once test, `FUN_140a7bbf0`), waiting ready for its reload.
#[test]
fn restart_on_target_at_its_point_is_ready_at_once() {
    let away = Position::new(13.0, 10.0, 0.0);
    let mut world = soldier_under_wait(UnitStance::StandGround, |attack| {
        attack.point = away;
    });
    world.parts.combat_mut(SOLDIER).unwrap().attack_timer = Seconds::new(0.5);
    wolf_to(&mut world, away.x());
    let after = restarted(&mut world);
    assert_eq!(
        (after.attack.state, after.attack.point, after.heard),
        (AttackState::Ready, away, vec![])
    );
}

/// An Attack without a target, or on one the world does not hold, fails the restart
/// (readyToAttack's part without a target is not ported), the Attack back on the list.
#[test]
fn restart_without_target_or_on_one_world_lacks_fails() {
    for target in [None, Some(object_id(99))] {
        let mut world = soldier_under_wait(UnitStance::Aggressive, |attack| {
            attack.target = target.map(AttackTargetId::new);
        });
        let error = delete_head_at(&mut world, PlayerId::new(1), ListIndex::FIRST).unwrap_err();
        assert_eq!(error.reason(), TARGET_GONE.reason(), "{target:?}");
        let list = &world.parts.actions_of(SOLDIER).list;
        assert_eq!(
            list.attack()
                .map(|attack| attack.target.map(AttackTargetId::id)),
            Some(target)
        );
    }
}

/// readyToAttack's sight test at the restart: a target of a player (its owner above 0) out of
/// the unit's player's sight fails the Attack of a unit that cannot move before its ready end
/// (`set_state(13)`, the point −1, no notify), where the same target seen goes ready on it.
#[test]
fn restart_on_target_of_player_out_of_sight_fails_and_seen_goes_ready() {
    for (seen, state, point) in [
        (false, AttackState::Failed, Position::new(NONE, NONE, NONE)),
        (true, AttackState::Ready, WOLF_AT),
    ] {
        let mut world = soldier_under_wait(UnitStance::StandGround, |_| {});
        world.parts.combat_mut(SOLDIER).unwrap().attack_timer = Seconds::new(0.5);
        // the wolf is the soldier's player's, its master in that player's table
        let wolf_master = world.model.masters[0].get(master_id(126)).unwrap().clone();
        let masters = world.model.masters[1].iter().cloned().chain([wolf_master]);
        world.model.masters[1] = Arc::new(Masters::new(masters));
        let mut placed = *world.spatial.get(WOLF).unwrap();
        placed.player = PlayerId::new(1);
        world.spatial.insert(WOLF, placed);
        world.spatial.sight = Some(sight(seen));
        let after = restarted(&mut world);
        assert_eq!(
            (after.attack.state, after.attack.point, after.heard),
            (state, point, vec![]),
            "seen {seen}"
        );
    }
}
