//! `bestUnitToAttack`'s statistics ([`Statistics`]): the entries, an object's add, the scores,
//! the ranks and the sort, and the order history's weights.

use crate::map::Distance;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::ClassMask;
use crate::unit_ai::best_target::ORDER_WINDOW;
use crate::unit_ai::best_target::{AttackCategory, ByCategory};
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::slots::CheckedIndex;

/// The statistics' table holds at most this many objects (`FUN_140c66590`: `+0x1668` below
/// 0x80).
pub const STATISTICS: usize = 128;
/// A kill time's stand-in when the unit does no damage to an object it may still work on
/// (`DAT_14330ebb8`), and the current target's bonus.
pub(super) const NO_DAMAGE_KILL: f32 = crate::math::image::best_target::FIVE;
/// The kill time of an object the unit neither damages nor works on (`DAT_14330ebf4`).
pub(super) const NEVER: f32 = crate::math::image::best_target::NEVER;
/// Damage at most this does not count (`DAT_142eb0c60`), nor a span below it.
pub(super) const EPSILON: f32 = crate::math::image::best_target::EPSILON;
/// The scores' scale (`DAT_142eb0f4c`), a distance score's top (`DAT_142efe418`) and the
/// targeting factor's per cent (`DAT_142eb99cc`).
pub(super) const SCORE: f32 = crate::math::image::best_target::SCORE;
pub(super) const DISTANCE_SCORE: f32 = crate::math::image::best_target::DISTANCE_SCORE;
pub(super) const PER_CENT: f32 = crate::math::image::best_target::PER_CENT;
/// The rank's step per tier (`DAT_142eea788`).
pub(super) const TIER: f32 = crate::math::image::best_target::TIER;
/// The classes whose objects stand fixed for the rank (`FUN_140c66590`'s mask
/// `0x12000000000008`: 3, 49, 52).
pub(super) const FIXED_CLASSES: ClassMask = ClassMask::new(0x0012_0000_0000_0008, 0x35);

/// An object in the statistics (0x2c bytes at `DAT_143e74538`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Entry {
    /// `+0`: its id.
    pub id: ObjectId,
    /// `+4`: it moves (its slot 0x208 above 0).
    pub moving: bool,
    /// `+5`: its class is one of [`FIXED_CLASSES`].
    pub fixed: bool,
    /// `+8`: the time the unit takes to kill it (its hit points over the unit's damage to it,
    /// over the unit's reload time).
    pub kill_time: f32,
    /// `+0xc`: the footprints' gap.
    pub distance: Distance,
    /// `+0x10`: its attack category ([`category`]).
    ///
    /// [`category`]: crate::unit_ai::best_target::category
    pub category: Option<AttackCategory>,
    /// `+0x14`: the kill-time score.
    pub kill_score: f32,
    /// `+0x18`: the distance score.
    pub distance_score: f32,
    /// `+0x1c`: the order history's weight of its category.
    pub weight: f32,
    /// `+0x20`: the current target's bonus (5, else 0).
    pub bonus: f32,
    /// `+0x24`: the sum of the four.
    pub total: f32,
    /// `+0x28`: the rank the sort orders by ([`Statistics::rank`]).
    pub rank: f32,
}

/// What `FUN_140c66590` reads of the unit and of an object it adds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    /// The object.
    pub id: ObjectId,
    /// Its footprints' gap to the unit (`FUN_14118f680`).
    pub distance: Distance,
    /// Its attack category (`FUN_140c4c8e0` on it, read where the candidate is made; counted
    /// when not −1).
    pub category: Option<AttackCategory>,
    /// Its master's class.
    pub class: ClassId,
    /// Its top speed (slot 0x208).
    pub speed: Speed,
    /// It is armed: the sum of its attacks above 0 (slot 0x228, `FUN_141084070`), or its unit AI
    /// could work on the unit (its slot 0x1b8 with 1: a priest's conversion test).
    pub armed: bool,
    /// The unit's weapon range against it (slot 0x238).
    pub reach: Distance,
    /// The unit's damage per hit to it (its slot 0x220, `FUN_141083f20`: the unit's elevation
    /// factor and its armor against the unit's attacks).
    pub damage: HitPoints,
    /// Its hit points (`+0x278`).
    pub hp: HitPoints,
    /// The unit's reload time (slot 0x218).
    pub reload: Seconds,
    /// The unit's unit AI could work on it (its slot 0x1b8 with 0: a priest's conversion).
    pub workable: bool,
    /// It is the unit AI's current target (`+0x48`).
    pub current: bool,
}

/// `bestUnitToAttack`'s statistics (the static table at `DAT_143e744d0`).
#[derive(Clone, Debug, PartialEq)]
pub struct Statistics {
    /// `+0`, `+4`: the least and the most distance.
    pub distance: Bounds,
    /// `+8`, `+0xc`: the least and the most kill time.
    pub kill_time: Bounds,
    /// `+0x10`, `+0x14`: the moving objects' least and most distance.
    pub moving_distance: Bounds,
    /// `+0x18`, `+0x1c`: the moving objects' least and most kill time.
    pub moving_kill_time: Bounds,
    /// `+0x20`: the objects other than the current target.
    pub others: i32,
    /// `+0x24`: the moving objects.
    pub moving: i32,
    /// `+0x28`: the armed objects.
    pub armed: i32,
    /// `+0x2c`: the objects within the unit's range against them.
    pub in_reach: i32,
    /// `+0x30`..: the objects per attack category.
    pub by_category: ByCategory<i32>,
    /// `+0x60`, `+0x64`: the closest moving object's distance and id.
    pub closest_moving: (f32, Option<ObjectId>),
    /// `+0x68`.., count `+0x1668`.
    pub entries: Vec<Entry>,
    /// `+0x166c`..: the entries' order, by rank after the sort.
    pub order: Vec<usize>,
}

impl Default for Statistics {
    /// `bestUnitToAttack`'s reset of the table: the least values the largest float, the most
    /// −1, the counts 0, no closest moving object (none at the largest float).
    fn default() -> Self {
        Self {
            distance: Bounds::RESET,
            kill_time: Bounds::RESET,
            moving_distance: Bounds::RESET,
            moving_kill_time: Bounds::RESET,
            others: 0,
            moving: 0,
            armed: 0,
            in_reach: 0,
            by_category: ByCategory::default(),
            closest_moving: (f32::MAX, None),
            entries: Vec::new(),
            order: Vec::new(),
        }
    }
}

/// The least and the most of a value over the table's entries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// The least.
    pub least: f32,
    /// The most.
    pub most: f32,
}

impl Bounds {
    /// `bestUnitToAttack`'s reset: the least the largest float, the most −1.
    pub const RESET: Self = Self {
        least: f32::MAX,
        most: -1.0,
    };

    /// `v` joins the bounds: the least takes it when above it (`COMISS least, v`, `JBE`: a NaN
    /// changes nothing), then the most when below it (`COMISS v, most`, `JBE`).
    pub fn take(&mut self, v: Distance) {
        if self.least > v.get() {
            self.least = v.get();
        }
        if v > Distance::new(self.most) {
            self.most = v.get();
        }
    }

    /// The most less the least.
    #[must_use]
    pub fn span(self) -> f32 {
        self.most - self.least
    }
}

impl Statistics {
    /// `FUN_140c66590`: the object `c` joins the table, while it holds fewer than
    /// [`STATISTICS`] (the caller's test that both have a unit AI, `+0x6b0`): its id, distance
    /// and category; the category's count when not −1; the count of others unless it is the
    /// current target; a moving one counts and may be the closest moving object (`COMISS`, a
    /// strictly nearer one); a fixed class marks it; an armed one counts, as does one within
    /// the unit's reach (`COMISS`, `JC`: a NaN reach does not); the distances' bounds; its kill
    /// time: hit points over damage over the reload time for damage above [`EPSILON`] (`JC`:
    /// a NaN damage too), else 5 for one the unit's AI could work on and the largest float for
    /// another; the kill times' bounds; the current target's bonus 5; a moving one's bounds of
    /// both.
    pub fn add(&mut self, c: &Candidate) {
        if STATISTICS <= self.entries.len() {
            return;
        }
        let mut e = Entry {
            id: c.id,
            distance: c.distance,
            category: c.category,
            ..Entry::default()
        };
        if let Some(k) = c.category {
            self.by_category[k] += 1;
        }
        if !c.current {
            self.others += 1;
        }
        if Speed::ZERO < c.speed {
            e.moving = true;
            if self.closest_moving.0 > c.distance.get() {
                self.closest_moving = (c.distance.get(), Some(c.id));
            }
            self.moving += 1;
        }
        e.fixed = FIXED_CLASSES.contains(c.class);
        if c.armed {
            self.armed += 1;
        }
        if !(c.reach < c.distance || c.reach.get().is_nan() || c.distance.get().is_nan()) {
            self.in_reach += 1;
        }
        self.distance.take(c.distance);
        e.kill_time = if crate::math::fmath::below_or_unordered(EPSILON, c.damage.get()) {
            (c.hp / c.damage) / c.reload.get()
        } else if c.workable {
            NO_DAMAGE_KILL
        } else {
            NEVER
        };
        self.kill_time.take(Distance::new(e.kill_time));
        e.bonus = if c.current { NO_DAMAGE_KILL } else { 0.0 };
        if Speed::ZERO < c.speed {
            self.moving_distance.take(c.distance);
            self.moving_kill_time.take(Distance::new(e.kill_time));
        }
        self.entries.push(e);
    }

    /// `FUN_140c48150`: every entry's scores, with the order history's `weights` by category and
    /// the unit's owner's targeting mode and factor (`player+0x614`, `+0x618`). The kill-time
    /// score's top is 10 (times the factor per cent in mode 1); it falls by the entry's share of
    /// the kill times' span (a moving entry's of the moving objects') when the span is not below
    /// [`EPSILON`] (`COMISS`, `JA`: a NaN span shares). The distance score is 75 less the
    /// entry's share of 75 the same way; a still entry's distance span below the epsilon gives
    /// 75 itself. The weight is the category's times 10 (0 without a category). The total is the
    /// sum with the bonus; in mode 2 an entry whose shown armor (`armored`, slot 0x870's first)
    /// is at least 100 takes the factor per cent of it.
    ///
    /// # Errors
    ///
    /// What `armored` does not model.
    pub fn score(
        &mut self,
        weights: &ByCategory<f32>,
        (mode, factor): (i32, i32),
        armored: &mut dyn FnMut(ObjectId) -> Result<bool, crate::GenieError>,
    ) -> Result<(), crate::GenieError> {
        let kill_span = self.kill_time.span();
        let distance_span = self.distance.span();
        let moving_kill_span = self.moving_kill_time.span();
        let moving_distance_span = self.moving_distance.span();
        let (kill_low, distance_low) = (self.kill_time.least, self.distance.least);
        let (moving_kill_low, moving_distance_low) =
            (self.moving_kill_time.least, self.moving_distance.least);
        for e in &mut self.entries {
            let top = if mode == 1 {
                factor as f32 * PER_CENT
            } else {
                1.0
            };
            let mut kill = top * SCORE;
            if e.moving {
                if crate::math::fmath::at_most_or_unordered(EPSILON, moving_kill_span) {
                    kill -= ((e.kill_time - moving_kill_low) / moving_kill_span) * kill;
                }
                e.kill_score = kill;
                let mut d = DISTANCE_SCORE;
                if crate::math::fmath::at_most_or_unordered(EPSILON, moving_distance_span) {
                    d -= ((e.distance.get() - moving_distance_low) / moving_distance_span)
                        * DISTANCE_SCORE;
                }
                e.distance_score = d;
            } else {
                if crate::math::fmath::at_most_or_unordered(EPSILON, kill_span) {
                    kill -= ((e.kill_time - kill_low) / kill_span) * kill;
                }
                e.kill_score = kill;
                e.distance_score = if EPSILON > distance_span {
                    DISTANCE_SCORE
                } else {
                    DISTANCE_SCORE
                        - ((e.distance.get() - distance_low) / distance_span) * DISTANCE_SCORE
                };
            }
            e.weight = e.category.map_or(0.0, |k| weights[k] * SCORE);
            e.total = ((e.kill_score + e.distance_score) + e.weight) + e.bonus;
            if mode == 2 && armored(e.id)? {
                e.total *= factor as f32 * PER_CENT;
            }
        }
        Ok(())
    }

    /// `bestUnitToAttack`'s ranks: each entry's order slot holds its index, and its rank is its
    /// tier (`tier`) times [`TIER`] plus its total.
    ///
    /// # Errors
    ///
    /// What `tier` does not model.
    pub fn rank(
        &mut self,
        tier: &mut dyn FnMut(&Entry) -> Result<i32, crate::GenieError>,
    ) -> Result<(), crate::GenieError> {
        self.order = (0..self.entries.len()).collect();
        for e in &mut self.entries {
            e.rank = tier(e)? as f32 * TIER + e.total;
        }
        Ok(())
    }

    /// `FUN_140c41830` over the order slots `lo..=hi`: the last one's entry is the pivot; each
    /// slot before it whose entry ranks above it, or the same with a lower id, moves to the
    /// front part (a NaN rank never does); then the pivot follows the front part. Its slot.
    ///
    /// # Errors
    ///
    /// A slot or an entry the sort does not hold.
    pub(super) fn partition(&mut self, lo: usize, hi: usize) -> Result<usize, crate::GenieError> {
        let pivot = *self.entries.checked(*self.order.checked(hi)?)?;
        let mut i = lo;
        for j in lo..hi {
            let e = *self.entries.checked(*self.order.checked(j)?)?;
            if pivot.rank < e.rank || (e.rank == pivot.rank && e.id < pivot.id) {
                self.order.swap(i, j);
                i += 1;
            }
        }
        self.order.swap(i, hi);
        Ok(i)
    }

    /// `FUN_140c41b40` over the order slots `lo..=hi` (the quicksort, its second half by the
    /// loop).
    ///
    /// # Errors
    ///
    /// A slot or an entry the sort does not hold.
    pub(super) fn quicksort(&mut self, mut lo: usize, hi: usize) -> Result<(), crate::GenieError> {
        while lo < hi {
            let p = self.partition(lo, hi)?;
            if p > 0 {
                self.quicksort(lo, p - 1)?;
            }
            lo = p + 1;
        }
        Ok(())
    }

    /// `bestUnitToAttack`'s sort of the order slots by rank, the highest first (ties to the
    /// lower id): with more than one entry, `FUN_140c41830` over all, then `FUN_140c41b40` over
    /// both sides of the pivot.
    ///
    /// # Errors
    ///
    /// A slot or an entry the sort does not hold.
    pub fn sort(&mut self) -> Result<(), crate::GenieError> {
        let Some(last) = self.entries.len().checked_sub(1).filter(|&l| 0 < l) else {
            return Ok(());
        };
        let p = self.partition(0, last)?;
        if p > 0 {
            self.quicksort(0, p - 1)?;
        }
        self.quicksort(p + 1, last)
    }
}

/// The order history's weights by attack category (`bestUnitToAttack`'s walk of `+0xa8` from
/// the newest): each order younger than a minute (`now` less its time, unsigned, at most 60000)
/// adds the rest of the minute over 60000 (`DAT_142efe420`) to its target's category; the first
/// older one ends the walk.
#[must_use]
pub fn order_weights(history: &[crate::unit_ai::OrderRecord], now: WorldTime) -> ByCategory<f32> {
    let mut w = ByCategory::default();
    for r in history {
        let age = now.since(r.time);
        if Millis::new(60000) < age {
            break;
        }
        if let Some(k) = r.category {
            w[k] += (60000 - age.get()) as f32 / ORDER_WINDOW;
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use crate::map::Distance;
    use crate::unit_ai::best_target::Bounds;

    /// The bounds start reset, take a value as both least and most, widen by each value beyond
    /// them, pass over a NaN, and span the most less the least.
    #[test]
    fn bounds_widen_by_each_value_beyond_them_and_pass_over_a_nan() {
        let mut b = Bounds::RESET;
        assert_eq!((b.least, b.most), (f32::MAX, -1.0));
        b.take(Distance::new(3.0));
        assert_eq!((b.least, b.most), (3.0, 3.0));
        b.take(Distance::new(1.0));
        b.take(Distance::new(7.0));
        b.take(Distance::new(f32::NAN));
        assert_eq!((b.least, b.most, b.span()), (1.0, 7.0, 6.0));
    }
}
