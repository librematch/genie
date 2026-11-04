//! The world update as a declared schedule (FD-045-D13): its steps in the game's order, each
//! with the game routine it ports, and the update that runs them with an observer.

use crate::world::Millis;
use derive_more::Display;
use strum::IntoStaticStr;

use crate::GenieError;
use crate::commands::Command;
use crate::math::rng::Stream;
use crate::players::PlayerId;
use crate::trace::UPDATE;
use crate::world::FrameSeed;
use crate::world::World;
use std::mem::take;

/// A step the world update runs once, before or after the players' steps. It shows as its
/// variant's name, the traces' and the harness's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display, IntoStaticStr)]
pub enum WorldStep {
    /// The command phase: the update's count and the world's time go on, the streams take the
    /// frame seed and the players' commands run (`BaseWorld::update` up to
    /// `RGE_Command__do_commands`, before `FUN_141552990`).
    Commands,
    /// The world stream draws the seed that `Reseed` gives the streams (`rand15` at
    /// `0x141551aca`); before it the frozen objects' unknown places grow by the update (genie's
    /// own, for the forced replay).
    Draw,
    /// Every player's ghosts update, Gaia first (`FUN_141166550` on each player's `+0x40`).
    Ghosts,
    /// Every player's watched list, Gaia first (`FUN_1410905e0` on each player's `+0x78`).
    Watched,
    /// All streams take the drawn seed (`SeedAll`, `0x140efe4f0`).
    Reseed,
    /// The turn passes to the next player, and the time of the player whose turn ends starts
    /// over (`BaseWorld::update`, `world+0x188`).
    Turn,
    /// The victory check (`TRIBE_World` slot 0x68, `FUN_1415c9480`).
    Victory,
    /// The world stream draws the next update's frame seed (`rand15` at `0x141551bd7`).
    FrameSeed,
    /// The multiplayer sync's draws of four streams (`setLastRandoms`, `0x140d3ce20`).
    LastRandoms,
    /// The players' vision masks again (`World::update`'s slot 0x78, `FUN_1415e92b0`).
    Vision,
}

/// A step the world update runs for each player in turn, Gaia first (`BaseWorld::update`'s
/// player loop). It shows as its variant's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display, IntoStaticStr)]
pub enum PlayerStep {
    /// The player's time goes on by the update's milliseconds (`BaseWorld::update`,
    /// `world+0x18c`).
    Clock,
    /// The gates of the players allied with the player open for its units (slot 0x318).
    OpenGates,
    /// In the first update of an Empire Wars game the player's idle villagers go to work
    /// (`BaseWorld::update` with the world's `+0x6c6`).
    StartWork,
    /// The player's unit groups update (`FUN_14103d910`); before them the pathing takes the
    /// tiles whose static marks changed (genie's own cache).
    Groups,
    /// The player's own update up to its objects: a world player's seconds or Gaia's time
    /// (`WorldPlayer::update`, slot 0x100), then its plans start over
    /// (`WorldPlayerBase::update`, `+0x67c`).
    WorldPlayer,
    /// The player's objects update in their list's order (`FUN_141166550` on the player's list).
    Objects,
    /// The tiles the player's map explored give their resources (`Process_New_Tiles`,
    /// `FUN_1412899a0`).
    NewTiles,
    /// A world player's scout map takes the tiles of its tile list (`WorldPlayer::update` after
    /// `WorldPlayerBase::update`).
    ScoutMap,
    /// The gates the player opened close again (slot 0x320).
    CloseGates,
    /// The player's tile list empties (`FUN_141661030` on `+0x180`).
    ClearTiles,
}

/// The steps before the players' (`BaseWorld::update` up to its player loop).
pub const BEFORE_PLAYERS: [WorldStep; 1] = [WorldStep::Commands];

/// Each player's steps, in the player loop of `BaseWorld::update`.
pub const EACH_PLAYER: [PlayerStep; 10] = [
    PlayerStep::Clock,
    PlayerStep::OpenGates,
    PlayerStep::StartWork,
    PlayerStep::Groups,
    PlayerStep::WorldPlayer,
    PlayerStep::Objects,
    PlayerStep::NewTiles,
    PlayerStep::ScoutMap,
    PlayerStep::CloseGates,
    PlayerStep::ClearTiles,
];

/// The steps after the players' (the rest of `BaseWorld::update`, then `World::update`'s).
pub const AFTER_PLAYERS: [WorldStep; 9] = [
    WorldStep::Draw,
    WorldStep::Ghosts,
    WorldStep::Watched,
    WorldStep::Reseed,
    WorldStep::Turn,
    WorldStep::Victory,
    WorldStep::FrameSeed,
    WorldStep::LastRandoms,
    WorldStep::Vision,
];

/// An update after one of its steps: a world step, or a player's step with the player. It shows
/// as its step's name, a player's step with the player (`Objects of player 2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Display)]
pub enum UpdateStep {
    /// A step the update runs once.
    World(WorldStep),
    /// A step the update runs for each player, with the player whose step it was.
    #[display("{_0} of player {_1}")]
    Player(PlayerStep, PlayerId),
}

impl UpdateStep {
    /// The step's name, its variant's (without the player): the harness names the game's save at
    /// the end of the step by it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::World(step) => step.into(),
            Self::Player(step, _) => step.into(),
        }
    }
}

/// What an update's steps hand on to its later steps.
struct UpdateRun<'a> {
    /// The update's game milliseconds.
    dt: Millis,
    /// The commands recorded in the update.
    commands: &'a [Command],
    /// Which players' gates are open (`BaseWorld::update`'s flags), from `OpenGates` to
    /// `CloseGates`.
    gates_open: Vec<bool>,
    /// The Empire Wars start's pass is due in this update (the world's `+0x6c6`, which the game
    /// sets after the commands).
    start_work: bool,
    /// The seed `Draw` took, which `Reseed` gives the streams.
    seed: u32,
}

/// A player without a time slot in the world.
const NO_CLOCK: GenieError = GenieError::unreachable("a player without a time slot");

impl World {
    /// One world update of `dt` game milliseconds, with the commands recorded in it: the
    /// schedule's steps in turn ([`World::tick_observed`]).
    pub fn tick_with(&mut self, dt: Millis, commands: &[Command]) {
        self.tick_observed(dt, commands, &mut |_, _| {});
    }

    /// One world update of `dt` game milliseconds, with the commands recorded in it: the steps
    /// before the players' ([`BEFORE_PLAYERS`]), each player's steps ([`EACH_PLAYER`]) player by
    /// player, then the steps after them ([`AFTER_PLAYERS`]). After each step `observe` sees the
    /// world as the step left it. A step's error stops its player, a world step's every player.
    pub fn tick_observed(
        &mut self,
        dt: Millis,
        commands: &[Command],
        observe: &mut dyn FnMut(&Self, UpdateStep),
    ) {
        let _update = tracing::debug_span!(
            target: UPDATE,
            "update",
            tick = self.ticks.get() + 1,
            world_time = self.world_time.after(dt).millis()
        )
        .entered();
        let mut run = UpdateRun {
            dt,
            commands,
            gates_open: vec![false; self.players.len()],
            start_work: false,
            seed: 0,
        };
        for step in BEFORE_PLAYERS {
            self.world_step(step, &mut run);
            observe(self, UpdateStep::World(step));
        }
        for player in self.player_ids() {
            let p = player;
            let _player =
                tracing::debug_span!(target: UPDATE, "player", player = p.index()).entered();
            for step in EACH_PLAYER {
                let _step = tracing::trace_span!(target: UPDATE, "step", step = %step).entered();
                if let Err(e) = self.player_step(step, &mut run, p) {
                    self.stop_player(p, e);
                }
                observe(self, UpdateStep::Player(step, player));
            }
        }
        for step in AFTER_PLAYERS {
            self.world_step(step, &mut run);
            observe(self, UpdateStep::World(step));
        }
        // the parts' bookkeeping (FD-045-D51): a part whose object the world no longer holds
        // stops every player
        if let Err(e) = self.check_parts() {
            for p in PlayerId::each(self.players.len()) {
                self.stop_player(p, e.clone());
            }
        }
    }

    /// Player `p`'s time slot (`world+0x18c[p]`).
    ///
    /// # Errors
    ///
    /// A player without a slot.
    fn clock_mut(&mut self, p: PlayerId) -> Result<&mut Millis, GenieError> {
        self.player_time
            .get_mut(p.index())
            .ok_or_else(|| NO_CLOCK.with("player", p))
    }

    /// The world's players in their order; a world holds at most a byte's players (a save's
    /// player count).
    fn player_ids(&self) -> impl Iterator<Item = PlayerId> + use<> {
        (0..=u8::MAX).map(PlayerId::from).take(self.players.len())
    }

    /// World step `step`; its error stops every player.
    fn world_step(&mut self, step: WorldStep, run: &mut UpdateRun<'_>) {
        let _step = tracing::trace_span!(target: UPDATE, "step", step = %step).entered();
        if let Err(e) = self.run_world_step(step, run) {
            for p in PlayerId::each(self.players.len()) {
                self.stop_player(p, e.clone());
            }
        }
    }

    /// World step `step`'s work.
    fn run_world_step(
        &mut self,
        step: WorldStep,
        run: &mut UpdateRun<'_>,
    ) -> Result<(), GenieError> {
        match step {
            WorldStep::Commands => {
                self.command_phase(run.dt, run.commands);
                run.start_work = take(&mut self.start_work_due);
            }
            WorldStep::Draw => {
                self.grow_unknown(run.dt.seconds());
                run.seed = self.streams.get(Stream::World).rand15();
            }
            // every player's ghosts (FUN_141166550 on +0x40), then every player's watched list
            // (FUN_1410905e0); the objects' visibility masks (FUN_14107d280 on +0x80, before the
            // draw) are the registry's, kept as the objects update
            WorldStep::Ghosts => {
                for p in PlayerId::each(self.players.len()) {
                    if let Err(e) = self.update_ghosts(p) {
                        self.stop_player(p, e);
                    }
                }
            }
            WorldStep::Watched => {
                for p in PlayerId::each(self.players.len()) {
                    if let Err(e) = self.watch_pass(p) {
                        self.stop_player(p, e);
                    }
                }
            }
            WorldStep::Reseed => self.streams.seed_all(run.seed),
            WorldStep::Turn => {
                *self.clock_mut(self.player_turn)? = Millis::ZERO;
                self.player_turn =
                    PlayerId::try_from((self.player_turn.index() + 1) % self.players.len())?;
            }
            WorldStep::Victory => self.victory_check()?,
            WorldStep::FrameSeed => {
                self.frame_seed = FrameSeed::of_data(self.streams.get(Stream::World).rand15());
            }
            // setLastRandoms (0x140d3ce20): the multiplayer sync's draws
            WorldStep::LastRandoms => {
                let _ = self.streams.set_last_randoms();
            }
            // World::update's last step (0x1415e9266), whatever dt: the world's slot 0x78 with 1
            // (FUN_1415e92b0): the players' masks again and the town centres' flares
            // (FUN_141552df0, [`World::rebuild_vision`]); its UI refresh is not simulation. Masks
            // the slice cannot rebuild leave every player's sight in doubt, as after the load
            WorldStep::Vision => self.rebuild_vision()?,
        }
        Ok(())
    }

    /// Player `p`'s step `step`.
    fn player_step(
        &mut self,
        step: PlayerStep,
        run: &mut UpdateRun<'_>,
        p: PlayerId,
    ) -> Result<(), GenieError> {
        match step {
            PlayerStep::Clock => {
                let time = self.clock_mut(p)?;
                *time = time.wrapping_add(run.dt);
            }
            PlayerStep::OpenGates => self.open_gates(p, &mut run.gates_open)?,
            PlayerStep::StartWork if run.start_work => self.start_work(p)?,
            PlayerStep::StartWork => {}
            PlayerStep::Groups => {
                self.pathing.tiles_changed(self.spatial.take_changes());
                self.update_groups(p)?;
            }
            // the world player's or Gaia's own update ([`crate::players::world_player`]), then
            // WorldPlayerBase::update: its plans start over (`+0x67c`), whatever the update met
            PlayerStep::WorldPlayer => {
                let done = self.update_world_player(p);
                self.pathing.reset_plans(p);
                done?;
            }
            PlayerStep::Objects => {
                let turn = if p == self.player_turn {
                    Some(*self.clock_mut(p)?)
                } else {
                    None
                };
                self.update_objects(p, (run.dt, turn))?;
            }
            PlayerStep::NewTiles => self.process_new_tiles(p)?,
            PlayerStep::ScoutMap => self.world_player_tail(p)?,
            PlayerStep::CloseGates => self.close_gates(p, &mut run.gates_open)?,
            PlayerStep::ClearTiles => {
                if let Some(v) = self.spatial.sight.as_mut() {
                    v.clear_new_tiles(p);
                }
            }
        }
        Ok(())
    }
}

/// The update's schedule: each step once, the world's around each player's, and an observer
/// that sees the world as each step left it.
#[cfg(test)]
mod tests {
    use crate::players::PlayerId;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::World;
    use crate::world::WorldTime;
    use crate::world::player::Player;
    use crate::world::schedule::{
        AFTER_PLAYERS, BEFORE_PLAYERS, EACH_PLAYER, PlayerStep, UpdateStep, WorldStep,
    };

    /// A world of `n` players without objects, its turn at player 1.
    fn bare(n: usize) -> World {
        World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(29910),
            PlayerId::new(1),
            vec![Millis::ZERO; n],
            vec![Player::default(); n],
        )
    }

    /// The number of world steps: [`world_place`]'s arms.
    const WORLD_STEPS: usize = 10;

    /// Each world step's place in the update; the match fails to compile for a new step.
    const fn world_place(step: WorldStep) -> usize {
        match step {
            WorldStep::Commands => 0,
            WorldStep::Draw => 1,
            WorldStep::Ghosts => 2,
            WorldStep::Watched => 3,
            WorldStep::Reseed => 4,
            WorldStep::Turn => 5,
            WorldStep::Victory => 6,
            WorldStep::FrameSeed => 7,
            WorldStep::LastRandoms => 8,
            WorldStep::Vision => 9,
        }
    }

    /// The number of player steps: [`player_place`]'s arms.
    const PLAYER_STEPS: usize = 10;

    /// Each player step's place in a player's part; the match fails to compile for a new step.
    const fn player_place(step: PlayerStep) -> usize {
        match step {
            PlayerStep::Clock => 0,
            PlayerStep::OpenGates => 1,
            PlayerStep::StartWork => 2,
            PlayerStep::Groups => 3,
            PlayerStep::WorldPlayer => 4,
            PlayerStep::Objects => 5,
            PlayerStep::NewTiles => 6,
            PlayerStep::ScoutMap => 7,
            PlayerStep::CloseGates => 8,
            PlayerStep::ClearTiles => 9,
        }
    }

    /// The schedule holds every step once, in the game's order.
    #[test]
    fn the_schedule_holds_every_step_once_in_order() {
        let world: Vec<usize> = BEFORE_PLAYERS
            .iter()
            .chain(&AFTER_PLAYERS)
            .map(|&s| world_place(s))
            .collect();
        assert_eq!(world, (0..WORLD_STEPS).collect::<Vec<_>>());
        let player: Vec<usize> = EACH_PLAYER.iter().map(|&s| player_place(s)).collect();
        assert_eq!(player, (0..PLAYER_STEPS).collect::<Vec<_>>());
    }

    /// The steps before the players' run first, then each player's steps player by player, Gaia
    /// first, then the steps after them.
    #[test]
    fn the_update_runs_each_player_steps_between_the_world_s() {
        let mut w = bare(3);
        let mut seen = Vec::new();
        w.tick_observed(Millis::new(10), &[], &mut |_, step| seen.push(step));
        let mut expected: Vec<UpdateStep> = BEFORE_PLAYERS.map(UpdateStep::World).to_vec();
        for p in 0..3 {
            expected.extend(
                EACH_PLAYER
                    .iter()
                    .map(|&s| UpdateStep::Player(s, PlayerId::from(p))),
            );
        }
        expected.extend(AFTER_PLAYERS.map(UpdateStep::World));
        assert_eq!(seen, expected);
    }

    /// The observer sees each step's result: a player's clock goes on in its own step, the turn
    /// passes in the world's.
    #[test]
    fn an_observer_sees_the_world_as_each_step_left_it() {
        let mut w = bare(3);
        let (mut clocks, mut turns) = (Vec::new(), Vec::new());
        w.tick_observed(Millis::new(10), &[], &mut |w, step| match step {
            UpdateStep::Player(PlayerStep::Clock, _) => clocks.push(w.player_time.clone()),
            UpdateStep::World(WorldStep::Turn) => {
                turns.push((w.player_turn, w.player_time.clone()));
            }
            _ => {}
        });
        let ms = |v: [u32; 3]| v.map(Millis::new).to_vec();
        assert_eq!(clocks, [ms([10, 0, 0]), ms([10, 10, 0]), ms([10, 10, 10])]);
        // player 1's turn ends: its time starts over and the turn passes to player 2
        assert_eq!(turns, [(PlayerId::new(2), ms([10, 0, 10]))]);
    }

    /// An observed update leaves the world as the update without an observer does.
    #[test]
    fn an_observed_update_is_the_update() {
        let (mut a, mut b) = (bare(3), bare(3));
        a.tick(Millis::new(10));
        b.tick_observed(Millis::new(10), &[], &mut |_, _| {});
        assert_eq!(a, b);
    }

    /// A step's name is its variant's, for the harness's game points and the traces.
    #[test]
    fn step_is_named_by_its_variant() {
        let seed = UpdateStep::World(WorldStep::FrameSeed);
        assert_eq!(
            (seed.name(), seed.to_string().as_str()),
            ("FrameSeed", "FrameSeed")
        );
        let objects = UpdateStep::Player(PlayerStep::Objects, PlayerId::new(2));
        assert_eq!(objects.name(), "Objects");
        assert_eq!(objects.to_string(), "Objects of player 2");
        assert_eq!(PlayerStep::ClearTiles.to_string(), "ClearTiles");
    }
}
