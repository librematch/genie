//! A builder's search for more work once its Build ended without queued orders (the Build's
//! case 0xdf): a resource around the building it finished, as the unit AI's
//! `ClosestResourceObject` names it.

use crate::GenieError;
use crate::map::Distance;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::MasterId;
use crate::objects::masters::class;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::unit::Unit;
use crate::orders::Work;
use crate::pathing::tile_path::GridLevel;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::drop_site;
use crate::world::others::OthersRef;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
    /// The Build's case 0xdf once its work ended without queued orders (`0x140ab0b43`): the
    /// builder looks for more work around building `building` of master `built`
    /// ([`crate::gathering::gather_search::more_work`] with the drop site's entry for the building, the
    /// builder's class and civ, the reach the building's line of sight held to 4 to 20, and the
    /// planner of the builder's restriction). Nothing found: None, the caller lets the foundation
    /// go. A farm found is not modelled; else the unit AI's `ClosestResourceObject` for the found
    /// object's gather group and workers cap ([`crate::gathering::gather_search::gather_group`],
    /// [`crate::gathering::gather_search::closest_resource`]) names the target, the found object when it
    /// names none, and slot 0x7e8 works it at its place (the work flag 1 only logged, any task,
    /// no carried-resource test, `+0xa14` as it stands outside slot 0x128: 1; the caller's).
    pub(crate) fn more_work(
        &self,
        actions: &Action,
        c: &mut Step<'_>,
        building: ObjectId,
        built: MasterId,
    ) -> Result<Option<Work>, GenieError> {
        use crate::gathering::gather_search::{self, Candidate};
        use crate::math::image::gather_search::{REACH_CAP, REACH_FLOOR};
        /// The reach a line of sight below the floor takes (the Build's immediate 4.0).
        const REACH_LOW: f32 = 4.0;
        let others = OthersRef::of(&c.w.players, &c.w.index, c.player)?;
        let masters =
            c.w.masters
                .get(c.player.index())
                .ok_or(GenieError::unsupported(
                    "a builder without its player's masters",
                ))?;
        let master = self.master_in(OwnerMasters::new(&c.w.parts.own_master, Some(masters)))?;
        let site = masters.get(built).ok_or(GenieError::unsupported(
            "a building of a master the slice does not have",
        ))?;
        let at = *c.w.spatial.get(building).ok_or(GenieError::unsupported(
            "a builder's building without a place",
        ))?;
        let entry = drop_site(
            &c.w.drop_sites,
            built,
            master.class,
            c.w.players.player(c.player)?.civ,
        );
        let los = site.line_of_sight.get();
        let held = if REACH_FLOOR <= los { los } else { REACH_LOW };
        let reach = if held <= REACH_CAP { held } else { REACH_CAP };
        let builder = gather_search::Builder {
            position: self.position(),
            radius: Distance::new(master.radius.x()),
            building: at.position,
            range: reach as i32 as i16,
        };
        let mover =
            c.w.parts
                .mover_of(self.id)
                .ok_or(GenieError::unsupported("a builder without movement state"))?;
        let restriction = mover.stats.restriction;
        let (player, me_id, every) = (
            c.player,
            self.id,
            MasterTables::new(&c.w.masters, &c.w.parts.own_master),
        );
        let candidate = |id: ObjectId, owner: PlayerId, u: &Self, position: Position| {
            let m = u.master(every, owner)?;
            Ok(Candidate {
                id,
                player: owner,
                class: m.class,
                master: m.id,
                state: u.life.state,
                workers: u.workers,
                position,
                pasture: u.pasture,
                untargetable: u.untargetable,
                attribute: u.life.attribute_type,
                amount: u.life.amount,
                auto_gather: m.auto_gather,
                gather_group: m.gather_group,
            })
        };
        let me = candidate(me_id, player, self, self.position())?;
        let spatial: &Spatial = &c.w.spatial;
        let objects = |tile: Tile| -> Result<Vec<Candidate>, GenieError> {
            spatial
                .objects_on(tile)
                .map(|(id, p)| {
                    if id == me_id {
                        return Ok(me);
                    }
                    let (owner, u) = others.find(id).ok_or(GenieError::unsupported(
                        "an object on a tile the slice does not hold",
                    ))?;
                    candidate(id, owner, u, p.position)
                })
                .collect()
        };
        let found = {
            let planner = c.w.pathing.planner(restriction, spatial.tiles()).ok_or(
                GenieError::unsupported("a builder's restriction without a planner"),
            )?;
            let tiles = GridLevel::TILES;
            let width = i64::from(planner.level_size(tiles).width());
            let cells = planner.level(tiles);
            let blocked = move |t: Tile| -> Result<bool, GenieError> {
                let cell = (width * i64::from(t.y()) + i64::from(t.x())) as usize;
                Ok(cells.checked(cell)?.edges.is_empty())
            };
            gather_search::more_work(entry, &builder, spatial.map, Some(&blocked), &objects)?
        };
        let Some(found) = found else {
            return Ok(None);
        };
        if found.class == class::FARM {
            return Err(GenieError::unsupported(
                "a builder's search for more work finding a farm",
            ));
        }
        let (kind, cap) = gather_search::gather_group(found.attribute);
        let target = self.resource_search(actions, c, kind, cap)?;
        let spatial: &Spatial = &c.w.spatial;
        let target = target
            .filter(|&t| spatial.get(t).is_some())
            .unwrap_or(found.id);
        let point = spatial
            .get(target)
            .ok_or(GenieError::unsupported(
                "a builder's new work without a place",
            ))?
            .position;
        Ok(Some(Work {
            target: Some(target),
            point,
            carry_test: false,
            filter: None,
            notified: true,
        }))
    }
}
