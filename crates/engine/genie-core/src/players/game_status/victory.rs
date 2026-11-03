//! The game's end and the players' status: RESIGN, the status setter with its defeat event
//! and the world's victory check.

use crate::GenieError;
use crate::math::conversions::cvttss2si;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::Relation;
use crate::players::game_status::VictoryState;
use crate::players::game_status::{Count, DEFEAT_IDS, GameStatus, KILLS_OF, STOP_AFTER_MS};
use crate::players::history::EventKind;
use crate::world::World;
use crate::world::slots::PlayerSlots;
use crate::world::{GameMode, PlayState};

impl World {
    /// The relations of players `a` and `b` each way (`player+0x260`).
    pub(super) fn relations(&self, a: PlayerId, b: PlayerId) -> (Relation, Relation) {
        let r = |x: PlayerId, y: PlayerId| {
            self.players
                .player(x)
                .ok()
                .and_then(|p| p.relations.get(y))
                .copied()
                .unwrap_or(Relation::Ally)
        };
        (r(a, b), r(b, a))
    }

    /// `FUN_14152f810`: the game is over: the world's game state is 1 or a player won; else,
    /// under the conquest victory, the players still playing form one side with someone lost,
    /// or nobody plays. A player's `+0x8a`, which would count it lost, is not held (0).
    pub(crate) fn game_ended(&self) -> bool {
        if self.game_state == PlayState::Over {
            return true;
        }
        let mut count = Count::default();
        for (i, p) in PlayerId::each(self.players.len())
            .zip(&self.players)
            .skip(1)
        {
            let rel = self.relations(count.first, i);
            count.add(i, p.game_status, p.allied_victory, rel);
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
    pub(crate) fn resign(&mut self, player: PlayerId, dropped: bool) -> Result<(), GenieError> {
        let p = self
            .players
            .get_mut(player.index())
            .ok_or(GenieError::unsupported(
                "RESIGN of a player the slice does not have",
            ))?;
        p.dropped = dropped;
        p.resigned = true;
        self.set_status(player, GameStatus::Lost)
    }

    /// The status setter (slot 1, `FUN_141639860`) of player `p`: `FUN_1416768e0` takes the new
    /// status unless the player resigned (a defeat it always takes). A defeat without a resignation
    /// (the world player's defeat check, [`World::defeat_check`]) then posts the interface's event
    /// 6 (`FUN_140c898a0`), the local player's comm message (`FUN_140d3c710`) and, with a comm
    /// handler in session (`DAT_143e8d7d0`'s `+0x388`), marks the player's comm players defeated or
    /// dropped in the game object (`+0xb1c`, `+0xb13`) and the local one's map flags
    /// (`FUN_140c915d0`, `FUN_140c915b0`): the lobby's and the local view's, which a playback has
    /// none of. A status that turned won or lost while the game goes on reveals the map to the
    /// player (`FUN_1412847b0`, with no lobby settings object or its setting `FUN_141300cd0`, as
    /// the playback holds it, [`Spatial::reveal_to`](crate::map::spatial::Spatial::reveal_to)); the
    /// battle royale's is not modelled. A lost player's objects stop ([`World::stop_everything`])
    /// once the game is a minute old; its fealty (`+0x12f8`) would give its resources and units
    /// away while the game goes on, which is not modelled. A newly lost player's history takes its
    /// defeat ([`World::defeat_event`]).
    pub(crate) fn set_status(&mut self, p: PlayerId, status: GameStatus) -> Result<(), GenieError> {
        let old = self.players.player(p)?.game_status;
        if status != old && (!self.players.player(p)?.resigned || status == GameStatus::Lost) {
            self.players.player_mut(p)?.game_status = status;
        }
        let now = self.players.player(p)?.game_status;
        if now != old && now.ended() && !self.game_ended() {
            // the map revealed to the player (FUN_1412847b0 with 0x4000000040000000): every
            // tile of the unified map and the player's masks take the ended bits; the view's
            // refresh (FUN_140c80bb0) is not held
            self.spatial.reveal_to(p);
        }
        if self.settings.game_mode == GameMode::BattleRoyale {
            return Err(GenieError::unsupported(
                "a status change in a battle royale (game mode 12)",
            ));
        }
        if now == GameStatus::Lost {
            if STOP_AFTER_MS < self.world_time {
                self.stop_everything(p)?;
            }
            if old != GameStatus::Lost {
                self.defeat_event(p)?;
            }
            if old != GameStatus::Lost
                && self.players.player(p)?.fealty.names_player()
                && self.game_state == PlayState::Playing
            {
                return Err(GenieError::unsupported(
                    "a defeated player's fealty (its resources and units given away)",
                ));
            }
        }
        Ok(())
    }

    /// The defeat's history event (`FUN_141639860` from `0x141639cd1`) of player `p`, unless it
    /// resigned (`+0x8b`) or is Gaia: the other players but Gaia (ids up to 24) are weighed by
    /// their attribute 300 + their own id (the game reads each one's own, not the defeated
    /// player's); the first strictly above the best so far (`COMISS` with `JBE`, from 0) is the
    /// event's `data1`, its value through `CVTTSS2SI` its `data2`; none gives player 1 and 0.
    pub(crate) fn defeat_event(&mut self, p: PlayerId) -> Result<(), GenieError> {
        if self.players.player(p)?.resigned || p == PlayerId::GAIA {
            return Ok(());
        }
        let mut killer = 1u8;
        let mut most = 0i32;
        for (i, other) in self.players.iter().enumerate() {
            if i == 0 || i == p.index() || DEFEAT_IDS <= i {
                continue;
            }
            let v = other
                .attributes
                .get(KILLS_OF + i)
                .copied()
                .unwrap_or(Amount::ZERO);
            if v > Amount::new(most as f32) {
                most = cvttss2si(v.get());
                killer = i as u8;
            }
        }
        let now = self.world_time;
        self.players.player_mut(p)?.history.add_event(
            EventKind::Defeat,
            f32::from(killer),
            most as f32,
            0,
            now,
        );
        Ok(())
    }

    /// `FUN_14165d720`: player `i`'s status for the victory check. A player still playing whose
    /// victory conditions are met (`FUN_1414f35d0`) wins (slot 1 with won; the world's `+0x8c`,
    /// 2, is not held); one counting its allies' victory as its own is not modelled.
    pub(super) fn own_victory(&mut self, i: PlayerId) -> Result<GameStatus, GenieError> {
        let p = &self.players.player(i)?;
        if p.game_status == GameStatus::Lost || p.victory != VictoryState::MET {
            return Ok(p.game_status);
        }
        if p.allied_victory != 0 {
            return Err(GenieError::unsupported(
                "a player's victory with its allies' (allied victory)",
            ));
        }
        self.set_status(i, GameStatus::Won)?;
        Ok(self.players.player(i)?.game_status)
    }

    /// The world's victory check after every update (`TRIBE_World` slot 0x68, `FUN_1415c9480`):
    /// its relic and wonder countdowns (`world+0x840`..`+0x8d9`) are not modelled; its tail
    /// (`FUN_14152e3d0` with 1, 0) counts the players by their own victory
    /// ([`World::own_victory`]). Someone won: the game is over (game state 1) and those still
    /// playing lose (`FUN_141666590`). Else under the conquest victory, one side left with
    /// someone lost, or nobody playing: the game is over and those still playing win
    /// (`FUN_1416778c0`; the world's `+0x8c`, 1, is not held).
    pub(crate) fn victory_check(&mut self) -> Result<(), GenieError> {
        let mut count = Count::default();
        for i in PlayerId::each(self.players.len()).skip(1) {
            let status = match self.own_victory(i) {
                Ok(s) => s,
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
        for i in PlayerId::each(self.players.len()).skip(1) {
            if self.players.player(i)?.game_status == GameStatus::Playing
                && let Err(e) = self.set_status(i, outcome)
            {
                self.stop_player(i, e);
            }
        }
        Ok(())
    }
}
