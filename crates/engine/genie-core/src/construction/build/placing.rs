//! The placement on a foundation no builder started (state 7), a builder's joining and its
//! work, the walk off the footprint (state 223) and back.

use crate::GenieError;
use crate::construction::FootprintWaits;
use crate::construction::build::BuildState;
use crate::construction::build::{
    BUILDER_UNMOVING, BuildAction, BuildReads, ClearingSite, EXTRA_BUILDER, FOREIGN_SITE,
    SITE_GONE, WONDER_RATE, footprint_clearing,
};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::Tile;
use crate::map::spatial::Placed;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::class;
use crate::objects::masters::ids::WONDER;
use crate::objects::unit::Unit;
use crate::world::Seconds;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

/// The radius a part of a building must pass to count as an annex the builder stands on
/// (`0x140aae200` case 7).
const EPSILON: f32 = crate::math::image::build::EPSILON;

impl BuildAction {
    /// `set_state(7)` (`0x140aae200` case 7) on a foundation no builder started: clear the
    /// footprint of what may be built over (`FUN_1415c3620` with `param_6`,
    /// [`footprint_clearing`]: the records of what goes go first, then the objects,
    /// [`crate::world::World::clear_object`]); test whether the foundation may stay (its master's
    /// slot 0x50, [`crate::world::World::foundation_code`]); if it may, send the units standing
    /// on it off but this builder (`FUN_14123bb20`, [`crate::world::World::evacuate`]; those just
    /// cleared are in state 7, which the test passes over), and send this builder off it if it
    /// stands there itself. Its main building and the annexes are tested first
    /// ([`BuildAction::part_tests`]). Another player's marks on the footprint
    /// ([`Spatial::foreign_marks`]) remove a human owner's foundation
    /// ([`BuildAction::remove_marked`]) and halt the Build; so does a part or a foundation that may
    /// not stay (`set_state(2)`).
    ///
    /// # Errors
    ///
    /// Another player's foundation, and what the clearing, the send-off and the removal do not
    /// model.
    ///
    /// [`Spatial::foreign_marks`]: crate::map::spatial::Spatial::foreign_marks
    pub(super) fn place(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
    ) -> Result<(), GenieError> {
        let desolid = Self::site_unit(c, reads)?
            .ok_or(FOREIGN_SITE)?
            .construction
            .is_some_and(|k| k.desolid);
        // every set_state first takes the builder out of FUN_140aaddb0's building's builders
        self.uncount(c)?;
        self.move_to = None;
        self.state = BuildState::Place;
        if !desolid {
            return self.join(c, builder);
        }
        let site = self.target.ok_or(SITE_GONE)?;
        let placed = *c.w.spatial.get_known(site)?.ok_or(SITE_GONE)?;
        let clearing = ClearingSite {
            player: placed.player,
            master: placed.master,
            class: placed.class,
            radius: placed.radius,
            at: placed.position.ground(),
            trees: true,
        };
        let cleared = footprint_clearing(&c.w.spatial, &clearing, &reads.looks)?;
        for cl in cleared.iter().filter(|cl| cl.record_goes) {
            c.w.pathing.removed(cl.id);
            c.w.spatial.drop_record(cl.id);
        }
        c.w.pathing.tiles_changed(c.w.spatial.take_changes());
        let cleared_ids: Vec<ObjectId> = cleared.iter().map(|cl| cl.id).collect();
        let occupants = site_occupants(c, site, &placed, &cleared_ids, reads)?;
        for cl in cleared {
            c.w.clear_object(cl, true)?;
        }
        let parts = self.part_tests(c, builder, (site, &cleared_ids), reads)?;
        // the foundation's placement test (its master's slot 0x50, FUN_14123aee0): one that may
        // not stay, or follows a part that may not, sends no unit off
        let own = with_annexes(c, site);
        // the builder lent to its slot, where the tests' scans meet it (FD-045-D32)
        let code = self.lend(c, builder, |w, _, _| w.foundation_code(&placed, true, &own))?;
        let stays = parts.stay && code.is_allowed();
        let others: Vec<ObjectId> = occupants
            .iter()
            .copied()
            .filter(|&id| id != builder.id)
            .collect();
        if stays && !others.is_empty() {
            c.w.evacuate(site, &others)?;
        }
        if c.w.spatial.foreign_marks(&placed) {
            // FUN_1411d4b80: a human owner's foundation goes (FUN_1411ccbb0), the interface hears
            // of it (FUN_140c898a0 0x88, not held), and set_state(2) halts the Build: the
            // standing sprite, the foundation let go, no point
            self.remove_marked(c)?;
            return self.halt_placing(c, builder, reads);
        }
        if !stays {
            // the interface hears of it (FUN_140c898a0 0x88, not held), and set_state(2) halts
            // the Build
            return self.halt_placing(c, builder, reads);
        }
        // the foundation's FUN_14123bb20 found no unit: the builder joins, or waits while units
        // stand on a part
        if occupants.is_empty() {
            if parts.occupied {
                return self.wait_or_halt(c, builder, reads);
            }
            return self.join(c, builder);
        }
        let (bx, by) = (builder.position().x(), builder.position().y());
        let (sx, sy) = (placed.position.x(), placed.position.y());
        let inside = (bx - sx).abs() <= placed.radius.x() && (by - sy).abs() <= placed.radius.y();
        if inside {
            return self.clear(c, builder);
        }
        if let Some(annex) = self.annex_beneath(c, builder, placed.radius)? {
            // set_state(223) with the annex as the target: the walk off the annex's footprint;
            // the foundation is the target again
            let site = self.target.replace(annex);
            let walk = self.clear(c, builder);
            self.target = site;
            return walk;
        }
        self.wait_or_halt(c, builder, reads)
    }

    /// The tests before the foundation's own in `0x140aae200` case 7: its main building's
    /// (`+0xbb8`, of a foundation that is an annex) and the main's annexes' (`+0xbc0` to
    /// `+0xbe0`, the foundation among them), then the foundation's own annexes', each by its
    /// master's slot 0x50 ([`crate::world::World::foundation_code`]) at its own place. While no
    /// part so far failed, the units on a part that may stay are sent off but this builder
    /// (`FUN_14123bb20`, [`crate::world::World::evacuate`]). The annexes go in id order, the
    /// order of their slots.
    ///
    /// # Errors
    ///
    /// A part gone from the map, and what the test and the send-off do not model.
    fn part_tests(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        (site, cleared): (ObjectId, &[ObjectId]),
        reads: &BuildReads,
    ) -> Result<PartTests, GenieError> {
        let mut parts = Vec::new();
        if let Some(main) = c.w.spatial.annex_of(site) {
            parts.push(main);
            parts.extend(c.w.spatial.annexes_of(main));
        }
        parts.extend(c.w.spatial.annexes_of(site));
        let mut tests = PartTests {
            stay: true,
            occupied: false,
        };
        for part in parts {
            let placed = *c.w.spatial.get_known(part)?.ok_or(SITE_GONE)?;
            let own = with_annexes(c, part);
            let (_, unit) = c.w.object(part).ok_or(SITE_GONE)?;
            let desolid = unit.construction.is_some_and(|k| k.desolid);
            let code = self.lend(c, builder, |w, _, _| {
                w.foundation_code(&placed, desolid, &own)
            })?;
            if !code.is_allowed() {
                tests.stay = false;
            } else if tests.stay {
                let others: Vec<ObjectId> = site_occupants(c, part, &placed, cleared, reads)?
                    .into_iter()
                    .filter(|&id| id != builder.id)
                    .collect();
                if !others.is_empty() {
                    c.w.evacuate(part, &others)?;
                    tests.occupied = true;
                }
            }
        }
        Ok(tests)
    }

    /// The first of the foundation's annexes whose radii are both above [`EPSILON`] and whose
    /// centre the builder stands nearer than the foundation's radii `radius` to, each way
    /// (`0x140aae200` case 7, the annexes in id order).
    ///
    /// # Errors
    ///
    /// An annex gone from the map.
    fn annex_beneath(
        &self,
        c: &Step<'_>,
        builder: &Unit,
        radius: Footprint,
    ) -> Result<Option<ObjectId>, GenieError> {
        let (bx, by) = (builder.position().x(), builder.position().y());
        // a Build without its foundation has no annexes
        let Some(site) = self.target else {
            return Ok(None);
        };
        for annex in c.w.spatial.annexes_of(site) {
            let placed = c.w.spatial.get_known(annex)?.ok_or(SITE_GONE)?;
            let (ax, ay) = (placed.position.x(), placed.position.y());
            let sized = EPSILON < placed.radius.x() && EPSILON < placed.radius.y();
            if sized && (bx - ax).abs() < radius.x() && (by - ay).abs() < radius.y() {
                return Ok(Some(annex));
            }
        }
        Ok(None)
    }

    /// `LAB_140aaf5d4`: the builder waits a second (`set_state` 222) up to 21 times; then the
    /// interface hears of it (`FUN_140c898a0` 0x88, not held), `set_state(2)` halts the Build
    /// (the standing sprite, the foundation let go, no point) and the waits start again.
    ///
    /// # Errors
    ///
    /// The halt's.
    fn wait_or_halt(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
    ) -> Result<(), GenieError> {
        if FootprintWaits::MOST <= builder.footprint_waits {
            builder.footprint_waits = FootprintWaits::NONE;
            return self.halt_placing(c, builder, reads);
        }
        builder.footprint_waits = builder.footprint_waits.plus_one();
        self.state = BuildState::Wait;
        self.timer = Seconds::new(1.0);
        Ok(())
    }

    /// `set_state(2)` from the placement: the builder takes its standing sprite, the
    /// foundation is let go, the point is −1 (the builder counts among no builders here, its
    /// `MoveTo` went).
    ///
    /// # Errors
    ///
    /// The sprite's and the release's.
    fn halt_placing(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
    ) -> Result<(), GenieError> {
        Self::stand(c, builder)?;
        let target = self.target.take();
        crate::world::lets::release(c, builder, target, reads.by)?;
        self.point = Position::from([-1.0; 3]);
        self.state = BuildState::Halted;
        Ok(())
    }

    /// `LAB_140aaf9f4`: the builder joins the work: unless walls of other players stand on both
    /// tiles beside it on the footprint's diagonal (`set_state` 2, not modelled), it counts among
    /// the builders of `FUN_140aaddb0`'s building ([`BuildAction::count`]). Its work sprite and
    /// sound are not held by the slice.
    ///
    /// # Errors
    ///
    /// A foundation gone from the map, the walls beside a diagonal builder, and the count's.
    pub(super) fn join(&mut self, c: &mut Step<'_>, builder: &Unit) -> Result<(), GenieError> {
        let placed = *c.w.spatial.get(self.target).ok_or(SITE_GONE)?;
        let [bx, by] = [builder.position().x() as i32, builder.position().y() as i32];
        let [tx, mut ty] = [placed.position.x() as i32, placed.position.y() as i32];
        let (rx, ry) = (placed.radius.x(), placed.radius.y());
        if (rx as i32) as f32 == rx && (ry as i32) as f32 == ry {
            if by < ty {
                if tx < bx {
                    ty -= 1;
                }
            } else if ty < by && bx < tx {
                ty -= 1;
            }
        }
        let (dx, dy) = (bx - tx, by - ty);
        if dx.abs() == dy.abs() {
            let (a, b) = beside_diagonal(bx, by, dx, dy);
            let player = placed.player;
            let walled = |t: Tile| {
                c.w.spatial
                    .object_on_tile(t, |o| o.class == class::WALL && o.player != player)
            };
            if walled(a) && walled(b) {
                return Err(GenieError::unsupported(
                    "walls beside a diagonal builder (set_state 2)",
                ));
            }
        }
        if !self.added {
            Self::count(c, self.target, 1)?;
            self.added = true;
        }
        Ok(())
    }

    /// The work (state 7, `0x140ab0370` case 6): the builder adds its rate times dt to the
    /// foundation's construction: its work rate times the task's work value, times the owner's
    /// construction rate (attribute 195) when that is above 0 (the Wonder's at most 1.2), each
    /// builder after the first adding a third ([`BuildAction::work_on`]). A finished
    /// foundation with hit points then ends the work ([`BuildAction::finish`]). A farm's
    /// reseeding and the instant-build cheat (`game+0xc14`) are not modelled.
    ///
    /// # Errors
    ///
    /// Another player's foundation, and what the work and its end do not model.
    pub(super) fn work(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
    ) -> Result<(), GenieError> {
        let site = Self::site_unit(c, reads)?.ok_or(FOREIGN_SITE)?;
        let mut rate = self.task.work_rate * self.task.work_value * c.dt.seconds();
        let mut factor = c.w.players.player(c.player)?.construction_rate().get();
        if site.life.master_id == WONDER && WONDER_RATE < factor {
            factor = WONDER_RATE;
        }
        if 0.0 < factor {
            rate *= factor;
        }
        let n = Self::builders(c, self.target)?.as_f32();
        if 1.0 < n {
            rate *= ((n - 1.0) * EXTRA_BUILDER + 1.0) / n;
        }
        if self.work_on(c, builder, rate)? {
            return Ok(());
        }
        // a finished foundation with hit points ends the work (its `+0x278`, decoded); one
        // without (deleted, it dies in its own update) leaves the Build as it is
        if Self::site_unit(c, reads)?.is_some_and(|u| HitPoints::ZERO < u.life.hp) {
            self.finish(c, builder, reads)?;
        }
        Ok(())
    }

    /// `set_state(223)`: a quiet `MoveTo` to the free tile beside the footprint (`FUN_1411566b0`,
    /// `FUN_140aa0d30` with the task's moving sprite, [`MoveToReads::to_point`]); it starts in
    /// state 3, as a `MoveTo` to a point is far from it (`FUN_140aa2100`), in 13 for a unit
    /// without speed.
    ///
    /// # Errors
    ///
    /// A builder without movement state or its master, a foundation gone from the map, no free
    /// tile beside it, and a sprite the table lacks.
    ///
    /// [`MoveToReads::to_point`]: crate::movement::move_to::MoveToReads::to_point
    pub(super) fn clear(&mut self, c: &mut Step<'_>, builder: &Unit) -> Result<(), GenieError> {
        let mover = builder.body.mover().ok_or(BUILDER_UNMOVING)?;
        let site = c.w.spatial.get(self.target).ok_or(SITE_GONE)?;
        let site = (site.position, site.radius);
        let at = builder.position();
        let spot =
            c.w.spatial
                .spot_beside(
                    at,
                    (builder.id, mover.stats.radius.x()),
                    mover.stats.restriction_mask,
                    site,
                )?
                .ok_or(GenieError::unsupported(
                    "no free tile beside the footprint (set_state 2)",
                ))?;
        self.state = BuildState::Clear;
        let move_reads = builder.move_to_reads(&c.w.masters, c.player, &c.w.graphics)?;
        self.move_to =
            move_reads.to_point(spot.with_height(at.z()), Distance::ZERO, self.task.moving)?;
        Ok(())
    }
}

/// What the tests of a foundation's main building and annexes found ([`BuildAction::part_tests`]).
struct PartTests {
    /// Every part may stay (`bVar5`).
    stay: bool,
    /// Units stood on a part that may stay (`bVar21`).
    occupied: bool,
}

/// The objects a placement test of building `id` passes over (`p12`): the building and its
/// annexes, theirs too (`FUN_14123d090`, through the four slots `+0xbc0` to `+0xbe0`).
fn with_annexes(c: &Step<'_>, id: ObjectId) -> Vec<ObjectId> {
    let mut own = vec![id];
    let mut next = 0;
    while let Some(&part) = own.get(next) {
        own.extend(c.w.spatial.annexes_of(part));
        next += 1;
    }
    own
}

/// The units the game counts on `site`'s footprint ([`Spatial::footprint_occupants`]) but those
/// just cleared off it (state 7, which `FUN_14123bb20` passes over).
///
/// # Errors
///
/// A footprint on a part of the map the world does not know.
///
/// [`Spatial::footprint_occupants`]: crate::map::spatial::Spatial::footprint_occupants
fn site_occupants(
    c: &Step<'_>,
    site: ObjectId,
    placed: &Placed,
    cleared: &[ObjectId],
    reads: &BuildReads,
) -> Result<Vec<ObjectId>, GenieError> {
    let obstructs = placed.obstruction_type.recorded();
    Ok(c.w
        .spatial
        .footprint_occupants(site, placed, obstructs, &reads.looks)?
        .into_iter()
        .filter(|id| !cleared.contains(id))
        .collect())
}

/// The two tiles beside a builder on a footprint's diagonal (`LAB_140aafc09`'s arguments).
pub(super) const fn beside_diagonal(bx: i32, by: i32, dx: i32, dy: i32) -> (Tile, Tile) {
    if dx < 1 {
        if dx < 0 && dy < 0 {
            (Tile::new(bx + 1, by), Tile::new(bx, by + 1))
        } else if dx < 0 && 0 < dy {
            (Tile::new(bx, by - 1), Tile::new(bx + 1, by))
        } else {
            (Tile::new(bx - 1, by), Tile::new(bx, by + 1))
        }
    } else if dy < 1 {
        (Tile::new(bx - 1, by), Tile::new(bx, by + 1))
    } else {
        (Tile::new(bx, by - 1), Tile::new(bx - 1, by))
    }
}
