//! The unit AI's searches through its view: the objects-in-sight registry's search with its pick
//! ([`Search`]), the attack test of a named object, the resource, foundation and drop-site
//! searches, the spot off a footprint, the call for help, and the auto scout's searches with its
//! scout map. The long answers live in their systems as `UnitView` methods (FD-041-D09): the
//! foundation searches in `construction/build/unit_ai_side.rs` and the resource search in
//! `gathering/closest_resource.rs`.

use crate::GenieResult;
use crate::construction::FoundationId;
use crate::gathering::DropSiteId;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::TileSight;
use crate::map::spatial::{SearchQuery, Spatial};
use crate::math::rng::{Stream, Well512};
use crate::movement::SpeedReads;
use crate::movement::explore::{
    InfluenceMap, SCOUT_MAP_SIZE, Seeker, beyond_sight, closest_undiscovered, unexplored_area,
};
use crate::movement::{Planning, footprint_gap_squared};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::ClassMask;
use crate::objects::masters::MasterTables;
use crate::orders::nearest_table_site;
use crate::players::PlayerId;
use crate::players::UnitRelation;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{Helper, UnitView, WorldPicker, town_centre};
use crate::unit_ai::{
    Found, Gatherable, OffFootprint, Pick, ResourceSearch, Seeker as UnitAiSeeker, helper_answers,
};
use crate::unit_ai::{Search, UnitAi};
use crate::vision::registry::NO_LAST_ANSWER;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::objects_ref::ObjectsRef;
use crate::world::slots::PlayerSlotsExt;

impl UnitView<'_, '_> {
    /// A search (the unit AI's `FUN_140c63e20` with its pick) over the objects-in-sight
    /// registry: the attack pick through the world as `bestUnitToAttack` reads it
    /// ([`WorldPicker`]), the reachability check (slot 0x3d0) on the unit's paths.
    ///
    /// # Errors
    ///
    /// What the search does not model.
    pub(crate) fn closest(&mut self, search: &Search<'_>) -> GenieResult<Found> {
        let roles = &self.step.world.model.roles.clone();
        let top_speed =
            self.step
                .world
                .top_speed_terms_with(self.player, self.unit, search.ai_target)?;
        let seeker = match search.pick {
            Pick::Attack { seeker, .. } => Some(seeker),
            _ => None,
        };
        let picker = WorldPicker {
            attacker: &self.attacker,
            seeker,
            me: self.unit.id,
            ai_target: search.ai_target,
            player: self.player,
            others: owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?,
            foreign: owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?
            .players(),
            every: MasterTables::new(
                &self.step.world.model.masters,
                &self.step.world.parts.own_master,
            ),
            parts: &self.step.world.parts,
            speeds: SpeedReads::of(
                &self.step.world.players,
                &self.step.world.index,
                &self.step.world.parts,
                &self.step.world.groups,
                &self.step.world.model,
            ),
            by_id: ObjectsRef::new(
                &self.step.world.players,
                &self.step.world.index,
                &self.step.world.model.masters,
                &self.step.world.parts,
                &self.step.world.model.roles,
                &self.step.world.model.tuning,
            ),
            restriction: self
                .unit
                .master(self.step.world.master_tables(), self.player)?
                .terrain_restriction,
            unit: &*self.unit,
            actions: &*self.actions,
            attributes: &self.step.world.players.player(self.player)?.attributes,
            relations: &self.step.world.players.player(self.player)?.relations,
            ai_settings: self.ai_settings,
            now: self.step.world.world_time,
            mode_680: self.step.world.mode_680,
        };
        let closest = self.step.world.closest_world();
        // ClosestObject's reachability check (slot 0x3d0) walks the unit's paths
        let mut plan = Planning {
            pathing: &mut self.step.world.pathing,
            now: self.step.world.world_time,
            player_kind: self.player_kind,
            object: self.step.world.streams.get_mut(Stream::Object),
            groups: &self.step.world.groups,
        };
        let unit = &*self.unit;
        let walker = unit.me(self.actions, top_speed);
        let (mover, master_id, at) = (
            self.step.world.parts.mover_of(unit.id),
            unit.life.master_id,
            unit.position(),
        );
        let by_id = ObjectsRef::new(
            &self.step.world.players,
            &self.step.world.index,
            &self.step.world.model.masters,
            &self.step.world.parts,
            &self.step.world.model.roles,
            &self.step.world.model.tuning,
        );
        let mut reach = |spatial: &mut Spatial, id: ObjectId, range: Distance| {
            // slot 0x3d0 of an object that does not move (types 10 to 25) is
            // `FUN_141071b50`'s 0
            let Some(mover) = mover else {
                return Ok(false);
            };
            mover.reaches_object(
                (at, &walker),
                (spatial, by_id),
                &mut plan,
                id,
                range,
                master_id,
            )
        };
        self.step.world.spatial.search(
            SearchQuery {
                searcher: unit.id,
                player: self.player,
                search,
                attack: Some(&picker),
                reach: &mut reach,
                objects: by_id,
                closest,
            },
            roles,
            &self.step.world.model.tuning.best_target,
            &self.step.world.model.tuning.spatial,
            self.step.world.model.tuning.registry,
        )
    }

    /// `AttackObject`'s test of object `target` (`FUN_140c43720`): the world holds it and the unit
    /// AI's attack check (slot 0xa0, [`can_attack`], as `seeker` stands) passes; then its master's
    /// class and its place.
    ///
    /// # Errors
    ///
    /// What the attack check does not model.
    ///
    /// [`can_attack`]: crate::unit_ai::can_attack
    pub(crate) fn may_attack(
        &self,
        target: ObjectId,
        seeker: &UnitAiSeeker,
    ) -> GenieResult<Option<(ClassId, Position)>> {
        let roles = &self.step.world.model.roles.clone();
        if !self.view_holds(target)? {
            return Ok(None);
        }
        self.attack_check(target, seeker, roles)
    }

    /// The unit AI's `ClosestResourceObject` (slot 0x80, `FUN_140c49760`) as `search` asks it:
    /// what it found, None for nothing.
    ///
    /// # Errors
    ///
    /// What the search does not model.
    pub(crate) fn closest_resource(
        &mut self,
        search: ResourceSearch,
    ) -> GenieResult<Option<Gatherable>> {
        self.closest_gatherable(search)
    }

    /// The builder's search for another foundation (`FUN_140c1c7c0`), None when it finds none.
    ///
    /// # Errors
    ///
    /// What the search does not model.
    pub(crate) fn foundation_search(&self) -> GenieResult<Option<ObjectId>> {
        self.own_foundation_near()
    }

    /// `FUN_141658e20` from the unit's place: the drop-site table's nearest building of the
    /// owner for what the unit carries (`+0x308`) from its class, with its place; None without
    /// one.
    ///
    /// # Errors
    ///
    /// A unit or an owner the world does not hold.
    pub(crate) fn table_drop_site(&self) -> GenieResult<Option<(DropSiteId, Position)>> {
        let class = self
            .unit
            .master(self.step.world.master_tables(), self.player)?
            .class;
        let at = self.unit.position().ground();
        let view = owner_view(
            &self.step.world.players,
            &self.step.world.index,
            self.step.player,
            self.new_owner,
        )?;
        let look = |id: ObjectId| view.own(id).map(|unit| (unit.life.state, unit.position()));
        Ok(nearest_table_site(
            (
                &self.step.world.model.drop_sites,
                self.civ,
                &self.step.world.players.player(self.step.player)?.tracker,
            ),
            (at, self.unit.life.attribute_kind, class),
            look,
            &self.step.world.model.tuning.order,
        ))
    }

    /// What the base handler's case 0x216 reads ([`OffFootprint`]) for a unit sent off the
    /// footprint of `site` (its master's radii `radius`).
    ///
    /// # Errors
    ///
    /// What the spot's search does not model.
    pub(crate) fn off_footprint(
        &mut self,
        site: FoundationId,
        radius: Footprint,
    ) -> GenieResult<OffFootprint> {
        self.spot_off_footprint(site, radius)
    }

    /// `AskForHelp`'s walk (`FUN_140c42ce0`) of the lists its search left of the unit's player's
    /// own objects and its friends' (`DAT_143f69180`, the relation kinds 1 and 2, each in its
    /// order) for helpers against `attacker`: the unit itself, an object without a unit AI or
    /// owner, a dead one (state 3 and up), one of the classes 1 to 3, 18 to 21, 27 and 39
    /// (`0x80083c000e`), the siege tower master 1105 (`FUN_141194f10`), a villager (class 4)
    /// against an attacker that is not Gaia's, one of a player that does not count the unit's
    /// as an ally (`FUN_141663d90` with 1) and one farther than 3 tiles (the squared footprint
    /// gap, `FUN_14118f8c0`, above 9) do not help. These tests read nothing the helper's attack
    /// check (slot 0xa0) between them changes.
    ///
    /// # Errors
    ///
    /// A search that read no lists yet, and a helper that passes them ([`helper_answers`]).
    pub(crate) fn call_for_help(
        &self,
        attacker: ObjectId,
        ai_target: Option<ObjectId>,
    ) -> GenieResult<()> {
        /// The classes that never help: 1 to 3, 18 to 21, 27 and 39.
        const NO_HELP_CLASSES: ClassMask = ClassMask::new(0x0080_083c_000e, 0x28);

        let roles = &self.step.world.model.roles.clone();
        let lists = self.step.world.spatial.searched().ok_or(NO_LAST_ANSWER)?;
        let others = owner_view(
            &self.step.world.players,
            &self.step.world.index,
            self.step.player,
            self.new_owner,
        )?;
        let find = |id: ObjectId| others.find(id);
        let gaia_attacker = find(attacker).is_some_and(|(owner, _)| owner == PlayerId::GAIA);
        let me = self.unit.id;
        let my_radius = self
            .unit
            .master(self.step.world.master_tables(), self.player)?
            .radius;
        for record in lists[UnitRelation::Own]
            .iter()
            .chain(&lists[UnitRelation::Ally])
        {
            if record.id == me {
                continue;
            }
            let Some((owner, unit)) = find(record.id) else {
                continue;
            };
            if !self.step.world.parts.unit_ai.contains(unit.id)
                || ObjectState::Dying <= unit.life.state
            {
                continue;
            }
            let master = unit.master(self.step.world.master_tables(), owner)?;
            let no_help = NO_HELP_CLASSES.contains(master.class);
            if no_help
                || roles
                    .of_master(master.id)
                    .contains(MasterRoles::SIEGE_TOWER)
            {
                continue;
            }
            if !gaia_attacker && roles.of_class(master.class).contains(ClassRoles::VILLAGER) {
                continue;
            }
            if owner != self.player && !self.step.world.spatial.allied_with(owner, self.player) {
                continue;
            }
            let gap = footprint_gap_squared(
                self.unit.position(),
                my_radius,
                unit.position(),
                master.radius,
            );
            if DistanceSquared::new(self.step.world.model.tuning.unit_ai.help_reach) < gap {
                continue;
            }
            let helper = Helper {
                owner,
                unit,
                actions: self.step.world.parts.actions_of(unit.id),
                master,
            };
            if !self.helper_free(helper, attacker, ai_target, roles)? {
                continue;
            }
            return helper_answers();
        }
        Ok(())
    }

    /// The explorer's point to explore (the base handler's case 0x202 for the auto scout): the
    /// search around its town centre (`FUN_140c65490`), else its unit AI's (`ai`) slot 0x90 with
    /// a draw of the AI stream (`rng`); None when neither finds one.
    ///
    /// # Errors
    ///
    /// What the searches do not model.
    pub(crate) fn seek_unexplored(
        &mut self,
        ai: &UnitAi,
        rng: &mut Well512,
    ) -> GenieResult<Option<Tile>> {
        let tc = town_centre(
            owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?,
            self.step.world.model.slots.master,
        )?;
        let (at, player_kind) = (self.unit.position(), self.player_kind);
        // the search about the town centre only while the player has no scout map
        let scout_map = self.step.world.spatial.scout_map(self.player).is_some();
        let (mut area, map, line_of_sight) = self.scout_area(ai.current_target)?;
        if !scout_map && let Some(tile) = unexplored_area(tc, map, &mut area)? {
            return Ok(Some(tile));
        }
        // none about the town centre: the unit AI's own search (slot 0x90)
        let seeker = Seeker {
            at,
            line_of_sight,
            order: ai.current_order,
            target_point: ai.target_point.ground(),
            player_kind,
        };
        closest_undiscovered(&mut area, seeker, map, rng)
    }

    /// The auto scout's last search (`FUN_140c4b130`, [`beyond_sight`]), drawing from the AI
    /// stream (`rng`), its unit AI's target `ai_target`; None when it finds no point. The point
    /// found is marked in the scout map.
    ///
    /// # Errors
    ///
    /// What the search does not model.
    pub(crate) fn seek_beyond_sight(
        &mut self,
        rng: &mut Well512,
        ai_target: Option<ObjectId>,
    ) -> GenieResult<Option<Tile>> {
        let (at, player) = (self.unit.position(), self.player);
        let scouted = self.step.world.spatial.scout_map(player).is_some();
        let (mut area, map, line_of_sight) = self.scout_area(ai_target)?;
        area.scouted = scouted;
        let found = beyond_sight(&mut area, at, line_of_sight, map, rng)?;
        // the point found is marked in the scout map (FUN_140f4d5e0 with 1)
        if let Some(tile) = found {
            self.step.world.spatial.scout(player, tile);
        }
        Ok(found)
    }

    /// The player's scout map, made when the auto scout's last search found nothing
    /// (`FUN_140c589c0`'s case 0x202): a new `InfluenceMap` of 480 by 480 (`FUN_140f4a240`, the
    /// old one goes, `+0x1368` 1) marks every tile the player sees now (its visible map's
    /// `FUN_141285350` 0xf) with 1.
    pub(crate) fn make_scout_map(&mut self) {
        let size = SCOUT_MAP_SIZE;
        let mut influence_map = InfluenceMap::new(GridSize::new(size, size), 0);
        let (width, height) = (
            self.step.world.spatial.map.width(),
            self.step.world.spatial.map.height(),
        );
        for y in 0..height {
            for x in 0..width {
                if self
                    .step
                    .world
                    .spatial
                    .scout_sight(self.player, Tile::new(x, y))
                    .is_some_and(TileSight::seen)
                {
                    influence_map.set(Tile::new(x, y), 1);
                }
            }
        }
        self.step
            .world
            .spatial
            .set_scout_map(self.player, influence_map);
    }
}
