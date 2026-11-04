use super::*;
use crate::garrison::ContainerId;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::RestrictionId;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{ClassAmount, CombatPart, Master};
use crate::objects::unit::UnitFlags;
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::ByRelation;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::Relation;
use crate::players::UnitRelation;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_id_list;
use crate::test_ids::{attack_category, category_of};
use crate::testing::class;
use crate::testing::ids;
use crate::testing::picker::with_picker;
use crate::testing::roles;
use crate::testing::scene::SceneObject;
use crate::testing::scene::{Scene, walking};
use crate::testing::tuning;
use crate::unit_ai::OrderRecord;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::best_target::FoundFlags;
use crate::unit_ai::best_target::choice::{add_extra, kept_by_owner_and_class, may_attack};
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::{Percent, TargetingMode};

fn candidate(id: i32, distance: Distance, speed: f32) -> Candidate {
    Candidate {
        id: object_id(id),
        distance,
        category: attack_category(1),
        class: class::CLASS_6,
        speed: Speed::new(speed),
        armed: true,
        reach: Distance::new(1.0),
        damage: HitPoints::new(4.0),
        hp: HitPoints::new(40.0),
        reload: Seconds::new(2.0),
        workable: false,
        current: false,
    }
}

/// `FUN_140c66590`: the bounds, the counts, the closest moving object, the kill time (hit
/// points over damage over reload: 40 / 4 / 2 = 5), 5 or the largest float without damage,
/// the fixed classes and the current target's bonus.
#[test]
fn the_statistics_count_bound_and_time_each_object() {
    let mut st = Statistics::default();
    st.add(
        &candidate(1, Distance::new(3.0), 1.0),
        &tuning().best_target,
    );
    st.add(
        &Candidate {
            current: true,
            class: class::BUILDING,
            damage: HitPoints::ZERO,
            workable: true,
            ..candidate(2, Distance::new(0.5), 0.0)
        },
        &tuning().best_target,
    );
    st.add(
        &Candidate {
            damage: HitPoints::ZERO,
            category: None,
            armed: false,
            ..candidate(3, Distance::new(7.0), 2.0)
        },
        &tuning().best_target,
    );
    let gaps = |least, most| Bounds {
        least: Distance::new(least),
        most: Distance::new(most),
    };
    let times = |least, most| Bounds {
        least: KillTime::new(least),
        most: KillTime::new(most),
    };
    assert_eq!(st.distance, gaps(0.5, 7.0));
    assert_eq!(st.kill_time, times(5.0, f32::MAX));
    assert_eq!(st.moving_distance, gaps(3.0, 7.0));
    assert_eq!(st.moving_kill_time, times(5.0, f32::MAX));
    assert_eq!(
        [st.others, st.moving, st.armed, st.in_reach].map(EntryCount::get),
        [2, 2, 2, 1]
    );
    assert_eq!(st.by_category[category_of(1)], EntryCount::new(2));
    assert_eq!(st.closest_moving, (Distance::new(3.0), Some(object_id(1))));
    let kill: Vec<f32> = st.entries.iter().map(|e| e.kill_time.get()).collect();
    assert_eq!(kill, [5.0, 5.0, f32::MAX]);
    let fixed: Vec<bool> = st.entries.iter().map(|e| e.fixed).collect();
    assert_eq!(fixed, [false, true, false]);
    assert_eq!(st.entries[1].bonus, TargetScore::new(5.0));
}

/// `FUN_140c48150`, then the rank and the sort: a still entry alone in its kind takes the
/// whole kill score (10) and, its distance span below the epsilon, 75; moving entries share
/// their spans; the weight is its category's times 10. The sort ranks the highest first,
/// a tie to the lower id.
#[test]
fn the_scores_rank_and_sort_the_entries() {
    let mut st = Statistics::default();
    st.add(
        &candidate(5, Distance::new(2.0), 1.0),
        &tuning().best_target,
    );
    st.add(
        &Candidate {
            hp: HitPoints::new(80.0),
            ..candidate(4, Distance::new(6.0), 1.0)
        },
        &tuning().best_target,
    );
    st.add(
        &candidate(9, Distance::new(4.0), 0.0),
        &tuning().best_target,
    );
    let mut weights = ByCategory::default();
    weights[category_of(1)] = OrderWeight::new(0.5);
    let targeting = (TargetingMode::of_data(0), Percent::new(0));
    st.score(
        &weights,
        targeting,
        &mut |_| Ok(false),
        &tuning().best_target,
    )
    .unwrap();
    let scores: Vec<(f32, f32, f32, f32)> = st
        .entries
        .iter()
        .map(|e| {
            (
                e.kill_score.get(),
                e.distance_score.get(),
                e.weight.get(),
                e.total.get(),
            )
        })
        .collect();
    // moving kill times 5 and 10: 10 - (0/5)·10 = 10 and 10 - (5/5)·10 = 0; moving
    // distances 2 and 6: 75 and 0; the still entry: kill span 10 - 5 over all, distance
    // span 6 - 2 over all
    assert_eq!(
        scores,
        [
            (10.0, 75.0, 5.0, 90.0),
            (0.0, 0.0, 5.0, 5.0),
            (10.0, 37.5, 5.0, 52.5)
        ]
    );
    st.rank(&mut |e| Ok(i32::from(!e.moving)), &tuning().best_target)
        .unwrap();
    st.sort().unwrap();
    let ids: Vec<ObjectId> = st.order.iter().map(|&i| st.entries[i].id).collect();
    assert_eq!(ids, [object_id(9), object_id(5), object_id(4)]);
    // equal ranks go to the lower id
    for e in &mut st.entries {
        e.rank = TargetScore::new(1.0);
    }
    st.order = vec![0, 1, 2];
    st.sort().unwrap();
    let ids: Vec<ObjectId> = st.order.iter().map(|&i| st.entries[i].id).collect();
    assert_eq!(ids, [object_id(4), object_id(5), object_id(9)]);
}

/// The order history's weights: the younger an order, the more its category weighs; the
/// first one older than a minute ends the walk.
#[test]
fn the_order_history_weighs_the_last_minute_categories() {
    let record = |time, category| OrderRecord {
        time,
        category: attack_category(category),
        ..OrderRecord::default()
    };
    let w = order_weights(
        &[
            record(WorldTime::from_millis(100_000), 1),
            record(WorldTime::from_millis(70_000), 1),
            record(WorldTime::from_millis(30_000), 3),
            record(WorldTime::from_millis(99_000), 2),
        ],
        WorldTime::from_millis(100_000),
        &tuning().best_target,
    );
    assert_eq!(w[category_of(1)].get(), 1.0 + 30_000.0 / 60_000.0);
    assert_eq!(w[category_of(3)], OrderWeight::ZERO);
    assert_eq!(w[category_of(2)], OrderWeight::ZERO);
}

/// The soldier's master (74): a walking melee unit of class 6 and radius 0.25 on land
/// (restriction 7) that hits 4 every 2 s within 0.5, or within `range` and a blast of
/// `blast` (a ranged one's).
fn soldier_master(range: f32, blast: f32) -> Master {
    Master {
        id: master_id(74),
        kind: MasterKind::COMBAT,
        class: class::CLASS_6,
        radius: Footprint::new(0.25, 0.25),
        height: Distance::new(1.0),
        hit_points: MasterHitPoints::of_data(40),
        line_of_sight: Distance::new(6.0),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: Some(walking(Speed::new(1.0))),
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 4)],
            reload: Seconds::new(2.0),
            weapon_range: Distance::new(range),
            area_effect: Distance::new(blast),
            ..CombatPart::default()
        }),
        ..Master::default()
    }
}

/// An enemy's master `id` of `class`: a unit of radius 0.25 and 40 hit points that hits 1 (an
/// armed one), walking at 1 or (`walks` false) still.
fn enemy_master(id: i16, class: ClassId, walks: bool) -> Master {
    Master {
        id: master_id(id),
        kind: MasterKind::COMBAT,
        class,
        radius: Footprint::new(0.25, 0.25),
        height: Distance::new(1.0),
        hit_points: MasterHitPoints::of_data(40),
        terrain_restriction: Some(RestrictionId::named(7)),
        move_stats: walks.then(|| walking(Speed::new(1.0))),
        combat: Some(CombatPart {
            weapons: vec![ClassAmount::new(4, 1)],
            ..CombatPart::default()
        }),
        ..Master::default()
    }
}

/// The walking enemies' master (4, class 0).
fn walker() -> Master {
    enemy_master(4, class::CLASS_0, true)
}

/// Where an enemy stands from the soldier: the side of its footprint.
#[derive(Clone, Copy)]
enum Side {
    East,
    West,
    North,
    South,
}

/// The soldier stands at (20.5, 20.5); a footprint of radius 0.25 `gap` from its own toward
/// `side` stands here.
fn beside(side: Side, gap: f32) -> Position {
    let (near, far) = (20.0 - gap, 21.0 + gap);
    match side {
        Side::East => Position::new(far, 20.5, 0.0),
        Side::West => Position::new(near, 20.5, 0.0),
        Side::North => Position::new(20.5, far, 0.0),
        Side::South => Position::new(20.5, near, 0.0),
    }
}

/// Enemy `id` of player 2 and `master`, attackable and with a unit AI, `gap` from the soldier
/// toward `side`.
fn enemy(id: i32, master: Master, (side, gap): (Side, f32)) -> SceneObject {
    SceneObject {
        ai: Some(UnitAi::default()),
        flags: UnitFlags::ATTACKABLE,
        ..SceneObject::new(id, PlayerId::new(2), master, beside(side, gap))
    }
}

/// The battlefield: a 40 by 40 map of land and three players; player 1's soldier (object 1 of
/// `master`, its soldier's unit AI targeting `target`) and the `enemies`, those of `walled` in
/// rocks (the eight tiles around theirs: no quick path reaches them); player 3 is player 1's
/// ally.
fn battle(
    (master, target): (Master, Option<i32>),
    enemies: Vec<SceneObject>,
    walled: &[i32],
) -> World {
    let soldier = SceneObject {
        ai: Some(UnitAi {
            ai_kind: UnitAiKind::Soldier,
            current_target: target.map(object_id),
            ..UnitAi::default()
        }),
        ..SceneObject::new(1, PlayerId::new(1), master, Position::new(20.5, 20.5, 0.0))
    };
    let mut scene = Scene::open(GridSize::new(40, 40))
        .players(3)
        .relation(PlayerId::new(1), PlayerId::new(3), Relation::Ally)
        .object(soldier);
    for e in enemies {
        if walled.contains(&e.id.get()) {
            let tile = e.at.tile();
            let ring = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                .filter(|&d| d != (0, 0))
                .map(|(dx, dy)| Tile::new(tile.x() + dx, tile.y() + dy));
            scene = scene.rocks(ring);
        }
        scene = scene.object(e);
    }
    scene.build()
}

/// Runs `f` on the soldier's field over `w`: its picker ([`with_picker`]) and its paths (slot
/// 0x3d0), answered by whether `open` holds their objects and logged in order; `f`'s answer
/// and the paths asked.
fn fielded<R>(
    w: &mut World,
    open: &[i32],
    f: impl FnOnce(&mut AttackField<'_, '_, '_>) -> R,
) -> (R, Vec<ObjectId>) {
    let mut asked = Vec::new();
    let answer = with_picker(w, (PlayerId::new(1), object_id(1)), |picker, world| {
        let mut reach = |_: &mut Spatial, id: ObjectId, _: Distance| {
            asked.push(id);
            Ok(open.contains(&id.get()))
        };
        f(&mut AttackField {
            picker,
            reach: &mut reach,
            world,
        })
    });
    (answer, asked)
}

/// `bestUnitToAttack` of searcher `me` over `w`'s soldier's field ([`fielded`]) and the lists
/// of relation 4 (enemies) `enemies`: the choice and the paths asked.
fn best(
    w: &mut World,
    me: &Searcher,
    ask: &Ask,
    enemies: &[i32],
    open: &[i32],
) -> (Choice, Vec<ObjectId>) {
    let lists = ByRelation::new([
        vec![],
        vec![],
        vec![],
        vec![],
        object_id_list(enemies.iter().copied()),
    ]);
    let (c, asked) = fielded(w, open, |field| {
        best_unit_to_attack(me, ask, &lists, field, &roles(), &tuning().best_target)
    });
    (c.unwrap(), asked)
}

/// An idle check with path checks over every class.
fn idle_ask() -> Ask {
    Ask {
        idle: true,
        paths: 1,
        any_class: true,
        ..Ask::default()
    }
}

/// What `bestUnitToAttack` reads of the soldier's AI and master as its search hands them in.
fn soldier() -> Searcher {
    Searcher {
        id: object_id(1),
        target: None,
        target_classes: [None, None],
        order: None,
        action: None,
        master: master_id(74),
        class: Some(class::CLASS_6),
        kind: MasterKind::COMBAT,
        attack_priority: AttackPriority::PRIORITY_1,
        can_move: true,
        sight: Distance::new(12.0),
        range: Distance::new(0.5),
        weapon_range: Distance::new(0.5),
        speed: Speed::new(1.0),
        now: WorldTime::from_millis(10_000),
        ..Searcher::default()
    }
}

/// The melee soldier's master and target.
fn melee(target: Option<i32>) -> (Master, Option<i32>) {
    (soldier_master(0.5, 0.0), target)
}

/// Three walking enemies 3 (7, east), 9 (8, west) and 5 (9, north, of 10 hit points) away.
fn three_enemies() -> Vec<SceneObject> {
    vec![
        enemy(7, walker(), (Side::East, 3.0)),
        enemy(8, walker(), (Side::West, 9.0)),
        SceneObject {
            hp: HitPoints::new(10.0),
            ..enemy(9, walker(), (Side::North, 5.0))
        },
    ]
}

/// `bestUnitToAttack` over three enemies (an idle check, path checks on): the statistics
/// rank them (the nearest's distance score outweighs the weakest's kill score: 7, 9, 8); the
/// quick paths of the first three (moving objects) are asked: 7's and 8's are walled in, 9's
/// answers without a path asked.
#[test]
fn the_best_unit_to_attack_ranks_then_takes_the_first_quick_path() {
    let mut w = battle(melee(None), three_enemies(), &[7, 8]);
    let (c, asked) = best(&mut w, &soldier(), &idle_ask(), &[8, 7, 9], &[]);
    assert_eq!(
        (c.target, asked, c.retries),
        (Some(object_id(9)), vec![], vec![])
    );
    // without a quick path the paths follow; 7's fails (a moving object: ignored for 1 s)
    let mut w = battle(melee(None), three_enemies(), &[7, 8, 9]);
    let (c, asked) = best(&mut w, &soldier(), &idle_ask(), &[8, 7, 9], &[9]);
    assert_eq!(c.target, Some(object_id(9)));
    assert_eq!(asked, [object_id(7), object_id(9)]);
    assert_eq!(c.retries, [(object_id(7), WorldTime::from_millis(11_000))]);
}

/// Each of the first three whose path fails is ignored for 1 s (a moving object) or 2 s
/// (a still one), and nothing found answers −1; a still one's quick path is not asked while
/// another moves. A retarget entry's target waits for the retry walk, which runs only without
/// other statistics.
#[test]
fn failed_paths_become_retarget_entries() {
    let still = SceneObject {
        hp: HitPoints::new(10.0),
        ..enemy(
            9,
            enemy_master(5, class::CLASS_0, false),
            (Side::North, 5.0),
        )
    };
    let mut w = battle(
        melee(None),
        vec![enemy(7, walker(), (Side::East, 3.0)), still],
        &[7],
    );
    let (c, asked) = best(&mut w, &soldier(), &idle_ask(), &[7, 9], &[]);
    assert_eq!(c.target, None);
    assert_eq!(asked, [object_id(7), object_id(9)]);
    let mut retries = c.retries;
    retries.sort_unstable();
    assert_eq!(
        retries,
        [
            (object_id(7), WorldTime::from_millis(11_000)),
            (object_id(9), WorldTime::from_millis(12_000))
        ]
    );
    let me = Searcher {
        retries: vec![object_id(9)],
        ..soldier()
    };
    let (c, asked) = best(&mut w, &me, &idle_ask(), &[7, 9], &[]);
    assert_eq!((c.target, asked), (None, vec![object_id(7)]));
    // with 7 a retarget entry too, the retry walk takes both
    let me = Searcher {
        retries: vec![object_id(9), object_id(7)],
        ..soldier()
    };
    let (c, _) = best(&mut w, &me, &idle_ask(), &[7, 9], &[9]);
    assert_eq!(c.target, Some(object_id(9)));
}

/// The lists' records pass the objects without a unit AI, inside another, dead or allied
/// (player 3's), and those beyond the sight (twice the line of sight: 12) or under the least
/// range; the one left answers by its quick path.
#[test]
fn the_lists_pass_over_what_the_unit_may_not_attack() {
    let enemies = vec![
        SceneObject {
            ai: None,
            ..enemy(30, walker(), (Side::West, 3.0))
        },
        SceneObject {
            inside: Some(ContainerId::new(object_id(36))),
            ..enemy(31, walker(), (Side::North, 3.0))
        },
        SceneObject {
            state: ObjectState::Dying,
            ..enemy(32, walker(), (Side::South, 3.0))
        },
        SceneObject {
            player: PlayerId::new(3),
            ..enemy(33, walker(), (Side::South, 6.0))
        },
        enemy(34, walker(), (Side::North, 12.5)),
        enemy(35, walker(), (Side::West, 0.125)),
        enemy(36, walker(), (Side::East, 11.875)),
    ];
    let mut w = battle(melee(None), enemies, &[]);
    let me = Searcher {
        min_range: Distance::new(0.2),
        ..soldier()
    };
    let ids: Vec<i32> = (30..=36).collect();
    let (c, asked) = best(&mut w, &me, &idle_ask(), &ids, &[]);
    assert_eq!((c.target, asked), (Some(object_id(36)), vec![]));
}

/// Walls (class 27) and gates (class 39): still objects of 100 hit points.
fn wall(id: i32, master: i16, class: ClassId, (side, gap): (Side, f32)) -> SceneObject {
    SceneObject {
        hp: HitPoints::new(100.0),
        ..enemy(id, enemy_master(master, class, false), (side, gap))
    }
}

/// A unit that attacks walls (its last target a wall) and finds only walls and gates takes
/// the closest gate; one that does not passes them over.
#[test]
fn walls_and_gates_count_for_unit_that_attacks_walls() {
    let mut w = battle(
        melee(None),
        vec![
            wall(20, 27, class::WALL, (Side::East, 2.0)),
            wall(21, 39, class::GATE, (Side::West, 4.0)),
            wall(22, 39, class::GATE, (Side::North, 3.0)),
        ],
        &[],
    );
    let me = Searcher {
        target_classes: [None, Some(class::WALL)],
        ..soldier()
    };
    let (c, _) = best(&mut w, &me, &idle_ask(), &[20, 21, 22], &[]);
    assert_eq!(c.target, Some(object_id(22)));
    let (c, _) = best(&mut w, &soldier(), &idle_ask(), &[20, 21, 22], &[]);
    assert_eq!(c.target, None);
}

/// `FUN_140c4c8e0`'s switch: the archers' class 0 is 3, but masters 6 and 7 (the crossbowman
/// lines' first) are 4; the siege's 13 is 6, but the rams' masters are 5; a class outside the
/// table is −1.
#[test]
fn the_categories_follow_the_classes_and_their_exceptions() {
    assert_eq!(
        category(master_id(4), class::CLASS_0, &roles()),
        attack_category(3)
    );
    assert_eq!(
        category(ids::ID_6, class::CLASS_0, &roles()),
        attack_category(4)
    );
    assert_eq!(
        category(ids::ID_7, class::CLASS_23, &roles()),
        attack_category(4)
    );
    assert_eq!(
        category(ids::VILLAGER_MALE, class::VILLAGER, &roles()),
        attack_category(7)
    );
    assert_eq!(
        category(master_id(74), class::CLASS_6, &roles()),
        attack_category(1)
    );
    assert_eq!(
        category(ids::ID_1744, class::CLASS_6, &roles()),
        attack_category(5)
    );
    assert_eq!(
        category(master_id(280), class::SIEGE, &roles()),
        attack_category(6)
    );
    assert_eq!(
        category(ids::ID_35, class::SIEGE, &roles()),
        attack_category(5)
    );
    assert_eq!(
        category(ids::ID_1258, class::CLASS_55, &roles()),
        attack_category(5)
    );
    assert_eq!(
        category(ids::TOWN_CENTRE, class::BUILDING, &roles()),
        attack_category(0)
    );
    assert_eq!(
        category(ids::ID_448, class::CLASS_47, &roles()),
        attack_category(1)
    );
    assert_eq!(category(master_id(1), class::ARTIFACT, &roles()), None);
    assert_eq!(category(master_id(1), class::CLASS_63, &roles()), None);
    assert_eq!(category(master_id(1), class::NO_CLASS, &roles()), None);
}

/// `bestUnitToAttack`'s extra objects ([`add_extra`]): each the world holds but
/// the current target counts at its distance; one under the least range counts beyond the
/// weapon range (its gap plus the weapon range plus 1), and not at all for a unit that does
/// not move.
#[test]
fn an_extra_object_under_the_least_range_counts_beyond_the_weapon_range() {
    let mut w = battle(
        melee(Some(42)),
        vec![
            enemy(40, walker(), (Side::East, 3.0)),
            enemy(41, walker(), (Side::West, 0.125)),
            enemy(42, walker(), (Side::North, 4.0)),
        ],
        &[],
    );
    let ask = Ask {
        extra: object_id_list([40, 41, 42, 43]),
        ..Ask::default()
    };
    let me = Searcher {
        target: Some(object_id(42)),
        min_range: Distance::new(0.2),
        ..soldier()
    };
    let mut counted = |me: &Searcher| {
        let mut st = Statistics::default();
        fielded(&mut w, &[], |field| {
            add_extra(me, &ask, &mut st, field, &roles(), &tuning().best_target)
        })
        .0
        .unwrap();
        st.entries
            .iter()
            .map(|e| (e.id, e.distance.get()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        counted(&me),
        [(object_id(40), 3.0), (object_id(41), 0.125_f32 + 0.5 + 1.0)]
    );
    let still = Searcher {
        speed: Speed::ZERO,
        ..me
    };
    assert_eq!(counted(&still), [(object_id(40), 3.0)]);
}

/// A gate answers at once only when nothing but walls and gates counts: with an enemy among
/// them the choice goes on to the quick paths, which take the moving enemy.
#[test]
fn gate_answers_at_once_only_among_walls() {
    let mut w = battle(
        melee(None),
        vec![
            wall(20, 27, class::WALL, (Side::East, 2.0)),
            wall(22, 39, class::GATE, (Side::West, 3.0)),
            enemy(7, walker(), (Side::North, 5.0)),
        ],
        &[],
    );
    let me = Searcher {
        target_classes: [None, Some(class::WALL)],
        ..soldier()
    };
    let (c, asked) = best(&mut w, &me, &idle_ask(), &[20, 22, 7], &[]);
    assert_eq!((c.target, asked), (Some(object_id(7)), vec![]));
}

/// Outside an idle check the current target answers by its path when it moves and engages
/// (without path checks at once); its path failing, the quick paths follow (the target's is
/// walled in, 9's answers). An idle check takes no such shortcut.
#[test]
fn the_current_target_answers_by_its_path_outside_an_idle_check() {
    let mut w = battle(
        melee(Some(7)),
        vec![
            enemy(7, walker(), (Side::East, 3.0)),
            enemy(9, walker(), (Side::West, 5.0)),
        ],
        &[7],
    );
    let ask = Ask {
        paths: 1,
        any_class: true,
        ..Ask::default()
    };
    let me = Searcher {
        target: Some(object_id(7)),
        ..soldier()
    };
    let (c, asked) = best(&mut w, &me, &ask, &[7, 9], &[7]);
    assert_eq!((c.target, asked), (Some(object_id(7)), vec![object_id(7)]));
    let at_once = Ask {
        paths: 0,
        ..ask.clone()
    };
    let (c, asked) = best(&mut w, &me, &at_once, &[7, 9], &[]);
    assert_eq!((c.target, asked), (Some(object_id(7)), vec![]));
    let (c, asked) = best(&mut w, &me, &ask, &[7, 9], &[]);
    assert_eq!((c.target, asked[0]), (Some(object_id(9)), object_id(7)));
    let idle = Ask { idle: true, ..ask };
    let (c, asked) = best(&mut w, &me, &idle, &[7, 9], &[7]);
    assert_eq!(c.target, Some(object_id(9)));
    assert!(!asked.contains(&object_id(7)));
}

/// The world's checks of a candidate ([`may_attack`]) in the game's order. One without a unit
/// AI, dead, allied or inside another fails before any; the attack check (an untargetable
/// enemy fails it); a combat unit's friends' scan, which a ranged soldier with a blast reaches
/// (the scan of the tiles around the target is not modelled: its error shows it ran) and a
/// unit of another type skips; and the neutral test of list 3 alone (a wall is spared).
#[test]
fn candidate_passes_the_attack_check_the_friends_scan_and_the_neutral_test_in_order() {
    let enemies = || {
        vec![
            enemy(7, walker(), (Side::East, 3.0)),
            SceneObject {
                flags: UnitFlags::ATTACKABLE | UnitFlags::UNTARGETABLE,
                ..enemy(8, walker(), (Side::West, 3.0))
            },
            wall(10, 27, class::WALL, (Side::North, 3.0)),
            SceneObject {
                ai: None,
                ..enemy(11, walker(), (Side::South, 3.0))
            },
            SceneObject {
                state: ObjectState::Dying,
                ..enemy(12, walker(), (Side::South, 5.0))
            },
            SceneObject {
                player: PlayerId::new(3),
                ..enemy(13, walker(), (Side::South, 7.0))
            },
            SceneObject {
                inside: Some(ContainerId::new(object_id(7))),
                ..enemy(14, walker(), (Side::South, 9.0))
            },
        ]
    };
    let checked = |w: &mut World, me: &Searcher, id: i32, relation| {
        fielded(w, &[], |field| {
            let t = field.found_object(object_id(id))?.expect("a held object");
            may_attack(me, (object_id(id), &t), relation, field)
        })
        .0
    };
    let me = soldier();
    let mut w = battle(melee(None), enemies(), &[]);
    for (id, relation, pass) in [
        (7, UnitRelation::Enemy, true),
        (7, UnitRelation::Neutral, true),
        (10, UnitRelation::Enemy, true),
        (10, UnitRelation::Neutral, false),
        (8, UnitRelation::Enemy, false),
    ] {
        assert_eq!(checked(&mut w, &me, id, relation), Ok(pass), "{id}");
    }
    let mut w = battle((soldier_master(4.0, 1.0), None), enemies(), &[]);
    assert!(checked(&mut w, &me, 7, UnitRelation::Enemy).is_err());
    assert!(checked(&mut w, &me, 10, UnitRelation::Neutral).is_err());
    assert_eq!(checked(&mut w, &me, 8, UnitRelation::Enemy), Ok(false));
    for id in 11..=14 {
        assert_eq!(
            checked(&mut w, &me, id, UnitRelation::Enemy),
            Ok(false),
            "{id}"
        );
    }
    let building = Searcher {
        kind: MasterKind::BUILDING,
        ..soldier()
    };
    assert_eq!(
        checked(&mut w, &building, 7, UnitRelation::Neutral),
        Ok(true)
    );
    assert_eq!(
        checked(&mut w, &building, 10, UnitRelation::Neutral),
        Ok(false)
    );
}

/// An enemy as the owner and class rules read it: player 2's armed walking unit of class 0,
/// 3 away.
fn seen_enemy() -> FoundObject {
    FoundObject {
        owner: PlayerId::new(2),
        state: ObjectState::Alive,
        flags: FoundFlags::HAS_AI | FoundFlags::TARGETABLE | FoundFlags::ATTACKABLE,
        master: master_id(4),
        class: class::CLASS_0,
        kind: MasterKind::COMBAT,
        distance: Distance::new(3.0),
        speed: Speed::new(1.0),
        ..FoundObject::default()
    }
}

/// The owner and class rules ([`kept_by_owner_and_class`]): without `any_class` a building is
/// passed over; a computer player's unit passes over Gaia's objects that hold something or that
/// others take by sight, and boats unless it is a boat itself, of class 52 or a boat hunter (the
/// castle); a human player's unit keeps them all.
#[test]
fn computer_unit_passes_over_gaia_holdings_and_boats() {
    let seen = seen_enemy();
    let any = Ask {
        any_class: true,
        ..Ask::default()
    };
    let human = soldier();
    let computer = Searcher {
        player_kind: PlayerKind::Computer,
        ..soldier()
    };
    let house = FoundObject {
        class: class::BUILDING,
        ..seen
    };
    assert!(!kept_by_owner_and_class(
        &human,
        &Ask::default(),
        &house,
        &roles()
    ));
    assert!(kept_by_owner_and_class(&human, &any, &house, &roles()));
    let gaia = FoundObject {
        owner: PlayerId::new(0),
        ..seen
    };
    let holding = FoundObject {
        held: Amount::new(5.0),
        ..gaia
    };
    let taken = FoundObject {
        capture: CaptureKind::BySight,
        ..gaia
    };
    assert!(kept_by_owner_and_class(&computer, &any, &gaia, &roles()));
    // a held amount that is not a number counts as held (`+0x310 <= 0` fails)
    let unordered = FoundObject {
        held: Amount::new(f32::NAN),
        ..gaia
    };
    for t in [holding, taken, unordered] {
        assert!(!kept_by_owner_and_class(&computer, &any, &t, &roles()));
        assert!(kept_by_owner_and_class(&human, &any, &t, &roles()));
    }
    let boat = FoundObject {
        class: class::WARSHIP,
        ..seen
    };
    assert!(!kept_by_owner_and_class(&computer, &any, &boat, &roles()));
    assert!(kept_by_owner_and_class(&human, &any, &boat, &roles()));
    for me in [
        Searcher {
            class: Some(class::TOWER),
            ..computer.clone()
        },
        Searcher {
            master: ids::CASTLE,
            ..computer.clone()
        },
        Searcher {
            class: Some(class::TRADE_BOAT),
            ..computer
        },
    ] {
        assert!(kept_by_owner_and_class(&me, &any, &boat, &roles()));
    }
}
