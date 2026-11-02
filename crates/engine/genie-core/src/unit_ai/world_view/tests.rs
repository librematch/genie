use super::*;
use crate::combat::Firepower;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::layers::Static;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::{Master, MasterTables};
use crate::objects::object_list::ObjectList;
use crate::ownership::CaptureKind;
use crate::players::ByPlayer;
use crate::players::Relation;
use crate::test_ids::attack_category;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::unit_ai::Attackers;
use crate::unit_ai::NearestAttacker;
use crate::unit_ai::UnitAiType;
use crate::world::Mode680;
use crate::world::Seconds;
use crate::world::index::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::parts::{Parts, no_own_masters, no_parts};
use std::sync::Arc;

/// Slot 0x228 (`FUN_141084070`): the attacks above 0 add up in their order from +0; none
/// (or none above 0) is +0.
#[test]
fn the_attacks_sum_is_the_positive_ones() {
    let m = |weapons: Vec<ClassAmount>| Master {
        weapons,
        ..Master::default()
    };
    assert_eq!(
        attacks(&m(vec![
            ClassAmount::new(4, 6),
            ClassAmount::new(3, -2),
            ClassAmount::new(11, 1)
        ]))
        .to_bits(),
        7.0f32.to_bits()
    );
    assert_eq!(attacks(&m(vec![])).to_bits(), 0.0f32.to_bits());
    assert_eq!(
        attacks(&m(vec![ClassAmount::new(4, 0), ClassAmount::new(3, -5)])).to_bits(),
        0.0f32.to_bits()
    );
}
/// The villager 123 (player 1) tests player 2's unit 50 of `master`: harmless when its
/// master has object flag 0x10, is its own copy and shows an attack below 5 (4 only below 60
/// hit points), no melee armor and a range of at most 0.5.
fn harmless(master: Master) -> bool {
    let me = Unit {
        id: object_id(123),
        life: Lifecycle {
            master_id: ids::VILLAGER_MALE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let enemy = Player {
        objects: vec![Unit {
            id: object_id(50),
            life: Lifecycle {
                master_id: master.id,
                state: ObjectState::Alive,
                ..Lifecycle::default()
            },
            ..Unit::default()
        }]
        .into(),
        ..Player::default()
    };
    let (gaia, after) = (Player::default(), [enemy]);
    let before = [gaia];
    let every = [
        Masters::default(),
        Masters::default(),
        Masters::new([master]),
    ]
    .map(Arc::new);
    let spatial = Spatial::new(GridSize::new(120, 120));
    let attacker = Attacker {
        ai_type: UnitAiType::CIVILIAN,
        player: PlayerId::new(1),
        player_type: PlayerType::HUMAN,
        state: ObjectState::Alive,
        held: None,
        top_action: None,
        master: ids::VILLAGER_MALE,
        kind: MasterKind::COMBAT,
        class: Some(class::VILLAGER),
        attack_priority: AttackPriority::PRIORITY_0,
        missile: None,
        range: Distance::ZERO,
        min_range: Distance::ZERO,
        charge_kind: ChargeKind::NONE,
        area_effect: Distance::ZERO,
        blast_level: BlastLevel::NONE,
        los: Distance::new(4.0),
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(0.2, 0.2),
        speed: Speed::new(0.8),
        scenario: false,
        firepower: Firepower::new(0.0),
        capture: CaptureKind::NONE,
    };
    let none = ObjectList::default();
    let foreign = Foreign {
        before: &before,
        after: &after,
        left: None,
    };
    let picker = WorldPicker {
        by_id: ObjectsRef::empty(),
        attacker: &attacker,
        seeker: None,
        me: object_id(123),
        player: PlayerId::new(1),
        others: OthersRef::new(PlayerId::new(1), &none, foreign, ObjectIndex::scanning()),
        foreign,
        every: MasterTables::new(&every, no_own_masters()),
        parts: no_parts(),
        restriction: None,
        unit: &me,
        actions: &Action::default(),
        attributes: &[],
        relations: &ByPlayer::new(vec![Relation::ALLY, Relation::ALLY, Relation::ALLY]),
        ai_settings: AiSettings::default(),
        now: WorldTime::ZERO,
        mode_680: Mode680::ZERO,
    };
    picker.harmless(&spatial, object_id(50)).unwrap()
}

/// Slot 0x78 of the ward's unit AI: of its attackers the world holds, the nearest by the
/// footprint gap, the first of a tie; −1 for none, None for a ward without a unit AI.
#[test]
fn ward_nearest_attacker_is_its_closest_held_one() {
    let master = |id, radius| Master {
        id,
        kind: MasterKind::COMBAT,
        radius,
        ..Master::default()
    };
    let unit = |id, x, master_id| Unit {
        id,
        place: Static::at(Position::new(x, 10.0, 0.0)),
        life: Lifecycle {
            master_id,
            state: ObjectState::Alive,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let attacked = |attackers| UnitAi {
        attackers,
        ..UnitAi::default()
    };
    let me = unit(object_id(123), 10.0, master_id(4));
    let others = ObjectList::from(vec![
        unit(object_id(20), 12.0, master_id(4)),
        unit(object_id(21), 12.0, master_id(4)),
        unit(object_id(22), 12.0, master_id(4)),
    ]);
    let mut parts = Parts::default();
    parts.set_unit_ai(
        object_id(20),
        Some(attacked(Attackers::of_saved(&[50, 51, 52, 53]))),
    );
    parts.set_unit_ai(object_id(22), Some(attacked(Attackers::of_saved(&[99]))));
    let enemy = Player {
        // 50 at a gap of 4.4, 51 of 2.4 (its radius 1), 53 of 2.4 too
        objects: vec![
            unit(object_id(50), 17.0, master_id(5)),
            unit(object_id(51), 16.0, ids::ID_6),
            unit(object_id(53), 16.0, ids::ID_6),
        ]
        .into(),
        ..Player::default()
    };
    let (gaia, after) = (Player::default(), [enemy]);
    let before = [gaia];
    let every = [
        Masters::default(),
        Masters::new([master(master_id(4), Footprint::new(0.3, 0.3))]),
        Masters::new([
            master(master_id(5), Footprint::new(0.3, 0.3)),
            master(ids::ID_6, Footprint::new(1.3, 0.3)),
        ]),
    ]
    .map(Arc::new);
    let attacker = Attacker {
        ai_type: UnitAiType::SOLDIER,
        player: PlayerId::new(1),
        player_type: PlayerType::HUMAN,
        state: ObjectState::Alive,
        held: None,
        top_action: None,
        master: master_id(4),
        kind: MasterKind::COMBAT,
        class: Some(class::CLASS_6),
        attack_priority: AttackPriority::PRIORITY_0,
        missile: None,
        range: Distance::ZERO,
        min_range: Distance::ZERO,
        charge_kind: ChargeKind::NONE,
        area_effect: Distance::ZERO,
        blast_level: BlastLevel::NONE,
        los: Distance::new(4.0),
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(0.3, 0.3),
        speed: Speed::new(1.0),
        scenario: false,
        firepower: Firepower::new(0.0),
        capture: CaptureKind::NONE,
    };
    let foreign = Foreign {
        before: &before,
        after: &after,
        left: None,
    };
    let picker = WorldPicker {
        by_id: ObjectsRef::empty(),
        attacker: &attacker,
        seeker: None,
        me: object_id(123),
        player: PlayerId::new(1),
        others: OthersRef::new(PlayerId::new(1), &others, foreign, ObjectIndex::scanning()),
        foreign,
        every: MasterTables::new(&every, no_own_masters()),
        parts: &parts,
        restriction: None,
        unit: &me,
        actions: &Action::default(),
        attributes: &[],
        relations: &ByPlayer::new(vec![Relation::ALLY, Relation::ALLY, Relation::ALLY]),
        ai_settings: AiSettings::default(),
        now: WorldTime::ZERO,
        mode_680: Mode680::ZERO,
    };
    assert_eq!(
        picker.nearest_attacker_of(object_id(20)),
        Ok(NearestAttacker::Attacker(object_id(51)))
    );
    assert_eq!(
        picker.nearest_attacker_of(object_id(21)),
        Ok(NearestAttacker::NoUnitAi)
    );
    assert_eq!(
        picker.nearest_attacker_of(object_id(22)),
        Ok(NearestAttacker::NoAttacker)
    );
    assert_eq!(
        picker.nearest_attacker_of(object_id(77)),
        Ok(NearestAttacker::NoUnitAi)
    );
    // an object the world does not hold is not weighed
    assert!(picker.duel(&Spatial::default(), object_id(77)).is_err());
}

#[test]
fn villager_fights_back_weak_attacker_of_its_own_master() {
    let villager = Master {
        id: ids::VILLAGER_MALE,
        copy_id: Some(ids::VILLAGER_MALE),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        object_flags: ObjectFlags::SCOUT,
        hit_points: MasterHitPoints::of_data(25),
        weapons: vec![ClassAmount::new(4, 3), ClassAmount::new(21, 6)],
        armors: vec![ClassAmount::new(4, 0), ClassAmount::new(3, 0)],
        weapon_range: Distance::ZERO,
        ..Master::default()
    };
    assert!(harmless(villager.clone()));
    let with = |f: &dyn Fn(&mut Master)| {
        let mut m = villager.clone();
        f(&mut m);
        harmless(m)
    };
    assert!(!with(&|m| m.object_flags = ObjectFlags::NONE));
    assert!(!with(&|m| m.copy_id = Some(ids::VILLAGER_FEMALE)));
    assert!(!with(&|m| m.weapons = vec![ClassAmount::new(4, 5)]));
    // an attack of 4 below 60 hit points
    assert!(with(&|m| m.weapons = vec![ClassAmount::new(4, 4)]));
    assert!(!with(&|m| (m.weapons, m.hit_points) =
        (vec![ClassAmount::new(4, 4)], MasterHitPoints::of_data(60))));
    assert!(with(&|m| (m.weapons, m.hit_points) =
        (vec![ClassAmount::new(4, 3)], MasterHitPoints::of_data(60))));
    assert!(!with(&|m| m.armors = vec![ClassAmount::new(4, 1)]));
    assert!(with(&|m| m.weapon_range = Distance::new(0.5)));
    assert!(!with(&|m| m.weapon_range = Distance::new(0.6)));
    assert!(!with(&|m| m.weapon_range = Distance::new(f32::NAN)));
}

/// What the statistics read of a candidate (`FUN_140c44680`'s reads): soldier 123 (master 4,
/// attack 6 of class 4, reload 2) of player 1 reads player 2's unit 50 (master 5, class 6,
/// attack 3, 40 hit points, no armor, standing): armed by its attacks, the soldier's range,
/// 6 damage a hit, its reload, the candidate's distance and category as given; not the
/// seeker's target, not a conversion. An object the world does not hold is not modelled.
#[test]
fn candidate_statistics_are_its_class_hit_points_and_the_damage_to_it() {
    use crate::unit_ai::AttackPicker;
    let master = |id, class, weapons| Master {
        id,
        kind: MasterKind::COMBAT,
        class,
        radius: Footprint::new(0.3, 0.3),
        weapons,
        reload: Seconds::new(2.0),
        // no terrain defense bonus: the damage reads no terrain
        terrain_bonus: RestrictionField::NONE,
        ..Master::default()
    };
    let unit = |id, x, master_id| Unit {
        id,
        place: Static::at(Position::new(x, 10.0, 0.0)),
        life: Lifecycle {
            master_id,
            state: ObjectState::Alive,
            hp: HitPoints::new(40.0),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let me = unit(object_id(123), 10.0, master_id(4));
    let enemy = Player {
        objects: vec![unit(object_id(50), 12.0, master_id(5))].into(),
        ..Player::default()
    };
    let (gaia, after) = (Player::default(), [enemy]);
    let before = [gaia];
    let every = [
        Masters::default(),
        Masters::new([master(
            master_id(4),
            class::CLASS_6,
            vec![ClassAmount::new(4, 6)],
        )]),
        Masters::new([master(
            master_id(5),
            class::CLASS_6,
            vec![ClassAmount::new(4, 3)],
        )]),
    ]
    .map(Arc::new);
    let spatial = Spatial::new(GridSize::new(120, 120));
    let attacker = Attacker {
        ai_type: UnitAiType::SOLDIER,
        player: PlayerId::new(1),
        player_type: PlayerType::HUMAN,
        state: ObjectState::Alive,
        held: None,
        top_action: None,
        master: master_id(4),
        kind: MasterKind::COMBAT,
        class: Some(class::CLASS_6),
        attack_priority: AttackPriority::PRIORITY_0,
        missile: None,
        range: Distance::new(0.5),
        min_range: Distance::ZERO,
        charge_kind: ChargeKind::NONE,
        area_effect: Distance::ZERO,
        blast_level: BlastLevel::NONE,
        los: Distance::new(4.0),
        position: Position::new(10.0, 10.0, 0.0),
        radius: Footprint::new(0.3, 0.3),
        speed: Speed::new(1.0),
        scenario: false,
        firepower: Firepower::new(0.0),
        capture: CaptureKind::NONE,
    };
    let none = ObjectList::default();
    let foreign = Foreign {
        before: &before,
        after: &after,
        left: None,
    };
    let mut picker = WorldPicker {
        by_id: ObjectsRef::empty(),
        attacker: &attacker,
        seeker: None,
        me: object_id(123),
        player: PlayerId::new(1),
        others: OthersRef::new(PlayerId::new(1), &none, foreign, ObjectIndex::scanning()),
        foreign,
        every: MasterTables::new(&every, no_own_masters()),
        parts: no_parts(),
        restriction: None,
        unit: &me,
        actions: &Action::default(),
        attributes: &[],
        relations: &ByPlayer::new(vec![Relation::ALLY, Relation::ALLY, Relation::ALLY]),
        ai_settings: AiSettings::default(),
        now: WorldTime::ZERO,
        mode_680: Mode680::ZERO,
    };
    let c = picker
        .candidate(
            &spatial,
            object_id(50),
            Distance::new(1.4),
            attack_category(2),
        )
        .unwrap();
    assert_eq!(
        (c.id, c.distance, c.category, c.class, c.speed),
        (
            object_id(50),
            Distance::new(1.4),
            attack_category(2),
            class::CLASS_6,
            Speed::ZERO
        )
    );
    assert_eq!(
        (c.armed, c.reach, c.damage, c.hp, c.reload),
        (
            true,
            Distance::new(0.5),
            HitPoints::new(6.0),
            HitPoints::new(40.0),
            Seconds::new(2.0)
        )
    );
    assert_eq!((c.workable, c.current), (false, false));
    assert!(
        picker
            .candidate(
                &spatial,
                object_id(77),
                Distance::new(1.4),
                attack_category(2)
            )
            .is_err()
    );
}
