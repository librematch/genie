//! What the unit AI asks of the world through its view: [`crate::unit_ai::UnitSide`] for
//! [`UnitView`]. The long answers live in their systems as inherent `UnitView` methods
//! (FD-041-D09): the attack command in `combat/attack/unit_ai_side.rs`, the build command and the
//! foundation searches in `construction/build/unit_ai_side.rs`, the hunt command in
//! `gathering/hunt/unit_ai_side.rs`, the wait for a foundation in `construction/wait_solidify.rs`
//! and the resource search in `gathering/closest_resource.rs`.

use crate::GenieError;
use crate::gathering::closest_resource::herded;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::{Placed, SearchQuery, Spatial};
use crate::math::rng::{Stream, Well512};
use crate::movement::{Planning, footprint_gap_squared};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::ClassMask;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::class;
use crate::objects::masters::player_master;
use crate::orders::object_gone;
use crate::players::PlayerId;
use crate::players::UnitRelation;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{UnitView, WorldPicker, patrol_information, town_centre};
use crate::unit_ai::{Search, UnitAi};
use crate::world::PETARD;
use crate::world::slots::PlayerSlots;

impl crate::unit_ai::UnitSide for UnitView<'_, '_> {
    fn table_drop_site(&mut self) -> Result<Option<crate::gathering::DropSiteId>, GenieError> {
        let class = self
            .unit
            .master(self.c.w.master_tables(), self.player)?
            .class;
        let at = self.unit.position().ground();
        let view = owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?;
        let look = |id: ObjectId| view.own(id).map(|u| (u.life.state, u.position()));
        Ok(crate::orders::nearest_table_site(
            (
                &self.c.w.drop_sites,
                self.civ,
                &self.c.w.players.player(self.c.player)?.tracker,
            ),
            (at, self.unit.life.attribute_type, class),
            look,
        ))
    }

    fn patrol_information(
        &self,
    ) -> Option<(crate::unit_ai::PatrolPath, crate::movement::WaypointIndex)> {
        patrol_information(self.c.w.parts.mover_of(self.unit.id))
    }

    /// Slot 0x278 (`FUN_141158720`, [`Spatial::point_clear`]): the unit, of its radius and its
    /// restriction's terrain classes, could stand at `point`.
    ///
    /// [`Spatial::point_clear`]: crate::map::spatial::Spatial::point_clear
    fn can_stand(&mut self, point: Point) -> Result<bool, GenieError> {
        let unit = &*self.unit;
        let mover = self
            .c
            .w
            .parts
            .mover_of(unit.id)
            .ok_or(GenieError::unsupported(
                "a stand test of a unit without movement state",
            ))?;
        let by_id = crate::world::index::ObjectsRef::new(
            &self.c.w.players,
            &self.c.w.index,
            &self.c.w.masters,
            &self.c.w.parts,
        );
        self.c.w.spatial.point_clear(
            unit.id,
            point,
            Distance::new(mover.stats.radius.x()),
            mover.stats.restriction_mask,
            by_id,
        )
    }

    fn off_footprint(
        &mut self,
        site: crate::construction::FoundationId,
        radius: Footprint,
    ) -> Result<crate::unit_ai::OffFootprint, GenieError> {
        self.spot_off_footprint(site, radius)
    }

    fn foundation_search(&mut self) -> Result<Option<ObjectId>, GenieError> {
        self.own_foundation_near()
    }

    fn call_for_help(&mut self, attacker: ObjectId) -> Result<(), GenieError> {
        /// The classes that never help: 1 to 3, 18 to 21, 27 and 39.
        const NO_HELP_CLASSES: ClassMask = ClassMask::new(0x0080_083c_000e, 0x28);
        /// The siege tower master (`FUN_141194f10`'s 0x451).
        const SIEGE_TOWER: MasterId = MasterId::named(0x451);
        /// `AskForHelp`'s reach (`DAT_142ecccc4`): the squared footprint gap of 3 tiles.
        const HELP_REACH: DistanceSquared =
            DistanceSquared::new(crate::math::image::unit_ai::HELP_REACH);
        let lists = self.c.w.spatial.searched().ok_or(GenieError::unsupported(
            "a search that reads the registry's last answer (before any query)",
        ))?;
        let others = owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?;
        let find = |id: ObjectId| others.find(id);
        let gaia_attacker = find(attacker).is_some_and(|(owner, _)| owner == PlayerId::GAIA);
        let me = self.unit.id;
        let my_radius = self
            .unit
            .master(self.c.w.master_tables(), self.player)?
            .radius;
        for r in lists[UnitRelation::OWN]
            .iter()
            .chain(&lists[UnitRelation::ALLY])
        {
            if r.id == me {
                continue;
            }
            let Some((owner, u)) = find(r.id) else {
                continue;
            };
            if !self.c.w.parts.unit_ai.contains(u.id) || ObjectState::Dying <= u.life.state {
                continue;
            }
            let m = u.master(self.c.w.master_tables(), owner)?;
            let no_help = NO_HELP_CLASSES.contains(m.class);
            if no_help || m.id == SIEGE_TOWER {
                continue;
            }
            if !gaia_attacker && m.class == class::VILLAGER {
                continue;
            }
            if owner != self.player && !self.c.w.spatial.allied_with(owner, self.player) {
                continue;
            }
            let gap =
                footprint_gap_squared(self.unit.position(), my_radius, u.position(), m.radius);
            if HELP_REACH < gap {
                continue;
            }
            if !self.helper_free(owner, u, self.actions, m, attacker)? {
                continue;
            }
            return crate::unit_ai::helper_answers();
        }
        Ok(())
    }

    fn duel(&mut self, other: ObjectId) -> Result<crate::unit_ai::Duel, GenieError> {
        self.picker(None)?.duel(&self.c.w.spatial, other)
    }

    fn harmless(&mut self, attacker: ObjectId) -> Result<bool, GenieError> {
        self.picker(None)?.harmless(&self.c.w.spatial, attacker)
    }

    fn defended_attacker(
        &mut self,
        target: ObjectId,
    ) -> Result<crate::unit_ai::NearestAttacker, GenieError> {
        self.picker(None)?.nearest_attacker_of(target)
    }

    fn seen_unit(&mut self, id: ObjectId) -> Result<Option<crate::unit_ai::UnitSeen>, GenieError> {
        let Some(p) = self.c.w.spatial.get(id) else {
            return Ok(None);
        };
        let (owner, u) =
            crate::world::others::OthersRef::of(&self.c.w.players, &self.c.w.index, self.c.player)?
                .find(id)
                .ok_or(GenieError::unsupported("an object the slice does not hold"))?;
        let m = u.master(self.c.w.master_tables(), owner)?;
        let me = self.unit.master(self.c.w.master_tables(), self.player)?;
        Ok(Some(crate::unit_ai::UnitSeen {
            id,
            class: m.class,
            owner,
            combat_level: m.combat_level,
            state: u.life.state,
            target: self
                .c
                .w
                .parts
                .unit_ai
                .get(u.id)
                .and_then(|a| a.current_target),
            gap_squared: footprint_gap_squared(
                self.unit.position(),
                me.radius,
                p.position,
                p.radius,
            ),
            position: p.position,
            hp: u.life.hp,
            master: u.life.master_id,
            kind: m.kind,
            main: self.c.w.parts.building_of(u.id).annex_of,
            allied: crate::unit_ai::task_finder::allied(
                &self.c.w.players.player(self.player)?.relations,
                self.player,
                owner,
                true,
            ),
        }))
    }

    fn closest_resource(
        &mut self,
        search: crate::unit_ai::ResourceSearch,
    ) -> Result<Option<crate::unit_ai::Gatherable>, GenieError> {
        self.closest_gatherable(search)
    }

    /// The player's master table (`FUN_14152fe40`, `+0x28`): the masters the slice holds of each
    /// player.
    fn master_class(
        &mut self,
        player: PlayerId,
        master: MasterId,
    ) -> Result<Option<ClassId>, GenieError> {
        let class = self
            .c
            .w
            .masters
            .get(player.index())
            .and_then(|ms| ms.get(master))
            .map(|m| m.class);
        Ok(class)
    }

    fn reaches(&mut self, target: ObjectId, range: Distance) -> Result<bool, GenieError> {
        let mut plan = Planning {
            pathing: &mut self.c.w.pathing,
            now: self.c.w.world_time,
            computer: self.computer,
            object: self.c.w.streams.get(Stream::Object),
            groups: &self.c.w.groups,
        };
        let unit = &*self.unit;
        let walker = unit.me(self.actions);
        let mover = self
            .c
            .w
            .parts
            .mover_of(unit.id)
            .ok_or(GenieError::unsupported(
                "a reach test of a unit that does not move",
            ))?;
        let by_id = crate::world::index::ObjectsRef::new(
            &self.c.w.players,
            &self.c.w.index,
            &self.c.w.masters,
            &self.c.w.parts,
        );
        mover.reaches_object(
            (unit.position(), &walker),
            (&mut self.c.w.spatial, by_id),
            &mut plan,
            target,
            range,
            unit.life.master_id,
        )
    }

    fn closest(&mut self, search: &Search<'_>) -> Result<crate::unit_ai::Found, GenieError> {
        let seeker = match search.pick {
            crate::unit_ai::Pick::Attack { seeker, .. } => Some(seeker),
            _ => None,
        };
        let mut picker = WorldPicker {
            attacker: &self.attacker,
            seeker,
            me: self.unit.id,
            player: self.player,
            others: owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?,
            foreign: owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?
            .players(),
            every: crate::objects::masters::MasterTables::new(
                &self.c.w.masters,
                &self.c.w.parts.own_master,
            ),
            parts: &self.c.w.parts,
            by_id: crate::world::index::ObjectsRef::new(
                &self.c.w.players,
                &self.c.w.index,
                &self.c.w.masters,
                &self.c.w.parts,
            ),
            restriction: self
                .unit
                .master(self.c.w.master_tables(), self.player)?
                .terrain_restriction,
            unit: &*self.unit,
            attributes: &self.c.w.players.player(self.player)?.attributes,
            relations: &self.c.w.players.player(self.player)?.relations,
            ai_settings: self.ai_settings,
            now: self.c.w.world_time,
            mode_680: self.c.w.mode_680,
        };
        let (others, every) = (
            owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?,
            &self.c.w.masters,
        );
        // ClosestObject's reachability check (slot 0x3d0) walks the unit's paths
        let mut plan = Planning {
            pathing: &mut self.c.w.pathing,
            now: self.c.w.world_time,
            computer: self.computer,
            object: self.c.w.streams.get(Stream::Object),
            groups: &self.c.w.groups,
        };
        let unit = &*self.unit;
        let walker = unit.me(self.actions);
        let (mover, master_id, at) = (
            self.c.w.parts.mover_of(unit.id),
            unit.life.master_id,
            unit.position(),
        );
        let by_id = crate::world::index::ObjectsRef::new(
            &self.c.w.players,
            &self.c.w.index,
            &self.c.w.masters,
            &self.c.w.parts,
        );
        let mut reach = |spatial: &mut Spatial, id: ObjectId, range: Distance| {
            let m = mover.ok_or(GenieError::unsupported(
                "a reach test of a unit that does not move",
            ))?;
            m.reaches_object(
                (at, &walker),
                (spatial, by_id),
                &mut plan,
                id,
                range,
                master_id,
            )
        };
        let moving = |id: ObjectId| herded(by_id, id);
        // a class-10 searcher passes master 440 and the masters with object flag 0x10
        let shuns = |p: &Placed| {
            let m = player_master(every, p.player, p.master).ok_or(GenieError::unsupported(
                "a found object of a master the slice does not have",
            ))?;
            Ok(p.master == PETARD || m.object_flags.contains(ObjectFlags::SCOUT))
        };
        // a farm's workers (`+0x644`) and its pasture (`+0x790`)
        let farm = |id: ObjectId| others.get(id).map(|u| (u.workers, u.pasture));
        self.c.w.spatial.search(SearchQuery {
            searcher: unit.id,
            player: self.player,
            search,
            attack: &mut picker,
            reach: &mut reach,
            moving: &moving,
            shuns: &shuns,
            farm: &farm,
        })
    }

    fn may_attack(
        &mut self,
        target: ObjectId,
        seeker: &crate::unit_ai::Seeker,
    ) -> Result<Option<(ClassId, Position)>, GenieError> {
        if !self.view_holds(target)? {
            return Ok(None);
        }
        self.attack_check(target, seeker)
    }

    fn attack(
        &mut self,
        target: ObjectId,
        force: bool,
        ai: &UnitAi,
    ) -> Result<crate::unit_ai::Attacked, GenieError> {
        self.start_attack_now(target, force, ai)
    }

    fn set_under_attack(&mut self, value: bool) {
        self.unit.under_attack = value;
    }

    /// The unit's call its unit AI makes, at once ([`Unit::run_effect`]): the notifies held for
    /// the AI are posted to it first (as the game posted them where they happened), then the
    /// call runs on the unit holding the AI, with the AI stream the world's for it.
    ///
    /// [`Unit::run_effect`]: crate::objects::unit::Unit::run_effect
    fn effect_now(
        &mut self,
        e: crate::unit_ai::Effect,
        ai: &mut UnitAi,
        rng: &mut Well512,
    ) -> Result<(), GenieError> {
        let held = self.c.side_notifies.take().unwrap_or_default();
        let posted = held
            .into_iter()
            .try_for_each(|(n, state)| ai.post(n, state));
        self.c.w.return_unit_ai(self.unit.id, std::mem::take(ai));
        std::mem::swap(self.c.w.streams.get(Stream::Ai), rng);
        let class = self
            .unit
            .master(self.c.w.master_tables(), self.c.player)
            .map(|m| m.class);
        let done = class.and_then(|class| {
            posted.and_then(|()| self.unit.run_effect(self.actions, e, Some(class), self.c))
        });
        std::mem::swap(self.c.w.streams.get(Stream::Ai), rng);
        self.c.side_notifies = Some(Vec::new());
        *ai = self.c.w.lend_unit_ai(self.unit.id).ok_or_else(|| {
            GenieError::unreachable("a call of the unit AI took the unit's AI")
                .with("object", self.unit.id)
        })?;
        done
    }

    fn busy_now(&mut self) -> Result<bool, GenieError> {
        Ok(self.actions.top_action_kind().is_some() || self.actions.unmodelled_action.is_some())
    }

    fn interruptible_now(&mut self) -> Result<bool, GenieError> {
        self.actions.interruptible()
    }

    fn seek_unexplored(
        &mut self,
        ai: &UnitAi,
        rng: &mut Well512,
    ) -> Result<Option<Tile>, GenieError> {
        let tc = town_centre(owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?)?;
        let (at, computer) = (self.unit.position(), self.computer);
        // the search about the town centre only while the player has no scout map
        let scout_map = self.c.w.spatial.scout_map(self.player).is_some();
        let (mut area, map, line_of_sight) = self.scout_area()?;
        if !scout_map
            && let Some(p) = crate::movement::explore::unexplored_area(tc, map, &mut area)?
        {
            return Ok(Some(p));
        }
        // none about the town centre: the unit AI's own search (slot 0x90)
        let seeker = crate::movement::explore::Seeker {
            at,
            line_of_sight,
            order: ai.current_order,
            target_point: ai.target_point.ground(),
            computer,
        };
        crate::movement::explore::closest_undiscovered(&mut area, seeker, map, rng)
    }

    fn owner_hears(&mut self, n: crate::unit_ai::Notify) -> Result<(), GenieError> {
        self.hears(n)
    }

    fn seek_beyond_sight(&mut self, rng: &mut Well512) -> Result<Option<Tile>, GenieError> {
        let (at, player) = (self.unit.position(), self.player);
        let scouted = self.c.w.spatial.scout_map(player).is_some();
        let (mut area, map, line_of_sight) = self.scout_area()?;
        area.scouted = scouted;
        let found = crate::movement::explore::beyond_sight(&mut area, at, line_of_sight, map, rng)?;
        // the point found is marked in the scout map (FUN_140f4d5e0 with 1)
        if let Some(t) = found {
            self.c.w.spatial.scout(player, t);
        }
        Ok(found)
    }

    /// The player's scout map (`FUN_140c589c0`'s case 0x202 when the auto scout found nothing):
    /// a new `InfluenceMap` of 480 by 480 (`FUN_140f4a240`, the old one goes, `+0x1368` 1) marks
    /// every tile the player sees now (its visible map's `FUN_141285350` 0xf) with 1.
    fn make_scout_map(&mut self) -> Result<(), GenieError> {
        let size = crate::movement::explore::SCOUT_MAP_SIZE;
        let mut m = crate::movement::explore::InfluenceMap::new(GridSize::new(size, size), 0);
        let (w, h) = (self.c.w.spatial.map.width(), self.c.w.spatial.map.height());
        for y in 0..h {
            for x in 0..w {
                if self
                    .c
                    .w
                    .spatial
                    .scout_sight(self.player, Tile::new(x, y))
                    .is_some_and(crate::map::spatial::TileSight::seen)
                {
                    m.set(Tile::new(x, y), 1);
                }
            }
        }
        self.c.w.spatial.set_scout_map(self.player, m);
        Ok(())
    }

    /// The unit's slot 0x388 (`FUN_141226fc0`) on the foundation `target`: the world's object
    /// (`FUN_1415417a0`; the owner's sleeping list, which the unit does not see here, holds
    /// only objects of a type below 30, never a building), then its slot 0x820 with any action
    /// type (`0xffff`) and the switch ([`task_search`] on the target and its main building);
    /// with a task the new wait replaces the unit's actions, which the caller makes after the
    /// update ([`Wait`], [`World::wait_for_foundation`]).
    fn command(&mut self, command: crate::unit_ai::UnitCommand) -> Result<bool, GenieError> {
        match command {
            crate::unit_ai::UnitCommand::Repair(target) => self.repair_command(target),
            crate::unit_ai::UnitCommand::Enter(target) => self.enter_command(target),
            // slot 0x3a8 (FUN_14108d050): its Transport, made at once, answers 1
            crate::unit_ai::UnitCommand::Unload {
                to,
                over_wall: false,
            } => {
                self.c
                    .lend_at((&mut *self.unit, &mut *self.actions), |w, p, i| {
                        w.transport_order(p, i, to)
                    })?;
                Ok(true)
            }
            _ => Err(crate::unit_ai::UNMODELLED_COMMAND),
        }
    }

    fn builds(&self, foundation: crate::construction::FoundationId) -> Result<bool, GenieError> {
        self.builds_on(foundation)
    }

    fn wait_solidify(&mut self, target: ObjectId) -> Result<bool, GenieError> {
        self.start_wait_now(target)
    }

    /// The soldier's `AttackObject` in a group (`FUN_140c17550`): the unit's future path data
    /// (slot 0x550), which the slice holds none of, goes; then a unit that moves (slot 0x5c8) or
    /// speeds up (slot 0x5a0: its forward acceleration above 0, `COMISS`/`JBE`) stops (slot
    /// 0x5d0).
    fn group_attack_stop(&mut self) -> Result<(), GenieError> {
        let Some(m) = self.c.w.parts.mover_mut(self.unit.id) else {
            return Ok(());
        };
        let speeding = m
            .velocity
            .as_ref()
            .is_some_and(|v| 0.0 < v.acceleration.y());
        if m.is_moving() || speeding {
            m.stop()?;
        }
        Ok(())
    }

    /// Slot 400 at the full update's tail: the top action's target ([`Unit::top_target`]), the
    /// actions the AI gave in its run made already (a wait for a foundation and a Transport hold
    /// none), while the world holds it.
    ///
    /// [`Unit::top_target`]: crate::objects::unit::Unit::top_target
    fn top_target_now(&mut self) -> Result<Option<ObjectId>, GenieError> {
        let target = self.actions.top_target();
        Ok(target.filter(|&t| self.c.w.spatial.get(t).is_some()))
    }

    /// The object as the full update's tail reads it, found as [`UnitSide::seen_unit`] finds
    /// it.
    fn target_flags(
        &mut self,
        id: ObjectId,
    ) -> Result<Option<crate::unit_ai::TargetFlags>, GenieError> {
        let Some(seen) = self.seen_unit(id)? else {
            return Ok(None);
        };
        let u = owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?
        .get(id)
        .ok_or_else(|| object_gone(id))?;
        let kind = u.master(self.c.w.master_tables(), seen.owner)?.kind;
        Ok(Some(crate::unit_ai::TargetFlags {
            owner: seen.owner,
            kind,
            capture: self.c.w.parts.combat_of(u.id).capture_flag,
            pasture: u.pasture,
            untargetable: u.untargetable,
            attackable: u.attackable,
        }))
    }
}
