use super::*;
use crate::combat::Charge;
use crate::combat::Firepower;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::movement::Speed;
use crate::movement::SpeedReads;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::layers::Static;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::ChargeTarget;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::{Master, MasterTables};
use crate::objects::object_list::ObjectList;
use crate::ownership::CaptureKind;
use crate::players::ByPlayer;
use crate::players::Relation;
use crate::test_ids::attack_category;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::testing::class;
use crate::testing::ids;
use crate::testing::picker::with_picker;
use crate::testing::roles;
use crate::testing::scene::{Scene, SceneObject};
use crate::testing::tuning;
use crate::unit_ai::Attackers;
use crate::unit_ai::NearestAttacker;
use crate::unit_ai::UnitAiKind;
use crate::world::Mode680;
use crate::world::Seconds;
use crate::world::objects_ref::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::parts::{Parts, no_own_masters, no_parts};
use std::sync::Arc;

/// Slot 0x228 (`FUN_141084070`): the attacks above 0 add up in their order from +0; none
/// (or none above 0) is +0.
#[test]
fn the_attacks_sum_is_the_positive_ones() {
    let master_of = |weapons: Vec<ClassAmount>| Master {
        combat: Some(CombatPart {
            weapons,
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    assert_eq!(
        attacks(&master_of(vec![
            ClassAmount::new(4, 6),
            ClassAmount::new(3, -2),
            ClassAmount::new(11, 1)
        ]))
        .to_bits(),
        7.0f32.to_bits()
    );
    assert_eq!(attacks(&master_of(vec![])).to_bits(), 0.0f32.to_bits());
    assert_eq!(
        attacks(&master_of(vec![
            ClassAmount::new(4, 0),
            ClassAmount::new(3, -5)
        ]))
        .to_bits(),
        0.0f32.to_bits()
    );
}
/// The villager 123 (player 1) tests player 2's unit 50 of `master`: harmless when its
/// master has object flag 0x10, is its own copy and shows an attack below 5 (4 only below 60
/// hit points), no melee armor and a range of at most 0.5.
fn harmless(master: Master) -> bool {
    harmless_on(master, None)
}

/// [`harmless`], with unit 50 (25 hit points) attacking player 2's object `attacked` (of master
/// 902, 25 hit points) where there is one.
fn harmless_on(master: Master, attacked: Option<i32>) -> bool {
    use crate::combat::attack::AttackState;
    use crate::combat::attack::tests::own_attack;
    use crate::objects::actions::ActionList;
    let me = Unit {
        id: object_id(123),
        life: Lifecycle {
            master_id: ids::VILLAGER_MALE,
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let enemy_unit = |id: i32, master_id| Unit {
        id: object_id(id),
        life: Lifecycle {
            master_id,
            state: ObjectState::Alive,
            hp: HitPoints::new(25.0),
            ..Lifecycle::default()
        },
        ..Unit::default()
    };
    let mut parts = Parts::default();
    let mut objects = vec![enemy_unit(50, master.id)];
    let mut masters = vec![master];
    if let Some(target) = attacked {
        let target_master = Master {
            id: master_id(902),
            kind: MasterKind::COMBAT,
            hit_points: MasterHitPoints::of_data(25),
            ..Master::default()
        };
        objects.push(enemy_unit(target, target_master.id));
        masters.push(target_master);
        parts.put_actions(
            object_id(50),
            Action {
                list: ActionList::default().with_attack(Some(own_attack(
                    target,
                    AttackState::New,
                    Distance::new(1.0),
                ))),
                ..Action::default()
            },
        );
    }
    let enemy = Player {
        objects: objects.into(),
        ..Player::default()
    };
    let (gaia, after) = (Player::default(), [enemy]);
    let before = [gaia];
    let every = [
        Masters::default(),
        Masters::default(),
        Masters::new(masters),
    ]
    .map(Arc::new);
    let spatial = Spatial::new(GridSize::new(120, 120));
    let attacker = Attacker {
        ai_kind: UnitAiKind::Civilian,
        player: PlayerId::new(1),
        player_kind: PlayerKind::Human,
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
        capture: CaptureKind::None,
    };
    let none = ObjectList::default();
    let foreign = Foreign {
        before: &before,
        after: &after,
        left: None,
    };
    let picker = WorldPicker {
        by_id: ObjectsRef::empty(),
        speeds: SpeedReads::empty(),
        attacker: &attacker,
        seeker: None,
        me: object_id(123),
        ai_target: None,
        player: PlayerId::new(1),
        others: OthersRef::new(PlayerId::new(1), &none, foreign, ObjectIndex::scanning()),
        foreign,
        every: MasterTables::new(&every, no_own_masters()),
        parts: &parts,
        restriction: None,
        unit: &me,
        actions: &Action::default(),
        attributes: &[],
        relations: &ByPlayer::new(vec![Relation::Ally, Relation::Ally, Relation::Ally]),
        ai_settings: AiSettings::default(),
        now: WorldTime::ZERO,
        mode_680: Mode680::ZERO,
    };
    picker
        .harmless(&spatial, object_id(50), &roles(), &tuning().unit_ai)
        .unwrap()
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
        ai_kind: UnitAiKind::Soldier,
        player: PlayerId::new(1),
        player_kind: PlayerKind::Human,
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
        capture: CaptureKind::None,
    };
    let foreign = Foreign {
        before: &before,
        after: &after,
        left: None,
    };
    let picker = WorldPicker {
        by_id: ObjectsRef::empty(),
        speeds: SpeedReads::empty(),
        attacker: &attacker,
        seeker: None,
        me: object_id(123),
        ai_target: None,
        player: PlayerId::new(1),
        others: OthersRef::new(PlayerId::new(1), &others, foreign, ObjectIndex::scanning()),
        foreign,
        every: MasterTables::new(&every, no_own_masters()),
        parts: &parts,
        restriction: None,
        unit: &me,
        actions: &Action::default(),
        attributes: &[],
        relations: &ByPlayer::new(vec![Relation::Ally, Relation::Ally, Relation::Ally]),
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
    assert!(
        picker
            .duel(
                &Spatial::default(),
                object_id(77),
                &roles(),
                &tuning().damage,
                &tuning().unit_ai
            )
            .is_err()
    );
}

/// The attacker's shown attack counts its Attack's target (`FUN_14107cc30`, `FUN_140a78450`): a
/// bonus damage task of 3 for each half of its hit points against master 902 makes an attacker of
/// 902's 50 fearsome, and one on the recyclable −51 of 902, which the world holds, harmless.
#[test]
fn attacker_on_recyclable_target_shows_no_bonus_damage() {
    use crate::combat::damage::BONUS_DAMAGE;
    use crate::objects::masters::{ActionPart, Task, WorkValue};
    use crate::players::attribute::AttributeId;
    let villager = Master {
        id: ids::VILLAGER_MALE,
        copy_id: Some(ids::VILLAGER_MALE),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        object_flags: ObjectFlags::SCOUT,
        hit_points: MasterHitPoints::of_data(25),
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 3)],
            weapon_range: Distance::ZERO,
            ..CombatPart::default()
        }),
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: BONUS_DAMAGE,
                object_id: Some(master_id(902)),
                attribute_kind4: Some(AttributeId::named(0)),
                work_val1: WorkValue::new(3.0),
                work_val2: WorkValue::new(0.5),
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..Master::default()
    };
    assert!(harmless(villager.clone()));
    assert!(!harmless_on(villager.clone(), Some(60)));
    assert!(harmless_on(villager, Some(-51)));
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
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 3), ClassAmount::new(21, 6)],
            armors: vec![ClassAmount::new(4, 0), ClassAmount::new(3, 0)],
            weapon_range: Distance::ZERO,
            ..CombatPart::default()
        }),
        ..Master::default()
    };
    assert!(harmless(villager.clone()));
    let with = |setup: &dyn Fn(&mut Master)| {
        let mut master = villager.clone();
        setup(&mut master);
        harmless(master)
    };
    assert!(!with(&|master| master.object_flags = ObjectFlags::NONE));
    assert!(!with(&|master| master.copy_id = Some(ids::VILLAGER_FEMALE)));
    assert!(!with(&|master| master
        .combat
        .get_or_insert_with(Default::default)
        .weapons = vec![ClassAmount::new(4, 5)]));
    // an attack of 4 below 60 hit points
    assert!(with(&|master| master
        .combat
        .get_or_insert_with(Default::default)
        .weapons = vec![ClassAmount::new(4, 4)]));
    assert!(!with(&|master| {
        master.combat.get_or_insert_with(Default::default).weapons = vec![ClassAmount::new(4, 4)];
        master.hit_points = MasterHitPoints::of_data(60);
    }));
    assert!(with(&|master| {
        master.combat.get_or_insert_with(Default::default).weapons = vec![ClassAmount::new(4, 3)];
        master.hit_points = MasterHitPoints::of_data(60);
    }));
    assert!(!with(&|master| master
        .combat
        .get_or_insert_with(Default::default)
        .armors = vec![ClassAmount::new(4, 1)]));
    assert!(with(&|master| master
        .combat
        .get_or_insert_with(Default::default)
        .weapon_range = Distance::new(0.5)));
    assert!(!with(&|master| master
        .combat
        .get_or_insert_with(Default::default)
        .weapon_range = Distance::new(0.6)));
    assert!(!with(&|master| master
        .combat
        .get_or_insert_with(Default::default)
        .weapon_range = Distance::new(f32::NAN)));
}

/// The picker's damage of its own unit reads `FUN_1412161b0` with the target of its unit AI,
/// which is out of the table while it runs ([`WorldPicker::ai_target`]): a charger of special
/// attack 3 with a full charge adds the charge to its first melee weapon only with a target.
#[test]
fn picker_damage_reads_its_unit_ai_target_for_the_charge() {
    let charger = Master {
        id: master_id(1755),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        hit_points: MasterHitPoints::of_data(100),
        radius: Footprint::new(0.5, 0.5),
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 3)],
            armors: vec![ClassAmount::new(4, 0)],
            terrain_bonus: RestrictionField::NONE,
            ..CombatPart::default()
        }),
        creatable: Some(CreatablePart {
            special_attack: SpecialAttack::CHARGE,
            charge_kind: ChargeKind::ATTACK,
            max_charge: Charge::new(10.0),
            charge_target: ChargeTarget::EVERY,
            ..CreatablePart::default()
        }),
        ..Master::default()
    };
    let mut me = SceneObject::new(
        5,
        PlayerId::new(1),
        charger.clone(),
        Position::new(8.0, 4.0, 0.0),
    );
    me.ai = Some(UnitAi {
        current_target: Some(object_id(6)),
        ..UnitAi::default()
    });
    let target = SceneObject::new(6, PlayerId::new(1), charger, Position::new(10.0, 4.0, 0.0));
    let mut world = Scene::open(GridSize::new(16, 16))
        .object(me)
        .object(target)
        .build();
    world.parts.combat_mut(object_id(5)).unwrap().charge = Charge::new(10.0);
    let roles = world.model.roles.clone();
    let damage_tuning = world.model.tuning.damage;
    let mut damage = |ai_target: Option<ObjectId>| {
        with_picker(
            &mut world,
            (PlayerId::new(1), object_id(5)),
            |picker, spatial| {
                picker.ai_target = ai_target;
                picker
                    .damage(spatial, object_id(5), object_id(6), &roles, &damage_tuning)
                    .unwrap()
            },
        )
    };
    assert_eq!(damage(Some(object_id(6))), HitPoints::new(10.0 + 3.0));
    assert_eq!(damage(None), HitPoints::new(3.0));
}

/// What the statistics read of a candidate (`FUN_140c44680`'s reads): soldier 123 (master 4,
/// attack 6 of class 4, reload 2) of player 1 reads player 2's unit 50 (master 5, class 6,
/// attack 3, 40 hit points, no armor, standing): armed by its attacks, the soldier's range,
/// 6 damage a hit, its reload, the candidate's distance and category as given; not the
/// seeker's target, not a conversion. An object the world does not hold is not modelled.
#[test]
fn candidate_statistics_are_its_class_hit_points_and_the_damage_to_it() {
    let master = |id, class, weapons| Master {
        id,
        kind: MasterKind::COMBAT,
        class,
        radius: Footprint::new(0.3, 0.3),
        combat: Some(CombatPart {
            weapons,
            reload: Seconds::new(2.0),
            // no terrain defense bonus: the damage reads no terrain
            terrain_bonus: RestrictionField::NONE,
            ..CombatPart::default()
        }),
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
        ai_kind: UnitAiKind::Soldier,
        player: PlayerId::new(1),
        player_kind: PlayerKind::Human,
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
        capture: CaptureKind::None,
    };
    let none = ObjectList::default();
    let foreign = Foreign {
        before: &before,
        after: &after,
        left: None,
    };
    let picker = WorldPicker {
        by_id: ObjectsRef::empty(),
        speeds: SpeedReads::empty(),
        attacker: &attacker,
        seeker: None,
        me: object_id(123),
        ai_target: None,
        player: PlayerId::new(1),
        others: OthersRef::new(PlayerId::new(1), &none, foreign, ObjectIndex::scanning()),
        foreign,
        every: MasterTables::new(&every, no_own_masters()),
        parts: no_parts(),
        restriction: None,
        unit: &me,
        actions: &Action::default(),
        attributes: &[],
        relations: &ByPlayer::new(vec![Relation::Ally, Relation::Ally, Relation::Ally]),
        ai_settings: AiSettings::default(),
        now: WorldTime::ZERO,
        mode_680: Mode680::ZERO,
    };
    let candidate = picker
        .candidate(
            &spatial,
            object_id(50),
            Distance::new(1.4),
            attack_category(2),
        )
        .unwrap();
    assert_eq!(
        (
            candidate.id,
            candidate.distance,
            candidate.category,
            candidate.class,
            candidate.speed
        ),
        (
            object_id(50),
            Distance::new(1.4),
            attack_category(2),
            class::CLASS_6,
            Speed::ZERO
        )
    );
    assert_eq!(
        (
            candidate.armed,
            candidate.reach,
            candidate.damage,
            candidate.hp,
            candidate.reload
        ),
        (
            true,
            Distance::new(0.5),
            HitPoints::new(6.0),
            HitPoints::new(40.0),
            Seconds::new(2.0)
        )
    );
    assert_eq!((candidate.workable, candidate.current), (false, false));
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
