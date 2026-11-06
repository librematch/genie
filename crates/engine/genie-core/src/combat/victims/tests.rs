//! A hit's victim: what the hit reads of it from the world and where it runs, the objects its
//! blast reaches, its write-back and the kill it leaves.

use crate::combat::Charge;
use crate::combat::KillState;
use crate::combat::damage::{Kill, KillCount, Razed, Victim};
use crate::combat::stingers::{STINGER, stinger_tasks};
use crate::combat::victims::{
    KillLeft, change_masters_of_hit, readable, victim, write_back, write_strike_back, write_struck,
};
use crate::construction::MainBuildingId;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::spatial::{Placed, TileSight};
use crate::objects::DamagedTimer;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionPart;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::CombatPart;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::Task;
use crate::objects::masters::WorkValue;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::test_ids::{master_id, object_id};
use crate::testing::class;
use crate::testing::scene::{Scene, SceneObject};
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::Seen;
use crate::unit_ai::UnitAi;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::parts::Part;
use crate::world::tests::in_update;

/// The archer's master (4, class 0, type 70), its blast's reach `area` (`area_effect`).
fn archer(area: f32) -> Master {
    Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        class: class::CLASS_0,
        hit_points: MasterHitPoints::of_data(30),
        radius: Footprint::new(0.2, 0.2),
        combat: Some(CombatPart {
            area_effect: Distance::new(area),
            terrain_bonus: RestrictionField::NONE,
            ..CombatPart::default()
        }),
        ..Master::default()
    }
}

/// The guard's master (5, class 0, type 70): 40 hit points, radii 0.3.
fn guard() -> Master {
    Master {
        id: master_id(5),
        kind: MasterKind::COMBAT,
        class: class::CLASS_0,
        hit_points: MasterHitPoints::of_data(40),
        radius: Footprint::new(0.3, 0.3),
        combat: Some(CombatPart {
            terrain_bonus: RestrictionField::NONE,
            ..CombatPart::default()
        }),
        ..Master::default()
    }
}

/// A building's master (20, class 3, type 80): 100 hit points.
fn building() -> Master {
    Master {
        id: master_id(20),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        hit_points: MasterHitPoints::of_data(100),
        radius: Footprint::new(0.5, 0.5),
        ..guard()
    }
}

/// Player 2's object `id` of `master` at (`x`, `y`).
fn of_player_2(id: i32, master: Master, x: f32, y: f32) -> SceneObject {
    SceneObject::new(id, PlayerId::new(2), master, Position::new(x, y, 0.0))
}

/// A 32-tile map of two players: player 1's archer 7 at (5.5, 5.5), its blast's reach `area`;
/// player 2's guards 8 (the target, at (6.5, 5.5)), 9 at (8.5, 5.5) and 10 far off at
/// (20.5, 20.5).
fn scene(area: f32) -> Scene {
    Scene::open(GridSize::new(32, 32))
        .players(2)
        .object(SceneObject::new(
            7,
            PlayerId::new(1),
            archer(area),
            Position::new(5.5, 5.5, 0.0),
        ))
        .object(of_player_2(8, guard(), 6.5, 5.5))
        .object(of_player_2(9, guard(), 8.5, 5.5))
        .object(of_player_2(10, guard(), 20.5, 20.5))
}

/// The scene's world, every player's 500 attributes 0.
fn built(scene: Scene) -> World {
    let mut world = scene.build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 500];
    }
    world
}

/// The world of [`scene`] with player 2's building 20 at (25.5, 25.5) and its annexes 21 at
/// (25.5, 26.5) and 22 at (26.5, 25.5).
fn with_annexes(scene: Scene) -> World {
    let annex = |id, x, y| SceneObject {
        annex_of: Some(MainBuildingId::new(object_id(20))),
        ..of_player_2(id, building(), x, y)
    };
    built(
        scene
            .object(of_player_2(20, building(), 25.5, 25.5))
            .object(annex(21, 25.5, 26.5))
            .object(annex(22, 26.5, 25.5)),
    )
}

/// Player 2's object `id` put to sleep: it moves from its live list to its sleeping list.
fn put_to_sleep(world: &mut World, id: i32) {
    let slot = world.players[2]
        .objects
        .position_of(object_id(id))
        .expect("a live object");
    let unit = world.players[2].objects.swap_remove(slot).unwrap();
    world.players[2].sleeping.push(unit);
    world.reindex();
}

/// Object `id` as a hit of player 1 reads it from the world.
fn read(world: &World, id: i32) -> Victim {
    world
        .victim(object_id(id), PlayerId::new(1))
        .unwrap()
        .expect("the world holds the object")
}

/// Object `id`'s hit points.
fn hp(world: &World, id: i32) -> HitPoints {
    world
        .object(object_id(id))
        .map(|(_, unit)| unit.life.hp)
        .expect("the world holds the object")
}

/// The ids of `victims`, in order.
fn ids(victims: &[Victim]) -> Vec<ObjectId> {
    victims.iter().map(|victim| victim.id).collect()
}

/// The archer's blast reaches the objects about it.
fn blasted(world: &World, target: Option<i32>) -> Vec<Victim> {
    let archer = world.players[1].objects[ListIndex::new(0)].clone();
    world
        .blasted(&archer, PlayerId::new(1), target.map(object_id))
        .unwrap()
}

/// `FUN_1415417a0`'s read of a held object: its state, type, owner, class, place, radii and
/// flags, and whether the viewer sees its tile (here player 1 every tile, player 2 none, and
/// nobody without the players' sight). An object the world holds as a place alone cannot be
/// read; one the world does not hold is none.
#[test]
fn seen_object_reads_as_its_viewer_sees_it() {
    let mut world = built(scene(0.0));
    let seen = Seen {
        state: ObjectState::Alive,
        kind: MasterKind::COMBAT,
        owner: PlayerId::new(2),
        class: class::CLASS_0,
        position: Position::new(6.5, 5.5, 0.0),
        radius: Footprint::new(0.3, 0.3),
        visible: true,
        flags: guard().object_flags,
        sight: TileSight::Seen,
    };
    assert_eq!(world.seen(object_id(8), PlayerId::new(1)), Ok(Some(seen)));
    let unseen = Seen {
        visible: false,
        sight: TileSight::Unknown,
        ..seen
    };
    assert_eq!(world.seen(object_id(8), PlayerId::new(2)), Ok(Some(unseen)));
    assert_eq!(world.seen(object_id(50), PlayerId::new(1)), Ok(None));
    world.spatial.insert(
        object_id(60),
        Placed {
            position: Position::new(7.5, 7.5, 0.0),
            radius: Footprint::new(0.3, 0.3),
            class: class::CLASS_0,
            player: PlayerId::new(2),
            master: master_id(5),
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: true,
            unknown: None,
            obstruction: None,
        },
    );
    assert!(world.seen(object_id(60), PlayerId::new(1)).is_err());
    world.spatial.sight = None;
    assert_eq!(world.seen(object_id(8), PlayerId::new(1)), Ok(Some(unseen)));
}

/// A master without an area effect blasts nothing about it: its hit reaches its target and,
/// for an annex, the annex's main building only.
#[test]
fn hit_without_area_effect_reaches_only_the_target_main_building() {
    let world = with_annexes(scene(0.0));
    assert_eq!(ids(&blasted(&world, Some(8))), []);
    assert_eq!(ids(&blasted(&world, Some(21))), [object_id(20)]);
    assert_eq!(ids(&blasted(&world, None)), []);
}

/// A blast of reach 1 reads the objects of the tiles about the unit, 8 tiles wider each way
/// (its 6 and the 2 its step may take), by id, but the unit and its target: guard 9, not guard
/// 10 far off; an object the slice holds as a place alone stands in with its place as a guess.
#[test]
fn blast_reaches_the_objects_about_it_but_its_unit_and_target() {
    let mut world = built(scene(1.0));
    world.spatial.insert(
        object_id(60),
        Placed {
            position: Position::new(14.0, 5.5, 0.0),
            radius: Footprint::new(0.3, 0.3),
            class: class::CLASS_0,
            player: PlayerId::new(2),
            master: master_id(5),
            obstruction_kind: ObstructionKind::NONE,
            buildable_on: BuildableOn::NO,
            tall: true,
            moves: true,
            unknown: None,
            obstruction: None,
        },
    );
    let near = blasted(&world, Some(8));
    assert_eq!(ids(&near), [object_id(9), object_id(60)]);
    assert_eq!(near[0], read(&world, 9));
    assert_eq!(
        near[1],
        Victim {
            id: object_id(60),
            position: Position::new(14.0, 5.5, 0.0),
            uncertain: true,
            ..Victim::default()
        }
    );
}

/// The blast's annexes take their main building (`+0xbb8`) along after the objects it reached,
/// once for both: annexes 21 and 22 within the blast bring building 20 from out of its reach.
#[test]
fn blasted_annexes_bring_their_main_building_once() {
    let mut world = with_annexes(scene(1.0));
    for (id, x, y) in [(21, 9.5, 6.5), (22, 9.5, 4.5)] {
        if let Some(placed) = world.spatial.get(object_id(id)).copied() {
            world.spatial.remove(object_id(id));
            world.spatial.insert(
                object_id(id),
                Placed {
                    position: Position::new(x, y, 0.0),
                    ..placed
                },
            );
        }
    }
    assert_eq!(
        ids(&blasted(&world, Some(8))),
        [object_id(9), object_id(21), object_id(22), object_id(20)]
    );
}

/// The updating unit's own part of its hit leaves nothing without a kill or counts; with them
/// it takes both off the unit's read, which another write-back then repeats none of.
#[test]
fn own_part_of_hit_takes_its_kill_and_counts_off_it() {
    let mut own = Victim {
        id: object_id(7),
        ..Victim::default()
    };
    assert_eq!(KillLeft::own(&mut own), None);
    let kill = Kill {
        attacker: object_id(8),
        attacker_player: PlayerId::new(2),
        to_gaia: false,
    };
    own.killed = Some(kill);
    own.kill_counts = vec![KillCount::Killer(PlayerId::new(2))];
    assert_eq!(
        KillLeft::own(&mut own),
        Some(KillLeft {
            id: object_id(7),
            kill: Some(kill),
            counts: vec![KillCount::Killer(PlayerId::new(2))],
            razed: Vec::new(),
        })
    );
    assert_eq!((own.killed, own.kill_counts), (None, Vec::new()));
}

/// A hit where it runs reads an object as the world's read does, its owner's attributes the
/// updating player's for its own objects and the other player's for the others (attribute 272
/// here); an object the world does not hold is none.
#[test]
fn hit_where_it_runs_reads_the_victim_as_the_world_does() {
    let mut world = built(scene(0.0).object(SceneObject::new(
        11,
        PlayerId::new(1),
        guard(),
        Position::new(5.5, 6.5, 0.0),
    )));
    world.players[1].attributes[272] = Amount::new(0.25);
    world.players[2].attributes[272] = Amount::new(0.5);
    let (foreign, own) = (read(&world, 8), read(&world, 11));
    let reads = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |_, _, step| {
            let roles = step.world.model.roles.clone();
            let found = readable(step, object_id(8))?
                .map(|(player_id, unit, master)| (player_id, unit.id, master.id));
            Ok((
                victim(step, object_id(8), &roles)?,
                victim(step, object_id(11), &roles)?,
                victim(step, object_id(50), &roles)?,
                found,
            ))
        },
    )
    .unwrap();
    assert_eq!(
        reads,
        (
            Some(foreign.clone()),
            Some(own.clone()),
            None,
            Some((PlayerId::new(2), object_id(8), master_id(5)))
        )
    );
    assert_eq!(
        (foreign.elevation.higher, own.elevation.higher),
        (0.5, 0.25)
    );
}

/// The write-back of a hit on an object the world no longer holds writes nothing; the hit
/// keeps neither its kill nor its counts.
#[test]
fn write_back_on_an_object_gone_writes_nothing() {
    let mut world = built(scene(0.0));
    let mut gone = Victim {
        id: object_id(50),
        killed: Some(Kill {
            attacker: object_id(7),
            attacker_player: PlayerId::new(1),
            to_gaia: false,
        }),
        kill_counts: vec![KillCount::Killer(PlayerId::new(1))],
        ..Victim::default()
    };
    let left = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |_, _, step| write_back(step, &mut gone),
    );
    assert_eq!(left, Ok(None));
    assert_eq!((gone.killed, gone.kill_counts), (None, Vec::new()));
}

/// The write-back of a hit on a live object: its hit points and sleep flag, its kill flag
/// (`+0x954`), its hit point powerup, its charge, the stingers the hit handed it at the world
/// time (3 s: one of 5 s ends at 8 s) and the damage notify (500) to its unit AI; without a
/// kill, counts or razings it leaves nothing for the caller, and the hit keeps neither its
/// stingers nor its notifies.
#[test]
fn write_back_gives_live_object_its_hit() {
    let mut world = built(scene(0.0));
    world.world_time = WorldTime::from_millis(3000);
    world
        .set_part(
            object_id(8),
            Part::UnitAi(Some(Box::new(UnitAi::default()))),
        )
        .unwrap();
    let stinger = stinger_tasks(&Master {
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: STINGER,
                work_val1: WorkValue::new(2.0),
                work_val2: WorkValue::new(5.0),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..guard()
    })[0];
    let powerup = HpPowerup {
        taken: HitPoints::new(1.0),
        max: HitPoints::new(4.0),
    };
    let mut hit = read(&world, 8);
    hit.hp = HitPoints::new(12.0);
    hit.goto_sleep = true;
    hit.kill_state = KillState::Counted;
    if let Some(armored) = hit.armored.as_mut() {
        armored.hp_powerup = powerup;
    }
    hit.source.charge = Charge::new(0.5);
    hit.stung = vec![(object_id(7), stinger)];
    hit.damaged = vec![(Some(object_id(7)), [7, 12, 40])];
    let left = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |_, _, step| write_back(step, &mut hit),
    );
    assert_eq!(left, Ok(None));
    let (_, unit) = world.object(object_id(8)).unwrap();
    assert_eq!(
        (unit.life.hp, unit.life.goto_sleep),
        (HitPoints::new(12.0), true)
    );
    let combat = world.parts.combat_of(object_id(8));
    assert_eq!(
        (combat.kill_state, combat.hp_powerup, combat.charge),
        (KillState::Counted, powerup, Charge::new(0.5))
    );
    let handed: Vec<(WorldTime, Option<ObjectId>)> = combat
        .active_stingers
        .iter()
        .map(|active| (active.end, active.attacker))
        .collect();
    assert_eq!(handed, [(WorldTime::from_millis(8000), Some(object_id(7)))]);
    assert_eq!(
        world
            .parts
            .unit_ai
            .get(object_id(8))
            .map(|ai| ai.notifies.clone()),
        Some(vec![Notify {
            caller: Some(object_id(7)),
            recipient: object_id(8),
            kind: NotifyKind::DAMAGED,
            params: [7, 12, 40],
        }])
    );
    assert_eq!((hit.stung.len(), hit.damaged.len()), (0, 0));
}

/// A building's hit that did not get past its hit point test leaves its `damaged_lately_timer`
/// and its annexes' hit points; a struck one gives both, to its live annexes only (annex 22
/// asleep keeps its own).
#[test]
fn write_back_gives_annexes_the_hit_points_of_struck_building_only() {
    let mut world = with_annexes(scene(0.0));
    put_to_sleep(&mut world, 22);
    let mut hit = read(&world, 20);
    hit.hp = HitPoints::new(50.0);
    if let Some(part) = hit.building.as_mut() {
        part.damaged_lately = DamagedTimer::of_data(60);
    }
    let mut struck = hit.clone();
    if let Some(part) = struck.building.as_mut() {
        part.struck = true;
    }
    let damaged = |world: &World| world.object(object_id(20)).unwrap().1.damaged_lately;
    let left = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |_, _, step| write_back(step, &mut hit),
    );
    assert_eq!(left, Ok(None));
    assert_eq!(
        (damaged(&world), hp(&world, 20), hp(&world, 21)),
        (
            DamagedTimer::of_data(0),
            HitPoints::new(50.0),
            HitPoints::new(100.0)
        )
    );
    let left = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |_, _, step| write_back(step, &mut struck),
    );
    assert_eq!(left, Ok(None));
    assert_eq!(
        (damaged(&world), hp(&world, 21), hp(&world, 22)),
        (
            DamagedTimer::of_data(60),
            HitPoints::new(50.0),
            HitPoints::new(100.0)
        )
    );
}

/// A sleeping object's hit of hit points that are not a number sleeps on (COMISS/JBE): it takes
/// them and stays in its sleeping list, its kill left out.
#[test]
fn write_back_of_sleeper_hit_to_nan_hit_points_sleeps_on() {
    let mut world = built(scene(0.0));
    put_to_sleep(&mut world, 9);
    let mut hit = read(&world, 9);
    hit.hp = HitPoints::new(f32::NAN);
    hit.killed = Some(Kill {
        attacker: object_id(7),
        attacker_player: PlayerId::new(1),
        to_gaia: false,
    });
    let left = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |_, _, step| write_back(step, &mut hit),
    );
    assert_eq!(left, Ok(None));
    assert!(hp(&world, 9).get().is_nan());
    assert_eq!(
        world.players[2].sleeping.position_of(object_id(9)),
        Some(ListIndex::new(0))
    );
}

/// A hit with kill counts or razings but no kill leaves them for the caller.
#[test]
fn write_back_leaves_kill_counts_and_razings_for_the_caller() {
    let mut world = with_annexes(scene(0.0));
    let mut counted = read(&world, 8);
    counted.kill_counts = vec![KillCount::Killer(PlayerId::new(1))];
    let razing = Razed {
        attacker_player: PlayerId::new(1),
        owner: PlayerId::new(2),
        event: None,
        adds: Vec::new(),
    };
    let mut razed = read(&world, 20);
    if let Some(part) = razed.building.as_mut() {
        part.razed = vec![razing.clone()];
    }
    let left = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |_, _, step| {
            Ok((
                write_back(step, &mut counted)?,
                write_back(step, &mut razed)?,
            ))
        },
    );
    assert_eq!(
        left,
        Ok((
            Some(KillLeft {
                id: object_id(8),
                kill: None,
                counts: vec![KillCount::Killer(PlayerId::new(1))],
                razed: Vec::new(),
            }),
            Some(KillLeft {
                id: object_id(20),
                kill: None,
                counts: Vec::new(),
                razed: vec![razing],
            })
        ))
    );
}

/// The masters a hit gave its victim go to a live object only: a sleeping victim with master
/// changes fails; one without changes writes nothing, whether the world holds it or not.
#[test]
fn master_changes_on_sleeping_victim_fail_and_none_write_nothing() {
    let mut world = built(scene(0.0));
    put_to_sleep(&mut world, 9);
    let mut asleep = Victim {
        master_changes: vec![master_id(4)],
        ..read(&world, 9)
    };
    assert!(change_masters_of_hit(&mut world, &mut asleep).is_err());
    let mut gone = Victim {
        id: object_id(50),
        ..Victim::default()
    };
    assert_eq!(change_masters_of_hit(&mut world, &mut gone), Ok(()));
}

/// A strike's write-back writes the blast's objects first, then its target: a blast and a
/// target on the same object leave the target's hit points; the blast's objects go back to
/// the update. The update's blast ([`write_strike_back`]) is written the same way.
#[test]
fn strike_writes_its_blast_before_its_target() {
    let mut world = built(scene(0.0));
    let blast = Victim {
        hp: HitPoints::new(3.0),
        ..read(&world, 8)
    };
    let mut struck = Victim {
        hp: HitPoints::new(5.0),
        ..read(&world, 8)
    };
    let kept = in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, actions, step| {
            let roles = step.world.model.roles.clone();
            write_struck(step, unit, actions, vec![blast], &mut struck, &roles)?;
            Ok(ids(&step.blasted))
        },
    );
    assert_eq!(kept, Ok(vec![object_id(8)]));
    assert_eq!(hp(&world, 8), HitPoints::new(5.0));
    let mut target = Victim {
        hp: HitPoints::new(6.0),
        ..read(&world, 8)
    };
    let blasted = Victim {
        hp: HitPoints::new(2.0),
        ..read(&world, 9)
    };
    in_update(
        &mut world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, actions, step| {
            let roles = step.world.model.roles.clone();
            step.blasted = vec![blasted];
            write_strike_back(step, unit, actions, &mut target, &roles)
        },
    )
    .unwrap();
    assert_eq!(
        (hp(&world, 8), hp(&world, 9)),
        (HitPoints::new(6.0), HitPoints::new(2.0))
    );
}

/// What a write-back left on an object the world holds live: its kill counts by its master,
/// then its razings' attribute adds in their order; on an object asleep or gone it counts
/// nothing.
#[test]
fn kill_left_counts_only_on_live_object() {
    let mut world = built(scene(0.0));
    put_to_sleep(&mut world, 9);
    let counts = vec![KillCount::Killer(PlayerId::new(1))];
    for id in [9, 50] {
        world
            .kill_left(&KillLeft {
                id: object_id(id),
                kill: None,
                counts: counts.clone(),
                razed: Vec::new(),
            })
            .unwrap();
    }
    assert_eq!(world.players[1].attributes[20], Amount::ZERO);
    let adds = vec![
        (PlayerId::new(1), AttributeId::named(43), Amount::new(2.0)),
        (PlayerId::new(2), AttributeId::named(43), Amount::new(1.0)),
        (PlayerId::new(1), AttributeId::named(43), Amount::new(0.5)),
    ];
    world
        .kill_left(&KillLeft {
            id: object_id(8),
            kill: None,
            counts,
            razed: vec![Razed {
                attacker_player: PlayerId::new(1),
                owner: PlayerId::new(2),
                event: None,
                adds,
            }],
        })
        .unwrap();
    assert_eq!(
        [
            world.players[1].attributes[20],
            world.players[1].attributes[43],
            world.players[2].attributes[43],
        ],
        [Amount::new(1.0), Amount::new(2.5), Amount::new(1.0)]
    );
}
