//! The information AI module ([`InformationAi`]).

use crate::ai::{
    AttackMemory, BuildingLot, ImportantObjectMemory, InfluenceMap, PerimeterWall, QuadrantLog,
    ResourceMemory, read_id_list,
};
use crate::element::ReadableHeaderElement;
use crate::reader::RecordingHeaderReader;
use crate::{ObjectId, Result, fits};
use byteorder::{LE, ReadBytesExt};
use genie_support::{ReadSkipExt, ReadStringsExt, read_list};
use std::io::Read;

/// The information AI module: what the AI knows about the map, objects and other players.
#[derive(Debug, Default, Clone)]
pub struct InformationAi {
    /// Four lists of `u32` values, one per resource type; their meaning is not known yet.
    pub resource_kinds: Vec<Vec<u32>>,
    /// The AI's doctrine value.
    pub doctrine: i32,
    /// The AI's random number.
    pub random_number: i32,
    /// The AI's 40 goal values.
    pub goals: Vec<i32>,
    /// The map size in tiles (x, y).
    pub map_size: (u32, u32),
    /// The building lots in use (empty slots are left out).
    pub building_lots: Vec<BuildingLot>,
    /// The AI's two perimeter walls.
    pub perimeter_walls: (PerimeterWall, PerimeterWall),
    /// What the AI remembers about attacks.
    pub attack_memories: Vec<AttackMemory>,
    /// The resources the AI knows of, one list per resource type.
    pub resource_memories: [Vec<ResourceMemory>; 4],
    /// What the AI remembers about important objects (empty slots are left out).
    pub important_objects: Vec<ImportantObjectMemory>,
    /// The IDs of the important objects.
    pub important_object_ids: Vec<ObjectId>,
    /// The IDs of the important units.
    pub important_unit_ids: Vec<ObjectId>,
    /// The IDs of other important objects.
    pub important_misc_ids: Vec<ObjectId>,
    /// The IDs of the objects the AI defends.
    pub items_to_defend: Vec<ObjectId>,
    /// The IDs of the AI player's buildings.
    pub player_buildings: Vec<ObjectId>,
    /// The IDs of the AI player's objects.
    pub player_objects: Vec<ObjectId>,
    /// Object counts by unit type: 750, 850 or 900 entries, depending on the version.
    pub object_counts: Vec<u32>,
    /// The influence map the AI uses for pathing.
    pub path_map: InfluenceMap,
    /// Statistics for each of the four map quadrants.
    pub quadrant_log: [QuadrantLog; 4],
}

impl ReadableHeaderElement for InformationAi {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut ai = Self::default();
        ai.read_goals_and_resources(input)?;
        let _last_update_row = input.read_u32::<LE>()?;
        ai.important_objects = read_important_objects(input)?;
        ai.building_lots = read_building_lots(input)?;
        ai.perimeter_walls = (
            PerimeterWall::read_from(input)?,
            PerimeterWall::read_from(input)?,
        );

        ai.attack_memories = read_attack_memories(input)?;
        ai.read_id_lists(input)?;
        ai.object_counts = read_object_counts(input)?;
        ai.read_paths(input)?;
        ai.resource_memories = read_resource_memories(input)?;
        skip_dropsites(input)?;
        skip_seen(input)?;
        skip_unit_counts(input)?;

        Err(crate::Error::NotRead("the rest of the AI information"))
    }
}

impl InformationAi {
    /// The unused words and resource type counts, the doctrine, the random number, the 40
    /// goals, the tribute memory (read past), the resource types and the map size.
    fn read_goals_and_resources<R: Read>(
        &mut self,
        input: &mut RecordingHeaderReader<R>,
    ) -> Result<()> {
        let ai = self;
        for _ in 0..4096 {
            let _garbage = input.read_u32::<LE>()?;
        }
        let mut resource_kind_counts = vec![0; 4096];
        input.read_u32_into::<LE>(&mut resource_kind_counts)?;
        ai.doctrine = input.read_i32::<LE>()?;
        ai.random_number = input.read_i32::<LE>()?;
        for _ in 0..40 {
            ai.goals.push(input.read_i32::<LE>()?);
        }

        // tribute memory
        for _ in 0..8 {
            for _ in 0..4 {
                let _a = input.read_u32::<LE>()?;
                let _b = input.read_u32::<LE>()?;
            }
        }

        if ai.resource_kinds.len() < 4 {
            ai.resource_kinds.resize(4, vec![]);
        }
        for resource_id in 0..4 {
            let num_res = input.read_u32::<LE>()?;
            let list = &mut ai.resource_kinds[resource_id];
            for _ in 0..num_res {
                list.push(input.read_u32::<LE>()?);
            }
        }

        ai.map_size = (input.read_u32::<LE>()?, input.read_u32::<LE>()?);

        Ok(())
    }

    /// The important objects', units' and others' ids, the items to defend and the player's
    /// buildings and objects, each behind its count.
    fn read_id_lists<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> Result<()> {
        let ai = self;
        ai.important_object_ids = read_id_list(input)?;
        ai.important_unit_ids = read_id_list(input)?;
        ai.important_misc_ids = read_id_list(input)?;
        ai.items_to_defend = read_id_list(input)?;
        ai.player_buildings = read_id_list(input)?;
        ai.player_objects = read_id_list(input)?;

        Ok(())
    }

    /// The building count (read past), the path map, the last wall positions, the learning
    /// file and the quadrant logs; the blocks old versions have between them, read past.
    fn read_paths<R: Read>(&mut self, input: &mut RecordingHeaderReader<R>) -> Result<()> {
        let ai = self;
        let _building_count = input.read_u32::<LE>()?;

        ai.path_map = InfluenceMap::read_from(input)?;
        let _last_wall_position = (input.read_i32::<LE>()?, input.read_i32::<LE>()?);
        let _last_wall_position_2 = (input.read_i32::<LE>()?, input.read_i32::<LE>()?);

        if input.version() < 10.78 {
            input.skip(4 + 4 * 16)?;
        }

        let _save_learn_information = input.read_u32::<LE>()? != 0;
        let _learn_path = input.read_u32_length_prefixed_str()?;

        if input.version() < 11.25 {
            input.skip(0xFF)?;
        }

        ai.quadrant_log = [
            QuadrantLog::read_from(input)?,
            QuadrantLog::read_from(input)?,
            QuadrantLog::read_from(input)?,
            QuadrantLog::read_from(input)?,
        ];

        Ok(())
    }
}

/// The important objects behind their count; one without an id is left out.
fn read_important_objects<R: Read>(
    input: &mut RecordingHeaderReader<R>,
) -> Result<Vec<ImportantObjectMemory>> {
    let max_important_object_memory = input.read_u32::<LE>()?;
    let mut important_objects = vec![];
    for _ in 0..max_important_object_memory {
        let important_object = ImportantObjectMemory::read_from(input)?;
        if important_object.id.is_none() {
            continue;
        }
        important_objects.push(important_object);
    }
    Ok(important_objects)
}

/// The building lots behind their count; one without a unit type is left out.
fn read_building_lots<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Vec<BuildingLot>> {
    let len = input.read_u32::<LE>()?;
    let mut lots = vec![];
    for _ in 0..len {
        let lot = BuildingLot::read_from(input)?;
        if lot.unit_kind_id.is_none() {
            continue;
        }
        lots.push(lot);
    }
    Ok(lots)
}

/// The attack memories behind their count.
fn read_attack_memories<R: Read>(
    input: &mut RecordingHeaderReader<R>,
) -> Result<Vec<AttackMemory>> {
    let len = input.read_u32::<LE>()?;
    let mut attacks = vec![];
    for _ in 0..len {
        let attack = AttackMemory::read_from(input)?;
        // if attack.unit_type_id.is_none() {
        //     continue;
        // }
        attacks.push(attack);
    }
    Ok(attacks)
}

/// The object counts: 750 before 11.51, 850 before 11.65, then 900.
fn read_object_counts<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Vec<u32>> {
    let num_counts = if input.version() < 11.51 {
        750
    } else if input.version() < 11.65 {
        850
    } else {
        900
    };

    let mut object_counts = vec![0; num_counts];
    input.read_u32_into::<LE>(&mut object_counts)?;
    Ok(object_counts)
}

/// The resource memories: the most and the count of each of the four resources, then each
/// resource's memories.
fn read_resource_memories<R: Read>(
    input: &mut RecordingHeaderReader<R>,
) -> Result<[Vec<ResourceMemory>; 4]> {
    let _max_resources = [
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
    ];
    let num_resources = [
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
    ];

    let mut resources = [vec![], vec![], vec![], vec![]];
    for (list, &num) in resources.iter_mut().zip(num_resources.iter()) {
        *list = read_list(fits(num)?, || ResourceMemory::read_from(input))?;
    }
    Ok(resources)
}

/// The drop sites by age and the closest ones, the forest tiles found, the block old
/// versions have after them and the relic and wonder victories from 10.90 on, read past.
fn skip_dropsites<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<()> {
    let _dropsites_by_age = {
        let mut outer = vec![];
        for _ in 0..4 {
            let mut inner = [0; 4];
            input.read_u32_into::<LE>(&mut inner)?;
            outer.push(inner);
        }
        outer
    };
    let _closest_dropsite = [
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
    ];
    let _closest_dropsite_resource_id = [
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
        input.read_u32::<LE>()?,
    ];
    let _found_forest_tiles = input.read_u32::<LE>()?;

    if input.version() < 10.85 {
        input.skip(64_000)?;
    }

    if input.version() >= 10.90 {
        let mut relics_victory = [0; 9];
        input.read_exact(&mut relics_victory)?;
        let mut wonder_victory = [0; 9];
        input.read_exact(&mut wonder_victory)?;
    }

    Ok(())
}

/// What the AI has seen (from 10.94 and 10.95 on) and its last player count refresh (after
/// 10.99), read past.
fn skip_seen<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<()> {
    if input.version() >= 10.94 {
        let should_farm = input.read_u32::<LE>()?;
        let have_seen_forage = input.read_u32::<LE>()?;
        let have_seen_gold = input.read_u32::<LE>()?;
        let have_seen_stone = input.read_u32::<LE>()?;
        dbg!(
            should_farm,
            have_seen_forage,
            have_seen_gold,
            have_seen_stone,
        );
    }
    if input.version() >= 10.95 {
        let have_seen_forest = input.read_u32::<LE>()?;
        dbg!(have_seen_forest);
    }

    if input.version() > 10.99 {
        let last_player_count_refresh_time = input.read_u32::<LE>()?;
        dbg!(last_player_count_refresh_time);
    }

    Ok(())
}

/// The players' unit counts (120 each from 11.51 on, else 102) and their building and unit
/// totals from 11.09 on, read past.
fn skip_unit_counts<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<()> {
    let player_unit_counts_size: usize = if input.version() >= 11.51 { 120 } else { 102 };
    let mut player_unit_counts = vec![vec![0; player_unit_counts_size]; 8];
    for unit_counts in &mut player_unit_counts {
        input.read_u32_into::<LE>(unit_counts)?;
    }

    if input.version() >= 11.09 {
        let mut player_total_building_counts = [0; 8];
        input.read_u32_into::<LE>(&mut player_total_building_counts)?;

        let mut player_real_total_building_counts = [0; 8];
        if input.version() >= 11.21 {
            input.read_u32_into::<LE>(&mut player_real_total_building_counts)?;
        }

        let mut player_total_unit_counts = [0; 8];
        input.read_u32_into::<LE>(&mut player_total_unit_counts)?;

        println!(
            "{player_total_building_counts:x?} {player_real_total_building_counts:x?} {player_total_unit_counts:x?}"
        );
    }

    Ok(())
}
