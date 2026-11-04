use crate::ErrorKind;
use crate::test_ids::object_id;
use crate::unit_ai::{AiAction, OrderKind};
use crate::world::load::fixtures::{combat_mut, combat_object, missile_tuning};
use crate::world::load::object::object as saved_unit_object;
use crate::world::load::object_parts::unit_ai;
use genie_save::generated::world::{
    Path, TribeTowerUnitAiModule, UnitAiModule, UnitAiModuleEntry1,
};

/// A two-player game's players with Gaia.
const PLAYERS: usize = 3;

/// A saved queue record holding order value `order`.
fn record(order: i32) -> [u8; 52] {
    let mut bytes = [0; 52];
    bytes[4..8].copy_from_slice(&order.to_le_bytes());
    bytes
}

/// A saved unit AI without an order or an action (−1 in each field, as the game's saves hold
/// none).
fn idle() -> UnitAiModule {
    UnitAiModule {
        current_order_value: -1,
        last_order_value: -1,
        current_action_value: -1,
        last_action_value: -1,
        ..UnitAiModule::default()
    }
}

/// A history record of order `order` and action `action`.
fn history(order: i32, action: i32) -> UnitAiModuleEntry1 {
    UnitAiModuleEntry1 {
        new_oh_order: order,
        new_oh_action: action,
        ..UnitAiModuleEntry1::default()
    }
}

/// A combat object whose unit AI is `ai`, a soldier's (`needUnitAI` 0xd, which the save
/// follows with the base's module).
fn with_ai(ai: UnitAiModule) -> genie_save::Object {
    let mut object = combat_object(5, 4);
    let combat = combat_mut(&mut object);
    combat.need_unit_ai = 0xd;
    combat.unit_ai_module_2 = Some(Box::new(ai));
    object
}

/// A combat object without a unit AI imports none.
#[test]
fn an_object_without_unit_ai_imports_none() {
    assert_eq!(
        unit_ai(&combat_object(5, 4), object_id(5), PLAYERS).map(|ai| ai.is_none()),
        Ok(true)
    );
}

/// A saved unit AI of type 1, which the load has the factory pick by class, fails the import:
/// no constructor of the switch makes it.
#[test]
fn saved_unit_ai_of_kind_one_fails_the_import() {
    let mut object = with_ai(idle());
    combat_mut(&mut object).need_unit_ai = 1;
    let error = unit_ai(&object, object_id(5), PLAYERS).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(
        error.to_string().contains("no constructor makes"),
        "{error}"
    );
}

/// The orders and actions import as their kinds: a field's −1 as none, the history's and the
/// queue's records as theirs.
#[test]
fn the_saved_orders_and_actions_import_as_their_kinds() {
    let ai = unit_ai(
        &with_ai(UnitAiModule {
            last_order_value: 0x2c5,
            current_action_value: 0x261,
            oh: vec![history(0x2c9, 0x265), history(-1, -1)],
            temp_order: vec![record(0x2c6)],
            ..idle()
        }),
        object_id(5),
        PLAYERS,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        (ai.current_order, ai.last_order),
        (None, Some(OrderKind::Gather))
    );
    assert_eq!(
        (ai.current_action, ai.last_action),
        (Some(AiAction::Gather), None)
    );
    let records: Vec<_> = ai
        .order_history
        .iter()
        .map(|order_record| (order_record.order, order_record.action))
        .collect();
    assert_eq!(
        records,
        [(Some(OrderKind::Hunt), Some(AiAction::Hunt)), (None, None)]
    );
    assert_eq!(ai.orders[0].order, OrderKind::Move);
    assert_eq!(ai.building_orders, []);
}

/// A tower's saved orders import as the building's queue.
#[test]
fn tower_saved_orders_import() {
    let mut object = combat_object(5, 4);
    let combat = combat_mut(&mut object);
    combat.need_unit_ai = 0xf;
    combat.tribe_tower_unit_ai_module = Some(Box::new(TribeTowerUnitAiModule {
        unit_ai_module: idle(),
        saved_order: vec![record(700)],
        ..TribeTowerUnitAiModule::default()
    }));
    let ai = unit_ai(&object, object_id(5), PLAYERS).unwrap().unwrap();
    assert_eq!(ai.building_orders[0].order, OrderKind::Attack);
}

/// An order or action value none names fails the import, wherever the save holds it.
#[test]
fn value_none_names_fails_the_import() {
    let saves = [
        UnitAiModule {
            current_order_value: 0x262,
            ..idle()
        },
        UnitAiModule {
            last_order_value: 0x2cf,
            ..idle()
        },
        UnitAiModule {
            oh: vec![history(0x269, -1)],
            ..idle()
        },
        UnitAiModule {
            temp_order: vec![record(0x323)],
            ..idle()
        },
        UnitAiModule {
            current_action_value: 0x2c6,
            ..idle()
        },
        UnitAiModule {
            last_action_value: 0x25f,
            ..idle()
        },
        UnitAiModule {
            oh: vec![history(-1, 0x268)],
            ..idle()
        },
    ];
    for save in saves {
        assert_eq!(
            unit_ai(&with_ai(save), object_id(5), PLAYERS)
                .map_err(|error| error.kind())
                .err(),
            Some(ErrorKind::InvalidInput)
        );
    }
}

/// The saved unit AI is the imported object's part, not its row's; a value none names leaves the
/// object without one, and the row holds the failure as its unmodelled part.
#[test]
fn the_saved_unit_ai_is_the_object_part() {
    use crate::players::CivId;
    use genie_save::generated::world::PlayerObjects;
    let whole = |object: &genie_save::Object| {
        saved_unit_object(
            object,
            &PlayerObjects::default(),
            &[],
            CivId::GAIA,
            None,
            PLAYERS,
            &missile_tuning(),
        )
    };
    let object = with_ai(UnitAiModule {
        current_order_value: 0x2c9,
        ..idle()
    });
    let loaded = whole(&object);
    assert_eq!(
        loaded.parts.unit_ai,
        unit_ai(&object, object_id(5), PLAYERS).unwrap()
    );
    assert!(loaded.parts.unit_ai.is_some());
    let odd = whole(&with_ai(UnitAiModule {
        current_order_value: 0x262,
        ..idle()
    }));
    assert_eq!(odd.parts.unit_ai, None);
    assert_eq!(
        odd.parts.unread.map(|unread| unread.error().kind()),
        Some(ErrorKind::InvalidInput)
    );
}

/// A saved patrol imports its points; one without points fails the import: the patrol's leash
/// (`FUN_140f95e10`) runs its loop past an empty path's end.
#[test]
fn saved_patrol_without_points_fails_the_import() {
    let patrol = |waypoints: Vec<[u8; 12]>| {
        with_ai(UnitAiModule {
            has_patrol_path: 1,
            patrol_path: Some(Box::new(Path {
                nwv: i32::try_from(waypoints.len()).unwrap(),
                waypoints,
                ..Path::default()
            })),
            ..idle()
        })
    };
    let point = [1.0_f32, 2.0, 0.0].map(f32::to_le_bytes).concat();
    let held = unit_ai(
        &patrol(vec![point.try_into().unwrap()]),
        object_id(5),
        PLAYERS,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        held.patrol_path
            .map(|patrol_path| patrol_path.path.points.len()),
        Some(1)
    );
    let empty = unit_ai(&patrol(Vec::new()), object_id(5), PLAYERS).unwrap_err();
    assert_eq!(
        (empty.kind(), empty.reason()),
        (ErrorKind::InvalidInput, "a saved patrol without points")
    );
}
