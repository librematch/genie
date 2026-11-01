use crate::task_finder::{FinderObject, FinderWorld, finder_objects};
use crate::test_ids::object_id;
use genie_core::map::Position;
use genie_core::objects::HitPoints;
use genie_core::objects::ObjectId;
use genie_core::objects::ObjectState;
use genie_core::objects::lifecycle::Held;
use genie_core::objects::masters::ids;
use genie_core::objects::masters::{Master, Masters};
use genie_core::players::Amount;
use genie_core::players::CivId;
use genie_core::players::PlayerId;
use genie_core::players::attribute;
use genie_core::players::{ByPlayer, Relations};
use genie_core::unit_ai::task_finder::Diplomacy;

/// An object of player 1, master 83, at the main building `annex_of` if it is an annex.
fn object(annex_of: Option<ObjectId>) -> FinderObject {
    FinderObject {
        player: PlayerId::new(1),
        master: ids::VILLAGER_MALE,
        state: ObjectState::Alive,
        held: Held {
            kind: Some(attribute::FOOD),
            amount: Amount::new(5.0),
        },
        hp: HitPoints::new(25.0),
        max_hp: HitPoints::new(30.0),
        attackable: true,
        pasture: None,
        annex_of,
        top_action: Some(genie_core::gathering::gather::GATHER),
        position: Position::new(1.0, 2.0, 0.0),
    }
}

/// Gaia and two players: player 1's relation to player 2 is `relation`.
fn world(relation: u8) -> FinderWorld {
    let master = Master {
        id: ids::VILLAGER_MALE,
        ..Master::default()
    };
    FinderWorld {
        masters: vec![Masters::default(), Masters::new([master])],
        objects: [
            (object_id(10), object(None)),
            (object_id(11), object(Some(object_id(10)))),
            (object_id(12), object(Some(object_id(11)))),
        ]
        .into_iter()
        .collect(),
        relations: ByPlayer::new(vec![
            Relations::default(),
            Relations::of_bytes([0, 0, relation]),
            Relations::default(),
        ]),
        attribute_95: vec![Amount::ZERO, Amount::new(1.5)],
        civs: vec![CivId::GAIA, CivId::new(4)],
        drop_sites: vec![],
    }
}

/// One's own player counts; Gaia never; relation 0 counts, 4 only with the ally flag, any
/// other or an unknown player not.
#[test]
fn allied_follows_the_relation() {
    let w = world(0);
    assert!(w.allied(PlayerId::new(1), PlayerId::new(1), false));
    assert!(!w.allied(PlayerId::new(1), PlayerId::new(0), true));
    assert!(w.allied(PlayerId::new(1), PlayerId::new(2), false));
    assert!(world(4).allied(PlayerId::new(1), PlayerId::new(2), true));
    assert!(!world(4).allied(PlayerId::new(1), PlayerId::new(2), false));
    assert!(!world(3).allied(PlayerId::new(1), PlayerId::new(2), true));
    assert!(!w.allied(PlayerId::new(5), PlayerId::new(2), true));
}

/// An annex stands for its main building, one step for each of `steps`; an object or a
/// master the world does not hold gives none.
#[test]
fn an_annex_stands_for_its_main_building() {
    let w = world(0);
    assert_eq!(
        w.target_up(object_id(12), 0).map(|t| t.id),
        Some(object_id(12))
    );
    assert_eq!(w.target(object_id(12)).map(|t| t.id), Some(object_id(11)));
    assert_eq!(
        w.target_up(object_id(12), 5).map(|t| t.id),
        Some(object_id(10))
    );
    let target = w.target(object_id(10)).unwrap();
    assert_eq!(
        (target.player, target.state, target.hp),
        (PlayerId::new(1), ObjectState::Alive, HitPoints::new(25.0))
    );
    assert_eq!(
        (target.pasture, target.top_action),
        (None, Some(genie_core::gathering::gather::GATHER))
    );
    assert!(w.target(object_id(99)).is_none());
    let mut lost = world(0);
    lost.masters.truncate(1);
    assert!(lost.target(object_id(10)).is_none());
}

#[test]
fn the_actor_reads_its_player_s_attribute_95_and_civ() {
    let w = world(0);
    let actor = w.actor(object_id(10)).unwrap();
    assert_eq!(
        (actor.id, actor.player, actor.civ),
        (object_id(10), PlayerId::new(1), CivId::new(4))
    );
    assert_eq!(actor.attribute_95.get().to_bits(), 1.5_f32.to_bits());
    assert!(w.actor(object_id(99)).is_none());
}

/// Every live object of each player's section is in the finder's view by its id, with its
/// player and its static part's values; a saved id of −1 names no object and fails.
#[test]
fn the_finder_s_objects_are_every_player_s_by_id() {
    use crate::fixtures::{combat_object, gaia, header, human, static_mut};
    let mut deer = combat_object(3, 65);
    static_mut(&mut deer).hp = 7.0;
    let save = header(vec![gaia(vec![deer]), human(vec![combat_object(10, 83)])]);
    let objects = finder_objects(&save.players()).unwrap();
    let mut ids: Vec<_> = objects.keys().copied().collect();
    ids.sort_unstable();
    assert_eq!(ids, [object_id(3), object_id(10)]);
    let (gaia_deer, villager) = (&objects[&object_id(3)], &objects[&object_id(10)]);
    assert_eq!(
        (gaia_deer.player, gaia_deer.hp),
        (PlayerId::GAIA, HitPoints::new(7.0))
    );
    assert_eq!(villager.player, PlayerId::new(1));
    let none = header(vec![gaia(Vec::new()), human(vec![combat_object(-1, 83)])]);
    assert!(matches!(
        finder_objects(&none.players()),
        Err(crate::Error::Unmodelled(_))
    ));
}
