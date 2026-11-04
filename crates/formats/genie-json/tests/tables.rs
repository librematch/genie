//! The JSON tables: the keys of each, an unknown key refused, and the installed game's files
//! read and written back to the same value (`GENIE_GAME_DAT` names `resources/_common/dat`).

use genie_json::{Buttons, DropSites, FutureUnits, LinkedTechs, Maps, StringId};
use serde::de::DeserializeOwned;
use std::env::var_os;
use std::fmt::Debug;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

#[test]
fn linked_techs_read_their_keys() {
    let linked_techs: LinkedTechs = genie_json::from_str(
        r#"{"LinkedTechs": [{"NameId": 424001, "Comment": "Satrapies", "Local": true,
            "Type": "MutuallyExclusive", "Techs": [1195, 1196]}]}"#,
    )
    .unwrap();
    let linked_tech_group = &linked_techs.linked_techs[0];
    assert_eq!(
        (
            linked_tech_group.name_id,
            linked_tech_group.local,
            linked_tech_group.kind.as_str(),
            &linked_tech_group.techs[..]
        ),
        (424_001, Some(true), "MutuallyExclusive", &[1195, 1196][..])
    );
    assert_eq!(
        (linked_tech_group.starting_tech, linked_tech_group.instant),
        (None, None)
    );
}

#[test]
fn an_unknown_key_is_refused() {
    let linked_techs: Result<LinkedTechs, _> =
        genie_json::from_str(r#"{"LinkedTechs": [], "Extra": 1}"#);
    assert!(linked_techs.is_err());
}

#[test]
fn drop_sites_read_their_targets() {
    let drop_sites: DropSites = genie_json::from_str(
        r#"{"drop_site_list": [{"name": "Town Center", "building_id": 109, "worker_object_group": 4,
            "accepts_livestock": true, "target_list": [{"name": "Hunted Animals", "object_group": 9,
            "object_state": 5, "attribute_type": 15}], "update_ai_resource_types": [{"resource_type": 0}]}]}"#,
    )
    .unwrap();
    let drop_site = &drop_sites.drop_site_list[0];
    assert_eq!(
        (
            drop_site.building_id,
            drop_site.target_list[0].object_state,
            drop_site.update_ai_resource_kinds.as_ref().unwrap()[0].resource_kind
        ),
        (109, Some(5), 0)
    );
}

#[test]
fn maps_rollover_string_is_number_or_name() {
    let maps: Maps = genie_json::from_str(
        r#"{"default_map_type": 13, "default_nomad_map_type": 33, "default_battle_royale_map_type": 145,
            "map_list": [{"data_name": "A", "name_string_id": 1, "rollover_string_id": 30110,
            "icon_reference": "I", "map_icon_filename": "m", "script_filename": "A.rms", "storage_id": 9,
            "style": "random"}, {"data_name": "B", "name_string_id": 2, "rollover_string_id": "RMS_B",
            "icon_reference": "I", "map_icon_filename": "m", "script_filename": "B.rms", "storage_id": 10,
            "style": "real world", "water": true}]}"#,
    )
    .unwrap();
    assert_eq!(maps.map_list[0].rollover_string_id, StringId::Number(30110));
    assert_eq!(
        maps.map_list[1].rollover_string_id,
        StringId::Name("RMS_B".into())
    );
    assert_eq!(
        (maps.map_list[0].water, maps.map_list[1].water),
        (None, Some(true))
    );
}

#[test]
fn future_units_keep_the_civ_order() {
    let future_units: FutureUnits = genie_json::from_str(
        r#"{"Gaia": {"Buildings": []}, "Aztecs": {"Buildings": [{"ID": 118, "Name": "Builder",
            "Techs": [], "Units": [{"ID": 87, "Name": "Archery Range", "RequiredAge": 2}]}]}}"#,
    )
    .unwrap();
    assert_eq!(
        future_units.0.keys().collect::<Vec<_>>(),
        ["Gaia", "Aztecs"]
    );
    assert_eq!(
        future_units.0["Aztecs"].buildings[0]
            .units
            .as_ref()
            .unwrap()[0]
            .required_age,
        Some(2)
    );
}

#[test]
fn buttons_read_their_keys() {
    let buttons: Buttons = genie_json::from_str(
        r#"{"command_button_list": [{"name": "Set Gather Point", "button_action_id": 51,
            "sequence_id": 4, "icon_id": 45, "help_string_id": 4144, "more_help_string_id": 4944}]}"#,
    )
    .unwrap();
    assert_eq!(
        buttons.command_button_list[0].more_help_string_id,
        Some(4944)
    );
}

/// Read a table, write it, read it again: the same value.
fn round_trip<T>(dir: &Path, file: &str)
where
    T: DeserializeOwned + serde::Serialize + PartialEq + Debug,
{
    let text = read_to_string(dir.join(file)).unwrap();
    let table: T = genie_json::from_str(&text).unwrap_or_else(|error| panic!("{file}: {error}"));
    let written = genie_json::to_string(&table).unwrap();
    let again: T = genie_json::from_str(&written).unwrap();
    assert_eq!(table, again, "{file}");
    // the written file holds the same JSON value as the original
    let original: serde_json::Value = serde_json::from_str(&text).unwrap();
    let rewritten: serde_json::Value = serde_json::from_str(&written).unwrap();
    assert_eq!(original, rewritten, "{file}");
}

#[test]
fn the_installed_tables_read_and_write_back_the_same_values() {
    let Some(dir) = var_os("GENIE_GAME_DAT").map(PathBuf::from) else {
        return eprintln!("GENIE_GAME_DAT is not set; skipping");
    };
    round_trip::<LinkedTechs>(&dir, "linkedTechs.json");
    round_trip::<DropSites>(&dir, "dropsites.json");
    round_trip::<FutureUnits>(&dir, "futuravailableunits.json");
    round_trip::<FutureUnits>(&dir, "paphosfutureavailableunits.json");
    round_trip::<Maps>(&dir, "maps.json");
    round_trip::<Buttons>(&dir, "buttons.json");
}

#[test]
fn an_unknown_key_in_future_unit_or_building_is_refused() {
    for text in [
        r#"{"Aztecs": {"Buildings": [{"ID": 1, "Name": "B", "Extra": 1}]}}"#,
        r#"{"Aztecs": {"Buildings": [{"ID": 1, "Name": "B", "Units": [{"ID": 2, "Name": "U", "Extra": 1}]}]}}"#,
    ] {
        assert!(genie_json::from_str::<FutureUnits>(text).is_err(), "{text}");
    }
}
