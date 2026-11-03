//! BUILD's tests before its foundation: a farm on rice terrain, the pending debit, the
//! builders, the placement test and the payment.

use crate::GenieError;
use crate::map::Point;
use crate::map::Position;
use crate::map::Restriction;
use crate::map::Tile;
use crate::map::spatial::Placed;
use crate::map::terrain::is_rice_terrain;
use crate::objects::ObjectId;
use crate::objects::masters::HillMode;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::cost_entries;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::production::spawn::PlacementCode;
use crate::production::spawn::SiteChecks;
use crate::production::spawn::{BuildSite, WorldMap, build_placement};
use crate::world::World;
use crate::world::layer::MasterRoles;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;

impl World {
    /// BUILD's first step (`FUN_140e7a400`): a farm (master 50) whose point's tile (truncated) is
    /// mangrove shallows or forest (terrains 54, 55, `FUN_1413fc100`) or a rice farm's (63 to 67,
    /// `FUN_1413fc220`) is a rice farm (1187); a point off the map keeps the farm.
    pub(crate) fn farm_on_rice_terrain(&self, master: MasterId, position: Position) -> MasterId {
        if !self
            .model
            .roles
            .of_master(master)
            .contains(MasterRoles::FARM)
        {
            return master;
        }
        let tile = Tile::new(position.x() as i32, position.y() as i32);
        let terrain = self.spatial.terrain.as_ref().and_then(|t| t.kind(tile));
        if terrain.is_some_and(is_rice_terrain) {
            self.model.slots.master.rice_farm
        } else {
            master
        }
    }

    /// BUILD's pending debit (`FUN_141630d10` with 0, before its placement test): every cost
    /// entry of the master, paid or not, by resource, comes off the player's pending debit.
    pub(crate) fn debit_listed(
        &mut self,
        player: PlayerId,
        master: MasterId,
    ) -> Result<(), GenieError> {
        let site = player_master(&self.model.masters, player, master).ok_or(
            GenieError::unsupported("BUILD of a master the player does not have"),
        )?;
        let mut listed = [0i16; 4];
        for (resource, amount, _) in cost_entries(&site.creatable_part().cost) {
            if let Some(r) = resource.is_resource().then(|| resource.index()) {
                let entry = listed.checked_mut(r)?;
                *entry = entry.wrapping_add(amount.get());
            }
        }
        for (debit, amount) in self
            .players
            .player_mut(player)?
            .pending_debit
            .iter_mut()
            .zip(listed)
        {
            *debit -= Amount::new(f32::from(amount));
        }
        Ok(())
    }

    /// BUILD's builders' test (`FUN_140e7a400` after the pending debit): a human player's BUILD
    /// needs one unit of the packet that can build the master ([`World::builds`]; one the world
    /// holds, `FUN_140e6bad0`, follows from it), else it places nothing; a computer's passes.
    pub(crate) fn build_has_builder(
        &self,
        player: PlayerId,
        master: MasterId,
        builders: &[Option<ObjectId>],
    ) -> Result<bool, GenieError> {
        if self.players.player(player)?.kind.is_computer() {
            return Ok(true);
        }
        let trained_at = self
            .master(player, master)?
            .creatable_part()
            .trained_at
            .clone();
        for &id in builders.iter().flatten() {
            if self.builds((master, &trained_at), id)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether BUILD (`FUN_140e7a400`) places a foundation of `master` at `at` for `player`: its
    /// placement test ([`crate::production::spawn::build_placement`], the master's slot 0x50), then the
    /// payment's (`FUN_14162fb60` -> `FUN_141618db0`: every cost entry of a resource, paid or
    /// not, at most what the player holds). A refused BUILD places, pays and draws nothing (the
    /// player's slot 0x380 shows why). A cost in population (attribute 4) is not modelled.
    pub(crate) fn build_allowed(
        &self,
        player: PlayerId,
        master: MasterId,
        at: Point,
    ) -> Result<bool, GenieError> {
        let human = !self.players.player(player)?.kind.is_computer();
        if !self.placement_code(player, master, at, human)?.is_allowed() {
            return Ok(false);
        }
        self.affords(player, master)
    }

    /// The master's placement test (slot 0x50, [`crate::production::spawn::build_placement`]) of a
    /// foundation of `master` for `player` at `at`, `human` its explored test (`p6`): 0 when it
    /// may go there, else the game's code.
    pub(crate) fn placement_code(
        &self,
        player: PlayerId,
        master: MasterId,
        at: Point,
        human: bool,
    ) -> Result<PlacementCode, GenieError> {
        self.placement_code_beside(player, master, at, human, &[])
    }

    /// [`World::placement_code`] whose scan passes over the objects `own` (the test's `p12` and
    /// its annexes).
    pub(crate) fn placement_code_beside(
        &self,
        player: PlayerId,
        master: MasterId,
        at: Point,
        human: bool,
        own: &[ObjectId],
    ) -> Result<PlacementCode, GenieError> {
        let site = self.master(player, master)?;
        let build = BuildSite {
            player,
            checks: if human {
                SiteChecks::HUMAN
            } else {
                SiteChecks::empty()
            },
            radius: site.clearance,
            tall: site.is_tall(),
            class: site.class,
            obstructs: site.obstruction_kind.recorded(),
            restriction: self.site_restriction(site)?,
            hill_mode: site.hill_mode,
            placement: site.placement,
            own,
        };
        let map = WorldMap { turns: self };
        build_placement(
            &build,
            &map,
            at,
            &self.model.roles,
            &self.model.tuning.spawn,
        )
    }

    /// The Build's placement test of the building `placed` (`0x140aae200` case 7: its master's slot
    /// 0x50, `FUN_14123aee0`, with `p6` 0, `p7` 0, `p8` 0, `p9` 0, `p10` 1, `p11` 1, the building
    /// as `p12`, `p13` 1, `p14` 1): by the master's size radii, without the explored test and the
    /// hill mode, with the static scans
    /// ([`SiteChecks::SCANS`](crate::production::spawn::SiteChecks::SCANS)); the scans pass over
    /// `own` (the building and its annexes); the updating builder must be lent to its slot for them
    /// ([`crate::world::step::Step::lend`]). A `desolid` building's master is its copy without a
    /// height (`FUN_1411cf770` writes `+0x5e0` 0), so the static scans do not run for it. 0 when
    /// the building may stay, else the game's code.
    ///
    /// # Errors
    ///
    /// A master the player lacks, and what the test does not model.
    pub(crate) fn foundation_code(
        &self,
        placed: &Placed,
        desolid: bool,
        own: &[ObjectId],
    ) -> Result<PlacementCode, GenieError> {
        let player = placed.player;
        let site = self.master(player, placed.master)?;
        let test = BuildSite {
            player,
            checks: SiteChecks::SCANS,
            radius: site.radius,
            tall: site.is_tall() && !desolid,
            class: site.class,
            obstructs: site.obstruction_kind.recorded(),
            restriction: self.site_restriction(site)?,
            hill_mode: HillMode::ANY,
            placement: site.placement,
            own,
        };
        let map = WorldMap { turns: self };
        let at = placed.position.ground();
        build_placement(&test, &map, at, &self.model.roles, &self.model.tuning.spawn)
    }

    /// The terrain restriction a placement test of `site` reads (`+0x606`), from the data file.
    ///
    /// # Errors
    ///
    /// A world without the players' sight, and a restriction the data file does not have.
    fn site_restriction(&self, site: &Master) -> Result<&Restriction, GenieError> {
        if self.spatial.sight.is_none() {
            return Err(GenieError::unsupported(
                "BUILD's placement test without the players' sight",
            ));
        }
        self.spatial
            .terrain
            .as_ref()
            .and_then(|t| site.terrain_restriction.and_then(|r| t.restriction(r)))
            .ok_or(GenieError::unsupported(
                "a foundation's terrain restriction the data file does not have",
            ))
    }

    /// The payment's test of a foundation of `master` for `player` (`FUN_14162fb60` ->
    /// `FUN_141618db0`): every cost entry of a resource, paid or not, at most what the player
    /// holds. A cost in population (attribute 4) is not modelled.
    pub(crate) fn affords(&self, player: PlayerId, master: MasterId) -> Result<bool, GenieError> {
        let site = self.master(player, master)?;
        let owner = self.players.player(player)?;
        for (resource, amount, _) in cost_entries(&site.creatable_part().cost) {
            let r = resource.index();
            if r == 4 {
                return Err(GenieError::unsupported(
                    "a BUILD whose cost lists the population",
                ));
            }
            let held = owner.attributes.get(r).copied().unwrap_or(Amount::ZERO);
            if held < Amount::new(amount.as_f32()) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
