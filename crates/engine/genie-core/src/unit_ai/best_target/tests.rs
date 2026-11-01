use super::*;
use crate::map::Distance;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ClassId;
use crate::objects::masters::class;
use crate::objects::masters::master_type;
use crate::players::Amount;
use crate::players::ByRelation;
use crate::players::PlayerId;
use crate::players::UnitRelation;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_id_list;
use crate::test_ids::{attack_category, category_of};
use crate::unit_ai::best_target::choice::{add_extra, kept_by_owner_and_class, may_attack};
use crate::world::Seconds;
use crate::world::WorldTime;

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
    st.add(&candidate(1, Distance::new(3.0), 1.0));
    st.add(&Candidate {
        current: true,
        class: class::BUILDING,
        damage: HitPoints::ZERO,
        workable: true,
        ..candidate(2, Distance::new(0.5), 0.0)
    });
    st.add(&Candidate {
        damage: HitPoints::ZERO,
        category: None,
        armed: false,
        ..candidate(3, Distance::new(7.0), 2.0)
    });
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
    st.add(&candidate(5, Distance::new(2.0), 1.0));
    st.add(&Candidate {
        hp: HitPoints::new(80.0),
        ..candidate(4, Distance::new(6.0), 1.0)
    });
    st.add(&candidate(9, Distance::new(4.0), 0.0));
    let mut weights = ByCategory::default();
    weights[category_of(1)] = OrderWeight::new(0.5);
    let targeting = (
        crate::world::TargetingMode::of_data(0),
        crate::world::Percent::new(0),
    );
    st.score(&weights, targeting, &mut |_| Ok(false)).unwrap();
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
    st.rank(&mut |e| Ok(i32::from(!e.moving))).unwrap();
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
fn the_order_history_weighs_the_last_minute_s_categories() {
    let record = |time, category| crate::unit_ai::OrderRecord {
        time,
        category: attack_category(category),
        ..crate::unit_ai::OrderRecord::default()
    };
    let w = order_weights(
        &[
            record(WorldTime::from_millis(100_000), 1),
            record(WorldTime::from_millis(70_000), 1),
            record(WorldTime::from_millis(30_000), 3),
            record(WorldTime::from_millis(99_000), 2),
        ],
        WorldTime::from_millis(100_000),
    );
    assert_eq!(w[category_of(1)].get(), 1.0 + 30_000.0 / 60_000.0);
    assert_eq!(w[category_of(3)], OrderWeight::ZERO);
    assert_eq!(w[category_of(2)], OrderWeight::ZERO);
}

/// A world of objects by id: each a [`Thing`] and its candidate's values; the paths that
/// succeed and the calls made; the object checks it refuses (`'a'` the attack check, `'f'`
/// the friends' scan, `'n'` the neutral test) and the ones made, in order.
#[derive(Default)]
struct Field {
    things: std::collections::BTreeMap<ObjectId, (Thing, Candidate)>,
    quick: Vec<ObjectId>,
    paths: Vec<ObjectId>,
    asked: Vec<(char, ObjectId)>,
    refused: Vec<char>,
    checked: Vec<char>,
}

impl Field {
    /// Check `c` is made: it passes unless refused.
    fn check(&mut self, c: char) -> bool {
        self.checked.push(c);
        !self.refused.contains(&c)
    }
}

impl Battlefield for Field {
    fn thing(&mut self, id: ObjectId) -> Result<Option<Thing>, crate::GenieError> {
        Ok(self.things.get(&id).map(|t| t.0))
    }
    fn can_attack(&mut self, _: ObjectId) -> Result<bool, crate::GenieError> {
        Ok(self.check('a'))
    }
    fn attacks_neutral(&mut self, _: ClassId) -> Result<bool, crate::GenieError> {
        Ok(self.check('n'))
    }
    fn spares_friends(&mut self, _: ObjectId) -> Result<bool, crate::GenieError> {
        Ok(self.check('f'))
    }
    fn shuns(&mut self, _: ObjectId) -> Result<bool, crate::GenieError> {
        Ok(false)
    }
    fn candidate(
        &mut self,
        id: ObjectId,
        distance: Distance,
        category: Option<AttackCategory>,
    ) -> Result<Candidate, crate::GenieError> {
        Ok(Candidate {
            distance,
            category,
            ..self.things[&id].1
        })
    }
    fn armored(&mut self, _: ObjectId) -> Result<bool, crate::GenieError> {
        Ok(false)
    }
    fn weapon_range(&mut self, _: ObjectId) -> Result<Distance, crate::GenieError> {
        Ok(Distance::ZERO)
    }
    fn engages(&mut self, _: ObjectId) -> Result<bool, crate::GenieError> {
        Ok(true)
    }
    fn quick_path(&mut self, id: ObjectId, _: Distance) -> Result<bool, crate::GenieError> {
        self.asked.push(('q', id));
        Ok(self.quick.contains(&id))
    }
    fn path(&mut self, id: ObjectId, _: Distance) -> Result<bool, crate::GenieError> {
        self.asked.push(('p', id));
        Ok(self.paths.contains(&id))
    }
}

fn soldier() -> Searcher {
    Searcher {
        id: object_id(1),
        target: None,
        target_classes: [None, None],
        order: None,
        action: None,
        master: master_id(74),
        class: Some(class::CLASS_6),
        kind: master_type::COMBAT,
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

fn enemy(id: i32, class: ClassId, distance: Distance, hp: HitPoints) -> (Thing, Candidate) {
    (
        Thing {
            owner: PlayerId::new(2),
            state: ObjectState::Alive,
            has_ai: true,
            master: master_id(4),
            class,
            kind: master_type::COMBAT,
            targetable: true,
            attackable: true,
            distance,
            speed: Speed::new(1.0),
            ..Thing::default()
        },
        Candidate {
            speed: Speed::new(1.0),
            hp,
            ..candidate(id, distance, 1.0)
        },
    )
}

/// `bestUnitToAttack` over three enemies (an idle check, path checks on): the statistics
/// rank them (the nearest's distance score outweighs the weakest's kill score: 7, 9, 8); the
/// quick paths of the first three (moving objects) are asked, 9's answers.
#[test]
fn the_best_unit_to_attack_ranks_then_takes_the_first_quick_path() {
    let mut field = Field::default();
    let _ = field.things.insert(
        object_id(7),
        enemy(7, class::CLASS_0, Distance::new(3.0), HitPoints::new(40.0)),
    );
    let _ = field.things.insert(
        object_id(8),
        enemy(8, class::CLASS_0, Distance::new(9.0), HitPoints::new(40.0)),
    );
    let _ = field.things.insert(
        object_id(9),
        enemy(9, class::CLASS_0, Distance::new(5.0), HitPoints::new(10.0)),
    );
    field.quick = vec![object_id(9)];
    let lists = ByRelation::new([vec![], vec![], vec![], vec![], object_id_list([8, 7, 9])]);
    let ask = Ask {
        idle: true,
        paths: 1,
        any_class: true,
        ..Ask::default()
    };
    let c = best_unit_to_attack(&soldier(), &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, Some(object_id(9)));
    assert_eq!(field.asked, [('q', object_id(7)), ('q', object_id(9))]);
    assert_eq!(c.retries, []);
    // without a quick path the paths follow; 7's fails (a moving object: ignored for 1 s)
    field.quick.clear();
    field.paths = vec![object_id(9)];
    field.asked.clear();
    let c = best_unit_to_attack(&soldier(), &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, Some(object_id(9)));
    assert_eq!(
        field.asked,
        [
            ('q', object_id(7)),
            ('q', object_id(9)),
            ('q', object_id(8)),
            ('p', object_id(7)),
            ('p', object_id(9))
        ]
    );
    assert_eq!(c.retries, [(object_id(7), WorldTime::from_millis(11_000))]);
}

/// Each of the first three whose path fails is ignored for 1 s (a moving object) or 2 s
/// (a still one), and nothing found answers −1. A retarget entry's target waits for the
/// retry walk, which runs only without other statistics.
#[test]
fn failed_paths_become_retarget_entries() {
    let mut field = Field::default();
    let _ = field.things.insert(
        object_id(7),
        enemy(7, class::CLASS_0, Distance::new(3.0), HitPoints::new(40.0)),
    );
    let mut still = enemy(9, class::CLASS_0, Distance::new(5.0), HitPoints::new(10.0));
    still.0.speed = Speed::ZERO;
    still.1.speed = Speed::ZERO;
    let _ = field.things.insert(object_id(9), still);
    let lists = ByRelation::new([vec![], vec![], vec![], vec![], object_id_list([7, 9])]);
    let ask = Ask {
        idle: true,
        paths: 1,
        any_class: true,
        ..Ask::default()
    };
    let c = best_unit_to_attack(&soldier(), &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, None);
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
    field.asked.clear();
    let c = best_unit_to_attack(&me, &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, None);
    assert_eq!(field.asked, [('q', object_id(7)), ('p', object_id(7))]);
    // with 7 a retarget entry too, the retry walk takes both
    let me = Searcher {
        retries: vec![object_id(9), object_id(7)],
        ..soldier()
    };
    field.paths = vec![object_id(9)];
    field.asked.clear();
    let c = best_unit_to_attack(&me, &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, Some(object_id(9)));
}

/// The lists' records pass the objects without a unit AI, inside another, dead or allied,
/// and those beyond the sight (twice the line of sight: 12) or under the least range; the
/// one left answers.
#[test]
fn the_lists_pass_over_what_the_unit_may_not_attack() {
    let mut field = Field::default();
    let mut no_ai = enemy(30, class::CLASS_0, Distance::new(3.0), HitPoints::new(40.0));
    no_ai.0.has_ai = false;
    let mut inside = enemy(31, class::CLASS_0, Distance::new(3.0), HitPoints::new(40.0));
    inside.0.inside = true;
    let mut dead = enemy(32, class::CLASS_0, Distance::new(3.0), HitPoints::new(40.0));
    dead.0.state = ObjectState::Dying;
    let mut ally = enemy(33, class::CLASS_0, Distance::new(3.0), HitPoints::new(40.0));
    ally.0.allied = true;
    for (id, t) in [(30, no_ai), (31, inside), (32, dead), (33, ally)] {
        let _ = field.things.insert(object_id(id), t);
    }
    let _ = field.things.insert(
        object_id(34),
        enemy(
            34,
            class::CLASS_0,
            Distance::new(12.5),
            HitPoints::new(40.0),
        ),
    );
    let _ = field.things.insert(
        object_id(35),
        enemy(35, class::CLASS_0, Distance::new(0.1), HitPoints::new(40.0)),
    );
    let _ = field.things.insert(
        object_id(36),
        enemy(
            36,
            class::CLASS_0,
            Distance::new(11.9),
            HitPoints::new(40.0),
        ),
    );
    field.quick = object_id_list(30..=36);
    let lists = ByRelation::new([vec![], vec![], vec![], vec![], object_id_list(30..=36)]);
    let ask = Ask {
        idle: true,
        paths: 1,
        any_class: true,
        ..Ask::default()
    };
    let me = Searcher {
        min_range: Distance::new(0.2),
        ..soldier()
    };
    let c = best_unit_to_attack(&me, &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, Some(object_id(36)));
    assert_eq!(field.asked, [('q', object_id(36))]);
}

/// A unit that attacks walls (its last target a wall) and finds only walls and gates takes
/// the closest gate; one that does not passes them over.
#[test]
fn walls_and_gates_count_for_a_unit_that_attacks_walls() {
    let mut field = Field::default();
    let _ = field.things.insert(
        object_id(20),
        enemy(20, class::WALL, Distance::new(2.0), HitPoints::new(100.0)),
    );
    let _ = field.things.insert(
        object_id(21),
        enemy(21, class::GATE, Distance::new(4.0), HitPoints::new(100.0)),
    );
    let _ = field.things.insert(
        object_id(22),
        enemy(22, class::GATE, Distance::new(3.0), HitPoints::new(100.0)),
    );
    let lists = ByRelation::new([vec![], vec![], vec![], vec![], object_id_list([20, 21, 22])]);
    let ask = Ask {
        idle: true,
        paths: 1,
        any_class: true,
        ..Ask::default()
    };
    let me = Searcher {
        target_classes: [None, Some(class::WALL)],
        ..soldier()
    };
    let c = best_unit_to_attack(&me, &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, Some(object_id(22)));
    let c = best_unit_to_attack(&soldier(), &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, None);
}

/// `FUN_140c4c8e0`'s switch: the archers' class 0 is 3, but masters 6 and 7 (the crossbowman
/// lines' first) are 4; the siege's 13 is 6, but the rams' masters are 5; a class outside the
/// table is −1.
#[test]
fn the_categories_follow_the_classes_and_their_exceptions() {
    assert_eq!(category(master_id(4), class::CLASS_0), attack_category(3));
    assert_eq!(category(ids::ID_6, class::CLASS_0), attack_category(4));
    assert_eq!(category(ids::ID_7, class::CLASS_23), attack_category(4));
    assert_eq!(
        category(ids::VILLAGER_MALE, class::VILLAGER),
        attack_category(7)
    );
    assert_eq!(category(master_id(74), class::CLASS_6), attack_category(1));
    assert_eq!(category(ids::ID_1744, class::CLASS_6), attack_category(5));
    assert_eq!(category(master_id(280), class::SIEGE), attack_category(6));
    assert_eq!(category(ids::ID_35, class::SIEGE), attack_category(5));
    assert_eq!(category(ids::ID_1258, class::CLASS_55), attack_category(5));
    assert_eq!(
        category(ids::TOWN_CENTRE, class::BUILDING),
        attack_category(0)
    );
    assert_eq!(category(ids::ID_448, class::CLASS_47), attack_category(1));
    assert_eq!(category(master_id(1), class::ARTIFACT), None);
    assert_eq!(category(master_id(1), class::CLASS_63), None);
    assert_eq!(category(master_id(1), class::NO_CLASS), None);
}

/// `bestUnitToAttack`'s extra objects ([`add_extra`]): each the world holds but
/// the current target counts at its distance; one under the least range counts beyond the
/// weapon range (its gap plus the weapon range plus 1), and not at all for a unit that does
/// not move.
#[test]
fn an_extra_object_under_the_least_range_counts_beyond_the_weapon_range() {
    let mut field = Field::default();
    for (id, d) in [(40, 3.0), (41, 0.1), (42, 4.0)] {
        let _ = field.things.insert(
            object_id(id),
            enemy(id, class::CLASS_0, Distance::new(d), HitPoints::new(40.0)),
        );
    }
    let ask = Ask {
        extra: vec![object_id(40), object_id(41), object_id(42), object_id(43)],
        ..Ask::default()
    };
    let me = Searcher {
        target: Some(object_id(42)),
        min_range: Distance::new(0.2),
        ..soldier()
    };
    let counted = |me: &Searcher, field: &mut Field| {
        let mut st = Statistics::default();
        add_extra(me, &ask, &mut st, field).unwrap();
        st.entries
            .iter()
            .map(|e| (e.id, e.distance.get()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        counted(&me, &mut field),
        [(object_id(40), 3.0), (object_id(41), 0.1_f32 + 0.5 + 1.0)]
    );
    let still = Searcher {
        speed: Speed::ZERO,
        ..me
    };
    assert_eq!(counted(&still, &mut field), [(object_id(40), 3.0)]);
}

/// A gate answers at once only when nothing but walls and gates counts: with an enemy among
/// them the choice goes on to the quick paths.
#[test]
fn a_gate_answers_at_once_only_among_walls() {
    let mut field = Field::default();
    let _ = field.things.insert(
        object_id(20),
        enemy(20, class::WALL, Distance::new(2.0), HitPoints::new(100.0)),
    );
    let _ = field.things.insert(
        object_id(22),
        enemy(22, class::GATE, Distance::new(3.0), HitPoints::new(100.0)),
    );
    let _ = field.things.insert(
        object_id(7),
        enemy(7, class::CLASS_0, Distance::new(5.0), HitPoints::new(40.0)),
    );
    field.quick = vec![object_id(7)];
    let lists = ByRelation::new([vec![], vec![], vec![], vec![], object_id_list([20, 22, 7])]);
    let ask = Ask {
        idle: true,
        paths: 1,
        any_class: true,
        ..Ask::default()
    };
    let me = Searcher {
        target_classes: [None, Some(class::WALL)],
        ..soldier()
    };
    let c = best_unit_to_attack(&me, &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, Some(object_id(7)));
    assert!(field.asked.contains(&('q', object_id(7))));
}

/// Outside an idle check the current target answers by its path when it moves and engages
/// (without path checks at once); its path failing, the quick paths follow. An idle check
/// takes no such shortcut.
#[test]
fn the_current_target_answers_by_its_path_outside_an_idle_check() {
    let mut field = Field::default();
    let _ = field.things.insert(
        object_id(7),
        enemy(7, class::CLASS_0, Distance::new(3.0), HitPoints::new(40.0)),
    );
    let _ = field.things.insert(
        object_id(9),
        enemy(9, class::CLASS_0, Distance::new(5.0), HitPoints::new(40.0)),
    );
    field.paths = vec![object_id(7)];
    let lists = ByRelation::new([vec![], vec![], vec![], vec![], object_id_list([7, 9])]);
    let ask = Ask {
        paths: 1,
        any_class: true,
        ..Ask::default()
    };
    let me = Searcher {
        target: Some(object_id(7)),
        ..soldier()
    };
    let c = best_unit_to_attack(&me, &ask, &lists, &mut field).unwrap();
    assert_eq!(
        (c.target, &field.asked[..]),
        (Some(object_id(7)), &[('p', object_id(7))][..])
    );
    field.asked.clear();
    let at_once = Ask {
        paths: 0,
        ..ask.clone()
    };
    let c = best_unit_to_attack(&me, &at_once, &lists, &mut field).unwrap();
    assert_eq!((c.target, field.asked.len()), (Some(object_id(7)), 0));
    field.paths.clear();
    field.quick = vec![object_id(9)];
    let c = best_unit_to_attack(&me, &ask, &lists, &mut field).unwrap();
    assert_eq!(c.target, Some(object_id(9)));
    assert_eq!(field.asked[0], ('p', object_id(7)));
    field.asked.clear();
    let idle = Ask { idle: true, ..ask };
    field.paths = vec![object_id(7)];
    let c = best_unit_to_attack(&me, &idle, &lists, &mut field).unwrap();
    assert_eq!(c.target, Some(object_id(9)));
    assert!(!field.asked.contains(&('p', object_id(7))));
}

/// The world's checks of a candidate ([`may_attack`]), in the game's order: one without a unit
/// AI, dead, allied or inside another fails before any; then the attack check, a combat unit's
/// friends' scan (a unit of another type skips it) and the neutral test of list 3 alone, each
/// failing it.
#[test]
fn a_candidate_passes_the_attack_check_the_friends_scan_and_the_neutral_test_in_order() {
    let (thing, _) = enemy(7, class::CLASS_0, Distance::new(3.0), HitPoints::new(40.0));
    let checked = |me: &Searcher, t: &Thing, kind, refused: &[char]| {
        let mut field = Field {
            refused: refused.to_vec(),
            ..Field::default()
        };
        let pass = may_attack(me, (object_id(7), t), kind, &mut field).unwrap();
        (pass, field.checked)
    };
    let me = soldier();
    assert_eq!(
        checked(&me, &thing, UnitRelation::ENEMY, &[]),
        (true, vec!['a', 'f'])
    );
    assert_eq!(
        checked(&me, &thing, UnitRelation::NEUTRAL, &[]),
        (true, vec!['a', 'f', 'n'])
    );
    assert_eq!(
        checked(&me, &thing, UnitRelation::NEUTRAL, &['a']),
        (false, vec!['a'])
    );
    assert_eq!(
        checked(&me, &thing, UnitRelation::NEUTRAL, &['f']),
        (false, vec!['a', 'f'])
    );
    assert_eq!(
        checked(&me, &thing, UnitRelation::NEUTRAL, &['n']),
        (false, vec!['a', 'f', 'n'])
    );
    let building = Searcher {
        kind: master_type::BUILDING,
        ..soldier()
    };
    assert_eq!(
        checked(&building, &thing, UnitRelation::ENEMY, &[]),
        (true, vec!['a'])
    );
    for t in [
        Thing {
            has_ai: false,
            ..thing
        },
        Thing {
            state: ObjectState::Dying,
            ..thing
        },
        Thing {
            allied: true,
            ..thing
        },
        Thing {
            inside: true,
            ..thing
        },
    ] {
        assert_eq!(checked(&me, &t, UnitRelation::ENEMY, &[]), (false, vec![]));
    }
}

/// The owner and class rules ([`kept_by_owner_and_class`]): without `any_class` a building is
/// passed over; a computer player's unit passes over Gaia's objects that hold something or that
/// others take by sight, and boats unless it is a boat itself, of class 52 or a boat hunter (the
/// castle); a human player's unit keeps them all.
#[test]
fn a_computer_s_unit_passes_over_gaia_s_holdings_and_boats() {
    let (thing, _) = enemy(7, class::CLASS_0, Distance::new(3.0), HitPoints::new(40.0));
    let any = Ask {
        any_class: true,
        ..Ask::default()
    };
    let human = soldier();
    let computer = Searcher {
        computer: true,
        ..soldier()
    };
    let house = Thing {
        class: class::BUILDING,
        ..thing
    };
    assert!(!kept_by_owner_and_class(&human, &Ask::default(), &house));
    assert!(kept_by_owner_and_class(&human, &any, &house));
    let gaia = Thing {
        owner: PlayerId::new(0),
        ..thing
    };
    let holding = Thing {
        held: Amount::new(5.0),
        ..gaia
    };
    let taken = Thing {
        capture: true,
        ..gaia
    };
    assert!(kept_by_owner_and_class(&computer, &any, &gaia));
    // a held amount that is not a number counts as held (`+0x310 <= 0` fails)
    let unordered = Thing {
        held: Amount::new(f32::NAN),
        ..gaia
    };
    for t in [holding, taken, unordered] {
        assert!(!kept_by_owner_and_class(&computer, &any, &t));
        assert!(kept_by_owner_and_class(&human, &any, &t));
    }
    let boat = Thing {
        class: class::WARSHIP,
        ..thing
    };
    assert!(!kept_by_owner_and_class(&computer, &any, &boat));
    assert!(kept_by_owner_and_class(&human, &any, &boat));
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
        assert!(kept_by_owner_and_class(&me, &any, &boat));
    }
}
