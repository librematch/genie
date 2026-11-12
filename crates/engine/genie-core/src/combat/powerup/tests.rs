//! An object's powerups: an aura's powerup and its hold, the reset, the attributes it raises and
//! a siege unit's infantry count.

use crate::combat::AttackBonus;
use crate::combat::attack::AttackState;
use crate::combat::attack::tests::own_attack;
use crate::combat::damage::ArmorBonuses;
use crate::combat::damage::BONUS_DAMAGE;
use crate::combat::powerup::{
    AppliedPowerup, ConversionPowerup, InfantryChange, InfantryCount, Powerup, PowerupAmount,
    PowerupAttribute, PowerupShare, armor_of_class, count_infantry, powerup_amount, powerup_until,
    reset_powerup_values, speed_of_infantry,
};
use crate::map::Distance;
use crate::map::GridSize;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::Regeneration;
use crate::objects::WorkRate;
use crate::objects::actions::ActionList;
use crate::objects::layers::Action;
use crate::objects::layers::Combat;
use crate::objects::masters::ActionPart;
use crate::objects::masters::AnimatedPart;
use crate::objects::masters::ClassAmount;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::DamageClass;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Task;
use crate::objects::masters::WorkValue;
use crate::players::attribute::AttributeId;
use crate::testing::class;
use crate::testing::scene::{PLAYER, Scene, SceneObject};
use crate::testing::tuning;
use crate::vision::VisionBits;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;

/// The reset clears the attack, reload, work rate, regeneration, most hit points, range, armor
/// and conversion powerups and the map of the powerups held; the hit points taken (`+0xa38`)
/// and the stingers stay, and the speed powerup is its infantry's, 0.05 a unit.
#[test]
fn reset_clears_the_powerups_but_the_hit_points_taken_and_the_stingers() {
    let mut combat = Combat {
        attack_powerup: AttackBonus::new(2.0),
        reload_powerup: Seconds::new(0.5),
        reload_stinger: Seconds::new(0.25),
        work_rate_powerup: WorkRate::new(0.15),
        regen_powerup: Regeneration::new(5.0),
        regen_stinger: Regeneration::new(1.0),
        hp_powerup: HpPowerup {
            taken: HitPoints::new(10.0),
            max: HitPoints::new(20.0),
        },
        range_powerup: Distance::new(1.0),
        armor_powerups: ArmorBonuses::new(1, 2),
        speed_powerup: Speed::new(0.7),
        conversion_powerup: ConversionPowerup::new(0.3),
        applied_powerups: vec![AppliedPowerup::default()],
        infantry_inside: InfantryCount::ZERO
            .changed(InfantryChange::Entered)
            .changed(InfantryChange::Entered)
            .changed(InfantryChange::Entered),
        ..Combat::default()
    };
    reset_powerup_values(&mut combat, tuning().powerup);
    assert_eq!(
        (
            combat.attack_powerup,
            combat.reload_powerup,
            combat.work_rate_powerup,
            combat.regen_powerup,
            combat.range_powerup,
            combat.armor_powerups,
            combat.conversion_powerup,
        ),
        (
            AttackBonus::ZERO,
            Seconds::ZERO,
            WorkRate::ZERO,
            Regeneration::ZERO,
            Distance::ZERO,
            ArmorBonuses::ZERO,
            ConversionPowerup::ZERO,
        )
    );
    assert_eq!(
        combat.hp_powerup,
        HpPowerup {
            taken: HitPoints::new(10.0),
            max: HitPoints::ZERO
        }
    );
    assert_eq!(
        (combat.reload_stinger, combat.regen_stinger),
        (Seconds::new(0.25), Regeneration::new(1.0))
    );
    assert_eq!(combat.applied_powerups, []);
    assert_eq!(
        combat.speed_powerup.get().to_bits(),
        (3.0_f32 * tuning().powerup.speed_per_infantry).to_bits()
    );
}

/// The count widens to 64 bits with its sign and converts as unsigned: −1 is 2^64 − 1.
#[test]
fn negative_infantry_count_speeds_up_by_its_unsigned_wide_value() {
    let tuning = tuning().powerup;
    let speed = speed_of_infantry(InfantryCount::ZERO.changed(InfantryChange::Left), tuning);
    assert_eq!(
        speed.get().to_bits(),
        (u64::MAX as f32 * tuning.speed_per_infantry).to_bits()
    );
    assert_eq!(speed_of_infantry(InfantryCount::ZERO, tuning), Speed::ZERO);
}

/// Slot 0x1b0 adds the change to the count and the change times the speed per unit to the speed
/// powerup (`ADDSS` on what it holds, so a leave below zero takes the speed below zero too).
#[test]
fn count_infantry_moves_the_count_and_the_speed_powerup_together() {
    let tuning = tuning().powerup;
    let mut combat = Combat {
        speed_powerup: Speed::new(0.5),
        ..Combat::default()
    };
    count_infantry(&mut combat, InfantryChange::Entered, tuning);
    assert_eq!(combat.infantry_inside.get(), 1);
    assert_eq!(
        combat.speed_powerup.get().to_bits(),
        (tuning.speed_per_infantry + 0.5).to_bits()
    );
    count_infantry(&mut combat, InfantryChange::Left, tuning);
    count_infantry(&mut combat, InfantryChange::Left, tuning);
    assert_eq!(combat.infantry_inside.get(), -1);
    let back = (tuning.speed_per_infantry + 0.5) - tuning.speed_per_infantry;
    assert_eq!(
        combat.speed_powerup.get().to_bits(),
        (back - tuning.speed_per_infantry).to_bits()
    );
}

/// A powerup holds a second plus its draw mod 100 past the world time it is taken at.
#[test]
fn powerup_holds_one_second_and_its_draw_mod_hundred() {
    let now = WorldTime::from_millis(5000);
    assert_eq!(powerup_until(now, 12_345), WorldTime::from_millis(6045));
    assert_eq!(powerup_until(now, 0), WorldTime::from_millis(6000));
}

/// The attribute is the wait time truncated; a NaN is the integer indefinite.
#[test]
fn powerup_attribute_truncates_task_wait_time() {
    assert_eq!(
        PowerupAttribute::of_task(Seconds::new(9.9)),
        PowerupAttribute::ATTACK
    );
    assert_eq!(PowerupAttribute::of_task(Seconds::new(-1.5)).get(), -1);
    assert_eq!(
        PowerupAttribute::of_task(Seconds::new(f32::NAN)).get(),
        i32::MIN
    );
}

/// A master of hit points 50, speed 1.2, reload 2, range 4, work rate 0.5, regeneration 3,
/// conversion chance 0.25, line of sight 6, base armor 1 and a pierce armor of 3.
fn soldier() -> Master {
    Master {
        id: MasterId::named(901),
        kind: MasterKind::COMBAT,
        class: class::VILLAGER,
        hit_points: MasterHitPoints::of_data(50),
        line_of_sight: Distance::new(6.0),
        animated: Some(AnimatedPart {
            max_speed: Speed::new(1.2),
        }),
        action: Some(ActionPart {
            work_rate: WorkRate::new(0.5),
            ..ActionPart::default()
        }),
        combat: Some(CombatPart {
            reload: Seconds::new(2.0),
            weapon_range: Distance::new(4.0),
            base_armor: DamageAmount::new(1),
            armors: vec![ClassAmount {
                class: DamageClass::PIERCE,
                amount: DamageAmount::new(3),
            }],
            ..CombatPart::default()
        }),
        creatable: Some(CreatablePart {
            regeneration: Regeneration::new(3.0),
            ..CreatablePart::default()
        }),
        ..Master::default()
    }
}

/// A master's armor of a class is its first entry of it, else its base armor.
#[test]
fn armor_of_class_is_its_entry_else_the_base() {
    let master = soldier();
    assert_eq!(
        armor_of_class(&master, DamageClass::PIERCE),
        DamageAmount::new(3)
    );
    assert_eq!(
        armor_of_class(&master, DamageClass::MELEE),
        DamageAmount::new(1)
    );
}

/// The powerup of `attribute`, `amount` and `share`.
fn powerup(attribute: PowerupAttribute, amount: f32, share: PowerupShare) -> Powerup {
    Powerup {
        attribute,
        source: MasterId::named(902),
        amount: PowerupAmount::new(amount),
        share,
    }
}

/// A plain amount is itself; a share multiplies the master's value (the speed's: the amount
/// less 1, or the amount itself below 0), the armors' their class's armor; the hit points
/// share of the regeneration is only plain, and an attribute the switch does not name is none.
#[test]
fn powerup_amount_is_itself_or_share_of_masters_value() {
    use PowerupShare::{MasterShare, Plain};
    let master = soldier();
    let amount = |attribute, amount, share| {
        powerup_amount(&master, powerup(attribute, amount, share)).map(PowerupAmount::get)
    };
    assert_eq!(amount(PowerupAttribute::HIT_POINTS, 0.5, Plain), Some(0.5));
    assert_eq!(
        amount(PowerupAttribute::HIT_POINTS, 0.5, MasterShare),
        Some(50.0 * 0.5)
    );
    assert_eq!(
        amount(PowerupAttribute::MOVEMENT_SPEED, 1.25, MasterShare),
        Some((1.25 - 1.0) * 1.2)
    );
    assert_eq!(
        amount(PowerupAttribute::MOVEMENT_SPEED, -0.5, MasterShare),
        Some(1.2 * -0.5)
    );
    assert!(
        amount(PowerupAttribute::MOVEMENT_SPEED, f32::NAN, MasterShare).is_some_and(f32::is_nan)
    );
    assert_eq!(
        amount(PowerupAttribute::ATTACK_RELOAD_TIME, 0.1, MasterShare),
        Some(0.1 * 2.0)
    );
    assert_eq!(
        amount(PowerupAttribute::MAXIMUM_RANGE, 0.5, MasterShare),
        Some(0.5 * 4.0)
    );
    assert_eq!(
        amount(PowerupAttribute::WORK_RATE, 0.2, MasterShare),
        Some(0.2 * 0.5)
    );
    assert_eq!(
        amount(PowerupAttribute::REGENERATION_RATE, 2.0, MasterShare),
        Some(2.0 * 3.0)
    );
    assert_eq!(
        amount(PowerupAttribute::PIERCE_ARMOR, 0.5, MasterShare),
        Some(3.0 * 0.5)
    );
    assert_eq!(amount(PowerupAttribute::MELEE_ARMOR, 2.0, Plain), Some(2.0));
    assert_eq!(
        amount(PowerupAttribute::REGENERATION_HP_PERCENT, 0.1, Plain),
        Some(50.0 * 0.1)
    );
    assert_eq!(
        amount(PowerupAttribute::REGENERATION_HP_PERCENT, 0.1, MasterShare),
        None
    );
    assert_eq!(amount(PowerupAttribute::LINE_OF_SIGHT, 1.0, Plain), None);
}

/// Object 7 of the soldier at (2.5, 2.5), its seer in player 1's sight registry; the world time
/// at 10 s.
fn soldier_world() -> World {
    let mut world = Scene::open(GridSize::new(8, 8))
        .object(SceneObject {
            seen_by: Some(VisionBits::visible(PLAYER)),
            ..SceneObject::new(7, PLAYER, soldier(), Position::new(2.5, 2.5, 0.0))
        })
        .build();
    world.world_time = WorldTime::from_millis(10_000);
    world
}

/// Object 7.
fn seven() -> ObjectId {
    ObjectId::try_from(7).unwrap()
}

/// A powerup goes on with the Object stream's draw for its end and adds its amount; the same
/// powerup again before its end changes nothing and draws nothing.
#[test]
fn held_powerup_waits_for_its_end() {
    let mut world = soldier_world();
    let mut streams = world.streams.clone();
    let draw = streams.get_mut(Stream::Object).rand15();
    let attack = powerup(PowerupAttribute::ATTACK, 2.7, PowerupShare::Plain);
    world.apply_powerup(seven(), attack).unwrap();
    let combat = world.parts.combat_of(seven());
    assert_eq!(combat.attack_powerup, AttackBonus::new(2.0));
    assert_eq!(combat.powerup_update, powerup_until(world.world_time, draw));
    assert_eq!(
        combat.applied_powerups,
        [AppliedPowerup {
            own: MasterId::named(901),
            source: MasterId::named(902),
            attribute: PowerupAttribute::ATTACK,
        }]
    );
    world.apply_powerup(seven(), attack).unwrap();
    assert_eq!(
        world.parts.combat_of(seven()).attack_powerup,
        AttackBonus::new(2.0)
    );
    assert_eq!(world.streams, streams);
}

/// Past its end the same powerup resets every powerup first, then goes on again.
#[test]
fn held_powerup_past_its_end_resets_every_powerup_first() {
    let mut world = soldier_world();
    let attack = powerup(PowerupAttribute::ATTACK, 2.0, PowerupShare::Plain);
    world.apply_powerup(seven(), attack).unwrap();
    let range = powerup(PowerupAttribute::MAXIMUM_RANGE, 1.5, PowerupShare::Plain);
    world.apply_powerup(seven(), range).unwrap();
    world.world_time = world.parts.combat_of(seven()).powerup_update;
    world.apply_powerup(seven(), attack).unwrap();
    let combat = world.parts.combat_of(seven());
    assert_eq!(combat.attack_powerup, AttackBonus::new(2.0));
    assert_eq!(combat.range_powerup, Distance::ZERO);
    assert_eq!(combat.applied_powerups.len(), 1);
}

/// Each attribute adds to its own powerup; the armors add whole numbers.
#[test]
fn powerups_add_to_their_attributes() {
    let mut world = soldier_world();
    let apply = |world: &mut World, attribute, amount| {
        world
            .apply_powerup(seven(), powerup(attribute, amount, PowerupShare::Plain))
            .unwrap();
    };
    apply(&mut world, PowerupAttribute::HIT_POINTS, 10.0);
    apply(&mut world, PowerupAttribute::MOVEMENT_SPEED, 0.25);
    apply(&mut world, PowerupAttribute::ATTACK_RELOAD_TIME, -0.5);
    apply(&mut world, PowerupAttribute::WORK_RATE, 0.1);
    apply(&mut world, PowerupAttribute::REGENERATION_RATE, 4.0);
    apply(&mut world, PowerupAttribute::CONVERSION_CHANCE, 0.2);
    apply(&mut world, PowerupAttribute::MELEE_ARMOR, 2.9);
    apply(&mut world, PowerupAttribute::PIERCE_ARMOR, -1.5);
    let combat = world.parts.combat_of(seven());
    assert_eq!(combat.hp_powerup.max, HitPoints::new(10.0));
    assert_eq!(combat.speed_powerup, Speed::new(0.25));
    assert_eq!(combat.reload_powerup, Seconds::new(-0.5));
    assert_eq!(combat.work_rate_powerup, WorkRate::new(0.1));
    assert_eq!(combat.regen_powerup, Regeneration::new(4.0));
    assert_eq!(combat.conversion_powerup, ConversionPowerup::new(0.2));
    assert_eq!(combat.armor_powerups, ArmorBonuses::new(2, -1));
}

/// The soldier with a melee attack of 6 and, with `bonus`, a bonus damage task against master
/// 902 (3 for each half of its own hit point share).
fn fighter(bonus: bool) -> Master {
    let mut master = soldier();
    if let Some(combat) = master.combat.as_mut() {
        combat.weapons = vec![ClassAmount {
            class: DamageClass::MELEE,
            amount: DamageAmount::new(6),
        }];
    }
    if bonus && let Some(action) = master.action.as_mut() {
        action.tasks = vec![Task {
            action_kind: BONUS_DAMAGE,
            object_id: Some(MasterId::named(902)),
            attribute_kind4: Some(AttributeId::named(0)),
            work_val1: WorkValue::new(3.0),
            work_val2: WorkValue::new(0.5),
            ..Task::default()
        }];
    }
    master
}

/// An attack powerup with the master's share replaces the powerup by the shown attack (slot
/// 0x878: the best attack plus the powerup as a whole number) less that whole number, times the
/// share.
#[test]
fn attack_share_replaces_powerup_by_share_of_shown_attack() {
    let mut world = Scene::open(GridSize::new(8, 8))
        .object(SceneObject::new(
            7,
            PLAYER,
            fighter(false),
            Position::new(2.5, 2.5, 0.0),
        ))
        .build();
    let plain = powerup(PowerupAttribute::ATTACK, 2.7, PowerupShare::Plain);
    world.apply_powerup(seven(), plain).unwrap();
    let share = Powerup {
        source: MasterId::named(903),
        ..powerup(PowerupAttribute::ATTACK, 0.5, PowerupShare::MasterShare)
    };
    world.apply_powerup(seven(), share).unwrap();
    // (6 + 2) − 2, times 0.5
    assert_eq!(
        world.parts.combat_of(seven()).attack_powerup,
        AttackBonus::new(3.0)
    );
}

/// With an Attack on top, the shown attack adds the bonus damage against its target: 3 for each
/// half of the attacker's full hit points. A target of a recyclable id (−8, which the world
/// holds) is no object to `FUN_141085c70`'s read (`FUN_140a78450`) and adds none.
#[test]
fn attack_share_counts_bonus_damage_against_attack_target() {
    let shared = |target_id: i32| {
        let attacking = Action {
            list: ActionList::default().with_attack(Some(own_attack(
                target_id,
                AttackState::New,
                Distance::new(1.0),
            ))),
            ..Action::default()
        };
        let target = Master {
            id: MasterId::named(902),
            ..soldier()
        };
        let mut world = Scene::open(GridSize::new(8, 8))
            .object(SceneObject {
                actions: Some(attacking),
                ..SceneObject::new(7, PLAYER, fighter(true), Position::new(2.5, 2.5, 0.0))
            })
            .object(SceneObject::new(
                target_id,
                PLAYER,
                target,
                Position::new(3.5, 2.5, 0.0),
            ))
            .build();
        let share = powerup(PowerupAttribute::ATTACK, 0.5, PowerupShare::MasterShare);
        world.apply_powerup(seven(), share).unwrap();
        world.parts.combat_of(seven()).attack_powerup
    };
    // (6 + 0 + 2 × 3) − 0, times 0.5
    assert_eq!(shared(8), AttackBonus::new(6.0));
    // (6 + 0) − 0, times 0.5
    assert_eq!(shared(-8), AttackBonus::new(3.0));
}

/// The line of sight's powerup goes through the seer (a share of the master's 6 tiles), and the
/// reset takes it off.
#[test]
fn sight_powerup_raises_the_seers_powerup_until_reset() {
    let mut world = soldier_world();
    let sight = powerup(
        PowerupAttribute::LINE_OF_SIGHT,
        0.5,
        PowerupShare::MasterShare,
    );
    world.apply_powerup(seven(), sight).unwrap();
    let seer = |world: &World| {
        world
            .spatial
            .sight
            .as_ref()
            .and_then(|visibility| visibility.seer(seven()))
            .map(|seer| seer.los_powerup)
    };
    assert_eq!(seer(&world), Some(Distance::new(0.5 * 6.0)));
    world.reset_powerups(seven()).unwrap();
    assert_eq!(seer(&world), Some(Distance::ZERO));
}

/// An object the world does not hold takes no powerup.
#[test]
fn powerup_of_unheld_object_fails() {
    let mut world = soldier_world();
    let attack = powerup(PowerupAttribute::ATTACK, 1.0, PowerupShare::Plain);
    assert!(
        world
            .apply_powerup(ObjectId::try_from(8).unwrap(), attack)
            .is_err()
    );
}
