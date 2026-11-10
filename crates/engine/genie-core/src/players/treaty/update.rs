//! The treaty monitor's starts, its end and its update (`FUN_141461620`, `FUN_141461710`,
//! `FUN_141461310`, `FUN_1414618a0`), the world update's step that runs it, and the load's
//! branch that makes it.

use crate::GenieResult;
use crate::players::PlayerId;
use crate::players::Relation;
use crate::players::treaty::Borders;
use crate::players::treaty::GameScreen;
use crate::players::treaty::NomadTreaty;
use crate::players::treaty::TreatyMinutes;
use crate::players::treaty::TreatyMonitor;
use crate::players::treaty::TreatySeconds;
use crate::players::treaty::TreatyState;
use crate::world::GameMode;
use crate::world::Seconds;
use crate::world::TreatyLength;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::slots::PlayerSlotsExt;

/// A forced nomad treaty of fewer minutes than this the load makes this long (`World::load_world`'s
/// `CMP BL, 5` at `0x1415d4b6c`, an unsigned compare of the save's byte); the constructor's own
/// bound is [`crate::players::treaty::NOMAD_LEAST`].
pub(crate) const LOADED_NOMAD_LEAST: u8 = 5;
/// The milliseconds of a minute in the load's test of the treaty's end against the world's time
/// (`IMUL R12D, EAX, 0xea60` at `0x1415d4be5`, then an unsigned compare).
const MS_PER_MINUTE: u32 = 60_000;

/// When the countdown's next message comes: the seconds left above which it comes `long`
/// seconds before the end, else above which it comes `short` seconds before, else at the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Countdown {
    /// The long lead.
    long: TreatySeconds,
    /// The short lead.
    short: TreatySeconds,
}

/// The timed start's countdown (`FUN_141461710`: `CMP` with `0x12d` and `0x3c`, `ADD` of −300
/// and −0x3c); the update's own is [`UPDATE_COUNTDOWN`].
const START_COUNTDOWN: Countdown = Countdown {
    long: TreatySeconds(300),
    short: TreatySeconds(60),
};
/// The update's countdown after a message (`FUN_1414618a0`: `CMP` with `0x12d` and `0x3c`,
/// `ADD` of −300 and −0x3c); the start's own is [`START_COUNTDOWN`].
const UPDATE_COUNTDOWN: Countdown = Countdown {
    long: TreatySeconds(300),
    short: TreatySeconds(60),
};

/// The second of the countdown's next message for a treaty ending at `end` with `left` seconds
/// left: `countdown`'s long lead before the end while more than it is left, else its short lead
/// while more than that is left, else the end.
const fn next_message(
    end: TreatySeconds,
    left: TreatySeconds,
    countdown: Countdown,
) -> TreatySeconds {
    if left.0 > countdown.long.0 {
        end.less(countdown.long)
    } else if left.0 > countdown.short.0 {
        end.less(countdown.short)
    } else {
        end
    }
}

impl World {
    /// The walk of the three routines below: every player past Gaia, toward every player past
    /// Gaia, whose relation (`FUN_141667f70`, `player+0x260`) is `from` takes `to`
    /// ([`World::set_relation`], `FUN_141676b80`), in the players' order.
    ///
    /// # Errors
    ///
    /// The relation change's.
    fn turn_relations(&mut self, from: Relation, to: Relation) -> GenieResult<()> {
        let count = self.players.len();
        for player in PlayerId::each(count).skip(1) {
            for other in PlayerId::each(count).skip(1) {
                let relation = *self.players.player(player)?.relations.checked(other)?;
                if relation == from {
                    self.set_relation(player, other, to)?;
                }
            }
        }
        Ok(())
    }

    /// `FUN_141461620` (HD's `TreatyMode::StartTreaty`), game mode 16's start: a new monitor
    /// turns every relation 3 into 4 ([`World::turn_relations`]) and has no time left (−1); then
    /// it holds, whatever its state was.
    ///
    /// # Errors
    ///
    /// A world without its monitor, and the relation changes'.
    pub(crate) fn start_treaty(&mut self) -> GenieResult<()> {
        if self.treaty_monitor()?.state == TreatyState::New {
            self.turn_relations(Relation::Enemy, Relation::Relation4)?;
            self.treaty_monitor_mut()?.seconds_left = TreatySeconds::UNTIMED;
        }
        self.treaty_monitor_mut()?.state = TreatyState::Holds;
        Ok(())
    }

    /// `FUN_141461710`, the timed start at the world's time `time`: a new monitor turns every
    /// relation 3 into 4 ([`World::turn_relations`]), takes its seconds left (its end less the
    /// time's seconds) and its countdown's next message ([`START_COUNTDOWN`]); an ended one
    /// turns them again. Then it holds, whatever its state was.
    ///
    /// # Errors
    ///
    /// A world without its monitor, and the relation changes'.
    pub(crate) fn start_treaty_at(&mut self, time: WorldTime) -> GenieResult<()> {
        match self.treaty_monitor()?.state {
            TreatyState::New => {
                self.turn_relations(Relation::Enemy, Relation::Relation4)?;
                let monitor = self.treaty_monitor_mut()?;
                let left = monitor.end.less(TreatySeconds::at(time));
                monitor.next_message = next_message(monitor.end, left, START_COUNTDOWN);
                monitor.seconds_left = left;
            }
            TreatyState::Ended => self.turn_relations(Relation::Enemy, Relation::Relation4)?,
            TreatyState::Holds | TreatyState::Done => {}
        }
        self.treaty_monitor_mut()?.state = TreatyState::Holds;
        Ok(())
    }

    /// `FUN_141461310` (HD's `TreatyMode::EndTreaty`) with its flag 1, which every caller passes:
    /// every relation 4 turns back into 3 ([`World::turn_relations`]), the zones count as
    /// undrawn and the monitor has ended. The flag's other work, the interface's list of the
    /// drawn zones (`FUN_1414d2010`), is not simulation.
    ///
    /// # Errors
    ///
    /// A world without its monitor, and the relation changes'.
    pub(crate) fn end_treaty(&mut self) -> GenieResult<()> {
        self.turn_relations(Relation::Relation4, Relation::Enemy)?;
        let monitor = self.treaty_monitor_mut()?;
        monitor.borders = Borders::Undrawn;
        monitor.state = TreatyState::Ended;
        Ok(())
    }

    /// The update's end of the treaty ([`World::end_treaty`]), after which its state is 3: the
    /// world's update lets the monitor go. The message and the players' colours
    /// (`FUN_140c931e0`, `Game::update_friend_or_foe_colors`) are the interface's.
    ///
    /// # Errors
    ///
    /// The end's.
    fn finish_treaty(&mut self) -> GenieResult<()> {
        self.end_treaty()?;
        self.treaty_monitor_mut()?.state = TreatyState::Done;
        Ok(())
    }

    /// `FUN_1414618a0` (HD's `TreatyMode::Update`) at the world's time `time`. Outside game
    /// modes 7 and 16 the seconds left are the end less the time's seconds. A monitor that
    /// holds with its zones undrawn draws them ([`World::draw_treaty_zones`], with the game's
    /// screen). Then, each in turn: game mode 16's Castle Age ends the treaty
    /// ([`World::finish_treaty`]), and so do the drawn zones of a lengthened nomad treaty; and
    /// outside modes 7 and 16 a time past the end ends it with no seconds left, else a time past
    /// the next message takes the countdown's next ([`UPDATE_COUNTDOWN`]). The messages and the
    /// countdown's sound are the interface's.
    ///
    /// # Errors
    ///
    /// A world without its monitor, and the zones' and the end's.
    pub(crate) fn update_treaty(&mut self, time: WorldTime) -> GenieResult<()> {
        let timed = !matches!(
            self.settings.game_mode,
            GameMode::DefendTheWonder | GameMode::OneUnit
        );
        let now = TreatySeconds::at(time);
        let monitor = self.treaty_monitor_mut()?;
        if timed {
            monitor.seconds_left = monitor.end.less(now);
        }
        if monitor.borders == Borders::Undrawn && monitor.state == TreatyState::Holds {
            let borders = self.draw_treaty_zones(GameScreen::Shown)?;
            self.treaty_monitor_mut()?.borders = borders;
        }
        if self.treaty_monitor()?.castle_age {
            self.finish_treaty()?;
        }
        let monitor = self.treaty_monitor()?;
        if monitor.borders == Borders::Drawn && monitor.lengthened {
            self.finish_treaty()?;
        }
        if timed {
            self.count_down_treaty(now)?;
        }
        Ok(())
    }

    /// [`World::update_treaty`]'s countdown at the time's seconds `now`: past the end the treaty
    /// ends with no seconds left (`CMP` and `JLE`: at its second it still holds), else past the
    /// next message the countdown takes its next.
    ///
    /// # Errors
    ///
    /// A world without its monitor, and the end's.
    fn count_down_treaty(&mut self, now: TreatySeconds) -> GenieResult<()> {
        let monitor = self.treaty_monitor_mut()?;
        if monitor.end < now {
            self.finish_treaty()?;
            self.treaty_monitor_mut()?.seconds_left = TreatySeconds::ZERO;
        } else if monitor.next_message < now {
            monitor.next_message =
                next_message(monitor.end, monitor.end.less(now), UPDATE_COUNTDOWN);
        }
        Ok(())
    }

    /// `World::update`'s treaty step (`0x1415e8e90`..`0x1415e8efa`), after `BaseWorld::update`:
    /// with time in the update (`COMISS` and `JBE`: a dt of 0 or a NaN skips it), a monitor
    /// updates at the world's time ([`World::update_treaty`]), and one that ended there (state
    /// 3) goes. The interface's list of its zones (`FUN_1414612a0`) goes with it, and the global
    /// forced nomad flag (`DAT_143f69470`), which only the world's save reads, takes 0.
    ///
    /// # Errors
    ///
    /// The update's.
    pub(crate) fn treaty_step(&mut self) -> GenieResult<()> {
        let has_time = Seconds::ZERO < self.dt_seconds;
        if !has_time || self.treaty.is_none() {
            return Ok(());
        }
        self.update_treaty(self.world_time)?;
        if self.treaty_monitor()?.state == TreatyState::Done {
            self.treaty = None;
        }
        Ok(())
    }

    /// `World::load_world`'s treaty (`0x1415d4a35`..`0x1415d4c7b`), from the save's
    /// `treatyLength` byte `length` (read from save version `0xbf5c2` on, as every save genie
    /// reads is) and its forced nomad flag `nomad`. In game mode 16 a length above 0 (a signed
    /// byte) makes a monitor without time (−1 minutes, not forced) and starts it
    /// ([`World::start_treaty`]). In any other the options' length takes the byte
    /// (`FUN_14130f950`, `+0x1064`); a forced nomad treaty shorter than five minutes counts five.
    /// A monitor of other minutes ends ([`World::end_treaty`]) and goes. Then a length whose end
    /// is past the world's time makes a monitor of the byte's minutes if none is left
    /// ([`TreatyMonitor::new`]) and starts it at the time ([`World::start_treaty_at`]); its zones
    /// are drawn again (`FUN_1414612b0`, the result left), before the game's screen exists.
    ///
    /// # Errors
    ///
    /// The starts', the end's and the zones'.
    pub(crate) fn load_treaty(&mut self, length: u8, nomad: NomadTreaty) -> GenieResult<()> {
        let game_mode = self.settings.game_mode;
        if game_mode == GameMode::OneUnit {
            // CMP byte [RSP+0x78], 0 and JLE: a signed byte
            if 0 < length as i8 {
                self.treaty = Some(TreatyMonitor::new(
                    TreatyMinutes::UNTIMED,
                    NomadTreaty::NotForced,
                    game_mode,
                ));
                self.start_treaty()?;
            }
            return Ok(());
        }
        self.settings.treaty_length = TreatyLength::of_minutes(u32::from(length));
        let minutes = if length < LOADED_NOMAD_LEAST && nomad == NomadTreaty::Forced {
            LOADED_NOMAD_LEAST
        } else {
            length
        };
        if let Some(monitor) = &self.treaty
            && monitor.end.minutes() != i32::from(minutes)
        {
            self.end_treaty()?;
            self.treaty = None;
        }
        if minutes == 0 || u32::from(minutes) * MS_PER_MINUTE <= self.world_time.millis() {
            return Ok(());
        }
        if self.treaty.is_none() {
            self.treaty = Some(TreatyMonitor::new(
                TreatyMinutes::of_byte(length),
                nomad,
                game_mode,
            ));
            self.start_treaty_at(self.world_time)?;
        }
        self.redraw_treaty_zones(GameScreen::Absent)
    }

    /// `FUN_1414612b0`: the zones count as undrawn and are drawn again on the game's screen
    /// `screen` ([`World::draw_treaty_zones`]); the load leaves its result, so the next update
    /// draws them once more.
    ///
    /// # Errors
    ///
    /// A world without its monitor, and the zones'.
    fn redraw_treaty_zones(&mut self, screen: GameScreen) -> GenieResult<()> {
        self.treaty_monitor_mut()?.borders = Borders::Undrawn;
        let _ = self.draw_treaty_zones(screen)?;
        Ok(())
    }
}
