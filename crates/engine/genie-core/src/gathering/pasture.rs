//! A pasture's livestock (`FUN_1411c8410`, `SetupPasture`): the completion's first part
//! (`FUN_1411c6bd0`) of a pasture (masters 1893 and 1897, class 49) whose build points reached its
//! build time puts its owner's livestock on points along its edges, each holding its share of the
//! pasture's food.

use crate::GenieError;
use crate::gathering::PastureId;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::math::msvc::sort;
use crate::math::rng::Stream;
use crate::movement::footprint_gap_squared;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::SpriteFlags;
use crate::objects::WorkerCount;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::players::PlayerId;
use crate::technology::Attributes;
use crate::world::World;
use crate::world::layer::MasterSlots;
use crate::world::layer::generated::PastureTuning;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;

/// The livestock a pasture holds (`DAT_143f51db0`, which the static constructor `FUN_140174900`
/// fills), in the table's order a draw picks from: the Argali and the Ibex (Pasture), the
/// build's `pasture_argali` and `pasture_ibex`.
#[must_use]
pub const fn livestock(masters: MasterSlots) -> [MasterId; 2] {
    [masters.pasture_argali, masters.pasture_ibex]
}

/// The points along the edges of a pasture at `at` whose ground radius on x is `radius_x`, in
/// the routine's order: the radius less `PastureTuning::inset` gives the corners (xâr, yâr), (x+r, yâr),
/// (x+r, y+r) and (xâr, y+r); each edge from its first corner to its second, `(b â a) Â· t + a`
/// in f32 (SUBSS, MULSS, ADDSS) with `t` from 0 by `PastureTuning::step` while at most `PastureTuning::end`: the bottom
/// edge from the first corner to the second, the right one from the third to the second, the
/// top one from the third to the fourth and the left one from the first to the fourth.
#[must_use]
pub fn edge_points(at: Position, radius_x: Distance, pasture_tuning: &PastureTuning) -> Vec<Point> {
    let r = radius_x - Distance::new(pasture_tuning.inset);
    let (x, y) = (at.x(), at.y());
    let c = [
        Point::new(x - r.get(), y - r.get()),
        Point::new(x + r.get(), y - r.get()),
        Point::new(x + r.get(), y + r.get()),
        Point::new(x - r.get(), y + r.get()),
    ];
    let edges = [(c[0], c[1]), (c[2], c[1]), (c[2], c[3]), (c[0], c[3])];
    let mut points = Vec::new();
    for (a, b) in edges {
        let mut t = 0.0f32;
        // COMISS END, t with JNC: on while END is at least t
        while t <= pasture_tuning.end {
            points.push(Point::new(
                (b.x() - a.x()) * t + a.x(),
                (b.y() - a.y()) * t + a.y(),
            ));
            t += pasture_tuning.step;
        }
    }
    points
}

/// A draw of the Object stream modulo `n` (`CDQE` then `DIV`: the draw is at most 0x7fff).
fn draw_below(turns: &mut World, n: usize) -> usize {
    turns.streams.get(Stream::Object).rand15() as usize % n
}

impl World {
    /// `FUN_1411c8410` on player `p`'s pasture `id`: the points along its edges ([`edge_points`]
    /// with its master's ground radius on x), its owner's
    /// [`AttributeSlots::pasture_livestock`](crate::world::layer::AttributeSlots::pasture_livestock)
    /// cut to an integer (CVTTSS2SI) as the count and
    /// [`AttributeSlots::pasture_food`](crate::world::layer::AttributeSlots::pasture_food) over it
    /// (DIVSS) as each one's food. Up to the count, while points are left: a draw of the Object
    /// stream picks the master ([`livestock`], `SetupPasture` line 0x1fa4), a second the point
    /// (0x1fa6). A point off the map (`FUN_140f5ec30`) or on a tile whose objects (its list,
    /// `+0x28`) hold a livestock master of this pasture (`+0x790`) goes, and a draw picks another
    /// (0x1fb2); none left ends it. On the point the owner's slot 0x1c0 makes the master at the
    /// pasture's height ([`World::make_new_object`], no footprint): the point goes, the pasture
    /// lists it ([`crate::objects::layers::Building::livestock`]), its state is 2 (slot 0xa0 with
    /// 2: it is already), no unit targets it (`+0x689` 0,
    /// [`crate::objects::unit::Unit::untargetable`]), it is the pasture's (`+0x790`) and holds its
    /// food (`+0x310`); its current sprite with a random first facet draws a facet below its facets
    /// less one (0x1fd2, IDIV). Each attempt counts, a master the owner has not got too. Not held:
    /// its selection flag (`+0x68b` 0, slot 0x6b0) and the drawn facet (`+0xe`, `FUN_14119d490`:
    /// the moving set-sprite derives it from the angle again, `FUN_141158620` -> `FUN_14114d6c0`,
    /// its dying sprite's included).
    ///
    /// # Errors
    ///
    /// Its master or attributes the player has not got, a facet draw on a sprite of one facet
    /// (the game divides by zero), and what the object's making does not model.
    pub(crate) fn setup_pasture(&mut self, p: PlayerId, id: PastureId) -> Result<(), GenieError> {
        let (at, master) = self
            .object(id)
            .map(|(_, u)| (u.position(), u.life.master_id))
            .ok_or(GenieError::unreachable("a pasture its completion names"))?;
        let radius_x = self.master(p, master)?.radius.x();
        let mut points = edge_points(at, Distance::new(radius_x), &self.model.tuning.pasture);
        let owner = self.players.player(p)?;
        let count =
            cvttss2si(Attributes::get(owner, self.model.slots.attribute.pasture_livestock)?.get());
        let food = Attributes::get(owner, self.model.slots.attribute.pasture_food)? / count as f32;
        let map = self.spatial.map;
        let mut made = 0;
        while made < count && !points.is_empty() {
            let kinds = livestock(self.model.slots.master);
            let kind = *kinds.checked(draw_below(self, kinds.len()))?;
            let mut i = draw_below(self, points.len());
            loop {
                let (x, y) = {
                    let p = points.checked(i)?;
                    (p.x(), p.y())
                };
                let (tx, ty) = (cvttss2si(x), cvttss2si(y));
                let on_map = 0 <= tx && tx < map.width() && 0 <= ty && ty < map.height();
                if on_map && !self.holds_livestock_of(id, Tile::new(tx, ty)) {
                    break;
                }
                let _ = points.remove(i);
                if points.is_empty() {
                    return Ok(());
                }
                i = draw_below(self, points.len());
            }
            let (x, y) = {
                let p = points.checked(i)?;
                (p.x(), p.y())
            };
            if let Some(new) = self.make_new_object(p, kind, Position::new(x, y, at.z()), false)? {
                let _ = points.remove(i);
                if let Some((_, layer)) = self.object_and_layer_mut(id)? {
                    layer.livestock.push(new);
                }
                let (_, u) = self
                    .object_mut(new)
                    .ok_or(GenieError::unreachable("the livestock just made"))?;
                u.untargetable = true;
                u.pasture = Some(id);
                u.life.amount = food;
                let graphic = u.sprite.map(|s| s.graphic);
                if let Some(g) =
                    graphic.filter(|g| g.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME))
                {
                    let d = self.streams.get(Stream::Object).rand15() as i32;
                    let _ =
                        d.checked_rem(g.facets.counted() - 1)
                            .ok_or(GenieError::unreachable(
                                "a random first facet of a sprite of one facet",
                            ))?;
                }
            }
            made += 1;
        }
        Ok(())
    }

    /// A pasture's slot 0x780 (`FUN_1411c5dd0`): the workers (`+0x644`) of its livestock
    /// ([`crate::objects::layers::Building::livestock`]) the world holds, summed; 0 for an object the world does
    /// not hold.
    pub(crate) fn livestock_workers(&self, pasture: PastureId) -> WorkerCount {
        WorkerCount::of_data(self.object(pasture).map_or(0, |(_, b)| {
            self.parts
                .building_of(b.id)
                .livestock
                .iter()
                .filter_map(|&id| self.object(id))
                .map(|(_, l)| l.workers.get())
                .sum()
        }))
    }

    /// `FUN_1411c7670`: the livestock of pasture `pasture` that player `p`'s unit at `idx` tends,
    /// â1 for none (a unit of another player, too): its livestock the world holds with their
    /// workers and their footprints' squared gap to the unit (`FUN_14118f8c0`), sorted by the gap
    /// (`std::sort`, [`crate::math::msvc::sort`]); the first without workers, else the first whose
    /// workers are below the owner's
    /// [`AttributeSlots::pasture_workers`](crate::world::layer::AttributeSlots::pasture_workers).
    ///
    /// # Errors
    ///
    /// A master the players have not got, or an attribute the owner has not got.
    pub(crate) fn pasture_pick(
        &self,
        pasture: PastureId,
        p: PlayerId,
        idx: ListIndex,
    ) -> Result<Option<ObjectId>, GenieError> {
        let Some((q, b)) = self.object(pasture) else {
            return Ok(None);
        };
        if q != p {
            return Ok(None);
        }
        let unit = &self.players.live(p, idx)?;
        let reach = unit.master(self.master_tables(), p)?.radius;
        let mut tended = Vec::new();
        for &id in &self.parts.building_of(b.id).livestock {
            let Some((r, l)) = self.object(id) else {
                continue;
            };
            let radius = l.master(self.master_tables(), r)?.radius;
            let gap = footprint_gap_squared(l.position(), radius, unit.position(), reach);
            tended.push((id, l.workers, gap));
        }
        sort(&mut tended, &|a, b| a.2 < b.2)?;
        let room = Attributes::get(
            self.players.player(q)?,
            self.model.slots.attribute.pasture_workers,
        )?;
        let mut pick = None;
        for (id, workers, _) in tended {
            if workers == WorkerCount::ZERO {
                return Ok(Some(id));
            }
            if pick.is_none() && workers.as_f32() < room.get() {
                pick = Some(id);
            }
        }
        Ok(pick)
    }

    /// `FUN_141189ca0`: player `p`'s unit at `idx` tends `pasture`, none (â1) to tend none. The
    /// building it names (the one the unit tends, `+0x794`, for none; none for an object that is
    /// no building) takes the unit last among its workers unless it lists it already, or for none
    /// drops it
    /// (`FUN_1411cafa0`, [`crate::objects::layers::Building::pasture_workers`]); then the unit tends `pasture`
    /// ([`crate::objects::unit::Unit::my_pasture`]).
    ///
    /// # Errors
    ///
    /// A master the players have not got.
    pub(crate) fn tend_pasture(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        pasture: Option<PastureId>,
    ) -> Result<(), GenieError> {
        let unit = &self.players.live(p, idx)?;
        let (id, named) = (unit.id, pasture.or(unit.my_pasture));
        let building = match self.object(named) {
            Some((q, b)) => b.master(self.master_tables(), q)?.kind == MasterKind::BUILDING,
            None => false,
        };
        if building && let Some((_, layer)) = self.object_and_layer_mut(named)? {
            if pasture.is_none() {
                layer.pasture_workers.retain(|&w| w != id);
            } else if !layer.pasture_workers.contains(&id) {
                layer.pasture_workers.push(id);
            }
        }
        self.players.live_mut(p, idx)?.my_pasture = pasture;
        Ok(())
    }

    /// Whether the objects on `tile` (its list, `+0x28`) hold a livestock master ([`livestock`])
    /// of the pasture `pasture` (`+0x790`). A ghost on it is not of a pasture.
    fn holds_livestock_of(&self, pasture: PastureId, tile: Tile) -> bool {
        let kinds = livestock(self.model.slots.master);
        self.spatial.tile_list(tile).iter().any(|&o| {
            self.object(o).is_some_and(|(_, u)| {
                u.pasture == Some(pasture) && kinds.contains(&u.life.master_id)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::tuning;

    /// 429381190's pasture 15342 (master 1897, radius 2) at (136, 72): sixteen points, four an
    /// edge; its livestock stood on the left edge's second and third (134.5, 71.4), (134.5,
    /// 72.3) and the bottom edge's fourth (137.2, 70.5).
    #[test]
    fn pasture_edges_take_four_points_each() {
        let points = edge_points(
            Position::new(136.0, 72.0, 0.0),
            Distance::new(2.0),
            &tuning().pasture,
        );
        assert_eq!(points.len(), 16);
        let t = [0.0f32, 0.3, 0.6, 0.3 + 0.3 + 0.3];
        assert_eq!(points[0], Point::new(134.5, 70.5));
        assert_eq!(points[3], Point::new(3.0 * t[3] + 134.5, 70.5));
        assert_eq!(points[3], Point::new(137.2, 70.5));
        // the right edge from (137.5, 73.5) down to (137.5, 70.5)
        assert_eq!(points[4], Point::new(137.5, 73.5));
        assert_eq!(points[5], Point::new(137.5, -3.0 * 0.3 + 73.5));
        // the top edge from (137.5, 73.5) to (134.5, 73.5)
        assert_eq!(points[9], Point::new(-3.0 * 0.3 + 137.5, 73.5));
        // the left edge from (134.5, 70.5) up
        assert_eq!(points[13], Point::new(134.5, 71.4));
        assert_eq!(points[14], Point::new(134.5, 72.3));
    }
}
