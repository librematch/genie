//! A new group's place and facing (FD-036): `SetInitialPosition` (`FUN_141039990`) on the first
//! clear point of a spiral round the centroid (`FUN_141007630`), `SetInitialOrientation`
//! (`FUN_141039680`) and the motion's orientation (`FUN_141049e10`).

use crate::GenieError;
use crate::groups::FormationKind;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::Motion;
use crate::groups::MotionFlags;
use crate::groups::WorldObject;
use crate::groups::formation::on_map;
use crate::map::Distance;
use crate::map::DistanceSquared;
use crate::map::TerrainMask;
use crate::map::spatial::RecordId;
use crate::map::{GroundOffset, Point};
use crate::math::fmath::BVector;
use crate::math::fmath::{add, line_distance, scale, sqrt, sub};
use crate::math::fmath::{cross, normalize};
use crate::math::image::group::{FARTHEST as GROUP_FARTHEST, SPIRAL_FIRST, SPIRAL_LAST};
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::players::PlayerId;
use crate::world::World;
use crate::world::index::ObjectsRef;
use crate::world::layer::generated::GroupTuning;
use crate::world::slots::CheckedIndex;

/// The spiral's steps (`FUN_141007630`'s table from `DAT_142f49460` and `DAT_142f49480`), x and y:
/// up y, up x, down y, down x.
const SPIRAL: [GroundOffset; 4] = GroundOffset::table([
    SPIRAL_FIRST[0],
    SPIRAL_FIRST[1],
    SPIRAL_LAST[0],
    SPIRAL_LAST[1],
]);
/// The points the spiral tries (`FUN_141007630`'s count from 1 below `0x191`).
const SPIRAL_POINTS: i32 = 400;

/// `FUN_141007630`'s spiral: where it is (x, y), its direction in [`SPIRAL`], its leg's length
/// and its count of steps (from 1).
struct Spiral {
    at: Point,
    direction: usize,
    leg: i32,
    count: i32,
}

impl From<Point> for Spiral {
    fn from(at: Point) -> Self {
        Self {
            at,
            direction: 0,
            leg: 1,
            count: 1,
        }
    }
}

impl Spiral {
    /// One step in the direction; it turns when the count divides by the leg's length, and the
    /// leg grows on a turn to y (an even direction).
    fn step(&mut self) -> Result<(), GenieError> {
        self.at = self.at.moved(*SPIRAL.checked(self.direction)?);
        if self.count % self.leg == 0 {
            self.direction = (self.direction + 1) % SPIRAL.len();
            if self.direction.is_multiple_of(2) {
                self.leg += 1;
            }
        }
        self.count += 1;
        Ok(())
    }
}

impl Motion {
    /// `FUN_141049e10`: the motion faces `forward` with `up`, both normalized, and their cross
    /// product `up × forward` (normalized) as its right; the new position takes all three, and with
    /// `both` (`param_4` 1) the position too. [`MotionFlags::MOVED`] is set. False, with nothing
    /// changed, when the dot product of the two lies outside `GroupTuning::square_min` to `GroupTuning::square_max`.
    pub fn set_orientation(
        &mut self,
        forward: BVector,
        up: BVector,
        both: bool,
        group_tuning: &GroupTuning,
    ) -> bool {
        let d = up.height() * forward.height() + up.x() * forward.x() + up.y() * forward.y();
        if d < group_tuning.square_min || group_tuning.square_max < d {
            return false;
        }
        let right = normalize(cross(up, forward));
        let (forward, up) = (normalize(forward), normalize(up));
        self.new_position[1] = forward;
        self.new_position[2] = up;
        self.new_position[3] = right;
        if both {
            self.position[1] = forward;
            self.position[2] = up;
            self.position[3] = right;
        }
        self.flags |= MotionFlags::MOVED;
        true
    }
}

impl World {
    /// `SetInitialPosition` (`FUN_141039990`) of group `group`, as its `DAT_1439e60e4` (1, which
    /// nothing writes) has it: the stored positions lose their points, the group stands, at once
    /// and as its new position, on the first clear point round its centroid
    /// ([`World::clear_spot`]), faces as its members do ([`World::set_initial_orientation`]), and
    /// it takes [`GroupFlags::PLACED`].
    ///
    /// # Errors
    ///
    /// What [`World::clear_spot`] does not model.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, and its
    /// callers pass a group they hold.
    pub fn set_initial_position(&mut self, group: GroupId) -> Result<(), GenieError> {
        let g = self.groups.held_mut(group)?;
        g.stored_positions.clear_points();
        let centroid = g.centroid;
        let at = self.clear_spot(group, centroid)?;
        let g = self.groups.held_mut(group)?;
        g.motion.new_position[0] = at;
        g.motion.position[0] = at;
        self.set_initial_orientation(group)?;
        let g = self.groups.held_mut(group)?;
        g.flags |= GroupFlags::PLACED;
        Ok(())
    }

    /// `FUN_141007630`: the first point of a spiral from `centroid` that no obstruction record
    /// holds. The obstruction manager's queries take the terrain mask of the first member's
    /// master's restriction (`+0x606`, `FUN_141054630`) and ignore the members (`FUN_1410562e0`,
    /// left set after). The centroid's tile, when the mask lacks it, takes a temporary record
    /// (`FUN_141052b90` with the buffer the last `FUN_1410562d0` left), which the routine never
    /// removes (no `FUN_1410561f0`): it stays for the queries after. Each point, kept on the map
    /// ([`on_map`]), is a point query with that buffer that leaves out the objects that move
    /// (flags 0x60, [`crate::map::spatial::Spatial::point_hit`]). From the
    /// centroid the spiral steps by [`SPIRAL`]'s directions in turn, a turn after each step that
    /// its count (from 1) divides by the leg's length, which grows on the turns to y; after
    /// [`SPIRAL_POINTS`] points, or for a group without members, the centroid. The point keeps the
    /// centroid's height.
    ///
    /// # Errors
    ///
    /// A first member the world does not hold (the game reads a null object) or of no terrain
    /// restriction, a map without its terrain, and a point query near a record whose place the
    /// slice does not know.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn clear_spot(&mut self, group: GroupId, centroid: BVector) -> Result<BVector, GenieError> {
        let g = self.groups.held(group)?;
        let Some(first) = g.members.first() else {
            return Ok(centroid);
        };
        let (p, u) = self.object(first.id).ok_or(GenieError::unsupported(
            "a group's clear spot without its first member (a null object)",
        ))?;
        let mask = self.master_mask(p, u.life.master_id)?;
        // the centroid's temporary tile records, with the buffer the last FUN_1410562d0 left,
        // which no one removes here: they stay for the queries after (FUN_141052b90)
        let grow = self.spatial.manager_grow();
        let at = centroid.ground();
        self.spatial.put_tile_records(at, at, grow, mask);
        let members: Vec<RecordId> = g.members.iter().map(|m| RecordId::from(m.id)).collect();
        // the manager's ignore list (FUN_1410562e0 with the group's +0xf0, +0xe4)
        self.spatial.set_manager_ignore(&members);
        let mut spiral = Spiral::from(centroid.ground());
        for _ in 0..SPIRAL_POINTS {
            let at = on_map(
                BVector::new(spiral.at.x(), centroid.height(), spiral.at.y()),
                self.spatial.map,
                &self.model.tuning.group,
            );
            if !self.spatial.point_hit(
                at.ground(),
                grow,
                &members,
                true,
                ObjectsRef::new(
                    &self.players,
                    &self.index,
                    &self.model.masters,
                    &self.parts,
                    &self.model.roles,
                    &self.model.tuning,
                ),
            )? {
                return Ok(at);
            }
            spiral.step()?;
        }
        Ok(centroid)
    }

    /// `SetInitialOrientation` (`FUN_141039680`) of group `group`: the forwards (slot 0x648,
    /// [`crate::groups::WorldObject::forward`]: 0 for an object that does not move) of the
    /// members the world's table holds, summed in their order and divided by their count, then
    /// normalized; when their squares sum to zero (`UCOMISS`/`JP`: not a NaN) the first held
    /// member's forward, else (`GroupTuning::default_forward`, 0, 0). The motion faces it with up
    /// (0, 1, 0), position and new position alike ([`Motion::set_orientation`]).
    ///
    /// # Errors
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, and its
    /// caller ([`World::set_initial_position`]) holds it.
    pub fn set_initial_orientation(&mut self, group: GroupId) -> Result<(), GenieError> {
        let g = self.groups.held(group)?;
        let forwards: Vec<BVector> = g
            .members
            .iter()
            .filter_map(|m| self.world_object(m.id).map(|o| o.forward()))
            .collect();
        let mut sum = [0.0f32; 3];
        for f in &forwards {
            sum = [sum[0] + f.x(), sum[1] + f.height(), sum[2] + f.y()];
        }
        if !forwards.is_empty() {
            let n = forwards.len() as f32;
            sum = [sum[0] / n, sum[1] / n, sum[2] / n];
        }
        let squared = sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2];
        let forward = if squared == 0.0 {
            forwards
                .first()
                .copied()
                .unwrap_or_else(|| BVector::new(self.model.tuning.group.default_forward, 0.0, 0.0))
        } else {
            let l = sqrt(squared);
            BVector::new(sum[0] / l, sum[1] / l, sum[2] / l)
        };
        let g = self.groups.held_mut(group)?;
        let _ = g.motion.set_orientation(
            forward,
            BVector::new(0.0, 1.0, 0.0),
            true,
            &self.model.tuning.group,
        );
        Ok(())
    }
}

impl World {
    /// `movePositionToFront` (`FUN_141025a20`) of group `group`, `ahead` (`param_2`, 100 in the
    /// update) the distance of the point ahead along the group's forward. The group's place is its
    /// own, or with [`MotionFlags::NEW`] the members' centre (slot 0x520, summed over the members
    /// the world holds, over the members' count). A flanked group (locked type 8) whose lead's
    /// object ([`World::lead_object`]) moves and could stand there among the members (slot 0x270,
    /// [`crate::map::spatial::Spatial::point_clear_among`], with `GroupTuning::front_share` of its
    /// radius; an object that does not move cannot) stands there. Else the members within the
    /// spacing (`GroupTuning::front_share` of it but for a staggered lock, type 7) of the line
    /// along the forward through that place (on the ground,
    /// [`fmath::line_distance`](crate::math::fmath::line_distance)) are in front: of them the one
    /// nearest the point ahead on the ground (the only one, however far), else the member nearest
    /// that point. The group's stored positions lose their points
    /// ([`crate::groups::PointLine::clear_points`]) and it stands on the chosen member's place
    /// (slot 0x520, height included). Nothing changes when the world holds no member.
    ///
    /// # Errors
    ///
    /// The lead's master, restriction or the map's terrain missing ([`World::master_mask`]),
    /// and its point test near a record whose place the slice does not know.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn move_position_to_front(
        &mut self,
        group: GroupId,
        ahead: Distance,
    ) -> Result<(), GenieError> {
        let by_id = ObjectsRef::new(
            &self.players,
            &self.index,
            &self.model.masters,
            &self.parts,
            &self.model.roles,
            &self.model.tuning,
        );
        let Some(g) = self.groups.get(group) else {
            return Ok(());
        };
        let m = &g.motion;
        let forward = m.position[1];
        let front = add(scale(ahead.get(), forward), m.position[0]);
        let held: Vec<(ObjectId, BVector)> = g
            .members
            .iter()
            .filter_map(|mb| {
                self.object(mb.id)
                    .map(|(_, u)| (mb.id, u.moved_vector(self.parts.mover_of(u.id))))
            })
            .collect();
        let centre = if m.flags.contains(MotionFlags::NEW) {
            let mut c = [0.0f32; 3];
            for &(_, p) in &held {
                c = [c[0] + p.x(), c[1] + p.height(), c[2] + p.y()];
            }
            let n = g.members.len() as f32;
            BVector::new(c[0] / n, c[1] / n, c[2] / n)
        } else {
            m.position[0]
        };
        // the lead's slot 0x270: an object that does not move (a ghost found by the lead -1)
        // stands nowhere (`FUN_141071e30`)
        if g.formation_locked_kind == Some(FormationKind::Flanked)
            && let Some(lead) = self.lead_object(m).filter(WorldObject::moves)
        {
            let master = self.master(lead.player(), lead.master_id(self.model.slots))?;
            let mask = self.master_mask(lead.player(), lead.master_id(self.model.slots))?;
            let members: Vec<ObjectId> = g.members.iter().map(|mb| mb.id).collect();
            if self.spatial.point_clear_among(
                lead.id(),
                centre.ground(),
                Distance::new(self.model.tuning.group.front_share * master.radius.x()),
                mask,
                &members,
                by_id,
            )? {
                let g = self.groups.held_mut(group)?;
                g.motion.new_position[0] = centre;
                g.motion.position[0] = centre;
                return Ok(());
            }
        }
        let limit = if g.formation_locked_kind == Some(FormationKind::Staggered) {
            g.desired_spacing
        } else {
            g.desired_spacing * self.model.tuning.group.front_share
        };
        let through = centre.with_height(0.0);
        let mut in_front: Vec<(ObjectId, BVector)> = Vec::new();
        let mut nearest = None;
        let mut nearest_d = DistanceSquared::new(FARTHEST);
        for &(id, p) in &held {
            let q = p.with_height(0.0);
            if line_distance(q, through, forward) < limit.get() {
                in_front.push((id, p));
            }
            let d = ground_squared(q, front);
            if d < nearest_d {
                nearest = Some(id);
                nearest_d = d;
            }
        }
        // of those in front the one nearest the point ahead: a lone one from the start
        let mut chosen = match in_front.as_slice() {
            [(id, _)] => Some(*id),
            _ => None,
        };
        let mut beside = FARTHEST;
        let mut chosen_d = DistanceSquared::new(FARTHEST);
        for &(id, p) in &in_front {
            let d = ground_squared(p, front);
            if d < chosen_d {
                chosen = Some(id);
                beside = (p.y() - centre.y()) * (p.y() - centre.y())
                    + (p.x() - centre.x()) * (p.x() - centre.x());
                chosen_d = d;
            }
        }
        let spot = match self.object(chosen) {
            Some((p, u))
                if !in_front.is_empty()
                    || ground_squared(u.moved_vector(self.parts.mover_of(u.id)), front)
                        <= nearest_d
                    || {
                        let r = u.master(self.master_tables(), p)?.radius.x();
                        beside <= (r + r) * (r + r)
                    } =>
            {
                Some(u.moved_vector(self.parts.mover_of(u.id)))
            }
            _ => self
                .object(nearest)
                .map(|(_, u)| u.moved_vector(self.parts.mover_of(u.id))),
        };
        let Some(at) = spot else {
            return Ok(());
        };
        let g = self.groups.held_mut(group)?;
        g.stored_positions.clear_points();
        g.motion.new_position[0] = at;
        g.motion.position[0] = at;
        Ok(())
    }

    /// The terrain classes master `master` of player `p` lets its objects stand on: its
    /// restriction (`+0x606`) in the map's table (`FUN_141054630`).
    ///
    /// # Errors
    ///
    /// No such master, a master without a restriction (the game indexes before the table) and a
    /// map without its terrain.
    pub(crate) fn master_mask(
        &self,
        p: PlayerId,
        master: MasterId,
    ) -> Result<TerrainMask, GenieError> {
        let restriction =
            self.master(p, master)?
                .terrain_restriction
                .ok_or(GenieError::unsupported(
                    "a terrain mask of a master without a restriction",
                ))?;
        Ok(self
            .spatial
            .terrain
            .as_ref()
            .ok_or(GenieError::unsupported(
                "a terrain mask without the map's terrain",
            ))?
            .restriction_mask(restriction))
    }
}

/// The first best distance of `movePositionToFront`'s searches (`DAT_14330ebf4`).
const FARTHEST: f32 = GROUP_FARTHEST;

/// `p − q` squared on the ground, the height difference zeroed (`FUN_14087e560`, then
/// `x² + 0 + y²`).
fn ground_squared(p: BVector, q: BVector) -> DistanceSquared {
    let d = sub(p, q);
    DistanceSquared::new(d.x() * d.x() + 0.0 + d.y() * d.y())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::gathering::hunt::tests::walker;
    use crate::groups::MotionFlags;
    #[cfg(test)]
    use crate::groups::formation::tests::ghost_at;
    #[cfg(test)]
    use crate::groups::{LinePoint, Member, UnitGroup, UnitGroups};
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Position;
    use crate::map::Restriction;
    use crate::map::RestrictionId;
    use crate::map::TerrainFlags;
    use crate::map::terrain::TileTerrain;
    #[cfg(test)]
    use crate::objects::layers::Static;
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::ObstructionClass;
    use crate::objects::masters::ObstructionKind;
    #[cfg(test)]
    use crate::objects::masters::{Master, Masters};
    #[cfg(test)]
    use crate::objects::unit::Unit;
    use crate::test_ids::group_id;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::testing::class;
    use crate::testing::tuning;
    use crate::world::Millis;
    use crate::world::WorldTime;
    #[cfg(test)]
    use crate::world::parts::Object;
    #[cfg(test)]
    use crate::world::{FrameSeed, Player};
    use std::sync::Arc;

    /// `FUN_141049e10` facing x with up y: right is `y × x`, (0, 0, -1), the constructor's.
    #[test]
    fn an_orientation_takes_the_cross_product_of_up_and_forward_as_its_right() {
        let mut m = Motion::new();
        m.flags = MotionFlags::NONE;
        assert!(m.set_orientation(
            BVector::new(2.0, 0.0, 0.0),
            BVector::new(0.0, 1.0, 0.0),
            true,
            &tuning().group
        ));
        assert_eq!(m.position[1], BVector::new(1.0, 0.0, 0.0));
        assert_eq!(m.position[3], BVector::new(0.0, 0.0, -1.0));
        assert_eq!(m.new_position[3], BVector::new(0.0, 0.0, -1.0));
        assert_eq!(m.flags, MotionFlags::MOVED);
        // a forward too far from square to the up is refused
        assert!(!m.set_orientation(
            BVector::new(1.0, 0.1, 0.0),
            BVector::new(0.0, 1.0, 0.0),
            true,
            &tuning().group
        ));
    }

    /// A unit at `[x, y]` (the unit's order: x, y, height) of master 4.
    fn unit(id: i32, x: f32, y: f32) -> Unit {
        let mut u = Unit {
            id: object_id(id),
            place: Static::at(Position::new(x, y, 0.0)),
            ..Unit::default()
        };
        u.life.master_id = master_id(4);
        u
    }

    /// A 20 by 20 map of terrain class 1, its objects on the obstruction manager, and group 1 of
    /// them at (10, 10) facing x, spacing 2, two stored points.
    fn front_world(objects: Vec<impl Into<Object>>) -> World {
        use crate::map::spatial::{Obstruction, Placed, Spatial, Tiles};
        use crate::map::terrain::{Terrain, TerrainKind};
        let (units, parts) = World::split_layers(objects.into_iter().map(Into::into));
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![Player {
                objects: units.clone().into(),
                ..Player::default()
            }],
        )
        .with_parts(parts);
        t.model.masters = vec![Arc::new(Masters::new([Master {
            id: master_id(4),
            radius: Footprint::new(0.4, 0.4),
            terrain_restriction: Some(RestrictionId::named(0)),
            ..Master::default()
        }]))];
        t.spatial = Spatial::with_tiles(
            GridSize::new(20, 20),
            Tiles::new(GridSize::new(20, 20), vec![1; 400]),
        );
        let kind = TerrainKind {
            shown: true,
            flags: TerrainFlags::WATER,
            forest: false,
        };
        t.spatial.terrain = Some(Terrain::new(
            GridSize::new(20, 20),
            vec![TileTerrain::of_save(0, 255, -1); 400],
            vec![kind],
            vec![Restriction::new(vec![1.0])],
        ));
        for u in &units {
            t.spatial.insert(
                u.id,
                Placed {
                    position: u.position(),
                    radius: Footprint::new(0.4, 0.4),
                    class: class::CLASS_6,
                    player: PlayerId::new(0),
                    master: master_id(4),
                    obstruction_kind: ObstructionKind::NONE,
                    buildable_on: BuildableOn::NO,
                    tall: true,
                    moves: false,
                    unknown: None,
                    obstruction: Some(Obstruction {
                        kind: ObstructionKind::CIRCLE,
                        class: ObstructionClass::UNIT,
                    }),
                },
            );
        }
        let mut g = UnitGroup::new(group_id(1));
        g.members = units
            .iter()
            .map(|u| Member {
                id: u.id,
                ..Member::default()
            })
            .collect();
        g.motion.position[0] = BVector::new(10.0, 0.0, 10.0);
        g.motion.position[1] = BVector::new(1.0, 0.0, 0.0);
        g.motion.flags = MotionFlags::NONE;
        g.desired_spacing = Distance::new(2.0);
        g.stored_positions.points = vec![LinePoint::default(); 2];
        g.stored_positions.length = Distance::new(3.0);
        t.groups = UnitGroups {
            groups: vec![g],
            ..UnitGroups::default()
        };
        t
    }

    fn placed(t: &World) -> (BVector, BVector, usize) {
        let g = &t.groups.groups[0];
        (
            g.motion.position[0],
            g.motion.new_position[0],
            g.stored_positions.points.len(),
        )
    }

    /// `FUN_141025a20`: of the members within half the spacing (1) of the line along the forward
    /// through the group, the one nearest the point 100 ahead, (110, 10): 10 at (15, 10.5),
    /// before 11 at (13, 9.2); 12, far off the line, is nearer that point but not in front. The
    /// group stands on 10's place, its stored points gone, their length kept.
    #[test]
    fn the_group_moves_onto_the_member_in_front_nearest_the_point_ahead() {
        let mut t = front_world(vec![
            unit(10, 15.0, 10.5),
            unit(11, 13.0, 9.2),
            unit(12, 19.0, 15.0),
        ]);
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(
            placed(&t),
            (
                BVector::new(15.0, 0.0, 10.5),
                BVector::new(15.0, 0.0, 10.5),
                0
            )
        );
        assert_eq!(
            t.groups.groups[0].stored_positions.length,
            Distance::new(3.0)
        );
    }

    /// With no member in front the group moves onto the member nearest the point ahead; with
    /// none the world holds nothing changes.
    #[test]
    fn without_member_in_front_the_group_moves_onto_the_nearest() {
        let mut t = front_world(vec![unit(11, 13.0, 12.0), unit(12, 19.0, 15.0)]);
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(
            placed(&t),
            (
                BVector::new(19.0, 0.0, 15.0),
                BVector::new(19.0, 0.0, 15.0),
                0
            )
        );
        let mut t = front_world(Vec::<Object>::new());
        t.groups.groups[0].members = vec![Member {
            id: object_id(99),
            ..Member::default()
        }];
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(
            placed(&t),
            (BVector::new(10.0, 0.0, 10.0), BVector::ZERO, 2)
        );
        // a group the world does not hold is left alone
        t.move_position_to_front(group_id(2), Distance::new(100.0))
            .unwrap();
    }

    /// A staggered lock (7) takes the whole spacing: 11 at 1.5 from the line is in front of it
    /// (and of no other lock), 12 nearer the point ahead only in the other.
    #[test]
    fn staggered_lock_takes_the_whole_spacing() {
        let units = vec![unit(11, 13.0, 11.5), unit(12, 19.0, 13.0)];
        let mut t = front_world(units.clone());
        t.groups.groups[0].formation_locked_kind = Some(FormationKind::Staggered);
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(placed(&t).0, BVector::new(13.0, 0.0, 11.5));
        let mut t = front_world(units);
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(placed(&t).0, BVector::new(19.0, 0.0, 13.0));
    }

    /// [`MotionFlags::NEW`] take the line through the members' centre, (13, 0, 12): 11 at
    /// (13, 12.2) lies on it, 10 at (6, 10.1) on the group's own line.
    #[test]
    fn with_flags_bit_1_the_line_runs_through_the_members_centre() {
        let units = vec![
            unit(10, 6.0, 10.1),
            unit(11, 13.0, 12.2),
            unit(12, 20.0, 13.7),
        ];
        let mut t = front_world(units.clone());
        t.groups.groups[0].motion.flags = MotionFlags::NEW;
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(placed(&t).0, BVector::new(13.0, 0.0, 12.2));
        let mut t = front_world(units);
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(placed(&t).0, BVector::new(6.0, 0.0, 10.1));
    }

    /// A flanked group (8) whose lead could stand on the group's place among the members stays
    /// there, its stored points kept: member 11's record holds the place (grown by half the
    /// lead's radius, 0.2) but the members are ignored. Object 50 of no member there sends it to
    /// the front: of 10 and 11, both in front, 10 is nearer the point ahead.
    #[test]
    fn flanked_group_stays_where_its_lead_could_stand() {
        // the lead moves (slot 0x270 is `FUN_141158940`), where it last moved to its place
        let lead = unit(10, 15.0, 10.5);
        let mut mover = walker();
        mover.moved_at = lead.position();
        let units = vec![Object::moving(lead, mover), unit(11, 10.2, 10.1).into()];
        let flanked = |t: &mut World| {
            t.groups.groups[0].formation_locked_kind = Some(FormationKind::Flanked);
            t.groups.groups[0].motion.lead = Some(object_id(10));
        };
        let mut t = front_world(units.clone());
        flanked(&mut t);
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(
            placed(&t),
            (
                BVector::new(10.0, 0.0, 10.0),
                BVector::new(10.0, 0.0, 10.0),
                2
            )
        );
        let mut other = units[1].clone();
        other.unit.id = object_id(50);
        let mut t = front_world(vec![units[0].clone(), units[1].clone(), other]);
        flanked(&mut t);
        t.groups.groups[0].members.truncate(2);
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(
            placed(&t),
            (
                BVector::new(15.0, 0.0, 10.5),
                BVector::new(15.0, 0.0, 10.5),
                0
            )
        );
    }

    /// The lead finds the world's object of its id (`FUN_1410465d0`): a moving one (a missile,
    /// say) could stand on the group's place among the members, so a flanked group stays; a
    /// ghost's slot 0x270 (`FUN_141071e30`) is 0, so the group goes to the front.
    #[test]
    fn flanked_group_tests_the_world_object_of_its_lead_id() {
        let units = vec![unit(10, 15.0, 10.5), unit(11, 10.2, 10.1)];
        let flanked = |t: &mut World| {
            t.groups.groups[0].formation_locked_kind = Some(FormationKind::Flanked);
            t.groups.groups[0].members.truncate(2);
            t.groups.groups[0].motion.lead = Some(object_id(-2));
        };
        let missile = Object::moving(unit(-2, 3.0, 3.0), walker());
        let mut t = front_world(vec![
            units[0].clone().into(),
            units[1].clone().into(),
            missile,
        ]);
        flanked(&mut t);
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(
            placed(&t),
            (
                BVector::new(10.0, 0.0, 10.0),
                BVector::new(10.0, 0.0, 10.0),
                2
            )
        );
        let mut t = front_world(units);
        flanked(&mut t);
        t.players[0].ghosts = vec![ghost_at(-2, 3.0, 3.0)];
        t.move_position_to_front(group_id(1), Distance::new(100.0))
            .unwrap();
        assert_eq!(
            placed(&t),
            (
                BVector::new(15.0, 0.0, 10.5),
                BVector::new(15.0, 0.0, 10.5),
                0
            )
        );
    }

    /// `FUN_141039680`: slot 0x648 of a member that moves is its forward, of one that does not
    /// (0, 0, 0): (0, 0, 1) and nothing average to (0, 0, 0.5), facing (0, 0, 1). Forwards that
    /// cancel out give the first held member's; no member held, x.
    #[test]
    fn the_initial_orientation_averages_the_members_forwards() {
        let facing = |forwards: &[Option<BVector>]| {
            let units: Vec<Object> = forwards
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let u = unit(10 + i as i32, 5.0, 5.0);
                    match f {
                        Some(f) => {
                            let mut mover = walker();
                            mover.forward = *f;
                            Object::moving(u, mover)
                        }
                        None => u.into(),
                    }
                })
                .collect();
            let mut t = front_world(units);
            t.set_initial_orientation(group_id(1)).unwrap();
            t.groups.groups[0].motion.position[1]
        };
        assert_eq!(
            facing(&[Some(BVector::new(0.0, 0.0, 1.0)), None]),
            BVector::new(0.0, 0.0, 1.0)
        );
        assert_eq!(
            facing(&[
                Some(BVector::new(0.0, 0.0, 1.0)),
                Some(BVector::new(0.0, 0.0, -1.0))
            ]),
            BVector::new(0.0, 0.0, 1.0)
        );
        let mut t = front_world(Vec::<Object>::new());
        t.groups.groups[0].members = vec![Member {
            id: object_id(99),
            ..Member::default()
        }];
        t.groups.groups[0].motion.position[1] = BVector::new(0.0, 0.0, 1.0);
        t.set_initial_orientation(group_id(1)).unwrap();
        assert_eq!(
            t.groups.groups[0].motion.position[1],
            BVector::new(1.0, 0.0, 0.0)
        );
    }

    /// The spiral's first points from (10, 10): the leg's count and turns as `FUN_141007630`
    /// takes them (a turn after steps 1, 2, 4, 6, 9, 12, ...).
    #[test]
    fn the_spiral_turns_where_its_step_count_divides_by_the_leg() {
        let mut spiral = Spiral::from(Point::new(10.0, 10.0));
        let mut points = vec![spiral.at];
        for _ in 0..12 {
            spiral.step().unwrap();
            points.push(spiral.at);
        }
        assert_eq!(
            points,
            [
                Point::new(10.0, 10.0),
                Point::new(10.0, 11.0),
                Point::new(11.0, 11.0),
                Point::new(11.0, 10.0),
                Point::new(11.0, 9.0),
                Point::new(10.0, 9.0),
                Point::new(9.0, 9.0),
                Point::new(9.0, 10.0),
                Point::new(9.0, 11.0),
                Point::new(9.0, 12.0),
                Point::new(10.0, 12.0),
                Point::new(11.0, 12.0),
                Point::new(12.0, 12.0),
            ]
        );
    }
}
