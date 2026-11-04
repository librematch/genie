//! GAME (0x67): `TRIBE_Command::do_command`'s case (`0x140e77b00`), which lets a player's
//! sub-command for another player through only with the game's cheats, and its switch on the
//! sub-command (`FUN_140e7c330`, [`World::game`]).
//!
//! 158041: the switch `FUN_140e7c330`; the world player's lookup `FUN_14152fe40`; the cases'
//! handlers are named at each.

use crate::commands::GameSub;
use crate::gathering::QueueCount;
use crate::map::Distance;
use crate::map::WholeDistance;
use crate::players::Amount;
use crate::players::Chance;
use crate::players::PlayerId;
use crate::players::Stance;
use crate::world::AiSwitches;
use crate::world::AutoQueues;
use crate::world::World;
use crate::world::settings::OptionFlags;
use crate::world::slots::CheckedIndexExt;
use crate::world::slots::PlayerSlotsExt;
use crate::world::{LureDestination, Percent, TargetingMode, VillagerGarrison};
use crate::{GenieError, GenieResult};

/// GAME's packet as `FUN_140e7c330` reads it: the sub-command at `+4` and its arguments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameCommand {
    /// Byte 1, the comm player who sent it (`FUN_140e6b210` maps it to a world player; the
    /// slice takes the two as equal).
    pub issuer: PlayerId,
    /// `+4`: the sub-command.
    pub sub: GameSub,
    /// `+8`: the world player it is for.
    pub player: i16,
    /// `+0xa`: a second player, a count, a code or a flag (its low byte).
    pub argument: i16,
    /// `+0xc`: the game speed of sub-command 1.
    pub speed: f32,
    /// `+0x10`: a relation, or an AI setting's value (its low byte for some).
    pub value: u32,
}

/// A cheat with the game's cheats on: the cheat code's handler (`FUN_1415c62c0`) and the
/// quick-build cheat's instant work (`+0xc14`, which the work actions take as 0) are not modelled.
pub(crate) const CHEAT: GenieError = GenieError::unsupported("a cheat code (FUN_1415c62c0)");

impl GameCommand {
    /// `do_command`'s case and its switch ([`World::game`]).
    pub(crate) fn run(&self, world: &mut World) -> GenieResult<()> {
        world.game(self)
    }

    /// `+8` as a world player of `count` players (`FUN_14152fe40`: none outside 0 to count − 1).
    #[must_use]
    pub fn target(&self, count: usize) -> Option<PlayerId> {
        u8::try_from(self.player)
            .ok()
            .map(PlayerId::from)
            .filter(|player_id| player_id.index() < count)
    }
}

impl World {
    /// `do_command`'s gate of GAME (`0x140e77b00` case 0x67; the packet's length, at least 0x14, is
    /// the decoder's): with the issuing player in the world, a sub-command other than 1, 4 and 0xf
    /// for another player runs only with the game's cheats
    /// ([`crate::world::settings::OptionFlags::CHEATS_ENABLED`]); without the issuing player it
    /// does not run. The world's `+0xb12` (the disable-verification cheat), which would let either
    /// through, is not held.
    pub(crate) fn game_allowed(&self, game_command: &GameCommand) -> bool {
        if self.players.get(game_command.issuer.index()).is_none() {
            return false;
        }
        matches!(
            game_command.sub,
            GameSub::Speed | GameSub::QuickBuild | GameSub::Open0xf
        ) || game_command.player == i16::from(game_command.issuer.get())
            || self.settings.flags.contains(OptionFlags::CHEATS_ENABLED)
    }

    /// GAME (0x67): `do_command`'s gate ([`World::game_allowed`]), then `FUN_140e7c330`'s switch
    /// on the sub-command ([`GameSub`]). Each case of a player looks it up first (`FUN_14152fe40`);
    /// one outside the world would be a null the game dereferences, which a recording that
    /// played on never holds: the case does nothing.
    ///
    /// - 0, diplomacy: with `allianceLocked` 0 ([`World::alliance_locked`]) the player's slot
    ///   0x30 toward the player at `+0xa` with the relation at `+0x10`
    ///   ([`World::set_diplomacy`]).
    /// - 1, the game speed: the options' `+0x65` (`mSpeedLocked`) unset, `+0xc` clamped to the
    ///   speed table's (`FUN_141324bd0`: 1.0 up to 2.0 in a game of several players, 8.0 alone)
    ///   goes to the game timer (`FUN_140d7a450`, its `+0x10`) and the options' `mGameSpeed`
    ///   (`+0x44`), with a chat line. A playback's update time is the recording's (the
    ///   timer's multiplier only paces the updates), so nothing the simulation reads changes.
    /// - 4, the quick-build cheat: with the game's cheats the game's `+0xc14` takes the player
    ///   at `+8` ([`World::instant_work`]); not 0, the work actions would do their work at once,
    ///   which they do not model: the player stops ([`CHEAT`]).
    /// - 5, allied victory: with `allianceLocked` 0 the player's `+0x2d0` takes the byte at
    ///   `+0xa` (`FUN_1416765b0`), then the world's teams ([`World::resolve_teams`]).
    /// - 6, a cheat code (`FUN_1415c92c0`): nothing without the game's cheats; with them the
    ///   world's `+0x818` sets and the cheat runs (`FUN_1415c62c0`), not modelled ([`CHEAT`]).
    /// - 9: the player's `prod_queue_on` (`+0x1358`) takes the byte at `+0xa`; then each of its
    ///   live buildings (master type 80) with an interface reason (`+0xc02`, not held) posts the
    ///   interface's event 0x7d and clears it.
    /// - 0xa, Treason: with a castle (`FUN_14160ffb0`) the pending debit of gold lowers by 400
    ///   (`FUN_141630ef0`), then the treason runs ([`World::treason`]).
    /// - 0xb, an AI setting: for a computer player (`FUN_141659cf0`: type 3) the setting `+0xa`
    ///   takes `+0x10` ([`World::set_ai_setting`]).
    /// - 0xd, 0xe, 0x15, 0x16: the mill's queue buys or gives back `+0xa` farms (0x32) or
    ///   pastures (0x761) ([`World::queue_farms`], [`World::unqueue_farms`]).
    /// - 0x10, 0x13: `autoFarmQueue` (`+0x1424`, `FUN_14163a5f0`) or `autoFishTrapQueue`
    ///   (`+0x1425`, `FUN_14163a610`) flips.
    /// - 0x11, 0x12: the dock's queue buys or gives back `+0xa` fish traps
    ///   ([`World::queue_fish_traps`], [`World::unqueue_fish_traps`]).
    /// - 0x14: the offensive posture (`+0x71c`, slot 0xa0 `FUN_14163a630`, Gaia's too) flips
    ///   between 0 and 1.
    pub(crate) fn game(&mut self, game_command: &GameCommand) -> GenieResult<()> {
        if !self.game_allowed(game_command) {
            return Ok(());
        }
        let count = self.players.len();
        let target = game_command.target(count);
        match game_command.sub {
            GameSub::Diplomacy if self.alliance_locked == 0 => {
                // a relation without a stance or a player number outside a byte changes nothing
                let other = u8::try_from(game_command.argument).ok().map(PlayerId::from);
                if let (Some(from_player), Some(to_player), Some(stance)) =
                    (target, other, Stance::of_diplomacy(game_command.value))
                {
                    self.set_diplomacy(from_player, to_player, stance)?;
                }
            }
            GameSub::QuickBuild if self.settings.flags.contains(OptionFlags::CHEATS_ENABLED) => {
                self.instant_work = i32::from(game_command.player);
                if self.instant_work != 0 {
                    return Err(CHEAT);
                }
            }
            GameSub::AlliedVictory if self.alliance_locked == 0 => {
                if let Some(player_id) = target {
                    self.players.player_mut(player_id)?.allied_victory =
                        game_command.argument as u8;
                    self.resolve_teams();
                }
            }
            GameSub::Cheat if self.settings.flags.contains(OptionFlags::CHEATS_ENABLED) => {
                return Err(CHEAT);
            }
            GameSub::ProductionQueue => {
                if let Some(player_id) = target {
                    self.players.player_mut(player_id)?.production_queue_on =
                        game_command.argument as u8;
                }
            }
            GameSub::Treason => {
                if let Some(player_id) = target {
                    self.game_treason(player_id)?;
                }
            }
            GameSub::AiSetting => {
                if let Some(player_id) = target {
                    self.game_ai_setting(player_id, game_command)?;
                }
            }
            GameSub::FarmQueue
            | GameSub::PastureQueue
            | GameSub::FarmUnqueue
            | GameSub::PastureUnqueue
            | GameSub::AutoFarmQueue
            | GameSub::FishTrapQueue
            | GameSub::FishTrapUnqueue
            | GameSub::AutoFishTrapQueue => {
                if let Some(player_id) = target {
                    self.game_queue(player_id, game_command)?;
                }
            }
            GameSub::Posture => {
                if let Some(player_id) = target {
                    let owner = self.players.player_mut(player_id)?;
                    owner.posture = owner.posture.flipped();
                }
            }
            // 1 (the game speed) changes nothing a playback's simulation reads; 0 and 5 with
            // `allianceLocked`, 4 and 6 without cheats, and the sub-commands without a case do
            // nothing
            _ => {}
        }
        Ok(())
    }

    /// Treason (`FUN_141620400`) of player `player_id`, when it has a treason building: the pending
    /// debit's entry of gold (`FUN_141630ef0` with 3) takes its cost.
    fn game_treason(&mut self, player_id: PlayerId) -> GenieResult<()> {
        if self.has_treason_building(player_id) {
            *self
                .players
                .player_mut(player_id)?
                .pending_debit
                .checked_mut(self.model.slots.attribute.gold.index())? -=
                Amount::new(self.model.tuning.turn.treason_cost);
            self.treason(player_id)?;
        }
        Ok(())
    }

    /// A computer player's AI setting.
    fn game_ai_setting(
        &mut self,
        player_id: PlayerId,
        game_command: &GameCommand,
    ) -> GenieResult<()> {
        if self.players.player(player_id)?.kind.is_computer() {
            self.set_ai_setting(player_id, game_command.argument, game_command.value)?;
        }
        Ok(())
    }

    /// `FUN_140e7c330`'s queue cases on player `player_id`: the mill's queue buys or gives back
    /// `+0xa` farms (0xd, 0xe) or pastures (0x15, 0x16) ([`World::queue_farms`],
    /// [`World::unqueue_farms`]), the dock's fish traps (0x11, 0x12, [`World::queue_fish_traps`],
    /// [`World::unqueue_fish_traps`]); `autoFarmQueue` (0x10, `+0x1424`) or `autoFishTrapQueue`
    /// (0x13, `+0x1425`) flips.
    ///
    /// # Errors
    ///
    /// What the purchase and the refund do not model.
    fn game_queue(&mut self, player_id: PlayerId, game_command: &GameCommand) -> GenieResult<()> {
        let count = QueueCount::of_data(i32::from(game_command.argument));
        match game_command.sub {
            GameSub::FarmQueue => {
                let _ = self.queue_farms(player_id, count, self.model.slots.master.farm)?;
            }
            GameSub::PastureQueue => {
                let _ = self.queue_farms(player_id, count, self.model.slots.master.pasture_1889)?;
            }
            GameSub::FarmUnqueue => {
                let _ = self.unqueue_farms(player_id, count, self.model.slots.master.farm)?;
            }
            GameSub::PastureUnqueue => {
                let _ =
                    self.unqueue_farms(player_id, count, self.model.slots.master.pasture_1889)?;
            }
            GameSub::AutoFarmQueue => {
                let owner = self.players.player_mut(player_id)?;
                owner.auto_queues.set(
                    AutoQueues::FARMS,
                    !owner.auto_queues.contains(AutoQueues::FARMS),
                );
            }
            GameSub::FishTrapQueue => {
                let _ = self.queue_fish_traps(player_id, count)?;
            }
            GameSub::FishTrapUnqueue => {
                let _ = self.unqueue_fish_traps(player_id, count)?;
            }
            GameSub::AutoFishTrapQueue => {
                let owner = self.players.player_mut(player_id)?;
                owner.auto_queues.set(
                    AutoQueues::FISH_TRAPS,
                    !owner.auto_queues.contains(AutoQueues::FISH_TRAPS),
                );
            }
            _ => {}
        }
        Ok(())
    }

    /// `FUN_140e7c330`'s case 0xb on computer player `player_id`: its inner switch on the setting
    /// `setting` (`+0xa`) with `value` (`+0x10`). A byte setting takes the value's low byte, a
    /// flag `value == 1` (6, 7; 8 tests the low byte), `value != 1` (0xb) or `value != 0` (0xe,
    /// 0x10, 0x12, 0x13); 0xd the value as a float (`CVTSI2SS` of the zero-extended 32 bits),
    /// 0xf and 0x11 the value as it is. 0x13 is `mAllowCapturingGaiaUnits` (`+0x670`,
    /// [`World::allow_gaia_capture`]). Another setting changes nothing.
    pub(crate) fn set_ai_setting(
        &mut self,
        player_id: PlayerId,
        setting: i16,
        value: u32,
    ) -> GenieResult<()> {
        let byte = value as u8;
        if setting == 0x13 {
            if self.allow_gaia_capture.len() <= player_id.index() {
                self.allow_gaia_capture.resize(player_id.index() + 1, false);
            }
            *self.allow_gaia_capture.checked_mut(player_id.index())? = value != 0;
            return Ok(());
        }
        let owner = self.players.player_mut(player_id)?;
        let ai_settings = &mut owner.ai_settings;
        match setting {
            1 => owner.dodge_chance = Chance::of_data(byte),
            2 => owner.keep_distance_chance = Chance::of_data(byte),
            3 => ai_settings.easiest_reaction_percentage = Percent::new(i32::from(byte)),
            4 => ai_settings.easier_reaction_percentage = Percent::new(i32::from(byte)),
            5 => ai_settings.task_ungrouped_soldiers = byte,
            6 => ai_settings
                .switches
                .set(AiSwitches::MAY_ATTACK_WALLS, value == 1),
            7 => ai_settings
                .switches
                .set(AiSwitches::LIVESTOCK_ONLY_TO_TC, value == 1),
            8 => ai_settings
                .switches
                .set(AiSwitches::DISABLE_BUILD_HELP, byte == 1),
            9 => ai_settings.local_targeting_mode = TargetingMode::of_data(i32::from(byte)),
            10 => ai_settings.targeting_factor_scalar = Percent::new(i32::from(byte)),
            0xb => ai_settings
                .switches
                .set(AiSwitches::TRADE_ATTACK_EVASION, value != 1),
            0xc => ai_settings.boar_lure_destination = LureDestination::of_data(byte),
            0xd => ai_settings.preferred_trade_distance = Distance::new(value as f32),
            0xe => ai_settings
                .switches
                .set(AiSwitches::DISABLE_TOWER_PRIORITY, value != 0),
            0xf => ai_settings.attack_move_max_distance = WholeDistance::of_data(value as i32),
            0x10 => ai_settings
                .switches
                .set(AiSwitches::VILLAGER_ATTACK_RESET, value != 0),
            0x11 => ai_settings.disable_villager_garrison = VillagerGarrison::of_data(value as i32),
            0x12 => ai_settings
                .switches
                .set(AiSwitches::ALLOW_SERJEANT_BUILDING, value != 0),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gathering::QueueCount;
    use crate::objects::masters::CreatablePart;
    use crate::players::Amount;
    use crate::players::Posture;
    #[cfg(test)]
    use crate::players::{ByPlayer, PlayerKind};
    use crate::test_ids::costs;
    use crate::test_ids::object_id;
    use crate::test_ids::player_id;
    use crate::testing::attribute;
    use crate::testing::class;
    use crate::testing::ids::CASTLE;
    use crate::testing::ids::FARM;
    use crate::testing::ids::FISH_TRAP;
    use crate::testing::ids::PASTURE_1889;
    use crate::testing::tuning;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;
    use std::sync::Arc;

    /// Gaia and three players with 300 attributes each.
    fn world() -> World {
        let player = || Player {
            attributes: vec![Amount::ZERO; 300],
            diplomacy: ByPlayer::new(vec![Stance::None; 9]),
            ..Player::default()
        };
        World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO; 4],
            vec![player(), player(), player(), player()],
        )
    }

    fn game(issuer: u8, sub: GameSub, player: i16, argument: i16, value: u32) -> GameCommand {
        GameCommand {
            issuer: player_id(issuer),
            sub,
            player,
            argument,
            speed: 0.0,
            value,
        }
    }

    /// GAME's diplomacy (0) and allied victory (5) run only with `allianceLocked` 0; Treason
    /// (0xa) lowers the pending gold by 400 only for a player with a castle in its tracker's done
    /// list.
    #[test]
    fn diplomacy_allied_victory_and_treason_for_the_command_player() {
        use crate::objects::masters::{Master, Masters};
        let mut world = world();
        world.alliance_locked = 1;
        world.game(&game(1, GameSub::Diplomacy, 1, 2, 0)).unwrap();
        world
            .game(&game(1, GameSub::AlliedVictory, 1, 1, 0))
            .unwrap();
        assert_eq!(
            (
                world.players[1].diplomacy.get(PlayerId::new(2)).copied(),
                world.players[1].allied_victory
            ),
            (Some(Stance::None), 0)
        );
        world.alliance_locked = 0;
        // relation 0: the ally's stance (4)
        world.game(&game(1, GameSub::Diplomacy, 1, 2, 0)).unwrap();
        world
            .game(&game(1, GameSub::AlliedVictory, 1, 1, 0))
            .unwrap();
        assert_eq!(
            (
                world.players[1].diplomacy.get(PlayerId::new(2)).copied(),
                world.players[1].allied_victory
            ),
            (Some(Stance::Ally), 1)
        );
        // no castle: no treason
        world.game(&game(1, GameSub::Treason, 1, 0, 0)).unwrap();
        assert_eq!(
            world.players[1].pending_debit[attribute::GOLD.index()],
            Amount::ZERO
        );
        let castle = Master {
            id: CASTLE,
            class: class::TOWER,
            ..Master::default()
        };
        world.model.masters = vec![Arc::new(Masters::new([])), Arc::new(Masters::new([castle]))];
        world
            .track_add(PlayerId::new(1), object_id(500), CASTLE)
            .unwrap();
        world
            .track_complete(PlayerId::new(1), object_id(500), CASTLE)
            .unwrap();
        world.game(&game(1, GameSub::Treason, 1, 0, 0)).unwrap();
        assert_eq!(
            world.players[1].pending_debit[attribute::GOLD.index()],
            -Amount::new(tuning().turn.treason_cost)
        );
    }

    /// GAME's queue cases for the command's player: 0xd and 0x15 buy farms and pastures for the
    /// mill's queue, 0xe and 0x16 give them back (refunded), 0x11 and 0x12 the dock's fish traps;
    /// a player outside the world changes nothing.
    #[test]
    fn the_queue_sub_commands_buy_and_give_back_for_their_player() {
        use crate::objects::masters::Master;
        use crate::objects::masters::MasterKind;
        use crate::objects::masters::Masters;
        let costing = |id, wood| Master {
            id,
            kind: MasterKind::COMBAT,
            creatable: Some(CreatablePart {
                cost: costs(&[(attribute::WOOD, wood, true)]),
                ..CreatablePart::default()
            }),
            ..Master::default()
        };
        let mut world = world();
        world.players[1].attributes[..4].copy_from_slice(&[
            Amount::ZERO,
            Amount::new(1000.0),
            Amount::ZERO,
            Amount::ZERO,
        ]);
        let masters = Masters::new([
            costing(FARM, 60),
            costing(PASTURE_1889, 100),
            costing(FISH_TRAP, 100),
        ]);
        world.model.masters = vec![Arc::new(Masters::new([])), Arc::new(masters)];
        let wood = |world: &World| world.players[1].attributes[1];
        world.game(&game(1, GameSub::FarmQueue, 1, 2, 0)).unwrap();
        assert_eq!(
            (world.players[1].farm_queue, wood(&world)),
            (QueueCount::of_data(2), Amount::new(880.0))
        );
        world
            .game(&game(1, GameSub::PastureQueue, 1, 1, 0))
            .unwrap();
        assert_eq!(
            (world.players[1].farm_queue, wood(&world)),
            (QueueCount::of_data(3), Amount::new(780.0))
        );
        world
            .game(&game(1, GameSub::PastureUnqueue, 1, 1, 0))
            .unwrap();
        assert_eq!(
            (world.players[1].farm_queue, wood(&world)),
            (QueueCount::of_data(2), Amount::new(880.0))
        );
        world.game(&game(1, GameSub::FarmUnqueue, 1, 1, 0)).unwrap();
        assert_eq!(
            (world.players[1].farm_queue, wood(&world)),
            (QueueCount::of_data(1), Amount::new(940.0))
        );
        world
            .game(&game(1, GameSub::FishTrapQueue, 1, 3, 0))
            .unwrap();
        assert_eq!(
            (world.players[1].fish_trap_queue, wood(&world)),
            (QueueCount::of_data(3), Amount::new(640.0))
        );
        world
            .game(&game(1, GameSub::FishTrapUnqueue, 1, 2, 0))
            .unwrap();
        assert_eq!(
            (world.players[1].fish_trap_queue, wood(&world)),
            (QueueCount::of_data(1), Amount::new(840.0))
        );
        // player 9 is outside the world: nothing
        world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
        world.game(&game(1, GameSub::FarmQueue, 9, 2, 0)).unwrap();
        assert_eq!(
            (world.players[1].farm_queue, wood(&world)),
            (QueueCount::of_data(1), Amount::new(840.0))
        );
    }

    /// `do_command`'s gate: a sub-command for another player runs only with the game's cheats,
    /// 1, 4 and 0xf for anyone; an issuer outside the world runs nothing.
    #[test]
    fn sub_command_for_another_player_needs_the_game_cheats() {
        let mut scene = world();
        assert!(scene.game_allowed(&game(1, GameSub::Posture, 1, 0, 0)));
        assert!(!scene.game_allowed(&game(1, GameSub::Posture, 2, 0, 0)));
        assert!(scene.game_allowed(&game(1, GameSub::Speed, 2, 0, 0)));
        assert!(scene.game_allowed(&game(1, GameSub::QuickBuild, 2, 0, 0)));
        assert!(scene.game_allowed(&game(1, GameSub::Open0xf, 2, 0, 0)));
        assert!(!scene.game_allowed(&game(9, GameSub::Posture, 9, 0, 0)));
        scene.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
        assert!(scene.game_allowed(&game(1, GameSub::Posture, 2, 0, 0)));
        // refused: nothing changes
        let mut world = world();
        world.game(&game(1, GameSub::Posture, 2, 0, 0)).unwrap();
        assert_eq!(world.players[2].posture, Posture::Aggressive);
    }

    /// 0x10, 0x13 and 0x14 flip their flags; a player outside the world changes nothing.
    #[test]
    fn the_toggles_flip_the_player_flags() {
        let mut world = world();
        world
            .game(&game(1, GameSub::AutoFarmQueue, 1, 0, 0))
            .unwrap();
        world
            .game(&game(1, GameSub::AutoFishTrapQueue, 1, 0, 0))
            .unwrap();
        world.game(&game(1, GameSub::Posture, 1, 0, 0)).unwrap();
        let player = &world.players[1];
        assert!(
            player.auto_queues.contains(AutoQueues::FARMS)
                && player.auto_queues.contains(AutoQueues::FISH_TRAPS)
        );
        assert_eq!(player.posture, Posture::Defensive);
        world.game(&game(1, GameSub::Posture, 1, 0, 0)).unwrap();
        assert_eq!(world.players[1].posture, Posture::Aggressive);
        world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
        let before = world.players.clone();
        world.game(&game(1, GameSub::Posture, 7, 0, 0)).unwrap();
        world.game(&game(1, GameSub::Posture, -1, 0, 0)).unwrap();
        assert_eq!(world.players, before);
    }

    /// 6 and 4 do nothing without the game's cheats; with them the cheat is not modelled. 4 with
    /// player 0 turns the instant work off, which is the game's state the slice holds.
    #[test]
    fn cheat_does_nothing_without_the_game_cheats() {
        let mut world = world();
        let before = world.players.clone();
        world.game(&game(1, GameSub::Cheat, 1, 4, 0)).unwrap();
        world.game(&game(1, GameSub::QuickBuild, 1, 0, 0)).unwrap();
        assert_eq!(world.players, before);
        assert_eq!(world.instant_work, 0);
        world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
        assert_eq!(world.game(&game(1, GameSub::Cheat, 1, 4, 0)), Err(CHEAT));
        assert_eq!(
            world.game(&game(1, GameSub::QuickBuild, 2, 0, 0)),
            Err(CHEAT)
        );
        assert_eq!(world.instant_work, 2);
        assert_eq!(world.game(&game(1, GameSub::QuickBuild, 0, 0, 0)), Ok(()));
        assert_eq!(world.instant_work, 0);
    }

    /// 1 changes nothing the simulation reads; 9 sets `prod_queue_on` to the byte at `+0xa`.
    #[test]
    fn the_speed_changes_nothing_and_9_sets_the_production_queue_flag() {
        let mut world = world();
        let before = world.players.clone();
        let mut speed = game(1, GameSub::Speed, 1, 0, 0);
        speed.speed = 2.0;
        world.game(&speed).unwrap();
        assert_eq!(world.players, before);
        world
            .game(&game(1, GameSub::ProductionQueue, 1, 0x101, 0))
            .unwrap();
        assert_eq!(world.players[1].production_queue_on, 1);
    }

    /// 0xb sets a computer player's setting from `+0x10` as its case reads it; a human
    /// player's stays.
    #[test]
    fn an_ai_setting_goes_to_computer_player_only() {
        let mut world = world();
        world.players[2].kind = PlayerKind::Computer;
        world.settings.flags.set(OptionFlags::CHEATS_ENABLED, true);
        world.game(&game(1, GameSub::AiSetting, 1, 1, 30)).unwrap();
        assert_eq!(world.players[1].dodge_chance, Chance::NONE);
        let set = |world: &mut World, setting: i16, value: u32| {
            world
                .game(&game(1, GameSub::AiSetting, 2, setting, value))
                .unwrap();
        };
        set(&mut world, 1, 0x1_1e);
        set(&mut world, 2, 40);
        set(&mut world, 3, 0x1_05);
        set(&mut world, 6, 0x101);
        set(&mut world, 7, 1);
        set(&mut world, 8, 0x101);
        set(&mut world, 0xb, 1);
        set(&mut world, 0xd, 0xffff_ffff);
        set(&mut world, 0xe, 2);
        set(&mut world, 0xf, 0xffff_fff0);
        set(&mut world, 0x13, 5);
        set(&mut world, 0x14, 5);
        let player = &world.players[2];
        assert_eq!(
            (player.dodge_chance, player.keep_distance_chance),
            (Chance::of_data(0x1e), Chance::of_data(40))
        );
        let ai_settings = player.ai_settings;
        assert_eq!(ai_settings.easiest_reaction_percentage, Percent::new(5));
        assert!(
            !ai_settings.switches.contains(AiSwitches::MAY_ATTACK_WALLS)
                && ai_settings
                    .switches
                    .contains(AiSwitches::LIVESTOCK_ONLY_TO_TC)
                && ai_settings
                    .switches
                    .contains(AiSwitches::DISABLE_BUILD_HELP)
        );
        assert!(
            !ai_settings
                .switches
                .contains(AiSwitches::TRADE_ATTACK_EVASION)
                && ai_settings
                    .switches
                    .contains(AiSwitches::DISABLE_TOWER_PRIORITY)
        );
        assert_eq!(
            ai_settings.preferred_trade_distance,
            Distance::new(4_294_967_296.0)
        );
        assert_eq!(
            ai_settings.attack_move_max_distance,
            WholeDistance::of_data(-16)
        );
        assert!(world.allow_gaia_capture[2]);
    }

    /// The settings the test above leaves: the bytes 4, 5, 9, 10 and 0xc take the value's low
    /// byte, the flags 0x10 and 0x12 `value != 0`, 0x11 the value as it is.
    #[test]
    fn the_other_ai_settings_take_their_values() {
        let mut world = world();
        world.players[2].kind = PlayerKind::Computer;
        let set = |world: &mut World, setting: i16, value: u32| {
            world.set_ai_setting(player_id(2), setting, value).unwrap();
        };
        set(&mut world, 4, 0x1_07);
        set(&mut world, 5, 0x2_03);
        set(&mut world, 9, 0x1_02);
        set(&mut world, 10, 0x1_50);
        set(&mut world, 0xc, 0x1_04);
        set(&mut world, 0x10, 3);
        set(&mut world, 0x11, 0xffff_fffe);
        set(&mut world, 0x12, 1);
        let ai_settings = world.players[2].ai_settings;
        assert_eq!(
            (
                ai_settings.easier_reaction_percentage,
                ai_settings.task_ungrouped_soldiers
            ),
            (Percent::new(7), 3)
        );
        assert_eq!(
            (
                ai_settings.local_targeting_mode,
                ai_settings.targeting_factor_scalar
            ),
            (TargetingMode::of_data(2), Percent::new(0x50))
        );
        assert_eq!(
            ai_settings.boar_lure_destination,
            LureDestination::of_data(4)
        );
        assert_eq!(
            ai_settings.disable_villager_garrison,
            VillagerGarrison::of_data(-2)
        );
        assert!(
            ai_settings
                .switches
                .contains(AiSwitches::VILLAGER_ATTACK_RESET)
                && ai_settings
                    .switches
                    .contains(AiSwitches::ALLOW_SERJEANT_BUILDING)
        );
        set(&mut world, 0x10, 0);
        assert!(
            !world.players[2]
                .ai_settings
                .switches
                .contains(AiSwitches::VILLAGER_ATTACK_RESET)
        );
    }
}
