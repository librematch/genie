//! A hit's victim that takes another master in the hit: what it keeps and what it takes.

use crate::combat::AttackBonus;
use crate::combat::damage::{ArmorBonuses, Armored, Attacker, Victim};
use crate::map::Distance;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectState;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::DamageClass;
use crate::objects::masters::MasterKind;
use crate::test_ids::master_id;
use crate::testing::class;

/// A combat victim of master `id` with `hp` of its master's `master_hp` hit points, a melee
/// weapon of `attack`, a base armor of `armor`, the attack powerup 2, the armor powerups (1, 2),
/// and a hit point powerup that took 3 of 5.
fn armed(id: i16, hp: f32, master_hp: i16, attack: i16, armor: i16) -> Victim {
    Victim {
        hp: HitPoints::new(hp),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        master_id: master_id(id),
        master_hp: MasterHitPoints::of_data(master_hp),
        weapons: vec![ClassAmount {
            class: DamageClass::MELEE,
            amount: DamageAmount::new(attack),
        }],
        source: Attacker {
            master_id: master_id(id),
            weapon_range: Distance::new(f32::from(attack)),
            attack_powerup: AttackBonus::new(2.0),
            ..Attacker::default()
        },
        armored: Some(Armored {
            base_armor: DamageAmount::new(armor),
            armor_powerups: ArmorBonuses::new(1, 2),
            hp_powerup: HpPowerup {
                taken: HitPoints::new(3.0),
                max: HitPoints::new(5.0),
            },
            busy: true,
            ..Armored::default()
        }),
        ..Victim::default()
    }
}

/// Below state 3 the victim keeps its share of the new master's hit points (whole) and takes the
/// new master's values; its own values stay. Its attack, armor and most hit points powerups go,
/// what its hit point powerup took stays.
#[test]
fn victim_takes_new_master_values_and_loses_its_powerups() {
    let mut victim = armed(1974, 40.5, 70, 6, 1);
    let reads = Victim {
        hp: HitPoints::new(80.0),
        ..armed(1976, 80.0, 80, 9, 3)
    };
    victim.take_master(&reads);
    // 40.5 / 70 * 80 = 46.29, whole
    assert_eq!(victim.hp, HitPoints::new(46.0));
    assert_eq!(
        (victim.master_id, victim.master_hp, victim.source.master_id),
        (
            master_id(1976),
            MasterHitPoints::of_data(80),
            master_id(1976)
        )
    );
    assert_eq!(victim.weapons, reads.weapons);
    assert_eq!(victim.source.weapon_range, Distance::new(9.0));
    assert_eq!(victim.source.attack_powerup, AttackBonus::ZERO);
    let armored = victim.armored.as_ref().unwrap();
    assert_eq!(armored.base_armor, DamageAmount::new(3));
    assert_eq!(armored.armor_powerups, ArmorBonuses::ZERO);
    assert_eq!(
        armored.hp_powerup,
        HpPowerup {
            taken: HitPoints::new(3.0),
            max: HitPoints::ZERO,
        }
    );
    assert!(armored.busy, "the object's own value");
}

/// From state 3 on the victim keeps its master and hit points; only its powerups go.
#[test]
fn dying_victim_keeps_its_master_and_loses_its_powerups() {
    let mut victim = Victim {
        state: ObjectState::Dying,
        ..armed(1974, 40.5, 70, 6, 1)
    };
    victim.take_master(&armed(1976, 80.0, 80, 9, 3));
    assert_eq!(
        (victim.master_id, victim.hp),
        (master_id(1974), HitPoints::new(40.5))
    );
    assert_eq!(victim.source.attack_powerup, AttackBonus::ZERO);
    assert_eq!(
        victim
            .armored
            .as_ref()
            .map(|armored| armored.armor_powerups),
        Some(ArmorBonuses::ZERO)
    );
}

/// A victim without an armor or a building part takes the new master's; one whose new master
/// has none loses its own.
#[test]
fn victim_parts_follow_the_new_master() {
    let mut bare = Victim {
        armored: None,
        ..armed(1974, 40.0, 70, 6, 1)
    };
    let reads = armed(1976, 70.0, 70, 9, 3);
    bare.take_master(&reads);
    assert_eq!(
        bare.armored.as_ref().map(|armored| armored.base_armor),
        Some(DamageAmount::new(3))
    );
    let mut armored = armed(1974, 40.0, 70, 6, 1);
    armored.take_master(&Victim {
        armored: None,
        ..reads
    });
    assert_eq!(armored.armored, None);
}

/// A victim without hit point transforms keeps its master in the hit.
#[test]
fn victim_without_transforms_keeps_its_master() {
    let mut victim = armed(1974, 40.0, 70, 6, 1);
    victim.transform_in_hit();
    assert_eq!(
        (victim.master_id, victim.master_changes.len()),
        (master_id(1974), 0)
    );
}
