//! A unit's strikes back, its volleys, its fire and its missile launch, on the archer's world
//! of [`archer_world`].

use crate::GenieResult;
use crate::combat::damage::{Armored, Attacker, Kill, KillCount, Victim};
use crate::combat::missile::Aimer;
use crate::combat::stingers::{STINGER, StingerTask, stinger_tasks};
use crate::combat::victims::KillLeft;
use crate::combat::volley::{Aimed, Shot, armed, same_members};
use crate::combat::{AttackCount, Charge, Firepower, KillState, MissileCount};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::Radii;
use crate::map::{GroundOffset, Offset};
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::layers::Combat;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ActionPart;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::CombatPart;
use crate::objects::masters::Master;
use crate::objects::masters::MasterField;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::Task;
use crate::objects::masters::WorkValue;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::attribute::AttributeId;
use crate::test_ids::{master_id, object_id};
use crate::testing::class;
use crate::testing::{roles, tuning};
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitAi;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::parts::Part;
use crate::world::tests::{archer_world, in_update, missiles};
use std::sync::Arc;

/// The archer of [`archer_world`] (123, player 1's first object).
const ARCHER: ObjectId = object_id(123);
/// The deer it aims at (50, Gaia's).
const DEER: ObjectId = object_id(50);

/// The fire's aim at the deer: at (13, 10), its radii 0.2 and 1.5 high, a predator of Gaia's.
fn at_deer() -> Aimed {
    Aimed {
        target: Some(DEER),
        position: Position::new(13.0, 10.0, 0.0),
        radius: Radii::new(0.2, 0.2, 1.5),
        class: class::PREDATOR,
        player: PlayerId::new(0),
    }
}

/// Master `id` changed by `change` in every player's table.
fn change_master(world: &mut World, id: i16, change: impl Fn(&mut Master)) {
    for masters in &mut world.model.masters {
        if let Some(master) = Arc::make_mut(masters).get_mut(master_id(id)) {
            change(master);
        }
    }
}

/// `master` joins every player's table.
fn add_master(world: &mut World, master: &Master) {
    for masters in &mut world.model.masters {
        *masters = Arc::new(Masters::new(
            masters.iter().cloned().chain([master.clone()]),
        ));
    }
}

/// The archer's combat part, changed by `change`.
fn change_combat(world: &mut World, change: impl Fn(&mut CombatPart)) {
    change_master(world, 4, |master| {
        change(master.combat.get_or_insert_with(Default::default));
    });
}

/// The archer's volley at `aimed` in its update ([`Unit::volley`]).
fn volley_at(world: &mut World, aimed: Aimed) -> GenieResult<()> {
    in_update(
        world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, _, step| unit.volley(step, aimed, &roles()),
    )
}

/// The archer's fire at `aimed` in its update ([`Unit::fire`]).
fn fire_at(world: &mut World, aimed: &Aimed) -> GenieResult<()> {
    in_update(
        world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, _, step| unit.fire(step, aimed, &roles()),
    )
}

/// The archer's volley's extra missile at `aimed`, unscattered, in its update
/// ([`Unit::volley_missile`]).
fn volley_missile_at(world: &mut World, aimed: &Aimed) -> GenieResult<()> {
    in_update(
        world,
        (PlayerId::new(1), ListIndex::new(0)),
        0,
        |unit, _, step| unit.volley_missile(step, aimed, GroundOffset::new(0.0, 0.0), &roles()),
    )
}

/// The reason of `result`'s error; none for success.
fn reason(result: GenieResult<()>) -> Option<String> {
    result.err().map(|error| error.reason().to_owned())
}

/// The kinds and parameters of the notifies queued at the archer's unit AI.
fn archer_notifies(world: &World) -> Vec<(NotifyKind, [i32; 3])> {
    world
        .parts
        .unit_ai
        .get(ARCHER)
        .map(|ai| {
            ai.notifies
                .iter()
                .map(|notify| (notify.kind, notify.params))
                .collect()
        })
        .unwrap_or_default()
}

/// A stinger task of 2 that lasts 5 seconds.
fn stinger() -> StingerTask {
    stinger_tasks(&Master {
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: STINGER,
                work_val1: WorkValue::new(2.0),
                work_val2: WorkValue::new(5.0),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..Master::default()
    })[0]
}

/// A live unit 7 of 30 hit points.
fn unit_7() -> Unit {
    Unit {
        id: object_id(7),
        life: Lifecycle {
            hp: HitPoints::new(30.0),
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        ..Unit::default()
    }
}

/// The unit's own part of its hit (a retaliation's): its hit points, its kill flag, the
/// stingers the hit handed it (at the world time: 3 s, one of 5 s ends at 8 s), its hit point
/// powerup, its charge and charge flag, and the damage notify (500) to its unit AI; without a
/// kill or counts nothing is left for the caller.
#[test]
fn own_hit_writes_back_on_the_unit_its_combat_layer_and_its_unit_ai() {
    let mut unit = unit_7();
    let (mut combat, mut ai) = (Combat::default(), UnitAi::default());
    let powerup = HpPowerup {
        taken: HitPoints::new(1.0),
        max: HitPoints::new(4.0),
    };
    let own = Victim {
        id: object_id(7),
        hp: HitPoints::new(21.5),
        kill_state: KillState::Counted,
        stung: vec![(object_id(8), stinger())],
        armored: Some(Armored {
            hp_powerup: powerup,
            ..Armored::default()
        }),
        source: Attacker {
            charge: Charge::new(0.25),
            charge_hit: true,
            ..Attacker::default()
        },
        damaged: vec![(Some(object_id(8)), [8, 21, 30])],
        ..Victim::default()
    };
    let left = unit.hit_back(
        own,
        (Some(&mut ai), &mut combat),
        WorldTime::from_millis(3000),
    );
    assert_eq!(left, Ok(None));
    assert_eq!(unit.life.hp, HitPoints::new(21.5));
    assert_eq!(
        (
            combat.kill_state,
            combat.hp_powerup,
            combat.charge,
            combat.charge_hit
        ),
        (KillState::Counted, powerup, Charge::new(0.25), true)
    );
    let handed: Vec<(WorldTime, Option<ObjectId>)> = combat
        .active_stingers
        .iter()
        .map(|active| (active.end, active.attacker))
        .collect();
    assert_eq!(handed, [(WorldTime::from_millis(8000), Some(object_id(8)))]);
    assert_eq!(
        ai.notifies,
        [Notify {
            caller: Some(object_id(8)),
            recipient: object_id(7),
            kind: NotifyKind::DAMAGED,
            params: [8, 21, 30],
        }]
    );
}

/// A unit without a unit AI posts no notify; a kill and its counts go to the caller.
#[test]
fn own_hit_without_unit_ai_leaves_its_kill_for_the_caller() {
    let mut unit = unit_7();
    let mut combat = Combat::default();
    let kill = Kill {
        attacker: object_id(8),
        attacker_player: PlayerId::new(2),
        to_gaia: false,
    };
    let own = Victim {
        id: object_id(7),
        hp: HitPoints::ZERO,
        damaged: vec![(Some(object_id(8)), [8, 0, 30])],
        killed: Some(kill),
        kill_counts: vec![KillCount::Killer(PlayerId::new(2))],
        ..Victim::default()
    };
    assert_eq!(
        unit.hit_back(own, (None, &mut combat), WorldTime::ZERO),
        Ok(Some(KillLeft {
            id: object_id(7),
            kill: Some(kill),
            counts: vec![KillCount::Killer(PlayerId::new(2))],
            razed: Vec::new(),
        }))
    );
    assert_eq!(unit.life.hp, HitPoints::ZERO);
}

/// A killed attacker that goes to Gaia (a livestock's own hit) is not modelled.
#[test]
fn own_hit_whose_kill_gives_the_attacker_to_gaia_is_unmodelled() {
    let own = Victim {
        id: object_id(7),
        killed: Some(Kill {
            attacker: object_id(8),
            attacker_player: PlayerId::new(2),
            to_gaia: true,
        }),
        ..Victim::default()
    };
    let left = unit_7().hit_back(own, (None, &mut Combat::default()), WorldTime::ZERO);
    assert_eq!(
        left.map_err(|error| error.reason().to_owned()),
        Err("a killed attacker that goes to Gaia".to_owned())
    );
}

/// The fire's aim at a strike's target: its place, its master's radii with its height, its
/// class and its player.
#[test]
fn aim_at_victim_takes_its_place_radii_height_class_and_player() {
    let victim = Victim {
        position: Position::new(4.5, 6.5, 0.25),
        radius: Footprint::new(0.3, 0.4),
        height: Distance::new(1.2),
        class: class::PREDATOR,
        player: PlayerId::new(2),
        ..Victim::default()
    };
    let aimed = Aimed::at(Some(DEER), &victim);
    assert_eq!(
        (
            aimed.target,
            aimed.position,
            aimed.radius,
            aimed.class,
            aimed.player
        ),
        (
            Some(DEER),
            Position::new(4.5, 6.5, 0.25),
            Radii::new(0.3, 0.4, 1.2),
            class::PREDATOR,
            PlayerId::new(2)
        )
    );
}

/// `FUN_14120ea30`: a building attacks as a packed or unpacked trebuchet (classes 51, 54), with
/// a missile, or with a firepower above 1 (`COMISS`/`SETA`: not at 1, not a NaN); any other
/// attacker always.
#[test]
fn building_is_armed_as_trebuchet_with_missile_or_above_firepower_1() {
    let building = |class, missile| Master {
        kind: MasterKind::BUILDING,
        class,
        combat: Some(CombatPart {
            missile: MasterField::of(missile),
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    let armed_with = |master: &Master, firepower: f32| {
        armed(master, Firepower::new(firepower), &roles(), &tuning().turn)
    };
    let tower = building(class::TOWER, None);
    assert_eq!(
        [1.0, 1.5, f32::NAN].map(|firepower| armed_with(&tower, firepower)),
        [false, true, false]
    );
    assert!(armed_with(&building(class::CLASS_51, None), 0.0));
    assert!(armed_with(&building(class::CLASS_54, None), 0.0));
    assert!(armed_with(
        &building(class::TOWER, Some(master_id(363))),
        0.0
    ));
    let soldier = Master {
        kind: MasterKind::COMBAT,
        ..tower
    };
    assert!(armed_with(&soldier, 0.0));
}

/// Two lists hold the same objects in any order, each as often.
#[test]
fn same_members_compares_lists_in_any_order_as_often() {
    let ids = |values: &[i32]| {
        values
            .iter()
            .map(|&value| object_id(value))
            .collect::<Vec<_>>()
    };
    assert!(same_members(&ids(&[1, 2, 3]), &ids(&[3, 1, 2])));
    assert!(same_members(&ids(&[1, 2, 2]), &ids(&[2, 1, 2])));
    assert!(!same_members(&ids(&[1, 2]), &ids(&[1, 2, 2])));
    assert!(!same_members(&ids(&[1, 1, 2]), &ids(&[1, 2, 2])));
    assert!(same_members(&[], &[]));
}

/// A self-destructing attacker (a petard, class 35) alive: its hit (its object 112 and its
/// death) is not modelled; dying, it fires as any other.
#[test]
fn self_destructing_attacker_hit_is_unmodelled_only_while_alive() {
    let mut world = archer_world(100, 0, 0.0);
    change_master(&mut world, 4, |master| master.class = class::CLASS_35);
    assert!(
        reason(volley_at(&mut world, at_deer()))
            .is_some_and(|text| text.starts_with("a self-destructing attacker's hit"))
    );
    world.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    assert_eq!(volley_at(&mut world, at_deer()), Ok(()));
    assert_eq!(missiles(&world).len(), 1);
}

/// A target of Gaia's makes no flare (master 112 of the target's player's slot 0x1c0); the
/// missile flies all the same.
#[test]
fn hit_on_gaia_target_makes_no_flare() {
    let mut world = archer_world(100, 0, 0.0);
    let flare = Master {
        id: master_id(112),
        kind: MasterKind::ANIMATED,
        ..Master::default()
    };
    add_master(&mut world, &flare);
    assert_eq!(volley_at(&mut world, at_deer()), Ok(()));
    let flares = world
        .players
        .iter()
        .flat_map(|player| player.objects.iter())
        .filter(|unit| unit.life.master_id == master_id(112))
        .count();
    assert_eq!((flares, missiles(&world).len()), (0, 1));
}

/// A master with the full volley ability (`combat_ability` 16) fires its whole volley in one
/// swing though it spreads: three missiles of a volley of 3, `next_volley` back at 0.
#[test]
fn full_volley_fires_all_its_missiles_in_one_swing() {
    let mut world = archer_world(100, 3, 0.5);
    change_combat(&mut world, |combat| {
        combat.combat_ability = CombatAbility::FULL_VOLLEY;
    });
    assert_eq!(volley_at(&mut world, at_deer()), Ok(()));
    assert_eq!(missiles(&world).len(), 3);
    assert_eq!(
        world.parts.combat_of(ARCHER).next_volley,
        MissileCount::ZERO
    );
}

/// Each missile of a volley tells its shooter's unit AI the action executed (0x200 with 600):
/// the extra missiles with no target, then the volley's own missile with its target. A
/// Chu Ko Nu (master 73) tells it once per whole volley instead, with no target.
#[test]
fn volley_notifies_its_unit_ai_per_missile_but_chu_ko_nu_once_per_volley() {
    let full = |world: &mut World| {
        change_combat(world, |combat| {
            combat.combat_ability = CombatAbility::FULL_VOLLEY;
        });
    };
    let mut world = archer_world(100, 3, 0.0);
    full(&mut world);
    assert_eq!(volley_at(&mut world, at_deer()), Ok(()));
    let executed = NotifyKind::ACTION_EXECUTED;
    assert_eq!(
        archer_notifies(&world),
        [
            (executed, [600, -1, 0]),
            (executed, [600, -1, 0]),
            (executed, [600, 50, 0])
        ]
    );
    let mut chu_ko_nu = archer_world(100, 3, 0.0);
    full(&mut chu_ko_nu);
    for masters in &mut chu_ko_nu.model.masters {
        *masters = Arc::new(Masters::new(masters.iter().cloned().map(|master| {
            if master.id == master_id(4) {
                Master {
                    id: master_id(73),
                    ..master
                }
            } else {
                master
            }
        })));
    }
    chu_ko_nu.players[1].objects[ListIndex::new(0)]
        .life
        .master_id = master_id(73);
    assert_eq!(volley_at(&mut chu_ko_nu, at_deer()), Ok(()));
    assert_eq!(missiles(&chu_ko_nu).len(), 3);
    assert_eq!(archer_notifies(&chu_ko_nu), [(executed, [600, -1, 0])]);
}

/// `FUN_141081020` turns the weapon offset by the shooter's facing (`+0x7e8`), which the slice
/// holds in its movement state: a shooter without one is not modelled.
#[test]
fn fire_of_shooter_without_movement_state_is_unmodelled() {
    let mut still = archer_world(100, 0, 0.0);
    still.set_part(ARCHER, Part::Motion(None)).unwrap();
    assert_eq!(
        reason(fire_at(&mut still, &at_deer())).as_deref(),
        Some("a ranged unit that does not move")
    );
}

/// A missile fired at a computer player's object whose shooter's blast does not spare friends
/// (a ranged one without `area_effect_level` 4) notifies the target (0x20f): not modelled; one
/// that spares them fires as at any other.
#[test]
fn fire_at_computer_target_is_unmodelled_unless_its_blast_spares_friends() {
    let mut world = archer_world(100, 0, 0.0);
    world.players[0].kind = PlayerKind::Computer;
    assert_eq!(
        reason(fire_at(&mut world, &at_deer())).as_deref(),
        Some("a missile fired at a computer player's object (its notify 0x20f)")
    );
    let mut sparing = archer_world(100, 0, 0.0);
    sparing.players[0].kind = PlayerKind::Computer;
    change_combat(&mut sparing, |combat| {
        combat.blast_level = BlastLevel::SPARES_FRIENDS;
    });
    assert_eq!(fire_at(&mut sparing, &at_deer()), Ok(()));
    assert_eq!(missiles(&sparing).len(), 1);
}

/// A building's (class 3) extra missile starts at its target's x and y when the target lies
/// nearer than its weapon offset's length (2 tiles; here 1); another attacker's starts at its
/// weapon offset.
#[test]
fn volley_missile_of_building_near_its_target_starts_at_the_target() {
    let near = Aimed {
        position: Position::new(11.0, 10.0, 0.0),
        ..at_deer()
    };
    let started = |class| {
        let mut world = archer_world(100, 0, 0.0);
        change_master(&mut world, 4, |master| master.class = class);
        change_combat(&mut world, |combat| {
            combat.weapon_offset = Offset::new(2.0, 0.0, 1.0);
        });
        assert_eq!(volley_missile_at(&mut world, &near), Ok(()));
        let at = missiles(&world)[0].2;
        (at.x(), at.y())
    };
    assert_eq!(started(class::BUILDING), (11.0, 10.0));
    assert_ne!(started(class::CLASS_0), (11.0, 10.0));
}

/// A volley's extra missile needs a missile master its player has: a missile the table lacks
/// fails, a master without a missile of either kind and a volley missile of master 527 (whose
/// aim reads its attack count) are not modelled.
#[test]
fn volley_missile_without_missile_its_player_has_fails() {
    let mut lacking = archer_world(100, 0, 0.0);
    change_combat(&mut lacking, |combat| {
        combat.missile = MasterField::of(Some(master_id(999)));
    });
    assert_eq!(
        reason(volley_missile_at(&mut lacking, &at_deer())).as_deref(),
        Some("a missile its player's masters do not have")
    );
    let mut unarmed = archer_world(100, 0, 0.0);
    change_combat(&mut unarmed, |combat| {
        combat.missile = MasterField::of(None);
    });
    assert_eq!(
        reason(volley_missile_at(&mut unarmed, &at_deer())).as_deref(),
        Some("a volley's extra missile of a master without a missile")
    );
    let mut demolition = archer_world(100, 0, 0.0);
    let ship = Master {
        id: master_id(527),
        ..demolition.model.masters[1]
            .get(master_id(363))
            .unwrap()
            .clone()
    };
    add_master(&mut demolition, &ship);
    change_master(&mut demolition, 4, |master| {
        master
            .creatable
            .get_or_insert_with(Default::default)
            .volley_missile = Some(master_id(527));
    });
    assert_eq!(
        reason(volley_missile_at(&mut demolition, &at_deer())).as_deref(),
        Some("a volley's extra missile of master 527 or 528 (its attack count)")
    );
}

/// `FUN_1410d5920` makes the missile of its player's master: one the table lacks fails; a
/// pooled missile that landed takes the ground by its fly mode, which a map without terrain
/// cannot give.
#[test]
fn launch_fails_without_its_missile_master_or_the_ground_it_takes() {
    let mut world = archer_world(100, 0, 0.0);
    let aimed = Aimed {
        target: None,
        ..at_deer()
    };
    let made = |world: &mut World, shot_master: i16| {
        in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            0,
            |unit, _, step| {
                let master = unit
                    .master(step.world.master_tables(), step.player)?
                    .clone();
                let shot = Shot {
                    made: master_id(shot_master),
                    at: Position::new(10.0, 10.0, 1.0),
                    point: aimed.position,
                    reach: Distance::new(10.0),
                    shooter: true,
                    aimer: Aimer {
                        hit_chance: master.combat_part().hit_chance,
                        spread: master.combat_part().missed_spread,
                        class: master.class,
                    },
                    attack_count: AttackCount::ZERO,
                };
                unit.launch(step, &aimed, &shot, &roles())
            },
        )
    };
    assert_eq!(
        reason(made(&mut world, 999)).as_deref(),
        Some("a missile its player's masters do not have")
    );
    world.recycling.missiles = vec![(object_id(-5), false)];
    world.spatial.terrain = None;
    assert_eq!(
        reason(made(&mut world, 363)).as_deref(),
        Some("a missile without the map's terrain")
    );
}

/// The launched missile holds its master's storage (slot 0x40's static storage): an entry of
/// flag 1 adds to its player's attribute, the last of flag 0 is what it holds.
#[test]
fn launched_missile_holds_its_master_storage() {
    let mut world = archer_world(100, 0, 0.0);
    world.players[1].attributes = vec![Amount::ZERO; 300];
    change_master(&mut world, 363, |master| {
        master.storage = vec![
            (Some(AttributeId::named(12)), Amount::new(1.0), 1),
            (Some(AttributeId::named(13)), Amount::new(3.0), 0),
        ];
    });
    assert_eq!(fire_at(&mut world, &at_deer()), Ok(()));
    let missile = world.players[1].objects.by_id(object_id(-2)).unwrap();
    assert_eq!(
        (missile.life.attribute_kind, missile.life.amount),
        (Some(AttributeId::named(13)), Amount::new(3.0))
    );
    assert_eq!(world.players[1].attributes[12], Amount::new(1.0));
}

/// The missile's action takes its target and, with the shooter, the shooter too (slot 0x178):
/// each one's worker count rises; a shooterless volley missile takes only the target.
#[test]
fn launch_takes_the_target_and_the_shooter() {
    let workers =
        |world: &World| [ARCHER, DEER].map(|id| world.object(id).map(|(_, unit)| unit.workers));
    let mut world = archer_world(100, 0, 0.0);
    assert_eq!(fire_at(&mut world, &at_deer()), Ok(()));
    assert_eq!(
        workers(&world),
        [Some(WorkerCount::ONE), Some(WorkerCount::ONE)]
    );
    let mut shooterless = archer_world(100, 0, 0.0);
    let own = Master {
        id: master_id(364),
        ..shooterless.model.masters[1]
            .get(master_id(363))
            .unwrap()
            .clone()
    };
    add_master(&mut shooterless, &own);
    change_master(&mut shooterless, 4, |master| {
        master
            .creatable
            .get_or_insert_with(Default::default)
            .volley_missile = Some(master_id(364));
    });
    assert_eq!(volley_missile_at(&mut shooterless, &at_deer()), Ok(()));
    assert_eq!(
        workers(&shooterless),
        [Some(WorkerCount::ZERO), Some(WorkerCount::ONE)]
    );
}
