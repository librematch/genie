use crate::errors::no_player;
use crate::fixtures::{combat_mut, combat_object, gaia, header, moving_mut, static_mut};
use crate::fixtures::{human, recording};
use crate::masters::MasterTable;
use crate::views::{checkpoint_lines, groups_of, movement_of, objects_of, unit_ais_of};
use crate::views::{checkpoints, groups, movement, objects, unit_ais};
use genie_core::objects::masters::FlyMode;
use genie_core::objects::masters::MasterId;
use genie_core::players::CivId;
use genie_rec::de::{Chapter, Checkpoint, Op};
use genie_save::generated::world::{
    Action, ActionListEntry1, Formation, FormationEntry1, Path, PathData, RgeAction,
    RgeActionByKind, TribeElephantUnitAiModule, UnitAiModule, UnitGroup, UnitGroupEntry1,
};
use std::iter::once;

/// A little-endian vector of three floats, as the save holds one.
fn vec3(vector: [f32; 3]) -> [u8; 12] {
    let mut bytes = [0; 12];
    for (index, float) in vector.iter().enumerate() {
        bytes[4 * index..4 * index + 4].copy_from_slice(&float.to_le_bytes());
    }
    bytes
}

/// An Explore action (type 4) in state `state`.
fn explore(state: u8) -> Action {
    Action {
        rge_action_by_kind: Some(Box::new(RgeActionByKind {
            rge_action_explore: Some(Box::new(RgeAction {
                state,
                ..RgeAction::default()
            })),
            ..RgeActionByKind::default()
        })),
        ..Action::default()
    }
}

/// A combat object `id` with the actions `actions` (type, action), the list's end after them.
fn with_actions(id: i32, actions: Vec<(i16, Action)>) -> genie_save::Object {
    let mut object = combat_object(id, 83);
    combat_mut(&mut object)
        .rge_combat_object
        .rge_action_object
        .actions
        .action_kind_list = actions
        .into_iter()
        .map(|(action_kind, action)| ActionListEntry1 {
            action_kind,
            action: Some(Box::new(action)),
        })
        .chain(once(ActionListEntry1::default()))
        .collect();
    object
}

#[test]
fn group_view_lists_its_units_formations_and_command() {
    let mut save = header(vec![]);
    let groups = &mut save.world.base_world.unit_groups;
    groups.number_unit_groups = 1;
    groups.unit_group_counter = 5;
    groups.unit_group = vec![UnitGroup {
        id: 3,
        state: 2,
        centroid: vec3([1.0, 2.0, 0.0]),
        units: vec![UnitGroupEntry1 {
            units: 700,
            ..UnitGroupEntry1::default()
        }],
        previous_formation: Some(Box::new(Formation {
            kind: 4,
            slots: vec![FormationEntry1 {
                slots_unit_id: 700,
                ..FormationEntry1::default()
            }],
            ..Formation::default()
        })),
        waypoints: vec![vec3([5.0, 6.0, 0.0])],
        current_command: 9,
        ..UnitGroup::default()
    }];
    let text = groups_of(&save, false);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "1 unit groups, counter 5");
    assert!(lines[1].starts_with("group 3 owner 0 state 2 flags 0x0 centroid [1.0, 2.0, 0.0]"));
    assert!(lines[2].starts_with("  unit 700 move active 0"));
    assert!(lines[3].starts_with("  previous formation type 4 slots 0"));
    assert!(lines[4].starts_with("    slot unit 700"));
    assert!(
        lines[5].starts_with("  command 9 target 0 type 0 range 0 waypoints [[5.0, 6.0, 0.0]]")
    );
    assert_eq!(lines.len(), 6);
}

/// With `full` the groups print as genie-core imports them; a group the import refuses says
/// why.
#[test]
fn full_group_view_imports_the_groups() {
    let mut save = header(vec![]);
    save.world.base_world.unit_groups.unit_group = vec![UnitGroup {
        id: 3,
        ..UnitGroup::default()
    }];
    let text = groups_of(&save, true);
    assert!(text.contains("UnitGroup {"), "{text}");
    // path data whose path (id 0) is not in the world's table
    save.world.base_world.unit_groups.unit_group[0]
        .motion
        .temp_path_data = vec![PathData::default()];
    let text = groups_of(&save, true);
    assert!(text.contains("the groups do not import: "), "{text}");
}

/// A row per live object: id, master, state, hit points, position, the top action and its
/// state, the retarget timer and the sprite with its node; `actions_of` prints that object's
/// action list first.
#[test]
fn an_object_view_has_row_per_live_object() {
    let mut walker = with_actions(5, vec![(4, explore(3))]);
    let rge_static_object = static_mut(&mut walker);
    rge_static_object.hp = 25.0;
    rge_static_object.tx = 1.0;
    rge_static_object.ty = 2.0;
    rge_static_object.sprite = -1;
    combat_mut(&mut walker).unit_ai_module = Some(Box::new(UnitAiModule {
        randomized_retarget_timer: 2500,
        ..UnitAiModule::default()
    }));
    let idle = combat_object(6, 4);
    let save = header(vec![gaia(vec![walker, idle])]);
    let text = objects_of(&save, 0, None).unwrap();
    let rows: Vec<&str> = text.lines().collect();
    assert_eq!(
        rows[0],
        "id\tmaster\tstate\thp\tx\ty\tz\taction\taction_state\tretarget\tsprite"
    );
    assert_eq!(rows[1], "5\t83\t0\t25\t1\t2\t0\t4\t3\t2500\t-1 -");
    // an idle object's list holds its end entry alone (type 0), as a save's does
    assert_eq!(rows[2], "6\t4\t0\t0\t0\t0\t0\t0\t\t\t0 -");
    let with_list = objects_of(&save, 0, Some(5)).unwrap();
    assert!(with_list.contains("action_kind_list"), "{with_list}");
    assert!(objects_of(&save, 1, None).is_err_and(|error| error == no_player(1)));
}

/// The objects asked for, with their velocity, orientation and path data; each level's path
/// from the world's table.
#[test]
fn movement_view_shows_the_paths_of_the_objects_asked_for() {
    let mut walker = combat_object(5, 83);
    let rge_moving_object = moving_mut(&mut walker);
    rge_moving_object.angle = 0.5;
    rge_moving_object.temp_path_data = vec![PathData {
        path_id: 7,
        ..PathData::default()
    }];
    let other = combat_object(6, 83);
    let mut save = header(vec![gaia(vec![walker, other])]);
    save.world.base_world.path = vec![Path {
        id_value: 7,
        waypoints: vec![vec3([3.0, 4.0, 0.0])],
        ..Path::default()
    }];
    let text = movement_of(&save, 0, &[5]).unwrap();
    assert!(
        text.starts_with("5 master 83 at (0, 0, 0) facet 0 sprite 0\n"),
        "{text}"
    );
    assert!(text.contains("  velocity (0, 0, 0) angle 0.5 "), "{text}");
    assert!(text.contains("  path data PathData {"), "{text}");
    assert!(
        text.contains("    path 7 points [[3.0, 4.0, 0.0]]"),
        "{text}"
    );
    assert!(!text.contains("6 master"), "{text}");
    assert!(movement_of(&save, 2, &[5]).is_err_and(|error| error == no_player(2)));
}

/// The objects asked for that have a unit AI: its order, action and target, the timers, an
/// elephant's origin, the actions and the queued orders.
#[test]
fn unit_ai_view_shows_the_ai_of_the_objects_asked_for() {
    let mut boar = with_actions(5, vec![(4, explore(3))]);
    combat_mut(&mut boar).tribe_elephant_unit_ai_module =
        Some(Box::new(TribeElephantUnitAiModule {
            unit_ai_module: UnitAiModule {
                current_order_value: 700,
                temp_order: vec![[0; 52]],
                ..UnitAiModule::default()
            },
            origin_x: 10.0,
            ..TribeElephantUnitAiModule::default()
        }));
    let no_ai = combat_object(6, 83);
    let save = header(vec![gaia(vec![boar, no_ai])]);
    let text = unit_ais_of(&save, 0, &[5, 6]).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "5 master 83 at (0, 0) state 0 unit-AI type 0");
    assert!(
        lines[1].starts_with("  origin (10, 0, 0) furious "),
        "{text}"
    );
    assert!(lines[2].starts_with("  order 700 action "), "{text}");
    assert!(
        lines[4].starts_with("  actions [(4, 3)] notifies 0 orders 0"),
        "{text}"
    );
    assert!(lines[5].starts_with("    order "), "{text}");
    assert_eq!(lines.len(), 6);
    assert!(unit_ais_of(&save, 1, &[5]).is_err_and(|error| error == no_player(1)));
}

/// Each checkpoint the run reaches: per recorded player (lane word 8) the differing words as
/// (word, slice, game), or ok; a checkpoint past the run's ticks ends the view.
#[test]
fn the_checkpoint_view_names_the_differing_words() {
    let lane = |player: u32, word0: u32| {
        let mut lane = [0; 11];
        lane[0] = word0;
        lane[8] = player;
        lane
    };
    let mut lanes = [[0; 11]; 8];
    lanes[0] = lane(1, 10);
    lanes[1] = lane(2, 20);
    let checkpoint = || {
        Box::new(Checkpoint {
            lanes,
            world_time: 0,
        })
    };
    let mut body = vec![];
    for op in [
        Op::Sync { dt: 50 },
        Op::Checkpoint {
            dt: 50,
            checkpoint: checkpoint(),
        },
        Op::Sync { dt: 50 },
        Op::Checkpoint {
            dt: 50,
            checkpoint: checkpoint(),
        },
    ] {
        op.write(&mut body).unwrap();
    }
    let chapter = Chapter {
        index: 1,
        offset: 0,
        compressed_header: &[],
        body_offset: 0,
        body: &body,
    };
    let blocks = vec![vec![[0; 11], lane(1, 10), lane(2, 21)]];
    assert_eq!(
        checkpoint_lines(&chapter, &blocks).unwrap(),
        "checkpoint at 1: p1 ok p2 [(0, 21, 20)]\n"
    );
}

/// The views of a recording's bytes are their header's; bytes that are no recording are an
/// error.
#[test]
fn the_views_of_recording_read_its_header() {
    let mut boar = combat_object(6, 4);
    // a unit AI module of type 1 (`needUnitAI`), which the save writes after the flag
    let combat = combat_mut(&mut boar);
    combat.need_unit_ai = 1;
    combat.unit_ai_module = Some(Box::new(UnitAiModule {
        current_order_value: 700,
        ..UnitAiModule::default()
    }));
    let save = header(vec![gaia(vec![boar])]);
    let rec = recording(&save, &[]);
    assert_eq!(
        objects(&rec, 0, None).unwrap(),
        objects_of(&save, 0, None).unwrap()
    );
    assert_eq!(groups(&rec, false).unwrap(), groups_of(&save, false));
    assert_eq!(
        movement(&rec, 0, &[6]).unwrap(),
        movement_of(&save, 0, &[6]).unwrap()
    );
    let ais = unit_ais(&rec, 0, &[6]).unwrap();
    assert!(ais.contains("order 700"), "{ais}");
    assert_eq!(ais, unit_ais_of(&save, 0, &[6]).unwrap());
    assert!(objects(&[], 0, None).is_err());
}

/// The checkpoint view runs the recording with the master table and compares each recorded
/// checkpoint with the players' blocks.
#[test]
fn the_checkpoint_view_runs_the_recording() {
    let save = header(vec![gaia(Vec::new()), human(Vec::new())]);
    let mut lanes = [[0; 11]; 8];
    lanes[0][8] = 1;
    lanes[0][0] = 9;
    let rec = recording(
        &save,
        &[
            Op::Sync { dt: 50 },
            Op::Checkpoint {
                dt: 50,
                checkpoint: Box::new(Checkpoint {
                    lanes,
                    world_time: 50,
                }),
            },
        ],
    );
    let table = MasterTable::of_master(
        CivId::GAIA,
        MasterId::of_data(83).unwrap(),
        None,
        FlyMode::of_data(0),
    );
    let text = checkpoints(&rec, &table, 2).unwrap();
    assert!(text.starts_with("checkpoint at 1: p1 [(0, "), "{text}");
}
