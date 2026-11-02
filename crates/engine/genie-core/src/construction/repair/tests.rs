use super::*;
use crate::combat::damage::ArmorPair;
use crate::construction::MainBuildingId;
use crate::gathering::hunt::tests::walker;
use crate::map::TerrainFlags;
use crate::math::rng::Well512;
use crate::movement::Me;
use crate::movement::Speed;
use crate::movement::move_to::TargetMoved;
use crate::movement::move_to::{
    MoveDefaults, MoveTo, MoveToAction, MoveToReads, MoveWait, SpriteReads, WalkTo,
};
use crate::objects::ActionState;
use crate::objects::HpPowerup;
use crate::objects::ListIndex;
use crate::objects::MasterHitPoints;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::Voice;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::{Master, Masters};
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::{FacetCount, Frame, Graphics};
use crate::orders::Work;
use crate::players::Amount;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::relation_row;
use crate::technology::Attributes;
use crate::test_ids::costs;
use crate::test_ids::object_id;
use crate::test_ids::task_id;
use crate::unit_ai::{AiAction, NotifyKind};
use crate::world::tests::{in_update, repairing};
use crate::world::{Player, World, WorldTime};

const TASK: RepairTask = RepairTask {
    id: task_id(7),
    work_value: WorkValue::new(0.5),
    moving: None,
    proceeding: None,
};

/// [`TASK`] with the moving sprite 3 and the proceeding sprite 4.
const SHOWING: RepairTask = RepairTask {
    moving: Some(SpriteId::named(3)),
    proceeding: Some(SpriteId::named(4)),
    ..TASK
};

/// The sprites' table of the tests: the task's 3 and 4, the master's standing sprite 5 and the
/// `MoveTo`s' defaults 20 (to a point) and 21 (to an object), each of speed factor 1.
fn graphics() -> Graphics {
    [3, 4, 5, 20, 21]
        .map(|id| {
            let g = Graphic {
                id: SpriteId::named(id),
                frames: Frame::of_data(10),
                facets: FacetCount::of_data(16),
                speed_multiplier: SpeedMultiplier::ONE,
                frame_duration: Seconds::new(0.1),
                replay_delay: Seconds::ZERO,
                flags: SpriteFlags::ANIMATED,
            };
            (g.id, g)
        })
        .into()
}

/// The `MoveTo`s' defaults of the tests' repairer: 20 to a point, 21 to an object.
const DEFAULTS: MoveDefaults = MoveDefaults {
    point: Some(SpriteId::named(20)),
    object: Some(SpriteId::named(21)),
};

/// What the tests' repairer's `MoveTo`s read of it: [`DEFAULTS`] in the table `graphics`, alive
/// (state 2), of speed 1, at (10, 10) with radii 0.2.
const fn move_reads(graphics: &Graphics) -> MoveToReads<'_> {
    MoveToReads {
        sprites: SpriteReads {
            defaults: DEFAULTS,
            graphics,
        },
        state: ObjectState::Alive,
        speed: Speed::new(1.0),
        at: Position::new(10.0, 10.0, 0.0),
        ground: Footprint::new(0.2, 0.2),
    }
}

/// The tests' target, the object 20 at (11, 10), 0.3 from the repairer and alive.
const TARGET: WalkTo = WalkTo {
    id: Some(object_id(20)),
    at: Position::new(11.0, 10.0, 0.0),
    gap: Distance::new(0.3),
    state: Some(ObjectState::Alive),
};

/// The Repair of the work command on [`TARGET`], within 0, with [`TASK`].
fn started() -> RepairAction {
    RepairAction::start(TARGET, TASK, Distance::ZERO, &move_reads(&graphics())).unwrap()
}

/// A villager's master work rate 1 with a powerup of 0: half a hit point per second per the
/// task, each repairer past the first half of it.
#[test]
fn the_work_counts_half_for_each_repairer_past_the_first() {
    assert_eq!(
        work_amount(
            WorkRate::new(1.0),
            WorkValue::new(0.5),
            Seconds::new(0.05),
            None
        ),
        HitPoints::new(1.0 * 0.5 * 0.05)
    );
    assert_eq!(
        work_amount(
            WorkRate::new(1.0),
            WorkValue::new(0.5),
            Seconds::new(0.05),
            Some(BuilderCount::of_data(1))
        ),
        HitPoints::new(1.0 * 0.5 * 0.05)
    );
    let alone = 1.0f32 * 0.5 * 0.05;
    assert_eq!(
        work_amount(
            WorkRate::new(1.0),
            WorkValue::new(0.5),
            Seconds::new(0.05),
            Some(BuilderCount::of_data(3))
        ),
        HitPoints::new(alone * (((3.0 - 1.0) * 0.5 + 1.0) / 3.0))
    );
}

/// The repairer's player's attributes as a list (food, wood, stone, gold at 0 to 3).
struct Held(Vec<Amount>);

impl Attributes for Held {
    fn get(&self, i: AttributeId) -> Result<Amount, GenieError> {
        Ok(self.0[i.index()])
    }
    fn set(&mut self, i: AttributeId, v: Amount) -> Result<(), GenieError> {
        self.0[i.index()] = v;
        Ok(())
    }
}

/// A house (70, class 3, 550 hit points, 25 wood) and a ram (35, class 13, 175 hit points,
/// 160 wood, 75 gold) of the repairer's player.
fn masters() -> Masters {
    Masters::new([
        Master {
            id: ids::HOUSE,
            terrain_restriction: Some(RestrictionId::named(0)),
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            hit_points: MasterHitPoints::of_data(550),
            cost: costs(&[(attribute::WOOD, 25, true)]),
            ..Master::default()
        },
        Master {
            id: ids::ID_35,
            terrain_restriction: Some(RestrictionId::named(0)),
            kind: MasterKind::COMBAT,
            class: class::SIEGE,
            hit_points: MasterHitPoints::of_data(175),
            cost: costs(&[(attribute::WOOD, 160, true), (attribute::GOLD, 75, true)]),
            ..Master::default()
        },
    ])
}

fn house(id: i32, hp: HitPoints) -> Repaired {
    Repaired {
        id: object_id(id),
        kind: MasterKind::BUILDING,
        class: class::BUILDING,
        master: ids::HOUSE,
        master_hp: MasterHitPoints::of_data(550),
        hp,
        hp_powerup: HpPowerup::default(),
        main: None,
        annexes: Vec::new(),
        unit_repair_cost: Amount::new(0.5),
        building_repair_cost: Amount::new(0.15),
    }
}

/// `FUN_1411d37d0` on a house: the amount, at most what its hit points lack, costs 0.15 of
/// its 25 wood per its 550 hit points; the last of it heals it full (1); a player without
/// the wood pays nothing and heals nothing (2); a house below 1 hit point takes nothing.
#[test]
fn building_repair_pays_its_share_of_the_cost_and_heals() {
    let masters = masters();
    let mut wood = Held(vec![
        Amount::ZERO,
        Amount::new(100.0),
        Amount::ZERO,
        Amount::ZERO,
    ]);
    let mut set = vec![house(1, HitPoints::new(500.0))];
    let mut payer = Payer {
        masters: &masters,
        attributes: &mut wood,
    };
    assert_eq!(
        repair_building(&mut set, 0, HitPoints::new(10.0), true, &mut payer),
        Ok(RepairOutcome::Healed)
    );
    assert_eq!(set[0].hp, HitPoints::new(510.0));
    let share = 0.15f32 * 10.0 / 550.0;
    assert_eq!(wood.0[1], Amount::new(100.0 - 25.0 * share));
    let mut payer = Payer {
        masters: &masters,
        attributes: &mut wood,
    };
    assert_eq!(
        repair_building(&mut set, 0, HitPoints::new(50.0), true, &mut payer),
        Ok(RepairOutcome::Full)
    );
    assert_eq!(set[0].hp, HitPoints::new(550.0));
    let mut broke = Held(vec![Amount::ZERO; 4]);
    let mut set = vec![house(1, HitPoints::new(500.0))];
    let mut payer = Payer {
        masters: &masters,
        attributes: &mut broke,
    };
    assert_eq!(
        repair_building(&mut set, 0, HitPoints::new(10.0), true, &mut payer),
        Ok(RepairOutcome::Unpaid)
    );
    assert_eq!(set[0].hp, HitPoints::new(500.0));
    let mut set = vec![house(1, HitPoints::new(0.5))];
    assert_eq!(
        repair_building(&mut set, 0, HitPoints::new(10.0), true, &mut payer),
        Ok(RepairOutcome::Unpaid)
    );
}

/// From its repairer an annex's repair is its main building's, which repairs its annexes
/// (here the annex itself) by the whole amount first, then itself.
#[test]
fn an_annex_repair_goes_to_its_main_building_and_its_annexes() {
    let masters = masters();
    let mut wood = Held(vec![
        Amount::ZERO,
        Amount::new(100.0),
        Amount::ZERO,
        Amount::ZERO,
    ]);
    let mut set = vec![
        Repaired {
            main: Some(MainBuildingId::new(object_id(2))),
            ..house(1, HitPoints::new(500.0))
        },
        Repaired {
            annexes: vec![object_id(1)],
            ..house(2, HitPoints::new(540.0))
        },
    ];
    let mut payer = Payer {
        masters: &masters,
        attributes: &mut wood,
    };
    assert_eq!(
        repair_building(&mut set, 0, HitPoints::new(20.0), true, &mut payer),
        Ok(RepairOutcome::Full)
    );
    assert_eq!(
        (set[0].hp, set[1].hp),
        (HitPoints::new(520.0), HitPoints::new(550.0))
    );
}

/// `FUN_1412143b0` on a ram (class 13): the amount, at most what it lacks of its most, costs
/// its owner's unit repair rate of its costs per its most hit points; then it heals.
#[test]
fn siege_unit_repair_pays_its_unit_share() {
    let masters = masters();
    let mut held = Held(vec![
        Amount::ZERO,
        Amount::new(1000.0),
        Amount::ZERO,
        Amount::new(1000.0),
    ]);
    let mut set = vec![Repaired {
        kind: MasterKind::COMBAT,
        class: class::SIEGE,
        master: ids::ID_35,
        master_hp: MasterHitPoints::of_data(175),
        ..house(5, HitPoints::new(100.0))
    }];
    let mut payer = Payer {
        masters: &masters,
        attributes: &mut held,
    };
    assert_eq!(
        repair_unit(&mut set, 0, HitPoints::new(10.0), &mut payer),
        Ok(RepairOutcome::Healed)
    );
    let share = 0.5f32 * 10.0 / 175.0;
    assert_eq!(set[0].hp, HitPoints::new(110.0));
    assert_eq!(
        (held.0[1], held.0[3]),
        (
            Amount::new(1000.0 - 160.0 * share),
            Amount::new(1000.0 - 75.0 * share)
        )
    );
}

/// Within the Repair's reach of the house 10028 (its footprint touching the house's).
const NEAR: Position = Position::new(30.0, 89.2, 0.0);
/// 0.3 tiles off the house: out of reach, and no quick path closer.
const OFF: Position = Position::new(29.5, 89.5, 0.0);

/// State 4 with its quiet `MoveTo` in `state` ([`MoveToAction::sub_in`]).
const fn moving(state: MoveTo) -> RepairState {
    RepairState::Move {
        move_to: Some(MoveToAction::sub_in(state)),
    }
}

/// [`crate::world::tests::repairing`] with the repairer 9175 at `at` on its work command's
/// Repair of the house 10028 (300 of 550 hit points), the Repair then set by `set`; its master
/// 118 with the standing sprite 5 and the walking sprite 20, the sprites' table [`graphics`],
/// the repairer showing its standing sprite.
fn repair_at(at: Position, set: impl FnOnce(&mut RepairAction)) -> World {
    let mut t = repairing(at);
    t.graphics = graphics();
    for masters in &mut t.masters {
        let m = masters.get_mut(ids::BUILDER).unwrap();
        (m.standing_sprite, m.move_sprite) = (Some(SpriteId::named(5)), Some(SpriteId::named(20)));
    }
    let work = Work {
        target: Some(object_id(10028)),
        point: Position::new(30.0, 88.0, 0.0),
        carry_test: false,
        filter: None,
        notified: true,
    };
    t.order_work(PlayerId::new(1), ListIndex::new(0), &work)
        .unwrap();
    let v = &mut t.players[1].objects[ListIndex::new(0)];
    let standing = *graphics().get(SpriteId::named(5)).unwrap();
    v.sprite = Some(ActiveSprite::start(standing, &mut Well512::default()));
    set(t
        .parts
        .actions
        .of_mut(v.id)
        .unwrap()
        .list
        .repair_mut()
        .unwrap());
    t
}

/// The map's terrain of class 4 (`terrain_class` of flags 0x20), the tiles' class: the
/// repairer's master's restriction then passes them, so its quick path reaches the house.
fn passable(t: &mut World) {
    t.spatial.terrain.as_mut().unwrap().terrains[0].flags = TerrainFlags::LAND;
}

/// The repairer's Repair part of 50 ms through its step (`Unit::repair_part`): its code.
fn repaired(t: &mut World) -> Result<HeadCode, GenieError> {
    in_update(
        t,
        (PlayerId::new(1), ListIndex::new(0)),
        50,
        |unit, actions, c| {
            let me = Me {
                human_order: WorldTime::ZERO,
                id: unit.id,
                group: unit.group.id,
                action_state: actions
                    .top_action()
                    .map_or(ActionState::NO_ACTION, |a| a.state),
            };
            unit.repair_part(actions, c, me)
        },
    )
}

/// The repairer's Repair after its update.
fn repair_of(t: &World) -> RepairAction {
    *t.parts
        .actions_of(t.players[1].objects[ListIndex::new(0)].id)
        .list
        .repair()
        .unwrap()
}

/// The notifies the repairer's unit AI heard, each with the repair's action value.
fn heard(t: &World) -> Vec<NotifyKind> {
    let ai = t
        .parts
        .unit_ai
        .get(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap();
    let repair = AiAction::Repair.value();
    ai.notifies
        .iter()
        .inspect(|n| assert_eq!(n.p, [repair, 0, 0]))
        .map(|n| n.kind)
        .collect()
}

/// The repairer's sprite.
fn sprite_of(t: &World) -> Option<SpriteId> {
    t.players[1].objects[ListIndex::new(0)]
        .sprite
        .map(|s| s.graphic.id)
}

/// The house's hit points, repairers and holders (its slot 0x180 count).
fn the_house(t: &World) -> (f32, u8, i32) {
    let h = t.object(object_id(10028)).unwrap().1;
    (
        h.life.hp.get(),
        t.parts.repairers_of(h.id).get(),
        h.workers.get(),
    )
}

/// The sprite its `MoveTo` shows: the task's moving sprite, else the default of its
/// constructor, toward the target (state 4, `FUN_140aa0df0`) or the point (11,
/// `FUN_140aa0d30`); a sprite the table lacks fails the move.
#[test]
fn repair_move_to_shows_its_task_moving_sprite_else_its_default() {
    let g = graphics();
    let shown = |task| {
        let r = RepairAction { task, ..started() };
        let id = |m: MoveToAction| m.sprite.map(|s| s.id);
        [
            id(r.walk(&move_reads(&g), Some(TARGET)).unwrap().unwrap()),
            id(r.to_point(Position::new(1.0, 2.0, 0.0), &move_reads(&g))
                .unwrap()
                .unwrap()),
        ]
    };
    assert_eq!(
        shown(TASK),
        [Some(SpriteId::named(21)), Some(SpriteId::named(20))]
    );
    assert_eq!(shown(SHOWING), [Some(SpriteId::named(3)); 2]);
    let lacking = RepairTask {
        moving: Some(SpriteId::named(6)),
        ..TASK
    };
    let r = RepairAction {
        task: lacking,
        ..started()
    };
    assert!(r.walk(&move_reads(&g), Some(TARGET)).is_err());
    assert!(
        r.to_point(Position::new(1.0, 2.0, 0.0), &move_reads(&g))
            .is_err()
    );
}

/// Every `MoveTo` of the Repair is quiet (`0x140ad9c60`: `FUN_140a7a170` with 1): the start's
/// to the target within the master's weapon range, a move's to its point within 0.
#[test]
fn repair_move_tos_are_quiet() {
    let g = graphics();
    let r = RepairAction::start(TARGET, TASK, Distance::new(1.5), &move_reads(&g)).unwrap();
    let to_target = MoveToAction {
        state: MoveTo::Start,
        point: Position::new(11.0, 10.0, 0.0),
        target: Some(object_id(20)),
        range: Distance::new(1.5),
        sprite: Some(*g.get(SpriteId::named(21)).unwrap()),
        voice: Voice::Quiet,
        on_target_moved: TargetMoved::Follows,
        wait: MoveWait::NEW,
    };
    assert_eq!(r.state.move_to().copied(), Some(to_target));
    assert_eq!(
        r.to_point(Position::new(1.0, 2.0, 0.0), &move_reads(&g)),
        Ok(Some(MoveToAction::sub_to_point(
            Position::new(1.0, 2.0, 0.0),
            Distance::ZERO,
            Some(*g.get(SpriteId::named(20)).unwrap())
        )))
    );
}

/// Its sub-action list adds each `MoveTo` as `FUN_140a977e0` does (`0x140aa2100`): beside its
/// target (0.01 or nearer) it arrived, a repairer without speed failed, and one in state 3 or
/// later adds none; with no target it walks to (0, 0, 0).
#[test]
fn repair_move_to_starts_as_its_list_adds_it() {
    let g = graphics();
    let state = |to, reads: MoveToReads<'_>| {
        RepairAction::start(to, TASK, Distance::ZERO, &reads)
            .unwrap()
            .state
            .move_to()
            .copied()
            .map(|m| m.state)
    };
    let beside = WalkTo {
        gap: Distance::new(0.01),
        ..TARGET
    };
    assert_eq!(state(beside, move_reads(&g)), Some(MoveTo::Arrived));
    let stuck = MoveToReads {
        speed: Speed::ZERO,
        ..move_reads(&g)
    };
    assert_eq!(state(TARGET, stuck), Some(MoveTo::Failed));
    let dying = MoveToReads {
        state: ObjectState::Dying,
        ..move_reads(&g)
    };
    assert_eq!(state(TARGET, dying), None);
    let none = started().walk(&move_reads(&g), None).unwrap().unwrap();
    assert_eq!((none.target, none.point), (None, Position::ORIGIN));
}

/// Its states show the task's sprites (slot 0x68, `set_state` `0x140ad9c60`): the turn toward
/// the target (6) the moving one, the work (7) the proceeding one; for none slot 0x68 shows the
/// master's standing sprite.
#[test]
fn repairer_shows_its_task_sprites_as_it_faces_and_works() {
    for (task, sprites) in [(SHOWING, [3, 4]), (TASK, [5, 5])] {
        let mut t = repair_at(NEAR, |r| {
            (r.state, r.task) = (moving(MoveTo::Arrived), task);
        });
        assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
        assert_eq!(
            (repair_of(&t).state, sprite_of(&t)),
            (RepairState::Face, Some(SpriteId::named(sprites[0])))
        );
        assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
        assert_eq!(
            (repair_of(&t).state, sprite_of(&t)),
            (RepairState::Work, Some(SpriteId::named(sprites[1])))
        );
    }
}

/// The walk to the point starts its quiet `MoveTo`, which shows its sprite before the motion
/// step (`0x140aa29e0`'s `set_state(4)`): the task's moving sprite, else the master's walking
/// one (`FUN_140aa0d30`'s default).
#[test]
fn repair_walk_to_its_point_shows_its_sprite_as_it_starts() {
    for (task, shown) in [(TASK, 20), (SHOWING, 3)] {
        let mut t = repair_at(NEAR, |r| r.task = task);
        let v = &mut t.players[1].objects[ListIndex::new(0)];
        let mover = t.parts.motion.mover_mut(v.id);
        let r = t
            .parts
            .actions
            .of_mut(v.id)
            .unwrap()
            .list
            .repair_mut()
            .unwrap();
        let g = graphics();
        let _ = r
            .take_point(Position::new(35.5, 95.5, 0.0), mover, &move_reads(&g))
            .unwrap();
        assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
        assert_eq!(
            repair_of(&t).state.move_to().copied().map(|m| m.state),
            Some(MoveTo::Moving)
        );
        assert_eq!(sprite_of(&t), Some(SpriteId::named(shown)));
    }
}

/// A target the world no longer holds, another player's or a dead one ends the Repair
/// (`set_state(1)`: notify 0x1fa, the standing sprite, the target let go); the next update
/// removes it.
#[test]
fn repair_ends_with_its_target_gone_foreign_or_dead() {
    type Change = fn(&mut World);
    let cases: [(Change, bool); 4] = [
        (
            |t| drop(t.remove_live(PlayerId::new(1), ListIndex::new(1)).unwrap()),
            false,
        ),
        (
            |t| t.players[1].objects[ListIndex::new(1)].life.state = ObjectState::Dying,
            true,
        ),
        (
            |t| t.players[1].objects[ListIndex::new(1)].life.hp = HitPoints::new(0.25),
            true,
        ),
        (
            |t| {
                let house = t.remove_live(PlayerId::new(1), ListIndex::new(1)).unwrap();
                t.players.push(Player {
                    objects: vec![house.unit].into(),
                    attributes: vec![Amount::ZERO; 300],
                    relations: relation_row([1, 0, 0]),
                    ..Player::default()
                });
                t.reindex();
                t.players[1].relations = relation_row([1, 0, 3]);
                t.masters.push(t.masters[1].clone());
            },
            true,
        ),
    ];
    for (change, held) in cases {
        let mut t = repair_at(NEAR, |r| {
            r.state = moving(MoveTo::Arrived);
        });
        change(&mut t);
        assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
        let r = repair_of(&t);
        assert_eq!((r.state, r.target), (RepairState::Done, None));
        assert_eq!(heard(&t), [NotifyKind::ACTION_COMPLETED]);
        assert_eq!(sprite_of(&t), Some(SpriteId::named(5)));
        if held {
            assert_eq!(the_house(&t).2, 0, "the house let go");
        }
        assert_eq!(repaired(&mut t), Ok(HeadCode::Deletes));
    }
}

/// State 4 when its `MoveTo`'s target died (its first check: code 5,
/// [`MoveToAction::check_held`]): the Repair lets its target go (slot 0x68 with none) and ends
/// (`set_state(1)`: notify 0x1fa, the standing sprite).
#[test]
fn repair_whose_move_to_target_died_lets_go_and_ends() {
    let mut t = repair_at(NEAR, |r| {
        r.state = RepairState::Move {
            move_to: Some(MoveToAction {
                target: Some(object_id(777)),
                ..MoveToAction::sub_in(MoveTo::Moving)
            }),
        };
    });
    let mut dying = Unit {
        id: object_id(777),
        ..Unit::default()
    };
    dying.life.state = ObjectState::Removed;
    let _ = t.push_live(PlayerId::new(1), dying).unwrap();
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    let r = repair_of(&t);
    assert_eq!(
        (r.state, r.target, r.state.move_to().copied()),
        (RepairState::Done, None, None)
    );
    assert_eq!(heard(&t), [NotifyKind::ACTION_COMPLETED]);
    assert_eq!(the_house(&t).2, 0, "the house let go");
}

/// Gaia's object is repaired: the repairer faces it, then works and joins its repairers.
#[test]
fn gaia_object_is_repaired() {
    let mut t = repair_at(NEAR, |r| r.state = RepairState::Face);
    let house = t.remove_live(PlayerId::new(1), ListIndex::new(1)).unwrap();
    let _ = t.push_live(PlayerId::new(0), house).unwrap();
    t.players[0].attributes = vec![Amount::ZERO; 300];
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(
        (repair_of(&t).state, the_house(&t).1),
        (RepairState::Work, 1)
    );
}

/// State 4 once the `MoveTo` ended: within reach the unit faces the target (6); out of reach
/// it waits (10) with a quick path to it, else fails (13), whose update posts 0x1f9.
#[test]
fn repairer_at_its_target_faces_it_or_waits_or_fails() {
    let arrived = |r: &mut RepairAction| r.state = moving(MoveTo::Arrived);
    let mut t = repair_at(NEAR, arrived);
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    let r = repair_of(&t);
    assert_eq!(
        (r.state, r.state.move_to().copied()),
        (RepairState::Face, None)
    );
    // no MoveTo (the sub-action list empty, FUN_140a992b0's 1) counts as at the target
    let mut t = repair_at(NEAR, |r| r.state = RepairState::Move { move_to: None });
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(repair_of(&t).state, RepairState::Face);
    let mut t = repair_at(Position::new(30.0, 95.0, 0.0), arrived);
    passable(&mut t);
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    let r = repair_of(&t);
    assert_eq!(
        (r.state, r.timer, sprite_of(&t)),
        (
            RepairState::Wait,
            Seconds::new(1.0),
            Some(SpriteId::named(5))
        )
    );
    let mut t = repair_at(OFF, arrived);
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(repair_of(&t).state, RepairState::Failed);
    assert_eq!(repaired(&mut t), Ok(HeadCode::Deletes));
    assert_eq!(repair_of(&t).state, RepairState::Halted);
    assert_eq!(heard(&t), [NotifyKind::ACTION_FAILED]);
    assert_eq!(the_house(&t).0, 300.0);
}

/// State 7: each update's work goes to the target's repair; full (1) the Repair ends, its
/// counted repairer out of the repairers, the armor back; the player short of resources (2)
/// posts 0x1fb and ends it too; a target that walked out of reach sends the unit walking.
#[test]
fn repairer_works_until_the_target_is_full_or_the_player_cannot_pay() {
    let working = |r: &mut RepairAction| r.state = RepairState::Work;
    let mut t = repair_at(NEAR, working);
    t.parts
        .building_mut(t.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .construction
        .as_mut()
        .unwrap()
        .builders = BuilderCount::of_data(1);
    t.parts
        .actions
        .of_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .repair_mut()
        .unwrap()
        .counted = true;
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(repair_of(&t).state.bare(), RepairState::Work);
    assert_eq!(
        the_house(&t),
        (
            300.0
                + work_amount(
                    WorkRate::new(1.0),
                    WorkValue::new(0.5),
                    Seconds::new(0.05),
                    Some(BuilderCount::of_data(1))
                )
                .get(),
            1,
            1
        )
    );
    let h = &mut t.players[1].objects[ListIndex::new(1)];
    h.life.hp = HitPoints::new(549.99);
    let id = h.id;
    t.parts.combat_mut(id).unwrap().armor_removed = ArmorPair::new(3, 4);
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(repair_of(&t).state.bare(), RepairState::Done);
    assert_eq!(the_house(&t), (550.0, 0, 0));
    assert_eq!(
        t.parts
            .combat_of(t.players[1].objects[ListIndex::new(1)].id)
            .armor_removed,
        ArmorPair::ZERO
    );
    assert_eq!(heard(&t), [NotifyKind::ACTION_COMPLETED]);
    // the player without the wood the building repair cost asks
    let mut t = repair_at(NEAR, working);
    t.players[1].attributes.resize(300, Amount::ZERO);
    t.players[1].attributes[BUILDING_REPAIR_COST] = Amount::new(1.0);
    t.players[1].attributes[..4].copy_from_slice(&[Amount::ZERO; 4]);
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(
        heard(&t),
        [NotifyKind::ACTION_INVALIDATED, NotifyKind::ACTION_COMPLETED]
    );
    assert_eq!(the_house(&t).0, 300.0);
    // the house moved out of reach: the repairer walks again, no longer counted
    let mut t = repair_at(NEAR, working);
    t.parts
        .building_mut(t.players[1].objects[ListIndex::new(1)].id)
        .unwrap()
        .construction
        .as_mut()
        .unwrap()
        .builders = BuilderCount::of_data(1);
    t.parts
        .actions
        .of_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .repair_mut()
        .unwrap()
        .counted = true;
    t.players[1].objects[ListIndex::new(1)].place_for_test(Position::new(30.0, 80.0, 0.0));
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    let r = repair_of(&t);
    assert_eq!(
        (
            r.state.bare(),
            r.state.move_to().copied().map(|m| m.state),
            r.counted,
            the_house(&t).1
        ),
        (
            RepairState::Move { move_to: None },
            Some(MoveTo::Start),
            false,
            0
        )
    );
}

/// State 10 waits a second (the world's dt off the timer), then faces a target within reach
/// or waits a second again.
#[test]
fn waiting_repairer_looks_again_each_second() {
    let waiting = |r: &mut RepairAction| {
        (r.state, r.timer) = (RepairState::Wait, Seconds::new(0.1));
    };
    let mut t = repair_at(Position::new(30.0, 95.0, 0.0), waiting);
    passable(&mut t);
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(
        (repair_of(&t).state, repair_of(&t).timer),
        (RepairState::Wait, Seconds::new(0.1 - 0.05))
    );
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(
        (repair_of(&t).state, repair_of(&t).timer),
        (RepairState::Wait, Seconds::new(1.0))
    );
    let mut t = repair_at(NEAR, |r| {
        (r.state, r.timer) = (RepairState::Wait, Seconds::ZERO);
    });
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(repair_of(&t).state, RepairState::Face);
}

/// Slot 8 (`FUN_140ad9700`): the point, the target let go (a counted repairer leaving its
/// repairers), the walk there; its end ends the Repair.
#[test]
fn move_sends_the_repairer_to_its_point() {
    let mut mover = walker();
    let mut r = RepairAction {
        state: RepairState::Work,
        counted: true,
        ..started()
    };
    let g = graphics();
    assert_eq!(
        r.take_point(
            Position::new(30.0, 5.0, 0.0),
            Some(&mut mover),
            &move_reads(&g)
        ),
        Ok((Some(object_id(20)), true))
    );
    assert_eq!(
        (
            r.state.bare(),
            r.target,
            r.state.move_to().copied().and_then(|m| m.goal()),
            r.counted
        ),
        (
            RepairState::WalkPoint { move_to: None },
            None,
            Some(Position::new(30.0, 5.0, 0.0)),
            false
        )
    );
    let mut t = repair_at(NEAR, |a| *a = r);
    t.parts
        .actions
        .of_mut(t.players[1].objects[ListIndex::new(0)].id)
        .unwrap()
        .list
        .repair_mut()
        .unwrap()
        .state = RepairState::WalkPoint {
        move_to: Some(MoveToAction::sub_in(MoveTo::Arrived)),
    };
    assert_eq!(repaired(&mut t), Ok(HeadCode::Keeps));
    assert_eq!(
        (repair_of(&t).state.bare(), heard(&t)),
        (RepairState::Done, vec![NotifyKind::ACTION_COMPLETED])
    );
}

/// Slot 7 (`FUN_140adaa00`, `set_state(2)`): a counted repairer is no longer counted (the
/// caller's to leave the repairers), the `MoveTo` goes, the target stays.
#[test]
fn halt_stops_the_repair_and_answers_whether_it_counted() {
    let mut mover = walker();
    let mut r = RepairAction {
        state: RepairState::Work,
        counted: true,
        ..started()
    };
    assert_eq!(r.halt(Some(&mut mover)), Ok(true));
    assert_eq!(
        (r.state, r.counted, r.state.move_to().copied(), r.target),
        (
            RepairState::Halted,
            false,
            None,
            Some(RepairTargetId::new(object_id(20)))
        )
    );
    assert_eq!(r.halt(None), Ok(false));
}

/// `set_state(4)` from a new target: the old `MoveTo` to the point goes, a new walk to the
/// target starts.
#[test]
fn walking_again_starts_new_move_to_the_target() {
    let mut mover = walker();
    let g = graphics();
    let r = started();
    let mut walking = RepairAction {
        state: RepairState::WalkPoint {
            move_to: Some(MoveToAction {
                state: MoveTo::Arrived,
                ..r.to_point(Position::new(3.0, 4.0, 0.0), &move_reads(&g))
                    .unwrap()
                    .unwrap()
            }),
        },
        ..r
    };
    assert_eq!(
        walking.walk_again(TARGET, Some(&mut mover), &move_reads(&g)),
        Ok(())
    );
    assert_eq!(
        walking.state,
        RepairState::Move {
            move_to: r.walk(&move_reads(&g), Some(TARGET)).unwrap()
        }
    );
}
