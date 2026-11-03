//! A new building's ground: the units on its foundation sent off, its terrain, the footprint
//! clearing and its place.

use crate::construction::FoundationId;
use crate::construction::build::Clearer;
use crate::map::Footprint;
use crate::map::Point;
use crate::map::Position;
use crate::map::TerrainId;
use crate::map::spatial::SpotPick;
use crate::movement::MoveState;
use crate::movement::PathState;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::players::PlayerId;
use crate::world::WholeSeconds;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;
use std::collections::BTreeMap;

use crate::GenieError;
use crate::construction::build::{ClearLook, clearing_objects, footprint_clearing};
use crate::construction::build::{Cleared, ClearingSite};
use crate::map::Snow;
use crate::map::spatial::Obstruction;
use crate::map::spatial::Placed;
use crate::map::terrain::Foundation;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::objects::lifecycle::death_parts::clear_look_of;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::{Master, player_master};
use crate::unit_ai::AiAction;
use crate::unit_ai::Notify;
use crate::unit_ai::NotifyKind;
use crate::vision::visibility::{BuildingSight, Seer, capture_kind};
use crate::world::World;
use crate::world::index::{ObjectsRef, Place};
use crate::world::layer::ClassRoles;

impl World {
    /// `FUN_14123bb20`'s sending off (with `DAT_1439e60e2` 1) of the units on `foundation`'s
    /// footprint: each of type 30 or more in state 2 whose top action is not on the foundation,
    /// that stands (`FUN_14114f740`: Gaia's of classes 9 and 10, or not walking, its animation
    /// speed `+0x7b8` at most 0, and its path level `+0x858` absent or in state 0 or 6) and
    /// whose unit AI's action is not 0x2bf or 0x2c0 (moving off already) is sent off: its unit AI
    /// gets notify 0x216 (slot 0x38) with the foundation master's radii times 100 (truncated) and
    /// the foundation ([`crate::unit_ai::NotifyKind::SENT_OFF`]). The foundation owner's own units of type 70
    /// and more of a formation category (`+0x8bc`) would go together to its farthest free spot
    /// (`FUN_1411566b0` with 1, `FUN_1416123a0`'s group move, then 0x21b each), which is not
    /// modelled.
    pub(crate) fn evacuate(
        &mut self,
        foundation: FoundationId,
        units: &[ObjectId],
    ) -> Result<(), GenieError> {
        let (site_owner, site) = self.object(foundation).ok_or(GenieError::unsupported(
            "a footprint's foundation the slice does not hold",
        ))?;
        let radius = site.master(self.master_tables(), site_owner)?.radius;
        let p = [
            (radius.x() * self.model.tuning.unit_ai.radius_scale) as i32,
            (radius.y() * self.model.tuning.unit_ai.radius_scale) as i32,
            foundation.id().get(),
        ];
        let mut together = Vec::new();
        for &id in units {
            let Some((owner, u)) = self.object(id) else {
                continue;
            };
            let master = u.master(self.master_tables(), owner)?;
            if master.kind < MasterKind::MOVING
                || u.life.state != ObjectState::Alive
                || self.parts.actions_of(u.id).top_target() == Some(foundation.id())
            {
                continue;
            }
            let walking = self
                .parts
                .anim_speed_of(u.id)
                .is_some_and(|s| Speed::ZERO < s);
            let level = self
                .parts
                .mover_of(u.id)
                .and_then(|m| m.paths.first())
                .map(|l| l.state);
            let stands = (owner == PlayerId::GAIA
                && self
                    .model
                    .roles
                    .of_class(master.class)
                    .intersects(ClassRoles::PREY | ClassRoles::PREDATOR))
                || (!walking
                    && level.is_none_or(|s| matches!(s, PathState::Idle | PathState::GaveUp)));
            let busy = self.parts.unit_ai.get(u.id).is_none_or(|ai| {
                matches!(
                    ai.current_action,
                    Some(AiAction::MoveAside | AiAction::GroupAside)
                )
            });
            if !stands || busy {
                continue;
            }
            if owner == site_owner
                && MasterKind::COMBAT <= master.kind
                && master.creatable_part().formation_category.grouped()
            {
                together.push(id);
                continue;
            }
            let state = u.life.state;
            let Some(Place::Live(q, i)) = self.place(id) else {
                return Err(GenieError::unsupported(
                    "a sleeping unit sent off a footprint",
                ));
            };
            if let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(q, i)?.id) {
                ai.post(Notify::own(id, NotifyKind::SENT_OFF, p), state)?;
            }
        }
        if !together.is_empty() {
            self.evacuate_together(foundation, radius, &together)?;
        }
        Ok(())
    }

    /// `FUN_14123bb20`'s owner's units of a formation category (`+0x8bc`) on `foundation`'s
    /// footprint (master radii `radius`), in the tiles' order: the farthest free spot beside the
    /// footprint from the first of them (`FUN_1411566b0` with 1,
    /// [`Spatial::spot_off`](crate::map::spatial::Spatial::spot_off)); then, with one,
    /// `FUN_1416123a0`: the order splitter on them ([`World::loose_units`]: a unit group it forms,
    /// its group move `FUN_14100efd0`, is not modelled) and each unit it leaves loose takes the
    /// action of moving off a footprint (`0x2bf`) and moves to the spot at its own height (slot
    /// 0x138 without a target); last each of them whose unit AI's action is not `0x2bf` gets notify
    /// `0x21b` ([`crate::unit_ai::NotifyKind::SENT_OFF_GROUP`]).
    pub(crate) fn evacuate_together(
        &mut self,
        foundation: FoundationId,
        radius: Footprint,
        units: &[ObjectId],
    ) -> Result<(), GenieError> {
        let by_id = ObjectsRef::new(
            &self.players,
            &self.index,
            &self.model.masters,
            &self.parts,
            &self.model.roles,
            &self.model.tuning,
        );
        let site_at = self
            .spatial
            .get(foundation)
            .ok_or(GenieError::unsupported(
                "a footprint's foundation without a place",
            ))?
            .position;
        let (_, first) = self
            .object(*units.checked(0)?)
            .ok_or(GenieError::unsupported("a unit sent off that is gone"))?;
        let mover = self
            .parts
            .mover_of(first.id)
            .ok_or(GenieError::unsupported(
                "a unit sent off without movement state",
            ))?;
        let spot = self.spatial.spot_off(
            first.position(),
            (first.id, mover.stats.radius.x()),
            mover.stats.restriction_mask,
            (site_at, radius),
            SpotPick::Farthest,
            by_id,
        )?;
        let Some(spot) = spot else {
            return Ok(());
        };
        let (x, y) = (spot.x(), spot.y());
        // the footprint's units come from the spatial index's raw ids
        let listed: Vec<Option<ObjectId>> = units.iter().map(|&id| Some(id)).collect();
        let loose = self.loose_units(&listed)?;
        for id in loose {
            let Some(Place::Live(q, i)) = self.place(id) else {
                continue;
            };
            let z = self.players.live(q, i)?.position().z();
            if let Some(ai) = self.parts.unit_ai.get_mut(self.players.live(q, i)?.id) {
                ai.set_action(Some(AiAction::MoveAside));
            }
            self.move_point_order(q, i, Position::new(x, y, z))?;
        }
        for &id in units {
            let Some(Place::Live(q, i)) = self.place(id) else {
                continue;
            };
            let unit = self.players.live_mut(q, i)?;
            let state = unit.life.state;
            if let Some(ai) = self.parts.unit_ai.get_mut(unit.id)
                && ai.current_action != Some(AiAction::MoveAside)
            {
                ai.post(
                    Notify::own(id, NotifyKind::SENT_OFF_GROUP, [0, 0, 0]),
                    state,
                )?;
            }
        }
        Ok(())
    }

    /// The building constructor's terrain (`FUN_1411de5f0`, for a master with a build time, not
    /// of class 51 or 54, nor 357 or 1188): a farm (master 50) lays terrain 29 under its
    /// clearance, a rice farm (1187) terrain 65 (`FUN_1411ce250` -> `FUN_1411ddd90`,
    /// [`crate::map::terrain::Terrain::cover`]), before the footprint clearing.
    pub(crate) fn construction_terrain(
        &mut self,
        player: PlayerId,
        master: MasterId,
        position: Position,
    ) -> Result<(), GenieError> {
        let s = self.model.slots.master;
        // the farms and the terrain their foundations lay (`0x1d`, `0x41`)
        let farms = [
            (s.farm, TerrainId::named(29)),
            (s.rice_farm, TerrainId::named(65)),
        ];
        let Some(&(_, laid)) = farms.iter().find(|(m, _)| *m == master) else {
            return Ok(());
        };
        let m = self.master(player, master)?;
        if m.creatable_part().build_time <= WholeSeconds::ZERO {
            return Ok(());
        }
        let f = Foundation {
            position,
            clearance: m.clearance,
            terrain: laid,
            snow: Snow::None,
            master,
            class: m.class,
            terrain_bonus: m.combat_part().terrain_bonus,
            mode_680: self.mode_680,
        };
        self.spatial
            .terrain
            .as_mut()
            .ok_or(GenieError::unsupported(
                "a farm's foundation without the map's terrain",
            ))?
            .cover(&f, &self.model.roles, self.model.tuning.terrain)
    }

    /// The footprint clearing (`FUN_1415c3620` with `param_7` 0) of a new building of `master`
    /// of `player` at `at`, by the [`Clearer`]: the building constructor (`FUN_1411de5f0`) or a
    /// new object (`FUN_14162ce00` with its flag). The objects it takes
    /// ([`crate::construction::build::footprint_clearing`]), each in the tiles' order: [`World::clear_object`].
    /// Nothing for a master the player has not got.
    ///
    /// # Errors
    ///
    /// What the choice and the deletes do not model.
    pub(crate) fn clear_footprint(
        &mut self,
        player: PlayerId,
        master: MasterId,
        at: Point,
        by: Clearer,
    ) -> Result<(), GenieError> {
        let Some(m) = player_master(&self.model.masters, player, master) else {
            return Ok(());
        };
        let site = ClearingSite {
            player,
            master,
            class: m.class,
            radius: m.radius,
            at,
            by,
        };
        let mut looks: BTreeMap<ObjectId, ClearLook> = BTreeMap::new();
        for id in clearing_objects(&self.spatial, at, &self.model.tuning.build) {
            if let Some(look) = self.clear_look(id)? {
                let _ = looks.insert(id, look);
            }
        }
        let cleared = footprint_clearing(
            &self.spatial,
            &site,
            &looks,
            &self.model.roles,
            &self.model.tuning.build,
        )?;
        for cl in cleared {
            self.clear_object(cl, false)?;
            if by == Clearer::Constructor {
                continue;
            }
            // FUN_141189670 after its destroy: unless +0x686 is set, a tall master's (COMISS: a
            // NaN height too) or a recorded object's record goes; then +0x686 (FUN_141189b60)
            let look = self.clear_look(cl.id)?.ok_or(GenieError::unsupported(
                "a footprint clearing of an object the slice does not hold",
            ))?;
            let recorded = self
                .spatial
                .get(cl.id)
                .is_some_and(|o| o.obstruction.is_some());
            if !look.roo_called && (below_or_unordered(0.0, look.height.get()) || recorded) {
                self.pathing.removed(cl.id);
                self.spatial.drop_record(cl.id);
                self.pathing.tiles_changed(self.spatial.take_changes());
            }
            if let Some((_, u)) = self.object_mut(cl.id) {
                u.roo_called = true;
            }
        }
        Ok(())
    }

    /// What the footprint clearing reads of object `id` ([`crate::construction::build::ClearLook`]).
    pub(crate) fn clear_look(&self, id: ObjectId) -> Result<Option<ClearLook>, GenieError> {
        let Some((p, u)) = self.object(id) else {
            return Ok(None);
        };
        Ok(Some(clear_look_of(
            u,
            self.parts.building_of(id),
            OwnerMasters::new(&self.parts.own_master, self.model.masters.of(p)),
        )?))
    }

    /// An object the footprint clearing deletes ([`crate::construction::build::Cleared`]): a wall first goes
    /// through the building's delete core (`FUN_1411ccbb0`, [`World::remove_building`]), then
    /// its destroy (slot 0xd0, [`World::destroy`]). The builder's clearing (`roo_done`) dropped
    /// its record in the Build's update already (`FUN_141189670`): `+0x686` is set
    /// (`FUN_141189b60` with 1).
    ///
    /// # Errors
    ///
    /// An object the slice does not hold, and what the deletes do not model.
    pub(crate) fn clear_object(&mut self, cl: Cleared, roo_done: bool) -> Result<(), GenieError> {
        let (owner, _) = self.object(cl.id).ok_or(GenieError::unsupported(
            "a footprint clearing of an object the slice does not hold",
        ))?;
        if cl.wall {
            self.remove_building(owner, cl.id)?;
        }
        self.destroy(owner, cl.id)?;
        if roo_done && let Some((_, u)) = self.object_mut(cl.id) {
            u.roo_called = true;
        }
        Ok(())
    }

    /// A new building's place (its static init's `new_position`, `0x1411a58b0`): it takes its
    /// place on the map, with its obstruction record when its master has a height
    /// (`FUN_14118c540`: `+0xb5f` above 0; a new building is in state 0 and inside nothing; a
    /// pasture's posts have none) unless it is desolid (`FUN_1411cf770` drops the one its place
    /// gave it; its first builder brings it back), and its sight, which waits for its
    /// construction to start (its place links its tiles to its anchor, `FUN_1411e6780`, and its
    /// desolid flag unlinks them).
    pub(crate) fn building_place(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        made: &Master,
        position: Position,
        desolid: bool,
    ) -> Result<(), GenieError> {
        let record = Obstruction {
            kind: made.obstruction_kind,
            class: made.obstruction_class,
        };
        let placed = Placed {
            position,
            radius: made.radius,
            class: made.class,
            player,
            master: made.id,
            obstruction_kind: made.obstruction_kind,
            buildable_on: made.buildable_on,
            tall: made.is_tall(),
            moves: false,
            unknown: None,
            obstruction: (!desolid
                && !at_most_or_unordered(made.height.get(), 0.0)
                && record.recorded())
            .then_some(record),
        };
        self.spatial.insert(id, placed);
        if placed.obstruction.is_some() {
            self.pathing.moved(id, &placed, MoveState::Standing);
            self.pathing.tiles_changed(self.spatial.take_changes());
        }
        if let Some(v) = self.spatial.sight.as_mut() {
            let umv = v.link_building(position, made.radius, self.model.tuning.visibility)?;
            if desolid {
                v.unlink_building(position, made.radius, self.model.tuning.visibility)?;
            }
            v.add_seer(
                id,
                Seer {
                    player,
                    master: made.id,
                    los: made.line_of_sight,
                    state: ObjectState::Foundation,
                    explored: false,
                    building: Some(BuildingSight::default()),
                    umv: umv as u8,
                    capture: capture_kind(player, made.class, made.id, &self.model.roles),
                    kind: MasterKind::BUILDING,
                },
            );
        }
        Ok(())
    }
}
