//! The world player's update: the loop's seconds, the explored share, the defeat check and
//! what keeps a player in the game.

use crate::GenieError;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::objects::ObjectState;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::ids::WONDER;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerType;
use crate::players::attribute;
use crate::players::game_status::GameStatus;
use crate::players::tracker::ListKey;
use crate::players::world_player::{
    ALIVE_STORE_MODE, PERCENT, POPULATION_STORAGE, SECOND, UNKEPT_HIGH, UNKEPT_LOW, get, set,
};
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::PlayerSlots;

impl World {
    /// Player `p`'s slot 0x100 up to `WorldPlayerBase::update`. Gaia (`WorldPlayerGaia::update`)
    /// adds the update's seconds to its own update time. A world player adds them to
    /// `mUpdateTime` (`ADDSS` of `world+0x164` and the time) and, while it is at least a second
    /// (`COMISS` with 1, `JC` ends the loop, also for a NaN), takes a second off and runs the
    /// second's work ([`World::world_player_second`]) with the time before the step.
    ///
    /// # Errors
    ///
    /// The second's work met a part of the game the slice does not model.
    pub fn update_world_player(&mut self, p: PlayerId) -> Result<(), GenieError> {
        let dt = self.dt_seconds;
        let player = self.players.player_mut(p)?;
        if player.kind == PlayerType::GAIA {
            player.gaia_update_time += dt;
            return Ok(());
        }
        player.update_time += dt;
        loop {
            let time = self.players.player(p)?.update_time;
            if below_or_unordered(time.get(), SECOND) {
                return Ok(());
            }
            self.players.player_mut(p)?.update_time = time - Seconds::new(SECOND);
            self.world_player_second(p, time)?;
        }
    }

    /// One pass of the world player's loop, `time` its update time before the step:
    /// - the history's update with the `.dat`'s period ([`crate::players::history::History::update`]);
    /// - attribute 22: 100 when every player explored the map from the start
    ///   ([`crate::world::settings::Settings::explored_all`]), else the explored share
    ///   ([`World::explored_share`]) times 100;
    /// - attribute 44: attribute 20 less (attribute 19 less attribute 11), 0 when that is below 0
    ///   (`COMISS` 0 with it, `JBE`: a NaN stays);
    /// - for a human or computer player (`FUN_141663f30`: type 1 or 3) the defeat check
    ///   ([`World::defeat_check`]), then, while it plays, the trickles ([`World::trickles`]) and
    ///   the mills' food ([`World::mill_food`]).
    ///
    /// # Errors
    ///
    /// The defeat check's ([`World::defeat_check`]).
    pub(super) fn world_player_second(
        &mut self,
        p: PlayerId,
        time: Seconds,
    ) -> Result<(), GenieError> {
        let (world_time, period) = (self.world_time, self.history_period);
        let player = self.players.player_mut(p)?;
        player
            .history
            .update(&player.attributes, world_time, period)?;
        let explored = if self.settings.explored_all() {
            Some(PERCENT)
        } else {
            self.explored_share(p).map(|share| share * PERCENT)
        };
        let player = self.players.player_mut(p)?;
        if let Some(e) = explored {
            set(&mut player.attributes, attribute::EXPLORED, Amount::new(e));
        }
        let a = &player.attributes;
        let ratio = get(a, attribute::KILLS)
            - (get(a, attribute::UNITS_OWNED) - get(a, attribute::POPULATION));
        set(&mut player.attributes, attribute::KILL_RATIO, ratio);
        if !at_most_or_unordered(0.0, get(&player.attributes, attribute::KILL_RATIO).get()) {
            set(&mut player.attributes, attribute::KILL_RATIO, Amount::ZERO);
        }
        if !matches!(player.kind, PlayerType::HUMAN | PlayerType::COMPUTER) {
            return Ok(());
        }
        self.defeat_check(p)?;
        if self.players.player(p)?.game_status == GameStatus::PLAYING {
            self.trickles(p)?;
            self.mill_food(p, time)?;
        }
        Ok(())
    }

    /// `FUN_141285c50` on player `p`'s visible map (`+0x70`): its explored tiles (`+0x28`) over
    /// its tiles (`+0x2c`, the width times the height its load sets), each converted
    /// (`CVTDQ2PS`) before the division. None in a slice without the players' sight, whose
    /// attribute 22 the update then leaves.
    #[must_use]
    pub fn explored_share(&self, p: PlayerId) -> Option<f32> {
        let sight = self.spatial.sight.as_ref()?;
        let map = sight.maps.get(p.index())?;
        Some(map.explored.get() as f32 / (sight.size.width() * sight.size.height()) as f32)
    }

    /// The defeat check of player `p` (`0x14163b72b`). The player stays in the game while one of
    /// its victory timers runs ([`VictoryTimes::running`]) or else one of its objects, in its
    /// list's order, keeps it ([`World::keeps_alive`]). Under the sudden death rule
    /// ([`crate::world::settings::Settings::sudden_death`]), asked only for a player that stays, its
    /// first built town centre ([`World::first_town_centre`]) decides once the player had one
    /// (`mSuddenDeathCanBeEliminated`, `+0x1380`, which the first one found sets). A player not
    /// yet lost then loses when it does not stay, or when it stays without a king (master 434 in
    /// its tracker, `FUN_14104eea0`) under the regicide rule
    /// ([`crate::world::settings::Settings::regicide`]): the status setter takes 2
    /// ([`World::set_status`]) and attribute 45 is cleared.
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have; the status setter's.
    ///
    /// [`VictoryTimes::running`]: crate::players::world_player::VictoryTimes::running
    pub fn defeat_check(&mut self, p: PlayerId) -> Result<(), GenieError> {
        let mut stays = self.players.player(p)?.victory_times.running();
        if !stays {
            for unit in &self.players.player(p)?.objects {
                if self.keeps_alive(p, unit)? {
                    stays = true;
                    break;
                }
            }
        }
        if stays && self.settings.sudden_death() {
            let centre = self.first_town_centre(p).is_some();
            let player = self.players.player_mut(p)?;
            if player.sudden_death_eliminable {
                stays = centre;
            } else if centre {
                player.sudden_death_eliminable = true;
            }
        }
        if self.players.player(p)?.game_status == GameStatus::LOST {
            return Ok(());
        }
        let kingless = self.settings.regicide()
            && self
                .players
                .player(p)?
                .tracker
                .list(ListKey::master(ids::KING))
                .is_empty();
        if !stays || kingless {
            self.set_status(p, GameStatus::LOST)?;
            set(
                &mut self.players.player_mut(p)?.attributes,
                attribute::SURVIVAL,
                Amount::ZERO,
            );
        }
        Ok(())
    }

    /// Whether object `unit` keeps player `p` in the game (`0x14163b790`), by its master's class
    /// (`+0x410`, the decoded short, compared unsigned) and its state (`+0x640`):
    /// - in states 1 to 4, a resource storage of the master holding attribute 11 with the store
    ///   mode 0x20 keeps it;
    /// - classes 11, 27 and 30 (other, walls, flags) and 1, 2, 14, 19, 20, 21, 37, 39, 42, 49, 58
    ///   and 60 do not;
    /// - classes 3 and 52 (buildings, towers) keep it in state 2 when the player can train or
    ///   will be able to train there (`FUN_141625a20` and `FUN_141610470`'s counts,
    ///   [`World::trainable_count`], [`World::upgradable_count`], summed as shorts);
    /// - any other class keeps it in state 2, and classes 51 and 54 (the packed and unpacked
    ///   siege) in state 0;
    /// - whatever the class, a wonder (master 276) not in state 0 keeps it in the scenario modes
    ///   ([`crate::world::scenario_mode`]; the game asks the lobby's settings, `FUN_14152f9c0`,
    ///   which hold the game's options).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    pub fn keeps_alive(&self, p: PlayerId, unit: &Unit) -> Result<bool, GenieError> {
        let master = unit.master(self.master_tables(), p)?;
        let state = unit.life.state;
        if (ObjectState::Unfinished..=ObjectState::DyingSprite).contains(&state)
            && master.storage.iter().any(|&(kind, _, mode)| {
                kind == Some(POPULATION_STORAGE) && mode == ALIVE_STORE_MODE
            })
        {
            return Ok(true);
        }
        let none = UNKEPT_LOW.contains(master.class);
        let builds = matches!(master.class, class::TOWER | class::BUILDING);
        let never = UNKEPT_HIGH.contains(master.class);
        let other = !(none || builds || never);
        if other && state == ObjectState::Alive {
            return Ok(true);
        }
        if builds && state == ObjectState::Alive {
            let count = self
                .trainable_count(p, master.id)
                .wrapping_add(self.upgradable_count(p, master.id));
            if 0 < count {
                return Ok(true);
            }
        }
        if (builds || other)
            && matches!(master.class, class::CLASS_51 | class::CLASS_54)
            && state == ObjectState::Foundation
        {
            return Ok(true);
        }
        Ok(master.id == WONDER
            && state != ObjectState::Foundation
            && self.settings.game_mode.scenario())
    }
}
