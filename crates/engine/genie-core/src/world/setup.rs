//! The world as the header save holds it ([`World::new`]): the next object id, the
//! planners and the shape mesh the game's load builds.

use crate::GenieError;
use crate::map::spatial::Spatial;
use crate::math::rng::Streams;
use crate::movement::Mover;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObstructionClass;
use crate::pathing::Pathing;
use crate::players::PlayerId;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::player::Player;
use crate::world::{HISTORY_PERIOD, PLANNED_MASTERS, Recycling, World};

impl World {
    /// The world's next object id (`FUN_14152fd70`) for a new object of `master` for `player`: it
    /// counts on, and the id goes to the trace ([`crate::trace::new_object`]).
    #[track_caller]
    pub(crate) fn take_object_id(&mut self, player: PlayerId, master: MasterId) -> ObjectId {
        let id = self.next_object_id;
        self.next_object_id = id.successor();
        crate::trace::new_object(self.ticks, player, id, master);
        id
    }

    /// The world as the header save holds it: its time, frame seed, turn order and players.
    #[must_use]
    pub fn new(
        world_time: WorldTime,
        frame_seed: crate::world::FrameSeed,
        player_turn: PlayerId,
        player_time: Vec<Millis>,
        players: Vec<Player>,
    ) -> Self {
        Self {
            world_time,
            frame_seed,
            player_turn,
            player_time,
            index: crate::world::index::ObjectIndex::of(&players),
            parts: crate::world::parts::Parts::default(),
            players,
            ticks: crate::world::Tick::ZERO,
            next_object_id: ObjectId::default(),
            masters: Vec::new(),
            spatial: Spatial::default(),
            pathing: Pathing::default(),
            graphics: crate::objects::Graphics::default(),
            drop_sites: Vec::new(),
            lines: crate::objects::lines::UnitLines::default(),
            linked_techs: std::collections::BTreeMap::new(),
            settings: crate::world::settings::Settings::default(),
            start_work_due: false,
            mode_680: crate::world::Mode680::ZERO,
            spawned_relics: crate::objects::RelicCount::ZERO,
            game_state: crate::world::PlayState::PLAYING,
            victory_conquest: false,
            zones: crate::pathing::zones::Zones::default(),
            recycling: Recycling::default(),
            streams: Streams::default(),
            techs: Vec::new(),
            effects: Vec::new(),
            market: crate::players::market::Market::default(),
            ai: crate::players::ai_module::AiModules::default(),
            groups: crate::groups::UnitGroups::default(),
            last_commanded: Vec::new(),
            dt_seconds: Seconds::ZERO,
            dt: Millis::ZERO,
            history_period: HISTORY_PERIOD,
            allow_gaia_capture: Vec::new(),
            local_player: PlayerId::new(0),
            tech_tree: crate::players::world_player::TechTree::default(),
            alliance_locked: 0,
            teams: [crate::players::Team::ALONE; 8],
            instant_work: 0,
        }
    }
}

impl World {
    /// `World::load_world`'s planners (`FUN_140f7d310`): the restrictions of Gaia's masters 83
    /// (the villager's) and 535 (the galley's), each unless one of the same values has one; any
    /// other restriction finds none (its units walk without the tile pathfinder).
    pub fn register_planners(&mut self) {
        for master in PLANNED_MASTERS {
            if let Some(r) = self
                .masters
                .first()
                .and_then(|m| m.get(master))
                .and_then(|m| m.terrain_restriction)
            {
                self.pathing.register(r);
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
    pub fn build_mesh(&mut self) -> Result<(), GenieError> {
        let width = u32::try_from(self.spatial.map.width()).unwrap_or(0);
        self.pathing.start_mesh(width);
        for (q, p) in PlayerId::each(self.players.len()).zip(&self.players) {
            for u in p.objects.iter().chain(&p.sleeping) {
                if u.life.inside.is_some() || ObjectState::Removed <= u.life.state {
                    continue;
                }
                let m = u.master(
                    crate::objects::masters::MasterTables::new(
                        &self.masters,
                        &self.parts.own_master,
                    ),
                    q,
                )?;
                let Some(placed) = self.spatial.get(u.id) else {
                    continue;
                };
                let moving = u.body.mover().is_some_and(Mover::is_moving);
                let desolid = u.building.construction.is_some_and(|c| c.desolid);
                let kind = m.obstruction_type;
                if placed.tall
                    && kind.recorded()
                    && m.obstruction_class != ObstructionClass::NONE
                    && !moving
                    && !desolid
                {
                    self.pathing.place_shape(u.id, placed);
                }
                self.pathing.refresh(u.id, placed, moving, m.outline);
            }
        }
        Ok(())
    }
}
