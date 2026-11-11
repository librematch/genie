//! A player ([`Player`]): its objects, attributes and formations, and the stop of its update.

use crate::gathering::QueueCount;
use crate::map::Distance;
use crate::map::WholeDistance;
use crate::math::fmath::at_most_or_unordered;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::object_list::ObjectList;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::Posture;
use crate::players::attribute::AttributeId;
use crate::players::game_status::{Fealty, GameStatus, VictoryState};
use crate::players::history::History;
use crate::players::tracker::Tracker;
use crate::players::world_player::VictoryTimes;
use crate::players::{
    ByPlayer, Chance, CivId, CivNameOverride, Culture, EscrowShare, LotVolume, PlayerKind,
    Relations, Stance, StringId,
};
use crate::technology::{Attributes, TechState};
use crate::trace::{player_stops, player_unsupported};
use crate::vision::ghost::{Ghost, Watched};
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::Slots;
use crate::world::layer::{AttributeRoles, Roles};
use crate::world::slots::PlayerSlotsExt;
use crate::world::{LureDestination, Percent, TargetingMode, Tick, VillagerGarrison};
use crate::{GenieError, GenieResult};

/// A player's settings for the groups an order's splitter forms ([`Player::formations`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Formations {
    /// The splitter groups units at all (`+0x6c0` not 0).
    pub(crate) auto: bool,
    /// How near two units join (`+0x6c4`).
    pub(crate) influence: Distance,
    /// Units whose speeds differ by more than [`crate::orders`]'s gap do not join (`+0x6c8` 1).
    pub(crate) by_speed: bool,
    /// `mMinColumnDistance` (`+0x6b8`): a group's move along a longer path forms a column
    /// (`commandMoveTo`).
    pub(crate) min_column_distance: WholeDistance,
    /// `mColumnToLineDistance` (`+0x6bc`): where a column on its last waypoint turns to a line
    /// (`UpdateDesiredPositions`).
    pub(crate) column_to_line_distance: WholeDistance,
}

bitflags::bitflags! {
    /// The settings of a player's AI that are on or off ([`AiSettings`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct AiSwitches: u8 {
        /// `may_attack_walls` (`+0x611`, setting 6).
        const MAY_ATTACK_WALLS = 1;
        /// `livestock_only_to_tc` (`+0x612`, setting 7).
        const LIVESTOCK_ONLY_TO_TC = 1 << 1;
        /// `disable_build_help` (`+0x613`, setting 8).
        const DISABLE_BUILD_HELP = 1 << 2;
        /// `disableTowerPriority` (`+0x620`, setting 0xe).
        const DISABLE_TOWER_PRIORITY = 1 << 3;
        /// `tradeAttackEvasion` (`+0x621`, setting 0xb).
        const TRADE_ATTACK_EVASION = 1 << 4;
        /// `mVillagerAttackReset` (`+0x622`, setting 0x10).
        const VILLAGER_ATTACK_RESET = 1 << 5;
        /// `mAllowSerjeantBuilding` (`+0x660`, setting 0x12).
        const ALLOW_SERJEANT_BUILDING = 1 << 6;
    }
}

/// The settings of a player's AI the GAME command's sub-command 0xb sets one by one
/// (`FUN_140e7c330`'s inner switch), each saved (`WorldPlayerBase::save`, `FUN_14166b820`) and
/// read by its unit AIs. The two chances it sets too are [`Player::dodge_chance`] and
/// [`Player::keep_distance_chance`], and `mAllowCapturingGaiaUnits` is
/// [`crate::world::World::allow_gaia_capture`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiSettings {
    /// `easiest_reaction_percentage` (`+0x608`, setting 3).
    pub(crate) easiest_reaction_percentage: Percent,
    /// `easier_reaction_percentage` (`+0x60c`, setting 4).
    pub(crate) easier_reaction_percentage: Percent,
    /// `task_ungrouped_soldiers` (`+0x610`, setting 5).
    pub(crate) task_ungrouped_soldiers: u8,
    /// Settings 6, 7, 8, 0xb, 0xe, 0x10 and 0x12.
    pub(crate) switches: AiSwitches,
    /// `local_targeting_mode` (`+0x614`, setting 9).
    pub(crate) local_targeting_mode: TargetingMode,
    /// `targeting_factor_scalar` (`+0x618`, setting 10).
    pub(crate) targeting_factor_scalar: Percent,
    /// `preferredTradeDistance` (`+0x61c`, setting 0xd).
    pub(crate) preferred_trade_distance: Distance,
    /// `boarLureDestination` (`+0x623`, setting 0xc).
    pub(crate) boar_lure_destination: LureDestination,
    /// `mAttackMoveMaxDistance` (`+0x624`, setting 0xf).
    pub(crate) attack_move_max_distance: WholeDistance,
    /// `mDisableVillagerGarrison` (`+0x650`, setting 0x11).
    pub(crate) disable_villager_garrison: VillagerGarrison,
}

bitflags::bitflags! {
    /// The automatic queues a player turned on ([`Player`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct AutoQueues: u8 {
        /// `autoFarmQueue` (`+0x1424`, saved), which GAME 0x10 flips: a farmer whose farm ran out
        /// buys another one without the mill's queue (`FUN_141618260`).
        const FARMS = 1;
        /// `autoFishTrapQueue` (`+0x1425`, saved), which GAME 0x13 flips (`FUN_141618270`).
        const FISH_TRAPS = 1 << 1;
    }
}

bitflags::bitflags! {
    /// How a player left the game ([`Player`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Departures: u8 {
        /// `+0x8b` (saved with `+0x8c` as its bit 1): the player resigned (slot 3, RESIGN).
        const RESIGNED = 1;
        /// `+0x8c`: its resignation was a drop from the game.
        const DROPPED = 1 << 1;
    }
}

bitflags::bitflags! {
    /// What a player shares with its allies ([`Player`]).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct AllySharing: u8 {
        /// `allied_LOS_Enable` (`+0x2cc`, saved): the player shares its mutual allies' sight
        /// ([`crate::vision`]).
        const VISION = 1;
        /// `mSharingIsCaring` (`+0x1426`, `FUN_141626780`, saved): the player's gathered resources
        /// go to its allies too.
        const RESOURCES = 1 << 1;
    }
}

/// One player's objects in live-list order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Player {
    /// Its live objects, in their list's order.
    pub objects: ObjectList,
    /// `player+0x10`, the player's type (the save's class byte): 1 human, 2 Gaia, 3 computer
    /// (0 the scenario editor's phantom).
    pub(crate) kind: PlayerKind,
    /// Every attribute (`player+0x100`, count `+0x90`): the first four are food, wood, stone and
    /// gold ([`Player::resources`]).
    pub attributes: Vec<Amount>,
    /// `+0x658` (saved, the save's second attribute array): per attribute what the player's
    /// generation tasks made and did not give yet ([`crate::combat::abilities::store`]).
    pub(crate) generated: Vec<Amount>,
    /// The civ (`player+0x738`), which drop sites read.
    pub civ: CivId,
    /// `culture` (`+0x88`, saved): its civ's icon set; a building of a player without one counts
    /// no relics and no livestock ([`Culture`]).
    pub(crate) culture: Culture,
    /// `relation[i]` (`player+0x260`): how the player stands to player i.
    pub(crate) relations: Relations,
    /// The sleeping objects, in their list's order: they are not updated; a unit's work on one
    /// wakes it.
    pub sleeping: ObjectList,
    /// `autoFarmQueue` and `autoFishTrapQueue` (`+0x1424`, `+0x1425`, saved).
    pub(crate) auto_queues: AutoQueues,
    /// `farm_queue_count` (`+0x141c`, saved): the farms and pastures the mill's queue paid for
    /// (GAME 0xd and 0x15 buy, 0xe and 0x16 take back, [`crate::gathering::farm_queue`]).
    pub(crate) farm_queue: QueueCount,
    /// `fishTrapQueueCount` (`+0x1420`, saved): the fish traps the dock's queue paid for (GAME
    /// 0x11 buys, 0x12 takes back).
    pub(crate) fish_trap_queue: QueueCount,
    /// `prod_queue_on` (`+0x1358`, saved): GAME 9 sets it; nothing in the simulation reads it.
    pub(crate) production_queue_on: u8,
    /// `diplomacy[i]` (`+0xe12`, saved, 9 entries): the stance toward player i a diplomacy change
    /// set (1 or 2 enemy, 3 neutral, 4 or 5 ally; [`crate::players::diplomacy`]).
    pub(crate) diplomacy: ByPlayer<Stance>,
    /// The settings a computer player's GAME 0xb sets (`+0x608`..`+0x670`, saved).
    pub(crate) ai_settings: AiSettings,
    /// The order splitter's settings (`+0x6c0` `mAutoFormations`, `+0x6c4`
    /// `mFormationInfluenceDistance`, `+0x6c8` `mBreakAutoFormationsBySpeed`).
    pub(crate) formations: Formations,
    /// `mPendingDebit` (`+0x6cc`): what the player's BUILD and RESEARCH commands listed as their
    /// costs, food, wood, stone and gold, taken off (`FUN_141630d10`); nothing modelled reads it.
    pub pending_debit: [Amount; 4],
    /// The player's state of each technology (`player+0x760`'s, saved).
    pub techs: Vec<TechState>,
    /// The first update this slice could not model, with the tick (1-based) and object index.
    pub unsupported: Option<(Tick, ListIndex, GenieError)>,
    /// Every object the slice stopped updating, with the tick and why.
    pub freezes: Vec<(Tick, ObjectId, GenieError)>,
    /// A command the slice could not model stopped the whole player: its objects freeze.
    pub stopped: bool,
    /// `game_status` (`+0x89`, saved): playing, won or lost ([`crate::players::game_status`]).
    pub game_status: GameStatus,
    /// `+0x8b` and `+0x8c` (saved as one byte, `+0x8c` its bit 1).
    pub departures: Departures,
    /// `allied_victory` (`+0x2d0`, saved, a byte GAME 5 sets): its allies' victory counts as its
    /// own while it is not 0; the teams count it only at 1 ([`crate::players::diplomacy`]).
    pub(crate) allied_victory: u8,
    /// Its victory conditions' state (`player+0x68`'s `+0x20`, the save's `victory`): 2 when
    /// they are met (`FUN_1414f35d0`).
    pub(crate) victory: VictoryState,
    /// `fealty` (`+0x12f8`, saved): the player a defeated player's resources and units would go
    /// to.
    pub(crate) fealty: Fealty,
    /// `+0x71c`, the offensive posture (0 or 1, GAME 0x14): not saved, a load sets 0; new soldiers
    /// and scorpions read it (their unit AI's constructor, [`crate::unit_ai::NewObject`]).
    pub(crate) posture: Posture,
    /// `chance_to_dodge_missiles` (`+0xe10`, saved): a ship's unit AI dodges a missile's warning
    /// (`0x20f`) on a draw below it ([`crate::unit_ai::Surroundings::dodge_chance`]).
    pub(crate) dodge_chance: Chance,
    /// `chance_for_archers_to_maintain_distance` (`+0xe11`, saved): a computer player's siege
    /// unit backs up from its target after a shot on a draw at least this
    /// ([`crate::unit_ai::Surroundings::keep_distance_chance`]).
    pub(crate) keep_distance_chance: Chance,
    /// `allied_LOS_Enable` (`+0x2cc`) and `mSharingIsCaring` (`+0x1426`), saved.
    pub(crate) ally_sharing: AllySharing,
    /// The players whose town centres the player's reveal (attribute 209) flared
    /// (`mTCFlared[i]`, saved; [`crate::vision`]).
    pub(crate) tc_flared: Vec<bool>,
    /// `commodity_volumn_delta` (`+0x140c`, saved): the lot a market trade moves (100).
    pub(crate) commodity_volume: LotVolume,
    /// The objects whose ghosts the player's list follows (`+0x78`, [`crate::vision::ghost`]): not
    /// saved; the load builds it ([`World::build_watched`]).
    pub(crate) watched: Vec<Watched>,
    /// The player's ghosts (its doppelganger list `+0x40`), in the list's order.
    pub ghosts: Vec<Ghost>,
    /// Its history (`+0xdb8`, saved, [`crate::players::history`]).
    pub history: History,
    /// Its object tracker (`+0x18`, [`crate::players::tracker`]): not saved; the load files the objects
    /// ([`World::load_trackers`]).
    pub(crate) tracker: Tracker,
    /// `+0x1378` (not saved): in game mode 16 the player has a military unit queued while its
    /// attribute 6 is at most 1 (QUEUE sets it, `FUN_141614450`; a cancel clears it,
    /// `FUN_14161e370`).
    pub(crate) one_military_queued: bool,
    /// `mUpdateTime` (`+0x7a4`, saved): the seconds the world player's update has not yet run
    /// its per-second loop for ([`crate::players::world_player`]); Gaia's stays 0.
    pub update_time: Seconds,
    /// Gaia's own `update_time` (`WorldPlayerGaia` `+0x1468`, saved): its update's seconds.
    pub gaia_update_time: Seconds,
    /// `escrow` (`+0x6dc`, saved): what a computer player's income set aside of food, wood,
    /// stone and gold ([`World::add_resource`]).
    pub(crate) escrow: [Amount; 4],
    /// `escrow_percentage` (`+0x6ec`, saved): the shares of its income a computer player sets
    /// aside.
    pub(crate) escrow_percentage: [EscrowShare; 4],
    /// The name the interface gives its civilisation in place of its own (`+0x628`..`+0x640`,
    /// not saved, none after a load): the store of the civilisation name override's attribute
    /// names a string ([`Player::store_attribute`]).
    pub(crate) civ_name_override: CivNameOverride,
    /// `mSuddenDeathCanBeEliminated` (`+0x1380`, saved): under the sudden death rule the player
    /// had a town centre, and loses without one.
    pub(crate) sudden_death_eliminable: bool,
    /// The victory timers that keep a defeated player in the game ([`crate::players::world_player`]).
    pub(crate) victory_times: VictoryTimes,
}

/// An attribute the player does not have.
pub(crate) const NO_ATTRIBUTE: GenieError =
    GenieError::unsupported("an attribute the player does not have");

impl Attributes for Player {
    /// An attribute's value.
    fn get(&self, attribute_id: AttributeId) -> GenieResult<Amount> {
        self.attribute(attribute_id).ok_or(NO_ATTRIBUTE)
    }

    /// An attribute less `amount`, as a payment takes it (no side effects).
    fn subtract(&mut self, attribute_id: AttributeId, amount: Amount) -> GenieResult<()> {
        *self
            .attributes
            .get_mut(attribute_id.index())
            .ok_or(NO_ATTRIBUTE)? -= amount;
        Ok(())
    }
}

impl World {
    /// A command or a pass the slice does not model for player `player_id`: the player stops
    /// ([`Player::stop`]), and a subscriber hears why ([`crate::trace`]).
    /// A player the world does not hold has nothing to stop: the trace's subscriber hears that
    /// instead (every caller steps the players it counts, so it does not happen).
    pub(crate) fn stop_player(&mut self, player_id: PlayerId, why: GenieError) {
        player_stops(self.ticks, player_id, &why);
        match self.players.player_mut(player_id) {
            Ok(player) => player.stop(self.ticks, why),
            Err(missing) => player_unsupported(self.ticks, player_id, &missing),
        }
    }
}

impl Player {
    /// A command the slice does not model for this player: the player stops.
    pub(crate) fn stop(&mut self, tick: Tick, why: GenieError) {
        let _ = self
            .unsupported
            .get_or_insert((tick, ListIndex::FIRST, why));
        self.stopped = true;
    }

    /// Attribute `attribute` (`player+0x100`, decoded), None past its count (`+0x90`).
    #[must_use]
    pub fn attribute(&self, attribute: AttributeId) -> Option<Amount> {
        self.attributes.get(attribute.index()).copied()
    }

    /// Attribute `attribute` set to `amount` straight, with no slot 0xf0 and so no computer's
    /// escrow ([`Player::store_attribute`]). A technology's payment (`FUN_1413d4370`), the
    /// refunds (`FUN_141630530`, `FUN_141630770`, `FUN_1413d15e0`, `FUN_1413ccc00`) and slot
    /// 0xf0's spread of attribute 0xc4 to the other players (`FUN_14162d610` at `0x14162d834`,
    /// [`World::set_player_attribute`]) write so.
    ///
    /// [`World::set_player_attribute`]: crate::world::World::set_player_attribute
    ///
    /// # Errors
    ///
    /// An attribute the player does not have.
    pub fn set_attribute(&mut self, attribute: AttributeId, amount: Amount) -> GenieResult<()> {
        *self
            .attributes
            .get_mut(attribute.index())
            .ok_or(NO_ATTRIBUTE)? = amount;
        Ok(())
    }

    /// Food, wood, stone and gold (attributes 0 to 3; 0 for one past the count), as the
    /// checkpoint's word 1 reads them.
    #[must_use]
    pub fn resources(&self) -> [Amount; 4] {
        [0, 1, 2, 3].map(|index| self.attributes.get(index).copied().unwrap_or(Amount::ZERO))
    }

    /// Attribute 195, the construction rate modifier
    /// ([`AttributeSlots::construction_rate`](crate::world::layer::AttributeSlots::construction_rate)),
    /// 0 past the count.
    #[must_use]
    pub fn construction_rate(&self, slots: Slots) -> Amount {
        self.attribute(slots.attribute.construction_rate)
            .unwrap_or(Amount::ZERO)
    }

    /// The base player's slot 0xf0 (`FUN_1416674b0`, the attribute's store) on this player alone:
    /// within its count (`+0x90`) attribute `attribute` takes `amount`, nothing past it. The
    /// civilisation name override's attribute (0x106, `roles`'
    /// [`AttributeRoles::CIV_NAME_OVERRIDE`]) names the string of the value, the text cleared
    /// (`FUN_140930a50` with 0, [`CivNameOverride::String`]). A computer player's (`+0x10` 3)
    /// resource sets its escrow to its share of the value ([`escrow_set_to`]). Slot 0xf0's
    /// branches before it (`FUN_14162d610`) are the world's ([`World::set_player_attribute`]).
    ///
    /// [`World::set_player_attribute`]: crate::world::World::set_player_attribute
    pub(crate) fn store_attribute(
        &mut self,
        attribute: AttributeId,
        amount: Amount,
        roles: &Roles,
    ) {
        let index = attribute.index();
        let Some(stored) = self.attributes.get_mut(index) else {
            return;
        };
        *stored = amount;
        if roles
            .of_attribute(attribute)
            .contains(AttributeRoles::CIV_NAME_OVERRIDE)
        {
            self.civ_name_override = CivNameOverride::String(StringId::of_amount(amount));
        }
        if self.kind == PlayerKind::Computer
            && attribute.is_resource()
            && let (Some(escrow), Some(&share)) = (
                self.escrow.get_mut(index),
                self.escrow_percentage.get(index),
            )
        {
            *escrow = escrow_set_to(*escrow, share, amount);
        }
    }

    /// The base player's attribute add (`FUN_14165d040`) on this player alone: within its count
    /// (`+0x90`) attribute `kind` grows by `amount`, nothing past it. A computer player's
    /// (`+0x10` 3) resource sets part of it aside ([`escrow_after`]). The players it shares its
    /// resources with are the world's ([`World::add_resource`]), as are slot 0xf8's branches
    /// (`FUN_141616820`) before it: a resource's gathered totals, gold's counter and the side
    /// effects of attributes 50, 183 and 196. So a slot 0xf8 call of a kind the data names (a
    /// storage's, a cost's, a load's) adds through the world; a slot of the build layer that
    /// none of the branches names (a count, a statistic) adds here. Attribute 11's peak
    /// (`+0x6fc`, unsaved) is not held.
    ///
    /// [`World::add_resource`]: crate::world::World::add_resource
    pub fn add_attribute(&mut self, kind: AttributeId, amount: Amount) {
        let index = kind.index();
        let Some(attribute) = self.attributes.get_mut(index) else {
            return;
        };
        *attribute += amount;
        let held = *attribute;
        if self.kind == PlayerKind::Computer
            && kind.is_resource()
            && let (Some(escrow), Some(&share)) = (
                self.escrow.get_mut(index),
                self.escrow_percentage.get(index),
            )
        {
            *escrow = escrow_after(*escrow, share, amount, held);
        }
    }

    /// Takes `amount` of attribute `kind`, at least 0 left (slot 0xf8 with the amount negated,
    /// then below 0, `COMISS` and `JBE`: not a NaN, slot 0xf0 with 0), for a slot of the build
    /// layer outside the branches of slots 0xf8 and 0xf0 ([`Player::add_attribute`],
    /// [`Player::store_attribute`] with `roles`).
    pub fn take_at_least_zero(&mut self, kind: AttributeId, amount: Amount, roles: &Roles) {
        self.add_attribute(kind, -amount);
        if self.attribute(kind).is_some_and(|left| left < Amount::ZERO) {
            self.store_attribute(kind, Amount::ZERO, roles);
        }
    }

    /// [`Player::add_attribute`] for each of `adds`, in order.
    pub fn add_attributes(&mut self, adds: impl IntoIterator<Item = (AttributeId, Amount)>) {
        for (kind, amount) in adds {
            self.add_attribute(kind, amount);
        }
    }
}

/// `FUN_14165d040`'s escrow (`+0x6dc`) after a computer player's resource add of `amount`, from
/// the `escrow` before it, after which the resource holds `held`: an amount that sets some aside
/// ([`sets_aside`]) adds the share times it to the escrow (`MULSS`, then `ADDSS` onto the
/// escrow), and the escrow is held to the resource ([`escrow_held`]).
fn escrow_after(escrow: Amount, share: EscrowShare, amount: Amount, held: Amount) -> Amount {
    let added = if sets_aside(share, amount) {
        share.of(amount) + escrow
    } else {
        escrow
    };
    escrow_held(added, held)
}

/// `FUN_1416674b0`'s escrow (`+0x6dc`) after a computer player's resource set to `amount`, from
/// the `escrow` before it: a value that sets some aside ([`sets_aside`]) sets the escrow to the
/// share times it (`MULSS`, then a store, where [`escrow_after`] adds onto it), and the escrow is
/// held to the resource, now `amount` ([`escrow_held`]).
fn escrow_set_to(escrow: Amount, share: EscrowShare, amount: Amount) -> Amount {
    let set = if sets_aside(share, amount) {
        share.of(amount)
    } else {
        escrow
    };
    escrow_held(set, amount)
}

/// A computer player's resource change of `amount` sets its `share` of it aside: both above 0
/// (`FUN_14165d040`'s and `FUN_1416674b0`'s `COMISS` with 0 and `JBE`: a NaN sets nothing aside).
fn sets_aside(share: EscrowShare, amount: Amount) -> bool {
    !at_most_or_unordered(amount.get(), 0.0) && !at_most_or_unordered(share.get(), 0.0)
}

/// An `escrow` held to the resource's `held`, as `FUN_14165d040` and `FUN_1416674b0` end: above
/// it (`COMISS`, `JBE`: not with a NaN) it falls to the resource, or to 0 when the resource is
/// below 0 (`COMISS` 0 with the resource, `JBE`).
fn escrow_held(escrow: Amount, held: Amount) -> Amount {
    if at_most_or_unordered(escrow.get(), held.get()) {
        escrow
    } else if at_most_or_unordered(0.0, held.get()) {
        held
    } else {
        Amount::ZERO
    }
}
