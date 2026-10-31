//! The master values the slice reads that the header save does not hold, per civ and master, read
//! from the game's data file with genie-dat: the dead-object values (genie-core's [`MasterData`])
//! and the default task's action. They are game data, which patches and DLCs change, so they come
//! from the `.dat` of the recording's game, never from constants.

use genie_core::GenieError;
use genie_core::objects::lifecycle::MasterData;
use genie_core::objects::masters as work;
use genie_core::objects::masters::{ActionType, MasterId};
use genie_core::players::AmountRate;
use genie_core::players::attribute::AttributeId;
use genie_dat::de::DatFile;
use std::collections::HashMap;

mod dat_work;

pub(crate) use self::dat_work::attributes;
use self::dat_work::dat_work;

/// The action an action object's first update starts from its master's default task
/// (`RGE_Action_Object::update`, 0x14106ac90).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultAction {
    /// The master has no default task.
    None,
    /// The default task's action type (107 is the Artifact action).
    Type(ActionType),
    /// The default task id names no task in the master's list; what the game starts then is not
    /// read yet.
    NoSuchTask,
}

/// The default action of a master whose default task id is `default_task_id` and whose own tasks
/// are `(id, action type)` pairs (DE keeps the tasks in each unit's own list).
pub fn default_action(
    default_task_id: i16,
    mut tasks: impl Iterator<Item = (i16, ActionType)>,
) -> DefaultAction {
    if default_task_id < 0 {
        return DefaultAction::None;
    }
    tasks
        .find(|&(id, _)| id == default_task_id)
        .map_or(DefaultAction::NoSuchTask, |(_, action)| {
            DefaultAction::Type(action)
        })
}

/// A row under its civ and master id.
type KeyedRow = ((u8, i16), Row);

/// The rows of every unit every civ of `dat` has ([`dat_row`]), by civ and master id.
///
/// # Errors
///
/// A task or a cost slot that holds no type or attribute.
fn dat_rows(dat: &DatFile) -> Result<Vec<KeyedRow>, GenieError> {
    let units = dat.civs.iter().enumerate().flat_map(|(c, civ)| {
        civ.units
            .iter()
            .enumerate()
            .filter_map(move |(m, unit)| Some((c as u8, m as i16, unit.as_ref()?)))
    });
    units
        .filter_map(|(c, m, unit)| {
            dat_row(m, unit, &dat.sounds)
                .transpose()
                .map(|row| row.map(|r| ((c, m), r)))
        })
        .collect()
}

/// A `.dat` unit's row (`m`, its slot and master id): its dead-object values (decay, undead),
/// its default action, its work-command values, turning and flight; none for a slot past the
/// masters' ids.
///
/// # Errors
///
/// A task or a cost slot that holds no type or attribute.
fn dat_row(
    m: i16,
    unit: &genie_dat::de::Unit,
    sounds: &[genie_dat::de::Sound],
) -> Result<Option<Row>, GenieError> {
    let Some(id) = MasterId::of_data(m) else {
        return Ok(None);
    };
    let default_action = match unit.bird.as_ref() {
        None => DefaultAction::None,
        Some(b) => {
            let tasks = b
                .tasks
                .iter()
                .map(|t| Ok((t.id, ActionType::try_from(t.action_type)?)))
                .collect::<Result<Vec<_>, GenieError>>()?;
            default_action(b.default_task_id, tasks.into_iter())
        }
    };
    Ok(Some(Row {
        data: MasterData {
            decay: AmountRate::new(unit.resource_decay),
            undead: unit.undead_mode != 0,
        },
        default_action,
        work: Some(dat_work(id, unit, sounds)?),
        turning: unit.dead_fish.as_ref().map(|d| Turning {
            move_algorithm: d.old_move_algorithm,
            min_collision_size_multiplier: d.min_collision_size_multiplier,
        }),
        projectile_type: unit.projectile.as_ref().map(|p| p.projectile_type),
        fly_mode: unit.fly_mode,
    }))
}

/// One master's row.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    data: MasterData,
    default_action: DefaultAction,
    /// What the work command reads of the master, as the `.dat` has it ([`MasterTable::work`]).
    work: Option<work::Master>,
    turning: Option<Turning>,
    /// A missile master's `projectile_type` (`+0x880`) and every master's `fly_mode` (`+0x608`),
    /// which the save does not hold.
    projectile_type: Option<u8>,
    fly_mode: u8,
}

/// What the save does not hold of a moving master's movement (the `.dat` unit's moving part).
/// Its turning is not here: every copy of a master derives it from its turn speed and walking
/// sprite ([`genie_core::movement::turn_rate`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turning {
    /// Master `+0x768`, `old_move_algorithm`: bit 0 has a stop reset the unit's sprite.
    pub move_algorithm: u8,
    /// Master `+0x780`.
    pub min_collision_size_multiplier: f32,
}

/// What the `.dat` says of terrain for passability: each terrain's class (`FUN_1413fbff0` of its
/// `is_water` flags: 1 water, 2 shallows and beach, 4 land, 0 none) and each terrain restriction's
/// mask, the classes it can pass (the obstruction manager's table, `FUN_141054630`). Checked
/// against the game's table, dumped by the tick oracle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainTable {
    classes: Vec<u8>,
    /// Each terrain's record as the map's terrain reads see it.
    kinds: Vec<genie_core::map::terrain::TerrainKind>,
    masks: Vec<genie_core::map::TerrainMask>,
    /// Each terrain restriction's value per terrain (`passable_buildable_dmg_multiplier`).
    passable: Vec<Vec<f32>>,
}

impl TerrainTable {
    /// The terrain table of `dat`: each terrain's restrictions and the restrictions' passability.
    #[must_use]
    pub fn from_dat(dat: &DatFile) -> Self {
        let classes: Vec<u8> = dat
            .terrain_block
            .terrains
            .iter()
            .map(|t| genie_core::map::terrain::terrain_class(t.is_water))
            .collect();
        let masks = dat
            .terrain_restrictions
            .iter()
            .map(|r| {
                let passable = r.passable_buildable_dmg_multiplier.iter().enumerate();
                genie_core::map::TerrainMask::of_bits(
                    passable
                        .filter(|&(_, &v)| 0.0 < v)
                        .fold(0, |m, (t, _)| m | classes.get(t).copied().unwrap_or(0)),
                )
            })
            .collect();
        let passable = dat
            .terrain_restrictions
            .iter()
            .map(|r| r.passable_buildable_dmg_multiplier.clone())
            .collect();
        let kinds = dat
            .terrain_block
            .terrains
            .iter()
            .map(|t| {
                // FUN_140f5efb0: the densities of the terrain's units of class 15 (Gaia's)
                let trees: i32 = t
                    .terrain_unit_id
                    .iter()
                    .zip(&t.terrain_unit_density)
                    .take(usize::try_from(t.number_of_terrain_units_used).unwrap_or(0))
                    .filter(|&(&id, _)| {
                        usize::try_from(id).is_ok_and(|i| {
                            dat.civs
                                .first()
                                .and_then(|c| c.units.get(i))
                                .and_then(Option::as_ref)
                                .is_some_and(|u| u.class == 15)
                        })
                    })
                    .map(|(_, &d)| i32::from(d))
                    .sum();
                genie_core::map::terrain::TerrainKind {
                    shown: t.enabled != 0 || 0 <= t.terrain_to_draw,
                    flags: t.is_water,
                    forest: 249 < trees,
                }
            })
            .collect();
        Self {
            classes,
            kinds,
            masks,
            passable,
        }
    }

    /// The class bit of terrain `id`; 0 for a terrain the data file does not have.
    #[must_use]
    pub fn class(&self, id: u8) -> u8 {
        self.classes.get(usize::from(id)).copied().unwrap_or(0)
    }

    /// Each terrain restriction's mask, by restriction id.
    #[must_use]
    pub fn masks(&self) -> &[genie_core::map::TerrainMask] {
        &self.masks
    }

    /// Each terrain restriction's value per terrain, by restriction id.
    #[must_use]
    pub fn passable(&self) -> &[Vec<f32>] {
        &self.passable
    }

    /// Each terrain's record as the map's terrain reads see it.
    #[must_use]
    pub fn kinds(&self) -> &[genie_core::map::terrain::TerrainKind] {
        &self.kinds
    }
}

/// Every master of every civ of a data file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MasterTable {
    rows: HashMap<(u8, i16), Row>,
    /// Each sprite's speed factor (the `.dat` graphic's `speed_multiplier`, sprite `+0xc`).
    sprite_speed: HashMap<i16, f32>,
    /// What the sprite clock reads of each graphic.
    pub graphics: std::collections::BTreeMap<i16, genie_core::objects::sprite::Graphic>,
    /// The terrain table.
    pub terrain: TerrainTable,
    /// The drop-site table (`dropsites.json` beside the `.dat`, [`crate::drop_sites`]); empty
    /// when not loaded.
    pub drop_sites: Vec<genie_core::unit_ai::task_finder::DropSite>,
    /// The linked technologies (`linkedTechs.json` beside the `.dat`, [`crate::linked_techs`]);
    /// empty when not loaded.
    pub linked_techs: std::collections::BTreeMap<i16, genie_core::technology::Linked>,
    /// The unit lines (`unitlines.json` beside the `.dat`, [`crate::unit_lines`]) as the file
    /// holds them; empty when not loaded (the game then builds tables of no line).
    pub unit_lines: Vec<genie_core::objects::lines::UnitLine>,
    /// The technologies, by id.
    pub techs: Vec<genie_core::technology::TechDef>,
    /// The effects' commands, by id (the world's effect table).
    pub effects: Vec<Vec<genie_core::technology::EffectCommand>>,
    /// The history period (the `.dat`'s `time_slice`, which the world keeps at `+0x8b8`); the
    /// world constructor's [`genie_core::world::HISTORY_PERIOD`] without a `.dat`.
    pub time_slice: i32,
}

/// A technology of the `.dat` as the tech tree holds it: its costs, its type and its effect's
/// commands.
fn tech_def(
    dat: &DatFile,
    t: &genie_dat::de::Tech,
) -> Result<genie_core::technology::TechDef, GenieError> {
    let effect = usize::try_from(t.effect_id)
        .ok()
        .and_then(|e| dat.effects.get(e))
        .map_or_else(|| Ok(Vec::new()), effect_commands)?;
    Ok(genie_core::technology::TechDef {
        costs: tech_costs(t)?,
        kind: genie_core::technology::TechType::from(t.kind),
        effect_id: t.effect_id,
        effect,
        required: t.required_techs,
        required_count: t.required_tech_count,
        civ: genie_core::players::CivId::of_tech(t.civ)?,
        mode: t.full_tech_mode,
        locations: t
            .research_locations
            .iter()
            .map(|l| (l.location_id, l.queue_time))
            .collect(),
    })
}

/// A technology's three costs: each its resource, amount and whether it is paid.
fn tech_costs(t: &genie_dat::de::Tech) -> Result<[genie_core::technology::Cost; 3], GenieError> {
    let [c0, c1, c2] = t.resource_costs.each_ref();
    let cost = |c: &genie_dat::de::ResearchResourceCost| -> Result<_, GenieError> {
        Ok(genie_core::technology::Cost {
            attribute: AttributeId::of_field(c.kind)?,
            amount: c.amount,
            paid: c.flag,
        })
    };
    Ok([cost(c0)?, cost(c1)?, cost(c2)?])
}

/// The technologies ([`tech_def`]) and the effect table ([`effect_commands`]), by id.
fn tech_tables(dat: &DatFile) -> Result<TechTables, GenieError> {
    let techs = dat
        .techs
        .iter()
        .map(|t| tech_def(dat, t))
        .collect::<Result<_, _>>()?;
    let effects = dat
        .effects
        .iter()
        .map(effect_commands)
        .collect::<Result<_, _>>()?;
    Ok((techs, effects))
}

/// The technologies and the effects' commands, by id.
type TechTables = (
    Vec<genie_core::technology::TechDef>,
    Vec<Vec<genie_core::technology::EffectCommand>>,
);

/// An effect's commands as the engine's effect table holds them, each parsed by its kind
/// ([`genie_core::technology::EffectCommand::of_data`]).
fn effect_commands(
    e: &genie_dat::de::Effect,
) -> Result<Vec<genie_core::technology::EffectCommand>, GenieError> {
    e.commands
        .iter()
        .map(|c| genie_core::technology::EffectCommand::of_data(c.kind, c.a, c.b, c.c, c.d))
        .collect()
}

/// What stops [`MasterTable::read`].
#[derive(Debug)]
pub enum ReadError {
    /// The data file could not be opened or read.
    Dat(genie_dat::de::Error),
    /// The drop-site table beside it is out of shape.
    DropSites(crate::DropSitesError),
    /// The linked technologies beside it are out of shape.
    LinkedTechs(serde_json::Error),
    /// The unit lines beside it are out of shape.
    UnitLines(crate::UnitLinesError),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dat(e) => write!(f, "the data file: {e}"),
            Self::DropSites(e) => write!(f, "dropsites.json: {e}"),
            Self::LinkedTechs(e) => write!(f, "linkedTechs.json: {e}"),
            Self::UnitLines(e) => write!(f, "unitlines.json: {e}"),
        }
    }
}

impl std::error::Error for ReadError {}

impl MasterTable {
    /// The table of the data file at `path` (an `empires2_x2_p1.dat`), with the drop-site table
    /// (`dropsites.json`), the linked technologies (`linkedTechs.json`) and the unit lines
    /// (`unitlines.json`) beside it when there are.
    ///
    /// # Errors
    ///
    /// When the data file cannot be read, or a table beside it is out of shape.
    pub fn read(path: &std::path::Path) -> Result<Self, ReadError> {
        let file = std::fs::File::open(path).map_err(|e| {
            ReadError::Dat(
                GenieError::io("a data file that does not open")
                    .with("path", path.display())
                    .with_source(e),
            )
        })?;
        let dat = DatFile::read_from(file).map_err(ReadError::Dat)?;
        let mut table = Self::from_dat(&dat).map_err(ReadError::Dat)?;
        table.read_tables_beside(path)?;
        Ok(table)
    }

    /// The drop sites (`dropsites.json`), linked technologies (`linkedTechs.json`) and unit
    /// lines (`unitlines.json`) beside the data file at `path`, each where there is one.
    fn read_tables_beside(&mut self, path: &std::path::Path) -> Result<(), ReadError> {
        let table = self;
        if let Ok(json) = std::fs::read_to_string(path.with_file_name("dropsites.json")) {
            table.drop_sites = crate::drop_sites(&json).map_err(ReadError::DropSites)?;
        }
        if let Ok(json) = std::fs::read_to_string(path.with_file_name("linkedTechs.json")) {
            table.linked_techs = crate::linked_techs(&json).map_err(ReadError::LinkedTechs)?;
        }
        if let Ok(json) = std::fs::read_to_string(path.with_file_name("unitlines.json")) {
            table.unit_lines = crate::unit_lines(&json).map_err(ReadError::UnitLines)?;
        }
        Ok(())
    }

    /// The rows of every unit every civ of `dat` has: decay (`resource_decay`), the undead flag
    /// (`undead_mode`) and the default action (the action part's default task).
    ///
    /// # Errors
    ///
    /// A task or a cost slot that holds no type or attribute.
    pub fn from_dat(dat: &DatFile) -> Result<Self, GenieError> {
        let rows = dat_rows(dat)?;
        let mut table =
            Self::from_rows(rows.iter().map(|(key, r)| (*key, r.data, r.default_action)));
        for (key, r) in rows {
            if let Some(row) = table.rows.get_mut(&key) {
                *row = r;
            }
        }
        table.terrain = TerrainTable::from_dat(dat);
        table.sprite_speed = dat
            .graphics
            .iter()
            .enumerate()
            .filter_map(|(id, g)| Some((id as i16, g.as_ref()?.speed_multiplier)))
            .collect();
        table.graphics = dat
            .graphics
            .iter()
            .enumerate()
            .filter_map(|(id, g)| {
                let g = g.as_ref()?;
                Some((
                    id as i16,
                    genie_core::objects::sprite::Graphic {
                        id: id as i16,
                        frames: g.frame_count,
                        facets: g.angle_count,
                        speed_multiplier: g.speed_multiplier,
                        frame_duration: g.frame_duration,
                        replay_delay: g.replay_delay,
                        flags: g.sequence_type,
                    },
                ))
            })
            .collect();
        (table.techs, table.effects) = tech_tables(dat)?;
        table.time_slice = dat.time_slice;
        Ok(table)
    }

    /// A table of the given `((civ, master), data, default action)` rows.
    pub fn from_rows(
        rows: impl IntoIterator<Item = ((u8, i16), MasterData, DefaultAction)>,
    ) -> Self {
        Self {
            rows: rows
                .into_iter()
                .map(|(key, data, default_action)| {
                    (
                        key,
                        Row {
                            data,
                            default_action,
                            work: None,
                            turning: None,
                            projectile_type: None,
                            fly_mode: 0,
                        },
                    )
                })
                .collect(),
            sprite_speed: HashMap::new(),
            graphics: std::collections::BTreeMap::default(),
            terrain: TerrainTable::default(),
            drop_sites: Vec::new(),
            linked_techs: std::collections::BTreeMap::new(),
            unit_lines: Vec::new(),
            techs: Vec::new(),
            effects: Vec::new(),
            time_slice: genie_core::world::HISTORY_PERIOD,
        }
    }

    /// The dead-object values of `master` for `civ`; None if the data file has no such master.
    #[must_use]
    pub fn get(&self, civ: u8, master: i16) -> Option<MasterData> {
        self.rows.get(&(civ, master)).map(|r| r.data)
    }

    /// The default action of `master` for `civ`; None if the data file has no such master.
    #[must_use]
    pub fn default_action(&self, civ: u8, master: i16) -> Option<DefaultAction> {
        self.rows.get(&(civ, master)).map(|r| r.default_action)
    }

    /// The `.dat`'s work-command values of a civ's master: the task swap group the save lacks, and
    /// the tasks a master without saved ones has.
    #[must_use]
    pub fn work(&self, civ: u8, master: i16) -> Option<&work::Master> {
        self.rows.get(&(civ, master)).and_then(|r| r.work.as_ref())
    }

    /// How a civ's moving master turns; None for a master without a moving part.
    #[must_use]
    pub fn turning(&self, civ: u8, master: i16) -> Option<Turning> {
        self.rows.get(&(civ, master)).and_then(|r| r.turning)
    }

    /// A missile master's `projectile_type` (`+0x880`).
    #[must_use]
    pub fn projectile_type(&self, civ: u8, master: i16) -> Option<u8> {
        self.rows
            .get(&(civ, master))
            .and_then(|r| r.projectile_type)
    }

    /// A master's `fly_mode` (`+0x608`): 0 keeps its objects on the ground, 1 above it.
    #[must_use]
    pub fn fly_mode(&self, civ: u8, master: i16) -> Option<u8> {
        self.rows.get(&(civ, master)).map(|r| r.fly_mode)
    }

    /// A sprite's speed factor; None for a sprite the data file does not have.
    #[must_use]
    pub fn sprite_speed(&self, sprite: i16) -> Option<f32> {
        self.sprite_speed.get(&sprite).copied()
    }
}

/// The tests' tables: one master's row, the sprites' speed factors and graphics and the terrain
/// restrictions' masks, without a data file.
#[cfg(test)]
impl MasterTable {
    /// A table of the master `master` of `civ`, turning as `turning` and flying as `fly_mode`.
    pub(crate) fn of_master(civ: u8, master: i16, turning: Option<Turning>, fly_mode: u8) -> Self {
        let row = Row {
            data: MasterData::default(),
            default_action: DefaultAction::None,
            work: None,
            turning,
            projectile_type: None,
            fly_mode,
        };
        Self {
            rows: std::iter::once(((civ, master), row)).collect(),
            ..Self::default()
        }
    }

    /// The sprite `sprite`'s speed factor and graphic of `facets` facets and `flags`.
    pub(crate) fn with_sprite(mut self, sprite: i16, speed: f32, facets: i16, flags: u8) -> Self {
        let _ = self.sprite_speed.insert(sprite, speed);
        let graphic = genie_core::objects::sprite::Graphic {
            id: sprite,
            frames: 1,
            facets,
            speed_multiplier: speed,
            frame_duration: 0.1,
            replay_delay: 0.0,
            flags,
        };
        let _ = self.graphics.insert(sprite, graphic);
        self
    }

    /// The default action of the table's master `master` of `civ`.
    ///
    /// # Panics
    ///
    /// For a master the table does not have.
    pub(crate) fn with_default_action(
        mut self,
        civ: u8,
        master: i16,
        action: DefaultAction,
    ) -> Self {
        self.rows
            .get_mut(&(civ, master))
            .expect("a master of the table")
            .default_action = action;
        self
    }

    /// The terrain restrictions' masks.
    pub(crate) fn with_masks(mut self, masks: Vec<u8>) -> Self {
        self.terrain.masks = masks
            .into_iter()
            .map(genie_core::map::TerrainMask::of_bits)
            .collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::masters::dat_work::sound_event;
    use genie_core::players::AmountRate;

    /// A directory of its own under the system's temporary one, emptied first.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("genie-oracle-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A data file that does not open is the data file's error; one that is not a `.dat`
    /// too.
    #[test]
    fn a_data_file_that_does_not_read_is_its_error() {
        let dir = scratch("no-dat");
        let path = dir.join("empires2_x2_p1.dat");
        assert!(matches!(MasterTable::read(&path), Err(ReadError::Dat(_))));
        std::fs::write(&path, b"not a dat").unwrap();
        assert!(matches!(MasterTable::read(&path), Err(ReadError::Dat(_))));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The tables beside the data file are read where there are; one out of shape is its own
    /// error, and none leaves the table as it was.
    #[test]
    fn the_tables_beside_the_data_file_are_read_where_there_are() {
        let dir = scratch("tables");
        let path = dir.join("empires2_x2_p1.dat");
        let mut table = MasterTable::default();
        table.read_tables_beside(&path).unwrap();
        assert_eq!(table, MasterTable::default());
        std::fs::write(dir.join("dropsites.json"), r#"{"drop_site_list": []}"#).unwrap();
        std::fs::write(dir.join("linkedTechs.json"), r#"{"LinkedTechs": []}"#).unwrap();
        std::fs::write(dir.join("unitlines.json"), r#"{"UnitLines": []}"#).unwrap();
        table.read_tables_beside(&path).unwrap();
        for (file, check) in [
            ("dropsites.json", 0),
            ("linkedTechs.json", 1),
            ("unitlines.json", 2),
        ] {
            let good = std::fs::read_to_string(dir.join(file)).unwrap();
            std::fs::write(dir.join(file), "[").unwrap();
            let error = table.read_tables_beside(&path).unwrap_err();
            let kind = match error {
                ReadError::DropSites(_) => 0,
                ReadError::LinkedTechs(_) => 1,
                ReadError::UnitLines(_) => 2,
                ReadError::Dat(_) => 3,
            };
            assert_eq!(kind, check, "{file}");
            assert_ne!(error.to_string(), "");
            std::fs::write(dir.join(file), good).unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The fields' −1 is no attribute and any other value 0 or more its attribute, in their
    /// order; a value below −1 fails the parse.
    #[test]
    fn the_attribute_fields_parse_in_their_order() {
        use genie_core::players::attribute::AttributeId;
        assert_eq!(
            attributes([-1, 0, 5]).unwrap(),
            [
                None,
                Some(AttributeId::try_from(0).unwrap()),
                Some(AttributeId::try_from(5).unwrap())
            ]
        );
        assert!(attributes([0, -2]).is_err());
    }

    fn sound(files: usize) -> genie_dat::de::Sound {
        let item = genie_dat::de::SoundItem {
            file_name: genie_dat::de::DebugString::default(),
            resource_id: 0,
            probability: 100,
            civilization: -1,
            icon_set: -1,
        };
        genie_dat::de::Sound {
            id: 0,
            play_delay: 0,
            file_count: files as i16,
            cache_time: 0,
            total_probability: 100,
            items: vec![item; files],
        }
    }

    #[test]
    fn a_task_s_sound_event_of_0_with_a_sound_of_files_takes_the_default_event() {
        // FUN_1413af9a0: a valid sound number whose sound has files gives an id of 0 the
        // default event; an id set stays, and so does 0 with no sound or an empty one
        let sounds = [sound(0), sound(2)];
        assert_eq!(sound_event(0, 1, &sounds), 0x62d4_b3ec);
        assert_eq!(sound_event(19598, 1, &sounds), 19598);
        assert_eq!(sound_event(0, 0, &sounds), 0);
        assert_eq!(sound_event(0, -1, &sounds), 0);
        assert_eq!(sound_event(0, 2, &sounds), 0);
        // the id is an int: the high bit makes it negative
        assert_eq!(sound_event(0xffff_fffe, -1, &sounds), -2);
    }

    #[test]
    fn the_default_action_is_the_action_type_of_the_default_task() {
        // the cow: default task 0, whose action type is 107 (the Artifact action)
        assert_eq!(
            default_action(
                0,
                [
                    (0, genie_core::ownership::artifact::ARTIFACT_TYPE),
                    (1, genie_core::objects::masters::action_type::HUNT_RANGE)
                ]
                .into_iter()
            ),
            DefaultAction::Type(genie_core::ownership::artifact::ARTIFACT_TYPE)
        );
        assert_eq!(
            default_action(
                -1,
                [(0, genie_core::ownership::artifact::ARTIFACT_TYPE)].into_iter()
            ),
            DefaultAction::None
        );
        // FARMDROP and two others name task 0 but list no task
        assert_eq!(
            default_action(0, std::iter::empty()),
            DefaultAction::NoSuchTask
        );
    }

    #[test]
    fn rows_are_found_by_civ_and_master() {
        let t = MasterTable::from_rows([
            (
                (1, 332),
                MasterData {
                    decay: AmountRate::new(1.0),
                    undead: false,
                },
                DefaultAction::None,
            ),
            (
                (2, 705),
                MasterData {
                    decay: AmountRate::new(0.25),
                    undead: false,
                },
                DefaultAction::Type(genie_core::ownership::artifact::ARTIFACT_TYPE),
            ),
        ]);
        assert_eq!(t.get(1, 332).map(|d| d.decay), Some(AmountRate::new(1.0)));
        assert_eq!(
            t.default_action(2, 705),
            Some(DefaultAction::Type(
                genie_core::ownership::artifact::ARTIFACT_TYPE
            ))
        );
        assert_eq!((t.get(2, 332), t.default_action(1, 705)), (None, None));
    }

    #[test]
    fn unit_lines_read_each_entrys_keys_as_the_builder_does() {
        use genie_core::objects::lines::UnitLine;
        // two entries of the 158041 file, one without its chain, one with an array of names
        let json = r#"{"Hint": "LineIDs have to be between (-400, -199).", "UnitLines": [
            {"Name": "Wall Line", "Identifier": "stone-wall-line", "Building": true,
             "LineID": -399, "IDChain": [117, 155]},
            {"Name": "Eagle Warrior Line", "Identifier": ["eagle-warrior-line",
             "eagle-scout-line"], "LineID": -266, "IDChain": [751, 753, 752]},
            {"Name": "Broken", "LineID": -1}
        ]}"#;
        let lines = crate::unit_lines(json).unwrap();
        assert_eq!(
            lines,
            [
                UnitLine {
                    line: Some(-399),
                    chain: Some(vec![117, 155]),
                    building: true,
                    identifiers: vec!["stone-wall-line".to_owned()],
                },
                UnitLine {
                    line: Some(-266),
                    chain: Some(vec![751, 753, 752]),
                    building: false,
                    identifiers: vec![
                        "eagle-warrior-line".to_owned(),
                        "eagle-scout-line".to_owned()
                    ],
                },
                UnitLine {
                    line: Some(-1),
                    ..UnitLine::default()
                },
            ]
        );
        assert!(crate::unit_lines(r#"{"UnitLines": [{"LineID": "x"}]}"#).is_err());
        assert_eq!(crate::unit_lines("{}").unwrap(), []);
    }

    #[test]
    fn linked_techs_take_their_groups_kind() {
        use genie_core::technology::Linked;
        // two groups of the 158041 file, and one of a kind the game does not know
        let json = r#"{"LinkedTechs": [
            {"NameId": 424008, "Comment": "Krypteia/Peloponnesian League",
             "Type": "MutuallyExclusive", "Techs": [1131, 1133]},
            {"NameId": 424010, "Comment": "Trade", "Type": "Cycle", "Techs": [1215, 1216],
             "StartingTech": 1215, "Instant": true},
            {"NameId": 1, "Comment": "", "Type": "Other", "Techs": [7]}
        ]}"#;
        let linked = crate::linked_techs(json).unwrap();
        assert_eq!(
            linked.into_iter().collect::<Vec<_>>(),
            [
                (1131, Linked::MutuallyExclusive),
                (1133, Linked::MutuallyExclusive),
                (1215, Linked::Cycle),
                (1216, Linked::Cycle)
            ]
        );
    }
}
