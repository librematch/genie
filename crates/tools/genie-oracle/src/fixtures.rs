//! Save parts for the tests, built from the generated layouts' defaults: genie holds no game
//! saves (FD-044), so a test builds the few fields its routine reads and leaves the rest zero.

use flate2::write::DeflateEncoder;
use genie_core::objects::masters::ActionKind;
use genie_rec::de::Op;
use genie_save::Header;
use genie_save::Positioned;
use genie_save::generated::world::{
    Action, ActionList, ActionListEntry1, HumanOrCoopPlayer, Master, PlayerMaster, PlayerObjects,
    RgeMasterMovingObject, RgeMasterStaticObject, TribeBuildingObject,
};
use genie_save::generated::world::{
    GaiaPlayer, Object, ObjectListEntry1, Player, RgeMovingObject, RgeStaticObject,
    RgeVictoryConditions, TribeCombatObject, WorldPlayer,
};
use std::io::Cursor;
use std::iter::once;

/// A combat object (type 70), its id `id` and master `master`; its town bell holds no
/// back-to-work action (−1, as the game's saves do).
pub(crate) fn combat_object(id: i32, master: i16) -> Object {
    let mut object = Object {
        tribe_combat_object: Some(Box::new(TribeCombatObject {
            back_to_work1_town_bell_action: -1,
            back_to_work2_town_bell_action: -1,
            ..TribeCombatObject::default()
        })),
        ..Object::default()
    };
    let s = static_mut(&mut object);
    s.id = id;
    s.master_obj = master;
    object
}

/// The combat part of an object of [`combat_object`].
///
/// # Panics
///
/// For an object without one.
pub(crate) fn combat_mut(object: &mut Object) -> &mut TribeCombatObject {
    object
        .tribe_combat_object
        .as_deref_mut()
        .expect("a combat object")
}

/// The moving part of an object of [`combat_object`].
pub(crate) fn moving_mut(object: &mut Object) -> &mut RgeMovingObject {
    &mut combat_mut(object)
        .rge_combat_object
        .rge_action_object
        .rge_moving_object
}

/// The static part of an object of [`combat_object`].
pub(crate) fn static_mut(object: &mut Object) -> &mut RgeStaticObject {
    &mut moving_mut(object).rge_animated_object.rge_static_object
}

/// Gaia, its live objects `objects`.
pub(crate) fn gaia(objects: Vec<Object>) -> Player {
    let mut gaia = GaiaPlayer::default();
    gaia.player_objects.objects.object_type_list = object_list(objects);
    Player {
        type_: 2,
        gaia_player: Some(Box::new(gaia)),
        ..Player::default()
    }
}

/// A human player (type 1), its live objects `objects`.
pub(crate) fn human(objects: Vec<Object>) -> Player {
    let mut human = HumanOrCoopPlayer::default();
    human.player_objects.objects.object_type_list = object_list(objects);
    Player {
        type_: 1,
        human_or_coop_player: Some(Box::new(human)),
        ..Player::default()
    }
}

/// The world part of a player of [`human`].
///
/// # Panics
///
/// For a player of another type.
pub(crate) fn human_mut(player: &mut Player) -> &mut WorldPlayer {
    &mut player
        .human_or_coop_player
        .as_deref_mut()
        .expect("a human player")
        .world_player_human_or_coop
}

/// An object list of `objects`, each its class's type, its end after them.
pub(crate) fn object_list(objects: Vec<Object>) -> Vec<ObjectListEntry1> {
    objects
        .into_iter()
        .map(|object| ObjectListEntry1 {
            object_type: if object.tribe_building_object.is_some() {
                80
            } else {
                70
            },
            object: Some(Box::new(object)),
        })
        .chain(once(ObjectListEntry1::default()))
        .collect()
}

/// Save 66.6, build 158041's own version: its fraction in sixteen bits.
const SAVE_66_6: u32 = 0x0042_9999;

/// A save of `players`, Gaia first, of the build's own version: the lists of one entry per
/// player as long as the players, so [`save_bytes`] writes a save that reads back.
pub(crate) fn header(players: Vec<Player>) -> Header {
    let n = players.len();
    let mut header = Header::default();
    header.file_header.new_save_game_version = SAVE_66_6;
    let world = &mut header.world.base_world;
    world.num_players = i16::try_from(n).expect("a test's few players");
    world.player_time_delta = vec![0; n];
    world.victory_conditions = vec![RgeVictoryConditions::default(); n];
    world.players = players
        .into_iter()
        .map(|mut val| {
            if let Some(wp) = world_player_mut(&mut val) {
                wp.base.relation.resize(n, 0);
                wp.base.unit_diplomacy.resize(n, 0);
                wp.tc_flared.resize(n, 0);
            }
            Positioned { pos: 0, val }
        })
        .collect();
    header
}

/// A player's world part, of any player type.
fn world_player_mut(player: &mut Player) -> Option<&mut WorldPlayer> {
    if let Some(g) = player.gaia_player.as_deref_mut() {
        return Some(&mut g.world_player_gaia.world_player);
    }
    if let Some(h) = player.human_or_coop_player.as_deref_mut() {
        return Some(&mut h.world_player_human_or_coop);
    }
    if let Some(c) = player.computer_player.as_deref_mut() {
        return Some(&mut c.world_player_computer);
    }
    player
        .scenario_editor_phantom_player
        .as_deref_mut()
        .map(|p| &mut p.world_player_scenario_editor_phantom)
}

/// A player's saved objects, of any player type.
///
/// # Panics
///
/// For a player without a type's class.
pub(crate) fn objects_mut(player: &mut Player) -> &mut PlayerObjects {
    if let Some(g) = player.gaia_player.as_deref_mut() {
        return &mut g.player_objects;
    }
    if let Some(h) = player.human_or_coop_player.as_deref_mut() {
        return &mut h.player_objects;
    }
    if let Some(c) = player.computer_player.as_deref_mut() {
        return &mut c.player_objects;
    }
    &mut player
        .scenario_editor_phantom_player
        .as_deref_mut()
        .expect("a player of a type with a class")
        .player_objects
}

/// `player` holding the masters `masters` ([`with_masters`]).
pub(crate) fn holding_masters(mut player: Player, masters: Vec<Master>) -> Player {
    let given = with_masters(masters);
    let objects = objects_mut(&mut player);
    objects.master_object_num = given.master_object_num;
    objects.enabled = given.enabled;
    objects.marker = given.marker;
    objects.masters = given.masters;
    player
}

/// A save's bytes, as genie-save writes them.
pub(crate) fn save_bytes(header: &Header) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    genie_save::write(header, &mut out).expect("a test's save writes");
    out.into_inner()
}

/// A recording of one chapter: the save `header` (deflated), then the ops `ops`.
pub(crate) fn recording(header: &Header, ops: &[Op<'_>]) -> Vec<u8> {
    use std::io::Write;
    let mut deflate = DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
    deflate
        .write_all(&save_bytes(header))
        .expect("a test's save deflates");
    let compressed = deflate.finish().expect("a test's save deflates");
    let header_end = u32::try_from(8 + compressed.len()).expect("a test's small save");
    let mut out = header_end.to_le_bytes().to_vec();
    // no chapter after this one
    out.extend(0_u32.to_le_bytes());
    out.extend(compressed);
    for op in ops {
        op.write(&mut out).unwrap();
    }
    out
}

/// A combat master (type 70) of id `id`.
pub(crate) fn combat_master(id: i16) -> Master {
    let mut master = Master {
        tribe_master_combat_object: Some(Box::default()),
        ..Master::default()
    };
    // the master's id and its copy's, as the save writes them first
    let s = master_static_mut(&mut master);
    s.read_temp = id;
    s.read_temp_2 = id;
    master
}

/// The moving part of a master of [`combat_master`].
///
/// # Panics
///
/// For a master without a combat part.
pub(crate) fn master_moving_mut(master: &mut Master) -> &mut RgeMasterMovingObject {
    &mut master
        .tribe_master_combat_object
        .as_deref_mut()
        .expect("a combat master")
        .rge_master_combat_object
        .rge_master_action_object
        .rge_master_moving_object
}

/// The static part of a master of [`combat_master`].
pub(crate) fn master_static_mut(master: &mut Master) -> &mut RgeMasterStaticObject {
    &mut master_moving_mut(master)
        .rge_master_animated_object
        .rge_master_static_object
}

/// A building (type 80), its id `id` and master `master`.
pub(crate) fn building_object(id: i32, master: i16) -> Object {
    let mut object = Object {
        tribe_building_object: Some(Box::default()),
        ..Object::default()
    };
    let s = &mut building_mut(&mut object)
        .tribe_combat_object
        .rge_combat_object
        .rge_action_object
        .rge_moving_object
        .rge_animated_object
        .rge_static_object;
    s.id = id;
    s.master_obj = master;
    object
}

/// The building part of an object of [`building_object`].
///
/// # Panics
///
/// For an object without one.
pub(crate) fn building_mut(object: &mut Object) -> &mut TribeBuildingObject {
    object
        .tribe_building_object
        .as_deref_mut()
        .expect("a building")
}

/// An action list of `actions` (type, action), its end after them.
pub(crate) fn action_list(actions: Vec<(ActionKind, Action)>) -> ActionList {
    ActionList {
        action_type_list: actions
            .into_iter()
            .map(|(action_type, action)| ActionListEntry1 {
                action_type: action_type.get(),
                action: Some(Box::new(action)),
            })
            .chain(once(ActionListEntry1::default()))
            .collect(),
    }
}

/// A player's saved objects with the masters `masters` (each of type 70).
pub(crate) fn with_masters(masters: Vec<Master>) -> PlayerObjects {
    let n = masters.len();
    PlayerObjects {
        // each master enabled, so the save's list holds them all
        master_object_num: i32::try_from(n).expect("a test's few masters"),
        enabled: vec![1; n],
        marker: (n > 0).then_some(()),
        masters: (n > 0).then(|| {
            masters
                .into_iter()
                .map(|master| PlayerMaster {
                    master_type: 70,
                    master,
                    end_marker: (),
                })
                .collect()
        }),
        ..PlayerObjects::default()
    }
}
