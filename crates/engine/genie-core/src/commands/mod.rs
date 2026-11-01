//! The commands ([`Command`]) and their dispatch ([`World::run`]): the handlers of the
//! players' orders, the buildings' placement and the production queue's.
//!
//! [`World::run`]: crate::world::World::run

use crate::GenieError;
use crate::map::Position;
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::players::PlayerId;
use crate::world::World;
use genie_diff::Diff;

pub mod all_back_to_work;
pub mod attack_move;
pub mod auto_scout;
pub mod build;
mod builders;
mod building_commands;
pub mod building_queue;
pub mod delete;
pub mod flare;
pub mod form_formation;
pub mod game;
mod game_sub;
pub mod gate;
pub mod gather_point;
mod gather_point_state;
pub mod go_back_to_work;
mod ground;
mod new_building;
pub mod order;
mod packet_values;
pub mod patrol;
mod placement;
mod production;
mod production_techs;
pub mod queue;
pub mod research;
pub mod resign;
mod run;
pub mod seek_shelter;
pub mod set_gather_state;
pub mod stop;
pub mod town_bell;
pub mod trade;
pub mod transform_object;
pub mod tribute;
pub mod unit_ai_state;
pub mod unit_order;
pub mod units;
pub mod unload;
pub mod unmodelled;
pub mod wall;

pub use self::all_back_to_work::AllBackToWorkCommand;
pub use self::attack_move::AttackMoveCommand;
pub use self::auto_scout::AutoScoutCommand;
pub use self::build::BuildCommand;
pub use self::building_queue::BuildingQueueCommand;
pub use self::delete::DeleteCommand;
pub use self::flare::FlareCommand;
pub use self::form_formation::FormFormationCommand;
pub use self::game::GameCommand;
pub use self::game_sub::GameSub;
pub use self::gate::GateCommand;
pub use self::gather_point::GatherPointCommand;
pub use self::gather_point_state::GatherPointState;
pub use self::go_back_to_work::{GoBackToWork2Command, GoBackToWorkCommand};
pub use self::order::{Order, OrderCommand};
pub use self::packet_values::{QueueId, ResourceWord, SpriteOverride, TradeLots};
pub use self::patrol::{PatrolCommand, Waypoints};
pub use self::queue::QueueCommand;
pub use self::research::ResearchCommand;
pub use self::resign::ResignCommand;
pub use self::seek_shelter::SeekShelterCommand;
pub use self::set_gather_state::SetGatherStateCommand;
pub use self::stop::StopCommand;
pub use self::town_bell::TownBellCommand;
pub use self::trade::TradeCommand;
pub use self::transform_object::TransformObjectCommand;
pub use self::tribute::TributeCommand;
pub use self::unit_ai_state::UnitAiStateCommand;
pub use self::unit_order::{UnitOrderAction, UnitOrderCommand};
pub use self::units::UnitList;
pub use self::unload::{UnloadCommand, UnloadPick};
pub use self::unmodelled::UnmodelledCommand;
pub use self::wall::BuildWallCommand;

#[cfg(test)]
pub(crate) use self::production::least_loaded;
#[cfg(test)]
mod tests;

/// A building's gather point (`+0xc30`..`+0xc44`: its state, x, y, z, the target and the target's
/// master; the save's `gather_point_*`). A state of 0 is no gather point ([`Option`] is `None`).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct GatherPoint {
    /// `+0xc30`: its state.
    pub state: GatherPointState,
    /// The point its trained units walk to.
    pub point: Position,
    /// The object it is on, if any.
    pub target: Option<ObjectId>,
    /// The target's master, −1 without a target.
    pub target_master: Option<MasterId>,
}

/// A recorded command, run in the command phase of its tick: after the frame seed's `SeedAll`,
/// before the players update (`TRIBE_Command::do_command`, `0x140e77b00`), each by its own run
/// ([`World::run`]). The slice runs only what other players' lanes see of a command; the issuing
/// player's lane stops.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// BUILD (0x66).
    Build(BuildCommand),
    /// `BUILD_WALL` (0x69).
    BuildWall(BuildWallCommand),
    /// SET GATHER POINT (0x78).
    GatherPoint(GatherPointCommand),
    /// SET GATHER STATE (0x82).
    SetGatherState(SetGatherStateCommand),
    /// QUEUE (0x81).
    Queue(QueueCommand),
    /// RESEARCH (0x65).
    Research(ResearchCommand),
    /// A command the slice runs part of or none: it stops the issuing player's lane.
    Unmodelled(UnmodelledCommand),
    /// GAME (0x67).
    Game(GameCommand),
    /// `BUILDING_COMMAND_QUEUE` (0x2d).
    CommandQueue(BuildingQueueCommand),
    /// `GO_BACK_TO_WORK` (0x80).
    GoBackToWork(GoBackToWorkCommand),
    /// `GO_BACK_TO_WORK2` (0x86).
    GoBackToWork2(GoBackToWork2Command),
    /// `ALL_BACK_TO_WORK` (0x8c).
    AllBackToWork(AllBackToWorkCommand),
    /// `TOWN_BELL` (0x7f).
    TownBell(TownBellCommand),
    /// `CANCEL_BUILD` (0x6a).
    Delete(DeleteCommand),
    /// FLARE (0x73).
    Flare(FlareCommand),
    /// GIVE ATTRIBUTES (0xc4).
    Tribute(TributeCommand),
    /// `SELL_COMMODITY` (0x7a) and `BUY_COMMODITY` (0x7b).
    Trade(TradeCommand),
    /// GATE (0x72).
    Gate(GateCommand),
    /// UNLOAD (0x6f).
    Unload(UnloadCommand),
    /// `TRANSFORM_OBJECT` (0x29).
    TransformObject(TransformObjectCommand),
    /// RESIGN (0x0b).
    Resign(ResignCommand),
    /// `AUTO_SCOUT` (0x26).
    AutoScout(AutoScoutCommand),
    /// STOP (0x01).
    Stop(StopCommand),
    /// PATROL (0x15).
    Patrol(PatrolCommand),
    /// `ATTACK_MOVE` (0x21).
    AttackMove(AttackMoveCommand),
    /// `SEEK_SHELTER` (0x2c).
    SeekShelter(SeekShelterCommand),
    /// `UNIT_AI_STATE` (0x12).
    UnitAiState(UnitAiStateCommand),
    /// `FORM_FORMATION` (0x17).
    FormFormation(FormFormationCommand),
    /// `UNIT_ORDER` (0x75).
    UnitOrder(UnitOrderCommand),
    /// INTERACT (0x00), WORK (0x02), MOVE (0x03), RETREAT (0x23), FOLLOW or GUARD.
    Order(OrderCommand),
}

/// A match over every kind of command: `$game`, `$unmodelled` and `$trade` for the three that
/// differ (with the command as `$g`, `$u`, `$t`), `$any` for each other (with it as `$c`). One
/// flat dispatch, so that a new command is a name in this list and not a branch of every
/// function that reads them.
macro_rules! by_kind {
    (
        $command:expr,
        $c:ident => $any:expr,
        $g:ident => $game:expr,
        $u:ident => $unmodelled:expr,
        $t:ident => $trade:expr $(,)?
    ) => {
        match $command {
            Command::Game($g) => $game,
            Command::Unmodelled($u) => $unmodelled,
            Command::Trade($t) => $trade,
            Command::Build($c) => $any,
            Command::BuildWall($c) => $any,
            Command::GatherPoint($c) => $any,
            Command::SetGatherState($c) => $any,
            Command::Queue($c) => $any,
            Command::Research($c) => $any,
            Command::CommandQueue($c) => $any,
            Command::GoBackToWork($c) => $any,
            Command::GoBackToWork2($c) => $any,
            Command::AllBackToWork($c) => $any,
            Command::TownBell($c) => $any,
            Command::Delete($c) => $any,
            Command::Flare($c) => $any,
            Command::Tribute($c) => $any,
            Command::Gate($c) => $any,
            Command::Unload($c) => $any,
            Command::TransformObject($c) => $any,
            Command::Resign($c) => $any,
            Command::AutoScout($c) => $any,
            Command::Stop($c) => $any,
            Command::Patrol($c) => $any,
            Command::AttackMove($c) => $any,
            Command::SeekShelter($c) => $any,
            Command::UnitAiState($c) => $any,
            Command::FormFormation($c) => $any,
            Command::UnitOrder($c) => $any,
            Command::Order($c) => $any,
        }
    };
}

impl Command {
    /// Its run on the world, by its kind: an unmodelled command fails with its reason.
    ///
    /// # Errors
    ///
    /// What the run does not model.
    pub(crate) fn run(&self, w: &mut World) -> Result<(), GenieError> {
        by_kind!(
            self,
            c => c.run(w),
            g => g.run(w),
            u => Err(u.why.clone()),
            t => t.run(w),
        )
    }

    /// The player whose lane a failed run stops, of `players` world players: a GAME
    /// sub-command's target, else its issuer ([`GameCommand::target`]); any other command's
    /// issuing player.
    #[must_use]
    pub fn stopped_player(&self, players: usize) -> PlayerId {
        match self {
            Self::Game(g) => g.target(players).unwrap_or(g.issuer),
            other => other.player(),
        }
    }

    /// The issuing player (GAME's issuer).
    #[must_use]
    pub const fn player(&self) -> PlayerId {
        by_kind!(
            self,
            c => c.player,
            g => g.issuer,
            u => u.player,
            t => t.player,
        )
    }
}

const ONE_UNIT_AGE: f32 = crate::math::image::turn::ONE_UNIT_AGE;

/// A build order on a foundation the world no longer holds.
pub(crate) const FOUNDATION_GONE: GenieError =
    GenieError::unsupported("a build order on a foundation that is gone");

/// The units a human player's command names at most (`FUN_140e6b290`'s `0x3c`).
const HUMAN_UNITS: usize = 60;
/// The units a computer player's command names at most (`FUN_141659cf0`, 500).
const COMPUTER_UNITS: usize = 500;

impl World {
    /// The command check's units (`FUN_140e553f0`, `FUN_140e55250`): each unit the world holds
    /// (`FUN_1415417a0`, live or sleeping) that is the player's own, or any while players may
    /// order each other's units ([`crate::world::settings::Settings::cheats_enabled`]); none
    /// (the game's −1) for the others.
    pub(crate) fn checked_units(
        &self,
        player: PlayerId,
        units: &[Option<ObjectId>],
    ) -> Vec<Option<ObjectId>> {
        units
            .iter()
            .map(|&id| match self.place(id) {
                Some(p) if p.player() == player || self.settings.cheats_enabled => id,
                _ => None,
            })
            .collect()
    }

    /// `FUN_140e6b290`: the units an order of comm player `player` names. Comm player 0 and
    /// one past the world's take the packet's list as it is; a count of `0xffff` there reads
    /// past the packet (no units here). Any other takes its first [`World::most_units`], or
    /// for a count of `0xffff` its last commanded units ([`World::last_commanded`], none before
    /// its first).
    pub(crate) fn command_units(
        &self,
        player: PlayerId,
        units: &UnitList,
    ) -> Vec<Option<ObjectId>> {
        let inside = player == PlayerId::GAIA || self.players.len() <= player.index();
        match units.names_last_commanded() {
            false if inside => units.first(usize::MAX).to_vec(),
            false => units.first(self.most_units(player)).to_vec(),
            true if inside => Vec::new(),
            true => self
                .last_commanded
                .get(player.index())
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// The units a command of `player` names at most: 60, 500 for a computer
    /// (`FUN_141659cf0`); none for a player the world lacks.
    #[must_use]
    pub fn most_units(&self, player: PlayerId) -> usize {
        match self.players.get(player.index()) {
            Some(p) if p.computer => COMPUTER_UNITS,
            Some(_) => HUMAN_UNITS,
            None => 0,
        }
    }

    /// Comm player `player`'s last commanded units become `units` (`FUN_140e6d140` and the
    /// copy after it, `FUN_140e6b290` with 0); nothing for comm player 0 or one past the
    /// world's.
    pub(crate) fn set_last_commanded(&mut self, player: PlayerId, units: Vec<Option<ObjectId>>) {
        if player == PlayerId::GAIA || self.players.len() <= player.index() {
            return;
        }
        if self.last_commanded.len() <= player.index() {
            self.last_commanded.resize(player.index() + 1, Vec::new());
        }
        if let Some(slot) = self.last_commanded.get_mut(player.index()) {
            *slot = units;
        }
    }
}
