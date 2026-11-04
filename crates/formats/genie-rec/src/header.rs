use crate::element::{ReadableHeaderElement, read_counted};
use crate::fits;
use crate::map::Map;
use crate::player::Player;
use crate::reader::RecordingHeaderReader;
use crate::string_table::StringTable;
use crate::{GameVersion, Result};
use byteorder::{LE, ReadBytesExt};
use genie_scx::TribeScen;
pub use genie_support::SpriteId;
use genie_support::{ReadSkipExt, read_bytes};
use std::fmt::{self, Debug};
use std::io::Read;

/// A fact or an action of an AI rule.
#[derive(Debug, Default, Clone, Copy)]
pub struct AiCommand {
    /// The kind of command.
    pub command_kind: i32,
    /// The fact or action id.
    pub id: u16,
    /// The command's four parameters.
    pub parameters: [i32; 4],
}

impl ReadableHeaderElement for AiCommand {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut cmd = Self {
            command_kind: input.read_i32::<LE>()?,
            id: input.read_u16::<LE>()?,
            ..Default::default()
        };

        input.skip(2)?;
        input.read_i32_into::<LE>(&mut cmd.parameters)?;
        Ok(cmd)
    }
}

/// A rule of an AI script: its facts (conditions) and actions.
#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub struct AiListRule {
    in_use: bool,
    enable: bool,
    rule_id: u16,
    next_in_group: u16,
    facts: Vec<AiCommand>,
    actions: Vec<AiCommand>,
}

impl ReadableHeaderElement for AiListRule {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut rule = Self {
            in_use: input.read_u32::<LE>()? != 0,
            enable: input.read_u32::<LE>()? != 0,
            rule_id: input.read_u16::<LE>()?,
            next_in_group: input.read_u16::<LE>()?,
            ..Default::default()
        };
        let num_facts = input.read_u8()?;
        let num_facts_actions = input.read_u8()?;
        let _padding = input.read_u16::<LE>()?;
        for i in 0..16 {
            let cmd = AiCommand::read_from(input)?;
            if i < num_facts {
                rule.facts.push(cmd);
            } else if i < num_facts_actions {
                rule.actions.push(cmd);
            }
        }
        Ok(rule)
    }
}

/// The rule list of an AI script.
#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub struct AiList {
    in_use: bool,
    id: i32,
    max_rules: u16,
    rules: Vec<AiListRule>,
}

impl ReadableHeaderElement for AiList {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut list = Self {
            in_use: input.read_u32::<LE>()? != 0,
            id: input.read_i32::<LE>()?,
            max_rules: input.read_u16::<LE>()?,
            ..Default::default()
        };
        let num_rules = input.read_u16::<LE>()?;
        // Not known yet; probably the in-memory list pointer, as in the string table.
        let _rules_ptr = input.read_u32::<LE>()?;
        for _ in 0..num_rules {
            list.rules.push(AiListRule::read_from(input)?);
        }
        Ok(list)
    }
}

/// The rule groups of an AI script.
#[derive(Debug, Default, Clone)]
pub struct AiGroupTable {
    #[allow(dead_code)]
    max_groups: u16,
    groups: Vec<u16>,
}

impl ReadableHeaderElement for AiGroupTable {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut table = Self {
            max_groups: input.read_u16::<LE>()?,
            ..Default::default()
        };

        let num_groups = input.read_u16::<LE>()?;
        // Not known yet; probably the in-memory list pointer, as in the string table.
        let _groups_ptr = input.read_u32::<LE>()?;
        for _ in 0..num_groups {
            table.groups.push(input.read_u16::<LE>()?);
        }
        Ok(table)
    }
}

/// The game settings and AI state the AI scripts' facts read.
#[derive(Clone)]
pub struct AiFactState {
    /// The save version of the AI state.
    pub save_version: f32,
    /// The version of the AI state.
    pub version: f32,
    /// Whether the game is a death match.
    pub death_match: bool,
    /// Whether the game is a regicide game.
    pub regicide: bool,
    /// The map size setting.
    pub map_size: u8,
    /// The map type setting.
    pub map_kind: u8,
    /// The starting resources setting.
    pub starting_resources: u8,
    /// The starting age setting.
    pub starting_age: u8,
    /// Whether cheats are allowed.
    pub cheats_enabled: bool,
    /// The difficulty setting.
    pub difficulty: u8,
    /// The AI timers: 10 for each of the 8 players.
    pub timers: [[i32; 10]; 8],
    /// The 256 goals the AIs share.
    pub shared_goals: [u32; 256],
    /// The 256 AI signals, which scenario triggers can set.
    pub signals: [u32; 256],
    /// The 256 AI trigger values.
    pub triggers: [u32; 256],
    /// The 256 taunt values for each of the 8 players.
    pub taunts: [[i8; 256]; 8],
}

impl Debug for AiFactState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AIFactState")
            .field("save_version", &self.save_version)
            .field("version", &self.version)
            .field("death_match", &self.death_match)
            .field("regicide", &self.regicide)
            .field("map_size", &self.map_size)
            .field("map_type", &self.map_kind)
            .field("starting_resources", &self.starting_resources)
            .field("starting_age", &self.starting_age)
            .field("cheats_enabled", &self.cheats_enabled)
            .field("difficulty", &self.difficulty)
            .field("timers", &"...")
            .field("shared_goals", &"...")
            .field("signals", &"...")
            .field("triggers", &"...")
            .field("taunts", &"...")
            .finish()
    }
}

impl ReadableHeaderElement for AiFactState {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let save_version = input.read_f32::<LE>()?;
        let version = input.read_f32::<LE>()?;
        let death_match = input.read_u32::<LE>()? != 0;
        let regicide = input.read_u32::<LE>()? != 0;
        let map_size = fits(input.read_u32::<LE>()?)?;
        let map_kind = fits(input.read_u32::<LE>()?)?;
        let starting_resources = fits(input.read_u32::<LE>()?)?;
        let starting_age = fits(input.read_u32::<LE>()?)?;
        let cheats_enabled = input.read_u32::<LE>()? != 0;
        let difficulty = fits(input.read_u32::<LE>()?)?;
        let mut timers = [[0; 10]; 8];
        let mut shared_goals = [0; 256];
        let mut signals = [0; 256];
        let mut triggers = [0; 256];
        let mut taunts = [[0; 256]; 8];
        for timer_values in &mut timers {
            input.read_i32_into::<LE>(&mut timer_values[..])?;
        }
        input.read_u32_into::<LE>(&mut shared_goals)?;
        input.read_u32_into::<LE>(&mut signals)?;
        input.read_u32_into::<LE>(&mut triggers)?;
        for taunts in &mut taunts {
            input.read_i8_into(&mut taunts[..])?;
        }

        Ok(Self {
            save_version,
            version,
            death_match,
            regicide,
            map_size,
            map_kind,
            starting_resources,
            starting_age,
            cheats_enabled,
            difficulty,
            timers,
            shared_goals,
            signals,
            triggers,
            taunts,
        })
    }
}

/// The AI scripts the game runs, with their state.
#[derive(Debug, Clone)]
pub struct AiScripts {
    /// The strings the scripts use.
    pub string_table: StringTable,
    /// The rule lists, one per script.
    pub lists: Vec<AiList>,
    /// The group tables, one per rule list.
    pub groups: Vec<AiGroupTable>,
    /// The game settings and AI state the scripts read.
    pub fact_state: AiFactState,
}

impl ReadableHeaderElement for AiScripts {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let string_table = StringTable::read_from(input)?;
        let _max_facts = input.read_u16::<LE>()?;
        let _max_actions = input.read_u16::<LE>()?;
        let max_lists = input.read_u16::<LE>()?;

        let mut lists = vec![];
        for _ in 0..max_lists {
            lists.push(AiList::read_from(input)?);
        }

        let mut groups = vec![];
        for _ in 0..max_lists {
            groups.push(AiGroupTable::read_from(input)?);
        }

        let fact_state = AiFactState::read_from(input)?;

        Ok(Self {
            string_table,
            lists,
            groups,
            fact_state,
        })
    }
}

/// The recorded game header (the layout before DE): versions, AI scripts, map, players and the
/// scenario.
#[derive(Debug, Default)]
pub struct Header {
    game_version: GameVersion,
    save_version: f32,
    ai_scripts: Option<AiScripts>,
    map: Map,
    particle_system: ParticleSystem,
    players: Vec<Player>,
    scenario: TribeScen,
}

impl Header {
    /// The players, in the header's order.
    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.players.iter()
    }
}

impl ReadableHeaderElement for Header {
    // Reads the header's parts in file order; the parts it does not keep it skips.
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let mut header = Self {
            game_version: GameVersion::read_from(input)?,
            save_version: input.read_f32::<LE>()?,
            ..Default::default()
        };

        // update reader state
        input.set_version(header.game_version, header.save_version);

        let includes_ai = input.read_u32::<LE>()? != 0;
        if includes_ai {
            header.ai_scripts = Some(AiScripts::read_from(input)?);
        }

        skip_world_clock(input)?;
        let num_players = input.read_u16::<LE>()?;
        input.set_num_players(num_players);
        skip_game_mode_and_campaign(input)?;

        header.map = Map::read_from(input)?;

        // TODO is there another num_players here for restored games?

        header.particle_system = ParticleSystem::read_from(input)?;

        if header.save_version >= 11.07 {
            let _identifier = input.read_u32::<LE>()?;
        }

        header.players = (0..num_players)
            .map(|_| Player::read_from(input))
            .collect::<Result<_>>()?;
        for player in &mut header.players {
            player.read_info(input)?;
        }

        header.scenario = TribeScen::read_from(&mut *input)?;

        skip_lobby_players(input)?;
        skip_sent_commanded_units(input)?;

        let _num_paths = input.read_u32::<LE>()?;
        // TODO: Read paths
        // TODO: Read unit groups

        Ok(header)
    }
}

/// The world's clock and speed, the next object ids, the random seeds and the current
/// player: skipped.
fn skip_world_clock<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<()> {
    let _old_time = input.read_u32::<LE>()?;
    let _world_time = input.read_u32::<LE>()?;
    let _old_world_time = input.read_u32::<LE>()?;
    let _world_time_delta = input.read_u32::<LE>()?;
    let _world_time_delta_seconds = input.read_f32::<LE>()?;
    let _timer = input.read_f32::<LE>()?;
    let _game_speed = input.read_f32::<LE>()?;
    let _temp_pause = input.read_i8()?;
    let _next_object_id = input.read_u32::<LE>()?;
    let _next_reusable_object_id = input.read_i32::<LE>()?;
    let _random_seed = input.read_u32::<LE>()?;
    let _random_seed2 = input.read_u32::<LE>()?;
    let _current_player = input.read_u16::<LE>()?;
    Ok(())
}

/// The aegis and cheats flags (11.76), the game mode, the campaign (the king campaign from
/// 10.13), the player turn and each player's time delta: skipped.
fn skip_game_mode_and_campaign<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<()> {
    if input.version() >= 11.76 {
        let _aegis_enabled = input.read_u8()?;
        let _cheats_enabled = input.read_u8()?;
    }
    let _game_mode = input.read_u8()?;
    let _campaign = input.read_u32::<LE>()?;
    let _campaign_player = input.read_u32::<LE>()?;
    let _campaign_scenario = input.read_u32::<LE>()?;
    if input.version() >= 10.13 {
        let _king_campaign = input.read_u32::<LE>()?;
        let _king_campaign_player = input.read_u8()?;
        let _king_campaign_scenario = input.read_u8()?;
    }
    let _player_turn = input.read_u32::<LE>()?;
    let mut player_time_delta = [0; 9];
    input.read_u32_into::<LE>(&mut player_time_delta[..])?;
    Ok(())
}

/// The difficulty (7.16), the team lock (10.23), each of nine players' id, humanity and
/// name (11.32), their resignations (11.35) and the player count (11.36): skipped.
fn skip_lobby_players<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<()> {
    if input.version() >= 7.16 {
        let _difficulty = input.read_u32::<LE>()?;
    }
    if input.version() >= 10.23 {
        let _lock_teams = input.read_u32::<LE>()?;
    }
    if input.version() >= 11.32 {
        for _ in 0..9 {
            let _player_id = input.read_u32::<LE>()?;
            let _player_humanity = input.read_u32::<LE>()?;
            let name_length = input.read_u32::<LE>()?;
            let _name = read_bytes(input, fits(name_length)?)?;
        }
    }
    if input.version() >= 11.35 {
        let mut resigned = [0; 9];
        input.read_u32_into::<LE>(&mut resigned)?;
    }
    if input.version() >= 11.36 {
        let _num_players = input.read_u32::<LE>()?;
    }
    Ok(())
}

/// The units sent as commanded (11.38; the valid count from 11.39) and each of nine
/// players' selection: skipped.
fn skip_sent_commanded_units<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<()> {
    if input.version() < 11.38 {
        return Ok(());
    }
    let _sent_commanded_count = input.read_u32::<LE>()?;
    if input.version() >= 11.39 {
        let _sent_commanded_valid = input.read_u32::<LE>()?;
    }
    let mut sent_commanded_units = [0u32; 40];
    input.read_u32_into::<LE>(&mut sent_commanded_units)?;
    for _ in 0..9 {
        let _num_selected = input.read_u8()?;
        let mut selection = [0u32; 40];
        input.read_u32_into::<LE>(&mut selection)?;
    }
    Ok(())
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
struct Particle {
    pub start: u32,
    pub facet: u32,
    pub update: u32,
    pub sprite_id: SpriteId,
    pub location: (f32, f32, f32),
    pub flags: u8,
}

impl ReadableHeaderElement for Particle {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        Ok(Self {
            start: input.read_u32::<LE>()?,
            facet: input.read_u32::<LE>()?,
            update: input.read_u32::<LE>()?,
            sprite_id: input.read_u16::<LE>()?.into(),
            location: (
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
                input.read_f32::<LE>()?,
            ),
            flags: input.read_u8()?,
        })
    }
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
struct ParticleSystem {
    pub world_time: u32,
    pub particles: Vec<Particle>,
}

impl ReadableHeaderElement for ParticleSystem {
    fn read_from<R: Read>(input: &mut RecordingHeaderReader<R>) -> Result<Self> {
        let world_time = input.read_u32::<LE>()?;
        let particles = read_counted(input)?;
        Ok(Self {
            world_time,
            particles,
        })
    }
}

#[cfg(test)]
mod tests;
