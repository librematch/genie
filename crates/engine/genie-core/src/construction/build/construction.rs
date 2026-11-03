//! A foundation's construction ([`Construction`]): its start and its work's share of hit
//! points, with the completion's storage, ground and counts
//! ([`crate::world::World::complete_building`]).

use crate::GenieError;
use crate::construction::build::{Construction, SITE_GONE, STARTED};
use crate::map::Snow;
use crate::map::TerrainId;
use crate::map::Tile;
use crate::map::spatial::Placed;
use crate::map::spatial::Spatial;
use crate::map::terrain::Foundation;
use crate::math::fmath::at_most_or_unordered;
use crate::movement::Mover;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::BuildingMaster;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::pathing::Pathing;
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::players::history::EventKind;
use crate::vision::SightLayer;
use crate::world::Millis;
use crate::world::Mode680;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use genie_model::model::Model;

/// The completion's storage ([`crate::world::World::complete_building`]): its master's entries of modes 4, 8 and 64 go to its
/// owner (slot 0xf8, into `attributes`), those of mode 16 to its own `store` (`FUN_1411abad0`: an
/// entry from 0); an entry of no resource or no amount gives nothing.
pub(super) fn hand_storage(
    master: &Master,
    attributes: &mut Vec<(AttributeId, Amount)>,
    mut store: Option<&mut Vec<(AttributeId, Amount)>>,
) {
    for &(kind, amount, flag) in &master.storage {
        let Some(kind) = kind.filter(|_| Amount::ZERO < amount) else {
            continue;
        };
        if matches!(flag, 4 | 8 | 64) {
            attributes.push((kind, amount));
        }
    }
    // the entries of mode 16 go to its own store (FUN_1411abad0: an entry from 0)
    for &(kind, amount, flag) in &master.storage {
        let Some(kind) = kind.filter(|_| Amount::ZERO < amount && flag & 0x10 != 0) else {
            continue;
        };
        if let Some(store) = store.as_deref_mut() {
            match store.iter_mut().find(|e| e.0 == kind) {
                Some(e) => e.1 += amount,
                None => store.push((kind, amount)),
            }
        }
    }
}

/// The foundation terrain a completed `building` lays ([`lay_foundation`]): its own, or terrain
/// 0x3f under master 1889's pasture on mangrove or rice (`stack_master` [`PASTURE_MANGROVE`]).
pub(super) fn foundation_laid(
    building: &BuildingMaster,
    stack_master: Option<MasterId>,
    roles: &Roles,
) -> Option<TerrainId> {
    /// The terrain the pasture master 1889's stack unit on mangrove or rice terrain lays
    /// (`0x3f`).
    const RICE_PASTURE_TERRAIN: TerrainId = TerrainId::named(0x3f);
    building.foundation_terrain.map(|foundation| {
        if roles
            .of_some_master(stack_master)
            .contains(MasterRoles::PASTURE_MANGROVE)
        {
            RICE_PASTURE_TERRAIN
        } else {
            foundation
        }
    })
}

/// The completion's ground ([`crate::world::World::complete_building`]): on snow the building gets its snow flag; its foundation
/// terrain `laid` ([`foundation_laid`]) covers its clearance
/// ([`crate::map::terrain::Terrain::cover`]).
///
/// # Errors
///
/// A map without its terrain, and a tile the terrain does not hold.
pub(super) fn lay_foundation(
    c: &mut Construction,
    world: &mut Spatial,
    placed: &Placed,
    master: &Master,
    laid: Option<TerrainId>,
    mode_680: Mode680,
    model: &Model,
) -> Result<(), GenieError> {
    let terrain = world.terrain.as_mut().ok_or(GenieError::unsupported(
        "a completion without the map's terrain",
    ))?;
    let (x, y) = (placed.position.x(), placed.position.y());
    if c.snow == Snow::None
        && 0 <= x as i32
        && 0 <= y as i32
        && terrain.snow_under(Tile::new(x as i32, y as i32))?
    {
        c.snow = Snow::Completed;
    }
    if let Some(laid) = laid {
        terrain.cover(
            &Foundation {
                position: placed.position,
                clearance: master.clearance,
                terrain: laid,
                snow: c.snow,
                master: master.id,
                class: master.class,
                terrain_bonus: master.terrain_bonus,
                mode_680,
            },
            &model.roles,
            model.tuning.terrain,
        )?;
    }
    Ok(())
}

/// The completion's counts ([`crate::world::World::complete_building`]) into `attributes`: the castle and the wonder count in
/// attributes 134 and 173, 42 and 174 and add their paid cost to 184; the monastery counts in
/// 52; any other building's paid cost grows 98 and 165 unless it is an annex (`main`) or has a
/// stack unit. The paid cost sums the cost entries of a resource flagged paid (`+0x880`) as
/// shorts. The castle's and the wonder's history event (0x1411dd094), returned.
pub(super) fn completion_counts(
    master: &Master,
    building: &BuildingMaster,
    main: bool,
    attributes: &mut Vec<(AttributeId, Amount)>,
    roles: &Roles,
    slots: Slots,
) -> Option<EventKind> {
    // the castle's and the wonder's history event (0x1411dd094), the counts (slot 0xf8 with
    // 1.0, 0x14330eb8c)
    let history = {
        let held = roles.of_master(master.id);
        if held.contains(MasterRoles::CASTLE) {
            Some(EventKind::CASTLE_BUILT)
        } else if held.contains(MasterRoles::WONDER) {
            Some(EventKind::WONDER_BUILT)
        } else {
            None
        }
    };
    {
        let held = roles.of_master(master.id);
        if held.contains(MasterRoles::MONASTERY) {
            attributes.push((slots.attribute.monasteries, Amount::new(1.0)));
        } else if held.contains(MasterRoles::WONDER) {
            attributes.extend([
                (slots.attribute.wonders, Amount::new(1.0)),
                (slots.attribute.wonders_built, Amount::new(1.0)),
            ]);
        } else if held.contains(MasterRoles::CASTLE) {
            attributes.extend([
                (slots.attribute.castles, Amount::new(1.0)),
                (slots.attribute.castles_built, Amount::new(1.0)),
            ]);
        }
    }
    // its paid cost: the cost entries of a resource flagged paid (`+0x880`), summed as shorts
    let paid: i32 = master
        .creatable_part()
        .cost
        .iter()
        .filter_map(|slot| slot.entry())
        .filter(|&(_, _, paid)| paid)
        .map(|(_, amount, _)| i32::from(amount.get()))
        .sum();
    if roles
        .of_master(master.id)
        .intersects(MasterRoles::CASTLE | MasterRoles::WONDER)
    {
        attributes.push((slots.attribute.castle_wonder_cost, Amount::new(paid as f32)));
    } else if !main && building.stack_unit.is_none() {
        attributes.push((slots.attribute.total_value, Amount::new(paid as f32)));
        attributes.push((slots.attribute.buildings_value, Amount::new(paid as f32)));
    }
    history
}

impl Construction {
    /// `FUN_1411cc430`'s work of `amount` seconds on a foundation in state 0 with at least 1 hit
    /// point: at most up to its build time, it adds its share of its master's hit points `hp`
    /// (all of them without a build time of at least 1), never past them; true when its build
    /// points reach its build time.
    pub(crate) fn add_work(&mut self, hp: &mut HitPoints, amount: Seconds) -> bool {
        let mut amount = amount.get();
        let build_time = Seconds::of_master(self.build_time).get();
        let hit_points = HitPoints::of_master(self.hit_points).get();
        if build_time < self.progress.get() + amount {
            amount = build_time - self.progress.get();
        }
        let new = if 1.0 <= build_time {
            (amount * hit_points) / build_time + hp.get()
        } else {
            hit_points
        };
        *hp = HitPoints::new(if hit_points < new { hit_points } else { new });
        let before = self.progress.get();
        self.progress = Seconds::new(amount + before);
        at_most_or_unordered(build_time, amount + before)
    }

    /// The construction starts (`FUN_1411e5670`) on a desolid foundation (one not desolid, or
    /// without its own master copy `+0x6c0`, which a desolid one always holds, returns at once,
    /// false): its master is its owner's again (the same in the slice), its missiles per volley
    /// its master's (`+0xa00`, the caller's: true); it is no longer desolid, holds a sliver of
    /// progress, and gets its obstruction record (`FUN_141056460`: the tiles it marks, which the
    /// planners see, and its shape in the mesh); its footprint comes into its player's sight and
    /// its tiles link to its anchor again (`FUN_140f73900`); its owner watches it
    /// (`FUN_141090430` when `FUN_1411cc980`, the caller's:
    /// [`crate::world::World::construction_starts`] calls the watch at once); its last move time becomes the world's time plus 1. Not held: its damage sprites (made
    /// for a player not allied with the local one), the tall count of its tile (`FUN_140f64540`)
    /// and `+0xca4`.
    ///
    /// # Errors
    ///
    /// A foundation the world does not hold.
    pub(crate) fn begin(
        id: ObjectId,
        c: &mut Self,
        mover: Option<&mut Mover>,
        world: &mut Spatial,
        pathing: &mut Pathing,
        now: WorldTime,
        sight: SightLayer<'_>,
    ) -> Result<bool, GenieError> {
        let SightLayer {
            roles,
            visibility_tuning,
        } = sight;
        if !c.desolid {
            return Ok(false);
        }
        let placed = *world.get(id).ok_or(SITE_GONE)?;
        c.desolid = false;
        c.progress = Seconds::new(f32::from_bits(STARTED));
        if let Some(v) = world.sight.as_mut() {
            v.construction_started(id, placed.position, placed.radius, roles, visibility_tuning)?;
        }
        // a master of obstruction kind 0 or 12, or of class 0, gets no record (FUN_141056460)
        if c.record.recorded() {
            world.add_record(id, c.record);
        }
        pathing.tiles_changed(world.take_changes());
        if let Some(p) = world.get(id) {
            pathing.moved(id, p, false);
        }
        if let Some(m) = mover {
            m.last_move = now.after(Millis::new(1));
        }
        Ok(true)
    }
}
