use super::*;
use crate::testing::attribute;
use crate::world::GameMode;
use crate::world::settings::OptionFlags;

/// A scenario's settings with the villagers' force drop on, where the carry rule
/// (`FUN_141302150`) holds, or the default settings, where it does not.
pub(crate) fn settings(force_drop: bool) -> Settings {
    if force_drop {
        Settings {
            game_mode: GameMode::Scenario,
            flags: OptionFlags::VILLAGER_FORCE_DROP,
            ..Settings::default()
        }
    } else {
        Settings::default()
    }
}

/// What a unit holds: `amount` of `kind`.
fn holding(kind: Option<AttributeId>, amount: f32) -> Held {
    Held {
        kind,
        amount: Amount::new(amount),
    }
}

/// `0x140ac49b0` and `0x140a8b3a0`'s load for a task of meat: a load of the task's kind stays, a
/// load of another food takes the task's kind with its amount, a load of another resource stays
/// unless the carry rule holds or a computer player carries nothing (0 or −0.0, not a NaN).
#[test]
fn load_for_task_keeps_food_and_drops_another_resource_by_the_rule_or_for_an_empty_computer() {
    let meat = Some(attribute::MEAT);
    let load = |held: Held, force_drop: bool, player_kind: PlayerKind| {
        load_for_task(held, meat, &settings(force_drop), player_kind)
    };
    let human = PlayerKind::Human;
    let computer = PlayerKind::Computer;
    assert_eq!(load(holding(meat, 7.0), true, computer), holding(meat, 7.0));
    assert_eq!(
        load(holding(Some(attribute::FOOD), 7.0), true, computer),
        holding(meat, 7.0)
    );
    assert_eq!(
        load(holding(Some(attribute::STONE), 7.0), false, human),
        holding(Some(attribute::STONE), 7.0)
    );
    assert_eq!(
        load(holding(Some(attribute::STONE), 7.0), true, human),
        holding(meat, 0.0)
    );
    for nothing in [0.0, -0.0] {
        assert_eq!(
            load(holding(Some(attribute::STONE), nothing), false, computer),
            holding(meat, 0.0)
        );
    }
    assert_eq!(
        load(holding(Some(attribute::STONE), 7.0), false, computer),
        holding(Some(attribute::STONE), 7.0)
    );
    let kept = load(holding(Some(attribute::STONE), f32::NAN), false, computer);
    assert_eq!(kept.kind, Some(attribute::STONE));
    assert!(kept.amount.get().is_nan());
    // a unit that carries none (−1) is no food
    assert_eq!(load(holding(None, 0.0), false, human), holding(None, 0.0));
}

/// A load of a task's kind that is no food stays as it is, by the carry rule and for a computer
/// player carrying nothing alike.
#[test]
fn load_for_task_keeps_load_of_task_kind_that_is_no_food() {
    let stone = Some(attribute::STONE);
    for amount in [7.0, 0.0] {
        assert_eq!(
            load_for_task(
                holding(stone, amount),
                stone,
                &settings(true),
                PlayerKind::Computer
            ),
            holding(stone, amount)
        );
    }
}

/// A task without a resource (−1) is no food: a load of food stays, and the carry rule leaves the
/// unit holding none.
#[test]
fn load_for_task_without_resource_drops_food_only_by_the_rule() {
    let food = holding(Some(attribute::FOOD), 7.0);
    assert_eq!(
        load_for_task(food, None, &settings(false), PlayerKind::Human),
        food
    );
    assert_eq!(
        load_for_task(food, None, &settings(true), PlayerKind::Human),
        holding(None, 0.0)
    );
}
