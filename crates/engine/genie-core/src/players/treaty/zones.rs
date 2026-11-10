//! The treaty monitor's zones: their walk over the players' town centres (`FUN_141461c00`), a
//! town centre's sweep of the foundations about it (`FUN_1411c7ae0`) and the no-build test
//! (`FUN_141461400`).

use crate::GenieResult;
use crate::construction::build::NO_CONSTRUCTION;
use crate::construction::build::NOT_HELD;
use crate::map::Footprint;
use crate::map::GridSize;
use crate::map::Point;
use crate::map::Tile;
use crate::map::TileRect;
use crate::map::WholeDistance;
use crate::math::conversions::cvttss2si;
use crate::math::image::treaty::ONE;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::unit::Unit;
use crate::players::AllyTest;
use crate::players::PlayerId;
use crate::players::treaty::Borders;
use crate::players::treaty::GameScreen;
use crate::players::treaty::TreatyMonitor;
use crate::unit_ai::task_finder::allied;
use crate::world::GameMode;
use crate::world::Player;
use crate::world::Seconds;
use crate::world::World;

/// The tiles past a zone on each side that a town centre's sweep walks (`FUN_1411c7ae0`'s
/// `SUB R12D, 5`, `ADD EDX, 5`, `ADD EAX, -5`, `ADD ECX, 5`).
const SWEEP_REACH: i32 = 5;

/// A town centre whose zone the walk draws, as the walk and its sweep read it: its id and its
/// owner (`+0x138`'s `+0x718`), its place (`+0x3c8`, `+0x4c8`), its state (`+0x640`) and its
/// master's clearance (`+0x5fc`, `+0x600`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct ZoneCentre {
    /// The town centre.
    id: ObjectId,
    /// Its owner.
    owner: PlayerId,
    /// Its place on the ground.
    at: Point,
    /// Its state.
    state: ObjectState,
    /// Its master's clearance.
    clearance: Footprint,
}

impl ZoneCentre {
    /// Player `owner`'s town centre `unit` of master `master`.
    const fn of(owner: PlayerId, unit: &Unit, master: &Master) -> Self {
        Self {
            id: unit.id,
            owner,
            at: unit.position().ground(),
            state: unit.life.state,
            clearance: master.clearance,
        }
    }
}

/// The zone about a town centre at `at` with `margin` on a map of `map`'s tiles
/// (`FUN_141461c00`). On each axis it runs from the centre's tile ([`Point::tile`], a NaN's
/// `i32::MIN`) less the margin, at least 0 (`CMOVG`), to that tile plus the margin less 1 while
/// that lies below the map's last tile (`CMOVL`, signed), else to the map's last tile; the int
/// arithmetic wraps.
pub(crate) fn zone_about(at: Point, margin: WholeDistance, map: GridSize) -> TileRect {
    let [x, y] = at.tile().get();
    let margin = margin.get();
    TileRect::new(
        x.wrapping_sub(margin).max(0),
        y.wrapping_sub(margin).max(0),
        x.wrapping_add(margin)
            .wrapping_sub(1)
            .min(map.width().wrapping_sub(1)),
        y.wrapping_add(margin)
            .wrapping_sub(1)
            .min(map.height().wrapping_sub(1)),
    )
}

impl TreatyMonitor {
    /// `FUN_141461400` (HD's `TreatyMode::IsWithinNoBuildZone`): whether a footprint of player
    /// `owner` at `at` with radii `radius` overlaps a zone ([`TileRect::overlaps`]) of a player
    /// past Gaia whom `players` shows not allied with it (`FUN_141663dc0` with its flag 0; in
    /// game mode 7 every other player). Its tiles run from the place less the radii to the place
    /// plus the radii less 1 (`CVTTSS2SI`). A player's first zone counts only where its low
    /// corner lies below its high one on both axes; its second, then, only once the second zones
    /// are on.
    #[must_use]
    pub(crate) fn in_no_build_zone(
        &self,
        players: &[Player],
        game_mode: GameMode,
        owner: PlayerId,
        at: Point,
        radius: Footprint,
    ) -> bool {
        let footprint = TileRect::new(
            cvttss2si(at.x() - radius.x()),
            cvttss2si(at.y() - radius.y()),
            cvttss2si(at.x() + radius.x() - ONE),
            cvttss2si(at.y() + radius.y() - ONE),
        );
        PlayerId::each(players.len())
            .zip(players)
            .skip(1)
            .zip(&self.zones)
            .any(|((player_id, player), zone)| {
                let exempt = if game_mode == GameMode::DefendTheWonder {
                    player_id == owner
                } else {
                    // the ally test reads the world's monitor (`world+0x820`) again: this one,
                    // whose test runs
                    allied(
                        &player.relations,
                        Some(self),
                        player_id,
                        owner,
                        AllyTest::AlliesOnly,
                    )
                };
                let first = zone.first;
                !exempt
                    && first.x0() < first.x1()
                    && first.y0() < first.y1()
                    && (first.overlaps(footprint)
                        || (self.second_zones && zone.second.overlaps(footprint)))
            })
    }
}

impl World {
    /// `FUN_141461c00` (HD's `TreatyMode::AttemptToDrawBorders`) on the game's screen `screen`:
    /// without it none is drawn. Else for each player past Gaia its first town centre
    /// ([`World::town_centre_unit_at`]), or, while it has none (the zones then stand not for
    /// every player), its first foundation of one ([`World::town_centre_foundation`]) that has
    /// had work (`COMISS` and `JNC`: a NaN has); that town centre's zones are drawn
    /// ([`World::draw_player_zones`]). The zones stand when every player had a town centre. The
    /// interface's list of the drawn zones (`FUN_1414d5cd0`, its flag) is not simulation.
    ///
    /// # Errors
    ///
    /// A world without its monitor, a town centre of a master the tables lack, and the
    /// sweep's.
    pub(crate) fn draw_treaty_zones(&mut self, screen: GameScreen) -> GenieResult<Borders> {
        if screen == GameScreen::Absent {
            return Ok(Borders::Undrawn);
        }
        let mut borders = Borders::Drawn;
        for player in PlayerId::each(self.players.len()).skip(1) {
            let centre = if let Some(unit) = self.town_centre_unit_at(player, 0) {
                let master = unit.master(self.master_tables(), player)?;
                Some(ZoneCentre::of(player, unit, master))
            } else {
                borders = Borders::Undrawn;
                self.begun_town_centre(player)?
            };
            if let Some(centre) = centre {
                self.draw_player_zones(centre)?;
            }
        }
        Ok(borders)
    }

    /// Player `player`'s first town centre foundation ([`World::town_centre_foundation`]) that
    /// the zones' walk takes: one of a master of type 80 only once its build points (`+0xc50`)
    /// are above 0 or a NaN.
    ///
    /// # Errors
    ///
    /// A master the tables lack, and a building without its construction.
    fn begun_town_centre(&self, player: PlayerId) -> GenieResult<Option<ZoneCentre>> {
        let Some(unit) = self.town_centre_foundation(player) else {
            return Ok(None);
        };
        let master = unit.master(self.master_tables(), player)?;
        if master.kind == MasterKind::BUILDING {
            let construction = self.parts.building_of(unit.id).construction;
            let unbegun = construction.ok_or(NO_CONSTRUCTION)?.progress <= Seconds::ZERO;
            if unbegun {
                return Ok(None);
            }
        }
        Ok(Some(ZoneCentre::of(player, unit, master)))
    }

    /// [`World::draw_treaty_zones`]'s work on its owner's town centre `centre`: an owner whose
    /// zones are set, and a town centre dying or past (`+0x640` − 3 as a byte at most 5), draw
    /// none. Else its first zone takes the tiles about the centre ([`zone_about`]), is set, and
    /// the centre sweeps the foundations about it ([`World::sweep_treaty_zone`]). Unless the
    /// second zones are on, an owner with at most one entry in its town centres' lists
    /// ([`World::town_centre_entries`]) stops there; else they are on, and its second town
    /// centre's tiles become its second zone and that town centre sweeps too.
    ///
    /// # Errors
    ///
    /// A world without its monitor, a town centre of a master the tables lack, and the
    /// sweeps'.
    fn draw_player_zones(&mut self, centre: ZoneCentre) -> GenieResult<()> {
        let map = self.spatial.map;
        let monitor = self.treaty_monitor_mut()?;
        let margin = monitor.margin;
        let zone = monitor.zone_mut(centre.owner)?;
        if zone.set || ObjectState::Dying <= centre.state {
            return Ok(());
        }
        zone.first = zone_about(centre.at, margin, map);
        zone.set = true;
        self.sweep_treaty_zone(centre)?;
        if !self.treaty_monitor()?.second_zones && self.town_centre_entries(centre.owner) <= 1 {
            return Ok(());
        }
        self.treaty_monitor_mut()?.second_zones = true;
        let Some(unit) = self.town_centre_unit_at(centre.owner, 1) else {
            return Ok(());
        };
        let master = unit.master(self.master_tables(), centre.owner)?;
        let second = ZoneCentre::of(centre.owner, unit, master);
        self.treaty_monitor_mut()?.zone_mut(centre.owner)?.second =
            zone_about(second.at, margin, map);
        self.sweep_treaty_zone(second)
    }

    /// `FUN_1411c7ae0`: town centre `centre`'s sweep of its owner's first zone (read once, at
    /// its start) and five tiles past it on each side, rows then columns, the high sides left
    /// out ([`World::sweep_tile`]).
    ///
    /// # Errors
    ///
    /// A world without its monitor, and the tiles'.
    fn sweep_treaty_zone(&mut self, centre: ZoneCentre) -> GenieResult<()> {
        let first = self.treaty_monitor()?.zone(centre.owner)?.first;
        let (x0, x1) = (
            first.x0().wrapping_sub(SWEEP_REACH),
            first.x1().wrapping_add(SWEEP_REACH),
        );
        for y in first.y0().wrapping_sub(SWEEP_REACH)..first.y1().wrapping_add(SWEEP_REACH) {
            for x in x0..x1 {
                self.sweep_tile(centre, Tile::new(x, y))?;
            }
        }
        Ok(())
    }

    /// The sweep's work on `tile` (none off the map): each object of its list in turn, the list
    /// read again after each (`+0x28`, its count `+0x1b`), that goes ([`World::swept_owner`]) is
    /// removed as a building's delete removes it ([`World::remove_building`], `FUN_1411ccbb0`).
    ///
    /// # Errors
    ///
    /// The test's and the removal's.
    fn sweep_tile(&mut self, centre: ZoneCentre, tile: Tile) -> GenieResult<()> {
        let mut index = 0;
        while let Some(&id) = self.spatial.tile_list(tile).get(index) {
            if let Some(owner) = self.swept_owner(centre, id)? {
                self.remove_building(owner, id)?;
            }
            index += 1;
        }
        Ok(())
    }

    /// The owner of object `id` on a tile of `centre`'s sweep if it goes: not the centre, of
    /// another player, of a master of type 80, without build points (`COMISS` and `JC`: at most
    /// 0, a NaN stays), while the centre's own footprint (its place and its master's clearance)
    /// lies in the no-build zone of a player not allied with the centre's owner
    /// ([`TreatyMonitor::in_no_build_zone`]). A ghost (its viewer's object of the ghost master,
    /// type 25) stays.
    ///
    /// # Errors
    ///
    /// An object of a tile's list the world does not hold, a master the tables lack, a
    /// building without its construction and a world without its monitor.
    fn swept_owner(&self, centre: ZoneCentre, id: ObjectId) -> GenieResult<Option<PlayerId>> {
        if id == centre.id {
            return Ok(None);
        }
        let Some((owner, unit)) = self.object(id) else {
            return self.ghost(id).map(|_| None).ok_or(NOT_HELD);
        };
        if owner == centre.owner
            || unit.master(self.master_tables(), owner)?.kind != MasterKind::BUILDING
        {
            return Ok(None);
        }
        let construction = self.parts.building_of(id).construction;
        let unbegun = construction.ok_or(NO_CONSTRUCTION)?.progress <= Seconds::ZERO;
        let zoned = unbegun
            && self.treaty_monitor()?.in_no_build_zone(
                &self.players,
                self.settings.game_mode,
                centre.owner,
                centre.at,
                centre.clearance,
            );
        Ok(zoned.then_some(owner))
    }
}
