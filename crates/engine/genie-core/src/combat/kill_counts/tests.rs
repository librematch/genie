//! A kill's counts: `FUN_1412005b0`'s for a killer, the damage's for its owner and their order.

use crate::combat::damage::KillCount;
use crate::combat::kill_counts::KilledMaster;
use crate::map::GridSize;
use crate::map::Position;
use crate::objects::masters::ClassId;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::test_ids::{costs, master_id, object_id};
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::scene::{Scene, SceneObject};
use crate::world::World;
use crate::world::parts::Part;

/// An archer's master (4, class 0) of a paid cost of 25 wood and 45 gold and an unpaid
/// headroom of 1.
fn archer() -> Master {
    Master {
        id: master_id(4),
        kind: MasterKind::COMBAT,
        class: class::CLASS_0,
        creatable: Some(CreatablePart {
            cost: costs(&[
                (attribute::WOOD, 25, true),
                (attribute::HEADROOM, 1, false),
                (attribute::GOLD, 45, true),
            ]),
            ..CreatablePart::default()
        }),
        ..Master::default()
    }
}

/// A world of three human players: player 1's archer 7 and player 2's archer 8, every player's
/// 500 attributes 0.
fn world() -> World {
    let mut world = Scene::open(GridSize::new(4, 4))
        .players(2)
        .kind(PlayerId::new(1), PlayerKind::Human)
        .kind(PlayerId::new(2), PlayerKind::Human)
        .object(SceneObject::new(
            7,
            PlayerId::new(1),
            archer(),
            Position::new(1.5, 1.5, 0.0),
        ))
        .object(SceneObject::new(
            8,
            PlayerId::new(2),
            archer(),
            Position::new(2.5, 1.5, 0.0),
        ))
        .build();
    for player in &mut world.players {
        player.attributes = vec![Amount::ZERO; 500];
    }
    world
}

/// Player `player`'s attributes `attributes`, as floats.
fn held<const N: usize>(world: &World, player: usize, attributes: [usize; N]) -> [f32; N] {
    attributes.map(|attribute| world.players[player].attributes[attribute].get())
}

/// What the kill counts read of the archer: its class and its paid cost (the unpaid headroom
/// left out).
#[test]
fn killed_master_reads_the_class_and_the_paid_cost() {
    assert_eq!(
        KilledMaster::of(&archer()),
        KilledMaster {
            class: class::CLASS_0,
            cost: Amount::new(70.0),
        }
    );
}

/// `FUN_1412005b0` on player 1's archer killed by player 2: player 2 counts a kill (20), one of
/// player 1's (301) and its paid cost (170, 401); player 1 counts a loss (154), one to player 2
/// (327) and its paid cost (152).
#[test]
fn military_kill_counts_the_kill_and_the_loss_for_both_players() {
    let mut world = world();
    world
        .count_kill_of(
            PlayerId::new(1),
            KilledMaster::of(&archer()),
            PlayerId::new(2),
        )
        .unwrap();
    assert_eq!(
        held(&world, 2, [20, 300, 301, 302, 170, 400, 401]),
        [1.0, 0.0, 1.0, 0.0, 70.0, 0.0, 70.0]
    );
    assert_eq!(
        held(&world, 1, [154, 325, 326, 327, 152]),
        [1.0, 0.0, 0.0, 1.0, 70.0]
    );
}

/// `FUN_1412005b0`'s classes of `FUN_141188b20` (here the prey, 9) count no kill (20) and no
/// loss (154); the counts by player and the paid cost stay.
#[test]
fn peaceful_class_kill_counts_neither_kill_nor_loss() {
    let mut world = world();
    let deer = KilledMaster {
        class: class::PREY,
        cost: Amount::new(5.0),
    };
    world
        .count_kill_of(PlayerId::new(1), deer, PlayerId::new(2))
        .unwrap();
    assert_eq!(held(&world, 2, [20, 301, 170, 401]), [0.0, 1.0, 5.0, 5.0]);
    assert_eq!(held(&world, 1, [154, 327, 152]), [0.0, 1.0, 5.0]);
}

/// A count past a player's attributes adds nothing (the base add's `+0x90` test,
/// `FUN_14165d040`): a killer with 100 attributes counts the kill (20) alone.
#[test]
fn kill_counted_for_player_without_its_attributes_adds_nothing_past_them() {
    let mut world = world();
    world.players[2].attributes = vec![Amount::ZERO; 100];
    world
        .count_kill_of(
            PlayerId::new(1),
            KilledMaster::of(&archer()),
            PlayerId::new(2),
        )
        .unwrap();
    let killer = &world.players[2].attributes;
    assert_eq!((killer.len(), killer[20]), (100, Amount::new(1.0)));
    assert_eq!(held(&world, 1, [154, 327, 152]), [1.0, 1.0, 70.0]);
}

/// `FUN_1412005b0` on an object by its master: the object's own one (here a prey's, no kill
/// counted, a paid cost of 30), else its player's; an object the world does not hold fails.
#[test]
fn kill_of_an_object_counts_by_its_own_master_first() {
    let mut world = world();
    world.count_kill(object_id(8), PlayerId::new(1)).unwrap();
    assert_eq!(held(&world, 1, [20, 170]), [1.0, 70.0]);
    let own = Master {
        class: class::PREY,
        creatable: Some(CreatablePart {
            cost: costs(&[(attribute::FOOD, 30, true)]),
            ..CreatablePart::default()
        }),
        ..archer()
    };
    world
        .set_part(object_id(8), Part::OwnMaster(Some(Box::new(own))))
        .unwrap();
    world.count_kill(object_id(8), PlayerId::new(1)).unwrap();
    assert_eq!(held(&world, 1, [20, 170, 402]), [1.0, 100.0, 100.0]);
    assert!(world.count_kill(object_id(9), PlayerId::new(1)).is_err());
}

/// The damage's counts for a non-villager's owner: none, whatever killed it.
#[test]
fn non_villager_loss_counts_nothing_for_its_owner() {
    let mut world = world();
    world
        .count_owner_loss(
            PlayerId::new(1),
            class::CLASS_0,
            class::PREDATOR,
            PlayerId::new(0),
        )
        .unwrap();
    assert_eq!(held(&world, 1, [226, 227, 228, 229]), [0.0; 4]);
}

/// A kill's counts in the game's order: the burns' credit for player 2 (its archer 8 burnt the
/// villager), the villager's loss to a human for its owner (229), then the killer's count, which
/// counts player 2's second kill.
#[test]
fn kill_counts_run_the_burns_the_owner_loss_and_the_killer_in_order() {
    let mut world = world();
    let villager = KilledMaster {
        class: class::VILLAGER,
        cost: Amount::new(50.0),
    };
    let counts = [
        KillCount::Stingers(vec![Some(object_id(8)), None, Some(object_id(8))]),
        KillCount::Owner {
            attacker_class: class::CLASS_0,
            attacker_player: PlayerId::new(2),
        },
        KillCount::Killer(PlayerId::new(2)),
    ];
    world
        .count_kills(PlayerId::new(1), villager, &counts)
        .unwrap();
    assert_eq!(held(&world, 2, [20, 301, 170]), [2.0, 2.0, 100.0]);
    assert_eq!(held(&world, 1, [154, 327, 229]), [2.0, 2.0, 1.0]);
}

/// A failing count stops the counts after it: a computer player's loss (its AI's slot 0x2f0)
/// leaves the killer's count undone.
#[test]
fn kill_counts_stop_at_the_first_failing_count() {
    let mut world = world();
    world.players[1].kind = PlayerKind::Computer;
    let counts = [
        KillCount::Owner {
            attacker_class: ClassId::named(99),
            attacker_player: PlayerId::new(2),
        },
        KillCount::Killer(PlayerId::new(2)),
    ];
    assert!(
        world
            .count_kills(PlayerId::new(1), KilledMaster::of(&archer()), &counts)
            .is_err()
    );
    assert_eq!(held(&world, 2, [20, 301]), [0.0, 0.0]);
}
