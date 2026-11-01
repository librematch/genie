/// `FUN_1411cc890`: a building's relics raise its factor (a specific one 1.2, else any 1.1),
/// a building victim's lower it back the same way; another victim's count for nothing.
#[test]
fn a_building_s_relics_scale_its_elevation_factor() {
    let up = 1.25_f32;
    assert_eq!(
        building_elevation_factor(
            up,
            Relics::new(0, 0),
            master_type::COMBAT,
            Relics::new(5, 5)
        )
        .to_bits(),
        up.to_bits()
    );
    assert_eq!(
        building_elevation_factor(
            up,
            Relics::new(3, 1),
            master_type::COMBAT,
            Relics::new(0, 0)
        )
        .to_bits(),
        (up * SPECIFIC_RELIC_BOOST).to_bits()
    );
    assert_eq!(
        building_elevation_factor(
            up,
            Relics::new(3, 0),
            master_type::COMBAT,
            Relics::new(0, 0)
        )
        .to_bits(),
        (up * RELIC_BOOST).to_bits()
    );
    assert_eq!(
        building_elevation_factor(up, Relics::new(3, 0), BUILDING, Relics::new(0, 2)).to_bits(),
        (up * RELIC_BOOST / SPECIFIC_RELIC_BOOST).to_bits()
    );
    assert_eq!(
        building_elevation_factor(up, Relics::new(0, 0), BUILDING, Relics::new(1, -1)).to_bits(),
        (up / RELIC_BOOST).to_bits()
    );
    // the counts compare signed
    assert_eq!(
        building_elevation_factor(up, Relics::new(-1, -1), BUILDING, Relics::new(-1, -1)).to_bits(),
        up.to_bits()
    );
}

use super::*;
use crate::combat::damage::armor::{OwnSlot, class_armor};
use crate::map::Tile;
use crate::objects::HpPowerup;
use crate::objects::masters::ChargeEvent;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::RestrictionField;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::masters::master_type::BUILDING;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::test_ids::master_id;
use crate::test_ids::object_id;

fn sheep() -> Victim {
    Victim {
        id: object_id(9520),
        hp: HitPoints::new(14.0),
        kind: master_type::COMBAT,
        class: class::LIVESTOCK,
        master_id: master_id(705),
        player: PlayerId::new(1),
        position: Position::new(10.5, 10.5, 0.0),
        master_hp: MasterHitPoints::of_data(14),
        armored: Some(Armored {
            armors: vec![
                ClassAmount::new(4, 0),
                ClassAmount::new(3, 0),
                ClassAmount::new(31, 0),
            ],
            terrain_bonus: RestrictionField::NONE,
            friendly_fire: crate::objects::masters::DamageShare::new(1.0),
            ..Armored::default()
        }),
        ..Victim::default()
    }
}

fn hunter() -> Victim {
    Victim {
        id: object_id(10029),
        hp: HitPoints::new(25.0),
        kind: master_type::COMBAT,
        class: class::VILLAGER,
        master_id: master_id(592),
        player: PlayerId::new(1),
        position: Position::new(10.5, 11.0, 0.0),
        source: Attacker {
            id: object_id(10029),
            kind: master_type::COMBAT,
            class: class::VILLAGER,
            master_id: master_id(592),
            missile: None,
            player: PlayerId::new(1),
            ..Attacker::default()
        },
        weapons: vec![
            ClassAmount::new(11, 3),
            ClassAmount::new(4, 3),
            ClassAmount::new(13, 6),
            ClassAmount::new(17, 0),
        ],
        master_hp: MasterHitPoints::of_data(25),
        armored: Some(Armored {
            armors: vec![ClassAmount::new(4, 0), ClassAmount::new(3, 0)],
            terrain_bonus: RestrictionField::NONE,
            friendly_fire: crate::objects::masters::DamageShare::new(1.0),
            ..Armored::default()
        }),
        ..Victim::default()
    }
}

/// MarvinBlue/426048731's update 1170: the hunter 10029 (master 592: classes 11, 4, 13 and
/// 17 at 3, 3, 6 and 0) strikes its sheep 9520 (master 705: armor 0 for 4, 3 and 31, base 0):
/// 3 + 3 + 6 = 12, the game's 14 -> 2 hit points.
/// Slot 0x870 (`FUN_1410857f0`): the best pierce armor, else the base armor and the best of
/// any; a wall foundation (class 27, state 0) has no case of its own.
#[test]
fn a_wall_foundation_s_pierce_armor_is_computed_as_any_other() {
    let a = Armored {
        armors: vec![ClassAmount::new(4, 2), ClassAmount::new(3, 5)],
        base_armor: 1,
        ..Armored::default()
    };
    assert_eq!(
        class_armor(&a, OwnSlot::Pierce, class::WALL, ObjectState::Foundation),
        5
    );
    assert_eq!(
        class_armor(&a, OwnSlot::Pierce, class::WALL, ObjectState::Alive),
        5
    );
    let none = Armored {
        armors: vec![ClassAmount::new(4, 2)],
        base_armor: 1,
        ..Armored::default()
    };
    assert_eq!(
        class_armor(&none, OwnSlot::Pierce, class::WALL, ObjectState::Foundation),
        2
    );
    // its melee armor (FUN_141213810's wall foundation) is 0
    assert_eq!(
        class_armor(&a, OwnSlot::Melee, class::WALL, ObjectState::Foundation),
        0
    );
}

/// Slot 0xf0 (`FUN_14118cab0`): a TRIBE attacker whose charge hits the target (slot 0x2f0)
/// starts the static damage at its charge; any other starts at 0.
#[test]
fn a_static_object_takes_a_ready_charge_first() {
    let tree = || Victim {
        hp: HitPoints::new(100.0),
        class: class::TREE,
        kind: master_type::STATIC,
        ..Victim::default()
    };
    let charging = Attacker {
        kind: master_type::COMBAT,
        charge_type: ChargeType::ATTACK,
        charge_target: ChargeTarget::EVERY,
        charge: Charge::new(5.0),
        max_charge: Charge::new(5.0),
        ..Attacker::default()
    };
    let mut t = tree();
    take_static(&mut t, &charging, &[ClassAmount::new(4, 3)], 1.0);
    assert_eq!(t.hp, HitPoints::new(100.0 - (5.0 + 3.0)));
    let mut t = tree();
    let plain = Attacker {
        charge_type: ChargeType::NONE,
        ..charging
    };
    take_static(&mut t, &plain, &[ClassAmount::new(4, 3)], 1.0);
    assert_eq!(t.hp, HitPoints::new(97.0));
}

/// Slot 0xe8 of a moving or action object (types 30 to 49) is the static object's
/// (`FUN_14118efd0`): the weapons times the factor, no armor; below 1 hit point it has 0,
/// wakes and loses what it holds but to a villager. A NaN hit point count stays (`COMISS`/`JBE`).
#[test]
fn a_moving_object_takes_a_hit_as_a_static_one() {
    let bird = |hp: HitPoints| Victim {
        hp,
        amount: Amount::new(50.0),
        kind: master_type::ACTION,
        class: class::CLASS_38,
        goto_sleep: true,
        ..Victim::default()
    };
    let mut hunter = hunter();
    let mut t = bird(HitPoints::new(10.0));
    take(
        &mut t,
        &mut hunter,
        &mut [],
        &[ClassAmount::new(4, 3), ClassAmount::new(3, 2)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(
        (t.hp, t.amount, t.goto_sleep),
        (HitPoints::new(5.0), Amount::new(50.0), true)
    );
    take(
        &mut t,
        &mut hunter,
        &mut [],
        &[ClassAmount::new(4, 3), ClassAmount::new(3, 2)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    // a villager's hit leaves what it holds
    assert_eq!(
        (t.hp, t.amount, t.goto_sleep),
        (HitPoints::ZERO, Amount::new(50.0), false)
    );
    let mut archer = hunter.clone();
    archer.source.class = class::CLASS_0;
    let mut t = bird(HitPoints::new(4.0));
    take(
        &mut t,
        &mut archer,
        &mut [],
        &[ClassAmount::new(3, 4)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!((t.hp, t.amount), (HitPoints::ZERO, Amount::ZERO));
    let mut t = bird(HitPoints::new(f32::NAN));
    take(
        &mut t,
        &mut archer,
        &mut [],
        &[ClassAmount::new(3, 4)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert!(t.hp.get().is_nan() && t.amount == Amount::new(50.0) && t.goto_sleep);
}

#[test]
fn a_hunter_strikes_its_sheep_for_twelve() {
    let (mut sheep, mut hunter) = (sheep(), hunter());
    let weapons = hunter.weapons.clone();
    assert_eq!(
        armor_damage(&sheep, &weapons, 1.0, &hunter.source, 0.0),
        Ok(HitPoints::new(12.0))
    );
    take(
        &mut sheep,
        &mut hunter,
        &mut [],
        &weapons,
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(sheep.hp, HitPoints::new(2.0));
    assert_eq!(sheep.damaged, [(Some(object_id(10029)), [10029, 2, 14])]);
    // the next hit kills it, and Gaia takes a player's livestock
    take(
        &mut sheep,
        &mut hunter,
        &mut [],
        &weapons,
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(sheep.hp, HitPoints::ZERO);
    assert_eq!(
        sheep.killed,
        Some(Kill {
            attacker: object_id(10029),
            attacker_player: PlayerId::new(1),
            attacker_class: class::VILLAGER,
            to_gaia: true
        })
    );
    // armor above the attack does nothing, and a hit does at least one
    let mut armored = self::sheep();
    let x = armored.armored.as_mut().unwrap();
    (x.armors, x.base_armor) = (vec![ClassAmount::new(4, 10)], 10000);
    assert_eq!(
        armor_damage(
            &armored,
            &[ClassAmount::new(4, 3)],
            1.0,
            &hunter.source,
            0.0
        ),
        Ok(HitPoints::new(1.0))
    );
    let zero = Attacker {
        mode_680: 1,
        ..hunter.source
    };
    assert_eq!(
        armor_damage(&armored, &[ClassAmount::new(4, 3)], 1.0, &zero, 0.0),
        Ok(HitPoints::ZERO)
    );
}

/// Slots 0xc0 and 0xc8 of a combat unit read its hit point powerups; a type-50 object's do
/// not. The share is at most 1 (`MINSS`), 0 without most hit points (lacking too), and a
/// NaN most divides.
#[test]
fn a_hit_point_share_reads_the_powerups_and_the_master() {
    let mut v = hunter();
    assert_eq!(
        (v.shown_hp(), v.most_hp(), v.hp_share(false)),
        (HitPoints::new(25.0), HitPoints::new(25.0), 1.0)
    );
    assert_eq!(v.hp_share(true), 0.0);
    v.hp = HitPoints::new(10.0);
    v.armored.as_mut().unwrap().hp_powerup = HpPowerup {
        taken: HitPoints::new(2.0),
        max: HitPoints::new(5.0),
    };
    assert_eq!(
        (v.shown_hp(), v.most_hp()),
        (HitPoints::new(13.0), HitPoints::new(30.0))
    );
    assert_eq!(v.hp_share(false), 13.0 / 30.0);
    assert_eq!(v.hp_share(true), 1.0 - 13.0 / 30.0);
    v.hp = HitPoints::new(40.0);
    assert_eq!(v.hp_share(false), 1.0);
    let mut rge = hunter();
    rge.kind = master_type::RGE_COMBAT;
    rge.hp = HitPoints::new(10.0);
    rge.armored.as_mut().unwrap().hp_powerup = HpPowerup {
        taken: HitPoints::new(2.0),
        max: HitPoints::new(5.0),
    };
    assert_eq!(
        (rge.shown_hp(), rge.most_hp()),
        (HitPoints::new(10.0), HitPoints::new(25.0))
    );
    let mut none = hunter();
    none.master_hp = MasterHitPoints::of_data(0);
    assert_eq!((none.hp_share(false), none.hp_share(true)), (0.0, 0.0));
    let mut nan = hunter();
    nan.armored.as_mut().unwrap().hp_powerup = HpPowerup {
        taken: HitPoints::ZERO,
        max: HitPoints::new(f32::NAN),
    };
    assert!(nan.hp_share(false).is_nan());
}

/// `FUN_14107ce60`: each task of action type 0xa0 that fits the target adds its amount per
/// whole step of a share (the target's with a work range above 0, else the attacker's; the
/// share lacking with `attribute_type4`), and the sum is at least 0.
#[test]
fn bonus_damage_counts_whole_steps_of_a_hit_point_share() {
    let task = |object_group: i16, object_id: Option<MasterId>, range: Distance, lacking: i16| {
        crate::objects::masters::Task {
            action_type: BONUS_DAMAGE,
            object_group: crate::test_ids::class_of(object_group),
            object_id,
            work_range: range,
            attribute_type4: AttributeId::of_data(lacking),
            work_val1: 2.0,
            work_val2: 0.25,
            ..crate::objects::masters::Task::default()
        }
    };
    let (mut sheep, mut hunter) = (sheep(), hunter());
    sheep.hp = HitPoints::new(7.0);
    // any target below type 80: the sheep lacks half its hit points, two steps of 0.25
    hunter.bonus_tasks = vec![task(-1, None, Distance::new(1.0), 1)];
    let steps = libm::floorf(0.5 / 0.25 + EPSILON);
    assert_eq!(bonus_damage(&hunter, &sheep), steps * 2.0);
    // the attacker's share (a work range of 0), whole: four steps
    hunter.bonus_tasks = vec![task(-1, None, Distance::ZERO, 0)];
    assert_eq!(bonus_damage(&hunter, &sheep), 4.0 * 2.0);
    // by class or master; another class, master or action type adds nothing
    hunter.bonus_tasks = vec![
        task(class::LIVESTOCK.get(), Some(ids::ID_0), Distance::ZERO, 0),
        task(-1, Some(master_id(705)), Distance::ZERO, 0),
        task(3, Some(ids::ID_0), Distance::ZERO, 0),
        task(-1, Some(master_id(9)), Distance::ZERO, 0),
        crate::objects::masters::Task {
            action_type: crate::combat::abilities::REMOVAL_REWARD,
            ..task(-1, None, Distance::ZERO, 0)
        },
    ];
    assert_eq!(bonus_damage(&hunter, &sheep), 16.0);
    // a building of another class than 51 and 54 needs its class or master
    let mut tc = sheep.clone();
    (tc.kind, tc.class) = (BUILDING, class::BUILDING);
    hunter.bonus_tasks = vec![task(-1, None, Distance::ZERO, 0)];
    assert_eq!(bonus_damage(&hunter, &tc), 0.0);
    tc.class = class::CLASS_51;
    assert_eq!(bonus_damage(&hunter, &tc), 8.0);
    // no step, no target hit points, or a negative sum: 0
    hunter.bonus_tasks = vec![crate::objects::masters::Task {
        work_val2: 0.0,
        ..task(-1, None, Distance::ZERO, 0)
    }];
    assert_eq!(bonus_damage(&hunter, &sheep), 0.0);
    hunter.bonus_tasks = vec![task(-1, None, Distance::ZERO, 0)];
    sheep.hp = HitPoints::ZERO;
    assert_eq!(bonus_damage(&hunter, &sheep), 0.0);
    sheep.hp = HitPoints::new(7.0);
    hunter.bonus_tasks = vec![crate::objects::masters::Task {
        work_val1: -3.0,
        ..task(-1, None, Distance::ZERO, 0)
    }];
    assert_eq!(bonus_damage(&hunter, &sheep), 0.0);
}

/// The first melee weapon adds the bonus with the powerup.
#[test]
fn the_bonus_damage_adds_to_the_first_melee_weapon() {
    let (sheep, mut hunter) = (sheep(), hunter());
    hunter.source.attack_powerup = 1.0;
    assert_eq!(
        armor_damage(
            &sheep,
            &[ClassAmount::new(4, 3), ClassAmount::new(3, 2)],
            1.0,
            &hunter.source,
            2.0
        ),
        Ok(HitPoints::new(3.0 + 2.0 + 1.0 + 2.0))
    );
}

/// Slots 0x878, 0x868 and 0x880: the best fighting attack (else any) with the whole
/// powerup, the bonus against the attack's target and a ready charge of type 1; a ready
/// charge of type 6 or 7 shows the projectile's attack and adds the charge event to the
/// range; type 50 shows neither powerups nor charges.
#[test]
fn a_combat_object_shows_its_attack_armor_and_range() {
    let (sheep, mut v) = (sheep(), hunter());
    let none = Shown::default();
    assert_eq!(shown_attack(&v, &none), 3);
    v.source.attack_powerup = 1.7;
    assert_eq!(shown_attack(&v, &none), 4);
    let mut far = v.clone();
    far.weapons = vec![ClassAmount::new(13, 6), ClassAmount::new(11, -2)];
    assert_eq!(shown_attack(&far, &none), 7);
    far.kind = master_type::RGE_COMBAT;
    assert_eq!(shown_attack(&far, &none), 6);
    // the bonus against its attack's target
    v.bonus_tasks = vec![crate::objects::masters::Task {
        action_type: BONUS_DAMAGE,
        attribute_type4: Some(attribute::FOOD),
        work_val1: 1.5,
        work_val2: 0.5,
        ..crate::objects::masters::Task::default()
    }];
    let at_sheep = Shown {
        target: Some(&sheep),
        ..Shown::default()
    };
    assert_eq!(shown_attack(&v, &at_sheep), 4 + 3);
    v.bonus_tasks.clear();
    // a ready charge of type 1 for every target, or for its target's class group
    (v.source.charge_type, v.source.max_charge, v.source.charge) =
        (ChargeType::ATTACK, Charge::new(5.0), Charge::new(5.5));
    v.source.charge_target = ChargeTarget::EVERY;
    assert_eq!(shown_attack(&v, &none), 4 + 5);
    v.source.charge_target = ChargeTarget::ALL_BUT_BUILDINGS;
    assert_eq!(shown_attack(&v, &none), 4);
    assert_eq!(shown_attack(&v, &at_sheep), 4 + 5);
    // a ready charge of type 6 shows the projectile's best fighting attack, and its range
    // the charge event
    (v.source.charge_type, v.source.charge_target) = (ChargeType::of_data(6), ChargeTarget::EVERY);
    let projectile = [
        ClassAmount::new(13, 20),
        ClassAmount::new(3, 9),
        ClassAmount::new(4, 2),
    ];
    let shot = Shown {
        projectile: Some(&projectile),
        charge_event: ChargeEvent::of_data(2),
        range_powerup: Distance::new(1.0),
        ..Shown::default()
    };
    assert_eq!(shown_attack(&v, &shot), 9);
    v.source.weapon_range = Distance::new(4.0);
    assert_eq!(shown_range(&v, &shot), Distance::new(1.0 + (2.0 + 4.0)));
    v.source.charge = Charge::new(0.5);
    assert_eq!(shown_attack(&v, &shot), 4);
    assert_eq!(shown_range(&v, &shot), Distance::new(5.0));
    v.kind = master_type::RGE_COMBAT;
    v.source.charge = Charge::new(1.0);
    assert_eq!(shown_range(&v, &shot), Distance::new(4.0));
    // the melee armor: its best of class 4 plus the powerup; a wall foundation none; type 50
    // without the powerup; else its best of any, else its base armor
    let mut a = hunter();
    a.armored.as_mut().unwrap().armor_powerups = ArmorPair::new(2, 0);
    assert_eq!(shown_melee_armor(&a), Ok(2));
    (a.class, a.state) = (class::WALL, ObjectState::Foundation);
    assert_eq!(shown_melee_armor(&a), Ok(0));
    a.kind = master_type::RGE_COMBAT;
    a.armored.as_mut().unwrap().armors = vec![ClassAmount::new(3, 5), ClassAmount::new(11, 7)];
    assert_eq!(shown_melee_armor(&a), Ok(7));
    a.armored.as_mut().unwrap().armors.clear();
    a.armored.as_mut().unwrap().base_armor = -1;
    assert_eq!(shown_melee_armor(&a), Ok(-1));
    a.armored = None;
    assert!(shown_melee_armor(&a).is_err());
}

/// Slot 0x2f8: a ready charge of type 6 or 7 that works on the target.
#[test]
fn a_range_charge_hits_a_target_its_class_group_names() {
    let (sheep, mut v) = (sheep(), hunter());
    (v.source.charge_type, v.source.max_charge, v.source.charge) =
        (ChargeType::of_data(7), Charge::new(5.0), Charge::new(1.0));
    v.source.charge_target = ChargeTarget::ALL_BUT_BUILDINGS;
    assert!(range_charge_hit(&v.source, &sheep));
    v.source.charge = Charge::new(0.5);
    assert!(!range_charge_hit(&v.source, &sheep));
    (v.source.charge_type, v.source.charge) = (ChargeType::ATTACK, Charge::new(9.0));
    assert!(!range_charge_hit(&v.source, &sheep));
}

/// The terrain defense scales each weapon's damage; a restriction value of 0 counts as 1 for
/// the caller ([`crate::map::terrain::Terrain::defense`]).
#[test]
fn a_terrain_defense_bonus_scales_the_damage() {
    let hunter = hunter();
    let mut sheep = sheep();
    let x = sheep.armored.as_mut().unwrap();
    (x.terrain_bonus, x.terrain) = (RestrictionField::of_data(4), Some(1.5));
    assert_eq!(
        armor_damage(&sheep, &[ClassAmount::new(4, 4)], 1.0, &hunter.source, 0.0),
        Ok(HitPoints::new(6.0))
    );
    sheep.armored.as_mut().unwrap().terrain = None;
    assert!(armor_damage(&sheep, &[ClassAmount::new(4, 4)], 1.0, &hunter.source, 0.0).is_err());
}

/// A ready charge of type 1 adds itself to the first melee weapon; the hit is the charge's,
/// which empties it (no area effect).
#[test]
fn a_ready_charge_adds_to_the_first_melee_weapon_and_empties() {
    let (mut sheep, mut hunter) = (sheep(), hunter());
    hunter.source.charge_type = ChargeType::ATTACK;
    hunter.source.charge_target = ChargeTarget::EVERY;
    hunter.source.max_charge = Charge::new(5.0);
    hunter.source.charge = Charge::new(5.0);
    assert_eq!(
        armor_damage(
            &sheep,
            &[ClassAmount::new(4, 3), ClassAmount::new(3, 2)],
            1.0,
            &hunter.source,
            0.0
        ),
        Ok(HitPoints::new(5.0 + 3.0 + 2.0))
    );
    take(
        &mut sheep,
        &mut hunter,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(sheep.hp, HitPoints::new(14.0 - 8.0));
    assert!(hunter.source.charge_hit);
    assert_eq!(hunter.source.charge, Charge::ZERO);
    // not ready: no charge
    hunter.source.charge = Charge::new(4.0);
    assert_eq!(
        armor_damage(&sheep, &[ClassAmount::new(4, 3)], 1.0, &hunter.source, 0.0),
        Ok(HitPoints::new(3.0))
    );
}

#[test]
fn a_charge_target_picks_the_class_groups() {
    // 0: everything but buildings outside classes 51 and 54
    assert!(charge_works(
        ChargeTarget::ALL_BUT_BUILDINGS,
        ChargeType::ATTACK,
        class::CLASS_6,
        master_type::COMBAT
    ));
    assert!(!charge_works(
        ChargeTarget::ALL_BUT_BUILDINGS,
        ChargeType::ATTACK,
        class::BUILDING,
        BUILDING
    ));
    assert!(charge_works(
        ChargeTarget::ALL_BUT_BUILDINGS,
        ChargeType::ATTACK,
        class::CLASS_51,
        master_type::COMBAT
    ));
    // bit 0 infantry (6), bit 8 buildings (3)
    assert!(charge_works(
        ChargeTarget::of_data(1),
        ChargeType::ATTACK,
        class::CLASS_6,
        master_type::COMBAT
    ));
    assert!(!charge_works(
        ChargeTarget::of_data(1),
        ChargeType::ATTACK,
        class::CLASS_12,
        master_type::COMBAT
    ));
    assert!(charge_works(
        ChargeTarget::of_data(0x100),
        ChargeType::ATTACK,
        class::BUILDING,
        BUILDING
    ));
    assert!(!charge_works(
        ChargeTarget::of_data(0x100),
        ChargeType::ATTACK,
        ClassId::of_master(99),
        master_type::COMBAT
    ));
    assert!(charge_works(
        ChargeTarget::of_data(0x10),
        ChargeType::EVERY_TARGET,
        ClassId::of_master(99),
        master_type::COMBAT
    ));
}

/// A shield of type 5 spends a charge of 1 on a melee hit; type 4 only on a missile.
#[test]
fn a_shield_spends_a_charge_per_blocked_hit() {
    let mut sheep = sheep();
    sheep.source.charge_type = ChargeType::MELEE_SHIELD;
    sheep.source.charge = Charge::new(1.5);
    assert!(!shield(&mut sheep, true));
    assert!(shield(&mut sheep, false));
    assert_eq!(sheep.source.charge, Charge::new(0.5));
    assert!(!shield(&mut sheep, false));
    // COMISS 1.0, charge; JA past: a NaN charge blocks the hit and stays NaN
    sheep.source.charge = Charge::new(f32::NAN);
    assert!(shield(&mut sheep, false));
    assert!(sheep.source.charge.get().is_nan());
}

#[test]
fn a_facing_offset_counts_facets_from_the_angle() {
    let at = Position::new(10.0, 10.0, 0.0);
    let facing = Facing {
        angle: Angle::ZERO,
        facets: 16,
    };
    assert_eq!(
        facing_offset(at, facing, Position::new(11.0, 10.0, 0.0)),
        0.0
    );
    // a quarter turn is four of 16 facets; fewer than 8 facets count as 8
    let q = facing_offset(at, facing, Position::new(10.0, 11.0, 0.0));
    assert!((q - 4.0).abs() < 1e-5);
    let few = Facing {
        facets: 4,
        ..facing
    };
    assert!((facing_offset(at, few, Position::new(10.0, 11.0, 0.0)) - 2.0).abs() < 1e-5);
    // a NaN difference goes on to atan2f (COMISS/JC, JNC: unordered is not "within 1e-7")
    assert!(facing_offset(at, facing, Position::new(f32::NAN, 10.0, 0.0)).is_nan());
    // behind wraps into −π..π, and no sprite or the same point gives 0
    let back = facing_offset(at, facing, Position::new(9.0, 9.999, 0.0));
    assert!(7.9 < back.abs() && back.abs() <= 8.0);
    assert_eq!(
        facing_offset(
            at,
            Facing {
                facets: 0,
                ..facing
            },
            Position::new(11.0, 12.0, 0.0)
        ),
        0.0
    );
    assert_eq!(facing_offset(at, facing, at), 0.0);
}

/// A special attack (flag 1) of an idle target facing its attacker takes the damage to 0.66.
#[test]
fn a_special_attack_takes_a_hit_from_the_front_to_two_thirds() {
    let (mut sheep, mut hunter) = (sheep(), hunter());
    let x = sheep.armored.as_mut().unwrap();
    x.special_attack = SpecialAttack::FRONT_DEFENCE;
    x.facing = Some(Facing {
        angle: Angle::new(std::f32::consts::FRAC_PI_2),
        facets: 8,
    });
    take(
        &mut sheep,
        &mut hunter,
        &mut [],
        &[ClassAmount::new(4, 10)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(sheep.hp, HitPoints::new(14.0 - 10.0 * SPECIAL));
    // a busy target does not, and an unknown facing is not modelled
    let mut busy = self::sheep();
    busy.armored.as_mut().unwrap().special_attack = SpecialAttack::FRONT_DEFENCE;
    busy.armored.as_mut().unwrap().busy = true;
    take(
        &mut busy,
        &mut hunter,
        &mut [],
        &[ClassAmount::new(4, 10)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(busy.hp, HitPoints::new(4.0));
    let mut unknown = self::sheep();
    unknown.armored.as_mut().unwrap().special_attack = SpecialAttack::FRONT_DEFENCE;
    assert!(
        take(
            &mut unknown,
            &mut hunter,
            &mut [],
            &[ClassAmount::new(4, 10)],
            1.0,
            HitPoints::ZERO
        )
        .is_err()
    );
}

/// A retaliation (flag 2) strikes a melee attacker first; an attacker it leaves at 1 hit
/// point or less takes the hit to nothing.
#[test]
fn a_retaliation_strikes_the_melee_attacker_first() {
    let (mut sheep, mut hunter) = (sheep(), hunter());
    sheep.weapons = vec![ClassAmount::new(4, 5)];
    let x = sheep.armored.as_mut().unwrap();
    x.special_attack = SpecialAttack::RETALIATION;
    x.unit_ai = true;
    x.alert_idle = Some(true);
    x.facing = Some(Facing {
        angle: Angle::new(std::f32::consts::FRAC_PI_2),
        facets: 8,
    });
    take(
        &mut sheep,
        &mut hunter,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(
        (hunter.hp, sheep.hp),
        (HitPoints::new(20.0), HitPoints::new(11.0))
    );
    assert_eq!(hunter.damaged, [(Some(object_id(9520)), [9520, 20, 25])]);
    hunter.hp = HitPoints::new(5.0);
    take(
        &mut sheep,
        &mut hunter,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    // the hunter dies; the sheep loses the least, one hit point
    assert_eq!(
        (hunter.hp, sheep.hp),
        (HitPoints::ZERO, HitPoints::new(10.0))
    );
    assert!(hunter.killed.is_some());
}

/// A target that reflects gives its attacker that share of its loss, at least 1.
#[test]
fn reflected_damage_comes_back_at_least_one() {
    let (mut sheep, mut hunter) = (sheep(), hunter());
    sheep.armored.as_mut().unwrap().reflection = crate::objects::masters::DamageShare::new(0.5);
    take(
        &mut sheep,
        &mut hunter,
        &mut [],
        &[ClassAmount::new(4, 6)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(
        (sheep.hp, hunter.hp),
        (HitPoints::new(8.0), HitPoints::new(22.0))
    );
    take(
        &mut sheep,
        &mut hunter,
        &mut [],
        &[ClassAmount::new(4, 1)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(
        (sheep.hp, hunter.hp),
        (HitPoints::new(7.0), HitPoints::new(21.0))
    );
    assert!(hunter.damaged.is_empty() && hunter.killed.is_none());
}

/// `FUN_14107f690`'s float compares as its assembly has them: a NaN gap past a radius counts
/// as 0 (`COMISS`/`JBE`), a NaN offset stays in a narrow cone (two `JA`s past), a NaN
/// `area_damage` leaves the factor as it is (`JC`, then `JBE` past the multiply); the tiles'
/// rectangle truncates with `CVTTSS2SI`; the answer is whether the radius is above 0 or NaN.
#[test]
fn a_blast_s_nan_paths_are_the_game_s() {
    let mut elephant = hunter();
    elephant.source.area_effect = Distance::new(1.0);
    elephant.source.blast_level = BlastLevel::from_bits_retain(0x42);
    elephant.source.area_damage = crate::objects::masters::DamageShare::new(0.5);
    elephant.weapons = vec![ClassAmount::new(4, 10)];
    let center = elephant.position;
    let enemy = |x: f32, y: f32| Victim {
        id: object_id(2),
        player: PlayerId::new(2),
        position: Position::new(x, y, 0.0),
        blast_defense: BlastDefense::new(2),
        ..sheep()
    };
    // a NaN radius on x: the gap on x is 0, the falloff takes the gap on y alone
    let mut t = Victim {
        radius: Footprint::new(f32::NAN, 0.0),
        ..enemy(10.8, 11.5)
    };
    assert_eq!(
        blast(&mut elephant, center, None, None, &mut [&mut t], 1.0),
        Ok(true)
    );
    let f = (1.0 - fmath::sqrt(0.5 * 0.5)) / 1.0 * 0.5;
    assert_eq!(t.hp, HitPoints::new(14.0 - 10.0 * f));
    // a narrow cone keeps a NaN offset
    elephant.source.blast_level = BlastLevel::from_bits_retain(0x82);
    elephant.armored.as_mut().unwrap().facing = Some(Facing {
        angle: Angle::new(f32::NAN),
        facets: 8,
    });
    let mut t = enemy(10.8, 11.0);
    assert!(
        facing_offset(
            center,
            Facing {
                angle: Angle::new(f32::NAN),
                facets: 8
            },
            t.position
        )
        .is_nan()
    );
    assert_eq!(
        blast(&mut elephant, center, None, None, &mut [&mut t], 1.0),
        Ok(true)
    );
    assert_eq!(t.hp, HitPoints::new(9.0));
    // a NaN area damage share leaves the factor at 1
    elephant.source.blast_level = BlastLevel::from_bits_retain(2);
    elephant.source.area_damage = crate::objects::masters::DamageShare::new(f32::NAN);
    let mut t = enemy(10.8, 11.0);
    assert_eq!(
        blast(&mut elephant, center, None, None, &mut [&mut t], 1.0),
        Ok(true)
    );
    assert_eq!(t.hp, HitPoints::new(4.0));
    // no radius, no blast; a NaN radius blasts
    elephant.source.area_effect = Distance::ZERO;
    assert_eq!(
        blast(&mut elephant, center, None, None, &mut [], 1.0),
        Ok(false)
    );
    elephant.source.area_effect = Distance::new(f32::NAN);
    assert_eq!(
        blast(&mut elephant, center, None, None, &mut [], 1.0),
        Ok(true)
    );
    // CVTTSS2SI gives 0x80000000 past the i32 range: 0 in the low short
    assert_eq!(
        blast_tiles(Position::new(3e9, 3e9, 0.0), Distance::new(1.0)),
        (Tile::new(0, 0), Tile::new(6, 6))
    );
    assert_eq!(
        blast_tiles(Position::new(10.5, 4.5, 0.0), Distance::new(1.0)),
        (Tile::new(3, 0), Tile::new(17, 11))
    );
}

/// A melee blast (radius 0.5, level 2, half the damage) reaches the objects about the
/// attacker but its friends, its target excluded; a falloff (bit 6) scales by the distance.
#[test]
fn a_blast_reaches_the_objects_about_its_attacker() {
    let mut elephant = hunter();
    elephant.source.area_effect = Distance::new(1.0);
    elephant.source.blast_level = BlastLevel::from_bits_retain(2);
    elephant.source.area_damage = crate::objects::masters::DamageShare::new(0.5);
    elephant.source.player = PlayerId::new(1);
    elephant.weapons = vec![ClassAmount::new(4, 10)];
    let enemy = |id: i32, x: f32| Victim {
        id: object_id(id),
        player: PlayerId::new(2),
        position: Position::new(x, 11.0, 0.0),
        blast_defense: BlastDefense::new(2),
        ..sheep()
    };
    let mut target = enemy(1, 10.6);
    let mut near = enemy(2, 10.8);
    let mut far = enemy(3, 12.0);
    let mut friend = Victim {
        allied: Allied::new(true, true),
        ..enemy(4, 10.7)
    };
    let center = elephant.position;
    let mut all = [&mut target, &mut near, &mut far, &mut friend];
    assert_eq!(
        blast(
            &mut elephant,
            center,
            Some(object_id(1)),
            None,
            &mut all,
            1.0
        ),
        Ok(true)
    );
    assert_eq!(
        (target.hp, near.hp, far.hp, friend.hp),
        (
            HitPoints::new(14.0),
            HitPoints::new(9.0),
            HitPoints::new(14.0),
            HitPoints::new(14.0)
        )
    );
    // the falloff: 0.3 of radius 1 away keeps 0.7
    elephant.source.blast_level = BlastLevel::from_bits_retain(0x42);
    let mut near = enemy(2, 10.8);
    let mut all = [&mut near];
    assert_eq!(
        blast(&mut elephant, center, None, None, &mut all, 1.0),
        Ok(true)
    );
    let dx = (10.8f32 - 10.5f32).abs();
    let f = (1.0 - fmath::sqrt(0.0 * 0.0 + dx * dx)) / 1.0 * 0.5;
    assert_eq!(near.hp, HitPoints::new(14.0 - 10.0 * f));
}

/// A player's house (master 70, class 3, type 80) of 550 hit points: melee armor 0, pierce
/// armor 7; it cost 25 wood, paid.
fn house() -> Victim {
    Victim {
        id: object_id(500),
        hp: HitPoints::new(550.0),
        kind: BUILDING,
        class: class::BUILDING,
        master_id: ids::HOUSE,
        player: PlayerId::new(2),
        position: Position::new(20.5, 20.5, 0.0),
        master_hp: MasterHitPoints::of_data(550),
        armored: Some(Armored {
            armors: vec![ClassAmount::new(4, 0), ClassAmount::new(3, 7)],
            terrain_bonus: RestrictionField::NONE,
            friendly_fire: crate::objects::masters::DamageShare::new(1.0),
            ..Armored::default()
        }),
        building: Some(Building {
            cost: 25.0,
            ..Building::default()
        }),
        ..Victim::default()
    }
}

/// `FUN_1411cea90`: a hit on a building is its damage (`FUN_141082e80`), then 60 for
/// `damaged_lately_timer` (`+0x30d`). Without a blast's damage so far (`param_8`, `UCOMISS`
/// with 0: exactly 0), a hit that took less than its least takes the least again (slot
/// 0xb0 with it, not the difference: `MOVAPS XMM1,XMM6` at `0x1411cef02`): 1, or 0.1 for an
/// attacker of class 0, 22, 36, 44 or 52 under the game's setting `+0x680` at 1.
#[test]
fn a_building_takes_its_damage_and_at_least_its_least_hit() {
    let mut villager = hunter();
    let mut h = house();
    take(
        &mut h,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(h.hp, HitPoints::new(547.0));
    assert_eq!(h.damaged, [(Some(object_id(10029)), [10029, 547, 550])]);
    let b = h.building.as_ref().unwrap();
    assert_eq!((b.damaged_lately, b.struck, b.razed.len()), (60, true, 0));
    // at 1 a hit may do nothing: the least takes 1 more
    villager.source.mode_680 = 1;
    let mut h = house();
    h.armored.as_mut().unwrap().armors = vec![ClassAmount::new(4, 10)];
    take(
        &mut h,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(h.hp, HitPoints::new(549.0));
    // half a hit point taken: the least is taken whole again
    let mut h = house();
    take(
        &mut h,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 1)],
        0.5,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(h.hp, HitPoints::new(548.5));
    // an archer's least is a tenth
    let mut archer = villager.clone();
    archer.source.class = class::CLASS_0;
    let mut h = house();
    h.armored.as_mut().unwrap().armors = vec![ClassAmount::new(4, 10)];
    take(
        &mut h,
        &mut archer,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(h.hp, HitPoints::new(550.0 - LEAST_CLASS_HIT));
    // a blast's damage so far, even a NaN one, takes no least
    let mut h = house();
    h.armored.as_mut().unwrap().armors = vec![ClassAmount::new(4, 10)];
    take(
        &mut h,
        &mut archer,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::new(2.0),
    )
    .unwrap();
    assert_eq!(h.hp, HitPoints::new(550.0));
    take(
        &mut h,
        &mut archer,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::new(f32::NAN),
    )
    .unwrap();
    assert_eq!(h.hp, HitPoints::new(550.0));
}

/// `COMISS 0, hp` / `JNC` past: a building at 0 hit points or fewer takes nothing (and is not
/// struck); a NaN goes on.
#[test]
fn a_building_without_hit_points_takes_no_hit() {
    let mut villager = hunter();
    let mut h = Victim {
        hp: HitPoints::ZERO,
        ..house()
    };
    take(
        &mut h,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!((h.hp, h.damaged.is_empty()), (HitPoints::ZERO, true));
    assert!(!h.building.as_ref().unwrap().struck);
    let mut h = Victim {
        hp: HitPoints::new(f32::NAN),
        ..house()
    };
    take(
        &mut h,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert!(!h.damaged.is_empty() && h.building.as_ref().unwrap().struck);
    // a NaN is not below 1: no razing
    assert_eq!(h.building.as_ref().unwrap().razed, Vec::new());
}

/// An annex (`+0xbb8`) gives a hit with the flag set (every caller's) to its main building's
/// slot 0xe8 with the flag clear: the main building's armor takes it, and each of its annexes
/// (`+0xbc0`..) takes its hit points; the annex itself is not struck. A main building the hit
/// does not hold is not modelled.
#[test]
fn a_hit_on_an_annex_goes_to_its_main_building() {
    let mut villager = hunter();
    let mut main = Victim {
        id: object_id(10),
        hp: HitPoints::new(2400.0),
        building: Some(Building {
            annexes: vec![object_id(11), object_id(12)],
            cost: 275.0,
            ..Building::default()
        }),
        ..house()
    };
    main.armored.as_mut().unwrap().armors = vec![ClassAmount::new(4, 1)];
    let annex = |id| Victim {
        id,
        hp: HitPoints::new(2400.0),
        building: Some(Building {
            main: Some(object_id(10)),
            ..Building::default()
        }),
        ..house()
    };
    let (mut a, mut other) = (annex(object_id(11)), annex(object_id(12)));
    take(
        &mut a,
        &mut villager,
        &mut [&mut main, &mut other],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(
        (main.hp, a.hp, other.hp),
        (
            HitPoints::new(2398.0),
            HitPoints::new(2398.0),
            HitPoints::new(2398.0)
        )
    );
    assert!(main.building.as_ref().unwrap().struck);
    assert!(!a.building.as_ref().unwrap().struck);
    assert_eq!(main.damaged, [(Some(object_id(10029)), [10029, 2398, 550])]);
    assert_eq!(a.damaged, []);
    let mut alone = annex(object_id(11));
    assert_eq!(
        take(
            &mut alone,
            &mut villager,
            &mut [],
            &[ClassAmount::new(4, 3)],
            1.0,
            HitPoints::ZERO
        ),
        Err(crate::construction::build::NOT_HELD)
    );
}

/// A player's building left below 1 hit point (`COMISS 1, hp` / `JBE` past) is razed: a
/// castle (82) adds event 7 and a wonder (276) event 5 to the attacker's player's history;
/// then, but for an annex or a wall (class 27), the razing counts (attacker's player 43,
/// 170; owner 152, 155; 350 + owner; owner 375 + attacker's player; 425 + owner) and 172
/// and 153 with the paid cost. Gaia's buildings count nothing.
#[test]
fn a_player_s_building_below_one_hit_point_is_razed() {
    let mut villager = hunter();
    let mut castle = Victim {
        hp: HitPoints::new(1.5),
        master_id: ids::CASTLE,
        ..house()
    };
    take(
        &mut castle,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(castle.hp, HitPoints::ZERO);
    assert!(castle.killed.is_some());
    let razed = &castle.building.as_ref().unwrap().razed;
    assert_eq!(
        razed,
        &[Razed {
            attacker_player: PlayerId::new(1),
            owner: PlayerId::new(2),
            event: Some(crate::players::history::EventKind::CASTLE_DESTROYED),
            adds: vec![
                (PlayerId::new(1), attribute::RAZINGS, 1.0),
                (PlayerId::new(1), attribute::KILLS_VALUE, 25.0),
                (PlayerId::new(2), attribute::KILLS_VALUE_OTHER, 25.0),
                (PlayerId::new(2), attribute::RAZINGS_OTHER, 1.0),
                (PlayerId::new(1), AttributeId::numbered(352), 1.0),
                (PlayerId::new(2), AttributeId::numbered(376), 1.0),
                (PlayerId::new(1), AttributeId::numbered(427), 25.0),
                (PlayerId::new(1), attribute::RAZINGS_VALUE, 25.0),
                (PlayerId::new(2), attribute::RAZINGS_VALUE_OTHER, 25.0),
            ],
        }]
    );
    let mut wonder = Victim {
        hp: HitPoints::new(1.5),
        master_id: ids::WONDER,
        class: class::WALL,
        ..house()
    };
    take(
        &mut wonder,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    let razed = &wonder.building.as_ref().unwrap().razed;
    assert_eq!(
        (razed[0].event, razed[0].adds.len()),
        (
            Some(crate::players::history::EventKind::WONDER_DESTROYED),
            0
        )
    );
    let mut gaia = Victim {
        hp: HitPoints::new(1.5),
        player: PlayerId::new(0),
        ..house()
    };
    take(
        &mut gaia,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!(gaia.building.as_ref().unwrap().razed, Vec::new());
}

/// A trebuchet (`FUN_140fde380`) counts as a unit's kill (attacker's player 20, 170; owner
/// 154, 152; 300 + owner; owner 325 + attacker's player; 400 + owner) unless its kill is
/// counted (`+0x954`: its damage's kill ran slot 0x8b0); 172 and 153 either way.
#[test]
fn a_trebuchet_s_razing_counts_a_kill_unless_its_damage_counted_it() {
    let trebuchet = |hp| Victim {
        hp,
        class: class::CLASS_54,
        master_id: ids::ID_42,
        building: Some(Building {
            trebuchet: true,
            cost: 400.0,
            ..Building::default()
        }),
        ..house()
    };
    let mut villager = hunter();
    villager.source.mode_680 = 1;
    // no damage at 1, the least takes it below 1 but its damage did not kill it
    let mut t = trebuchet(HitPoints::new(1.5));
    t.armored.as_mut().unwrap().armors = vec![ClassAmount::new(4, 10)];
    take(
        &mut t,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert_eq!((t.hp, t.killed), (HitPoints::new(0.5), None));
    assert_eq!(
        t.building.as_ref().unwrap().razed[0].adds,
        vec![
            (PlayerId::new(1), attribute::KILLS, 1.0),
            (PlayerId::new(1), attribute::KILLS_VALUE, 400.0),
            (PlayerId::new(2), attribute::KILLS_OTHER, 1.0),
            (PlayerId::new(2), attribute::KILLS_VALUE_OTHER, 400.0),
            (PlayerId::new(1), AttributeId::numbered(302), 1.0),
            (PlayerId::new(2), AttributeId::numbered(326), 1.0),
            (PlayerId::new(1), AttributeId::numbered(402), 400.0),
            (PlayerId::new(1), attribute::RAZINGS_VALUE, 400.0),
            (PlayerId::new(2), attribute::RAZINGS_VALUE_OTHER, 400.0),
        ]
    );
    let mut t = trebuchet(HitPoints::new(2.0));
    take(
        &mut t,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    assert!(t.killed.is_some());
    assert_eq!(
        t.building.as_ref().unwrap().razed[0].adds,
        vec![
            (PlayerId::new(1), attribute::RAZINGS_VALUE, 400.0),
            (PlayerId::new(2), attribute::RAZINGS_VALUE_OTHER, 400.0)
        ]
    );
}

/// With the flag, an attacker of type 50 and up leaves a building of 1 hit point or more (`COMISS
/// hp, 1` / `JC` past) to its combat ability's effects (`FUN_14107bdf0`): only
/// [`crate::objects::masters::CombatAbility::HIT_EFFECTS`] against a player not allied both ways
/// does anything, which is not modelled. The kill bonus (`FUN_1415c6510`) of a razing: only an
/// attacker of type 40 and up with a task of action type 0x9a does anything, which is not modelled.
#[test]
fn a_building_s_attacker_effects_and_kill_bonus_are_the_game_s() {
    let mut villager = hunter();
    villager.source.ability = CombatAbility::HIT_EFFECTS;
    let mut h = house();
    assert!(
        take(
            &mut h,
            &mut villager,
            &mut [],
            &[ClassAmount::new(4, 3)],
            1.0,
            HitPoints::ZERO
        )
        .is_err()
    );
    let mut h = Victim {
        allied: Allied::new(true, true),
        ..house()
    };
    take(
        &mut h,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    let mut h = Victim {
        hp: HitPoints::new(2.0),
        ..house()
    };
    take(
        &mut h,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
    let mut villager = hunter();
    villager.source.kill_bonus_tasks = true;
    let mut h = Victim {
        hp: HitPoints::new(2.0),
        ..house()
    };
    assert!(
        take(
            &mut h,
            &mut villager,
            &mut [],
            &[ClassAmount::new(4, 3)],
            1.0,
            HitPoints::ZERO
        )
        .is_err()
    );
    let mut h = Victim {
        hp: HitPoints::new(2.0),
        class: class::WALL,
        ..house()
    };
    take(
        &mut h,
        &mut villager,
        &mut [],
        &[ClassAmount::new(4, 3)],
        1.0,
        HitPoints::ZERO,
    )
    .unwrap();
}

/// `FUN_14107f690` on buildings: every building on its tiles starts it with nothing taken
/// (`+0xb90`); an annex reads its main building's `area_damage` (`+0x834`), blast level
/// (`+0x62b`) and blast so far, which its loss adds to; a building's hit is less what the
/// blast has done to it (`FUN_141082e80`'s `param_8`). Here the annex (area damage 0: a
/// least hit of 1 on the main building) comes first in the walk; the main building's own hit
/// of 5 then takes 4.
#[test]
fn a_blast_takes_a_main_building_less_what_its_annexes_took() {
    let mut mangonel = hunter();
    mangonel.source.area_effect = Distance::new(2.0);
    mangonel.source.blast_level = BlastLevel::from_bits_retain(2);
    mangonel.source.area_damage = crate::objects::masters::DamageShare::new(0.5);
    mangonel.weapons = vec![ClassAmount::new(4, 10)];
    mangonel.position = Position::new(10.5, 10.5, 0.0);
    let center = mangonel.position;
    let mut main = Victim {
        id: object_id(20),
        hp: HitPoints::new(100.0),
        position: Position::new(11.5, 10.5, 0.0),
        radius: Footprint::new(1.0, 1.0),
        blast_defense: BlastDefense::new(2),
        building: Some(Building {
            annexes: vec![object_id(21)],
            blast_taken: HitPoints::new(7.0),
            ..Building::default()
        }),
        ..house()
    };
    main.source.area_damage = crate::objects::masters::DamageShare::new(0.0);
    let mut annex = Victim {
        id: object_id(21),
        hp: HitPoints::new(100.0),
        position: Position::new(10.5, 10.5, 0.0),
        radius: Footprint::new(0.5, 0.5),
        blast_defense: BlastDefense::new(0),
        building: Some(Building {
            main: Some(object_id(20)),
            ..Building::default()
        }),
        ..house()
    };
    let mut all = [&mut main, &mut annex];
    assert_eq!(
        blast(&mut mangonel, center, None, None, &mut all, 1.0),
        Ok(true)
    );
    assert_eq!(all.map(|t| t.id), [object_id(21), object_id(20)]);
    assert_eq!(
        (main.hp, annex.hp),
        (HitPoints::new(95.0), HitPoints::new(95.0))
    );
    assert_eq!(
        main.building.as_ref().unwrap().blast_taken,
        HitPoints::new(5.0)
    );
    // each hit posts its damage notify to the main building's unit AI: the annex's
    // forwarded one, then its own (426048706's gate 7228: 271, then -58)
    let hp_after: Vec<i32> = main.damaged.iter().map(|&(_, p)| p[1]).collect();
    assert_eq!(hp_after, [99, 95]);
    assert_eq!(annex.damaged, []);
}

/// `FUN_14107f690` walks the rows (y outer, x inner) and a tile's objects in its list's
/// order (`+0x28`), not by id: the blast's targets end in that order.
#[test]
fn a_blast_walks_a_tile_s_objects_in_its_list_order() {
    let mut elephant = hunter();
    elephant.source.area_effect = Distance::new(1.0);
    elephant.source.blast_level = BlastLevel::from_bits_retain(2);
    elephant.source.area_damage = crate::objects::masters::DamageShare::new(0.5);
    elephant.source.player = PlayerId::new(1);
    elephant.weapons = vec![ClassAmount::new(4, 10)];
    let enemy = |id: i32, x: f32, y: f32, tile_place| Victim {
        id: object_id(id),
        tile_place,
        player: PlayerId::new(2),
        position: Position::new(x, y, 0.0),
        blast_defense: BlastDefense::new(2),
        ..sheep()
    };
    let (mut a, mut b, mut c) = (
        enemy(3, 10.6, 11.0, 1),
        enemy(7, 10.8, 11.2, 0),
        enemy(5, 11.2, 10.4, 0),
    );
    let center = elephant.position;
    let mut all = [&mut a, &mut b, &mut c];
    assert_eq!(
        blast(&mut elephant, center, None, None, &mut all, 1.0),
        Ok(true)
    );
    assert_eq!(
        all.map(|t| t.id),
        [object_id(5), object_id(7), object_id(3)]
    );
}

/// A hit point powerup that took `taken` of its most `max`.
const fn pu(taken: f32, max: f32) -> HpPowerup {
    HpPowerup {
        taken: HitPoints::new(taken),
        max: HitPoints::new(max),
    }
}

/// `FUN_141203be0`: hit points gain up to the master's plus the powerup room; the overflow
/// comes off the powerup, never below 0; a powerup above its room first takes the room; at
/// its most a gain only takes off the powerup.
#[test]
fn a_gain_fills_hit_points_and_its_overflow_takes_off_the_powerup() {
    let mut hp = HitPoints::new(50.0);
    let mut ups = pu(0.0, 0.0);
    gain_hp(
        &mut hp,
        &mut ups,
        MasterHitPoints::of_data(60),
        HitPoints::new(5.0),
    );
    assert_eq!((hp, ups), (HitPoints::new(55.0), pu(0.0, 0.0)));
    gain_hp(
        &mut hp,
        &mut ups,
        MasterHitPoints::of_data(60),
        HitPoints::new(10.0),
    );
    assert_eq!((hp, ups), (HitPoints::new(60.0), pu(0.0, 0.0)));
    let mut hp = HitPoints::new(65.0);
    let mut ups = pu(8.0, 10.0);
    gain_hp(
        &mut hp,
        &mut ups,
        MasterHitPoints::of_data(60),
        HitPoints::new(10.0),
    );
    assert_eq!((hp, ups), (HitPoints::new(70.0), pu(3.0, 10.0)));
    gain_hp(
        &mut hp,
        &mut ups,
        MasterHitPoints::of_data(60),
        HitPoints::new(5.0),
    );
    assert_eq!((hp, ups), (HitPoints::new(70.0), pu(0.0, 10.0)));
    let mut ups = pu(12.0, 10.0);
    gain_hp(
        &mut hp,
        &mut ups,
        MasterHitPoints::of_data(60),
        HitPoints::new(0.0),
    );
    assert_eq!(ups, pu(10.0, 10.0));
}

/// `FUN_141193b30`: a loss leaves at least 1 hit point; full hit points (room less powerup
/// plus hit points at least the most) become exactly the master's and answer true; a NaN is
/// not full.
#[test]
fn a_heal_to_full_holds_the_master_s_hit_points() {
    let mut hp = HitPoints::new(30.0);
    let mut ups = HpPowerup::default();
    assert!(!heal_hp(
        &mut hp,
        &mut ups,
        MasterHitPoints::of_data(60),
        HitPoints::new(10.0)
    ));
    assert_eq!(hp, HitPoints::new(40.0));
    assert!(heal_hp(
        &mut hp,
        &mut ups,
        MasterHitPoints::of_data(60),
        HitPoints::new(25.0)
    ));
    assert_eq!(hp, HitPoints::new(60.0));
    let mut hp = HitPoints::new(0.5);
    assert!(!heal_hp(
        &mut hp,
        &mut ups,
        MasterHitPoints::of_data(60),
        HitPoints::new(-3.0)
    ));
    assert_eq!(hp, HitPoints::new(1.0));
    let mut hp = HitPoints::new(55.0);
    let mut ups = pu(5.0, 5.0);
    assert!(!heal_hp(
        &mut hp,
        &mut ups,
        MasterHitPoints::of_data(60),
        HitPoints::new(4.0)
    ));
    assert_eq!(hp, HitPoints::new(59.0));
    let mut hp = HitPoints::new(f32::NAN);
    assert!(!heal_hp(
        &mut hp,
        &mut HpPowerup::default(),
        MasterHitPoints::of_data(60),
        HitPoints::new(1.0)
    ));
}

/// The blast's ledger (`+0xbb8`): an annex keeps its damage on its main building among the
/// targets, another building on itself, any other object on none; an annex whose main
/// building is not among them fails. The values read through it: an annex its main building's
/// share, defense and damage so far, a building the attacker's share and its own.
#[test]
fn a_blast_keeps_a_building_s_damage_on_it_or_its_main_building() {
    use crate::combat::damage::blasts::{blast_ledger, ledger_values};
    let mut main = house();
    main.source.area_damage = crate::objects::masters::DamageShare::new(0.25);
    main.blast_defense = BlastDefense::new(3);
    main.building.as_mut().unwrap().blast_taken = HitPoints::new(7.0);
    let mut annex = Victim {
        id: object_id(501),
        building: Some(Building {
            main: Some(object_id(500)),
            blast_taken: HitPoints::new(2.0),
            ..Building::default()
        }),
        ..house()
    };
    let mut lamb = sheep();
    let all = [&mut main, &mut annex, &mut lamb];
    assert_eq!(blast_ledger(&all, 0), Ok(Some(0)));
    assert_eq!(blast_ledger(&all, 1), Ok(Some(0)));
    assert_eq!(blast_ledger(&all, 2), Ok(None));
    let a = Attacker {
        area_damage: crate::objects::masters::DamageShare::new(0.5),
        ..Attacker::default()
    };
    assert_eq!(
        ledger_values(&all, 1, Some(0), &a),
        (0.25, BlastDefense::new(3), HitPoints::new(7.0))
    );
    assert_eq!(
        ledger_values(&all, 0, Some(0), &a),
        (0.5, BlastDefense::new(3), HitPoints::new(7.0))
    );
    assert_eq!(
        ledger_values(&all, 2, None, &a),
        (0.5, BlastDefense::new(0), HitPoints::ZERO)
    );
    let mut orphan = Victim {
        building: Some(Building {
            main: Some(object_id(999)),
            ..Building::default()
        }),
        ..house()
    };
    assert!(blast_ledger(&[&mut orphan], 0).is_err());
}

/// The square distance to an object's box: 0 inside it, the gaps past its radii beyond; none
/// at or past the blast's radius; a NaN radius gives no gap on its axis.
#[test]
fn a_blast_measures_to_an_object_s_box_within_its_radius() {
    use crate::combat::damage::blasts::blast_distance2;
    let at = |x: f32, y: f32, radius| Victim {
        position: Position::new(x, y, 0.0),
        radius,
        ..sheep()
    };
    let center = Position::new(10.0, 10.0, 0.0);
    assert_eq!(
        blast_distance2(&at(10.2, 10.0, Footprint::new(0.5, 0.5)), center, 1.0),
        Some(0.0)
    );
    assert_eq!(
        blast_distance2(&at(11.0, 10.0, Footprint::new(0.5, 0.5)), center, 1.0),
        Some(0.25)
    );
    assert_eq!(
        blast_distance2(&at(11.5, 10.0, Footprint::new(0.5, 0.5)), center, 1.0),
        None
    );
    assert_eq!(
        blast_distance2(&at(12.0, 10.5, Footprint::new(f32::NAN, 0.25)), center, 1.0),
        Some(0.0625)
    );
}

/// The blast's factor: an ally's `friendly_fire_damage` scales it; a melee blast falls off
/// with `FALLS_OFF` only and not on `main`; a missile's falls off everywhere, but at
/// distance 0 on a `main` building.
#[test]
fn a_blast_s_factor_takes_the_friendly_fire_and_the_falloff() {
    use crate::combat::damage::blasts::blast_falloff;
    let attacker = hunter();
    let melee = Attacker {
        blast_level: BlastLevel::from_bits_retain(2),
        ..attacker.source
    };
    let mut enemy = Victim {
        player: PlayerId::new(2),
        ..sheep()
    };
    let d2 = 0.25;
    assert_eq!(
        blast_falloff(&attacker, &melee, &enemy, (1.0, d2), false),
        1.0
    );
    let falling = Attacker {
        blast_level: BlastLevel::from_bits_retain(0x42),
        ..melee
    };
    assert_eq!(
        blast_falloff(&attacker, &falling, &enemy, (1.0, d2), false),
        0.5
    );
    assert_eq!(
        blast_falloff(&attacker, &falling, &enemy, (1.0, d2), true),
        1.0
    );
    let missile = Attacker {
        missile: Some(master_id(5)),
        ..melee
    };
    assert_eq!(
        blast_falloff(&attacker, &missile, &enemy, (1.0, d2), true),
        0.5
    );
    let mut main = Victim {
        player: PlayerId::new(2),
        ..house()
    };
    assert_eq!(
        blast_falloff(&attacker, &missile, &main, (1.0, d2), true),
        1.0
    );
    main.kind = BUILDING;
    assert_eq!(
        blast_falloff(&attacker, &missile, &main, (1.0, d2), false),
        0.5
    );
    enemy.player = PlayerId::new(1);
    enemy.armored.as_mut().unwrap().friendly_fire = crate::objects::masters::DamageShare::new(0.25);
    assert_eq!(
        blast_falloff(&attacker, &melee, &enemy, (1.0, d2), false),
        0.25
    );
}

/// A cone blast (`CONE`) leaves out what lies past half a facet of the attacker's facing; a
/// wide one (`WIDE_CONE`) what lies past its radius or 1.67 facets; a blast of no cone leaves out
/// nothing; a cone blast of an attacker without a facing fails.
#[test]
fn a_cone_blast_leaves_out_what_lies_beside_its_facing() {
    use crate::combat::damage::blasts::outside_cone;
    let mut attacker = hunter();
    attacker.position = Position::new(10.0, 10.0, 0.0);
    let at = |x: f32, y: f32| Victim {
        position: Position::new(x, y, 0.0),
        ..sheep()
    };
    let (front, side, behind) = (at(11.0, 10.0), at(10.7, 10.7), at(9.0, 10.0));
    let level = |blast_level| Attacker {
        blast_level,
        ..attacker.source
    };
    assert_eq!(
        outside_cone(
            &attacker,
            &level(BlastLevel::from_bits_retain(2)),
            &behind,
            (1.0, 0.0)
        ),
        Ok(false)
    );
    assert!(
        outside_cone(
            &attacker,
            &level(BlastLevel::from_bits_retain(0x82)),
            &front,
            (1.0, 0.0)
        )
        .is_err()
    );
    attacker.armored.as_mut().unwrap().facing = Some(Facing {
        angle: Angle::ZERO,
        facets: 8,
    });
    let narrow = level(BlastLevel::from_bits_retain(0x82));
    assert_eq!(
        outside_cone(&attacker, &narrow, &front, (1.0, 0.0)),
        Ok(false)
    );
    assert_eq!(
        outside_cone(&attacker, &narrow, &side, (1.0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        outside_cone(&attacker, &narrow, &behind, (1.0, 0.0)),
        Ok(true)
    );
    let wide = level(BlastLevel::from_bits_retain(0xa2));
    assert_eq!(outside_cone(&attacker, &wide, &side, (1.0, 2.0)), Ok(false));
    assert_eq!(outside_cone(&attacker, &wide, &side, (1.0, 0.5)), Ok(true));
    assert_eq!(
        outside_cone(&attacker, &wide, &behind, (1.0, 2.0)),
        Ok(true)
    );
}

/// A fixed blast damage takes the share's size alone; below 1 hit point the object has 0
/// and holds nothing, a player's combat unit that had 1 counted killed by the attacker,
/// Gaia's not; an attacker with a kill bonus task fails there.
#[test]
fn a_fixed_blast_damage_takes_its_size_and_kills_below_one() {
    use crate::combat::damage::blasts::fixed_blast;
    let attacker = hunter();
    let mut t = Victim {
        player: PlayerId::new(2),
        amount: Amount::new(5.0),
        ..sheep()
    };
    fixed_blast(&attacker, &attacker.source, &mut t, -5.0).unwrap();
    assert_eq!((t.hp, t.killed), (HitPoints::new(9.0), None));
    fixed_blast(&attacker, &attacker.source, &mut t, -20.0).unwrap();
    assert_eq!((t.hp, t.amount), (HitPoints::ZERO, Amount::ZERO));
    assert_eq!(
        t.killed.map(|k| (k.attacker, k.attacker_player)),
        Some((object_id(10029), PlayerId::new(1)))
    );
    let mut gaia = Victim {
        player: PlayerId::new(0),
        ..sheep()
    };
    fixed_blast(&attacker, &attacker.source, &mut gaia, -20.0).unwrap();
    assert_eq!((gaia.hp, gaia.killed), (HitPoints::ZERO, None));
    let bonus = Attacker {
        kill_bonus_tasks: true,
        ..attacker.source
    };
    let mut t = Victim {
        player: PlayerId::new(2),
        ..sheep()
    };
    assert!(fixed_blast(&attacker, &bonus, &mut t, -20.0).is_err());
}

/// The blast's hit takes the attacker's weapons at the factor; a building's loss goes onto
/// its ledger. An annex passes its hit on to its main building, and its own loss (none) onto
/// the main building's ledger.
#[test]
fn a_blast_s_hit_adds_a_building_s_loss_to_its_ledger() {
    use crate::combat::damage::blasts::blast_hit;
    let mut attacker = hunter();
    let weapons = [ClassAmount::new(4, 10)];
    let mut enemy = Victim {
        player: PlayerId::new(2),
        ..sheep()
    };
    blast_hit(
        &mut attacker,
        &mut [&mut enemy],
        0,
        (&weapons, 0.5),
        (None, HitPoints::ZERO),
    )
    .unwrap();
    assert_eq!(enemy.hp, HitPoints::new(9.0));
    let mut main = house();
    let mut annex = Victim {
        id: object_id(501),
        building: Some(Building {
            main: Some(object_id(500)),
            ..Building::default()
        }),
        ..house()
    };
    let mut all = [&mut main, &mut annex];
    blast_hit(
        &mut attacker,
        &mut all,
        0,
        (&weapons, 1.0),
        (Some(0), HitPoints::ZERO),
    )
    .unwrap();
    let lost = 550.0 - all[0].hp.get();
    assert!(0.0 < lost);
    assert_eq!(
        all[0].building.as_ref().map(|b| b.blast_taken),
        Some(HitPoints::new(lost))
    );
    blast_hit(
        &mut attacker,
        &mut all,
        1,
        (&weapons, 1.0),
        (Some(0), HitPoints::ZERO),
    )
    .unwrap();
    assert_eq!(all[1].hp, HitPoints::new(550.0));
    assert!(all[0].hp < HitPoints::new(550.0 - lost));
    assert_eq!(
        all[0].building.as_ref().map(|b| b.blast_taken),
        Some(HitPoints::new(lost))
    );
}

/// One object under a melee blast (radius 1, level 2, half the damage): an enemy in reach
/// takes the hit; one off the tiles, a spared friend, the object left out, the attacker, an
/// artifact, one defending the blast's level and a ram's non-building are passed; an object
/// whose player's update is not modelled fails.
#[test]
fn a_blast_passes_over_what_it_spares_and_hits_the_rest() {
    use crate::combat::damage::blasts::{Blast, blast_one, blast_tiles};
    let mut elephant = hunter();
    elephant.source.area_effect = Distance::new(1.0);
    elephant.source.blast_level = BlastLevel::from_bits_retain(2);
    elephant.source.area_damage = crate::objects::masters::DamageShare::new(0.5);
    let center = elephant.position;
    let blast = |a: Attacker| Blast {
        a,
        center,
        r: Distance::new(1.0),
        wide: Distance::ZERO,
        tiles: blast_tiles(center, Distance::new(1.0)),
        spares_friends: true,
        exclude: Some(object_id(9)),
        main: None,
        factor: 1.0,
    };
    let weapons = [ClassAmount::new(4, 10)];
    let enemy = Victim {
        id: object_id(2),
        player: PlayerId::new(2),
        position: Position::new(10.8, 11.0, 0.0),
        blast_defense: BlastDefense::new(2),
        ..sheep()
    };
    let b = blast(elephant.source);
    let mut hit = |b: &Blast, t: Victim| {
        let mut t = t;
        let r = blast_one(&mut elephant, &mut [&mut t], 0, (b, &weapons));
        r.map(|()| t.hp)
    };
    assert_eq!(hit(&b, enemy.clone()), Ok(HitPoints::new(9.0)));
    for passed in [
        Victim {
            position: Position::new(30.0, 30.0, 0.0),
            ..enemy.clone()
        },
        Victim {
            allied: Allied::new(true, true),
            ..enemy.clone()
        },
        Victim {
            id: object_id(9),
            ..enemy.clone()
        },
        Victim {
            id: object_id(10029),
            ..enemy.clone()
        },
        Victim {
            class: class::ARTIFACT,
            ..enemy.clone()
        },
        Victim {
            blast_defense: BlastDefense::new(1),
            ..enemy.clone()
        },
    ] {
        assert_eq!(hit(&b, passed), Ok(HitPoints::new(14.0)));
    }
    let ram = blast(Attacker {
        master_id: RAMS[0],
        ..b.a
    });
    assert_eq!(hit(&ram, enemy.clone()), Ok(HitPoints::new(14.0)));
    let unsure = Victim {
        uncertain: true,
        ..enemy
    };
    assert_eq!(
        hit(&b, unsure).unwrap_err().reason(),
        "a blast over an object whose player's update is not modelled"
    );
}
