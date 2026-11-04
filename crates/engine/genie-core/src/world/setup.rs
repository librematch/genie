//! The world as the header save holds it ([`World::new`]): the next object id, the
//! planners and the shape mesh the game's load builds.

use crate::GenieResult;
use crate::groups::UnitGroups;
use crate::map::spatial::Spatial;
use crate::math::rng::Streams;
use crate::movement::MoveState;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::RelicCount;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterTables;
use crate::objects::masters::ObstructionClass;
use crate::pathing::Pathing;
use crate::pathing::zones::Zones;
use crate::players::PlayerId;
use crate::players::Team;
use crate::players::ai_module::AiModules;
use crate::players::market::Market;
#[cfg(test)]
use crate::testing::build_158041::build_158041;
use crate::trace::new_object;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::index::ObjectIndex;
use crate::world::parts::Parts;
use crate::world::player::Player;
use crate::world::settings::Settings;
use crate::world::{FrameSeed, Mode680, PlayState, Tick};
use crate::world::{HISTORY_PERIOD, Recycling, World};
use genie_model::layer::BuildLayer;
use genie_model::model::Model;

impl World {
    /// The world's next object id (`FUN_14152fd70`) for a new object of `master` for `player`: it
    /// counts on, and the id goes to the trace ([`crate::trace::new_object`]).
    #[track_caller]
    pub(crate) fn take_object_id(&mut self, player: PlayerId, master: MasterId) -> ObjectId {
        let id = self.next_object_id;
        self.next_object_id = id.successor();
        new_object(self.ticks, player, id, master);
        id
    }

    /// The world as the header save holds it: its time, frame seed, turn order and players.
    #[must_use]
    pub fn new(
        world_time: WorldTime,
        frame_seed: FrameSeed,
        player_turn: PlayerId,
        player_time: Vec<Millis>,
        players: Vec<Player>,
    ) -> Self {
        Self {
            world_time,
            frame_seed,
            player_turn,
            player_time,
            index: ObjectIndex::of(&players),
            parts: Parts::default(),
            players,
            ticks: Tick::ZERO,
            next_object_id: ObjectId::default(),
            model: Model::default().with_layer(&starting_layer()),
            spatial: Spatial::default(),
            pathing: Pathing::default(),
            settings: Settings::default(),
            start_work_due: false,
            mode_680: Mode680::ZERO,
            spawned_relics: RelicCount::ZERO,
            game_state: PlayState::Playing,
            victory_conquest: false,
            zones: Zones::default(),
            recycling: Recycling::default(),
            streams: Streams::default(),
            market: Market::default(),
            ai: AiModules::default(),
            groups: UnitGroups::default(),
            last_commanded: Vec::new(),
            dt_seconds: Seconds::ZERO,
            dt: Millis::ZERO,
            history_period: HISTORY_PERIOD,
            allow_gaia_capture: Vec::new(),
            local_player: PlayerId::new(0),
            alliance_locked: 0,
            teams: [Team::ALONE; 8],
            instant_work: 0,
        }
    }
}

impl World {
    /// `World::load_world`'s planners (`FUN_140f7d310`): the restrictions of Gaia's masters 83
    /// (the villager's) and 535 (the galley's: the build's `villager_male` and `boarding_galley`,
    /// Gaia's entries `+0x298` and `+0x10b8`), each unless one of the same values has one; any
    /// other restriction finds none (its units walk without the tile pathfinder).
    pub fn register_planners(&mut self) {
        let master_slots = self.model.slots.master;
        for master in [master_slots.villager_male, master_slots.boarding_galley] {
            if let Some(restriction_id) = self
                .model
                .masters
                .first()
                .and_then(|masters| masters.get(master))
                .and_then(|m| m.terrain_restriction)
            {
                self.pathing.register(restriction_id);
            }
        }
    }

    /// The shape mesh the game's load builds, which no save holds: an empty mesh of the map's
    /// width, then the load's rehook of each object (`RGE_Static_Object::rehook`, slot 9, through
    /// `FUN_141665650` and `FUN_1411689f0`: each player's live list, then its sleeping list):
    /// its position set again (slot 0x58 -> `FUN_14118c540`: below state 7 and tall, then
    /// `FUN_141056460`'s shape by its master's obstruction kind, not 0 or 12, and class, not 0,
    /// for an object that does not move, [`Pathing::place_shape`]), then with an obstruction
    /// record the refresh an update ends with (`FUN_141054fe0`, [`Pathing::refresh`]). An object
    /// inside another has none (it left the obstruction manager); `mROOAlreadyCalled`
    /// (`+0x686`) is not held (0 at a game's start), and a desolid foundation in a save is left
    /// without one, as the game's own placement leaves it (unchecked at a load).
    ///
    /// # Errors
    ///
    /// An object of a master its owner lacks.
    pub fn build_mesh(&mut self) -> GenieResult<()> {
        let width = u32::try_from(self.spatial.map.width()).unwrap_or(0);
        self.pathing.start_mesh(width);
        for (player_id, player) in PlayerId::each(self.players.len()).zip(&self.players) {
            for unit in player.objects.iter().chain(&player.sleeping) {
                if unit.life.inside.is_some() || ObjectState::Removed <= unit.life.state {
                    continue;
                }
                let master = unit.master(
                    MasterTables::new(&self.model.masters, &self.parts.own_master),
                    player_id,
                )?;
                let Some(placed) = self.spatial.get(unit.id) else {
                    continue;
                };
                let moving = MoveState::of_mover(self.parts.mover_of(unit.id));
                let desolid = self
                    .parts
                    .building_of(unit.id)
                    .construction
                    .is_some_and(|construction| construction.desolid);
                let kind = master.obstruction_kind;
                if placed.tall
                    && kind.recorded()
                    && master.obstruction_class != ObstructionClass::NONE
                    && moving == MoveState::Standing
                    && !desolid
                {
                    self.pathing.place_shape(unit.id, placed);
                }
                self.pathing
                    .refresh(unit.id, placed, moving, master.outline);
            }
        }
        Ok(())
    }
}

/// The build layer a new world starts with: none, the import applies the recording's build's
/// (FD-046-D08); genie-core's own tests build their worlds on build 158041's fixture, as their
/// masters and classes are that build's.
#[cfg(not(test))]
fn starting_layer() -> BuildLayer {
    BuildLayer::default()
}

/// The build layer a new world of genie-core's tests starts with: build 158041's fixture.
#[cfg(test)]
fn starting_layer() -> BuildLayer {
    build_158041()
}
