//! GAME (0x67): `TRIBE_Command::do_command`'s case (`0x140e77b00`), which lets a player's
//! sub-command for another player through only with the game's cheats, and its switch on the
//! sub-command (`FUN_140e7c330`, [`World::game`]).
//!
//! 158041: the switch `FUN_140e7c330`; the world player's lookup `FUN_14152fe40`; the cases'
//! handlers are named at each.

use crate::GenieError;
use crate::commands::GameSub;
use crate::gathering::QueueCount;
use crate::gathering::farm_queue::PASTURE;
use crate::map::Distance;
use crate::map::WholeDistance;
use crate::math::image::turn::TREASON_COST as TURN_TREASON_COST;
use crate::objects::masters::ids::FARM;
use crate::players::Amount;
use crate::players::Chance;
use crate::players::PlayerId;
use crate::players::Stance;
use crate::players::attribute;
use crate::world::World;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;
use crate::world::{LureDestination, Percent, TargetingMode, VillagerGarrison};

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
    pub a: i16,
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
    pub(crate) fn run(&self, w: &mut World) -> Result<(), GenieError> {
        w.game(self)
    }

    /// `+8` as a world player of `count` players (`FUN_14152fe40`: none outside 0 to count − 1).
    #[must_use]
    pub fn target(&self, count: usize) -> Option<PlayerId> {
        u8::try_from(self.player)
            .ok()
            .map(PlayerId::from)
            .filter(|p| p.index() < count)
    }
}

impl World {
    /// `do_command`'s gate of GAME (`0x140e77b00` case 0x67; the packet's length, at least 0x14,
    /// is the decoder's): with the issuing player in the world, a sub-command other than 1, 4
    /// and 0xf for another player runs only with the game's cheats
    /// ([`crate::world::settings::Settings::cheats_enabled`]); without the issuing player it does not
    /// run. The world's `+0xb12` (the disable-verification cheat), which would let either
    /// through, is not held.
    pub(crate) fn game_allowed(&self, g: &GameCommand) -> bool {
        if self.players.get(g.issuer.index()).is_none() {
            return false;
        }
        matches!(
            g.sub,
            GameSub::SPEED | GameSub::QUICK_BUILD | GameSub::OPEN_0XF
        ) || g.player == i16::from(g.issuer.get())
            || self.settings.cheats_enabled
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
    pub(crate) fn game(&mut self, g: &GameCommand) -> Result<(), GenieError> {
        if !self.game_allowed(g) {
            return Ok(());
        }
        let n = self.players.len();
        let target = g.target(n);
        match g.sub {
            GameSub::DIPLOMACY if self.alliance_locked == 0 => {
                // a relation without a stance or a player number outside a byte changes nothing
                let other = u8::try_from(g.a).ok().map(PlayerId::from);
                if let (Some(p), Some(q), Some(stance)) =
                    (target, other, Stance::of_diplomacy(g.value))
                {
                    self.set_diplomacy(p, q, stance)?;
                }
            }
            GameSub::QUICK_BUILD if self.settings.cheats_enabled => {
                self.instant_work = i32::from(g.player);
                if self.instant_work != 0 {
                    return Err(CHEAT);
                }
            }
            GameSub::ALLIED_VICTORY if self.alliance_locked == 0 => {
                if let Some(p) = target {
                    self.players.player_mut(p)?.allied_victory = g.a as u8;
                    self.resolve_teams();
                }
            }
            GameSub::CHEAT if self.settings.cheats_enabled => return Err(CHEAT),
            GameSub::PRODUCTION_QUEUE => {
                if let Some(p) = target {
                    self.players.player_mut(p)?.production_queue_on = g.a as u8;
                }
            }
            GameSub::TREASON => {
                if let Some(p) = target {
                    self.game_treason(p)?;
                }
            }
            GameSub::AI_SETTING => {
                if let Some(p) = target {
                    self.game_ai_setting(p, g)?;
                }
            }
            GameSub::FARM_QUEUE
            | GameSub::PASTURE_QUEUE
            | GameSub::FARM_UNQUEUE
            | GameSub::PASTURE_UNQUEUE
            | GameSub::AUTO_FARM_QUEUE
            | GameSub::FISH_TRAP_QUEUE
            | GameSub::FISH_TRAP_UNQUEUE
            | GameSub::AUTO_FISH_TRAP_QUEUE => {
                if let Some(p) = target {
                    self.game_queue(p, g)?;
                }
            }
            GameSub::POSTURE => {
                if let Some(p) = target {
                    let owner = self.players.player_mut(p)?;
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

    /// Treason (`FUN_141620400`) of player `p`, when it has a treason building: the pending debit's
    /// entry of gold (`FUN_141630ef0` with 3) takes its cost.
    fn game_treason(&mut self, p: PlayerId) -> Result<(), GenieError> {
        if self.has_treason_building(p) {
            *self
                .players
                .player_mut(p)?
                .pending_debit
                .checked_mut(attribute::GOLD.index())? -= TREASON_COST;
            self.treason(p)?;
        }
        Ok(())
    }

    /// A computer player's AI setting.
    fn game_ai_setting(&mut self, p: PlayerId, g: &GameCommand) -> Result<(), GenieError> {
        if self.players.player(p)?.kind.is_computer() {
            self.set_ai_setting(p, g.a, g.value)?;
        }
        Ok(())
    }

    /// `FUN_140e7c330`'s queue cases on player `p`: the mill's queue buys or gives back `+0xa`
    /// farms (0xd, 0xe) or pastures (0x15, 0x16) ([`World::queue_farms`],
    /// [`World::unqueue_farms`]), the dock's fish traps (0x11, 0x12, [`World::queue_fish_traps`],
    /// [`World::unqueue_fish_traps`]); `autoFarmQueue` (0x10, `+0x1424`) or `autoFishTrapQueue`
    /// (0x13, `+0x1425`) flips.
    ///
    /// # Errors
    ///
    /// What the purchase and the refund do not model.
    fn game_queue(&mut self, p: PlayerId, g: &GameCommand) -> Result<(), GenieError> {
        let count = QueueCount::of_data(i32::from(g.a));
        match g.sub {
            GameSub::FARM_QUEUE => {
                let _ = self.queue_farms(p, count, FARM)?;
            }
            GameSub::PASTURE_QUEUE => {
                let _ = self.queue_farms(p, count, PASTURE)?;
            }
            GameSub::FARM_UNQUEUE => {
                let _ = self.unqueue_farms(p, count, FARM)?;
            }
            GameSub::PASTURE_UNQUEUE => {
                let _ = self.unqueue_farms(p, count, PASTURE)?;
            }
            GameSub::AUTO_FARM_QUEUE => {
                let owner = self.players.player_mut(p)?;
                owner.auto_farm_queue = !owner.auto_farm_queue;
            }
            GameSub::FISH_TRAP_QUEUE => {
                let _ = self.queue_fish_traps(p, count)?;
            }
            GameSub::FISH_TRAP_UNQUEUE => {
                let _ = self.unqueue_fish_traps(p, count)?;
            }
            GameSub::AUTO_FISH_TRAP_QUEUE => {
                let owner = self.players.player_mut(p)?;
                owner.auto_fishtrap_queue = !owner.auto_fishtrap_queue;
            }
            _ => {}
        }
        Ok(())
    }

    /// `FUN_140e7c330`'s case 0xb on computer player `p`: its inner switch on the setting
    /// `setting` (`+0xa`) with `value` (`+0x10`). A byte setting takes the value's low byte, a
    /// flag `value == 1` (6, 7; 8 tests the low byte), `value != 1` (0xb) or `value != 0` (0xe,
    /// 0x10, 0x12, 0x13); 0xd the value as a float (`CVTSI2SS` of the zero-extended 32 bits),
    /// 0xf and 0x11 the value as it is. 0x13 is `mAllowCapturingGaiaUnits` (`+0x670`,
    /// [`World::allow_gaia_capture`]). Another setting changes nothing.
    pub(crate) fn set_ai_setting(
        &mut self,
        p: PlayerId,
        setting: i16,
        value: u32,
    ) -> Result<(), GenieError> {
        let byte = value as u8;
        if setting == 0x13 {
            if self.allow_gaia_capture.len() <= p.index() {
                self.allow_gaia_capture.resize(p.index() + 1, false);
            }
            *self.allow_gaia_capture.checked_mut(p.index())? = value != 0;
            return Ok(());
        }
        let owner = self.players.player_mut(p)?;
        let s = &mut owner.ai_settings;
        match setting {
            1 => owner.dodge_chance = Chance::of_data(byte),
            2 => owner.keep_distance_chance = Chance::of_data(byte),
            3 => s.easiest_reaction_percentage = Percent::new(i32::from(byte)),
            4 => s.easier_reaction_percentage = Percent::new(i32::from(byte)),
            5 => s.task_ungrouped_soldiers = byte,
            6 => s.may_attack_walls = value == 1,
            7 => s.livestock_only_to_tc = value == 1,
            8 => s.disable_build_help = byte == 1,
            9 => s.local_targeting_mode = TargetingMode::of_data(i32::from(byte)),
            10 => s.targeting_factor_scalar = Percent::new(i32::from(byte)),
            0xb => s.trade_attack_evasion = value != 1,
            0xc => s.boar_lure_destination = LureDestination::of_data(byte),
            0xd => s.preferred_trade_distance = Distance::new(value as f32),
            0xe => s.disable_tower_priority = value != 0,
            0xf => s.attack_move_max_distance = WholeDistance::of_data(value as i32),
            0x10 => s.villager_attack_reset = value != 0,
            0x11 => s.disable_villager_garrison = VillagerGarrison::of_data(value as i32),
            0x12 => s.allow_serjeant_building = value != 0,
            _ => {}
        }
        Ok(())
    }
}

/// What Treason's command takes off the pending debit of gold (`DAT_142eb5bd4`, 400.0, read at
/// `0x140e7c73d`).
const TREASON_COST: Amount = Amount::new(TURN_TREASON_COST);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gathering::QueueCount;
    use crate::objects::masters::class;
    use crate::objects::masters::ids::CASTLE;
    use crate::objects::masters::ids::FISH_TRAP;
    use crate::players::Amount;
    use crate::players::Posture;
    use crate::players::attribute;
    #[cfg(test)]
    use crate::players::{ByPlayer, PlayerType};
    use crate::test_ids::costs;
    use crate::test_ids::object_id;
    use crate::test_ids::player_id;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;

    /// Gaia and three players with 300 attributes each.
    fn world() -> World {
        let player = || Player {
            attributes: vec![Amount::ZERO; 300],
            diplomacy: ByPlayer::new(vec![Stance::NONE; 9]),
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

    fn game(issuer: u8, sub: GameSub, player: i16, a: i16, value: u32) -> GameCommand {
        GameCommand {
            issuer: player_id(issuer),
            sub,
            player,
            a,
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
        let mut t = world();
        t.alliance_locked = 1;
        t.game(&game(1, GameSub::DIPLOMACY, 1, 2, 0)).unwrap();
        t.game(&game(1, GameSub::ALLIED_VICTORY, 1, 1, 0)).unwrap();
        assert_eq!(
            (
                t.players[1].diplomacy.get(PlayerId::new(2)).copied(),
                t.players[1].allied_victory
            ),
            (Some(Stance::NONE), 0)
        );
        t.alliance_locked = 0;
        // relation 0: the ally's stance (4)
        t.game(&game(1, GameSub::DIPLOMACY, 1, 2, 0)).unwrap();
        t.game(&game(1, GameSub::ALLIED_VICTORY, 1, 1, 0)).unwrap();
        assert_eq!(
            (
                t.players[1].diplomacy.get(PlayerId::new(2)).copied(),
                t.players[1].allied_victory
            ),
            (Some(Stance::ALLY), 1)
        );
        // no castle: no treason
        t.game(&game(1, GameSub::TREASON, 1, 0, 0)).unwrap();
        assert_eq!(
            t.players[1].pending_debit[attribute::GOLD.index()],
            Amount::ZERO
        );
        let castle = Master {
            id: CASTLE,
            class: class::TOWER,
            ..Master::default()
        };
        t.masters = vec![Masters::new([]), Masters::new([castle])];
        t.track_add(PlayerId::new(1), object_id(500), CASTLE)
            .unwrap();
        t.track_complete(PlayerId::new(1), object_id(500), CASTLE)
            .unwrap();
        t.game(&game(1, GameSub::TREASON, 1, 0, 0)).unwrap();
        assert_eq!(
            t.players[1].pending_debit[attribute::GOLD.index()],
            -TREASON_COST
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
            cost: costs(&[(attribute::WOOD, wood, true)]),
            ..Master::default()
        };
        let mut t = world();
        t.players[1].attributes[..4].copy_from_slice(&[
            Amount::ZERO,
            Amount::new(1000.0),
            Amount::ZERO,
            Amount::ZERO,
        ]);
        let masters = Masters::new([
            costing(FARM, 60),
            costing(PASTURE, 100),
            costing(FISH_TRAP, 100),
        ]);
        t.masters = vec![Masters::new([]), masters];
        let wood = |t: &World| t.players[1].attributes[1];
        t.game(&game(1, GameSub::FARM_QUEUE, 1, 2, 0)).unwrap();
        assert_eq!(
            (t.players[1].farm_queue, wood(&t)),
            (QueueCount::of_data(2), Amount::new(880.0))
        );
        t.game(&game(1, GameSub::PASTURE_QUEUE, 1, 1, 0)).unwrap();
        assert_eq!(
            (t.players[1].farm_queue, wood(&t)),
            (QueueCount::of_data(3), Amount::new(780.0))
        );
        t.game(&game(1, GameSub::PASTURE_UNQUEUE, 1, 1, 0)).unwrap();
        assert_eq!(
            (t.players[1].farm_queue, wood(&t)),
            (QueueCount::of_data(2), Amount::new(880.0))
        );
        t.game(&game(1, GameSub::FARM_UNQUEUE, 1, 1, 0)).unwrap();
        assert_eq!(
            (t.players[1].farm_queue, wood(&t)),
            (QueueCount::of_data(1), Amount::new(940.0))
        );
        t.game(&game(1, GameSub::FISH_TRAP_QUEUE, 1, 3, 0)).unwrap();
        assert_eq!(
            (t.players[1].fish_trap_queue, wood(&t)),
            (QueueCount::of_data(3), Amount::new(640.0))
        );
        t.game(&game(1, GameSub::FISH_TRAP_UNQUEUE, 1, 2, 0))
            .unwrap();
        assert_eq!(
            (t.players[1].fish_trap_queue, wood(&t)),
            (QueueCount::of_data(1), Amount::new(840.0))
        );
        // player 9 is outside the world: nothing
        t.settings.cheats_enabled = true;
        t.game(&game(1, GameSub::FARM_QUEUE, 9, 2, 0)).unwrap();
        assert_eq!(
            (t.players[1].farm_queue, wood(&t)),
            (QueueCount::of_data(1), Amount::new(840.0))
        );
    }

    /// `do_command`'s gate: a sub-command for another player runs only with the game's cheats,
    /// 1, 4 and 0xf for anyone; an issuer outside the world runs nothing.
    #[test]
    fn sub_command_for_another_player_needs_the_game_cheats() {
        let mut t = world();
        assert!(t.game_allowed(&game(1, GameSub::POSTURE, 1, 0, 0)));
        assert!(!t.game_allowed(&game(1, GameSub::POSTURE, 2, 0, 0)));
        assert!(t.game_allowed(&game(1, GameSub::SPEED, 2, 0, 0)));
        assert!(t.game_allowed(&game(1, GameSub::QUICK_BUILD, 2, 0, 0)));
        assert!(t.game_allowed(&game(1, GameSub::OPEN_0XF, 2, 0, 0)));
        assert!(!t.game_allowed(&game(9, GameSub::POSTURE, 9, 0, 0)));
        t.settings.cheats_enabled = true;
        assert!(t.game_allowed(&game(1, GameSub::POSTURE, 2, 0, 0)));
        // refused: nothing changes
        let mut t = world();
        t.game(&game(1, GameSub::POSTURE, 2, 0, 0)).unwrap();
        assert_eq!(t.players[2].posture, Posture::AGGRESSIVE);
    }

    /// 0x10, 0x13 and 0x14 flip their flags; a player outside the world changes nothing.
    #[test]
    fn the_toggles_flip_the_player_flags() {
        let mut t = world();
        t.game(&game(1, GameSub::AUTO_FARM_QUEUE, 1, 0, 0)).unwrap();
        t.game(&game(1, GameSub::AUTO_FISH_TRAP_QUEUE, 1, 0, 0))
            .unwrap();
        t.game(&game(1, GameSub::POSTURE, 1, 0, 0)).unwrap();
        let p = &t.players[1];
        assert!(p.auto_farm_queue && p.auto_fishtrap_queue);
        assert_eq!(p.posture, Posture::DEFENSIVE);
        t.game(&game(1, GameSub::POSTURE, 1, 0, 0)).unwrap();
        assert_eq!(t.players[1].posture, Posture::AGGRESSIVE);
        t.settings.cheats_enabled = true;
        let before = t.players.clone();
        t.game(&game(1, GameSub::POSTURE, 7, 0, 0)).unwrap();
        t.game(&game(1, GameSub::POSTURE, -1, 0, 0)).unwrap();
        assert_eq!(t.players, before);
    }

    /// 6 and 4 do nothing without the game's cheats; with them the cheat is not modelled. 4 with
    /// player 0 turns the instant work off, which is the game's state the slice holds.
    #[test]
    fn cheat_does_nothing_without_the_game_cheats() {
        let mut t = world();
        let before = t.players.clone();
        t.game(&game(1, GameSub::CHEAT, 1, 4, 0)).unwrap();
        t.game(&game(1, GameSub::QUICK_BUILD, 1, 0, 0)).unwrap();
        assert_eq!(t.players, before);
        assert_eq!(t.instant_work, 0);
        t.settings.cheats_enabled = true;
        assert_eq!(t.game(&game(1, GameSub::CHEAT, 1, 4, 0)), Err(CHEAT));
        assert_eq!(t.game(&game(1, GameSub::QUICK_BUILD, 2, 0, 0)), Err(CHEAT));
        assert_eq!(t.instant_work, 2);
        assert_eq!(t.game(&game(1, GameSub::QUICK_BUILD, 0, 0, 0)), Ok(()));
        assert_eq!(t.instant_work, 0);
    }

    /// 1 changes nothing the simulation reads; 9 sets `prod_queue_on` to the byte at `+0xa`.
    #[test]
    fn the_speed_changes_nothing_and_9_sets_the_production_queue_flag() {
        let mut t = world();
        let before = t.players.clone();
        let mut speed = game(1, GameSub::SPEED, 1, 0, 0);
        speed.speed = 2.0;
        t.game(&speed).unwrap();
        assert_eq!(t.players, before);
        t.game(&game(1, GameSub::PRODUCTION_QUEUE, 1, 0x101, 0))
            .unwrap();
        assert_eq!(t.players[1].production_queue_on, 1);
    }

    /// 0xb sets a computer player's setting from `+0x10` as its case reads it; a human
    /// player's stays.
    #[test]
    fn an_ai_setting_goes_to_computer_player_only() {
        let mut t = world();
        t.players[2].kind = PlayerType::COMPUTER;
        t.settings.cheats_enabled = true;
        t.game(&game(1, GameSub::AI_SETTING, 1, 1, 30)).unwrap();
        assert_eq!(t.players[1].dodge_chance, Chance::NONE);
        let set = |t: &mut World, setting: i16, value: u32| {
            t.game(&game(1, GameSub::AI_SETTING, 2, setting, value))
                .unwrap();
        };
        set(&mut t, 1, 0x1_1e);
        set(&mut t, 2, 40);
        set(&mut t, 3, 0x1_05);
        set(&mut t, 6, 0x101);
        set(&mut t, 7, 1);
        set(&mut t, 8, 0x101);
        set(&mut t, 0xb, 1);
        set(&mut t, 0xd, 0xffff_ffff);
        set(&mut t, 0xe, 2);
        set(&mut t, 0xf, 0xffff_fff0);
        set(&mut t, 0x13, 5);
        set(&mut t, 0x14, 5);
        let p = &t.players[2];
        assert_eq!(
            (p.dodge_chance, p.keep_distance_chance),
            (Chance::of_data(0x1e), Chance::of_data(40))
        );
        let s = p.ai_settings;
        assert_eq!(s.easiest_reaction_percentage, Percent::new(5));
        assert!(!s.may_attack_walls && s.livestock_only_to_tc && s.disable_build_help);
        assert!(!s.trade_attack_evasion && s.disable_tower_priority);
        assert_eq!(s.preferred_trade_distance, Distance::new(4_294_967_296.0));
        assert_eq!(s.attack_move_max_distance, WholeDistance::of_data(-16));
        assert!(t.allow_gaia_capture[2]);
    }
}
