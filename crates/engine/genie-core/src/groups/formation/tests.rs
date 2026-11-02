use super::*;
use crate::groups::FormationKind;
use crate::groups::GroupFlags;
use crate::groups::GroupTarget;
use crate::groups::TransformType;
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Rect;
use crate::map::Restriction;
use crate::map::RestrictionId;
use crate::map::TerrainFlags;
use crate::map::Tile;
use crate::map::terrain::TileTerrain;
use crate::movement::LevelFlags;
use crate::movement::PathFlags;
use crate::movement::PathState;
use crate::movement::WaypointLevel;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::BuildableOn;
use crate::objects::masters::FormationClass;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObstructionClass;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::master_type;
use crate::objects::masters::master_type::DOPPELGANGER;
use crate::players::PlayerId;
use crate::players::PlayerType;
use crate::test_ids::group_id;
use crate::test_ids::master_id;
use crate::test_ids::object_id;
use crate::test_ids::object_id_list;
use crate::vision::VisionBits;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;

/// `FUN_14102d580` copies a slot in use without its transform mark (`+0x44`).
#[test]
fn larger_slots_array_drops_the_transform_marks() {
    let mut f = Formation::new();
    f.resize_slots(1);
    f.use_slots(1);
    f.slots[0].unit_id = Some(object_id(7));
    f.slots[0].transformed = 1;
    f.resize_slots(4);
    assert_eq!(
        (f.slots[0].unit_id, f.slots[0].transformed),
        (Some(object_id(7)), 0)
    );
    assert_eq!(f.slot_capacity(), 4);
}

/// A 20 by 20 map of terrain class 1 but class 2 on `blocked`, masters 4 and 5 (category 1)
/// and 6 (category 3), and player 1's `units` (id, master, x, y) on it and on the
/// obstruction manager, all in group 1 at the origin facing x.
pub(crate) fn slot_world(units: &[(i32, i16, f32, f32)], blocked: &[(usize, usize)]) -> World {
    use crate::map::spatial::{Obstruction, Placed, Spatial, Tiles};
    use crate::map::terrain::{Terrain, TerrainKind};
    let objects: Vec<crate::objects::unit::Unit> = units
        .iter()
        .map(|&(id, master, x, y)| {
            let mut u = crate::objects::unit::Unit {
                id: object_id(id),
                place: crate::objects::layers::Static::at(Position::new(x, y, 0.0)),
                ..crate::objects::unit::Unit::default()
            };
            u.life.master_id = master_id(master);
            u
        })
        .collect();
    let player = crate::world::Player {
        objects: objects.clone().into(),
        ..crate::world::Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![crate::world::Player::default(), player],
    );
    let master = |id: MasterId, category: u8| crate::objects::masters::Master {
        id,
        kind: master_type::COMBAT,
        formation_category: FormationClass::of_data(category),
        terrain_restriction: Some(RestrictionId::named(0)),
        radius: Footprint::new(0.2, 0.2),
        ..crate::objects::masters::Master::default()
    };
    let masters = crate::objects::masters::Masters::new([
        master(master_id(4), 1),
        master(master_id(5), 1),
        master(ids::ID_6, 3),
    ]);
    t.masters = vec![masters.clone(), masters];
    let mut classes = vec![1u8; 400];
    for &(x, y) in blocked {
        classes[y * 20 + x] = 2;
    }
    t.spatial = Spatial::with_tiles(
        GridSize::new(20, 20),
        Tiles::new(GridSize::new(20, 20), classes),
    );
    let kinds = vec![
        TerrainKind {
            shown: true,
            flags: TerrainFlags::WATER,
            forest: false,
        },
        TerrainKind {
            shown: true,
            flags: TerrainFlags::SHALLOWS,
            forest: false,
        },
    ];
    t.spatial.terrain = Some(Terrain::new(
        GridSize::new(20, 20),
        vec![TileTerrain::of_save(0, 255, -1); 400],
        kinds,
        vec![Restriction::new(vec![1.0, 0.0])],
    ));
    for u in &objects {
        t.spatial.insert(
            u.id,
            Placed {
                position: u.position(),
                radius: Footprint::new(0.2, 0.2),
                class: class::CLASS_6,
                player: PlayerId::new(1),
                master: u.life.master_id,
                obstruction_type: ObstructionType::NONE,
                buildable_on: BuildableOn::NO,
                tall: true,
                moves: false,
                unknown: None,
                obstruction: Some(Obstruction {
                    kind: ObstructionType::CIRCLE,
                    class: ObstructionClass::UNIT,
                }),
            },
        );
    }
    let mut g = UnitGroup::new(group_id(1));
    g.owner = Some(PlayerId::new(1));
    g.members = units
        .iter()
        .map(|&(id, ..)| crate::groups::Member {
            id: object_id(id),
            ..crate::groups::Member::default()
        })
        .collect();
    g.motion.position[1] = BVector::new(1.0, 0.0, 0.0);
    g.motion.position[2] = BVector::new(0.0, 1.0, 0.0);
    g.desired_spacing = Distance::new(2.0);
    t.groups = crate::groups::UnitGroups {
        groups: vec![g],
        ..crate::groups::UnitGroups::default()
    };
    t
}

/// A slot at `at` (x, height, y) of category `category`, transformed and active.
pub(crate) fn slot_at(at: BVector, category: i32) -> Slot {
    Slot {
        unit_id: None,
        transform_position: at,
        desired_category: FormationCategory::named(category),
        desired_sub_category: UnitType::NONE,
        active: 1,
        transformed: 1,
        ..Slot::default()
    }
}

/// `FUN_141016990`: along the stored positions on the ground from the front (the first
/// point): 0 is the front, 2.5 halfway along the first stretch (5 long), 8 halfway along the
/// second (6 long) at the second point's height, 11 its end; 12 is past the line, and one
/// point is no line.
#[test]
fn the_point_from_the_front_runs_along_the_stored_positions() {
    let point = |p: BVector| crate::groups::LinePoint {
        point: p,
        ..crate::groups::LinePoint::default()
    };
    let line = crate::groups::PointLine {
        points: vec![
            point(BVector::new(0.0, 7.0, 0.0)),
            point(BVector::new(3.0, 1.0, 4.0)),
            point(BVector::new(3.0, 0.0, 10.0)),
        ],
        ..crate::groups::PointLine::default()
    };
    assert_eq!(
        find_point_from_front(&line, Distance::ZERO),
        Some(BVector::new(0.0, 7.0, 0.0))
    );
    assert_eq!(
        find_point_from_front(&line, Distance::new(-1.0)),
        Some(BVector::new(0.0, 7.0, 0.0))
    );
    assert_eq!(
        find_point_from_front(&line, Distance::new(2.5)),
        Some(BVector::new(1.5, 7.0, 2.0))
    );
    assert_eq!(
        find_point_from_front(&line, Distance::new(8.0)),
        Some(BVector::new(3.0, 1.0, 7.0))
    );
    assert_eq!(
        find_point_from_front(&line, Distance::new(11.0)),
        Some(BVector::new(3.0, 1.0, 10.0))
    );
    assert_eq!(find_point_from_front(&line, Distance::new(12.0)), None);
    let one = crate::groups::PointLine {
        points: vec![point(BVector::ZERO)],
        ..crate::groups::PointLine::default()
    };
    assert_eq!(find_point_from_front(&one, Distance::ZERO), None);
}

/// `FUN_141011300`: a null object (none in the world's table) is -1; a ghost (its master 243
/// of type 25) is below 70, -1; a ram (class 13, master 35, of a human player) keeps its own
/// category (2), as the rams' rule never holds; a combat master whose hero flag grants
/// [`crate::objects::masters::HeroFlags::PROTECTED_FORMATION`] is 5.
#[test]
fn unit_category_is_its_masters_and_none_for_null_object_or_ghost() {
    let mut t = slot_world(&[(10, 4, 1.0, 0.0), (35, 35, 2.0, 0.0)], &[]);
    for masters in &mut t.masters {
        *masters = crate::objects::masters::Masters::new([
            crate::objects::masters::Master {
                id: master_id(4),
                kind: master_type::COMBAT,
                formation_category: FormationClass::of_data(1),
                hero_flag: HeroFlags::PROTECTED_FORMATION,
                ..crate::objects::masters::Master::default()
            },
            crate::objects::masters::Master {
                id: ids::ID_35,
                kind: master_type::COMBAT,
                class: class::SIEGE,
                formation_category: FormationClass::of_data(2),
                ..crate::objects::masters::Master::default()
            },
            crate::objects::masters::Master {
                id: crate::vision::ghost::GHOST_MASTER,
                kind: DOPPELGANGER,
                formation_category: FormationClass::of_data(1),
                ..crate::objects::masters::Master::default()
            },
        ]);
    }
    t.players[1].kind = PlayerType::HUMAN;
    t.players[1].ghosts = vec![crate::vision::ghost::Ghost {
        id: object_id(-2),
        original: None,
        original_id: None,
        original_master: None,
        original_owner: PlayerId::new(1),
        position: Position::new(3.0, 4.0, 0.0),
        tile: Tile::new(3, 4),
        footprint: 0,
        bits: VisionBits::NONE,
        cant_see: crate::players::PlayerSet::NONE,
        seen: false,
        state: ObjectState::Alive,
    }];
    assert_eq!(t.unit_category(None).unwrap(), FormationCategory::named(-1));
    assert_eq!(
        t.unit_category(t.world_object(object_id(99))).unwrap(),
        FormationCategory::named(-1)
    );
    let ghost = t.world_object(object_id(-2));
    assert!(matches!(
        ghost,
        Some(crate::groups::WorldObject::Ghost(p, _)) if p == PlayerId::new(1)
    ));
    assert_eq!(
        t.unit_category(ghost).unwrap(),
        FormationCategory::named(-1)
    );
    assert_eq!(
        t.unit_category(t.world_object(object_id(35))).unwrap(),
        FormationCategory::named(2)
    );
    assert_eq!(
        t.unit_category(t.world_object(object_id(10))).unwrap(),
        FormationCategory::named(5)
    );
    // the world's table holds ghosts only in its negative half, none for −1 (no object takes
    // it) and none for i32::MIN
    assert!(t.world_object(None::<ObjectId>).is_none());
    assert!(t.world_object(crate::objects::unit::TAKEN_OUT).is_none());
}

/// `FUN_14100e370` from the origin: of all, 12 (category 3) is nearest; of category 1, 11;
/// of master 4, 10; with 11 used, 10; of category 2, none.
#[test]
fn the_closest_unit_matches_its_slots_category_and_type() {
    let t = slot_world(
        &[(10, 4, 5.0, 0.0), (11, 5, 2.0, 0.0), (12, 6, 1.0, 0.0)],
        &[],
    );
    let g = t.groups.get(group_id(1)).unwrap();
    let o = BVector::ZERO;
    assert_eq!(
        t.closest_unit(g, o, FormationCategory::named(-1), UnitType::NONE, &[])
            .unwrap(),
        Some(object_id(12))
    );
    assert_eq!(
        t.closest_unit(g, o, FormationCategory::named(1), UnitType::NONE, &[])
            .unwrap(),
        Some(object_id(11))
    );
    assert_eq!(
        t.closest_unit(g, o, FormationCategory::named(1), UnitType::of_word(4), &[])
            .unwrap(),
        Some(object_id(10))
    );
    assert_eq!(
        t.closest_unit(
            g,
            o,
            FormationCategory::named(1),
            UnitType::NONE,
            &[Some(object_id(11))]
        )
        .unwrap(),
        Some(object_id(10))
    );
    assert_eq!(
        t.closest_unit(g, o, FormationCategory::named(2), UnitType::NONE, &[])
            .unwrap(),
        None
    );
}

/// `FUN_141009760`: slots at 0 and 3 on x, units at 1 and -5. In order the first takes the
/// unit at 1 and the second the one at -5 (1 + 8); with the first unit barred from the first
/// slot, 5 + 2 is shorter, so that order wins. Units at 1 and 5 keep the plain order
/// (1 + 2 against 5 + 2).
#[test]
fn the_slots_take_their_units_in_the_shorter_of_two_orders() {
    let run = |second: f32| {
        let t = slot_world(&[(10, 4, 1.0, 0.0), (11, 4, second, 0.0)], &[]);
        let g = t.groups.get(group_id(1)).unwrap().clone();
        let mut f = Formation::new();
        f.slots = vec![
            slot_at(BVector::ZERO, 1),
            slot_at(BVector::new(3.0, 0.0, 0.0), 1),
        ];
        t.populate_slots(&mut f, &g).unwrap();
        f.slots
            .iter()
            .map(|s| (s.unit_id, s.position_filled))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        run(-5.0),
        [(Some(object_id(11)), 1), (Some(object_id(10)), 1)]
    );
    assert_eq!(
        run(5.0),
        [(Some(object_id(10)), 1), (Some(object_id(11)), 1)]
    );
}

/// `FUN_14100a0c0`: slots at (0, 0) and (3, 0), member 10 at the first and 11 at
/// (1/3, 2.98), 3 from the first slot and 4 from the second. The trials of `FUN_141009760`
/// give 10 the first slot (4 in all against 6); the feature keeps the start whose squared
/// distances differ least, 11 in the first (9) and 10 in the second (9). A member the world
/// does not hold (12) keeps its row of costs (0 as the process starts) and takes a slot.
#[test]
fn the_feature_assignment_keeps_the_start_of_least_variance() {
    let y = (80.0f32 / 9.0).sqrt();
    let run = |feature: bool, gone: bool| {
        let mut t = slot_world(&[(10, 4, 0.0, 0.0), (11, 4, 1.0 / 3.0, y)], &[]);
        if gone {
            t.groups.groups[0].members.push(crate::groups::Member {
                id: object_id(12),
                ..crate::groups::Member::default()
            });
        }
        let g = t.groups.get(group_id(1)).unwrap().clone();
        let mut f = Formation::new();
        f.slots = vec![
            slot_at(BVector::ZERO, 1),
            slot_at(BVector::new(3.0, 0.0, 0.0), 1),
        ];
        if feature {
            t.assign_slots_by_variance(&mut f, &g, &mut Vec::new())
                .unwrap();
        } else {
            t.populate_slots(&mut f, &g).unwrap();
        }
        f.slots
            .iter()
            .map(|s| (s.unit_id, s.position_filled))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        run(false, false),
        [(Some(object_id(10)), 1), (Some(object_id(11)), 1)]
    );
    assert_eq!(
        run(true, false),
        [(Some(object_id(11)), 1), (Some(object_id(10)), 1)]
    );
    assert_eq!(
        run(true, true),
        [(Some(object_id(11)), 1), (Some(object_id(12)), 1)]
    );
}

/// The feature's costs persist between calls (`DAT_143f161e0`): a member the world no
/// longer holds is placed by the row its last call wrote, and a cost row runs into the next
/// member's with more slots than members.
#[test]
fn the_feature_costs_of_gone_member_are_its_last() {
    let mut t = slot_world(&[(10, 4, 0.0, 0.0), (11, 4, 3.0, 0.0)], &[]);
    let g = t.groups.get(group_id(1)).unwrap().clone();
    let mut costs = Vec::new();
    let two = || {
        vec![
            slot_at(BVector::ZERO, 1),
            slot_at(BVector::new(3.0, 0.0, 0.0), 1),
        ]
    };
    let mut f = Formation::new();
    f.slots = two();
    t.assign_slots_by_variance(&mut f, &g, &mut costs).unwrap();
    assert_eq!(costs[..4], [0.0, 9.0, 9.0, 0.0]);
    // 11 leaves the world: its row (9, 0) stays and still gives it the second slot
    let _ = t.remove_live(PlayerId::new(1), ListIndex::new(1)).unwrap();
    let mut f = Formation::new();
    f.slots = two();
    t.assign_slots_by_variance(&mut f, &g, &mut costs).unwrap();
    assert_eq!(f.slots[1].unit_id, Some(object_id(11)));
    // three slots for two members: row 0 runs into row 1's first place, which row 1 writes
    let t = slot_world(&[(10, 4, 0.0, 0.0), (11, 4, 3.0, 0.0)], &[]);
    let mut f = Formation::new();
    f.slots = vec![
        slot_at(BVector::ZERO, 1),
        slot_at(BVector::new(3.0, 0.0, 0.0), 1),
        slot_at(BVector::new(1.0, 0.0, 0.0), 1),
    ];
    let mut costs = Vec::new();
    t.assign_slots_by_variance(&mut f, &g, &mut costs).unwrap();
    assert_eq!(costs[..5], [0.0, 9.0, 9.0, 0.0, 4.0]);
}

/// `FUN_141009760` measures from the slot's `+0x1c`, which the load fills from the save's
/// `mTransformPosition` (0x141018637), not from `+0x10` (`mDesiredTransformPosition`).
#[test]
fn slot_takes_the_unit_nearest_its_transform_position() {
    let t = slot_world(&[(10, 4, 1.0, 0.0), (11, 4, 9.0, 0.0)], &[]);
    let g = t.groups.get(group_id(1)).unwrap().clone();
    let mut f = Formation::new();
    let mut s = slot_at(BVector::new(10.0, 0.0, 0.0), 1);
    s.desired_transform_position = BVector::ZERO;
    f.slots = vec![s];
    t.populate_slots(&mut f, &g).unwrap();
    assert_eq!(f.slots[0].unit_id, Some(object_id(11)));
}

/// `FUN_141009060` after a column: a line of 3 takes its members (in order) into its places
/// from the middle out (1, 0, 2), one of 4 into (2, 0, 1, 3); with a line part-transformed,
/// or no column before, the slots take the closest units or none.
#[test]
fn line_after_column_fills_from_its_middle_out() {
    let run = |width: usize, transformed: i32, previous: FormationKind| {
        let units: Vec<(i32, i16, f32, f32)> = (0..width)
            .map(|i| (10 + i as i32, 4, i as f32, 0.0))
            .collect();
        let mut t = slot_world(&units, &[]);
        let mut column = Formation::new();
        column.kind = previous;
        t.groups.groups[0].previous_formation = Some(column);
        let g = t.groups.get(group_id(1)).unwrap().clone();
        let mut f = Formation::new();
        f.kind = FormationKind::LINE;
        f.maximum_width = width as i32;
        f.maximum_depth = 1;
        f.number_transformed_slots = transformed;
        f.slots = (0..width)
            .map(|i| slot_at(BVector::new(i as f32, 0.0, 0.0), -1))
            .collect();
        t.from_previous_column(&mut f, &g).unwrap();
        f.slots.iter().map(|s| s.unit_id).collect::<Vec<_>>()
    };
    assert_eq!(
        run(3, 3, FormationKind::COLUMN),
        [
            Some(object_id(11)),
            Some(object_id(10)),
            Some(object_id(12))
        ]
    );
    assert_eq!(
        run(4, 4, FormationKind::COLUMN),
        [
            Some(object_id(11)),
            Some(object_id(12)),
            Some(object_id(10)),
            Some(object_id(13))
        ]
    );
    // part-transformed: each slot its closest
    assert_eq!(
        run(3, 2, FormationKind::COLUMN),
        [
            Some(object_id(10)),
            Some(object_id(11)),
            Some(object_id(12))
        ]
    );
    assert_eq!(run(3, 3, FormationKind::LINE), [None, None, None]);
}

/// `FUN_14103a1f0`'s group (1), target (2) and horde (3) transforms of every slot of the
/// array: the group's place plus the offset turned to the group's forward (for 1), as it is
/// (2 and 3),
/// the target's original place for 2; a turn of more than a right angle takes the units anew.
#[test]
fn the_group_target_and_horde_transforms_place_every_slot() {
    let mut t = slot_world(&[(10, 4, 3.0, 3.0)], &[]);
    t.groups.groups[0].motion.position[0] = BVector::new(5.0, 0.0, 5.0);
    t.groups.groups[0].motion.position[1] = BVector::new(0.0, 0.0, 1.0);
    t.groups.groups[0].current_target.original_position = BVector::new(9.0, 0.0, 9.0);
    let mut run = |kind: i32| {
        let mut f = Formation::new();
        f.transform_type = TransformType::of_data(kind);
        f.transform_orientation = BVector::new(0.0, 0.0, 1.0);
        f.slots = vec![Slot {
            offset: BVector::new(1.0, 0.0, 0.0),
            ..slot_at(BVector::ZERO, 1)
        }];
        f.spare_slots = vec![Slot::default()];
        let took = t.formation_transform(&mut f, group_id(1), false).unwrap();
        (
            took,
            f.slots[0].transform_position,
            f.number_transformed_slots,
        )
    };
    let (_, turned, n) = run(1);
    assert_eq!(n, 2);
    // the offset ahead (x) turned to the group's forward (z)
    assert!(
        (turned.x() - 5.0).abs() < 1e-6 && (turned.y() - 6.0).abs() < 1e-6,
        "{turned:?}"
    );
    assert_eq!(run(3), (false, BVector::new(6.0, 0.0, 5.0), 2));
    assert_eq!(run(2), (false, BVector::new(10.0, 0.0, 9.0), 2));
    // facing back: the slot takes its (only) unit anew
    let mut f = Formation::new();
    f.transform_type = TransformType::UNTURNED;
    f.transform_orientation = BVector::new(0.0, 0.0, -1.0);
    f.slots = vec![slot_at(BVector::ZERO, 1)];
    assert!(t.formation_transform(&mut f, group_id(1), false).unwrap());
    assert_eq!(
        (f.slots[0].unit_id, f.slots[0].position_filled),
        (Some(object_id(10)), 1)
    );
}

/// `FUN_14103a1f0`'s passability: slot 1 lies on a blocked tile (8, 5), behind which the
/// line from the group's place meets the tile's temporary record; its unit at (7.5, 5.5)
/// searches from its cell (x+1 blocked, then x-1) and finds slot 0's cell, so the slot takes
/// that cell's middle; below a spacing of 1 nudged by the cell's count (1: x + 0.25).
#[test]
fn an_unreachable_slot_moves_to_the_reachable_cell_its_unit_finds() {
    let run = |spacing: f32| {
        let mut t = slot_world(&[(10, 4, 5.5, 5.5), (11, 4, 7.5, 5.5)], &[(8, 5)]);
        t.groups.groups[0].motion.position[0] = BVector::new(5.5, 0.0, 5.5);
        t.groups.groups[0].motion.lead = Some(object_id(10));
        t.groups.groups[0].desired_spacing = Distance::new(spacing);
        let mut f = Formation::new();
        f.slots = vec![
            Slot {
                unit_id: Some(object_id(10)),
                ..slot_at(BVector::new(5.5, 0.0, 5.5), 1)
            },
            Slot {
                unit_id: Some(object_id(11)),
                ..slot_at(BVector::new(8.5, 0.0, 5.5), 1)
            },
        ];
        t.passable_slots(&mut f, group_id(1)).unwrap();
        f.slots
            .iter()
            .map(|s| s.transform_position)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        run(2.0),
        [BVector::new(5.5, 0.0, 5.5), BVector::new(5.5, 0.0, 5.5)]
    );
    assert_eq!(
        run(0.5),
        [BVector::new(5.5, 0.0, 5.5), BVector::new(5.75, 0.0, 5.5)]
    );
    // COMISS/JC at 0x14103cde3: a NaN spacing nudges as one below 1
    assert_eq!(
        run(f32::NAN),
        [BVector::new(5.5, 0.0, 5.5), BVector::new(5.75, 0.0, 5.5)]
    );
    // every slot standable: nothing moves
    let mut t = slot_world(&[(10, 4, 5.5, 5.5)], &[]);
    t.groups.groups[0].motion.lead = Some(object_id(10));
    let mut f = Formation::new();
    f.slots = vec![slot_at(BVector::new(12.5, 0.0, 5.5), 1)];
    t.passable_slots(&mut f, group_id(1)).unwrap();
    assert_eq!(f.slots[0].transform_position, BVector::new(12.5, 0.0, 5.5));
}

/// A ghost of id `id` at (`x`, `y`) in its player's doppelganger list.
pub(crate) fn ghost_at(id: i32, x: f32, y: f32) -> crate::vision::ghost::Ghost {
    crate::vision::ghost::Ghost {
        id: object_id(id),
        original: None,
        original_id: None,
        original_master: None,
        original_owner: PlayerId::new(1),
        position: Position::new(x, y, 0.0),
        tile: Tile::new(x as i32, y as i32),
        footprint: 0,
        bits: VisionBits::NONE,
        cant_see: crate::players::PlayerSet::NONE,
        seen: false,
        state: ObjectState::Alive,
    }
}

/// `FUN_1410465d0` for the constructor's lead −1 finds no object: no id is −1 (the world's
/// reusable ids start at −2), so a ghost (master 243, its restriction 1 standing on class 2
/// only, at −2) changes nothing. The first active slot's unit tests the slots, and slot 1
/// moves as with that unit as the lead.
#[test]
fn the_passability_tests_with_the_world_object_of_the_lead_id() {
    use crate::map::terrain::{Terrain, TerrainKind};
    let run = |ghost: bool| {
        let mut t = slot_world(&[(10, 4, 5.5, 5.5), (11, 4, 7.5, 5.5)], &[(8, 5)]);
        let kind = |flags| TerrainKind {
            shown: true,
            flags,
            forest: false,
        };
        t.spatial.terrain = Some(Terrain::new(
            GridSize::new(20, 20),
            vec![TileTerrain::of_save(0, 255, -1); 400],
            vec![kind(TerrainFlags::WATER), kind(TerrainFlags::SHALLOWS)],
            vec![
                Restriction::new(vec![1.0, 0.0]),
                Restriction::new(vec![0.0, 1.0]),
            ],
        ));
        for masters in &mut t.masters {
            let ghost = crate::objects::masters::Master {
                id: crate::vision::ghost::GHOST_MASTER,
                kind: DOPPELGANGER,
                terrain_restriction: Some(RestrictionId::named(1)),
                ..crate::objects::masters::Master::default()
            };
            *masters =
                crate::objects::masters::Masters::new(masters.iter().cloned().chain([ghost]));
        }
        if ghost {
            t.players[1].ghosts = vec![ghost_at(-2, 1.0, 1.0)];
        }
        t.groups.groups[0].motion.position[0] = BVector::new(5.5, 0.0, 5.5);
        assert_eq!(t.groups.groups[0].motion.lead, None);
        let mut f = Formation::new();
        f.slots = vec![
            Slot {
                unit_id: Some(object_id(10)),
                ..slot_at(BVector::new(5.5, 0.0, 5.5), 1)
            },
            Slot {
                unit_id: Some(object_id(11)),
                ..slot_at(BVector::new(8.5, 0.0, 5.5), 1)
            },
        ];
        t.passable_slots(&mut f, group_id(1)).unwrap();
        f.slots
            .iter()
            .map(|s| s.transform_position)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        run(false),
        [BVector::new(5.5, 0.0, 5.5), BVector::new(5.5, 0.0, 5.5)]
    );
    assert_eq!(run(true), run(false));
}

/// The passability's extra step: slot 1's unit at (7.5, 7.5) finds slot 2's cell (6, 7)
/// first, which points toward the nearest slot (0, at (5.5, 5.5): -y), and the line to the
/// next cell's middle (6.5, 6.5) is clear, so the slot takes that cell.
#[test]
fn found_cell_steps_toward_the_nearest_slot_when_the_way_is_clear() {
    let mut t = slot_world(&[(10, 4, 5.5, 5.5), (11, 4, 7.5, 7.5)], &[(8, 5)]);
    t.groups.groups[0].motion.position[0] = BVector::new(5.5, 0.0, 5.5);
    t.groups.groups[0].motion.lead = Some(object_id(10));
    let mut f = Formation::new();
    f.slots = vec![
        Slot {
            unit_id: Some(object_id(10)),
            ..slot_at(BVector::new(5.5, 0.0, 5.5), 1)
        },
        Slot {
            unit_id: Some(object_id(11)),
            ..slot_at(BVector::new(8.5, 0.0, 5.5), 1)
        },
        slot_at(BVector::new(6.5, 0.0, 7.5), 1),
    ];
    t.passable_slots(&mut f, group_id(1)).unwrap();
    assert_eq!(f.slots[1].transform_position, BVector::new(6.5, 0.0, 6.5));
    assert_eq!(f.slots[2].transform_position, BVector::new(6.5, 0.0, 7.5));
}

/// The passability's box and nearest slot as the assembly takes them: a NaN place keeps the
/// box's old bounds (`MINSS`/`MAXSS` of the new value first) and the earlier nearest slot
/// (`COMISS`/`CMOVBE`); an inactive slot counts for neither.
#[test]
fn the_passability_box_keeps_its_bounds_and_nearest_for_nan_place() {
    let nan = BVector::new(f32::NAN, 0.0, f32::NAN);
    let mut idle = slot_at(BVector::new(50.0, 0.0, 50.0), 1);
    idle.active = 0;
    let slots = [
        slot_at(BVector::new(4.0, 0.0, 6.0), 1),
        slot_at(nan, 1),
        idle,
        slot_at(BVector::new(3.0, 0.0, 2.0), 1),
    ];
    let (bounds, nearest) = passability_box(
        &slots,
        BVector::new(5.0, 0.0, 5.0),
        BVector::new(3.0, 0.0, 3.0),
    );
    assert_eq!(bounds, Rect::new(3.0, 2.0, 5.0, 6.0));
    assert_eq!(nearest, Some(3));
    let (_, nearest) = passability_box(
        &slots[..2],
        BVector::new(5.0, 0.0, 5.0),
        BVector::new(3.0, 0.0, 3.0),
    );
    assert_eq!(nearest, Some(0));
    // a NaN tester place keeps the starting bounds for its part too
    let (bounds, _) = passability_box(&slots[..1], nan, BVector::new(3.0, 0.0, 3.0));
    assert_eq!(bounds, Rect::new(4.0, 6.0, 4.0, 6.0));
    assert_eq!(passability_box(&slots[1..2], nan, BVector::ZERO).1, None);
}

/// A reachable slot's cell mark: along y for `|dx| <= |dz|` and for a NaN (bit 7 for a `dz`
/// below 0 or NaN), else along x (bits 6 and 7 for a `dx` below 0 or NaN, bit 6 else); the
/// count goes up and bit 0 is set.
#[test]
fn cell_mark_points_toward_the_nearest_slot_and_nan_along_y() {
    assert_eq!(cell_mark(0, 0.0, 2.0), 3);
    assert_eq!(cell_mark(0, 0.0, -2.0), 0x83);
    assert_eq!(cell_mark(0, 3.0, 1.0), 0x43);
    assert_eq!(cell_mark(0, -3.0, 1.0), 0xc3);
    assert_eq!(cell_mark(0xc3, 0.0, 2.0), 5);
    assert_eq!(cell_mark(0, f32::NAN, 2.0), 3);
    assert_eq!(cell_mark(0, 3.0, f32::NAN), 0x83);
    assert_eq!(cell_mark(0, f32::NAN, f32::NAN), 0x83);
    // a slot's tile by CVTTSS2SI: a NaN place is off any map
    assert_eq!(
        slot_tile(BVector::new(f32::NAN, 0.0, 2.5)),
        Tile::new(i32::MIN, 2)
    );
}

/// The line transform's distance back (0x14103a850, `COMISS`/`JA`): a line's own when it is
/// above 0, else, a NaN too, the line's index times the spacing.
#[test]
fn nan_distance_back_takes_the_index_times_the_spacing() {
    let mut t = slot_world(&[(10, 4, 5.0, 5.0)], &[]);
    t.groups.groups[0].motion.position[0] = BVector::new(9.0, 0.0, 5.0);
    t.groups.groups[0].desired_spacing = Distance::new(2.0);
    let mut f = Formation::new();
    f.transform_type = TransformType::BY_LINES;
    f.number_lines = 2;
    f.slots = vec![slot_at(BVector::ZERO, 1), slot_at(BVector::ZERO, 1)];
    f.lines = vec![
        FormationLine {
            indices: vec![0],
            ..FormationLine::default()
        },
        FormationLine {
            indices: vec![1],
            distance_back: Distance::new(f32::NAN),
            ..FormationLine::default()
        },
    ];
    let _ = t.formation_transform(&mut f, group_id(1), false).unwrap();
    assert_eq!(f.slots[1].transform_position, BVector::new(7.0, 0.0, 5.0));
}

/// `FUN_141041810`: every slot of the array, spare ones too, loses its unit and filled mark.
#[test]
fn clearing_the_units_empties_every_slot_of_the_array() {
    let mut f = Formation::new();
    let taken = Slot {
        unit_id: Some(object_id(7)),
        position_filled: 1,
        ..Slot::default()
    };
    f.slots = vec![taken];
    f.spare_slots = vec![taken];
    f.clear_units();
    assert!(
        f.slots
            .iter()
            .chain(&f.spare_slots)
            .all(|s| (s.unit_id, s.position_filled) == (None, 0))
    );
}

/// The grid's count (bits 1 to 5) counts on and wraps, the other bits stay.
#[test]
fn grid_cell_counts_its_slots_in_bits_1_to_5() {
    assert_eq!(count_up(0), 2);
    assert_eq!(count_up(3), 5);
    assert_eq!(count_up(0xc1 | 0x3e), 0xc1);
}

/// One statistic of category 1 holding `units`, all of master 4, spacing 1.
fn one_statistic(t: &mut World, units: &[ObjectId]) {
    let mut unit_types = vec![UnitType::CLEARED; 50];
    unit_types[0] = UnitType::of_word(4);
    let mut counts = vec![0u32; 50];
    counts[0] = units.len() as u32;
    let g = &mut t.groups.groups[0];
    g.desired_spacing = Distance::new(1.0);
    g.unit_types = vec![crate::groups::UnitTypeStatistic {
        formation_category: FormationCategory::named(1),
        units: units.to_vec(),
        desired_spacing: Distance::new(1.0),
        number_unit_types: 1,
        unit_types,
        unit_type_counts: counts,
        ..crate::groups::UnitTypeStatistic::default()
    }];
}

/// `FUN_141012280` lays its slots, transforms them and takes the units, the column-to-line
/// flag set for a line after a column (0x141013d20..0x141013d3a): a line of three after a
/// column takes its members from the middle out (11, 10, 12), after a line the closest
/// (10, 11, 12); each as its slots and `lay_units` with that flag.
#[test]
fn line_after_column_takes_its_units_in_the_column_order() {
    let run = |previous: FormationKind| {
        let mut t = slot_world(
            &[(10, 4, 5.5, 5.5), (11, 4, 5.5, 6.5), (12, 4, 5.5, 7.5)],
            &[],
        );
        one_statistic(&mut t, &[object_id(10), object_id(11), object_id(12)]);
        t.groups.groups[0].motion.position[0] = BVector::new(6.5, 0.0, 6.5);
        let mut before = Formation::new();
        before.kind = previous;
        t.groups.groups[0].previous_formation = Some(before);
        let g = t.groups.get(group_id(1)).unwrap().clone();
        let mut whole = Formation::new();
        assert!(
            t.create_column_line(&mut whole, group_id(1), false, (false, false))
                .unwrap()
        );
        let mut parts = Formation::new();
        assert!(column_line_slots(&mut parts, &g, false, (false, false), t.world_time).unwrap());
        t.lay_units(&mut parts, group_id(1), previous == FormationKind::COLUMN)
            .unwrap();
        assert_eq!(whole, parts);
        whole.slots.iter().map(|s| s.unit_id).collect::<Vec<_>>()
    };
    assert_eq!(
        run(FormationKind::COLUMN),
        [
            Some(object_id(11)),
            Some(object_id(10)),
            Some(object_id(12))
        ]
    );
    assert_eq!(
        run(FormationKind::LINE),
        [
            Some(object_id(10)),
            Some(object_id(11)),
            Some(object_id(12))
        ]
    );
}

/// A path level of `points` points at waypoint `waypoint` (none without points).
fn level(points: &[BVector], waypoint: i32, flags: u32) -> PathData {
    PathData {
        level: WaypointLevel::WALKED,
        linked_type: crate::movement::LinkedPathType::of_data(0),
        points: points.to_vec(),
        has_path: !points.is_empty(),
        path_flags: PathFlags::from_bits_retain(flags),
        waypoint: crate::movement::WaypointIndex::of_data(waypoint),
        state: PathState::Idle,
        range: Distance::ZERO,
        target: None,
        pause_time: Seconds::ZERO,
        flags: LevelFlags::NONE,
        length: Distance::ZERO,
        created: WorldTime::ZERO,
        continue_counter: crate::movement::PlanTries::of_data(10),
        repath_time: Seconds::ZERO,
        ignore: Vec::new(),
        ignored: Vec::new(),
    }
}

/// Player 1 (minimum column distance 10, column-to-line 5) with unit 7 of a soldier master
/// (type 70, category 1) in group 1 at the origin along `paths` (walked first).
fn type_world(paths: Vec<PathData>) -> World {
    let mut u = crate::objects::unit::Unit {
        id: object_id(7),
        ..crate::objects::unit::Unit::default()
    };
    u.life.master_id = master_id(4);
    let player = crate::world::Player {
        objects: vec![u].into(),
        formations: crate::world::Formations {
            min_column_distance: crate::map::WholeDistance::of_data(10),
            column_to_line_distance: crate::map::WholeDistance::of_data(5),
            ..crate::world::Formations::default()
        },
        ..crate::world::Player::default()
    };
    let mut t = World::new(
        WorldTime::ZERO,
        crate::world::FrameSeed::of_data(0),
        PlayerId::new(0),
        vec![Millis::ZERO, Millis::ZERO],
        vec![crate::world::Player::default(), player],
    );
    let masters = crate::objects::masters::Masters::new([crate::objects::masters::Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        formation_category: FormationClass::of_data(1),
        ..crate::objects::masters::Master::default()
    }]);
    t.masters = vec![masters.clone(), masters];
    let mut g = UnitGroup::new(group_id(1));
    g.owner = Some(PlayerId::new(1));
    g.members = vec![crate::groups::Member {
        id: object_id(7),
        ..crate::groups::Member::default()
    }];
    g.motion.paths = paths;
    t.groups = crate::groups::UnitGroups {
        groups: vec![g],
        ..crate::groups::UnitGroups::default()
    };
    t
}

fn own(t: &World) -> FormationKind {
    t.own_formation_type(t.groups.get(group_id(1)).unwrap())
        .unwrap()
}

/// `FUN_141048180`: the top ancestor (the last level) on its last waypoint; a lone level has
/// no ancestor, a top without a path is not on it.
#[test]
fn the_last_high_level_waypoint_is_the_top_levels_last_point() {
    let top = level(
        &[
            BVector::ZERO,
            BVector::new(1.0, 0.0, 0.0),
            BVector::new(2.0, 0.0, 0.0),
        ],
        2,
        0,
    );
    let walked = level(&[BVector::ZERO, BVector::new(1.0, 0.0, 0.0)], 0, 0);
    assert!(on_last_high_level_waypoint(&[walked.clone(), top.clone()]));
    assert!(!on_last_high_level_waypoint(std::slice::from_ref(&top)));
    let mut early = top;
    early.waypoint = crate::movement::WaypointIndex::of_data(1);
    assert!(!on_last_high_level_waypoint(&[walked.clone(), early]));
    assert!(!on_last_high_level_waypoint(&[walked, level(&[], 5, 0)]));
}

/// `UpdateDesiredPositions`'s own choice for a soldier: a line without a path, a column off
/// the last high waypoint or on a patrol, and on it a column only beyond both distances
/// (measured on the ground to the path's last point): 30 is beyond 10 and 5, 8 within the
/// minimum column distance, whatever the current type; a target's radius and range lengthen
/// the column-to-line distance.
#[test]
fn group_chooses_column_far_along_its_path_and_line_near_its_end() {
    assert_eq!(own(&type_world(Vec::new())), FormationKind::LINE);
    let walked = level(&[BVector::ZERO, BVector::new(1.0, 0.0, 0.0)], 0, 0);
    let path_to = |x: f32, waypoint: i32, flags: u32| {
        vec![
            walked.clone(),
            level(&[BVector::ZERO, BVector::new(x, 9.0, 0.0)], waypoint, flags),
        ]
    };
    assert_eq!(own(&type_world(path_to(30.0, 0, 0))), FormationKind::COLUMN);
    assert_eq!(own(&type_world(path_to(30.0, 1, 1))), FormationKind::COLUMN);
    assert_eq!(own(&type_world(path_to(30.0, 1, 0))), FormationKind::COLUMN);
    assert_eq!(own(&type_world(path_to(8.0, 1, 0))), FormationKind::LINE);
    // a column (whose type the game tests first) turns to a line within 10 too; at 12 it
    // stays one until a target's radius and range lengthen 5 past 12
    let mut t = type_world(path_to(8.0, 1, 0));
    let mut column = Formation::new();
    column.kind = FormationKind::COLUMN;
    t.groups.groups[0].current_formation = Some(column);
    assert_eq!(own(&t), FormationKind::LINE);
    t.groups.groups[0].motion.paths = path_to(12.0, 1, 0);
    assert_eq!(own(&t), FormationKind::COLUMN);
    let g = &mut t.groups.groups[0];
    g.current_target.target = GroupTarget::of_saved(3, -1);
    g.current_target.target_radius = Distance::new(5.0);
    g.current_target.range = Distance::new(2.5);
    assert_eq!(own(&t), FormationKind::LINE);
    // a lone level measures its own path; one point measures to the origin
    let mut t = type_world(vec![level(&[BVector::new(30.0, 0.0, 0.0)], 0, 0)]);
    t.groups.groups[0].motion.position[0] = BVector::new(40.0, 0.0, 0.0);
    assert_eq!(own(&t), FormationKind::COLUMN);
}

/// [`crate::groups::GroupFlags::LINE`] (`MinAutoDistanceLock`) chooses a line, horde units only a
/// horde; given a type, or locked to one (not -1 or 0), the group takes it.
#[test]
fn lock_or_given_type_overrides_the_groups_own_choice() {
    let walked = level(&[BVector::ZERO, BVector::new(1.0, 0.0, 0.0)], 0, 0);
    let far = vec![
        walked,
        level(&[BVector::ZERO, BVector::new(30.0, 0.0, 0.0)], 1, 0),
    ];
    let mut t = type_world(far);
    t.groups.groups[0].flags |= GroupFlags::LINE;
    assert_eq!(own(&t), FormationKind::LINE);
    t.masters[1] = crate::objects::masters::Masters::new([crate::objects::masters::Master {
        id: master_id(4),
        kind: master_type::COMBAT,
        ..crate::objects::masters::Master::default()
    }]);
    assert_eq!(own(&t), FormationKind::HORDE);
    let g = t.groups.get(group_id(1)).unwrap().clone();
    assert_eq!(
        t.formation_type(&g, Some(FormationKind::BOX)).unwrap(),
        FormationKind::BOX
    );
    let mut locked = g;
    locked.formation_locked_type = Some(FormationKind::STAGGERED);
    assert_eq!(
        t.formation_type(&locked, None).unwrap(),
        FormationKind::STAGGERED
    );
    locked.formation_locked_type = Some(FormationKind::HORDE);
    assert_eq!(
        t.formation_type(&locked, None).unwrap(),
        FormationKind::HORDE
    );
    locked.formation_locked_type = None;
    assert_eq!(
        t.formation_type(&locked, None).unwrap(),
        FormationKind::HORDE
    );
}

/// createColumnLine's sizes: a line at least twice as wide as it is deep, a column the other
/// way round.
#[test]
fn line_is_twice_as_wide_as_deep_and_column_the_reverse() {
    assert_eq!(sizes(1, true), (1, 1));
    // root 4: 10 / 4 = 2 lines of 5
    assert_eq!(sizes(10, true), (2, 5));
    // root 3: 9 / 3 = 3 lines of 3, too deep: 2 lines of 5
    assert_eq!(sizes(9, true), (2, 5));
    assert_eq!(sizes(16, true), (3, 6));
    assert_eq!(sizes(10, false), (5, 2));
}

fn group_of(category: i32, count: usize, spacing: f32, types: &[(u32, u32)]) -> UnitGroup {
    let mut unit_types = vec![UnitType::CLEARED; 50];
    let mut counts = vec![0u32; 50];
    for (i, &(t, n)) in types.iter().enumerate() {
        unit_types[i] = UnitType::of_word(t);
        counts[i] = n;
    }
    UnitGroup {
        members: (0..count)
            .map(|i| crate::groups::Member {
                id: object_id(i as i32),
                ..crate::groups::Member::default()
            })
            .collect(),
        unit_types: vec![crate::groups::UnitTypeStatistic {
            formation_category: FormationCategory::named(category),
            units: object_id_list(0..count as i32),
            desired_spacing: Distance::new(spacing),
            number_unit_types: types.len() as i32,
            unit_types,
            unit_type_counts: counts,
            ..crate::groups::UnitTypeStatistic::default()
        }],
        ..UnitGroup::default()
    }
}

/// Ten units of two types in a line: two lines of five, centred (an odd line starts two
/// offsets forward), the types alternating with the second line starting on the type the
/// first ended with.
#[test]
fn line_of_two_unit_types_alternates_its_slots() {
    let g = group_of(3, 10, 1.0, &[(4, 5), (24, 5)]);
    let mut f = Formation::new();
    assert!(
        column_line_slots(
            &mut f,
            &g,
            false,
            (false, false),
            WorldTime::from_millis(77)
        )
        .unwrap()
    );
    assert_eq!(
        (f.kind, f.maximum_width, f.maximum_depth),
        (FormationKind::LINE, 5, 2)
    );
    assert_eq!(f.slots.len(), 10);
    assert_eq!(f.slots[0].offset, BVector::new(0.0, 0.0, -2.0));
    assert_eq!(f.slots[4].offset, BVector::new(0.0, 0.0, 2.0));
    let subs: Vec<i32> = f
        .slots
        .iter()
        .map(|s| s.desired_sub_category.get())
        .collect();
    assert_eq!(subs, [4, 24, 4, 24, 4, 4, 24, 4, 24, 24]);
    assert_eq!(f.lines[1].distance_back, Distance::new(1.0));
    assert_eq!(f.lines[1].indices, [5, 6, 7, 8, 9]);
}

/// A staggered line doubles its spacing and moves its lines a quarter step apart (the even
/// back, the odd forward); a flanked line's slots move apart from its middle by 2.5 less the
/// line's share of the depth.
#[test]
fn staggered_line_shifts_its_lines_and_flanked_one_parts_them() {
    let g = group_of(3, 10, 1.0, &[(4, 10)]);
    let offsets = |f: &Formation, line: usize| -> Vec<f32> {
        f.slots[line * 5..line * 5 + 5]
            .iter()
            .map(|s| s.offset.y())
            .collect()
    };
    let mut f = Formation::new();
    assert!(column_line_slots(&mut f, &g, false, (true, false), WorldTime::ZERO).unwrap());
    assert_eq!(f.kind, FormationKind::STAGGERED);
    assert_eq!(offsets(&f, 0), [-3.5, -1.5, 0.5, 2.5, 4.5]);
    assert_eq!(offsets(&f, 1), [-4.5, -2.5, -0.5, 1.5, 3.5]);
    assert_eq!(f.lines[1].distance_back, Distance::new(2.0));
    let mut f = Formation::new();
    assert!(column_line_slots(&mut f, &g, false, (false, true), WorldTime::ZERO).unwrap());
    assert_eq!(f.kind, FormationKind::FLANKED);
    assert_eq!(offsets(&f, 0), [-4.5, -3.5, 2.5, 3.5, 4.5]);
    assert_eq!(offsets(&f, 1), [-4.0, -3.0, 2.0, 3.0, 4.0]);
    let mut f = Formation::new();
    assert!(column_line_slots(&mut f, &g, true, (false, false), WorldTime::ZERO).unwrap());
    assert_eq!(
        (f.kind, f.maximum_width, f.maximum_depth),
        (FormationKind::COLUMN, 2, 5)
    );
    assert!(
        !column_line_slots(
            &mut f,
            &UnitGroup::default(),
            false,
            (false, false),
            WorldTime::ZERO
        )
        .unwrap()
    );
}

/// Two sub-formations: the narrower (three of category 0) takes one line of the widest's
/// width, ahead of the wider (ten of category 3, spacing 1.5); the wider one's first line
/// stands back by the narrower's spacing plus the difference of the spacings.
#[test]
fn narrow_sub_formation_takes_one_line_ahead_of_wider_one() {
    let mut g = group_of(3, 10, 1.5, &[(4, 10)]);
    let narrow = group_of(0, 3, 1.0, &[(83, 3)]);
    g.unit_types.insert(0, narrow.unit_types[0].clone());
    g.members.extend((10..13).map(|id| crate::groups::Member {
        id: object_id(id),
        ..crate::groups::Member::default()
    }));
    let mut f = Formation::new();
    assert!(column_line_slots(&mut f, &g, false, (false, false), WorldTime::ZERO).unwrap());
    assert_eq!((f.maximum_width, f.maximum_depth), (5, 3));
    let first: Vec<(f32, i32)> = f.slots[..3]
        .iter()
        .map(|s| (s.offset.y(), s.desired_category.get()))
        .collect();
    assert_eq!(first, [(-1.0, 0), (0.0, 0), (1.0, 0)]);
    assert_eq!(f.lines[0].indices, [0, 1, 2]);
    assert_eq!(f.lines[1].indices, [3, 4, 5, 6, 7]);
    let back: Vec<f32> = f.lines.iter().map(|l| l.distance_back.get()).collect();
    assert_eq!(back, [0.0, 1.5, 3.0]);
}

/// A sub-formation beside the widest: within its width one line of all its units; else the
/// lines its width needs and the columns over them, a line less while its columns are fewer
/// than its empty places (7 units at 1.3: 6 lines of 2 leave 5 empty, 5 leave 3, 4 leave 1); a
/// NaN width (`CVTTSS2SI` of a NaN, clamped) one line.
#[test]
fn sub_formation_fits_the_widest_line() {
    use super::column_line::{Category, fit_to_widest};
    let cat = |count, spacing| Category {
        count,
        spacing,
        types: 1,
        unit_types: Vec::new(),
        unit_type_counts: Vec::new(),
    };
    assert_eq!(fit_to_widest(&cat(4, Distance::new(1.0)), 4.0), (1, 4));
    assert_eq!(fit_to_widest(&cat(9, Distance::new(1.0)), 8.0), (2, 5));
    assert_eq!(fit_to_widest(&cat(7, Distance::new(1.0)), 1.3), (4, 2));
    assert_eq!(fit_to_widest(&cat(7, Distance::new(1.0)), f32::NAN), (1, 7));
}
