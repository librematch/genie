use crate::combat::AbilityTimer;
use crate::combat::combat_update::{FAITH_TOP, refilled, regained_faith, special_sprite};
use crate::combat::{Charge, ChargeRate, SpecialSprite};
use crate::gathering::{DecayTimer, SalvageAmounts};
use crate::map::GridSize;
use crate::map::Position;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ActionPart;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::CombatPart;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::{Master, Task};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::testing::attribute;
use crate::testing::class;
use crate::testing::scene::{Scene, SceneObject};
use crate::unit_ai::AiAction;
use crate::unit_ai::AlertMode;
use crate::unit_ai::UnitAi;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

/// The monk's master (125): class 18, reload 1.6.
fn monk() -> Master {
    Master {
        id: MasterId::named(125),
        kind: MasterKind::COMBAT,
        class: class::MONK,
        combat: Some(CombatPart {
            reload: Seconds::new(1.6),
            ..CombatPart::default()
        }),
        ..Master::default()
    }
}

/// A world of player 1's object 7 of master `master`, alive, holding `holds`; its player's
/// attributes 0.
fn world_of(master: Master, holds: Option<(AttributeId, Amount)>) -> World {
    world_with(master, holds, None)
}

/// [`world_of`] with object 7's unit AI `ai`.
fn world_with(master: Master, holds: Option<(AttributeId, Amount)>, ai: Option<UnitAi>) -> World {
    let mut world = Scene::open(GridSize::new(4, 4))
        .object(SceneObject {
            ai,
            holds,
            ..SceneObject::new(7, PlayerId::new(1), master, Position::new(1.5, 1.5, 0.0))
        })
        .build();
    world.players[1].attributes = vec![Amount::ZERO; 500];
    world
}

/// Object 7's update start ([`Unit::combat_update_start`]) with a dt of `dt`, as the update takes
/// it out of its slot; the unit it leaves.
fn start(world: &mut World, dt: Millis, actions: &Action) -> Unit {
    let player = PlayerId::new(1);
    let id = ObjectId::try_from(7).unwrap();
    let index = world.live_index(player, id).unwrap();
    let mut unit = world.players.live(player, index).unwrap().clone();
    let roles = world.model.roles.clone();
    let mut step = Step {
        world,
        me: id,
        dt,
        turn: None,
        blasted: Vec::new(),
        taken_over: None,
        building_handover: false,
        player,
        side_notifies: None,
    };
    unit.combat_update_start(actions, &mut step, &roles, MasterKind::COMBAT)
        .unwrap();
    unit
}

/// The faith gains its rate times the dt (the product first, then the faith); at its top or
/// more it holds 100; a NaN never reaches it.
#[test]
fn faith_regains_its_rate_times_the_dt_up_to_its_top() {
    let regained = regained_faith(Amount::new(50.0), 1.6, Seconds::new(0.5), 100.0);
    assert_eq!(regained.get().to_bits(), (1.6_f32 * 0.5 + 50.0).to_bits());
    assert_eq!(
        regained_faith(Amount::new(99.5), 1.6, Seconds::new(0.5), 100.0),
        FAITH_TOP
    );
    assert!(
        regained_faith(Amount::new(f32::NAN), 1.6, Seconds::new(0.5), 100.0)
            .get()
            .is_nan()
    );
}

/// A charge gains its rate times the dt, the product first.
#[test]
fn charge_refills_by_its_rate_times_the_dt() {
    let charge = refilled(Charge::new(3.1), ChargeRate::new(0.7), Seconds::new(0.3));
    assert_eq!(charge.get().to_bits(), (0.7_f32 * 0.3 + 3.1).to_bits());
}

/// The special sprite shows only for an alert unit AI with no attackers and no current action,
/// its action list empty; no unit AI hides it.
#[test]
fn special_sprite_shows_on_an_alert_idle_unit_ai_without_actions() {
    let alert = UnitAi {
        alert_mode: AlertMode::Alert,
        ..UnitAi::default()
    };
    let empty = Action::default();
    assert_eq!(special_sprite(Some(&alert), &empty), SpecialSprite::Shown);
    assert_eq!(special_sprite(None, &empty), SpecialSprite::Hidden);
    let calm = UnitAi::default();
    assert_eq!(special_sprite(Some(&calm), &empty), SpecialSprite::Hidden);
    let acting = UnitAi {
        current_action: Some(AiAction::Make),
        ..alert
    };
    assert_eq!(special_sprite(Some(&acting), &empty), SpecialSprite::Hidden);
}

/// A live monk regains its master's reload time times the dt as faith, of kind 34; a dead one
/// keeps what it has.
#[test]
fn monk_regains_faith_by_its_reload_time() {
    let mut world = world_of(monk(), Some((attribute::FAITH, Amount::new(10.0))));
    let unit = start(&mut world, Millis::new(500), &Action::default());
    assert_eq!(
        (unit.life.amount.get().to_bits(), unit.life.attribute_kind),
        ((1.6_f32 * 0.5 + 10.0).to_bits(), Some(attribute::FAITH))
    );
    let mut dead = world_of(monk(), None);
    dead.players[1].objects[ListIndex::new(0)].life.state = ObjectState::Dying;
    let unit = start(&mut dead, Millis::new(500), &Action::default());
    assert_eq!(
        (unit.life.amount, unit.life.attribute_kind),
        (Amount::ZERO, None)
    );
}

/// An object of another class whose master has a conversion task (0x68) regains its player's
/// attribute 282 times the dt; without one, nothing.
#[test]
fn converter_regains_faith_by_its_player_attribute() {
    let converter = Master {
        class: class::VILLAGER,
        action: Some(ActionPart {
            tasks: vec![Task {
                action_kind: ActionKind::CONVERT,
                ..Task::default()
            }],
            ..ActionPart::default()
        }),
        ..monk()
    };
    let mut world = world_of(converter, None);
    world.players[1].attributes[282] = Amount::new(2.0);
    let unit = start(&mut world, Millis::new(250), &Action::default());
    assert_eq!(unit.life.amount, Amount::new(2.0 * 0.25));
    let plain = Master {
        class: class::VILLAGER,
        ..monk()
    };
    let mut world = world_of(plain, None);
    world.players[1].attributes[282] = Amount::new(2.0);
    let unit = start(&mut world, Millis::new(250), &Action::default());
    assert_eq!(
        (unit.life.amount, unit.life.attribute_kind),
        (Amount::ZERO, None)
    );
}

/// A salvage pile with an amount above 0 runs its decay timer down by the dt; at 0 or below its
/// amount and each of its six amounts lose one and the timer starts at the salvage decay rate
/// times 1000. An empty pile's timer stands.
#[test]
fn salvage_pile_decays_when_its_timer_runs_out() {
    let pile = Master {
        class: ClassId::named(40),
        ..monk()
    };
    let food = Some((attribute::FOOD, Amount::new(5.0)));
    let id = ObjectId::try_from(7).unwrap();
    let mut world = world_of(pile.clone(), food);
    world.players[1].attributes[attribute::SALVAGE_DECAY_RATE.index()] = Amount::new(5.0);
    {
        let combat = world.parts.combat_mut(id).unwrap();
        combat.decay_timer = DecayTimer::of_data(300);
        combat.salvage = SalvageAmounts::of_data([3, 0, 0, 0, 0, 1]);
    }
    let unit = start(&mut world, Millis::new(200), &Action::default());
    assert_eq!(unit.life.amount, Amount::new(5.0));
    assert_eq!(
        world.parts.combat_of(id).decay_timer,
        DecayTimer::of_data(100)
    );
    let unit = start(&mut world, Millis::new(100), &Action::default());
    assert_eq!(unit.life.amount, Amount::new(4.0));
    let combat = world.parts.combat_of(id);
    assert_eq!(
        (combat.decay_timer, combat.salvage.get()),
        (DecayTimer::of_data(5000), [2, -1, -1, -1, -1, 0])
    );
    let mut empty = world_of(pile, Some((attribute::FOOD, Amount::ZERO)));
    empty.parts.combat_mut(id).unwrap().decay_timer = DecayTimer::of_data(300);
    let _ = start(&mut empty, Millis::new(200), &Action::default());
    assert_eq!(
        empty.parts.combat_of(id).decay_timer,
        DecayTimer::of_data(300)
    );
}

/// A master of special attack 2 with a special sprite shows it while its unit AI is alert and
/// idle; any other master leaves the flag as it is.
#[test]
fn special_sprite_flag_follows_the_unit_ai_of_masters_with_one() {
    let special = Master {
        creatable: Some(CreatablePart {
            special_attack: SpecialAttack::RETALIATION,
            special_sprite: Some(SpriteId::named(9)),
            ..CreatablePart::default()
        }),
        class: class::VILLAGER,
        ..monk()
    };
    let id = ObjectId::try_from(7).unwrap();
    let alert = UnitAi {
        alert_mode: AlertMode::Alert,
        ..UnitAi::default()
    };
    let mut world = world_with(special, None, Some(alert));
    let _ = start(&mut world, Millis::new(100), &Action::default());
    assert_eq!(
        world.parts.combat_of(id).special_sprite,
        SpecialSprite::Shown
    );
    let plain = Master {
        class: class::VILLAGER,
        ..monk()
    };
    let mut world = world_of(plain, None);
    world.parts.combat_mut(id).unwrap().special_sprite = SpecialSprite::Shown;
    let _ = start(&mut world, Millis::new(100), &Action::default());
    assert_eq!(
        world.parts.combat_of(id).special_sprite,
        SpecialSprite::Shown
    );
}

/// Below its master's most a charge gains the recharge rate times the dt; at its most it
/// stays; a timed ability's charge (type −3) waits while the ability lasts.
#[test]
fn charge_refills_below_its_most_unless_timed_ability_lasts() {
    let charger = |kind| Master {
        creatable: Some(CreatablePart {
            max_charge: Charge::new(20.0),
            recharge_rate: ChargeRate::new(0.5),
            charge_kind: kind,
            ..CreatablePart::default()
        }),
        ..monk()
    };
    let id = ObjectId::try_from(7).unwrap();
    let slot = (PlayerId::new(1), ListIndex::new(0), id);
    let mut world = world_of(charger(ChargeKind::ATTACK), None);
    world.parts.combat_mut(id).unwrap().charge = Charge::new(3.0);
    world.refill_object_charge(slot, Seconds::new(2.0)).unwrap();
    assert_eq!(world.parts.combat_of(id).charge, Charge::new(4.0));
    world.parts.combat_mut(id).unwrap().charge = Charge::new(20.0);
    world.refill_object_charge(slot, Seconds::new(2.0)).unwrap();
    assert_eq!(world.parts.combat_of(id).charge, Charge::new(20.0));
    let mut timed = world_of(charger(ChargeKind::TIMED_ABILITY), None);
    timed.world_time = WorldTime::from_millis(1_000);
    timed.parts.combat_mut(id).unwrap().ability = AbilityTimer {
        duration: Millis::new(10_000),
        activated: WorldTime::ZERO,
    };
    timed.refill_object_charge(slot, Seconds::new(2.0)).unwrap();
    assert_eq!(timed.parts.combat_of(id).charge, Charge::ZERO);
    timed.world_time = WorldTime::from_millis(10_001);
    timed.refill_object_charge(slot, Seconds::new(2.0)).unwrap();
    assert_eq!(timed.parts.combat_of(id).charge, Charge::new(1.0));
}
