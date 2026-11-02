//! The AI modules read from zeros: every count 0, so each module or record takes its fixed
//! size, which its version gates change. The information and tactical modules' readers end
//! where the format is not known (`todo!`).

use super::*;
use crate::test_support::{header_reader, read_all};

/// How many bytes reading `T` from zeros at `version` takes.
fn size_of_zeros<T: ReadableHeaderElement>(version: f32) -> usize {
    let zeros = vec![0u8; 1 << 16];
    let mut reader = header_reader(&zeros, version);
    let _ = T::read_from(&mut reader).unwrap();
    let mut rest = vec![];
    (1 << 16) - reader.read_to_end(&mut rest).unwrap()
}

/// The modules' sizes without entries.
#[test]
fn the_modules_take_their_fixed_sizes() {
    for (version, build, strategy) in [(9.5, 2432, 76), (10.5, 2432, 80), (11.97, 2436, 80)] {
        assert_eq!(size_of_zeros::<MainAi>(version), 4);
        assert_eq!(size_of_zeros::<BuildAi>(version), build, "{version}");
        assert_eq!(size_of_zeros::<ConstructionAi>(version), 32);
        assert_eq!(size_of_zeros::<DiplomacyAi>(version), 90);
        assert_eq!(size_of_zeros::<EmotionalAi>(version), 24);
        assert_eq!(size_of_zeros::<ResourceAi>(version), 4);
        assert_eq!(size_of_zeros::<StrategyAi>(version), strategy, "{version}");
    }
}

/// The records' sizes: a build item gains its forward flag after 10.87, a resource memory
/// four bytes from 10.91 on, a wall line and a perimeter wall grow with 10.78, 11.20, 11.22,
/// 11.29 and 11.34.
#[test]
fn the_records_take_their_sizes_by_version() {
    for (version, build_item, wall_line, perimeter, resource) in [
        (10.5, 93, 24, 4, 19),
        (10.8, 93, 32, 4, 19),
        (10.9, 97, 32, 4, 19),
        (11.0, 97, 32, 4, 23),
        (11.2, 97, 32, 20, 23),
        (11.25, 97, 32, 24, 23),
        (11.3, 97, 36, 32, 23),
        (11.97, 97, 40, 36, 23),
    ] {
        assert_eq!(size_of_zeros::<BuildItem>(version), build_item, "{version}");
        assert_eq!(size_of_zeros::<WallLine>(version), wall_line, "{version}");
        assert_eq!(
            size_of_zeros::<PerimeterWall>(version),
            perimeter,
            "{version}"
        );
        assert_eq!(
            size_of_zeros::<ResourceMemory>(version),
            resource,
            "{version}"
        );
        assert_eq!(size_of_zeros::<ConstructionItem>(version), 56);
        assert_eq!(size_of_zeros::<ImportantObjectMemory>(version), 39);
        assert_eq!(size_of_zeros::<BuildingLot>(version), 8);
        assert_eq!(size_of_zeros::<AttackMemory>(version), 24);
        assert_eq!(size_of_zeros::<InfluenceMap>(version), 17);
        assert_eq!(size_of_zeros::<QuadrantLog>(version), 12);
    }
}

/// A list of ids: its count, then the ids.
#[test]
fn an_id_list_is_its_count_and_ids() {
    let bytes = [2, 0, 0, 0, 7, 0, 0, 0, 9, 0, 0, 0];
    let ids: Vec<ObjectId> = read_id_list(&mut bytes.as_slice()).unwrap();
    assert_eq!(
        ids.iter().map(|&id| u32::from(id)).collect::<Vec<_>>(),
        [7, 9]
    );
}

/// The main module: its objects behind their count.
#[test]
fn the_main_module_reads_its_objects() {
    let main: MainAi = read_all(&[1, 0, 0, 0, 5, 0, 0, 0], 11.97);
    assert_eq!(main.objects.len(), 1);
}

/// The information module of an old save reads its skipped blocks and ends where the format
/// is not known.
#[test]
fn the_information_module_of_10_5_ends_at_its_unknown_part() {
    let zeros = vec![0u8; 1 << 18];
    let e = InformationAi::read_from(&mut header_reader(&zeros, 10.5)).unwrap_err();
    assert!(matches!(e, crate::Error::NotRead(_)));
}

/// From 10.94 on it reads what the AI has seen; from 11.09 on the players' building counts.
#[test]
fn the_information_module_of_11_97_ends_at_its_unknown_part() {
    let zeros = vec![0u8; 1 << 18];
    let e = InformationAi::read_from(&mut header_reader(&zeros, 11.97)).unwrap_err();
    assert!(matches!(e, crate::Error::NotRead(_)));
}

/// The tactical module ends after its unit lists.
#[test]
fn the_tactical_module_ends_after_its_unit_lists() {
    let zeros = vec![0u8; 1 << 10];
    let e = TacticalAi::read_from(&mut header_reader(&zeros, 11.97)).unwrap_err();
    assert!(matches!(e, crate::Error::NotRead(_)));
}

/// A player's AI reads its modules in order and stops in the information module.
#[test]
fn a_player_s_ai_stops_in_the_information_module() {
    let zeros = vec![0u8; 1 << 18];
    let e = PlayerAi::read_from(&mut header_reader(&zeros, 11.97)).unwrap_err();
    assert!(matches!(e, crate::Error::NotRead(_)));
}
