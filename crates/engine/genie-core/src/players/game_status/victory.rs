//! The game's end and the players' status: RESIGN, the status setter with its defeat event
//! and the world's victory check.

use crate::commands::Departure;
use crate::math::conversions::cvttss2si;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::Relation;
use crate::players::game_status::VictoryState;
use crate::players::game_status::{Count, DEFEAT_IDS, GameStatus, KILLS_OF, STOP_AFTER_MS};
use crate::players::history::EventKind;
use crate::world::Departures;
use crate::world::World;
use crate::world::slots::PlayerSlots;
use crate::world::{GameMode, PlayState};
use crate::{GenieError, GenieResult};

impl World {
    /// The relations of players `first` and `second` each way (`player+0x260`).
    pub(super) fn relations(&self, first: PlayerId, second: PlayerId) -> (Relation, Relation) {
        let r = |x: PlayerId, y: PlayerId| {
            self.players
                .player(x)
                .ok()
                .and_then(|player| player.relations.get(y))
                .copied()
                .unwrap_or(Relation::Ally)
        };
        (r(first, second), r(second, first))
    }

    /// `FUN_14152f810`: the game is over: the world's game state is 1 or a player won; else,
    /// under the conquest victory, the players still playing form one side with someone lost,
    /// or nobody plays. A player's `+0x8a`, which would count it lost, is not held (0).
    pub(crate) fn game_ended(&self) -> bool {
        if self.game_state == PlayState::Over {
            return true;
        }
        let mut count = Count::default();
        for (player_id, player) in PlayerId::each(self.players.len())
            .zip(&self.players)
            .skip(1)
        {
            let rel = self.relations(count.first, player_id);
            count.add(player_id, player.game_status, player.allied_victory, rel);
        }
        if 0 < count.won {
            return true;
        }
        self.victory_conquest && ((0 < count.lost && 0 < count.side) || count.playing == 0)
    }

    /// RESIGN (`FUN_140d25af0`): the comm player resigns (its flags are the comm handler's);
    /// once every comm player of its world player has (one each in the slice's games), the
    /// world player's slot 3 (`FUN_1416610f0`): `+0x8c` takes the drop, `+0x8b` is set and the
    /// status becomes lost ([`World::set_status`]); its message is the interface's.
    pub(crate) fn resign(&mut self, player: PlayerId, departure: Departure) -> GenieResult<()> {
        let p = self
            .players
            .get_mut(player.index())
            .ok_or(GenieError::unsupported(
                "RESIGN of a player the slice does not have",
            ))?;
        p.departures
            .set(Departures::DROPPED, departure == Departure::Dropped);
        p.departures.set(Departures::RESIGNED, true);
        self.set_status(player, GameStatus::Lost)
    }

    /// The status setter (slot 1, `FUN_141639860`) of player `player_id`: `FUN_1416768e0` takes the
    /// new status unless the player resigned (a defeat it always takes). A defeat without a
    /// resignation (the world player's defeat check, [`World::defeat_check`]) then posts the
    /// interface's event 6 (`FUN_140c898a0`), the local player's comm message (`FUN_140d3c710`)
    /// and, with a comm handler in session (`DAT_143e8d7d0`'s `+0x388`), marks the player's comm
    /// players defeated or dropped in the game object (`+0xb1c`, `+0xb13`) and the local one's map
    /// flags (`FUN_140c915d0`, `FUN_140c915b0`): the lobby's and the local view's, which a playback
    /// has none of. A status that turned won or lost while the game goes on reveals the map to the
    /// player (`FUN_1412847b0`, with no lobby settings object or its setting `FUN_141300cd0`, as
    /// the playback holds it, [`Spatial::reveal_to`](crate::map::spatial::Spatial::reveal_to)); the
    /// battle royale's is not modelled. A lost player's objects stop ([`World::stop_everything`])
    /// once the game is a minute old; its fealty (`+0x12f8`) would give its resources and units
    /// away while the game goes on, which is not modelled. A newly lost player's history takes its
    /// defeat ([`World::defeat_event`]).
    pub(crate) fn set_status(
        &mut self,
        player_id: PlayerId,
        status: GameStatus,
    ) -> GenieResult<()> {
        let old = self.players.player(player_id)?.game_status;
        if status != old
            && (!self
                .players
                .player(player_id)?
                .departures
                .contains(Departures::RESIGNED)
                || status == GameStatus::Lost)
        {
            self.players.player_mut(player_id)?.game_status = status;
        }
        let now = self.players.player(player_id)?.game_status;
        if now != old && now.ended() && !self.game_ended() {
            // the map revealed to the player (FUN_1412847b0 with 0x4000000040000000): every
            // tile of the unified map and the player's masks take the ended bits; the view's
            // refresh (FUN_140c80bb0) is not held
            self.spatial.reveal_to(player_id);
        }
        if self.settings.game_mode == GameMode::BattleRoyale {
            return Err(GenieError::unsupported(
                "a status change in a battle royale (game mode 12)",
            ));
        }
        if now == GameStatus::Lost {
            if STOP_AFTER_MS < self.world_time {
                self.stop_everything(player_id)?;
            }
            if old != GameStatus::Lost {
                self.defeat_event(player_id)?;
            }
            if old != GameStatus::Lost
                && self.players.player(player_id)?.fealty.names_player()
                && self.game_state == PlayState::Playing
            {
                return Err(GenieError::unsupported(
                    "a defeated player's fealty (its resources and units given away)",
                ));
            }
        }
        Ok(())
    }

    /// The defeat's history event (`FUN_141639860` from `0x141639cd1`) of player `player_id`,
    /// unless it resigned (`+0x8b`) or is Gaia: the other players but Gaia (ids up to 24) are
    /// weighed by their attribute 300 + their own id (the game reads each one's own, not the
    /// defeated player's); the first strictly above the best so far (`COMISS` with `JBE`, from 0)
    /// is the event's `data1`, its value through `CVTTSS2SI` its `data2`; none gives player 1
    /// and 0.
    pub(crate) fn defeat_event(&mut self, player_id: PlayerId) -> GenieResult<()> {
        if self
            .players
            .player(player_id)?
            .departures
            .contains(Departures::RESIGNED)
            || player_id == PlayerId::GAIA
        {
            return Ok(());
        }
        let mut killer = 1u8;
        let mut most = 0i32;
        for (index, other) in self.players.iter().enumerate() {
            if index == 0 || index == player_id.index() || DEFEAT_IDS <= index {
                continue;
            }
            let amount = other
                .attributes
                .get(KILLS_OF + index)
                .copied()
                .unwrap_or(Amount::ZERO);
            if amount > Amount::new(most as f32) {
                most = cvttss2si(amount.get());
                killer = index as u8;
            }
        }
        let now = self.world_time;
        self.players.player_mut(player_id)?.history.add_event(
            EventKind::Defeat,
            f32::from(killer),
            most as f32,
            0,
            now,
        );
        Ok(())
    }

    /// `FUN_14165d720`: player `player_id`'s status for the victory check. A player still playing
    /// whose victory conditions are met (`FUN_1414f35d0`) wins (slot 1 with won; the world's
    /// `+0x8c`, 2, is not held); one counting its allies' victory as its own is not modelled.
    pub(super) fn own_victory(&mut self, player_id: PlayerId) -> GenieResult<GameStatus> {
        let player = &self.players.player(player_id)?;
        if player.game_status == GameStatus::Lost || player.victory != VictoryState::MET {
            return Ok(player.game_status);
        }
        if player.allied_victory != 0 {
            return Err(GenieError::unsupported(
                "a player's victory with its allies' (allied victory)",
            ));
        }
        self.set_status(player_id, GameStatus::Won)?;
        Ok(self.players.player(player_id)?.game_status)
    }

    /// The world's victory check after every update (`TRIBE_World` slot 0x68, `FUN_1415c9480`):
    /// its relic and wonder countdowns (`world+0x840`..`+0x8d9`) are not modelled; its tail
    /// (`FUN_14152e3d0` with 1, 0) counts the players by their own victory
    /// ([`World::own_victory`]). Someone won: the game is over (game state 1) and those still
    /// playing lose (`FUN_141666590`). Else under the conquest victory, one side left with
    /// someone lost, or nobody playing: the game is over and those still playing win
    /// (`FUN_1416778c0`; the world's `+0x8c`, 1, is not held).
    pub(crate) fn victory_check(&mut self) -> GenieResult<()> {
        let mut count = Count::default();
        for i in PlayerId::each(self.players.len()).skip(1) {
            let status = match self.own_victory(i) {
                Ok(game_status) => game_status,
                Err(e) => {
                    self.stop_player(i, e);
                    return Ok(());
                }
            };
            let rel = self.relations(count.first, i);
            let allied_victory = self.players.player(i)?.allied_victory;
            count.add(i, status, allied_victory, rel);
        }
        let outcome = if 0 < count.won {
            GameStatus::Lost
        } else if self.victory_conquest
            && ((0 < count.lost && 0 < count.side) || count.playing == 0)
        {
            GameStatus::Won
        } else {
            return Ok(());
        };
        self.game_state = PlayState::Over;
        for player_id in PlayerId::each(self.players.len()).skip(1) {
            if self.players.player(player_id)?.game_status == GameStatus::Playing
                && let Err(error) = self.set_status(player_id, outcome)
            {
                self.stop_player(player_id, error);
            }
        }
        Ok(())
    }
}
