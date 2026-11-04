//! The world player's update: the loop's seconds, the explored share, the defeat check and
//! what keeps a player in the game.

use crate::GenieResult;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::objects::ObjectState;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::players::game_status::GameStatus;
use crate::players::tracker::ListKey;
use crate::players::world_player::{
    ALIVE_STORE_MODE, PERCENT, SECOND, UNKEPT_HIGH, UNKEPT_LOW, get, set,
};
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::slots::PlayerSlots;

impl World {
    /// Player `player_id`'s slot 0x100 up to `WorldPlayerBase::update`. Gaia
    /// (`WorldPlayerGaia::update`) adds the update's seconds to its own update time. A world player
    /// adds them to `mUpdateTime` (`ADDSS` of `world+0x164` and the time) and, while it is at least
    /// a second (`COMISS` with 1, `JC` ends the loop, also for a NaN), takes a second off and runs
    /// the second's work ([`World::world_player_second`]) with the time before the step.
    ///
    /// # Errors
    ///
    /// The second's work met a part of the game the slice does not model.
    pub fn update_world_player(&mut self, player_id: PlayerId) -> GenieResult<()> {
        let dt = self.dt_seconds;
        let player = self.players.player_mut(player_id)?;
        if player.kind == PlayerKind::Gaia {
            player.gaia_update_time += dt;
            return Ok(());
        }
        player.update_time += dt;
        loop {
            let time = self.players.player(player_id)?.update_time;
            if below_or_unordered(time.get(), SECOND) {
                return Ok(());
            }
            self.players.player_mut(player_id)?.update_time = time - Seconds::new(SECOND);
            self.world_player_second(player_id, time)?;
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
        player_id: PlayerId,
        time: Seconds,
    ) -> GenieResult<()> {
        let (world_time, period) = (self.world_time, self.history_period);
        let player = self.players.player_mut(player_id)?;
        player.history.update(
            &player.attributes,
            world_time,
            period,
            self.model.slots,
            &self.model.tuning.history,
        )?;
        let explored = if self.settings.explored_all() {
            Some(PERCENT)
        } else {
            self.explored_share(player_id).map(|share| share * PERCENT)
        };
        let player = self.players.player_mut(player_id)?;
        if let Some(e) = explored {
            set(
                &mut player.attributes,
                self.model.slots.attribute.explored,
                Amount::new(e),
            );
        }
        let amounts = &player.attributes;
        let ratio = get(amounts, self.model.slots.attribute.kills)
            - (get(amounts, self.model.slots.attribute.units_owned)
                - get(amounts, self.model.slots.attribute.population));
        set(
            &mut player.attributes,
            self.model.slots.attribute.kill_ratio,
            ratio,
        );
        if !at_most_or_unordered(
            0.0,
            get(&player.attributes, self.model.slots.attribute.kill_ratio).get(),
        ) {
            set(
                &mut player.attributes,
                self.model.slots.attribute.kill_ratio,
                Amount::ZERO,
            );
        }
        if !matches!(player.kind, PlayerKind::Human | PlayerKind::Computer) {
            return Ok(());
        }
        self.defeat_check(player_id)?;
        if self.players.player(player_id)?.game_status == GameStatus::Playing {
            self.trickles(player_id)?;
            self.mill_food(player_id, time)?;
        }
        Ok(())
    }

    /// `FUN_141285c50` on player `player_id`'s visible map (`+0x70`): its explored tiles (`+0x28`)
    /// over its tiles (`+0x2c`, the width times the height its load sets), each converted
    /// (`CVTDQ2PS`) before the division. None in a slice without the players' sight, whose
    /// attribute 22 the update then leaves.
    #[must_use]
    pub fn explored_share(&self, player_id: PlayerId) -> Option<f32> {
        let sight = self.spatial.sight.as_ref()?;
        let map = sight.maps.get(player_id.index())?;
        Some(map.explored.get() as f32 / (sight.size.width() * sight.size.height()) as f32)
    }

    /// The defeat check of player `player_id` (`0x14163b72b`). The player stays in the game while
    /// one of its victory timers runs ([`VictoryTimes::running`]) or else one of its objects, in
    /// its list's order, keeps it ([`World::keeps_alive`]). Under the sudden death rule
    /// ([`crate::world::settings::Settings::sudden_death`]), asked only for a player that stays,
    /// its first built town centre ([`World::first_town_centre`]) decides once the player had one
    /// (`mSuddenDeathCanBeEliminated`, `+0x1380`, which the first one found sets). A player not yet
    /// lost then loses when it does not stay, or when it stays without a king (master 434 in its
    /// tracker, `FUN_14104eea0`) under the regicide rule
    /// ([`crate::world::settings::Settings::regicide`]): the status setter takes 2
    /// ([`World::set_status`]) and attribute 45 is cleared.
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have; the status setter's.
    ///
    /// [`VictoryTimes::running`]: crate::players::world_player::VictoryTimes::running
    pub fn defeat_check(&mut self, player_id: PlayerId) -> GenieResult<()> {
        let mut stays = self.players.player(player_id)?.victory_times.running();
        if !stays {
            for unit in &self.players.player(player_id)?.objects {
                if self.keeps_alive(player_id, unit)? {
                    stays = true;
                    break;
                }
            }
        }
        if stays && self.settings.sudden_death() {
            let centre = self.first_town_centre(player_id).is_some();
            let player = self.players.player_mut(player_id)?;
            if player.sudden_death_eliminable {
                stays = centre;
            } else if centre {
                player.sudden_death_eliminable = true;
            }
        }
        if self.players.player(player_id)?.game_status == GameStatus::Lost {
            return Ok(());
        }
        let kingless = self.settings.regicide()
            && self
                .players
                .player(player_id)?
                .tracker
                .list(ListKey::master(self.model.slots.master.king))
                .is_empty();
        if !stays || kingless {
            self.set_status(player_id, GameStatus::Lost)?;
            set(
                &mut self.players.player_mut(player_id)?.attributes,
                self.model.slots.attribute.survival,
                Amount::ZERO,
            );
        }
        Ok(())
    }

    /// Whether object `unit` keeps player `player_id` in the game (`0x14163b790`), by its master's
    /// class (`+0x410`, the decoded short, compared unsigned) and its state (`+0x640`):
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
    ///   ([`crate::world::GameMode::scenario`]; the game asks the lobby's settings,
    ///   `FUN_14152f9c0`, which hold the game's options).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    pub fn keeps_alive(&self, player_id: PlayerId, unit: &Unit) -> GenieResult<bool> {
        let master = unit.master(self.master_tables(), player_id)?;
        let state = unit.life.state;
        if (ObjectState::Unfinished..=ObjectState::DyingSprite).contains(&state)
            && master.storage.iter().any(|&(kind, _, mode)| {
                self.model
                    .roles
                    .of_some_attribute(kind)
                    .contains(AttributeRoles::ALIVE_STORAGE)
                    && mode == ALIVE_STORE_MODE
            })
        {
            return Ok(true);
        }
        let none = UNKEPT_LOW.contains(master.class);
        let builds = self
            .model
            .roles
            .of_class(master.class)
            .intersects(ClassRoles::TOWER | ClassRoles::BUILDING);
        let never = UNKEPT_HIGH.contains(master.class);
        let other = !(none || builds || never);
        if other && state == ObjectState::Alive {
            return Ok(true);
        }
        if builds && state == ObjectState::Alive {
            let count = self
                .trainable_count(player_id, master.id)
                .wrapping_add(self.upgradable_count(player_id, master.id));
            if 0 < count {
                return Ok(true);
            }
        }
        if (builds || other)
            && self
                .model
                .roles
                .of_class(master.class)
                .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET)
            && state == ObjectState::Foundation
        {
            return Ok(true);
        }
        Ok(self
            .model
            .roles
            .of_master(master.id)
            .contains(MasterRoles::WONDER)
            && state != ObjectState::Foundation
            && self.settings.game_mode.scenario())
    }
}
