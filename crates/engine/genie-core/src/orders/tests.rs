use super::*;
use crate::commands::UnitOrderAction;
use crate::commands::UnloadPick;
use crate::construction::MainBuildingId;
use crate::construction::build::BuildState;
use crate::garrison::ContainerId;
use crate::gathering::DropSiteId;
use crate::gathering::gather::GatherState;
use crate::gathering::hunt::HuntState;
use crate::groups::Differentiator;
use crate::groups::FormationKind;
use crate::groups::GroupCommandKind;
use crate::groups::GroupFlags;
use crate::groups::GroupTarget;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Restriction;
use crate::map::RestrictionId;
use crate::map::TerrainFlags;
use crate::map::Tile;
use crate::map::terrain::TileTerrain;
use crate::math::fmath::BVector;
use crate::movement::PathFlags;
use crate::objects::Graphics;
use crate::objects::HpPowerup;
use crate::objects::ObjectId;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::actions::ActionList;
use crate::objects::lifecycle::Held;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::ClassId;
use crate::objects::masters::FogVisibility;
use crate::objects::masters::FormationClass;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::TargetOwner;
use crate::objects::masters::TaskGroup;
use crate::objects::masters::TaskId;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::action_type;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::masters::master_type::BUILDING;
use crate::players::Amount;
use crate::players::ByPlayer;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::relation_row;
use crate::players::relations;
use crate::test_ids::carrying;
use crate::test_ids::class_of;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_id_list;
use crate::test_ids::object_ids;
use crate::test_ids::player_id;
use crate::unit_ai::ActionKind;
use crate::unit_ai::Issuer;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAiType;
use crate::unit_ai::UnitStance;
use crate::vision::VisionBits;
use crate::world::Seconds;
use crate::world::WholeSeconds;

/// The object's own update knows its slot: it is taken while it still holds the object, and
/// the object is found by its id once the list moved or the slot is out of the lists; an
/// object asleep or gone has no live slot.
#[test]
fn an_object_s_own_slot_is_taken_while_it_holds_the_object() {
    let unit = |id| Unit {
        id,
        ..Unit::default()
    };
    let t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            crate::world::Player {
                objects: vec![unit(object_id(1)), unit(object_id(2))].into(),
                ..crate::world::Player::default()
            },
            crate::world::Player {
                objects: vec![unit(object_id(3))].into(),
                sleeping: vec![unit(object_id(4))].into(),
                ..crate::world::Player::default()
            },
        ],
    );
    assert_eq!(
        t.live_slot((PlayerId::new(1), ListIndex::new(0), object_id(3))),
        Some((PlayerId::new(1), ListIndex::new(0)))
    );
    assert_eq!(
        t.live_slot((PlayerId::new(0), ListIndex::new(0), object_id(2))),
        Some((PlayerId::new(0), ListIndex::new(1)))
    );
    assert_eq!(
        t.live_slot((PlayerId::new(5), ListIndex::new(9), object_id(3))),
        Some((PlayerId::new(1), ListIndex::new(0)))
    );
    assert_eq!(
        t.live_slot((PlayerId::new(1), ListIndex::new(0), object_id(4))),
        None
    );
    assert_eq!(
        t.live_slot((PlayerId::new(0), ListIndex::new(0), object_id(7))),
        None
    );
}

/// records/425906646's gate 27745 (masters 88 and 91, 1650 hit points each, three damage
/// sprites): unhurt, its share is 0 under either master; a resource's share is not modelled.
#[test]
fn a_damage_share_is_the_whole_less_the_hit_points_share() {
    assert_eq!(
        damage_share(
            HitPoints::new(1650.0),
            MasterHitPoints::of_data(1650),
            class::GATE
        ),
        Ok(0)
    );
    assert_eq!(
        damage_share(
            HitPoints::new(825.0),
            MasterHitPoints::of_data(1650),
            class::GATE
        ),
        Ok(50)
    );
    assert_eq!(
        damage_share(
            HitPoints::new(1.0),
            MasterHitPoints::of_data(1650),
            class::GATE
        ),
        Ok(99)
    );
    assert_eq!(
        damage_share(
            HitPoints::new(2000.0),
            MasterHitPoints::of_data(1650),
            class::GATE
        ),
        Ok(0)
    );
    assert!(
        damage_share(
            HitPoints::new(10.0),
            MasterHitPoints::of_data(100),
            class::LIVESTOCK
        )
        .is_err()
    );
}

/// records/425906646's update 1641: player 6's drop-off order reaches its goose 28766 (class
/// 58, 100 food, no drop sites, its default task on top): no table entry takes class 58, so
/// it does nothing. An entry of its class (taking food) leads to the nearest such building
/// of the player's tracker in state 2 (`FUN_141658e20`), but a goose (below type 70) does no
/// work on it.
#[test]
fn a_drop_off_order_leaves_a_livestock_without_a_drop_site_alone() {
    let goose = Unit {
        id: object_id(28766),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(1243),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO],
        vec![crate::world::Player {
            objects: vec![goose].into(),
            ..crate::world::Player::default()
        }],
    );
    t.masters = vec![crate::objects::masters::Masters::new([Master {
        id: master_id(1243),
        class: class::LIVESTOCK,
        ..Master::default()
    }])];
    let tc = crate::unit_ai::task_finder::DropSite {
        building: ids::TOWN_CENTRE,
        worker_class: Some(class::VILLAGER),
        ..crate::unit_ai::task_finder::DropSite::default()
    };
    t.drop_sites = vec![tc.clone()];
    assert!(
        t.drop_off_unasked(PlayerId::new(0), ListIndex::new(0), true)
            .is_ok()
    );
    t.drop_sites = vec![
        tc,
        crate::unit_ai::task_finder::DropSite {
            building: ids::PASTURE,
            worker_class: Some(class::LIVESTOCK),
            resource_types: vec![attribute::FOOD],
            ..crate::unit_ai::task_finder::DropSite::default()
        },
    ];
    let pen = |id, x: f32, state| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, 10.0, 0.0)),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: ids::PASTURE,
            state,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    t.masters[0] = crate::objects::masters::Masters::new([
        t.masters[0].get(master_id(1243)).unwrap().clone(),
        Master {
            id: ids::PASTURE,
            kind: BUILDING,
            class: class::BUILDING,
            ..Master::default()
        },
    ]);
    t.players[0].objects[ListIndex::new(0)].life.attribute_type = Some(attribute::FOOD);
    t.players[0].objects[ListIndex::new(0)].place_for_test(Position::new(10.0, 10.0, 0.0));
    for (id, x, state) in [
        (500, 30.0, ObjectState::Alive),
        (501, 12.0, ObjectState::Foundation),
        (502, 20.0, ObjectState::Alive),
        (503, 15.0, ObjectState::Alive),
    ] {
        t.players[0].objects.push(pen(object_id(id), x, state));
        t.track_add(PlayerId::new(0), object_id(id), ids::PASTURE)
            .unwrap();
    }
    // the tracker's done list: the foundation 501 waits, 503 is the nearest standing one
    let key = t.tracker_key(PlayerId::new(0), ids::PASTURE);
    for id in [503, 502, 500] {
        t.players[0].tracker.complete(object_id(id), key);
    }
    assert_eq!(
        t.table_drop_site(
            PlayerId::new(0),
            Point::new(10.0, 10.0),
            Some(attribute::FOOD),
            class::LIVESTOCK
        )
        .unwrap(),
        Some(DropSiteId::new(object_id(503)))
    );
    assert_eq!(
        t.table_drop_site(
            PlayerId::new(0),
            Point::new(10.0, 10.0),
            Some(attribute::WOOD),
            class::LIVESTOCK
        )
        .unwrap(),
        None
    );
    let before = t.players[0].objects[ListIndex::new(0)].clone();
    assert!(
        t.drop_off_unasked(PlayerId::new(0), ListIndex::new(0), true)
            .is_ok()
    );
    assert_eq!(t.players[0].objects[ListIndex::new(0)], before);
}

/// `UNIT_ORDER`'s drop-off on a builder carrying wood (`FUN_14161e370` on a Build: slot 0x78,
/// `FUN_140a78060`, then `set_state(8)`): it leaves its foundation's builders, takes the
/// drop-site table's nearest mill (`FUN_141658e20`) as its second target and walks to it
/// within the Build task's range (its master has no task on the mill). Without a mill in the
/// table it walks back to its foundation; carrying nothing it is not ordered at all.
#[test]
fn a_unit_order_drop_off_sends_a_carrying_builder_to_the_table_s_drop_site() {
    use crate::map::spatial::Placed;
    let placed = |position, master, radius| Placed {
        position,
        radius,
        class: class::BUILDING,
        player: PlayerId::new(0),
        master,
        obstruction_type: ObstructionType::TYPE_2,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    let villager = Unit {
        id: object_id(10),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: ids::VILLAGER_MALE,
            state: ObjectState::Alive,
            attribute_type: Some(attribute::WOOD),
            amount: Amount::new(5.0),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        body: crate::objects::layers::Body::new(
            Position::new(10.0, 10.0, 0.0),
            None,
            Some(crate::gathering::hunt::tests::walker()),
        ),
        actions: crate::objects::layers::ActionLayer {
            list: ActionList::default().with_build(Some(crate::construction::build::BuildAction {
                state: BuildState::Work,
                added: true,
                ..crate::construction::build::tests::started(
                    object_id(600),
                    Position::new(5.0, 10.0, 0.0),
                    Distance::ZERO,
                    crate::construction::build::BuildTask {
                        range: Distance::new(0.1),
                        work_value: crate::objects::masters::WorkValue::new(1.0),
                        work_rate: WorkRate::new(1.0),
                        resource: None,
                        id: TaskId::of_task(-1),
                        moving: None,
                    },
                )
            })),
            ..Default::default()
        },
        ..Unit::default()
    };
    let foundation = Unit {
        id: object_id(600),
        body: crate::objects::layers::Body::at(Position::new(5.0, 10.0, 0.0)),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: ids::HOUSE,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        building: crate::objects::layers::Building {
            construction: Some(crate::construction::build::Construction {
                progress: Seconds::new(1.0),
                desolid: false,
                builders: crate::construction::BuilderCount::of_data(1),
                hit_points: MasterHitPoints::of_data(550),
                build_time: WholeSeconds::of_data(25),
                record: crate::map::spatial::Obstruction {
                    kind: ObstructionType::TYPE_2,
                    class: ObstructionClass::UNIT,
                },
                snow: false,
                sprite_override: false,
            }),
            ..Default::default()
        },
        ..Unit::default()
    };
    let mill = Unit {
        id: object_id(503),
        body: crate::objects::layers::Body::at(Position::new(15.0, 10.0, 0.0)),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(68),
            state: ObjectState::Alive,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    let world = || {
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![crate::world::Player {
                objects: vec![villager.clone(), foundation.clone(), mill.clone()].into(),
                ..crate::world::Player::default()
            }],
        );
        t.masters = vec![crate::objects::masters::Masters::new([
            Master {
                id: ids::VILLAGER_MALE,
                kind: master_type::COMBAT,
                class: class::VILLAGER,
                radius: Footprint::new(0.2, 0.2),
                // no sprites (the world's sprites' table is empty)
                standing_sprite: None,
                move_sprite: None,
                run_sprite: None,
                ..Master::default()
            },
            Master {
                id: ids::HOUSE,
                kind: BUILDING,
                class: class::BUILDING,
                radius: Footprint::new(1.0, 1.0),
                ..Master::default()
            },
            Master {
                id: master_id(68),
                kind: BUILDING,
                class: class::BUILDING,
                radius: Footprint::new(1.0, 1.0),
                ..Master::default()
            },
        ])];
        t.spatial.insert(
            object_id(10),
            placed(
                Position::new(10.0, 10.0, 0.0),
                ids::VILLAGER_MALE,
                Footprint::new(0.2, 0.2),
            ),
        );
        t.spatial.insert(
            object_id(600),
            placed(
                Position::new(5.0, 10.0, 0.0),
                ids::HOUSE,
                Footprint::new(1.0, 1.0),
            ),
        );
        t.spatial.insert(
            object_id(503),
            placed(
                Position::new(15.0, 10.0, 0.0),
                master_id(68),
                Footprint::new(1.0, 1.0),
            ),
        );
        t.track_add(PlayerId::new(0), object_id(503), master_id(68))
            .unwrap();
        let key = t.tracker_key(PlayerId::new(0), master_id(68));
        t.players[0].tracker.complete(object_id(503), key);
        t
    };
    let mut t = world();
    t.drop_sites = vec![crate::unit_ai::task_finder::DropSite {
        building: master_id(68),
        worker_class: Some(class::VILLAGER),
        resource_types: vec![attribute::WOOD],
        ..crate::unit_ai::task_finder::DropSite::default()
    }];
    t.unit_order(
        player_id(0),
        &object_ids([10]),
        UnitOrderAction::DropOffAndBack,
    )
    .unwrap();
    let build = t.players[0].objects[ListIndex::new(0)]
        .actions
        .list
        .build()
        .copied()
        .unwrap();
    let walk = build.move_to.unwrap();
    assert_eq!(
        (
            build.state,
            build.site,
            ObjectId::field(walk.target),
            walk.range
        ),
        (
            BuildState::Drop,
            Some(DropSiteId::new(object_id(503))),
            503,
            Distance::new(0.1)
        )
    );
    assert_eq!(
        (walk.state, walk.on_target_moved, build.added),
        (
            crate::movement::move_to::MoveTo::Start,
            crate::movement::move_to::TargetMoved::AsksAgain,
            false
        )
    );
    assert_eq!(
        t.players[0].objects[ListIndex::new(1)]
            .building
            .construction
            .map(|k| k.builders.get()),
        Some(0)
    );
    assert_eq!(
        t.players[0].objects[ListIndex::new(2)].workers,
        crate::objects::WorkerCount::of_data(1)
    );
    // no table entry for wood: back to the foundation
    let mut t = world();
    t.unit_order(
        player_id(0),
        &object_ids([10]),
        UnitOrderAction::DropOffAndBack,
    )
    .unwrap();
    let build = t.players[0].objects[ListIndex::new(0)]
        .actions
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!(
        (
            build.state,
            build.site,
            build.move_to.map(|m| ObjectId::field(m.target))
        ),
        (BuildState::Move, None, Some(600))
    );
    // carrying nothing: not ordered
    let mut t = world();
    t.players[0].objects[ListIndex::new(0)].life.amount = Amount::ZERO;
    t.unit_order(
        player_id(0),
        &object_ids([10]),
        UnitOrderAction::DropOffAndBack,
    )
    .unwrap();
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .actions
            .list
            .build()
            .map(|b| b.state),
        Some(BuildState::Work)
    );
    // 7 asks the Build as 8 does; 6 does not ask it: the search alone (no drop site in the
    // table here, nothing to work on)
    let mut t = world();
    t.drop_sites = vec![crate::unit_ai::task_finder::DropSite {
        building: master_id(68),
        worker_class: Some(class::VILLAGER),
        resource_types: vec![attribute::WOOD],
        ..crate::unit_ai::task_finder::DropSite::default()
    }];
    t.unit_order(player_id(0), &object_ids([10]), UnitOrderAction::DropOff)
        .unwrap();
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .actions
            .list
            .build()
            .map(|b| (b.state, b.site)),
        Some((BuildState::Drop, Some(DropSiteId::new(object_id(503)))))
    );
    let mut t = world();
    t.unit_order(
        player_id(0),
        &object_ids([10]),
        UnitOrderAction::DropOffSearch,
    )
    .unwrap();
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .actions
            .list
            .build()
            .map(|b| b.state),
        Some(BuildState::Work)
    );
    assert!(
        t.unit_order(player_id(0), &object_ids([10]), UnitOrderAction::Gather)
            .is_err()
    );
}

/// The work command's Attack (`0x140a7f8c0`) of a soldier that may move, touching Gaia's
/// wolf: the start ends in `set_state(4)`, whose `MoveTo` to the target starts arrived (state
/// 1 within 0.01, `FUN_140aa2100`; 426954810's p6 at 1730). Its `set_state(1)`
/// (`0x140aa29e0`, not quiet) shows the standing sprite (slot 0x798) at the standing speed and
/// clears `+0x7b8`; the Attack waits in state 4 for the `MoveTo`'s end.
#[test]
fn a_work_attack_on_a_touching_target_starts_its_move_to_arrived() {
    let (mut t, task) = work_attack(UnitStance::AGGRESSIVE);
    t.start_attack(
        PlayerId::new(1),
        ListIndex::new(0),
        object_id(50),
        &task,
        Position::new(10.4, 10.0, 0.0),
        true,
    )
    .unwrap();
    let unit = &t.players[1].objects[ListIndex::new(0)];
    let attack = unit.actions.list.attack().expect("the soldier's Attack");
    assert_eq!(
        (
            attack.state,
            attack.move_to.map(|m| m.state),
            attack.target2
        ),
        (
            crate::combat::attack::AttackState::Move,
            Some(crate::movement::move_to::MoveTo::Arrived),
            Some(object_id(50))
        )
    );
    assert_eq!(
        unit.sprite.map(|s| s.graphic.id),
        Some(SpriteId::named(300))
    );
    assert_eq!(unit.body.anim_speed(), Some(Speed::ZERO));
    assert_eq!(
        unit.body.mover().map(|m| m.stats.speed),
        Some(Speed::new(0.8))
    );
}

/// The work command's check of the top Attack (`FUN_14106ff70`, [`World::keep_attack`]): an
/// Attack in state 4 on this target is kept when the filter is none or 9, and its unit AI hears
/// of it as of a new one (the retarget time takes the world's time); another target or filter
/// does not keep it.
#[test]
fn a_work_command_keeps_the_top_attack_on_its_target_and_tells_its_unit_ai() {
    let (mut t, task) = work_attack(UnitStance::AGGRESSIVE);
    let at = Position::new(10.4, 10.0, 0.0);
    t.start_attack(
        PlayerId::new(1),
        ListIndex::new(0),
        object_id(50),
        &task,
        at,
        true,
    )
    .unwrap();
    t.world_time = WorldTime::from_millis(3500);
    assert_eq!(
        t.keep_attack(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(50)),
            Some(crate::garrison::ENTER),
            at,
            true
        ),
        Ok(false)
    );
    assert_eq!(
        t.keep_attack(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(51)),
            None,
            at,
            true
        ),
        Ok(false)
    );
    let ai = |t: &World| {
        t.players[1].objects[ListIndex::new(0)]
            .combat
            .ai
            .as_ref()
            .map(|a| a.last_retarget_time)
    };
    assert_ne!(ai(&t), Some(WorldTime::from_millis(3500)));
    assert_eq!(
        t.keep_attack(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(50)),
            Some(crate::combat::attack::ATTACK),
            at,
            true
        ),
        Ok(true)
    );
    assert_eq!(ai(&t), Some(WorldTime::from_millis(3500)));
}

/// The attack task's aim (`FUN_141189b20`): Gaia's wolf aims at itself; a building annex at
/// its main building, with the annex's owner, class and fog visibility but the main building's
/// place and class; a target or a main building the slice does not hold fails.
#[test]
fn an_attack_task_aims_at_its_target_or_an_annex_s_main_building() {
    let (mut t, _) = work_attack(UnitStance::AGGRESSIVE);
    let aim = t.attack_aim(object_id(50)).unwrap();
    assert_eq!(
        (aim.owner, aim.class, aim.fog_visibility),
        (PlayerId::new(0), class::PREDATOR, FogVisibility::INVISIBLE)
    );
    assert_eq!(
        (aim.aimed, aim.at, aim.aimed_class),
        (
            object_id(50),
            Position::new(10.4, 10.0, 0.0),
            class::PREDATOR
        )
    );
    annexed(&mut t);
    let aim = t.attack_aim(object_id(61)).unwrap();
    assert_eq!(
        (aim.owner, aim.class, aim.fog_visibility),
        (PlayerId::new(0), class::TOWER, FogVisibility::VISIBLE)
    );
    assert_eq!(
        (aim.aimed, aim.at, aim.aimed_class),
        (
            object_id(60),
            Position::new(20.5, 20.5, 0.0),
            class::BUILDING
        )
    );
    let r = t.attack_aim(object_id(98));
    assert_eq!(
        r.err().map(|e| e.reason()),
        Some("an attack task's target the slice does not hold")
    );
    t.players[0].objects[ListIndex::new(2)].building.annex_of =
        Some(MainBuildingId::new(object_id(99)));
    let r = t.attack_aim(object_id(61));
    assert_eq!(
        r.err().map(|e| e.reason()),
        Some("an attack task's main building is gone")
    );
}

/// The attack task's word to its unit AI (`FUN_140c64b50`, `FUN_140c64c10`, `FUN_14106e4e0`
/// kind 9): the aimed main building becomes the soldier's last target, the annex its target
/// with its class at the work's point under order 700 and action 600; the last retarget time
/// and the order history take the world's time, and the annex counts the soldier as a worker.
/// A soldier without a unit AI only takes the target.
#[test]
fn an_attack_task_tells_its_unit_ai_the_order_the_action_and_the_target() {
    let (mut t, _) = work_attack(UnitStance::AGGRESSIVE);
    annexed(&mut t);
    t.world_time = WorldTime::from_millis(1730);
    let aim = t.attack_aim(object_id(61)).unwrap();
    t.attack_notifies(
        PlayerId::new(1),
        ListIndex::new(0),
        (object_id(61), &aim),
        Position::new(21.0, 21.0, 0.0),
        true,
    )
    .unwrap();
    let ai = t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .as_ref()
        .unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Attack),
            Some(ActionKind::Attack),
            Some(object_id(61))
        )
    );
    assert_eq!(
        (ai.target_class, ai.target_point),
        (class_of(52), Position::new(21.0, 21.0, 0.0))
    );
    assert_eq!(
        (ai.last_target, ai.last_target_class, ai.last_retarget_time),
        (
            Some(object_id(60)),
            class_of(3),
            WorldTime::from_millis(1730)
        )
    );
    let record = ai.order_history.last().expect("the order history");
    assert_eq!(
        (record.order, record.action, record.time, record.target),
        (
            Some(OrderKind::Attack),
            Some(ActionKind::Attack),
            WorldTime::from_millis(1730),
            Some(object_id(61))
        )
    );
    assert_eq!(
        t.players[0].objects[ListIndex::new(2)].workers,
        crate::objects::WorkerCount::of_data(1)
    );
    t.players[1].objects[ListIndex::new(0)].combat.ai = None;
    t.attack_notifies(
        PlayerId::new(1),
        ListIndex::new(0),
        (object_id(61), &aim),
        Position::new(21.0, 21.0, 0.0),
        true,
    )
    .unwrap();
    assert_eq!(
        t.players[0].objects[ListIndex::new(2)].workers,
        crate::objects::WorkerCount::of_data(2)
    );
}

/// The attack task's Attack of a soldier without a unit AI (it cannot move) touching the wolf
/// goes ready at its start (`set_state(6)`, [`crate::combat::attack::Attack::command`]): with its reload
/// under way it waits ready on its standing sprite; with it done it swings at once on its
/// attack sprite.
#[test]
fn an_attack_task_of_a_unit_that_cannot_move_goes_ready_in_reach() {
    use crate::combat::attack::AttackState;
    for (timer, state, pending, sprite) in [
        (0.5, AttackState::Ready, false, 300),
        (0.0, AttackState::Swing, true, 301),
    ] {
        let (mut t, task) = work_attack(UnitStance::AGGRESSIVE);
        let unit = &mut t.players[1].objects[ListIndex::new(0)];
        unit.combat.ai = None;
        unit.combat.attack_timer = Seconds::new(timer);
        t.start_attack(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(50),
            &task,
            Position::new(10.4, 10.0, 0.0),
            true,
        )
        .unwrap();
        let unit = &t.players[1].objects[ListIndex::new(0)];
        let attack = unit
            .actions
            .list
            .attack()
            .copied()
            .expect("the soldier's Attack");
        assert_eq!(
            (attack.state, attack.pending, attack.target2),
            (state, pending, Some(object_id(50))),
            "reload timer {timer}"
        );
        assert_eq!(
            unit.sprite.map(|s| s.graphic.id),
            Some(SpriteId::named(sprite))
        );
    }
}

/// Gaia's main building 60 (master 200, class 3) at (20.5, 20.5) and its annex 61 (master 201,
/// class 52) at (22, 20.5), both fog visible, after the wolf of [`work_attack`]'s world.
fn annexed(t: &mut World) {
    let wolf = t.masters[0]
        .get(master_id(126))
        .cloned()
        .expect("the wolf's master");
    t.masters[0] = crate::objects::masters::Masters::new([
        wolf,
        Master {
            id: master_id(200),
            kind: BUILDING,
            class: class::BUILDING,
            fog_visibility: FogVisibility::VISIBLE,
            ..Master::default()
        },
        Master {
            id: master_id(201),
            kind: BUILDING,
            class: class::TOWER,
            fog_visibility: FogVisibility::VISIBLE,
            ..Master::default()
        },
    ]);
    let building = |id, master_id, position, annex_of| Unit {
        id,
        body: crate::objects::layers::Body::at(position),
        building: crate::objects::layers::Building {
            annex_of,
            ..Default::default()
        },
        life: crate::objects::lifecycle::Lifecycle {
            master_id,
            state: ObjectState::Alive,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[0].objects.push(building(
        object_id(60),
        master_id(200),
        Position::new(20.5, 20.5, 0.0),
        None,
    ));
    t.players[0].objects.push(building(
        object_id(61),
        master_id(201),
        Position::new(22.0, 20.5, 0.0),
        Some(MainBuildingId::new(object_id(60))),
    ));
}

/// [`soldiers`]' soldier 11 (master 4, melee, reload 2) at (10, 10) on its feet with a walker
/// of speed 1.2 and a unit AI of `stance`, touching Gaia's wolf 50 (master 126, class 10, 25
/// hit points) at (10.4, 10), and the attack task (action type 7, no sprites of its own).
fn work_attack(stance: UnitStance) -> (World, crate::objects::masters::Task) {
    use crate::map::spatial::Placed;
    use crate::math::rng::Well512;
    use crate::objects::sprite::Graphic;
    let standing = Graphic {
        id: SpriteId::named(300),
        frames: crate::objects::Frame::of_data(10),
        facets: crate::objects::FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ONE,
        frame_duration: Seconds::new(0.1),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED,
    };
    let mut t = soldiers(&[(11, 4, Point::new(10.0, 10.0))], &[]);
    t.graphics = Graphics::from([
        (SpriteId::named(300), standing),
        (
            SpriteId::named(301),
            Graphic {
                id: SpriteId::named(301),
                ..standing
            },
        ),
        (
            SpriteId::named(302),
            Graphic {
                id: SpriteId::named(302),
                speed_multiplier: SpeedMultiplier::of_data(1.5),
                ..standing
            },
        ),
    ]);
    let mut walker = crate::gathering::hunt::tests::walker();
    // the soldier's speed 0.8: its standing sprite's factor 1, its walking one's 1.5
    walker.stats.speed = Speed::new(1.2);
    let soldier = t.masters[1].get_mut(master_id(4)).unwrap();
    soldier.standing_sprite = Some(SpriteId::named(300));
    soldier.attack_sprite = Some(SpriteId::named(301));
    soldier.move_sprite = Some(SpriteId::named(302));
    soldier.weapon_range = Distance::ZERO;
    soldier.reload = Seconds::new(2.0);
    soldier.moving = Some(walker.stats);
    t.masters[0] = crate::objects::masters::Masters::new([Master {
        id: master_id(126),
        kind: master_type::COMBAT,
        class: class::PREDATOR,
        ..Master::default()
    }]);
    t.players[0].objects.push(Unit {
        id: object_id(50),
        body: crate::objects::layers::Body::at(Position::new(10.4, 10.0, 0.0)),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(126),
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    });
    let placed = |x: f32, class, player, master| Placed {
        position: Position::new(x, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        class,
        player,
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: BuildableOn::NO,
        tall: true,
        moves: false,
        unknown: None,
        obstruction: None,
    };
    t.spatial = crate::map::spatial::Spatial::new(GridSize::new(120, 120));
    t.spatial.insert(
        object_id(11),
        placed(10.0, class::CLASS_0, PlayerId::new(1), master_id(4)),
    );
    t.spatial.insert(
        object_id(50),
        placed(10.4, class::PREDATOR, PlayerId::new(0), master_id(126)),
    );
    let mut rng = Well512::default();
    let unit = &mut t.players[1].objects[ListIndex::new(0)];
    unit.body.put_mover(Some(walker));
    unit.sprite = Some(crate::objects::sprite::ActiveSprite::start(
        Graphic {
            id: SpriteId::named(302),
            ..standing
        },
        &mut rng,
    ));
    unit.body.set_anim_speed(Speed::new(1.0));
    unit.combat.ai = Some(crate::unit_ai::UnitAi {
        ai_type: UnitAiType::RANGED_6,
        current_order: None,
        current_action: None,
        current_target: None,
        stance,
        ..crate::unit_ai::UnitAi::default()
    });
    let task = crate::objects::masters::Task {
        action_type: ATTACK_TASK,
        sprites: TaskSprites::NONE,
        ..crate::objects::masters::Task::default()
    };
    (t, task)
}

/// A world of player 1's soldiers (master 4, formation category 3) and villagers (master 83,
/// category 0) at `units`' places, auto formations on with an influence of 3 tiles, and one
/// group of id 5 holding `members`.
fn soldiers(units: &[(i32, i16, Point)], members: &[i32]) -> World {
    let objects = units
        .iter()
        .map(|&(id, master, at)| Unit {
            id: object_id(id),
            body: crate::objects::layers::Body::at(at.with_height(0.0)),
            group: crate::groups::GroupLink {
                id: if members.contains(&id) {
                    Some(group_id(5))
                } else {
                    None
                },
                flag: -2,
            },
            life: crate::objects::lifecycle::Lifecycle {
                master_id: master_id(master),
                state: ObjectState::Alive,
                ..crate::objects::lifecycle::Lifecycle::default()
            },
            ..Unit::default()
        })
        .collect();
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![
            crate::world::Player::default(),
            crate::world::Player {
                objects,
                formations: crate::world::Formations {
                    auto: true,
                    influence: Distance::new(3.0),
                    by_speed: false,
                    ..crate::world::Formations::default()
                },
                ..crate::world::Player::default()
            },
        ],
    );
    // no sprites: a MoveTo the masters make shows none
    let master = |id, formation_category| Master {
        id,
        terrain_restriction: Some(RestrictionId::named(0)),
        kind: master_type::COMBAT,
        formation_category,
        move_sprite: None,
        standing_sprite: None,
        ..Master::default()
    };
    t.masters = vec![
        crate::objects::masters::Masters::new([]),
        crate::objects::masters::Masters::new([
            master(master_id(4), FormationClass::of_data(3)),
            master(ids::VILLAGER_MALE, FormationClass::UNGROUPED),
        ]),
    ];
    if !members.is_empty() {
        t.groups.groups.push(crate::groups::UnitGroup {
            id: group_id(5),
            members: members
                .iter()
                .map(|&id| crate::groups::Member {
                    id: object_id(id),
                    ..crate::groups::Member::default()
                })
                .collect(),
            ..crate::groups::UnitGroup::default()
        });
    }
    t
}

/// `FUN_140fd76d0`: villagers move loose, soldiers within the influence distance of each
/// other form a group (the one with the most joiners first, by id), one too far moves loose;
/// with Ctrl held each soldier is a group of its own.
#[test]
fn the_splitter_groups_soldiers_near_each_other_and_leaves_villagers_loose() {
    let units = [
        (10, 83, Point::new(10.0, 10.0)),
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(23.5, 20.0)),
        (14, 4, Point::new(40.0, 20.0)),
    ];
    let mut t = soldiers(&units, &[]);
    let split = t
        .split(&object_ids([14, 13, 12, 11, 10]), &|_| true, false)
        .unwrap();
    // 12 has two to join it (11 and 13), 11 and 13 one each
    assert_eq!(split.groups, [object_id_list([12, 11, 13])]);
    assert_eq!(split.loose, object_id_list([10, 14]));
    let split = t
        .split(&object_ids([14, 13, 12, 11, 10]), &|_| true, true)
        .unwrap();
    assert_eq!(split.loose, object_id_list([10]));
    assert_eq!(split.groups.len(), 4);
    // without auto formations every unit moves loose
    t.players[1].formations.auto = false;
    let split = t
        .split(&object_ids([14, 13, 12, 11, 10]), &|_| true, false)
        .unwrap();
    assert_eq!(split.loose, object_id_list([10, 11, 12, 13, 14]));
    assert_eq!(split.groups, Vec::<Vec<ObjectId>>::new());
}

/// `FUN_140fd76d0`'s first pass: units in a group the world holds keep their group with the
/// order's other members, auto formations or not; a member ordered alone moves loose.
#[test]
fn the_splitter_keeps_the_order_s_members_of_an_existing_group_together() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(60.0, 20.0)),
        (13, 4, Point::new(21.0, 20.0)),
    ];
    let mut t = soldiers(&units, &[11, 12]);
    t.players[1].formations.auto = false;
    let split = t
        .split(&object_ids([13, 12, 11]), &|_| true, false)
        .unwrap();
    assert_eq!(split.groups, [object_id_list([11, 12])]);
    assert_eq!(split.loose, object_id_list([13]));
    let split = t.split(&object_ids([12, 13]), &|_| true, false).unwrap();
    assert_eq!(split.loose, object_id_list([12, 13]));
}

/// `FUN_140e55a30`: of the first 60 ids (500 for a computer) one the world lacks or another
/// player's (without the setting `+0x1074`) becomes -1; a human's command left without a unit
/// is refused.
#[test]
fn the_command_check_drops_gone_units_and_others_units_of_its_first_60() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut t = soldiers(&units, &[]);
    t.players[0].objects.push(Unit {
        id: object_id(30),
        ..Unit::default()
    });
    assert_eq!(
        t.command_check(PlayerId::new(1), &object_ids([11, 30, 99, -1]))
            .unwrap(),
        Some(object_ids([11, -1, -1, -1]))
    );
    t.settings.cheats_enabled = true;
    assert_eq!(
        t.command_check(PlayerId::new(1), &object_ids([11, 30, 99]))
            .unwrap(),
        Some(object_ids([11, 30, -1]))
    );
    t.settings.cheats_enabled = false;
    assert_eq!(
        t.command_check(PlayerId::new(1), &object_ids([30, 99]))
            .unwrap(),
        None
    );
    t.players[1].computer = true;
    assert_eq!(
        t.command_check(PlayerId::new(1), &object_ids([30, 99]))
            .unwrap(),
        Some(object_ids([-1, -1]))
    );
    t.players[1].computer = false;
    // past the first 60 nothing is checked
    let mut ids = vec![99; 60];
    ids.push(30);
    let check = |t: &World, ids: &[i32]| {
        t.command_check(PlayerId::new(1), &object_ids(ids.iter().copied()))
    };
    assert_eq!(check(&t, &ids).unwrap(), None);
    ids[0] = 11;
    let checked = check(&t, &ids).unwrap().unwrap();
    assert_eq!(
        (checked[0], checked[1], checked[60]),
        (Some(object_id(11)), None, Some(object_id(30)))
    );
}

/// `FUN_140e6f620`: UNLOAD's check of its first 60 ids (500 for a computer), which alone go on.
/// Its own unit stays and one the world lacks becomes −1. Another player's stays when the two
/// count each other as allies (`FUN_1416642c0`) and the command names the issuer (flag 1) or
/// an object of the issuer's (flags 3 to 6); with the setting `+0x1074` any stays. A human's
/// command left without a unit is refused.
#[test]
fn the_unload_check_keeps_an_ally_s_object_the_command_ties_to_the_issuer() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut t = soldiers(&units, &[]);
    t.players.push(crate::world::Player {
        objects: vec![Unit {
            id: object_id(30),
            ..Unit::default()
        }]
        .into(),
        ..crate::world::Player::default()
    });
    t.players[1].relations = relation_row([3, 0, 4]);
    t.players[2].relations = relation_row([3, 0, 0]);
    let check = |t: &World, flag, named| {
        t.unload_check(
            PlayerId::new(1),
            &object_ids([11, 30, 99]),
            UnloadPick::of_packet(flag, named),
        )
        .unwrap()
    };
    assert_eq!(check(&t, 3, 12), Some(object_ids([11, 30, -1])));
    assert_eq!(check(&t, 6, 12), Some(object_ids([11, 30, -1])));
    assert_eq!(check(&t, 1, 1), Some(object_ids([11, 30, -1])));
    // the named object of another player's, the issuer not named, another flag: out
    assert_eq!(check(&t, 3, 30), Some(object_ids([11, -1, -1])));
    assert_eq!(check(&t, 1, 2), Some(object_ids([11, -1, -1])));
    assert_eq!(check(&t, 0, 12), Some(object_ids([11, -1, -1])));
    assert_eq!(check(&t, 7, 12), Some(object_ids([11, -1, -1])));
    // allied one way only: out, unless the setting lets any through
    t.players[2].relations = relation_row([3, 3, 0]);
    assert_eq!(check(&t, 3, 12), Some(object_ids([11, -1, -1])));
    t.settings.cheats_enabled = true;
    assert_eq!(check(&t, 0, 12), Some(object_ids([11, 30, -1])));
    t.settings.cheats_enabled = false;
    assert_eq!(
        t.unload_check(
            PlayerId::new(1),
            &object_ids([30, 99]),
            UnloadPick::of_packet(3, 12)
        )
        .unwrap(),
        None
    );
    t.players[1].computer = true;
    assert_eq!(
        t.unload_check(
            PlayerId::new(1),
            &object_ids([30, 99]),
            UnloadPick::of_packet(3, 12)
        )
        .unwrap(),
        Some(object_ids([-1, -1]))
    );
    t.players[1].computer = false;
    // past the first 60 nothing goes on
    let mut ids = vec![11; 60];
    ids.push(12);
    assert_eq!(
        t.unload_check(
            PlayerId::new(1),
            &object_ids(ids),
            UnloadPick::of_packet(3, 12)
        )
        .unwrap()
        .map(|c| c.len()),
        Some(60)
    );
}

/// `FUN_140e6b750`: each id's object, live or sleeping, None for a gone one, in the ids' order.
#[test]
fn the_lookup_keeps_a_gone_unit_s_entry() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    t.players[1].sleeping.push(Unit {
        id: object_id(40),
        ..Unit::default()
    });
    assert_eq!(
        t.lookup(&object_ids([99, 40, 11, -1])),
        [
            None,
            Some(Place::Sleeping(PlayerId::new(1), ListIndex::new(0))),
            Some(Place::Live(PlayerId::new(1), ListIndex::new(0))),
            None
        ]
    );
}

/// `FUN_14161e200`: the units of a STOP leave their unit group unless they are exactly its
/// members (a gone unit's entry counts in the list); each unit AI's order queue empties and
/// its idle timer takes its adjusted timeout; another player's unit the check dropped and a
/// sleeping object take nothing.
#[test]
fn a_stop_takes_its_units_out_of_their_group_and_empties_their_orders() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut t = soldiers(&units, &[11, 12]);
    let order = crate::unit_ai::QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: crate::unit_ai::OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(1.0, 1.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(1.0, 1.0),
        ready: true,
        target_type: None,
    };
    t.players[1].objects[ListIndex::new(0)].combat.ai = Some(crate::unit_ai::UnitAi {
        orders: vec![order],
        idle_timer: Millis::new(5),
        adjusted_idle_timeout: Millis::new(300),
        ..crate::unit_ai::UnitAi::default()
    });
    let ai = t.players[1].objects[ListIndex::new(0)].combat.ai.clone();
    t.players[0].objects.push(Unit {
        id: object_id(30),
        combat: crate::objects::layers::Combat {
            ai,
            ..Default::default()
        },
        ..Unit::default()
    });
    t.players[1].sleeping.push(Unit {
        id: object_id(40),
        ..Unit::default()
    });
    t.stop_command(player_id(1), &object_ids([11, 12, 99, 30, 40]))
        .unwrap();
    let ai = t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .as_ref()
        .unwrap();
    assert_eq!((ai.orders.len(), ai.idle_timer), (0, Millis::new(300)));
    assert_eq!(t.players[1].objects[ListIndex::new(0)].group.id, None);
    assert_eq!(t.players[1].objects[ListIndex::new(1)].group.id, None);
    assert_eq!(t.groups.get(group_id(5)).unwrap().members.len(), 0);
    assert_eq!(
        t.players[0].objects[ListIndex::new(0)]
            .combat
            .ai
            .as_ref()
            .unwrap()
            .orders
            .len(),
        1
    );
}

/// With the game setting `+0x1074` (`mCheatsEnabled`) the command's check passes another
/// player's unit, and MOVE and INTERACT (no owner test past it) order it as its own player's:
/// a queued order joins its unit AI's queue with the issuer as the order's.
#[test]
fn an_order_to_another_player_s_unit_goes_to_its_queue_with_the_setting() {
    let mut t = soldiers(&[(11, 83, Point::new(20.0, 20.0))], &[]);
    t.settings.cheats_enabled = true;
    t.players.push(crate::world::Player::default());
    t.player_time.push(Millis::ZERO);
    t.masters.push(crate::objects::masters::Masters::new([]));
    let walker = crate::gathering::hunt::tests::walker();
    if let Some(m) = t.masters[1].get_mut(ids::VILLAGER_MALE) {
        m.moving = Some(walker.stats);
    }
    let unit = &mut t.players[1].objects[ListIndex::new(0)];
    unit.body.put_mover(Some(walker));
    unit.combat.ai = Some(crate::unit_ai::UnitAi::default());
    assert_eq!(
        t.checked_units(PlayerId::new(2), &object_ids([11])),
        object_ids([11])
    );
    let order = MoveOrder {
        target: None,
        point: Point::new(30.0, 30.0),
        queued: true,
        direct: false,
        human: false,
        control: false,
    };
    t.move_order(PlayerId::new(2), &object_ids([11]), &order)
        .unwrap();
    let interact = Interact {
        target: None,
        point: Point::new(25.0, 30.0),
        carry_test: false,
        queued: true,
        direct: false,
        human: false,
        filter: None,
    };
    t.interact(PlayerId::new(2), &object_ids([11]), &interact)
        .unwrap();
    let ai = t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .as_ref()
        .unwrap();
    let queued: Vec<(Option<Issuer>, OrderKind)> =
        ai.orders.iter().map(|o| (o.issuer, o.order)).collect();
    let player_2 = Some(Issuer::Player(player_id(2)));
    assert_eq!(
        queued,
        [(player_2, OrderKind::Move), (player_2, OrderKind::OnTarget)]
    );
}

/// MOVE and INTERACT (`FUN_140e67800`, `FUN_140e68b90`): their loose units in a group the
/// world holds leave it (`FUN_14102c160`), one unit or several; a unit in no group, or in a
/// group the world does not hold, stays as it is.
#[test]
fn the_loose_units_of_a_move_or_an_interact_leave_their_groups() {
    let units = [
        (11, 83, Point::new(20.0, 20.0)),
        (12, 83, Point::new(40.0, 20.0)),
    ];
    let order = MoveOrder {
        target: None,
        point: Point::new(30.0, 30.0),
        queued: false,
        direct: false,
        human: false,
        control: false,
    };
    let mut t = soldiers(&units, &[11]);
    t.move_order(PlayerId::new(1), &object_ids([11]), &order)
        .unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(0)].group.id, None);
    assert_eq!(t.groups.get(group_id(5)).unwrap().members.len(), 0);
    // two groups' members ordered alone: both loose
    let two = || {
        let mut t = soldiers(&units, &[11]);
        t.groups.groups.push(crate::groups::UnitGroup {
            id: group_id(6),
            members: vec![crate::groups::Member {
                id: object_id(12),
                ..crate::groups::Member::default()
            }],
            ..crate::groups::UnitGroup::default()
        });
        t.players[1].objects[ListIndex::new(1)].group.id = Some(group_id(6));
        t
    };
    let mut t = two();
    t.move_order(PlayerId::new(1), &object_ids([11, 12]), &order)
        .unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(0)].group.id, None);
    assert_eq!(t.players[1].objects[ListIndex::new(1)].group.id, None);
    assert_eq!(t.groups.get(group_id(5)).unwrap().members.len(), 0);
    assert_eq!(t.groups.get(group_id(6)).unwrap().members.len(), 0);
    // a group link the world does not hold
    let mut t = soldiers(&units, &[]);
    t.players[1].objects[ListIndex::new(0)].group.id = Some(group_id(9));
    t.move_order(PlayerId::new(1), &object_ids([11]), &order)
        .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].group.id,
        Some(group_id(9))
    );
    let interact = Interact {
        target: None,
        point: Point::new(30.0, 30.0),
        carry_test: false,
        queued: false,
        direct: false,
        human: false,
        filter: None,
    };
    let mut t = two();
    for u in &mut t.players[1].objects {
        u.pasture = None;
        u.my_pasture = None;
    }
    t.interact(PlayerId::new(1), &object_ids([11, 12]), &interact)
        .unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(0)].group.id, None);
    assert_eq!(t.players[1].objects[ListIndex::new(1)].group.id, None);
}

/// A queued MOVE's new group (`FUN_140e67800` at `0x140e67b5d`): the first of its units the
/// world holds (99 is not), with a unit AI, names the target the group's first command takes
/// when the world holds it, at its place; a unit without a unit AI or a target the world
/// does not hold names none.
#[test]
fn a_queued_move_s_new_group_takes_its_first_unit_s_target() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (13, 4, Point::new(21.0, 20.0)),
    ];
    let ai = |target| crate::unit_ai::UnitAi {
        current_order: None,
        current_action: None,
        current_target: target,
        ..crate::unit_ai::UnitAi::default()
    };
    let mut t = soldiers(&units, &[]);
    t.players[1].objects[ListIndex::new(0)].combat.ai = Some(ai(Some(object_id(13))));
    t.players[1].objects[ListIndex::new(1)].combat.ai = Some(ai(Some(object_id(11))));
    assert_eq!(
        t.first_unit_target(&object_id_list([99, 11, 13])),
        Some((object_id(13), Point::new(21.0, 20.0), true))
    );
    assert_eq!(
        t.first_unit_target(&object_id_list([13, 11])),
        Some((object_id(11), Point::new(20.0, 20.0), true))
    );
    t.players[1].objects[ListIndex::new(0)].combat.ai = Some(ai(Some(object_id(98))));
    assert_eq!(t.first_unit_target(&object_id_list([11, 13])), None);
    t.players[1].objects[ListIndex::new(0)].combat.ai = None;
    assert_eq!(t.first_unit_target(&object_id_list([11, 13])), None);
    assert_eq!(t.first_unit_target(&object_id_list([99])), None);
}

/// GATE and `FUN_1411dba30`: player 1's building 50 toggles its lock, a lock setting its last
/// move time to the world time plus 1, an unlock leaving it; player 2's GATE of it does
/// nothing unless players may order each other's units; a unit (type 70) takes nothing.
#[test]
fn a_gate_command_toggles_the_lock_of_a_building_of_its_player() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    t.world_time = WorldTime::from_millis(400);
    t.players.push(crate::world::Player::default());
    t.masters.push(crate::objects::masters::Masters::new([]));
    t.masters[1] = crate::objects::masters::Masters::new([
        Master {
            id: master_id(4),
            kind: master_type::COMBAT,
            ..Master::default()
        },
        Master {
            id: master_id(79),
            kind: BUILDING,
            ..Master::default()
        },
    ]);
    t.players[1].objects.push(Unit {
        id: object_id(50),
        building: crate::objects::layers::Building {
            gate: Some(crate::world::Gate::default()),
            ..Default::default()
        },
        body: crate::objects::layers::Body::Moving(crate::gathering::hunt::tests::walker()),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(79),
            state: ObjectState::Alive,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    });
    let building = |t: &World| {
        let b = &t.players[1].objects[ListIndex::new(1)];
        (
            b.building.gate.unwrap().locked,
            b.body.mover().unwrap().last_move,
        )
    };
    t.gate(player_id(1), object_id(50)).unwrap();
    assert_eq!(building(&t), (true, WorldTime::from_millis(401)));
    t.world_time = WorldTime::from_millis(500);
    t.gate(player_id(2), object_id(50)).unwrap();
    assert_eq!(building(&t), (true, WorldTime::from_millis(401)));
    t.settings.cheats_enabled = true;
    t.gate(player_id(2), object_id(50)).unwrap();
    assert_eq!(building(&t), (false, WorldTime::from_millis(401)));
    t.gate(player_id(1), object_id(11)).unwrap();
    t.gate(player_id(1), object_id(99)).unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(0)].building.gate, None);
}

/// `TRANSFORM_OBJECT` and `FUN_1411e6d90`: player 1's trophy 60 (master 1649) takes trophy
/// 1650, keeping its share of hit points, and in state 2 its build points take the new
/// master's build time; in state 1 they stay.
#[test]
fn a_transformed_building_takes_its_new_master_s_build_time_in_state_2() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    let master = |id, build_time| Master {
        id,
        kind: BUILDING,
        hit_points: MasterHitPoints::of_data(100),
        build_time,
        volley_fire: crate::combat::Firepower::new(f32::from(build_time.get() / 15)),
        ..Master::default()
    };
    t.masters[1] = crate::objects::masters::Masters::new([
        master(TROPHY, WholeSeconds::of_data(10)),
        master(master_id(1650), WholeSeconds::of_data(30)),
    ]);
    let trophy = |state| Unit {
        id: object_id(60),
        combat: crate::objects::layers::Combat {
            firepower: crate::combat::Firepower::new(7.0),
            ..Default::default()
        },
        building: crate::objects::layers::Building {
            construction: Some(crate::construction::build::Construction {
                progress: Seconds::new(10.0),
                desolid: false,
                builders: crate::construction::BuilderCount::of_data(0),
                hit_points: MasterHitPoints::of_data(100),
                build_time: WholeSeconds::of_data(10),
                record: crate::map::spatial::Obstruction {
                    kind: ObstructionType::TYPE_2,
                    class: ObstructionClass::UNIT,
                },
                snow: false,
                sprite_override: false,
            }),
            ..Default::default()
        },
        life: crate::objects::lifecycle::Lifecycle {
            master_id: TROPHY,
            state,
            hp: HitPoints::new(50.0),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[1].objects = vec![trophy(ObjectState::Alive)].into();
    t.transform_object(player_id(1), object_id(60), master_id(1650))
        .unwrap();
    let unit = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(unit.life.master_id, master_id(1650));
    assert_eq!(unit.life.hp, HitPoints::new(50.0));
    assert_eq!(
        unit.building.construction.unwrap().progress,
        Seconds::new(30.0)
    );
    // FUN_141225b70 in state 2: the volleys are the new master's volley fire
    assert_eq!(unit.combat.firepower, crate::combat::Firepower::new(2.0));
    t.players[1].objects = vec![trophy(ObjectState::Unfinished)].into();
    t.transform_object(player_id(1), object_id(60), master_id(1650))
        .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .building
            .construction
            .unwrap()
            .progress,
        Seconds::new(10.0)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].combat.firepower,
        crate::combat::Firepower::new(7.0)
    );
}

/// INTERACT to an object that does not move (`FUN_140e68b90`): it takes the order at once
/// (slot 0x128); a queued order then goes on as a mover's and its unit AI queues it (order
/// 0x2db at the point), with no movement state to touch.
#[test]
fn a_queued_interact_to_an_object_that_does_not_move_also_queues_it() {
    let interact = |queued| Interact {
        target: None,
        point: Point::new(25.0, 30.0),
        carry_test: false,
        queued,
        direct: false,
        human: false,
        filter: None,
    };
    let run = |queued| {
        let mut t = soldiers(&[(11, 83, Point::new(20.0, 20.0))], &[]);
        if let Some(m) = t.masters[1].get_mut(ids::VILLAGER_MALE) {
            m.kind = BUILDING;
        }
        let unit = &mut t.players[1].objects[ListIndex::new(0)];
        (unit.pasture, unit.my_pasture) = (None, None);
        unit.combat.ai = Some(crate::unit_ai::UnitAi::default());
        t.interact(PlayerId::new(1), &object_ids([11]), &interact(queued))
            .unwrap();
        let ai = t.players[1].objects[ListIndex::new(0)]
            .combat
            .ai
            .clone()
            .unwrap();
        ai.orders.iter().map(|o| o.order).collect::<Vec<_>>()
    };
    assert_eq!(run(false), Vec::<OrderKind>::new());
    assert_eq!(run(true), [OrderKind::OnTarget]);
}

/// INTERACT neither queued nor direct to a unit AI holding an order not ready
/// (`FUN_140e68b90` at 0x140e6983a): the order goes into its queue not ready, the readiness
/// pass with 1 makes both ready, and its tail stops the unit (the object's slot 0x148,
/// `FUN_14108c050`, 0x140c42cd4): the AI's order ends and it forgets its targets.
#[test]
fn an_order_to_a_busy_unit_ai_readies_its_queue_and_stops_the_unit() {
    let mut t = soldiers(&[(11, 83, Point::new(20.0, 20.0))], &[]);
    let walker = crate::gathering::hunt::tests::walker();
    if let Some(m) = t.masters[1].get_mut(ids::VILLAGER_MALE) {
        m.moving = Some(walker.stats);
    }
    let unit = &mut t.players[1].objects[ListIndex::new(0)];
    unit.body.put_mover(Some(walker));
    (unit.pasture, unit.my_pasture) = (None, None);
    unit.combat.ai = Some(crate::unit_ai::UnitAi {
        current_order: Some(OrderKind::Move),
        current_action: Some(ActionKind::Move),
        current_target: Some(object_id(99)),
        orders: vec![crate::unit_ai::QueuedOrder {
            order: OrderKind::Move,
            ready: false,
            ..crate::unit_ai::QueuedOrder::default()
        }],
        ..crate::unit_ai::UnitAi::default()
    });
    let interact = Interact {
        target: None,
        point: Point::new(25.0, 30.0),
        carry_test: false,
        queued: false,
        direct: false,
        human: false,
        filter: None,
    };
    t.interact(PlayerId::new(1), &object_ids([11]), &interact)
        .unwrap();
    let ai = t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .clone()
        .unwrap();
    let queue: Vec<_> = ai.orders.iter().map(|o| (o.order, o.ready)).collect();
    assert_eq!(
        queue,
        [(OrderKind::Move, true), (OrderKind::OnTarget, true)]
    );
    assert_eq!((ai.current_order, ai.current_target), (None, None));
}

/// One unit of INTERACT (`FUN_140e68b90`): a unit the world does not hold takes nothing; a
/// queued order's unit AI queues it at the unit's offset from the centroid added to the order's
/// point, the order's own point shown, its ignore list the order's other units; a unit the order
/// targets itself takes it without a work command. A sleeping unit and a mover without movement
/// state are not modelled.
#[test]
fn an_interact_s_unit_queues_its_offset_point_and_takes_no_work_on_itself() {
    let interact = Interact {
        target: None,
        point: Point::new(25.0, 30.0),
        carry_test: false,
        queued: true,
        direct: false,
        human: false,
        filter: None,
    };
    let mover = || {
        let mut t = soldiers(&[(11, 83, Point::new(20.0, 20.0))], &[]);
        let walker = crate::gathering::hunt::tests::walker();
        if let Some(m) = t.masters[1].get_mut(ids::VILLAGER_MALE) {
            m.moving = Some(walker.stats);
        }
        let unit = &mut t.players[1].objects[ListIndex::new(0)];
        unit.body.put_mover(Some(walker));
        (unit.pasture, unit.my_pasture) = (None, None);
        unit.combat.ai = Some(crate::unit_ai::UnitAi::default());
        t.spatial.map = GridSize::new(100, 100);
        t
    };
    let mut t = mover();
    t.interact_unit(PlayerId::new(1), object_id(99), &[], None, &interact)
        .unwrap();
    assert_eq!(t, mover());
    t.interact_unit(
        PlayerId::new(1),
        object_id(11),
        &[object_id(12)],
        Some(Point::new(21.0, 20.0)),
        &interact,
    )
    .unwrap();
    let u = &t.players[1].objects[ListIndex::new(0)];
    let m = u.body.mover().unwrap();
    assert_eq!((m.waiting, &m.ignore_units[..]), (1, &[object_id(12)][..]));
    let queued: Vec<_> = u
        .combat
        .ai
        .as_ref()
        .unwrap()
        .orders
        .iter()
        .map(|o| (o.order, o.point, o.shown, o.ready))
        .collect();
    assert_eq!(
        queued,
        [(
            OrderKind::OnTarget,
            Position::new(24.0, 30.0, 0.0),
            Point::new(25.0, 30.0),
            false
        )]
    );
    let mut t = mover();
    let ai = t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .as_mut()
        .unwrap();
    ai.orders.push(crate::unit_ai::QueuedOrder::default());
    let itself = Interact {
        target: Some(object_id(11)),
        queued: false,
        direct: true,
        ..interact
    };
    t.interact_unit(PlayerId::new(1), object_id(11), &[], None, &itself)
        .unwrap();
    let u = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(u.combat.ai.as_ref().unwrap().orders, []);
    assert_eq!(u.actions.list.move_to().copied(), None);
    let mut t = mover();
    t.players[1].objects[ListIndex::new(0)].body.put_mover(None);
    let reason = |t: &mut World| {
        t.interact_unit(PlayerId::new(1), object_id(11), &[], None, &interact)
            .unwrap_err()
            .reason()
    };
    assert_eq!(reason(&mut t), "an ordered unit without movement state");
    let mut t = mover();
    let u = t.players[1].objects.remove(ListIndex::new(0)).unwrap();
    t.players[1].sleeping.push(u);
    assert_eq!(
        reason(&mut t),
        "an order to a unit the slice does not hold live"
    );
}

/// `FUN_140c42b60` with 1: from the last order to the first, an order not ready becomes
/// ready and a ready one leaves the queue, the others keeping their order; with 0 the ready
/// ones stay.
#[test]
fn the_readiness_pass_readies_the_unready_and_drops_the_ready() {
    // each order told apart by its issuer
    let order = |issuer, ready| crate::unit_ai::QueuedOrder {
        issuer,
        ready,
        ..crate::unit_ai::QueuedOrder::default()
    };
    let mut orders = vec![
        order(Some(Issuer::Player(player_id(1))), true),
        order(Some(Issuer::Player(player_id(2))), false),
        order(Some(Issuer::Player(player_id(3))), true),
        order(Some(Issuer::Player(player_id(4))), false),
    ];
    let mut kept = orders.clone();
    readiness(&mut orders, true);
    assert_eq!(
        orders,
        [
            order(Some(Issuer::Player(player_id(2))), true),
            order(Some(Issuer::Player(player_id(4))), true)
        ]
    );
    let mut none: Vec<crate::unit_ai::QueuedOrder> = Vec::new();
    readiness(&mut none, true);
    assert_eq!(none.len(), 0);
    // with 0 (BUILD's) the ready ones stay and every order is ready
    readiness(&mut kept, false);
    assert_eq!(
        kept,
        [
            order(Some(Issuer::Player(player_id(1))), true),
            order(Some(Issuer::Player(player_id(2))), true),
            order(Some(Issuer::Player(player_id(3))), true),
            order(Some(Issuer::Player(player_id(4))), true)
        ]
    );
}

/// `FUN_14106ff70`'s carried-resource test: a gather or hunt task of another resource than
/// the one held leaves the unit holding none of the task's (the food kinds 0 and 15 to 17
/// are one); an attack task (7 or 9) turns a unit's retargeting off unless it is a villager.
#[test]
fn the_carried_resource_test_drops_another_resource_and_stops_a_soldier_s_retargeting() {
    let task = |action_type, attribute_type| crate::objects::masters::Task {
        action_type,
        attribute_type,
        ..crate::objects::masters::Task::default()
    };
    let run = |master: MasterId, held: Held, task: crate::objects::masters::Task| {
        let mut t = soldiers(&[(11, master.get(), Point::new(20.0, 20.0))], &[]);
        for (id, class) in [(83, class::VILLAGER), (4, class::CLASS_6)] {
            if let Some(m) = t.masters[1].get_mut(master_id(id)) {
                m.class = class;
            }
        }
        let unit = &mut t.players[1].objects[ListIndex::new(0)];
        (unit.life.attribute_type, unit.life.amount) = (held.kind, held.amount);
        unit.combat.ai = Some(crate::unit_ai::UnitAi::default());
        t.carried_resource_test(PlayerId::new(1), ListIndex::new(0), &task)
            .unwrap();
        let unit = &t.players[1].objects[ListIndex::new(0)];
        (
            unit.life.attribute_type,
            unit.life.amount,
            unit.combat.ai.as_ref().unwrap().retarget_disabled,
        )
    };
    // wood held, a gather of gold: none held, of gold
    assert_eq!(
        run(
            ids::VILLAGER_MALE,
            carrying(Some(attribute::WOOD), 10.0),
            task(crate::gathering::gather::GATHER, Some(attribute::GOLD))
        ),
        (Some(attribute::GOLD), Amount::ZERO, 0)
    );
    // meat (17) held, a hunt of food (0): both food, kept
    assert_eq!(
        run(
            ids::VILLAGER_MALE,
            carrying(Some(AttributeId::numbered(17)), 10.0),
            task(HUNT, Some(attribute::FOOD))
        ),
        (Some(AttributeId::numbered(17)), Amount::new(10.0), 0)
    );
    // the same resource: kept
    assert_eq!(
        run(
            ids::VILLAGER_MALE,
            carrying(Some(attribute::GOLD), 4.0),
            task(crate::gathering::gather::GATHER, Some(attribute::GOLD))
        ),
        (Some(attribute::GOLD), Amount::new(4.0), 0)
    );
    // an attack task: a soldier's retargeting goes off, a villager's stays on
    assert_eq!(
        run(master_id(4), carrying(None, 0.0), task(ATTACK_TASK, None)),
        (None, Amount::ZERO, 1)
    );
    assert_eq!(
        run(
            ids::VILLAGER_MALE,
            carrying(None, 0.0),
            task(ATTACK_TASK, None)
        ),
        (None, Amount::ZERO, 0)
    );
}

/// `FUN_141216f10` to a point: the unit AI's order queue empties only with the unit's
/// `+0xa14`, which slot 0x128 with 0 clears while its work command runs; the move follows
/// either way.
#[test]
fn a_move_inside_a_work_command_with_0_keeps_the_order_queue() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    let order = crate::unit_ai::QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: crate::unit_ai::OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(1.0, 1.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(1.0, 1.0),
        ready: true,
        target_type: None,
    };
    t.players[1].objects[ListIndex::new(0)].combat.ai = Some(crate::unit_ai::UnitAi {
        orders: vec![order],
        ..crate::unit_ai::UnitAi::default()
    });
    t.move_point_order_with(
        PlayerId::new(1),
        ListIndex::new(0),
        Position::new(30.0, 30.0, 0.0),
        false,
    )
    .unwrap();
    let unit = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(unit.combat.ai.as_ref().unwrap().orders.len(), 1);
    assert_eq!(
        unit.actions.list.move_to().copied().unwrap().goal(),
        Some(Position::new(30.0, 30.0, 0.0))
    );
    t.move_point_order(
        PlayerId::new(1),
        ListIndex::new(0),
        Position::new(31.0, 30.0, 0.0),
    )
    .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .combat
            .ai
            .as_ref()
            .unwrap()
            .orders
            .len(),
        0
    );
}

/// `FUN_140e666f0`: another player's scout takes the order as its owner's, a unit without
/// bit 4 nothing; of a human's ids only the first 60 count.
#[test]
fn an_auto_scout_reaches_any_player_s_scout_of_its_first_60_ids() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    t.players.push(crate::world::Player::default());
    let scout = Master {
        id: ids::ID_448,
        kind: master_type::COMBAT,
        class: class::CLASS_12,
        object_flags: ObjectFlags::SCOUT,
        // no sprites: the Explore's MoveTo takes none (the world's sprites' table holds none)
        move_sprite: None,
        standing_sprite: None,
        ..Master::default()
    };
    t.masters
        .push(crate::objects::masters::Masters::new([scout.clone()]));
    t.masters[1] = crate::objects::masters::Masters::new([
        scout,
        Master {
            id: master_id(4),
            kind: master_type::COMBAT,
            ..Master::default()
        },
    ]);
    let ai = crate::unit_ai::UnitAi {
        current_order: None,
        current_action: None,
        ..crate::unit_ai::UnitAi::default()
    };
    let unit = |id| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(20.0, 20.0, 0.0)),
        combat: crate::objects::layers::Combat {
            ai: Some(ai.clone()),
            ..Default::default()
        },
        life: crate::objects::lifecycle::Lifecycle {
            master_id: ids::ID_448,
            state: ObjectState::Alive,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[1].objects[ListIndex::new(0)].combat.ai = Some(ai.clone());
    t.players[1].objects.push(unit(object_id(12)));
    t.players[2].objects.push(unit(object_id(30)));
    let mut ids = vec![99; 59];
    ids.extend([30, 11, 12]);
    t.auto_scout(player_id(1), &object_ids(ids)).unwrap();
    let order = |t: &World, p: usize, i: ListIndex| {
        t.players[p].objects[i]
            .combat
            .ai
            .as_ref()
            .unwrap()
            .current_order
    };
    assert_eq!(order(&t, 2, ListIndex::new(0)), Some(OrderKind::AutoScout));
    assert_eq!(order(&t, 1, ListIndex::new(0)), None);
    assert_eq!(order(&t, 1, ListIndex::new(1)), None);
    t.auto_scout(player_id(1), &object_ids([12])).unwrap();
    assert_eq!(order(&t, 1, ListIndex::new(1)), Some(OrderKind::AutoScout));
}

/// `FUN_140c64d50`: the stance and its place are set; `NoAttack` on an attacking unit AI
/// stops its action (keeping an order not of the action), asks for its stopAction and
/// forgets its target; any other stance leaves them.
#[test]
fn no_attack_stops_an_attacking_unit_ai_and_forgets_its_target() {
    let attacking = crate::unit_ai::UnitAi {
        current_order: Some(OrderKind::Attack),
        current_action: Some(ActionKind::Attack),
        current_target: Some(object_id(40)),
        target_class: class_of(6),
        ..crate::unit_ai::UnitAi::default()
    };
    let mut ai = attacking;
    let mut fx: Vec<crate::unit_ai::Effect> = Vec::new();
    set_stance(
        &mut ai,
        UnitStance::DEFENSIVE,
        (Point::new(3.0, 4.0), Some(class::CLASS_6)),
        &mut fx,
        &mut crate::math::rng::Well512::default(),
    )
    .unwrap();
    assert_eq!(
        (ai.stance, ai.state_position),
        (UnitStance::DEFENSIVE, Point::new(3.0, 4.0))
    );
    assert_eq!(
        (ai.current_action, ai.current_target),
        (Some(ActionKind::Attack), Some(object_id(40)))
    );
    assert_eq!(fx, []);
    set_stance(
        &mut ai,
        UnitStance::NO_ATTACK,
        (Point::new(3.0, 5.0), Some(class::CLASS_6)),
        &mut fx,
        &mut crate::math::rng::Well512::default(),
    )
    .unwrap();
    assert_eq!(ai.stance, UnitStance::NO_ATTACK);
    // order 700 is the action's (600 + 100): the stop ends it, the state position the place
    assert_eq!((ai.current_order, ai.current_action), (None, None));
    assert_eq!(
        (ai.current_target, ai.last_target),
        (None, Some(object_id(40)))
    );
    assert_eq!(fx, [crate::unit_ai::Effect::Stop]);
}

/// `FUN_140e6a7c0` after `FUN_140e55d30`: another player's unit takes the stance only while
/// players may order each other's; a class-54 unit takes no stance 1.
#[test]
fn a_stance_reaches_another_player_s_unit_only_under_the_setting() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    t.players.push(crate::world::Player::default());
    t.masters
        .push(crate::objects::masters::Masters::new([Master {
            id: master_id(4),
            kind: master_type::COMBAT,
            class: class::CLASS_54,
            ..Master::default()
        }]));
    t.players[1].objects[ListIndex::new(0)].combat.ai = Some(crate::unit_ai::UnitAi::default());
    t.players[2].objects.push(Unit {
        id: object_id(30),
        combat: crate::objects::layers::Combat {
            ai: Some(crate::unit_ai::UnitAi::default()),
            ..Default::default()
        },
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(4),
            state: ObjectState::Alive,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    });
    let stance = |t: &World, p: usize| {
        t.players[p].objects[ListIndex::new(0)]
            .combat
            .ai
            .as_ref()
            .unwrap()
            .stance
    };
    t.unit_ai_state(
        player_id(1),
        &object_ids([11, 30]),
        UnitStance::STAND_GROUND,
    )
    .unwrap();
    assert_eq!(
        (stance(&t, 1), stance(&t, 2)),
        (UnitStance::STAND_GROUND, UnitStance::AGGRESSIVE)
    );
    t.settings.cheats_enabled = true;
    t.unit_ai_state(player_id(1), &object_ids([30]), UnitStance::DEFENSIVE)
        .unwrap();
    assert_eq!(stance(&t, 2), UnitStance::AGGRESSIVE);
    t.unit_ai_state(player_id(1), &object_ids([30]), UnitStance::NO_ATTACK)
        .unwrap();
    assert_eq!(stance(&t, 2), UnitStance::NO_ATTACK);
}

/// `FUN_1411a6a70` on a unit inside a building (`+0x2f0` set, `+0x2e8` 0): no sight slots and
/// no tile, list or record change; its record (for its way out) takes the new master's values,
/// its hit points their share, its storage leaves and comes as anyone's.
#[test]
fn a_master_change_inside_a_building_keeps_to_its_record() {
    let old = Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        hit_points: MasterHitPoints::of_data(40),
        radius: Footprint::new(0.2, 0.2),
        standing_sprite: None,
        ..Master::default()
    };
    let next = Master {
        id: master_id(5),
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        hit_points: MasterHitPoints::of_data(60),
        radius: Footprint::new(0.3, 0.3),
        standing_sprite: None,
        ..Master::default()
    };
    let mut spatial = crate::map::spatial::Spatial::new(GridSize::new(10, 10));
    spatial.insert(
        object_id(7),
        crate::map::spatial::Placed {
            position: Position::new(3.5, 3.5, 0.0),
            radius: Footprint::new(0.2, 0.2),
            class: class::CLASS_6,
            player: PlayerId::new(1),
            master: master_id(4),
            obstruction_type: ObstructionType::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    spatial.go_inside(object_id(7), Position::new(5.0, 5.0, 0.0));
    let mut unit = Unit {
        id: object_id(7),
        body: crate::objects::layers::Body::at(Position::new(5.0, 5.0, 0.0)),
        life: crate::objects::lifecycle::Lifecycle {
            hp: HitPoints::new(20.0),
            master_id: master_id(4),
            inside: Some(ContainerId::new(object_id(100))),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut pathing = crate::pathing::Pathing::default();
    let graphics = Graphics::new();
    let mut rng = crate::math::rng::Well512::default();
    let world = SwitchWorld {
        spatial: &mut spatial,
        pathing: &mut pathing,
        graphics: &graphics,
        sprite: &mut rng,
    };
    let switched = switch_master(&mut unit, &old, &next, world).unwrap();
    assert!(switched.promoted);
    assert_eq!(
        (unit.life.master_id, unit.life.hp),
        (master_id(5), HitPoints::new(30.0))
    );
    let record = spatial.get(object_id(7)).copied().unwrap();
    assert_eq!(
        (record.master, record.radius),
        (master_id(5), Footprint::new(0.3, 0.3))
    );
    assert!(!spatial.tile_list(Tile::new(5, 5)).contains(&object_id(7)));
}

/// Slot 0x90's unit part ([`switch_master`]) of `unit`, at (3.5, 3.5) in the open, from `old`
/// to `next`, with the graphics `shown`: what it leaves to its owner, the unit after it and its
/// record.
fn switched_in_the_open(
    mut unit: Unit,
    old: &Master,
    next: &Master,
    shown: &[i16],
) -> Result<(actions::Switched, Unit, crate::map::spatial::Placed), GenieError> {
    let mut spatial = crate::map::spatial::Spatial::new(GridSize::new(10, 10));
    spatial.insert(
        unit.id,
        crate::map::spatial::Placed {
            position: Position::new(3.5, 3.5, 0.0),
            radius: old.radius,
            class: old.class,
            player: PlayerId::new(1),
            master: old.id,
            obstruction_type: old.obstruction_type,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: Some(crate::map::spatial::Obstruction {
                kind: ObstructionType::CIRCLE,
                class: ObstructionClass::UNIT,
            }),
        },
    );
    let mut pathing = crate::pathing::Pathing::default();
    let graphics = shown
        .iter()
        .map(|&id| {
            let g = crate::objects::sprite::Graphic {
                id: SpriteId::named(id),
                frames: crate::objects::Frame::of_data(10),
                facets: crate::objects::FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            };
            (g.id, g)
        })
        .collect();
    let mut rng = crate::math::rng::Well512::default();
    let world = SwitchWorld {
        spatial: &mut spatial,
        pathing: &mut pathing,
        graphics: &graphics,
        sprite: &mut rng,
    };
    let switched = switch_master(&mut unit, old, next, world)?;
    let record = spatial.get(unit.id).copied().unwrap();
    Ok((switched, unit, record))
}

/// The two masters of [`switched_in_the_open`]'s tests: 4 (40 hit points, standing sprite 10)
/// and 5 (60, sprite 11, volley fire 3), each with storage of several flags.
fn switch_masters() -> (Master, Master) {
    let old = Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        hit_points: MasterHitPoints::of_data(40),
        radius: Footprint::new(0.2, 0.2),
        standing_sprite: Some(SpriteId::named(10)),
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(1.0), 2),
            (Some(attribute::POPULATION), Amount::new(1.0), 16),
            (Some(AttributeId::numbered(12)), Amount::new(2.0), 32),
            (Some(AttributeId::numbered(13)), Amount::new(3.0), 1),
            (None, Amount::new(1.0), 2),
        ],
        ..Master::default()
    };
    let next = Master {
        id: master_id(5),
        kind: master_type::COMBAT,
        class: class::CLASS_6,
        hit_points: MasterHitPoints::of_data(60),
        radius: Footprint::new(0.3, 0.3),
        standing_sprite: Some(SpriteId::named(11)),
        storage: vec![
            (Some(attribute::HEADROOM), Amount::new(1.0), 2),
            (Some(AttributeId::numbered(14)), Amount::new(1.0), 16),
        ],
        volley_fire: crate::combat::Firepower::new(3.0),
        obstruction_type: ObstructionType::TYPE_3,
        obstruction_class: ObstructionClass::RESOURCE,
        ..Master::default()
    };
    (old, next)
}

/// `FUN_141225b70` with 0 on a unit in the open below state 3: the old master's storage of the
/// flags 2, 16 and 32 leaves, the new master's of flag 2 comes; the hit points keep their share
/// of the new master's (at most them, at least 1); its record and obstruction take the new
/// master's values; it shows the new standing sprite; in state 2 its volleys are the new
/// master's volley fire. In state 3 only the powerups go.
#[test]
fn a_master_change_in_the_open_moves_the_storage_and_shows_the_new_standing_sprite() {
    let (old, next) = switch_masters();
    let unit = |hp, state| Unit {
        id: object_id(7),
        body: crate::objects::layers::Body::at(Position::new(3.5, 3.5, 0.0)),
        life: crate::objects::lifecycle::Lifecycle {
            hp,
            state,
            master_id: master_id(4),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut sprites = crate::math::rng::Well512::default();
    let showing = Unit {
        sprite: Some(crate::objects::sprite::ActiveSprite::start(
            crate::objects::sprite::Graphic {
                id: SpriteId::named(10),
                frames: crate::objects::Frame::of_data(10),
                facets: crate::objects::FacetCount::of_data(8),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            },
            &mut sprites,
        )),
        ..unit(HitPoints::new(20.0), ObjectState::Alive)
    };
    let (switched, u, record) = switched_in_the_open(showing, &old, &next, &[10, 11]).unwrap();
    assert_eq!(
        switched.leaving,
        crate::test_ids::adds([
            (attribute::HEADROOM, -1.0),
            (attribute::POPULATION, -1.0),
            (AttributeId::numbered(12), -2.0)
        ])
    );
    assert_eq!(
        switched.coming,
        crate::test_ids::adds([(attribute::HEADROOM, 1.0)])
    );
    assert!(switched.promoted);
    assert_eq!(
        (u.life.master_id, u.life.hp, u.combat.firepower),
        (
            master_id(5),
            HitPoints::new(30.0),
            crate::combat::Firepower::new(3.0)
        )
    );
    assert_eq!(u.sprite.map(|s| s.graphic.id), Some(SpriteId::named(11)));
    assert_eq!(
        (record.master, record.radius, record.obstruction_type),
        (
            master_id(5),
            Footprint::new(0.3, 0.3),
            ObstructionType::TYPE_3
        )
    );
    assert_eq!(
        record.obstruction,
        Some(crate::map::spatial::Obstruction {
            kind: ObstructionType::TYPE_3,
            class: ObstructionClass::RESOURCE
        })
    );
    let hp = |hp| {
        let (_, u, _) =
            switched_in_the_open(unit(hp, ObjectState::Unfinished), &old, &next, &[10, 11])
                .unwrap();
        (u.life.hp, u.combat.firepower)
    };
    assert_eq!(
        hp(HitPoints::new(50.0)),
        (HitPoints::new(60.0), crate::combat::Firepower::new(0.0))
    );
    assert_eq!(
        hp(HitPoints::new(0.5)),
        (HitPoints::new(1.0), crate::combat::Firepower::new(0.0))
    );
    let (switched, u, record) = switched_in_the_open(
        unit(HitPoints::new(20.0), ObjectState::Dying),
        &old,
        &next,
        &[],
    )
    .unwrap();
    assert!(!switched.promoted && switched.leaving.is_empty());
    assert_eq!(
        (u.life.master_id, record.master),
        (master_id(4), master_id(4))
    );
}

/// [`switch_master`]'s unmodelled parts: a share that changes the damage sprites, a moving
/// unit's new master that does not move and a new standing sprite the data file lacks.
#[test]
fn a_master_change_s_unmodelled_parts_fail() {
    let (old, next) = switch_masters();
    let unit = Unit {
        id: object_id(7),
        life: crate::objects::lifecycle::Lifecycle {
            hp: HitPoints::new(0.1),
            master_id: master_id(4),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    let damaged = Master {
        damage_sprites: true,
        ..old.clone()
    };
    let reason = |r: Result<_, GenieError>| r.unwrap_err().reason();
    assert_eq!(
        reason(switched_in_the_open(
            unit.clone(),
            &damaged,
            &next,
            &[10, 11]
        )),
        "a master change that changes the damage sprites (FUN_1411859a0)"
    );
    let walking = Unit {
        body: crate::objects::layers::Body::Moving(crate::gathering::hunt::tests::walker()),
        ..unit.clone()
    };
    assert_eq!(
        reason(switched_in_the_open(walking, &old, &next, &[10, 11])),
        "a moving unit's new master does not move"
    );
    assert_eq!(
        reason(switched_in_the_open(unit, &old, &next, &[10])),
        "a new master's standing sprite the data file does not have"
    );
}

/// `FUN_1412066b0`: the attack, reload, work rate, most hit points and armor powerups go, the
/// hit points powerup and the reload stinger stay.
#[test]
fn a_master_change_clears_the_powerups_but_the_hit_points_one() {
    let mut unit = Unit {
        combat: crate::objects::layers::Combat {
            attack_powerup: crate::combat::AttackPowerup::new(2.0),
            reload_powerup: Seconds::new(0.5),
            reload_stinger: Seconds::new(0.25),
            work_rate_powerup: WorkRate::new(0.15),
            hp_powerup: HpPowerup {
                taken: HitPoints::new(10.0),
                max: HitPoints::new(20.0),
            },
            armor_powerups: ArmorPair::new(1, 2),
            ..Default::default()
        },
        ..Unit::default()
    };
    clear_powerups(&mut unit);
    assert_eq!(
        (
            unit.combat.attack_powerup,
            unit.combat.reload_powerup,
            unit.combat.work_rate_powerup
        ),
        (
            crate::combat::AttackPowerup::ZERO,
            Seconds::ZERO,
            WorkRate::ZERO
        )
    );
    assert_eq!(
        (unit.combat.hp_powerup, unit.combat.armor_powerups),
        (
            HpPowerup {
                taken: HitPoints::new(10.0),
                max: HitPoints::ZERO
            },
            ArmorPair::ZERO
        )
    );
    assert_eq!(unit.combat.reload_stinger, Seconds::new(0.25));
}

/// `FUN_140a97730`: an empty list and a `MoveTo` head take no move; an Artifact head takes
/// the point (its goal); an action the slice does not hold is refused.
#[test]
fn the_head_action_takes_a_move_only_where_its_slot_0x40_does() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    let point = Position::new(25.0, 21.0, 0.0);
    assert_eq!(
        t.head_takes_move(PlayerId::new(1), ListIndex::new(0), None, point),
        Ok(false)
    );
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .set_move_to(Some(crate::movement::move_to::MoveToAction {
            sprite: None,
            state: crate::movement::move_to::MoveTo::Start,
            point: Position::new(30.0, 30.0, 0.0),
            target: None,
            range: Distance::ZERO,
            quiet: false,
            on_target_moved: crate::movement::move_to::TargetMoved::Follows,
            wait: crate::movement::move_to::MoveWait::default(),
        }));
    assert_eq!(
        t.head_takes_move(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(11)),
            point
        ),
        Ok(false)
    );
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .set_move_to(None);
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .set_artifact(Some(crate::ownership::artifact::Artifact::new(false, None)));
    assert_eq!(
        t.head_takes_move(PlayerId::new(1), ListIndex::new(0), None, point),
        Ok(true)
    );
    let artifact = t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .artifact()
        .copied()
        .unwrap();
    assert_eq!(artifact.goal, Some(point));
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .set_artifact(None);
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .unmodelled_action = Some(GenieError::unsupported("a test's action"));
    assert!(
        t.head_takes_move(PlayerId::new(1), ListIndex::new(0), None, point)
            .is_err()
    );
}

/// `FUN_14161c920`'s box: each unit's x is set against its own y, the units before the last
/// fold into the running least and greatest; units side by side on a diagonal pass, two near
/// each other far off it do not.
#[test]
fn follow_s_box_sets_each_unit_s_x_against_its_y() {
    assert_eq!(
        follow_spread(&[Point::new(20.0, 20.0), Point::new(21.0, 20.0)]),
        Some(Point::new(20.5, 20.0))
    );
    assert_eq!(
        follow_spread(&[Point::new(10.0, 30.0), Point::new(10.5, 30.0)]),
        None
    );
    // the last unit's spread: 8 passes, more does not
    assert_eq!(
        follow_spread(&[Point::new(10.0, 18.0)]),
        Some(Point::new(10.0, 18.0))
    );
    assert_eq!(follow_spread(&[Point::new(10.0, 18.5)]), None);
    // the running spread holds the units before the last
    assert_eq!(
        follow_spread(&[
            Point::new(10.0, 10.0),
            Point::new(30.0, 30.0),
            Point::new(30.0, 30.0)
        ]),
        None
    );
    assert_eq!(follow_spread(&[]), Some(Point::new(0.0, 0.0)));
}

/// A FOLLOW offset stays on the map: below 0 it is 0, past the size less 0.01 that.
#[test]
fn a_follow_offset_stays_on_the_map() {
    assert_eq!(follow_axis(-0.5, 120), 0.0);
    assert_eq!(follow_axis(130.0, 120), 119.99);
    assert_eq!(follow_axis(60.25, 120), 60.25);
    assert!(follow_axis(f32::NAN, 120).is_nan());
}

/// Player 1's soldiers 11 and 12 (moving, each with a unit AI) and player 2's 30 at (40, 20),
/// a master with a footprint of class 6; auto formations off, a map of 120 tiles.
fn followers() -> World {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut t = soldiers(&units, &[]);
    t.players[1].formations.auto = false;
    t.spatial.map = GridSize::new(120, 120);
    let walker = crate::gathering::hunt::tests::walker();
    t.masters[1] = crate::objects::masters::Masters::new([Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        moving: Some(walker.stats),
        ..Master::default()
    }]);
    for u in &mut t.players[1].objects {
        u.body.put_mover(Some(walker.clone()));
        u.combat.ai = Some(crate::unit_ai::UnitAi {
            current_order: None,
            current_action: None,
            current_target: None,
            ..crate::unit_ai::UnitAi::default()
        });
    }
    t.players.push(crate::world::Player::default());
    t.masters
        .push(crate::objects::masters::Masters::new([Master {
            id: master_id(5),
            kind: master_type::COMBAT,
            class: class::CLASS_6,
            radius: Footprint::new(0.5, 0.5),
            ..Master::default()
        }]));
    t.players[2].objects.push(Unit {
        id: object_id(30),
        body: crate::objects::layers::Body::at(Position::new(40.0, 20.0, 0.0)),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(5),
            state: ObjectState::Alive,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    });
    t.spatial.insert(
        object_id(30),
        crate::map::spatial::Placed {
            position: Position::new(40.0, 20.0, 0.0),
            radius: Footprint::new(0.5, 0.5),
            class: class::CLASS_6,
            player: PlayerId::new(2),
            master: master_id(5),
            obstruction_type: ObstructionType::NONE,
            buildable_on: BuildableOn::NO,
            tall: false,
            moves: false,
            unknown: None,
            obstruction: None,
        },
    );
    t
}

/// GUARD (`FUN_14161dee0`): each unit's AI takes the defend order on the target at once,
/// run outside its update: it stops, forgets its target and defends the target within 2.5
/// (the defend action, the target at its place), the order processed and gone. Slot 0x30
/// (`FUN_140c563d0`) makes the order ready (its last argument, 1) and shows it at the
/// target's x and y, whole ones too (430024841's scout guarding a villager at x 144).
#[test]
fn a_guard_gives_its_units_the_defend_order_at_once() {
    for x in [20.3, 40.0] {
        let mut t = followers();
        t.players[2].objects[ListIndex::new(0)].place_for_test(Position::new(x, 20.0, 0.0));
        t.spatial
            .set_position(object_id(30), Position::new(x, 20.0, 0.0))
            .unwrap();
        t.guard(PlayerId::new(1), &object_ids([11]), object_id(30))
            .unwrap();
        let ai = t.players[1].objects[ListIndex::new(0)]
            .combat
            .ai
            .as_ref()
            .unwrap();
        assert_eq!(
            (ai.current_order, ai.current_action, ai.order_priority.get()),
            (Some(OrderKind::Defend), Some(ActionKind::Defend), 100)
        );
        assert_eq!(
            (ai.current_target, ai.defend_target, ai.desired_distance),
            (
                Some(object_id(30)),
                Some(object_id(30)),
                Distance::new(GUARD_RANGE)
            )
        );
        assert_eq!(ai.target_point, Position::new(x, 20.0, 0.0));
        assert_eq!(ai.orders.len(), 0);
    }
}

/// `FUN_14161c920` for one unit: it follows the target (a `MoveTo` to it), its unit AI takes
/// the follow order and action and the target with its class at its place.
#[test]
fn a_follower_takes_the_follow_order_and_walks_to_its_target() {
    let mut t = followers();
    t.follow(PlayerId::new(1), &object_ids([11]), object_id(30))
        .unwrap();
    let unit = &t.players[1].objects[ListIndex::new(0)];
    let ai = unit.combat.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action),
        (Some(OrderKind::Follow), Some(ActionKind::Follow))
    );
    assert_eq!(
        (ai.current_target, ai.target_class),
        (Some(object_id(30)), class_of(6))
    );
    assert_eq!(ai.target_point, Position::new(40.0, 20.0, 0.0));
    assert_eq!(
        ObjectId::field(unit.actions.list.move_to().copied().unwrap().target),
        30
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(1)]
            .actions
            .list
            .move_to()
            .copied(),
        None
    );
}

/// `FUN_14161c920` for two units side by side: each moves to the target at its offset from
/// the two's mean, takes the move order and action, and ignores the other.
#[test]
fn followers_side_by_side_take_their_offsets_from_the_target() {
    let mut t = followers();
    t.follow(PlayerId::new(1), &object_ids([11, 12]), object_id(30))
        .unwrap();
    for (i, (other, x)) in [(12, 39.5), (11, 40.5)].into_iter().enumerate() {
        let unit = &t.players[1].objects[ListIndex::new(i)];
        let ai = unit.combat.ai.as_ref().unwrap();
        assert_eq!(
            (ai.current_order, ai.current_action),
            (Some(OrderKind::Move), Some(ActionKind::Move))
        );
        assert_eq!(
            (ai.current_target, ai.target_point),
            (Some(object_id(30)), Position::new(x, 20.0, 0.0))
        );
        assert_eq!(unit.body.mover().unwrap().ignore_units, [object_id(other)]);
        assert_eq!(
            ObjectId::field(unit.actions.list.move_to().copied().unwrap().target),
            30
        );
    }
}

/// FOLLOW's check: another player's unit takes nothing while players may not order each
/// other's units.
#[test]
fn a_follow_of_another_player_s_unit_does_nothing() {
    let mut t = followers();
    t.follow(PlayerId::new(2), &object_ids([11]), object_id(30))
        .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .combat
            .ai
            .as_ref()
            .unwrap()
            .current_order,
        None
    );
}

/// `FUN_140aaddb0`: a Build on an annex counts its main building's builders one less; a
/// foundation the world no longer holds counts nothing.
#[test]
fn a_builder_leaves_the_main_building_of_an_annex() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    t.masters[1] = crate::objects::masters::Masters::new([Master {
        id: master_id(79),
        kind: BUILDING,
        ..Master::default()
    }]);
    let construction = crate::construction::build::Construction {
        progress: Seconds::ZERO,
        desolid: true,
        builders: crate::construction::BuilderCount::of_data(2),
        hit_points: MasterHitPoints::of_data(100),
        build_time: WholeSeconds::of_data(10),
        record: crate::map::spatial::Obstruction {
            kind: ObstructionType::TYPE_2,
            class: ObstructionClass::UNIT,
        },
        snow: false,
        sprite_override: false,
    };
    let building = |id, annex_of| Unit {
        id,
        building: crate::objects::layers::Building {
            annex_of,
            construction: Some(construction),
            ..Default::default()
        },
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(79),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[1].objects = vec![
        building(object_id(60), None),
        building(object_id(70), Some(MainBuildingId::new(object_id(60)))),
    ]
    .into();
    t.leave_builders(object_id(70)).unwrap();
    let builders = |t: &World, i: ListIndex| {
        t.players[1].objects[i]
            .building
            .construction
            .unwrap()
            .builders
            .get()
    };
    assert_eq!(
        (
            builders(&t, ListIndex::new(0)),
            builders(&t, ListIndex::new(1))
        ),
        (1, 2)
    );
    t.leave_builders(object_id(60)).unwrap();
    t.leave_builders(object_id(99)).unwrap();
    assert_eq!(builders(&t, ListIndex::new(0)), 0);
}

/// `FUN_140c4da50`: the first queued order with a target that the unit itself issued; an
/// order of another issuer or without a target does not count.
#[test]
fn a_unit_s_own_queued_order_names_its_target() {
    let order = |issuer, target| crate::unit_ai::QueuedOrder {
        issuer,
        order: OrderKind::Move,
        priority: crate::unit_ai::OrderPriority::of_data(100),
        target,
        target_owner: None,
        point: Position::new(1.0, 1.0, 0.0),
        range: Distance::ZERO,
        shown: Point::new(1.0, 1.0),
        ready: true,
        target_type: None,
    };
    let mut ai = crate::unit_ai::UnitAi {
        orders: vec![
            order(Some(Issuer::Player(player_id(1))), Some(object_id(50))),
            order(Some(Issuer::Object(object_id(11))), None),
            order(Some(Issuer::Object(object_id(11))), Some(object_id(60))),
            order(Some(Issuer::Object(object_id(11))), Some(object_id(70))),
        ],
        ..crate::unit_ai::UnitAi::default()
    };
    assert_eq!(own_order_target(&ai, object_id(11)), Some(object_id(60)));
    ai.orders.truncate(2);
    assert_eq!(own_order_target(&ai, object_id(11)), None);
}

/// `FUN_140a7a230`: a new target is taken (its workers count one more) and the old one let
/// go; a quiet sub-action does neither; a target in state 7 or more is not taken; the same
/// target changes nothing.
#[test]
fn an_action_s_new_target_is_taken_and_its_old_one_let_go() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    t.masters[0] = crate::objects::masters::Masters::new([Master {
        id: master_id(66),
        kind: master_type::COMBAT,
        ..Master::default()
    }]);
    let deer = |id, state, workers| Unit {
        id,
        workers,
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(66),
            state,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[0].objects = vec![
        deer(
            object_id(40),
            ObjectState::Alive,
            crate::objects::WorkerCount::of_data(1),
        ),
        deer(
            object_id(41),
            ObjectState::Alive,
            crate::objects::WorkerCount::of_data(0),
        ),
        deer(
            object_id(42),
            ObjectState::Removed,
            crate::objects::WorkerCount::of_data(0),
        ),
    ]
    .into();
    let workers = |t: &World| -> Vec<i32> {
        t.players[0]
            .objects
            .iter()
            .map(|u| u.workers.get())
            .collect()
    };
    assert_eq!(
        t.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(40)),
            object_id(40),
            false
        ),
        Ok(Some(object_id(40)))
    );
    assert_eq!(workers(&t), [1, 0, 0]);
    assert_eq!(
        t.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(40)),
            object_id(41),
            true
        ),
        Ok(Some(object_id(41)))
    );
    assert_eq!(workers(&t), [1, 0, 0]);
    assert_eq!(
        t.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(40)),
            object_id(41),
            false
        ),
        Ok(Some(object_id(41)))
    );
    assert_eq!(workers(&t), [0, 1, 0]);
    assert_eq!(
        t.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(41)),
            object_id(42),
            false
        ),
        Ok(None)
    );
    assert_eq!(workers(&t), [0, 0, 0]);
    assert_eq!(
        t.retarget_action(
            PlayerId::new(1),
            ListIndex::new(0),
            Some(object_id(99)),
            // a new target the world does not hold (the game's −1 finds none either)
            object_id(98),
            false
        ),
        Ok(None)
    );
}

/// `FUN_140ab0300`: a Build counted among its foundation's builders leaves them for a new
/// target, which it takes; the same target keeps the count.
#[test]
fn a_build_s_new_target_leaves_the_old_foundation_s_builders() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    t.masters[1] = crate::objects::masters::Masters::new([
        Master {
            id: master_id(4),
            kind: master_type::COMBAT,
            ..Master::default()
        },
        Master {
            id: master_id(79),
            kind: BUILDING,
            ..Master::default()
        },
    ]);
    let foundation = |id, builders| Unit {
        id,
        building: crate::objects::layers::Building {
            construction: Some(crate::construction::build::Construction {
                progress: Seconds::ZERO,
                desolid: true,
                builders,
                hit_points: MasterHitPoints::of_data(100),
                build_time: WholeSeconds::of_data(10),
                record: crate::map::spatial::Obstruction {
                    kind: ObstructionType::TYPE_2,
                    class: ObstructionClass::UNIT,
                },
                snow: false,
                sprite_override: false,
            }),
            ..Default::default()
        },
        workers: crate::objects::WorkerCount::of_data(1),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(79),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[1].objects.push(foundation(
        object_id(60),
        crate::construction::BuilderCount::of_data(1),
    ));
    t.players[1].objects.push(foundation(
        object_id(61),
        crate::construction::BuilderCount::of_data(0),
    ));
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .set_build(Some(crate::construction::build::BuildAction {
            target: Some(object_id(60)),
            state: BuildState::Work,
            move_to: None,
            point: Position::new(-1.0, -1.0, -1.0),
            task: crate::construction::build::BuildTask {
                range: Distance::new(0.1),
                work_value: crate::objects::masters::WorkValue::new(1.0),
                work_rate: WorkRate::new(1.0),
                resource: None,
                id: TaskId::of_task(-1),
                moving: None,
            },
            added: true,
            timer: Seconds::ZERO,
            site: None,
        }));
    assert_eq!(
        t.build_set_target(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(object_id(60)), true),
            object_id(60)
        ),
        Ok(Some(object_id(60)))
    );
    assert!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .list
            .build()
            .copied()
            .unwrap()
            .added
    );
    assert_eq!(
        t.build_set_target(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(object_id(60)), true),
            object_id(61)
        ),
        Ok(Some(object_id(61)))
    );
    let build = t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .build()
        .copied()
        .unwrap();
    assert_eq!((build.target, build.added), (Some(object_id(61)), false));
    let site = |t: &World, i: ListIndex| {
        let u = &t.players[1].objects[i];
        (
            u.building.construction.unwrap().builders.get(),
            u.workers.get(),
        )
    };
    assert_eq!(
        (site(&t, ListIndex::new(1)), site(&t, ListIndex::new(2))),
        ((0, 0), (0, 2))
    );
}

/// `FUN_141211340` and `FUN_14118f580` in a gate that shut: a soldier dies (no hit points),
/// class 42 and livestock with food are spared, livestock with a NaN load is not, and a
/// sleeping object (below type 70) wakes to the end of the live list and dies.
#[test]
fn a_shutting_gate_kills_the_units_in_it_but_the_spared() {
    let units = [(11, 4, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    let master = |id, kind, class| Master {
        id,
        kind,
        class,
        ..Master::default()
    };
    t.masters[1] = crate::objects::masters::Masters::new([
        master(master_id(4), master_type::COMBAT, class::CLASS_6),
        master(ids::ID_42, master_type::COMBAT, class::RELIC),
        master(master_id(58), master_type::COMBAT, class::LIVESTOCK),
        master(master_id(10), master_type::STATIC, class::CLASS_5),
    ]);
    let unit = |id, master, amount| Unit {
        id,
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master,
            hp: HitPoints::new(10.0),
            amount,
            state: ObjectState::Alive,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[1].objects = vec![
        unit(object_id(11), master_id(4), Amount::ZERO),
        unit(object_id(12), ids::ID_42, Amount::ZERO),
        unit(object_id(13), master_id(58), Amount::new(50.0)),
        unit(object_id(14), master_id(58), Amount::new(f32::NAN)),
    ]
    .into();
    t.players[1].sleeping = vec![unit(object_id(15), master_id(10), Amount::ZERO)].into();
    for id in 11..=16 {
        t.shut_in_gate(object_id(id)).unwrap();
    }
    let hp: Vec<(i32, f32)> = t.players[1]
        .objects
        .iter()
        .map(|u| (u.id.get(), u.life.hp.get()))
        .collect();
    assert_eq!(
        hp,
        [(11, 0.0), (12, 10.0), (13, 10.0), (14, 0.0), (15, 0.0)]
    );
    assert_eq!(t.players[1].sleeping.len(), 0);
}

/// `FUN_14106c6a0`: a villager's own master without a task for its player's foundation
/// takes the first master of its class, type and swap group with one, searched with any
/// filter, and becomes it only with the change; one with its own task keeps its master.
#[test]
fn slot_0x820_takes_the_first_master_of_the_swap_group_with_a_task() {
    let units = [(11, 83, Point::new(20.0, 20.0))];
    let mut t = soldiers(&units, &[]);
    let task = |action_type, object_id| crate::objects::masters::Task {
        action_type,
        object_group: None,
        object_id,
        work_val1: crate::objects::masters::WorkValue::new(1.0),
        ..crate::objects::masters::Task::default()
    };
    let villager = |id, tasks| Master {
        id,
        kind: master_type::COMBAT,
        class: class::VILLAGER,
        task_by_group: TaskGroup::MALE_VILLAGERS,
        hit_points: MasterHitPoints::of_data(25),
        tasks,
        ..Master::default()
    };
    t.masters[1] = crate::objects::masters::Masters::new([
        villager(
            ids::VILLAGER_MALE,
            vec![task(crate::gathering::gather::GATHER, Some(master_id(53)))],
        ),
        villager(ids::BUILDER, vec![task(action_type::BUILD, None)]),
        Master {
            id: ids::HOUSE,
            kind: BUILDING,
            class: class::BUILDING,
            hit_points: MasterHitPoints::of_data(550),
            ..Master::default()
        },
    ]);
    t.players[1].objects[ListIndex::new(0)].life.hp = HitPoints::new(25.0);
    t.players[1].objects.push(Unit {
        id: object_id(60),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: ids::HOUSE,
            state: ObjectState::Foundation,
            hp: HitPoints::new(550.0),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    });
    let found = t
        .find_work_task(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(60),
            Some(GARRISON),
            false,
        )
        .unwrap();
    assert_eq!(found.map(|t| t.action_type), Some(action_type::BUILD));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].life.master_id,
        ids::VILLAGER_MALE
    );
    let found = t
        .find_work_task(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(60),
            None,
            true,
        )
        .unwrap();
    assert_eq!(found.map(|t| t.action_type), Some(action_type::BUILD));
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].life.master_id,
        ids::BUILDER
    );
    // 118's own task: no change
    assert!(
        t.find_work_task(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(60),
            None,
            true
        )
        .unwrap()
        .is_some()
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)].life.master_id,
        ids::BUILDER
    );
}

/// `0x140ac60e0` case 6 and `FUN_140ac4c60`: the hunt's own sheep goes to the drop site, and
/// so does the player's idle livestock in state 2 on a tile within 3 that the player sees,
/// then the livestock around that; another player's or one too far stays.
#[test]
fn a_led_sheep_takes_the_idle_livestock_about_it_to_the_drop_site() {
    let units = [(11, 4, Point::new(30.0, 30.0))];
    let mut t = soldiers(&units, &[]);
    t.spatial = crate::map::spatial::Spatial::new(GridSize::new(40, 40));
    let sight = crate::vision::visibility::Visibility::new(
        GridSize::new(40, 40),
        vec![],
        vec![VisionBits::from_bits_retain(u64::MAX); 1600],
        vec![],
        0,
        vec![],
    );
    t.spatial.sight = Some(sight);
    let walker = crate::gathering::hunt::tests::walker();
    let sheep_master = Master {
        id: master_id(594),
        kind: master_type::COMBAT,
        class: class::LIVESTOCK,
        radius: Footprint::new(0.2, 0.2),
        moving: Some(walker.stats),
        ..Master::default()
    };
    t.masters[1] = crate::objects::masters::Masters::new([
        sheep_master.clone(),
        Master {
            id: master_id(4),
            kind: master_type::COMBAT,
            radius: Footprint::new(1.5, 1.5),
            ..Master::default()
        },
    ]);
    t.players.push(crate::world::Player::default());
    t.masters
        .push(crate::objects::masters::Masters::new([sheep_master]));
    let sheep = |id, x: f32, y: f32| Unit {
        id,
        body: crate::objects::layers::Body::new(
            Position::new(x, y, 0.0),
            None,
            Some(walker.clone()),
        ),
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master_id(594),
            state: ObjectState::Alive,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    t.players[1].objects.extend([
        sheep(object_id(20), 10.5, 10.5),
        sheep(object_id(21), 12.5, 11.5),
        sheep(object_id(22), 20.5, 10.5),
    ]);
    t.players[2].objects.push(sheep(object_id(23), 11.5, 10.5));
    let placed = |player, master, at: Point, radius| crate::map::spatial::Placed {
        position: at.with_height(0.0),
        radius: Footprint::new(radius, radius),
        class: class::LIVESTOCK,
        player,
        master,
        obstruction_type: ObstructionType::NONE,
        buildable_on: BuildableOn::NO,
        tall: false,
        moves: true,
        unknown: None,
        obstruction: None,
    };
    t.spatial.insert(
        object_id(11),
        placed(PlayerId::new(1), master_id(4), Point::new(30.0, 30.0), 1.5),
    );
    for (id, p, at) in [
        (20, 1, Point::new(10.5, 10.5)),
        (21, 1, Point::new(12.5, 11.5)),
        (22, 1, Point::new(20.5, 10.5)),
        (23, 2, Point::new(11.5, 10.5)),
    ] {
        t.spatial.insert(
            object_id(id),
            placed(PlayerId::from(p), master_id(594), at, 0.2),
        );
    }
    t.lead_herd(
        PlayerId::new(1),
        object_id(20),
        DropSiteId::new(object_id(11)),
    )
    .unwrap();
    let goes = |p: usize, id: i32| {
        t.players[p]
            .objects
            .iter()
            .find(|u| u.id == object_id(id))
            .and_then(|u| u.actions.list.move_to().copied())
            .map(|m| ObjectId::field(m.target))
    };
    assert_eq!(goes(1, 20), Some(11));
    assert_eq!(goes(1, 21), Some(11));
    assert_eq!(goes(1, 22), None);
    assert_eq!(goes(2, 23), None);
}

/// `FUN_1410395a0`: the old list goes, the new one is copied whole, an empty one too.
#[test]
fn a_group_s_differentiator_groups_are_the_list_given() {
    let mut g = crate::groups::UnitGroup {
        differentiator_groups: vec![group_id(1), group_id(2), group_id(3)],
        ..crate::groups::UnitGroup::default()
    };
    set_differentiator_groups(&mut g, &[group_id(7), group_id(8)]);
    assert_eq!(g.differentiator_groups, [group_id(7), group_id(8)]);
    set_differentiator_groups(&mut g, &[]);
    assert_eq!(g.differentiator_groups, Vec::new());
}

/// `FUN_14118c790`: each id joins the end of the ignore list, a repeated one again.
#[test]
fn an_ignored_unit_joins_the_end_of_the_ignore_list() {
    let mut ignore = vec![object_id(7)];
    for id in [9, 8, 9] {
        ignore_unit(&mut ignore, object_id(id));
    }
    assert_eq!(ignore, object_id_list([7, 9, 8, 9]));
}

/// `FUN_1410862c0`: too close below the least range (COMISS, SETA: NaN not), in reach at
/// most the weapon range (COMISS, JC: NaN out) and not too close.
#[test]
fn a_target_is_in_reach_within_the_weapon_range_and_not_below_the_least() {
    assert_eq!(
        work_reach(Distance::new(3.0), Distance::new(5.0), Distance::new(2.0)),
        (true, false)
    );
    assert_eq!(
        work_reach(Distance::new(5.0), Distance::new(5.0), Distance::new(2.0)),
        (true, false)
    );
    assert_eq!(
        work_reach(Distance::new(5.5), Distance::new(5.0), Distance::new(2.0)),
        (false, false)
    );
    assert_eq!(
        work_reach(Distance::new(1.0), Distance::new(5.0), Distance::new(2.0)),
        (false, true)
    );
    assert_eq!(
        work_reach(Distance::new(2.0), Distance::new(5.0), Distance::new(2.0)),
        (true, false)
    );
    assert_eq!(
        work_reach(
            Distance::new(f32::NAN),
            Distance::new(5.0),
            Distance::new(2.0)
        ),
        (false, false)
    );
    assert_eq!(
        work_reach(Distance::ZERO, Distance::ZERO, Distance::ZERO),
        (true, false)
    );
}

/// Player 1's soldier 10 (master 4, ranges 2 to 5) at (10, 10) and player 2's unit 20 at
/// (`x`, 10), the two enemies, each with a unit AI in state 2.
fn soldier_and_enemy(x: f32, class: ClassId) -> World {
    let unit = |id, master, x: f32| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(x, 10.0, 0.0)),
        combat: crate::objects::layers::Combat {
            ai: Some(crate::unit_ai::UnitAi::default()),
            ..Default::default()
        },
        life: crate::objects::lifecycle::Lifecycle {
            master_id: master,
            state: ObjectState::Alive,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    };
    let player = |objects, relations| crate::world::Player {
        objects,
        relations,
        ..crate::world::Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(1),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO, Millis::ZERO],
        vec![
            player(vec![].into(), relation_row([0, 3, 3])),
            player(
                vec![unit(object_id(10), master_id(4), 10.0)].into(),
                relation_row([3, 0, 3]),
            ),
            player(
                vec![unit(object_id(20), master_id(5), x)].into(),
                relation_row([3, 3, 0]),
            ),
        ],
    );
    let master = |id, class| Master {
        id,
        kind: master_type::COMBAT,
        class,
        weapon_range: Distance::new(5.0),
        min_range: Distance::new(2.0),
        radius: Footprint::new(0.2, 0.2),
        ..Master::default()
    };
    t.masters = vec![
        crate::objects::masters::Masters::new([]),
        crate::objects::masters::Masters::new([master(master_id(4), class)]),
        crate::objects::masters::Masters::new([master(master_id(5), class::CLASS_6)]),
    ];
    for (id, player, x) in [(10, 1, 10.0), (20, 2, x)] {
        t.spatial.insert(
            object_id(id),
            crate::map::spatial::Placed {
                position: Position::new(x, 10.0, 0.0),
                radius: Footprint::new(0.2, 0.2),
                class,
                player: PlayerId::from(player),
                master: master_id(4),
                obstruction_type: ObstructionType::NONE,
                buildable_on: BuildableOn::NO,
                tall: false,
                moves: false,
                unknown: None,
                obstruction: None,
            },
        );
    }
    t
}

/// `FUN_141227190` on an enemy nearer than the least range: the unit posts notify 0x1fe
/// with the target to its own unit AI (slot 0x328) and takes no work.
#[test]
fn an_order_on_an_enemy_below_the_least_range_notifies_the_unit_ai() {
    let mut t = soldier_and_enemy(11.0, class::CLASS_6);
    let work = Work {
        target: Some(object_id(20)),
        point: Position::new(11.0, 10.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let unit = &t.players[1].objects[ListIndex::new(0)];
    let ai = unit.combat.ai.as_ref().unwrap();
    assert_eq!(
        ai.notifies,
        [crate::unit_ai::Notify::own(
            object_id(10),
            NotifyKind::TOO_CLOSE,
            [20, -1, -1]
        )]
    );
    assert_eq!(unit.top_action_kind(), None);
}

/// `FUN_140fd9ae0` with 61 or more: the sorted list (a gone unit null, sorted first) splits
/// in runs of 60, each split on its own (`FUN_140fd76d0`) into the one split: its groups count
/// on, and an id repeated across two runs is in each.
#[test]
fn the_splitter_splits_an_order_to_more_than_60_units_in_runs_of_60() {
    let units: Vec<_> = (100..161)
        .map(|id| (id, 4, Point::new(20.0, 20.0)))
        .collect();
    let mut t = soldiers(&units, &[]);
    let ids = object_ids((100..161).rev());
    let split = t.split(&ids, &|_| true, false).unwrap();
    assert_eq!(split.groups, [object_id_list(100..160)]);
    assert_eq!(split.loose, object_id_list([160]));
    // the gone 999 is the first run's first entry
    let gone: Vec<_> = ids.iter().copied().chain(object_ids([999])).collect();
    let split = t.split(&gone, &|_| true, false).unwrap();
    assert_eq!(
        split.groups,
        [object_id_list(100..159), object_id_list([159, 160])]
    );
    assert_eq!(split.loose, Vec::<ObjectId>::new());
    // 159 twice: the first run's last entry has no next one to repeat it
    let twice = object_ids((100..160).chain([159]));
    let split = t.split(&twice, &|_| true, false).unwrap();
    assert_eq!(split.groups, [object_id_list(100..160)]);
    assert_eq!(split.loose, object_id_list([159]));
    // with Ctrl held the first run fills the 60 groups, the second's group is dropped
    let split = t.split(&ids, &|_| true, true).unwrap();
    assert_eq!(split.groups.len(), 60);
    assert_eq!(split.groups[59], [object_id(159)]);
    assert_eq!(split.loose, Vec::<ObjectId>::new());
}

/// MarvinBlue/426048731's MOVE of 1569: player 2's villagers 9175 and 9177 (by id) take a
/// grid of one cell a row, half a tile apart, from the order's point (their `MoveTos`' points in
/// the save after 1568).
#[test]
fn units_of_a_move_take_the_cells_of_a_grid() {
    let point = Point::new(29.78125, 92.489_586);
    assert_eq!(
        grid_point(0, 2, point, GridSize::new(120, 120)),
        Point::new(29.78125, 92.489_586)
    );
    assert_eq!(
        grid_point(1, 2, point, GridSize::new(120, 120)),
        Point::new(29.78125, 92.989_586)
    );
    // nine units: three a row around the point; off the map an axis keeps a tile inside
    assert_eq!(
        grid_point(0, 9, Point::new(10.0, 10.0), GridSize::new(120, 120)),
        Point::new(9.5, 9.5)
    );
    assert_eq!(
        grid_point(4, 9, Point::new(10.0, 10.0), GridSize::new(120, 120)),
        Point::new(10.0, 10.0)
    );
    assert_eq!(
        grid_point(8, 9, Point::new(119.75, 0.25), GridSize::new(120, 120)),
        Point::new(119.0, 0.75)
    );
    assert_eq!(
        grid_point(0, 9, Point::new(0.25, 0.25), GridSize::new(120, 120)),
        Point::new(1.0, 1.0)
    );
}

/// MarvinBlue/426048731's INTERACT of 927: player 1's builders 9171 and 9173 stand side by
/// side; the queued orders of the save after 927 hold their points around the order's.
#[test]
fn units_of_an_order_take_their_offsets_from_their_centroid() {
    let (a, b) = (
        Point::new(104.32466, 39.299_885),
        Point::new(103.67509, 39.299_885),
    );
    let c = centroid(&[a, b]).unwrap();
    let point = Point::new(108.37596, 43.91338);
    assert_eq!(
        offset_point(a, c, point, GridSize::new(120, 120)),
        Point::new(108.700_745, 43.91338)
    );
    assert_eq!(
        offset_point(b, c, point, GridSize::new(120, 120)),
        Point::new(108.05117, 43.91338)
    );
}

/// A unit left of the first one counted moves only the least x, so the greatest stays 0 and
/// the box passes however far apart the units stand; a unit 9 tiles right of the first does
/// not.
#[test]
fn the_box_of_an_order_moves_only_its_least_value_for_a_lower_one() {
    assert!(centroid(&[Point::new(50.0, 50.0), Point::new(30.0, 50.0)]).is_some());
    assert_eq!(
        centroid(&[Point::new(30.0, 50.0), Point::new(39.0, 50.0)]),
        None
    );
    assert_eq!(
        centroid(&[Point::new(30.0, 50.0), Point::new(38.0, 50.0)]),
        Some(Point::new(34.0, 50.0))
    );
}

/// MarvinBlue/426048731's MOVE of 2979 (the save after 2977): player 2's livestock 9528 and
/// 9529 (formation category 0) stay loose, the scout 9185 has none to join it.
#[test]
fn the_splitter_joins_units_of_one_kind_within_the_influence_distance() {
    let f = crate::world::Formations {
        auto: true,
        influence: Distance::new(3.0),
        by_speed: false,
        ..crate::world::Formations::default()
    };
    let unit = |id, grouped, position: Position| Splittable {
        id: object_id(id),
        group: None,
        kind: master_type::COMBAT,
        grouped,
        turns: false,
        speed: Speed::new(1.2),
        position,
        moved_at: position.ground(),
        state: ObjectState::Alive,
        radius: Distance::new(0.2),
        restriction: Some(RestrictionId::named(1)),
    };
    // no planner: the distance decides
    let mut none = |_: &Splittable, _: &Splittable| Ok(None);
    let mut away = std::collections::BTreeSet::new();
    let scout = unit(9185, true, Position::new(42.047_367, 94.338_486, 0.0));
    let sheep = unit(9529, false, Position::new(45.5, 93.5, 0.0));
    assert!(!splits_together(&scout, &sheep, f, false, &mut none, &mut away).unwrap());
    // nearer than 20 tiles the planner's path decides: its goal reached (1) within the
    // influence distance
    let other = unit(9186, true, Position::new(44.0, 94.0, 0.0));
    let mut short = |_: &Splittable, _: &Splittable| Ok(Some((Found::Goal, Distance::new(2.5))));
    assert!(splits_together(&scout, &other, f, false, &mut short, &mut away).unwrap());
    let mut long = |_: &Splittable, _: &Splittable| Ok(Some((Found::Goal, Distance::new(3.5))));
    assert!(!splits_together(&scout, &other, f, false, &mut long, &mut away).unwrap());
    let mut nearest =
        |_: &Splittable, _: &Splittable| Ok(Some((Found::Nearest, Distance::new(2.5))));
    assert!(!splits_together(&scout, &other, f, false, &mut nearest, &mut away).unwrap());
    // a path that did not join puts the unit among the far ones; a NaN length joins
    assert_eq!(away, [object_id(9186)].into());
    let mut lost =
        |_: &Splittable, _: &Splittable| Ok(Some((Found::Goal, Distance::new(f32::NAN))));
    assert!(splits_together(&scout, &other, f, false, &mut lost, &mut away).unwrap());
    // a NaN distance takes no path and is within the influence distance
    let nowhere = Splittable {
        moved_at: Point::new(f32::NAN, 94.0),
        ..other
    };
    assert!(splits_together(&scout, &nowhere, f, false, &mut short, &mut away).unwrap());
    let far = unit(9187, true, Position::new(80.0, 94.0, 0.0));
    assert!(!splits_together(&scout, &far, f, false, &mut none, &mut away).unwrap());
    let wide = crate::world::Formations {
        influence: Distance::new(40.0),
        ..f
    };
    assert!(splits_together(&scout, &far, wide, false, &mut none, &mut away).unwrap());
    // a member must stand in state 2 or more
    let born = Splittable {
        state: ObjectState::Foundation,
        ..far
    };
    assert!(!splits_together(&scout, &born, wide, true, &mut none, &mut away).unwrap());
}

#[test]
fn a_point_off_the_map_stays_inside_its_edge() {
    let c = Point::new(10.0, 10.0);
    assert_eq!(
        offset_point(
            Point::new(5.0, 15.0),
            c,
            Point::new(2.0, 118.0),
            GridSize::new(120, 120)
        ),
        Point::new(1.0, 119.0)
    );
}

/// `FUN_14161b970`'s timers: 750 ms for the first six entries of the list (the gone ones
/// count), the world's dt more for each six after.
#[test]
fn a_patrol_staggers_its_units_timeouts_by_sixes() {
    assert_eq!(patrol_timeout(0, Millis::new(100)), Millis::new(750));
    assert_eq!(patrol_timeout(5, Millis::new(100)), Millis::new(750));
    assert_eq!(patrol_timeout(6, Millis::new(100)), Millis::new(850));
    assert_eq!(patrol_timeout(13, Millis::new(33)), Millis::new(816));
    assert_eq!(patrol_timeout(499, Millis::ZERO), Millis::new(750));
}

/// `FUN_14161b970`'s filter: a waypoint less than a tile from the last one kept on both x
/// and y is passed over; a NaN difference does not count (`COMISS`/`JNC`).
#[test]
fn a_patrol_passes_over_a_waypoint_within_a_tile_of_the_last() {
    let w = [
        Point::new(10.5, 10.5),
        Point::new(11.0, 10.0),
        Point::new(11.5, 10.9),
        Point::new(11.5, 9.0),
        Point::new(f32::NAN, 9.0),
        Point::new(12.5, 9.0),
    ];
    assert_eq!(
        waypoints_from(Point::new(10.0, 10.0), &w),
        [
            Point::new(11.0, 10.0),
            Point::new(11.5, 9.0),
            Point::new(12.5, 9.0)
        ]
    );
    assert_eq!(
        waypoints_from(Point::new(10.0, 10.0), &w[..1]),
        Vec::<Point>::new()
    );
}

/// Player 1's soldiers of `soldiers`, each walking and with a unit AI off any order, at a
/// world dt of 100 ms.
fn patrollers(units: &[(i32, i16, Point)], members: &[i32]) -> World {
    let mut t = soldiers(units, members);
    t.dt = Millis::new(100);
    for u in &mut t.players[1].objects {
        u.body
            .put_mover(Some(crate::gathering::hunt::tests::walker()));
        u.combat.ai = Some(crate::unit_ai::UnitAi {
            ai_type: UnitAiType::SOLDIER,
            current_order: None,
            current_action: None,
            current_target: None,
            target_class: None,
            idle_timer: Millis::new(400),
            adjusted_idle_timeout: Millis::new(2000),
            secondary_timer: Millis::new(3000),
            patrol_path: Some(crate::unit_ai::PatrolPath::default()),
            patrol_waypoint: crate::movement::WaypointIndex::of_data(2),
            ..crate::unit_ai::UnitAi::default()
        });
    }
    t
}

/// `FORM_FORMATION` (`FUN_14161dba0`) of three soldiers: the splitter makes one group of them, its
/// differentiator the command count, its formation locked to the command's type (5), the group
/// listed as its own differentiator group and the command count one higher.
#[test]
fn a_formation_of_soldiers_makes_one_group_locked_to_its_type() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let mut t = searchable(patrollers(&units, &[]));
    t.form_formation(
        player_id(1),
        &object_ids([11, 12, 13]),
        FormationKind::USER_DEFINED,
    )
    .unwrap();
    let g = &t.groups.groups;
    assert_eq!(g.len(), 1);
    let ids: Vec<ObjectId> = g[0].members.iter().map(|m| m.id).collect();
    assert_eq!(ids, [object_id(11), object_id(12), object_id(13)]);
    assert_eq!(
        g[0].formation_locked_type,
        Some(FormationKind::USER_DEFINED)
    );
    assert_eq!(g[0].differentiator_id, Differentiator::of_data(0));
    assert_eq!(g[0].differentiator_groups, [g[0].id]);
    assert_eq!(t.groups.commands, Differentiator::of_data(1));
    // the same units again: their group takes the command without a new one
    t.form_formation(
        player_id(1),
        &object_ids([11, 12, 13]),
        FormationKind::TYPE_6,
    )
    .unwrap();
    assert_eq!(t.groups.groups.len(), 1);
    assert_eq!(
        t.groups.groups[0].formation_locked_type,
        Some(FormationKind::TYPE_6)
    );
    assert_eq!(
        t.groups.groups[0].differentiator_id,
        Differentiator::of_data(1)
    );
    assert_eq!(t.groups.commands, Differentiator::of_data(2));
}

/// A unit whose order is a defend or a follow stops first: its ignore list empties and its
/// order ends; any other unit's order stays. Units the world lacks make no group, and a human
/// whose units are all gone is refused (no command is counted).
#[test]
fn a_formation_stops_the_units_that_march_and_ignores_gone_ones() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
    ];
    let mut t = searchable(patrollers(&units, &[]));
    for (i, order) in [Some(OrderKind::Defend), Some(OrderKind::Patrol)]
        .into_iter()
        .enumerate()
    {
        let u = &mut t.players[1].objects[ListIndex::new(i)];
        u.combat.ai.as_mut().unwrap().current_order = order;
        u.body.mover_mut().unwrap().ignore_units = vec![object_id(99)];
    }
    t.form_formation(
        player_id(1),
        &object_ids([11, 12]),
        FormationKind::USER_DEFINED,
    )
    .unwrap();
    let marched = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(marched.body.mover().unwrap().ignore_units, []);
    assert_ne!(
        marched.combat.ai.as_ref().unwrap().current_order,
        Some(OrderKind::Defend)
    );
    let other = &t.players[1].objects[ListIndex::new(1)];
    assert_eq!(other.body.mover().unwrap().ignore_units, [object_id(99)]);
    // all gone: refused
    let before = t.groups.commands;
    t.form_formation(
        player_id(1),
        &object_ids([70, 71]),
        FormationKind::USER_DEFINED,
    )
    .unwrap();
    assert_eq!(t.groups.commands, before);
}

/// MarvinBlue/426049701's first PATROL (update 25): player 1's scout alone, one waypoint,
/// `+0x8` set. It moves loose (slot 0x340: a forced, walking `MoveTo` to the waypoint at its
/// height), waits to move with an empty ignore list and no waypoints of its own; its unit AI
/// takes the patrol order, the move action and no target at the waypoint, its timers 0 and
/// 750, its saved patrol gone.
#[test]
fn a_lone_unit_patrols_to_its_waypoint() {
    let mut t = patrollers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    let at = t.players[1].objects[ListIndex::new(0)].position();
    t.players[1].objects[ListIndex::new(0)].place_for_test(at.with_z(1.5));
    t.patrol(
        player_id(1),
        &object_ids([11]),
        &[Point::new(30.0, 20.0)],
        true,
    )
    .unwrap();
    let u = &t.players[1].objects[ListIndex::new(0)];
    let move_to = u.actions.list.move_to().copied().unwrap();
    assert_eq!(
        (
            move_to.goal(),
            ObjectId::field(move_to.target),
            move_to.range
        ),
        (Some(Position::new(30.0, 20.0, 1.5)), -1, Distance::ZERO)
    );
    let m = u.body.mover().unwrap();
    assert_eq!(
        (m.waiting, m.ignore_units.len(), m.user_waypoints.len()),
        (1, 0, 0)
    );
    let ai = u.combat.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.last_order, ai.current_action),
        (Some(OrderKind::Patrol), None, Some(ActionKind::Move))
    );
    assert_eq!(
        (ai.current_target, ai.target_class, ai.target_point),
        (None, None, Position::new(30.0, 20.0, 1.5))
    );
    assert_eq!(
        (ai.idle_timer, ai.adjusted_idle_timeout, ai.secondary_timer),
        (Millis::ZERO, Millis::new(750), Millis::new(750))
    );
    assert_eq!(
        (ai.patrol_path.is_none(), ai.patrol_waypoint.get()),
        (true, 0)
    );
    assert_eq!(t.groups.commands, Differentiator::of_data(1));
}

/// A lone unit's waypoints but the last become its own (slot 0x448, a whole coordinate moved
/// to the tile's middle); the move goes to the last one kept, the unit AI's point is the
/// command's last even where the filter left it out. Without `+0x8` the saved patrol stays.
#[test]
fn a_lone_unit_s_waypoints_but_the_last_become_its_own() {
    let mut t = patrollers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    let w = [
        Point::new(25.0, 20.0),
        Point::new(25.5, 20.5),
        Point::new(30.0, 20.0),
        Point::new(30.5, 20.5),
    ];
    t.patrol(player_id(1), &object_ids([11]), &w, false)
        .unwrap();
    let u = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        u.actions.list.move_to().copied().unwrap().goal(),
        Some(Position::new(30.0, 20.0, 0.0))
    );
    assert_eq!(
        u.body.mover().unwrap().user_waypoints,
        [BVector::new(25.5, 0.0, 20.5)]
    );
    let ai = u.combat.ai.as_ref().unwrap();
    assert_eq!(ai.target_point, Position::new(30.5, 20.5, 0.0));
    assert!(ai.patrol_path.is_some());
}

/// `FUN_14106d340`'s `FUN_140c5dc10` with a completion of the unit AI's current action
/// (no order) pending: the action stops (slot 0xb0 with 1: its value the last, none now) and
/// the unit's stopAction runs at once, before the move.
#[test]
fn a_move_runs_the_stop_of_a_pending_completion_first() {
    let mut t = patrollers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    let ai = t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .as_mut()
        .unwrap();
    ai.current_action = Some(ActionKind::Move);
    ai.notifies = vec![crate::unit_ai::Notify::own(
        object_id(11),
        NotifyKind::ACTION_COMPLETED,
        [ActionKind::Move.value(), 0, 0],
    )];
    assert_eq!(
        t.move_to_point(
            PlayerId::new(1),
            ListIndex::new(0),
            Position::new(30.0, 20.0, 0.0)
        ),
        Ok(false)
    );
    let u = &t.players[1].objects[ListIndex::new(0)];
    let ai = u.combat.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_action, ai.last_action, ai.notifies.len()),
        (None, Some(ActionKind::Move), 0)
    );
    assert_eq!(
        u.actions.list.move_to().copied().unwrap().goal(),
        Some(Position::new(30.0, 20.0, 0.0))
    );
}

/// The attack move of 428506340's update 940 (`+0x8` and `+0xa` set): player 1's lone unit
/// first resets its AI as for an order (`FUN_141631040`: its saved patrol goes even without
/// `+0x8`), then moves loose as a patrol's unit but unforced (a `MoveTo` already to the point
/// stands), its unit AI on the attack move order (`0x2dd`), the move action and no target at
/// the waypoint.
#[test]
fn a_lone_unit_attack_moves_to_its_waypoint() {
    let sweep = Sweep {
        patrol: false,
        reset: false,
        queued: false,
        ignore_pending: true,
    };
    let mut t = patrollers(&[(11, 4, Point::new(20.0, 20.0))], &[]);
    // the reset redraws the adjusted idle timeout from it
    t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .as_mut()
        .unwrap()
        .idle_timeout = Millis::new(3000);
    t.attack_move(
        player_id(1),
        &object_ids([11]),
        &[Point::new(30.0, 20.0)],
        sweep,
    )
    .unwrap();
    let u = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        u.actions.list.move_to().copied().unwrap().goal(),
        Some(Position::new(30.0, 20.0, 0.0))
    );
    let ai = u.combat.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.target_point),
        (
            Some(OrderKind::AttackMove),
            Some(ActionKind::Move),
            Position::new(30.0, 20.0, 0.0)
        )
    );
    assert_eq!(
        (ai.idle_timer, ai.adjusted_idle_timeout, ai.secondary_timer),
        (Millis::ZERO, Millis::new(750), Millis::new(750))
    );
    assert!(ai.patrol_path.is_none());
    // unforced: the MoveTo to the same point stands
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .move_to_mut()
        .unwrap()
        .range = Distance::new(0.25);
    t.attack_move(
        player_id(1),
        &object_ids([11]),
        &[Point::new(30.0, 20.0)],
        sweep,
    )
    .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .list
            .move_to()
            .copied()
            .unwrap()
            .range,
        Distance::new(0.25)
    );
    // a patrol's forced move makes a new one
    t.patrol(
        player_id(1),
        &object_ids([11]),
        &[Point::new(30.0, 20.0)],
        false,
    )
    .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .list
            .move_to()
            .copied()
            .unwrap()
            .range,
        Distance::ZERO
    );
}

/// An attack move through the unit AI's queue (`+0x9`, or without `+0xa` an order in the
/// queue not ready) leaves the record's shown point and ready flag to the stack, which is
/// not modelled; with `+0xa` an unready order does not send it there.
#[test]
fn an_attack_move_through_the_unit_ai_s_queue_is_not_modelled() {
    let sweep = |queued, ignore_pending| Sweep {
        patrol: false,
        reset: true,
        queued,
        ignore_pending,
    };
    let patrollers = |units: &[(i32, i16, Point)]| {
        let mut t = patrollers(units, &[]);
        t.players[1].objects[ListIndex::new(0)]
            .combat
            .ai
            .as_mut()
            .unwrap()
            .idle_timeout = Millis::new(3000);
        t
    };
    let mut t = patrollers(&[(11, 4, Point::new(20.0, 20.0))]);
    assert_eq!(
        t.attack_move(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            sweep(true, true)
        ),
        Err(ATTACK_MOVE_QUEUED)
    );
    let unready = crate::unit_ai::QueuedOrder {
        issuer: Some(Issuer::Player(player_id(1))),
        order: OrderKind::Move,
        priority: crate::unit_ai::OrderPriority::of_data(100),
        target: None,
        target_owner: None,
        point: Position::new(25.0, 20.0, 0.0),
        range: Distance::ZERO,
        shown: Point::ORIGIN,
        ready: false,
        target_type: None,
    };
    let mut t = patrollers(&[(11, 4, Point::new(20.0, 20.0))]);
    t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .as_mut()
        .unwrap()
        .orders = vec![unready];
    assert_eq!(
        t.attack_move(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            sweep(false, false)
        ),
        Err(ATTACK_MOVE_QUEUED)
    );
    let mut t = patrollers(&[(11, 4, Point::new(20.0, 20.0))]);
    t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .as_mut()
        .unwrap()
        .orders = vec![unready];
    assert_eq!(
        t.attack_move(
            player_id(1),
            &object_ids([11]),
            &[Point::new(30.0, 20.0)],
            sweep(false, true)
        ),
        Ok(())
    );
}

/// Loose units of a PATROL (villagers, category 0) list each other in their ignore lists;
/// one whose waypoints all lie within a tile of it stays where it is, its order unchanged.
#[test]
fn loose_patrollers_ignore_each_other_and_one_at_its_waypoint_stays() {
    let mut t = patrollers(
        &[
            (11, 83, Point::new(20.0, 20.0)),
            (12, 83, Point::new(29.5, 20.0)),
        ],
        &[],
    );
    t.patrol(
        player_id(1),
        &object_ids([11, 12]),
        &[Point::new(30.0, 20.0)],
        true,
    )
    .unwrap();
    let [a, b] = t.players[1].objects.as_slice() else {
        panic!("two units")
    };
    assert_eq!(a.body.mover().unwrap().ignore_units, [object_id(12)]);
    assert_eq!(b.body.mover().unwrap().ignore_units, [object_id(11)]);
    assert!(a.actions.list.move_to().is_some() && b.actions.list.move_to().is_none());
    assert_eq!(b.combat.ai.as_ref().unwrap().current_order, None);
    assert_eq!(b.body.mover().unwrap().waiting, 1);
}

/// `FUN_140fd9f40`: the splitter keeps group 5 (the order's 11 and 12 are all of it); the
/// loose 13 of type 70 with a formation category within the owner's influence of the
/// group's place joins it (flags bit 17), the villager does not, and a unit among the far
/// ones does not either.
#[test]
fn a_kept_group_takes_the_loose_soldiers_about_it() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
        (14, 83, Point::new(21.0, 21.0)),
        (15, 4, Point::new(21.0, 19.0)),
    ];
    let mut t = soldiers(&units, &[11, 12]);
    for u in &mut t.players[1].objects {
        let mut m = crate::gathering::hunt::tests::walker();
        m.moved_at = u.position();
        u.body.put_mover(Some(m));
    }
    t.groups.get_mut(group_id(5)).unwrap().motion.position[0] = BVector::new(21.0, 0.0, 20.0);
    let mut split = Split {
        owner: Some(PlayerId::new(1)),
        loose: object_id_list([13, 14, 15]),
        groups: vec![object_id_list([11, 12])],
        far: [object_id(15)].into(),
    };
    t.keep_group(&mut split, group_id(5), None).unwrap();
    assert_eq!(split.loose, object_id_list([14, 15]));
    let g = t.groups.get(group_id(5)).unwrap();
    assert!(g.has(object_id(13)));
    assert_eq!(g.flags & GroupFlags::JOINED, GroupFlags::JOINED);
    // the unit after one that joins is passed over: 13 joins, 15 slides into its place
    let mut t = soldiers(&units, &[11, 12]);
    t.groups.get_mut(group_id(5)).unwrap().motion.position[0] = BVector::new(21.0, 0.0, 20.0);
    let mut split = Split {
        owner: Some(PlayerId::new(1)),
        loose: object_id_list([13, 15]),
        ..Split::default()
    };
    t.keep_group(&mut split, group_id(5), None).unwrap();
    assert_eq!(split.loose, object_id_list([15]));
}

/// `FUN_140fd9f40` with a target the world holds: a loose soldier joins the kept group only when
/// the target is out of its line of sight (the squared gap on the ground above its master's line
/// of sight squared); one that sees it stays loose.
#[test]
fn a_kept_group_takes_only_the_soldiers_that_do_not_see_the_target() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let world = |target_x: f32| {
        let mut t = soldiers(&units, &[11, 12]);
        t.masters[1].get_mut(master_id(4)).unwrap().line_of_sight = Distance::new(5.0);
        t.masters[0] = crate::objects::masters::Masters::new([Master {
            id: ids::HOUSE,
            kind: BUILDING,
            class: class::BUILDING,
            ..Master::default()
        }]);
        t.players[0].objects.push(Unit {
            id: object_id(60),
            body: crate::objects::layers::Body::at(Position::new(target_x, 20.0, 0.0)),
            life: crate::objects::lifecycle::Lifecycle {
                master_id: ids::HOUSE,
                state: ObjectState::Alive,
                ..crate::objects::lifecycle::Lifecycle::default()
            },
            ..Unit::default()
        });
        t.groups.get_mut(group_id(5)).unwrap().motion.position[0] = BVector::new(21.0, 0.0, 20.0);
        for u in &mut t.players[1].objects {
            let mut m = crate::gathering::hunt::tests::walker();
            m.moved_at = u.position();
            u.body.put_mover(Some(m));
        }
        t
    };
    for (target_x, joins) in [(30.0, true), (24.0, false)] {
        let mut t = world(target_x);
        let mut split = Split {
            owner: Some(PlayerId::new(1)),
            loose: object_id_list([13]),
            groups: vec![object_id_list([11, 12])],
            ..Split::default()
        };
        t.keep_group(&mut split, group_id(5), Some(object_id(60)))
            .unwrap();
        assert_eq!(split.loose.is_empty(), joins, "target at x {target_x}");
        assert_eq!(t.groups.get(group_id(5)).unwrap().has(object_id(13)), joins);
    }
}

/// [`patrollers`]' two soldiers near each other in a world whose statistics search works.
fn two_patrollers() -> World {
    searchable(patrollers(
        &[
            (11, 4, Point::new(20.0, 20.0)),
            (12, 4, Point::new(21.0, 20.0)),
        ],
        &[],
    ))
}

/// `t` on a 60 by 60 map of open land, its sight's registry for the searches.
fn searchable(mut t: World) -> World {
    // the group's statistics search the members' surroundings
    let mut s = crate::map::spatial::Spatial::with_tiles(
        GridSize::new(60, 60),
        crate::map::spatial::Tiles::new(GridSize::new(60, 60), vec![1; 3600]),
    );
    let kind = crate::map::terrain::TerrainKind {
        shown: true,
        flags: TerrainFlags::WATER,
        forest: false,
    };
    s.terrain = Some(crate::map::terrain::Terrain::new(
        GridSize::new(60, 60),
        vec![TileTerrain::of_save(0, 255, -1); 3600],
        vec![kind],
        vec![Restriction::new(vec![1.0])],
    ));
    s.relations = ByPlayer::new(vec![relation_row([0, 3]), relation_row([0, 0])]);
    s.unit_diplomacy = ByPlayer::new(vec![relations([1, 4]), relations([0, 1])]);
    let mut v = crate::vision::visibility::Visibility::new(
        GridSize::new(60, 60),
        vec![],
        vec![],
        vec![],
        0,
        vec![],
    );
    v.registry = Some(crate::vision::registry::Registry::new(2, 65));
    s.sight = Some(v);
    t.spatial = s;
    t
}

/// An attack move of two soldiers near each other: their group's move to the last waypoint
/// holds (`commandMoveTo` with `param_6` 0, `param_8` 1): its path does not loop (bit 0)
/// and its flags bit 11 is set, where a patrol's loops and leaves it clear.
#[test]
fn an_attack_moving_group_holds_its_path_to_the_last_waypoint() {
    let sweep = Sweep {
        patrol: false,
        reset: true,
        queued: false,
        ignore_pending: true,
    };
    let mut t = two_patrollers();
    for u in &mut t.players[1].objects {
        u.combat.ai.as_mut().unwrap().idle_timeout = Millis::new(3000);
    }
    t.attack_move(
        player_id(1),
        &object_ids([11, 12]),
        &[Point::new(25.0, 25.0), Point::new(30.0, 20.0)],
        sweep,
    )
    .unwrap();
    let g = &t.groups.groups[0];
    let top = g.motion.paths.last().unwrap();
    assert_eq!(
        (
            top.path_flags & PathFlags::LOOPS,
            g.flags & GroupFlags::PATROL
        ),
        (PathFlags::NONE, GroupFlags::PATROL)
    );
    let mut t = two_patrollers();
    t.patrol(
        player_id(1),
        &object_ids([11, 12]),
        &[Point::new(25.0, 25.0), Point::new(30.0, 20.0)],
        true,
    )
    .unwrap();
    let g = &t.groups.groups[0];
    let top = g.motion.paths.last().unwrap();
    assert_eq!(
        (
            top.path_flags & PathFlags::LOOPS,
            g.flags & GroupFlags::PATROL
        ),
        (PathFlags::LOOPS, GroupFlags::NONE)
    );
}

/// A PATROL of two soldiers near each other: the splitter's group becomes a new group of
/// player 1, listed (its differentiators the command count before and itself), with the
/// waypoints but the last as its own and a looping path to the last, back and forth from 5
/// tiles; the command count goes on.
#[test]
fn a_patrolling_group_walks_a_loop_through_its_waypoints() {
    let mut t = two_patrollers();
    let w = [Point::new(25.0, 25.0), Point::new(30.0, 20.0)];
    t.patrol(player_id(1), &object_ids([11, 12]), &w, true)
        .unwrap();
    assert_eq!(t.groups.groups.len(), 1);
    let g = &t.groups.groups[0];
    assert_eq!(
        (g.owner, g.has(object_id(11)), g.has(object_id(12))),
        (Some(PlayerId::new(1)), true, true)
    );
    assert_eq!(
        (g.differentiator_id, g.current_command),
        (Differentiator::of_data(0), GroupCommandKind::MOVE)
    );
    let top = g.motion.paths.last().unwrap();
    assert_eq!(
        top.path_flags & (PathFlags::LOOPS | PathFlags::BACK_AND_FORTH),
        PathFlags::LOOPS | PathFlags::BACK_AND_FORTH
    );
    assert_eq!(top.points.len(), 3);
    assert_eq!(
        top.points[1..],
        [BVector::new(25.0, 0.0, 25.0), BVector::new(30.0, 0.0, 20.0)]
    );
    assert_eq!(t.groups.commands, Differentiator::of_data(1));
    let ai = t.players[1].objects[ListIndex::new(0)]
        .combat
        .ai
        .as_ref()
        .unwrap();
    assert_eq!(
        (ai.adjusted_idle_timeout, ai.patrol_path.is_none()),
        (Millis::new(750), true)
    );
}

/// MOVE (`FUN_140e67800`) of three soldiers in no group: the splitter groups them (each has
/// two joiners: the first by id leads), and a new group of their owner (`FUN_14152efc0`)
/// takes them, its statistics, its place and the command; with Ctrl held each is a group of
/// its own (its formation the line's lock) and the three list each other as their
/// differentiator groups.
#[test]
fn a_move_of_soldiers_in_no_group_forms_new_groups() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let order = MoveOrder {
        target: None,
        point: Point::new(40.0, 40.0),
        queued: false,
        direct: false,
        human: false,
        control: false,
    };
    let world = || searchable(patrollers(&units, &[]));
    let mut t = world();
    t.move_order(PlayerId::new(1), &object_ids([11, 12, 13]), &order)
        .unwrap();
    let g = &t.groups.groups;
    assert_eq!(g.len(), 1);
    let ids: Vec<ObjectId> = g[0].members.iter().map(|m| m.id).collect();
    assert_eq!(ids, [object_id(11), object_id(12), object_id(13)]);
    assert_eq!(t.groups.commands, Differentiator::of_data(1));
    let mut t = world();
    let ctrl = MoveOrder {
        control: true,
        ..order
    };
    t.move_order(PlayerId::new(1), &object_ids([11, 12, 13]), &ctrl)
        .unwrap();
    let g = &t.groups.groups;
    assert_eq!(g.len(), 3);
    for g in g {
        assert_eq!(g.members.len(), 1);
        assert_eq!(
            g.differentiator_groups,
            [group_id(0), group_id(1), group_id(2)]
        );
    }
}

/// MOVE (`FUN_140e67800`) of the whole of group 5 and a loose soldier beside it: the
/// splitter keeps the group (`FUN_140fd9f40`), which takes the soldier and the command
/// (`BUnitGroup::command`); no new group is made.
#[test]
fn a_move_of_a_whole_group_keeps_it() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let order = MoveOrder {
        target: None,
        point: Point::new(40.0, 40.0),
        queued: false,
        direct: false,
        human: false,
        control: false,
    };
    let kept = || {
        let mut t = soldiers(&units, &[11, 12]);
        t.groups.get_mut(group_id(5)).unwrap().motion.position[0] = BVector::new(21.0, 0.0, 20.0);
        t
    };
    let mut t = kept();
    t.move_order(PlayerId::new(1), &object_ids([11, 12, 13]), &order)
        .unwrap();
    assert_eq!(t.groups.groups.len(), 1);
    let g = t.groups.get(group_id(5)).unwrap();
    assert!(g.has(object_id(13)));
    // the keep sets flags bit 17, which the group's command then clears
    assert_eq!(
        (g.flags & GroupFlags::JOINED, g.differentiator_id),
        (GroupFlags::NONE, Differentiator::of_data(0))
    );
    assert_eq!(t.groups.commands, Differentiator::of_data(1));
    let mut t = kept();
    // with a target the world holds the kept group takes the command on it (a unit of the
    // order itself, which has no task: the plain move to it)
    let at_12 = MoveOrder {
        target: Some(object_id(12)),
        ..order
    };
    assert!(
        t.move_order(PlayerId::new(1), &object_ids([11, 12, 13]), &at_12)
            .is_ok()
    );
}

/// The Gather's slot 0x48 on a building (`0x140a903a0`, [`World::retarget_gather`]): its
/// player's farm in state 2 (class 49) it takes, walking there; its player's farm foundation
/// (the task finder's build), a building at work (packing) and an annex whose main building is
/// gone are not modelled; another building it does not take (false), so the work command makes
/// a new action.
#[test]
fn a_gather_takes_a_farm_but_no_other_building() {
    let mut t = crate::world::tests::forage_world();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    t.masters[1] =
        crate::objects::masters::Masters::new(t.masters[1].iter().cloned().chain([Master {
            id: ids::FARM,
            kind: BUILDING,
            class: class::FARM,
            radius: Footprint::new(1.5, 1.5),
            ..Master::default()
        }]));
    let farm = |id, state, annex_of| Unit {
        id,
        body: crate::objects::layers::Body::at(Position::new(26.5, 89.5, 0.0)),
        building: crate::objects::layers::Building {
            annex_of,
            ..Default::default()
        },
        life: crate::objects::lifecycle::Lifecycle {
            state,
            master_id: ids::FARM,
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        pasture: None,
        my_pasture: None,
        ..Unit::default()
    };
    t.players[1]
        .objects
        .push(farm(object_id(800), ObjectState::Alive, None));
    t.players[1]
        .objects
        .push(farm(object_id(801), ObjectState::Foundation, None));
    t.players[1].objects.push(farm(
        object_id(802),
        ObjectState::Alive,
        Some(MainBuildingId::new(object_id(4444))),
    ));
    let world = t.clone();
    assert_eq!(
        t.retarget_gather(PlayerId::new(1), ListIndex::new(0), object_id(800)),
        Ok(true)
    );
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .gather()
        .copied()
        .unwrap();
    assert_eq!(
        (g.target, g.state, g.point),
        (
            Some(object_id(800)),
            GatherState::Walk,
            Position::new(26.5, 89.5, 0.0)
        )
    );
    assert_eq!(
        (
            t.players[0].objects[ListIndex::new(0)].workers,
            t.players[1].objects[ListIndex::new(3)].workers
        ),
        (
            crate::objects::WorkerCount::of_data(0),
            crate::objects::WorkerCount::of_data(1)
        )
    );
    let mut t = world.clone();
    assert!(
        t.retarget_gather(PlayerId::new(1), ListIndex::new(0), object_id(801))
            .is_err()
    );
    assert!(
        t.retarget_gather(PlayerId::new(1), ListIndex::new(0), object_id(802))
            .is_err()
    );
    t.players[1].objects[ListIndex::new(3)]
        .actions
        .unmodelled_action = Some(GenieError::unsupported("a pack"));
    assert!(
        t.retarget_gather(PlayerId::new(1), ListIndex::new(0), object_id(800))
            .is_err()
    );
    let mut t = world;
    assert_eq!(
        t.retarget_gather(PlayerId::new(1), ListIndex::new(0), object_id(700)),
        Ok(false)
    );
    assert!(
        t.retarget_gather(PlayerId::new(1), ListIndex::new(0), object_id(4444))
            .is_err()
    );
}

/// [`crate::world::tests::forage_world`]'s forager 9175 carrying 6 berries (kind 17), its
/// Gather on bush 600 from a work command, its player holding 10 attributes.
fn foraging() -> World {
    let mut t = crate::world::tests::forage_world();
    let work = Work {
        target: Some(object_id(600)),
        point: Position::new(32.5, 89.5, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    t.players[1].attributes = vec![Amount::ZERO; 10];
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    (v.life.attribute_type, v.life.amount) = (Some(AttributeId::numbered(17)), Amount::new(6.0));
    t
}

/// The drop-off order's ask (`FUN_14161e370`): the orders 7 and 8 ask a top Gather, Build or
/// Hunt, whose task's resource (berries, kind 0) asks for a load of its kind and not for wood
/// (kind 1); the order 8 to an action without a target or a point is unaimed; the order 6
/// and a unit without such an action ask nothing.
#[test]
fn a_drop_off_order_asks_the_top_gather_build_or_hunt() {
    use super::unit_order::{DropOffAsk, DropOffBy};
    let mut t = foraging();
    let ask = |by, asked, unaimed| Some(DropOffAsk { by, asked, unaimed });
    assert_eq!(
        t.drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        ask(DropOffBy::Gather, true, false)
    );
    assert_eq!(
        t.drop_off_ask(PlayerId::new(1), ListIndex::new(0), (false, true))
            .unwrap(),
        None
    );
    t.players[1].objects[ListIndex::new(0)].life.attribute_type = Some(attribute::WOOD);
    assert_eq!(
        t.drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, false))
            .unwrap(),
        ask(DropOffBy::Gather, false, false)
    );
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .gather_mut()
        .unwrap();
    (g.target, g.point) = (None, Position::new(NO_POINT, NO_POINT, 0.0));
    assert_eq!(
        t.drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        ask(DropOffBy::Gather, false, true)
    );
    assert_eq!(
        t.drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, false))
            .unwrap(),
        ask(DropOffBy::Gather, false, false)
    );
    // a Hunt and a Build of a wood task, aimed
    let mut hunt = crate::gathering::hunt::tests::hunting(600);
    hunt.task.resource = Some(attribute::WOOD);
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.actions.list.set_gather(None);
    v.actions.list.set_hunt(Some(hunt));
    assert_eq!(
        t.drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        ask(DropOffBy::Hunt, true, false)
    );
    let task = crate::construction::build::BuildTask {
        range: Distance::new(0.1),
        work_value: crate::objects::masters::WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: Some(attribute::WOOD),
        id: TaskId::of_task(-1),
        moving: None,
    };
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.actions
        .list
        .set_build(Some(crate::construction::build::tests::started(
            object_id(10028),
            Position::new(10.5, 10.5, 0.0),
            Distance::new(5.0),
            task,
        )));
    assert_eq!(
        t.drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        ask(DropOffBy::Build, true, false)
    );
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.actions.list.set_build(None);
    v.actions.list.set_hunt(None);
    assert_eq!(
        t.drop_off_ask(PlayerId::new(1), ListIndex::new(0), (true, true))
            .unwrap(),
        None
    );
}

/// The drop-off order 7 to the forager's Gather: it carries its berries to the town centre
/// 700 (state 8), which counts it as a worker; a load of wood instead posts the other load
/// (0x21c) and leaves the Gather; the order 8 to an unaimed Gather records its work for the
/// town bell, drops off and queues the way back.
#[test]
fn a_drop_off_order_takes_a_gather_to_its_site_or_posts_the_other_load() {
    let mut t = foraging();
    t.unit_order(player_id(1), &object_ids([9175]), UnitOrderAction::DropOff)
        .unwrap();
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .gather()
        .copied()
        .expect("the Gather");
    assert_eq!(
        (g.carry_to, g.state),
        (Some(DropSiteId::new(object_id(700))), GatherState::Drop)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(2)].workers,
        crate::objects::WorkerCount::of_data(1)
    );
    let mut t = foraging();
    t.players[1].objects[ListIndex::new(0)].life.attribute_type = Some(attribute::WOOD);
    let before = t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .gather()
        .copied();
    t.unit_order(player_id(1), &object_ids([9175]), UnitOrderAction::DropOff)
        .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(v.actions.list.gather().copied(), before);
    let ai = v.combat.ai.as_ref().expect("the forager's unit AI");
    assert!(
        ai.notifies.iter().any(|n| n.kind == NotifyKind::OTHER_LOAD),
        "{:?}",
        ai.notifies
    );
    let mut t = foraging();
    let g = t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .gather_mut()
        .unwrap();
    (g.target, g.point) = (None, Position::new(NO_POINT, NO_POINT, 0.0));
    t.unit_order(
        player_id(1),
        &object_ids([9175]),
        UnitOrderAction::DropOffAndBack,
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        v.actions.list.gather().map(|g| (g.carry_to, g.state)),
        Some((Some(DropSiteId::new(object_id(700))), GatherState::Drop))
    );
    assert_eq!(v.combat.ai.as_ref().map(|ai| ai.orders.len()), Some(1));
}

/// The drop-off order 7 to a Hunt carrying its load: it goes to the town centre 700 (state 8),
/// which counts it as a worker.
#[test]
fn a_drop_off_order_takes_a_hunt_to_its_site() {
    let mut t = foraging();
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    v.actions.list.set_gather(None);
    let mut hunt = crate::gathering::hunt::tests::hunting(600);
    hunt.task.resource = Some(AttributeId::numbered(17));
    // the sprites' table of the world holds none
    hunt.task.sprites = TaskSprites::NONE;
    v.actions.list.set_hunt(Some(hunt));
    t.unit_order(player_id(1), &object_ids([9175]), UnitOrderAction::DropOff)
        .unwrap();
    let h = t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .hunt()
        .copied()
        .expect("the Hunt");
    assert_eq!(
        (h.carry_to, h.state),
        (Some(DropSiteId::new(object_id(700))), HuntState::Drop)
    );
    assert_eq!(
        t.players[1].objects[ListIndex::new(2)].workers,
        crate::objects::WorkerCount::of_data(1)
    );
}

/// The drop-off orders fail on a sleeping object and on a unit whose action the slice does not
/// hold; they pass over a unit not in state 2, below type 40 or carrying nothing.
#[test]
fn a_drop_off_order_fails_on_what_it_does_not_hold() {
    let mut t = foraging();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .unmodelled_action = Some(GenieError::unsupported("a test's action"));
    assert_eq!(
        t.unit_order(player_id(1), &object_ids([9175]), UnitOrderAction::DropOff)
            .err()
            .map(|e| e.reason()),
        Some("a UNIT_ORDER drop-off of an action not held")
    );
    let mut t = foraging();
    t.players[1].objects[ListIndex::new(0)].life.amount = Amount::ZERO;
    let before = t.players[1].objects[ListIndex::new(0)].clone();
    t.unit_order(
        player_id(1),
        &object_ids([9175, 12345]),
        UnitOrderAction::DropOff,
    )
    .unwrap();
    assert_eq!(t.players[1].objects[ListIndex::new(0)], before);
}

/// The attack task's start ends where the game's start does ([`crate::combat::attack::Attack::command`]
/// on the soldier's step): with the wolf out of reach a soldier that may move posts 0x1fd to its
/// unit AI and turns to state 4, and one that cannot move (no unit AI) fails at once
/// (`set_state(13)`: its animation at speed 1, its standing sprite); with the wolf in reach a
/// soldier that may move starts its `MoveTo` arrived and shows its standing sprite.
#[test]
fn an_attack_task_s_start_ends_where_the_start_does() {
    use crate::combat::attack::AttackState;
    let started = |ai: bool, at: f32| {
        let (mut t, task) = work_attack(UnitStance::AGGRESSIVE);
        let unit = &mut t.players[1].objects[ListIndex::new(0)];
        if !ai {
            unit.combat.ai = None;
        }
        unit.body.set_anim_speed(Speed::new(0.5));
        let mut wolf = *t.spatial.get(object_id(50)).unwrap();
        wolf.position = Position::new(at, 10.0, 0.0);
        t.spatial.insert(object_id(50), wolf);
        t.players[0].objects[ListIndex::new(0)].body =
            crate::objects::layers::Body::at(Position::new(at, 10.0, 0.0));
        t.start_attack(
            PlayerId::new(1),
            ListIndex::new(0),
            object_id(50),
            &task,
            Position::new(at, 10.0, 0.0),
            true,
        )
        .unwrap();
        let unit = &t.players[1].objects[ListIndex::new(0)];
        let state = unit.actions.list.attack().map(|a| a.state);
        let notifies = unit.combat.ai.as_ref().map(|ai| ai.notifies.len());
        let sprite = unit.sprite.map(|s| s.graphic.id);
        (state, unit.body.anim_speed(), sprite, notifies)
    };
    let (near, _, sprite, near_notifies) = started(true, 10.4);
    assert_eq!(sprite, Some(SpriteId::named(300)));
    let (far, _, _, far_notifies) = started(true, 30.0);
    assert_eq!(
        (far, far_notifies),
        (Some(AttackState::Move), near_notifies.map(|n| n + 1))
    );
    assert_ne!(near, None);
    let (failed, speed, sprite, _) = started(false, 30.0);
    assert_eq!(
        (failed, speed, sprite),
        (
            Some(AttackState::Failed),
            Some(Speed::new(1.0)),
            Some(SpriteId::named(300))
        )
    );
}

/// The work command's task (`FUN_14106ff70`): none on a point; the forager's Gather task 2 on
/// Gaia's bush (its master 120); none on its town centre, which no task of it names.
#[test]
fn a_work_command_finds_its_master_and_task_on_a_target() {
    let t = crate::world::tests::forage_world();
    assert_eq!(
        t.work_task(
            PlayerId::new(1),
            ListIndex::new(0),
            (None, Position::from([1.0; 3])),
            None
        )
        .unwrap(),
        None
    );
    let (with, task) = t
        .work_task(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(object_id(600)), Position::from([1.0; 3])),
            None,
        )
        .unwrap()
        .expect("the Gather task");
    assert_eq!(
        (with, task.id, task.action_type),
        (
            master_id(120),
            TaskId::test(2),
            crate::gathering::gather::GATHER
        )
    );
    assert_eq!(
        t.work_task(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(object_id(700)), Position::from([1.0; 3])),
            None
        )
        .unwrap(),
        None
    );
}

/// A work command on the other bush retargets the forager's Gather on the same task, and on
/// the other deer the hunter's Hunt: the unit AI hears the gather or hunt order on it.
#[test]
fn a_work_command_retargets_a_gather_or_a_hunt_on_its_task() {
    let mut t = crate::world::tests::forage_world();
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let took = t
        .retarget_gather_work(
            PlayerId::new(1),
            ListIndex::new(0),
            (object_id(601), Position::new(36.5, 89.5, 0.0), true),
        )
        .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        (took, v.actions.list.gather().map(|g| g.target)),
        (true, Some(Some(object_id(601))))
    );
    let ai = v.combat.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Gather),
            Some(ActionKind::Gather),
            Some(object_id(601))
        )
    );
    let mut t = crate::world::tests::hunt_world();
    t.hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    t.retarget_hunt_work(
        PlayerId::new(1),
        ListIndex::new(0),
        (object_id(501), Position::new(36.5, 89.5, 0.0), true),
    )
    .unwrap();
    let v = &t.players[1].objects[ListIndex::new(0)];
    assert_eq!(
        v.actions.list.hunt().map(|h| h.target),
        Some(Some(object_id(501)))
    );
    let ai = v.combat.ai.as_ref().unwrap();
    assert_eq!(
        (ai.current_order, ai.current_action, ai.current_target),
        (
            Some(OrderKind::Hunt),
            Some(ActionKind::Hunt),
            Some(object_id(501))
        )
    );
}

/// A Gather or a Hunt sent on to an object of its own player's master 84 is not modelled.
#[test]
fn a_work_command_s_retarget_to_its_player_s_master_84_fails() {
    let with_84 = |mut t: World| {
        for masters in &mut t.masters {
            let m84 = Master {
                id: ids::MARKET,
                kind: master_type::COMBAT,
                class: class::PREY,
                ..Master::default()
            };
            *masters = crate::objects::masters::Masters::new(masters.iter().cloned().chain([m84]));
        }
        t.players[1].objects.push(Unit {
            id: object_id(800),
            body: crate::objects::layers::Body::at(Position::new(33.5, 89.5, 0.0)),
            life: crate::objects::lifecycle::Lifecycle {
                master_id: ids::MARKET,
                state: ObjectState::Alive,
                hp: HitPoints::new(5.0),
                ..crate::objects::lifecycle::Lifecycle::default()
            },
            ..Unit::default()
        });
        t
    };
    let mut t = with_84(crate::world::tests::forage_world());
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    let r = t.retarget_gather_work(
        PlayerId::new(1),
        ListIndex::new(0),
        (object_id(800), Position::new(33.5, 89.5, 0.0), true),
    );
    assert_eq!(
        r.err().map(|e| e.reason()),
        Some("a gather sent to its player's master 84")
    );
    let mut t = with_84(crate::world::tests::hunt_world());
    t.hunt_command(PlayerId::new(1), ListIndex::new(0), object_id(500))
        .unwrap();
    let r = t.retarget_hunt_work(
        PlayerId::new(1),
        ListIndex::new(0),
        (object_id(800), Position::new(33.5, 89.5, 0.0), true),
    );
    assert_eq!(
        r.err().map(|e| e.reason()),
        Some("a hunt sent to its player's master 84")
    );
}

/// The retake of the work's task by the unit's top action: a Gather on the same task keeps
/// the work (retargeted); a base action type on top of the task's type answers 0 (the factory
/// follows); another current task, or a Build of the unit's former master, is not modelled;
/// no action on top answers 0.
#[test]
fn a_work_command_s_task_is_retaken_by_the_action_on_top() {
    use crate::objects::masters::Task;
    let mut t = crate::world::tests::forage_world();
    let task = *t.masters[1]
        .get(master_id(120))
        .unwrap()
        .tasks
        .first()
        .unwrap();
    let at = (Some(object_id(601)), Position::new(36.5, 89.5, 0.0), true);
    assert_eq!(
        t.retake_task(PlayerId::new(1), ListIndex::new(0), at, &task, true),
        Ok(false)
    );
    t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
        .unwrap();
    assert_eq!(
        t.retake_task(PlayerId::new(1), ListIndex::new(0), at, &task, true),
        Ok(true)
    );
    let other = Task {
        id: TaskId::test(9),
        ..task
    };
    assert_eq!(
        t.retake_task(PlayerId::new(1), ListIndex::new(0), at, &other, true)
            .err()
            .map(|e| e.reason()),
        Some("a work command on the unit's current task")
    );
    let mut t = crate::world::tests::forage_world();
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .set_move_to(Some(crate::movement::move_to::MoveToAction {
            sprite: None,
            state: crate::movement::move_to::MoveTo::Moving,
            point: Position::new(40.0, 89.5, 0.0),
            target: None,
            range: Distance::ZERO,
            quiet: false,
            on_target_moved: crate::movement::move_to::TargetMoved::Follows,
            wait: crate::movement::move_to::MoveWait::default(),
        }));
    let moving = Task {
        action_type: crate::movement::move_to::MOVE_TO,
        ..task
    };
    assert_eq!(
        t.retake_task(PlayerId::new(1), ListIndex::new(0), at, &moving, true),
        Ok(false)
    );
    let mut t = crate::world::tests::forage_world();
    let build = crate::construction::build::BuildTask {
        range: Distance::new(0.1),
        work_value: crate::objects::masters::WorkValue::new(1.0),
        work_rate: WorkRate::new(1.0),
        resource: None,
        id: TaskId::test(3),
        moving: None,
    };
    t.players[1].objects[ListIndex::new(0)]
        .actions
        .list
        .set_build(Some(crate::construction::build::tests::started(
            object_id(10028),
            Position::new(30.0, 88.0, 0.0),
            Distance::new(1.0),
            build,
        )));
    let building = Task {
        action_type: action_type::BUILD,
        ..task
    };
    assert_eq!(
        t.retake_task(PlayerId::new(1), ListIndex::new(0), at, &building, false)
            .err()
            .map(|e| e.reason()),
        Some("a work command on the Build task of the unit's former master")
    );
}

/// The action list's factory for the work's task: the Gather task starts a Gather on the
/// bush; a task of an action type the slice does not make is not modelled.
#[test]
fn a_work_command_s_factory_starts_the_task_s_action() {
    use crate::objects::masters::Task;
    let mut t = crate::world::tests::forage_world();
    let task = *t.masters[1]
        .get(master_id(120))
        .unwrap()
        .tasks
        .first()
        .unwrap();
    t.start_task(
        PlayerId::new(1),
        ListIndex::new(0),
        (Some(object_id(600)), Position::new(32.5, 89.5, 0.0)),
        &task,
        (true, true),
    )
    .unwrap();
    assert_eq!(
        t.players[1].objects[ListIndex::new(0)]
            .actions
            .list
            .gather()
            .map(|g| (g.target, g.state)),
        Some((Some(object_id(600)), GatherState::Walk))
    );
    let unknown = Task {
        action_type: ActionType::named(0x55),
        ..task
    };
    assert_eq!(
        t.start_task(
            PlayerId::new(1),
            ListIndex::new(0),
            (Some(object_id(600)), Position::new(32.5, 89.5, 0.0)),
            &unknown,
            (true, true)
        )
        .err()
        .map(|e| e.reason()),
        Some("the action factory for a task the slice does not model")
    );
}

/// INTERACT (`FUN_140e68b90`) of three soldiers on an enemy building: the splitter groups them,
/// a new group of their owner takes them, its statistics and its place, the command count as its
/// differentiator and the order: the soldiers' attack task (type 7) on the building is the
/// group's command 9 on it, and the command count goes on. The soldiers are in the group, none
/// is loose.
#[test]
fn an_interact_of_soldiers_on_an_enemy_building_forms_a_group_that_attacks_it() {
    let units = [
        (11, 4, Point::new(20.0, 20.0)),
        (12, 4, Point::new(21.0, 20.0)),
        (13, 4, Point::new(22.0, 20.0)),
    ];
    let mut t = searchable(patrollers(&units, &[]));
    t.players.push(crate::world::Player::default());
    t.players[2].objects = vec![Unit {
        id: object_id(60),
        body: crate::objects::layers::Body::at(Position::new(40.0, 40.0, 0.0)),
        attackable: true,
        life: crate::objects::lifecycle::Lifecycle {
            master_id: ids::HOUSE,
            state: ObjectState::Alive,
            hp: HitPoints::new(550.0),
            ..crate::objects::lifecycle::Lifecycle::default()
        },
        ..Unit::default()
    }]
    .into();
    let building = Master {
        id: ids::HOUSE,
        kind: BUILDING,
        class: class::BUILDING,
        radius: Footprint::new(2.0, 2.0),
        hit_points: MasterHitPoints::of_data(550),
        ..Master::default()
    };
    let soldier = Master {
        tasks: vec![crate::objects::masters::Task {
            action_type: action_type::COMBAT,
            owner_type: TargetOwner::ENEMY,
            object_group: Some(class::BUILDING),
            ..crate::objects::masters::Task::default()
        }],
        ..t.masters[1].get(master_id(4)).unwrap().clone()
    };
    t.masters[1] = crate::objects::masters::Masters::new([soldier, building.clone()]);
    t.masters
        .push(crate::objects::masters::Masters::new([building]));
    for u in &mut t.players[1].objects {
        u.life.hp = HitPoints::new(25.0);
    }
    let order = Interact {
        target: Some(object_id(60)),
        point: Point::new(40.0, 40.0),
        carry_test: false,
        queued: false,
        direct: true,
        human: true,
        filter: None,
    };
    t.interact(PlayerId::new(1), &object_ids([11, 12, 13]), &order)
        .unwrap();
    let g = &t.groups.groups;
    assert_eq!(g.len(), 1);
    let ids: Vec<ObjectId> = g[0].members.iter().map(|m| m.id).collect();
    assert_eq!(ids, [object_id(11), object_id(12), object_id(13)]);
    assert_eq!(
        (g[0].current_command, g[0].current_target.target),
        (GroupCommandKind::ATTACK, GroupTarget::Object(object_id(60)))
    );
    assert_eq!(t.groups.commands, Differentiator::of_data(1));
}
