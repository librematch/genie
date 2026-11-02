//! The master values the slice reads that the header save does not hold, per civ and master, read
//! from the game's data file with genie-dat: the dead-object values (genie-core's [`MasterData`])
//! and the default task's action. They are game data, which patches and DLCs change, so they come
//! from the `.dat` of the recording's game, never from constants.

use genie_dat::de::DatFile;
use genie_model::GenieError;
use genie_model::combat::missile::ProjectileType;
use genie_model::map::TerrainFlags;
use genie_model::movement::MoveAlgorithm;
use genie_model::objects::FacetCount;
use genie_model::objects::Frame;
use genie_model::objects::SpeedMultiplier;
use genie_model::objects::SpriteId;
use genie_model::objects::lifecycle::MasterData;
use genie_model::objects::masters as work;
use genie_model::objects::masters::CostAmount;
use genie_model::objects::masters::FlyMode;
use genie_model::objects::masters::{ActionType, MasterId};
use genie_model::players::AmountRate;
use genie_model::players::CivId;
use genie_model::players::attribute::AttributeId;
use genie_model::technology::CostPaid;
use genie_model::technology::EffectId;
use genie_model::technology::TechId;
use genie_model::world::Seconds;
use genie_model::world::WholeSeconds;
use genie_save::generated::world::PlayerObjects;
use std::collections::HashMap;

mod dat_work;

pub use self::dat_work::attributes;
use self::dat_work::dat_work;
use crate::tables::DropSitesError;
use crate::tables::UnitLinesError;
use crate::tables::drop_sites;
use crate::tables::linked_techs;
use crate::tables::unit_lines;
use genie_dat::de::{Effect, Error, ResearchResourceCost, Sound, Tech, Unit};
use genie_model::map::terrain::{TerrainKind, terrain_class};
use genie_model::map::{Restriction, TerrainMask};
use genie_model::objects::lines::UnitLine;
use genie_model::objects::sprite::Graphic;
use genie_model::objects::{Graphics, SpriteFlags};
use genie_model::players::history::HISTORY_PERIOD;
use genie_model::players::history::HistoryUpdates;
use genie_model::technology::{Cost, EffectCommand, Linked, TechDef, TechMode, TechType};
use genie_model::unit_ai::task_finder::DropSite;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::fs::{File, read_to_string};
use std::iter::once;
use std::path::Path;

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

/// The master a save's raw id `id` names in the data file ([`PlayerObjects::dat_master`]: the
/// master an upgraded one copies); none for a negative id, which no row has.
#[must_use]
pub fn dat_id(objects: &PlayerObjects, id: i16) -> Option<MasterId> {
    MasterId::of_data(objects.dat_master(id))
}

/// A row under its civ and master id.
type KeyedRow = ((CivId, MasterId), Row);

/// The rows of every unit every civ of `dat` has ([`dat_row`]), by civ and master id.
///
/// # Errors
///
/// A task or a cost slot that holds no type or attribute.
fn dat_rows(dat: &DatFile) -> Result<Vec<KeyedRow>, GenieError> {
    let units = dat.civs.iter().enumerate().flat_map(|(c, civ)| {
        civ.units.iter().enumerate().filter_map(move |(m, unit)| {
            Some((
                CivId::new(c as u8),
                MasterId::of_data(m as i16)?,
                unit.as_ref()?,
            ))
        })
    });
    units
        .map(|(c, m, unit)| dat_row(m, unit, &dat.sounds).map(|r| ((c, m), r)))
        .collect()
}

/// A `.dat` unit's row (`id`, its slot and master id): its dead-object values (decay, undead),
/// its default action, its work-command values, turning and flight.
///
/// # Errors
///
/// A task or a cost slot that holds no type or attribute.
fn dat_row(id: MasterId, unit: &Unit, sounds: &[Sound]) -> Result<Row, GenieError> {
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
    Ok(Row {
        data: MasterData {
            decay: AmountRate::new(unit.resource_decay),
            undead: unit.undead_mode != 0,
        },
        default_action,
        work: Some(dat_work(id, unit, sounds)?),
        turning: unit.dead_fish.as_ref().map(|d| Turning {
            move_algorithm: MoveAlgorithm::from_bits_retain(d.old_move_algorithm),
            min_collision_size_multiplier: d.min_collision_size_multiplier,
        }),
        projectile_type: unit
            .projectile
            .as_ref()
            .map(|p| ProjectileType::of_data(p.projectile_type)),
        fly_mode: FlyMode::of_data(unit.fly_mode),
    })
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
    projectile_type: Option<ProjectileType>,
    fly_mode: FlyMode,
}

/// What the save does not hold of a moving master's movement (the `.dat` unit's moving part).
/// Its turning is not here: every copy of a master derives it from its turn speed and walking
/// sprite ([`turn_rate`](fn@genie_model::movement::turn_rate)).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turning {
    /// Master `+0x768`, `old_move_algorithm`: bit 0 has a stop reset the unit's sprite.
    pub move_algorithm: MoveAlgorithm,
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
    kinds: Vec<TerrainKind>,
    masks: Vec<TerrainMask>,
    /// Each terrain restriction's value per terrain (`passable_buildable_dmg_multiplier`).
    passable: Vec<Restriction>,
}

impl TerrainTable {
    /// The terrain table of `dat`: each terrain's restrictions and the restrictions' passability.
    #[must_use]
    pub fn from_dat(dat: &DatFile) -> Self {
        let classes: Vec<u8> = dat
            .terrain_block
            .terrains
            .iter()
            .map(|t| terrain_class(TerrainFlags::from_bits_retain(t.is_water)).bits())
            .collect();
        let masks = dat
            .terrain_restrictions
            .iter()
            .map(|r| {
                let passable = r.passable_buildable_dmg_multiplier.iter().enumerate();
                TerrainMask::of_bits(
                    passable
                        .filter(|&(_, &v)| 0.0 < v)
                        .fold(0, |m, (t, _)| m | classes.get(t).copied().unwrap_or(0)),
                )
            })
            .collect();
        let passable = dat
            .terrain_restrictions
            .iter()
            .map(|r| Restriction::new(r.passable_buildable_dmg_multiplier.clone()))
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
                TerrainKind {
                    shown: t.enabled != 0 || 0 <= t.terrain_to_draw,
                    flags: TerrainFlags::from_bits_retain(t.is_water),
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
    pub fn masks(&self) -> &[TerrainMask] {
        &self.masks
    }

    /// Each terrain restriction's value per terrain, by restriction id.
    #[must_use]
    pub fn passable(&self) -> &[Restriction] {
        &self.passable
    }

    /// Each terrain's record as the map's terrain reads see it.
    #[must_use]
    pub fn kinds(&self) -> &[TerrainKind] {
        &self.kinds
    }
}

/// Every master of every civ of a data file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MasterTable {
    rows: HashMap<(CivId, MasterId), Row>,
    /// Each sprite's speed factor (the `.dat` graphic's `speed_multiplier`, sprite `+0xc`).
    sprite_speed: HashMap<SpriteId, SpeedMultiplier>,
    /// What the sprite clock reads of each graphic.
    pub graphics: Graphics,
    /// The terrain table.
    pub terrain: TerrainTable,
    /// The drop-site table (`dropsites.json` beside the `.dat`, [`drop_sites`]); empty
    /// when not loaded.
    pub drop_sites: Vec<DropSite>,
    /// The linked technologies (`linkedTechs.json` beside the `.dat`, [`linked_techs`]);
    /// empty when not loaded.
    pub linked_techs: BTreeMap<TechId, Linked>,
    /// The unit lines (`unitlines.json` beside the `.dat`, [`unit_lines`]) as the file
    /// holds them; empty when not loaded (the game then builds tables of no line).
    pub unit_lines: Vec<UnitLine>,
    /// The technologies, by id.
    pub techs: Vec<TechDef>,
    /// The effects' commands, by id (the world's effect table).
    pub effects: Vec<Vec<EffectCommand>>,
    /// The history period (the `.dat`'s `time_slice`, which the world keeps at `+0x8b8`); the
    /// world constructor's [`genie_model::players::history::HISTORY_PERIOD`] without a `.dat`.
    pub time_slice: HistoryUpdates,
}

/// A technology of the `.dat` as the tech tree holds it: its costs, its type and its effect's
/// commands.
fn tech_def(dat: &DatFile, t: &Tech) -> Result<TechDef, GenieError> {
    let effect = usize::try_from(t.effect_id)
        .ok()
        .and_then(|e| dat.effects.get(e))
        .map_or_else(|| Ok(Vec::new()), effect_commands)?;
    Ok(TechDef {
        costs: tech_costs(t)?,
        kind: TechType::from(t.kind),
        effect_id: EffectId::of_data(t.effect_id),
        effect,
        required: t.required_techs.map(TechId::of_data),
        required_count: t.required_tech_count,
        civ: CivId::of_tech(t.civ)?,
        mode: TechMode::of_data(t.full_tech_mode),
        locations: t
            .research_locations
            .iter()
            .map(|l| {
                (
                    MasterId::of_data(l.location_id),
                    WholeSeconds::of_data(l.queue_time),
                )
            })
            .collect(),
    })
}

/// A technology's three costs: each its resource, amount and whether it is paid.
fn tech_costs(t: &Tech) -> Result<[Cost; 3], GenieError> {
    let [c0, c1, c2] = t.resource_costs.each_ref();
    let cost = |c: &ResearchResourceCost| -> Result<_, GenieError> {
        Ok(Cost {
            attribute: AttributeId::of_field(c.kind)?,
            amount: CostAmount::of_data(c.amount),
            paid: CostPaid::of_data(c.flag),
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
type TechTables = (Vec<TechDef>, Vec<Vec<EffectCommand>>);

/// An effect's commands as the engine's effect table holds them, each parsed by its kind
/// ([`genie_model::technology::EffectCommand::of_data`]).
fn effect_commands(e: &Effect) -> Result<Vec<EffectCommand>, GenieError> {
    e.commands
        .iter()
        .map(|c| EffectCommand::of_data(c.kind, c.a, c.b, c.c, c.d))
        .collect()
}

/// What stops [`MasterTable::read`].
#[derive(Debug)]
pub enum ReadError {
    /// The data file could not be opened or read.
    Dat(Error),
    /// The drop-site table beside it is out of shape.
    DropSites(DropSitesError),
    /// The linked technologies beside it are out of shape.
    LinkedTechs(serde_json::Error),
    /// The unit lines beside it are out of shape.
    UnitLines(UnitLinesError),
}

impl Display for ReadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        match self {
            Self::Dat(e) => write!(f, "the data file: {e}"),
            Self::DropSites(e) => write!(f, "dropsites.json: {e}"),
            Self::LinkedTechs(e) => write!(f, "linkedTechs.json: {e}"),
            Self::UnitLines(e) => write!(f, "unitlines.json: {e}"),
        }
    }
}

impl StdError for ReadError {}

impl MasterTable {
    /// The table of the data file at `path` (an `empires2_x2_p1.dat`), with the drop-site table
    /// (`dropsites.json`), the linked technologies (`linkedTechs.json`) and the unit lines
    /// (`unitlines.json`) beside it when there are.
    ///
    /// # Errors
    ///
    /// When the data file cannot be read, or a table beside it is out of shape.
    pub fn read(path: &Path) -> Result<Self, ReadError> {
        let file = File::open(path).map_err(|e| {
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
    fn read_tables_beside(&mut self, path: &Path) -> Result<(), ReadError> {
        let table = self;
        if let Ok(json) = read_to_string(path.with_file_name("dropsites.json")) {
            table.drop_sites = drop_sites(&json).map_err(ReadError::DropSites)?;
        }
        if let Ok(json) = read_to_string(path.with_file_name("linkedTechs.json")) {
            table.linked_techs = linked_techs(&json).map_err(ReadError::LinkedTechs)?;
        }
        if let Ok(json) = read_to_string(path.with_file_name("unitlines.json")) {
            table.unit_lines = unit_lines(&json).map_err(ReadError::UnitLines)?;
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
            .filter_map(|(id, g)| {
                let id = SpriteId::of_data(i16::try_from(id).ok()?)?;
                Some((id, SpeedMultiplier::of_data(g.as_ref()?.speed_multiplier)))
            })
            .collect();
        table.graphics = dat
            .graphics
            .iter()
            .enumerate()
            .filter_map(|(id, g)| {
                let g = g.as_ref()?;
                let id = SpriteId::of_data(i16::try_from(id).ok()?)?;
                Some((
                    id,
                    Graphic {
                        id,
                        frames: Frame::of_data(g.frame_count),
                        facets: FacetCount::of_data(g.angle_count),
                        speed_multiplier: SpeedMultiplier::of_data(g.speed_multiplier),
                        frame_duration: Seconds::new(g.frame_duration),
                        replay_delay: Seconds::new(g.replay_delay),
                        flags: SpriteFlags::from_bits_retain(g.sequence_type),
                    },
                ))
            })
            .collect();
        (table.techs, table.effects) = tech_tables(dat)?;
        table.time_slice = HistoryUpdates::of_data(dat.time_slice);
        Ok(table)
    }

    /// A table of the given `((civ, master), data, default action)` rows.
    pub fn from_rows(
        rows: impl IntoIterator<Item = ((CivId, MasterId), MasterData, DefaultAction)>,
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
                            fly_mode: FlyMode::default(),
                        },
                    )
                })
                .collect(),
            sprite_speed: HashMap::new(),
            graphics: Graphics::default(),
            terrain: TerrainTable::default(),
            drop_sites: Vec::new(),
            linked_techs: BTreeMap::new(),
            unit_lines: Vec::new(),
            techs: Vec::new(),
            effects: Vec::new(),
            time_slice: HISTORY_PERIOD,
        }
    }

    /// The dead-object values of `master` for `civ`; None if the data file has no such master.
    #[must_use]
    pub fn get(&self, civ: CivId, master: MasterId) -> Option<MasterData> {
        self.rows.get(&(civ, master)).map(|r| r.data)
    }

    /// The default action of `master` for `civ`; None if the data file has no such master.
    #[must_use]
    pub fn default_action(&self, civ: CivId, master: MasterId) -> Option<DefaultAction> {
        self.rows.get(&(civ, master)).map(|r| r.default_action)
    }

    /// The `.dat`'s work-command values of a civ's master: the task swap group the save lacks, and
    /// the tasks a master without saved ones has.
    #[must_use]
    pub fn work(&self, civ: CivId, master: MasterId) -> Option<&work::Master> {
        self.rows.get(&(civ, master)).and_then(|r| r.work.as_ref())
    }

    /// How a civ's moving master turns; None for a master without a moving part.
    #[must_use]
    pub fn turning(&self, civ: CivId, master: MasterId) -> Option<Turning> {
        self.rows.get(&(civ, master)).and_then(|r| r.turning)
    }

    /// A missile master's `projectile_type` (`+0x880`).
    #[must_use]
    pub fn projectile_type(&self, civ: CivId, master: MasterId) -> Option<ProjectileType> {
        self.rows
            .get(&(civ, master))
            .and_then(|r| r.projectile_type)
    }

    /// A master's `fly_mode` (`+0x608`): 0 keeps its objects on the ground, 1 above it.
    #[must_use]
    pub fn fly_mode(&self, civ: CivId, master: MasterId) -> Option<FlyMode> {
        self.rows.get(&(civ, master)).map(|r| r.fly_mode)
    }

    /// A sprite's speed factor; None for a sprite the data file does not have.
    #[must_use]
    pub fn sprite_speed(&self, sprite: SpriteId) -> Option<SpeedMultiplier> {
        self.sprite_speed.get(&sprite).copied()
    }
}

/// Tables without a data file, which the tools' tests build: one master's row, the sprites' speed
/// factors and graphics and the terrain restrictions' masks.
impl MasterTable {
    /// A table of the master `master` of `civ`, turning as `turning` and flying as `fly_mode`.
    #[must_use]
    pub fn of_master(
        civ: CivId,
        master: MasterId,
        turning: Option<Turning>,
        fly_mode: FlyMode,
    ) -> Self {
        let row = Row {
            data: MasterData::default(),
            default_action: DefaultAction::None,
            work: None,
            turning,
            projectile_type: None,
            fly_mode,
        };
        Self {
            rows: once(((civ, master), row)).collect(),
            ..Self::default()
        }
    }

    /// The sprite `sprite`'s speed factor and graphic of `facets` facets and `flags`.
    #[must_use]
    pub fn with_sprite(
        mut self,
        sprite: SpriteId,
        speed: SpeedMultiplier,
        facets: FacetCount,
        flags: SpriteFlags,
    ) -> Self {
        let _ = self.sprite_speed.insert(sprite, speed);
        let graphic = Graphic {
            id: sprite,
            frames: Frame::ONE,
            facets,
            speed_multiplier: speed,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags,
        };
        let _ = self.graphics.insert(sprite, graphic);
        self
    }

    /// The default action of the table's master `master` of `civ`; none for a master the table
    /// does not have.
    #[must_use]
    pub fn with_default_action(
        mut self,
        civ: CivId,
        master: MasterId,
        action: DefaultAction,
    ) -> Option<Self> {
        self.rows.get_mut(&(civ, master))?.default_action = action;
        Some(self)
    }

    /// The terrain restrictions' masks.
    #[must_use]
    pub fn with_masks(mut self, masks: Vec<u8>) -> Self {
        self.terrain.masks = masks.into_iter().map(TerrainMask::of_bits).collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dat::dat_work::sound_event;
    #[cfg(test)]
    use genie_dat::de::{DebugString, SoundItem};
    #[cfg(test)]
    use genie_model::objects::lines::LineId;
    #[cfg(test)]
    use genie_model::objects::masters::action_type::HUNT_RANGE;
    #[cfg(test)]
    use genie_model::players::AmountRate;
    #[cfg(test)]
    use std::env::temp_dir;
    #[cfg(test)]
    use std::fs::{create_dir_all, remove_dir_all, write};
    #[cfg(test)]
    use std::iter::empty;
    #[cfg(test)]
    use std::path::PathBuf;
    #[cfg(test)]
    use std::process::id;

    /// The Artifact action's type (genie-core's `ownership::artifact::ARTIFACT_TYPE`).
    const ARTIFACT_TYPE: ActionType = ActionType::named(107);

    /// A directory of its own under the system's temporary one, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let dir = temp_dir().join(format!("genie-import-{}-{name}", id()));
        let _ = remove_dir_all(&dir);
        create_dir_all(&dir).unwrap();
        dir
    }

    /// A data file that does not open is the data file's error; one that is not a `.dat`
    /// too.
    #[test]
    fn data_file_that_does_not_read_is_its_error() {
        let dir = scratch("no-dat");
        let path = dir.join("empires2_x2_p1.dat");
        assert!(matches!(MasterTable::read(&path), Err(ReadError::Dat(_))));
        write(&path, b"not a dat").unwrap();
        assert!(matches!(MasterTable::read(&path), Err(ReadError::Dat(_))));
        remove_dir_all(dir).unwrap();
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
        write(dir.join("dropsites.json"), r#"{"drop_site_list": []}"#).unwrap();
        write(dir.join("linkedTechs.json"), r#"{"LinkedTechs": []}"#).unwrap();
        write(dir.join("unitlines.json"), r#"{"UnitLines": []}"#).unwrap();
        table.read_tables_beside(&path).unwrap();
        for (file, check) in [
            ("dropsites.json", 0),
            ("linkedTechs.json", 1),
            ("unitlines.json", 2),
        ] {
            let good = read_to_string(dir.join(file)).unwrap();
            write(dir.join(file), "[").unwrap();
            let error = table.read_tables_beside(&path).unwrap_err();
            let kind = match error {
                ReadError::DropSites(_) => 0,
                ReadError::LinkedTechs(_) => 1,
                ReadError::UnitLines(_) => 2,
                ReadError::Dat(_) => 3,
            };
            assert_eq!(kind, check, "{file}");
            assert_ne!(error.to_string(), "");
            write(dir.join(file), good).unwrap();
        }
        remove_dir_all(dir).unwrap();
    }

    /// The fields' −1 is no attribute and any other value 0 or more its attribute, in their
    /// order; a value below −1 fails the parse.
    #[test]
    fn the_attribute_fields_parse_in_their_order() {
        use genie_model::players::attribute::AttributeId;
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

    fn sound(files: usize) -> Sound {
        let item = SoundItem {
            file_name: DebugString::default(),
            resource_id: 0,
            probability: 100,
            civilization: -1,
            icon_set: -1,
        };
        Sound {
            id: 0,
            play_delay: 0,
            file_count: files as i16,
            cache_time: 0,
            total_probability: 100,
            items: vec![item; files],
        }
    }

    #[test]
    fn task_sound_event_of_0_with_sound_of_files_takes_the_default_event() {
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
            default_action(0, [(0, ARTIFACT_TYPE), (1, HUNT_RANGE)].into_iter()),
            DefaultAction::Type(ARTIFACT_TYPE)
        );
        assert_eq!(
            default_action(-1, [(0, ARTIFACT_TYPE)].into_iter()),
            DefaultAction::None
        );
        // FARMDROP and two others name task 0 but list no task
        assert_eq!(default_action(0, empty()), DefaultAction::NoSuchTask);
    }

    #[test]
    fn rows_are_found_by_civ_and_master() {
        let t = MasterTable::from_rows([
            (
                (CivId::new(1), MasterId::of_data(332).unwrap()),
                MasterData {
                    decay: AmountRate::new(1.0),
                    undead: false,
                },
                DefaultAction::None,
            ),
            (
                (CivId::new(2), MasterId::of_data(705).unwrap()),
                MasterData {
                    decay: AmountRate::new(0.25),
                    undead: false,
                },
                DefaultAction::Type(ARTIFACT_TYPE),
            ),
        ]);
        assert_eq!(
            t.get(CivId::new(1), MasterId::of_data(332).unwrap())
                .map(|d| d.decay),
            Some(AmountRate::new(1.0))
        );
        assert_eq!(
            t.default_action(CivId::new(2), MasterId::of_data(705).unwrap()),
            Some(DefaultAction::Type(ARTIFACT_TYPE))
        );
        assert_eq!(
            (
                t.get(CivId::new(2), MasterId::of_data(332).unwrap()),
                t.default_action(CivId::new(1), MasterId::of_data(705).unwrap())
            ),
            (None, None)
        );
    }

    #[test]
    fn unit_lines_read_each_entrys_keys_as_the_builder_does() {
        use genie_model::objects::lines::UnitLine;
        // two entries of the 158041 file, one without its chain, one with an array of names
        let json = r#"{"Hint": "LineIDs have to be between (-400, -199).", "UnitLines": [
            {"Name": "Wall Line", "Identifier": "stone-wall-line", "Building": true,
             "LineID": -399, "IDChain": [117, 155]},
            {"Name": "Eagle Warrior Line", "Identifier": ["eagle-warrior-line",
             "eagle-scout-line"], "LineID": -266, "IDChain": [751, 753, 752]},
            {"Name": "Broken", "LineID": -1}
        ]}"#;
        let lines = unit_lines(json).unwrap();
        assert_eq!(
            lines,
            [
                UnitLine {
                    line: Some(LineId::of_data(-399)),
                    chain: Some(vec![117, 155]),
                    building: true,
                    identifiers: vec!["stone-wall-line".to_owned()],
                },
                UnitLine {
                    line: Some(LineId::of_data(-266)),
                    chain: Some(vec![751, 753, 752]),
                    building: false,
                    identifiers: vec![
                        "eagle-warrior-line".to_owned(),
                        "eagle-scout-line".to_owned()
                    ],
                },
                UnitLine {
                    line: Some(LineId::of_data(-1)),
                    ..UnitLine::default()
                },
            ]
        );
        assert!(unit_lines(r#"{"UnitLines": [{"LineID": "x"}]}"#).is_err());
        assert_eq!(unit_lines("{}").unwrap(), []);
    }

    #[test]
    fn linked_techs_take_their_groups_kind() {
        use genie_model::technology::Linked;
        // two groups of the 158041 file, and one of a kind the game does not know
        let json = r#"{"LinkedTechs": [
            {"NameId": 424008, "Comment": "Krypteia/Peloponnesian League",
             "Type": "MutuallyExclusive", "Techs": [1131, 1133]},
            {"NameId": 424010, "Comment": "Trade", "Type": "Cycle", "Techs": [1215, 1216],
             "StartingTech": 1215, "Instant": true},
            {"NameId": 1, "Comment": "", "Type": "Other", "Techs": [7]}
        ]}"#;
        let linked = linked_techs(json).unwrap();
        assert_eq!(
            linked
                .into_iter()
                .map(|(t, k)| (t.get(), k))
                .collect::<Vec<_>>(),
            [
                (1131, Linked::MutuallyExclusive),
                (1133, Linked::MutuallyExclusive),
                (1215, Linked::Cycle),
                (1216, Linked::Cycle)
            ]
        );
    }
}
