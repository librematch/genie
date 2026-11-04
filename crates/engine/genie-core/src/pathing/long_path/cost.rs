//! The long pather's costs ([`LongCost`], in quarter tiles squared) and its open tiles' heap
//! ([`Heap`]), sifted the game's way.

use crate::map::Tile;
use crate::world::slots::CheckedIndex;
use crate::{GenieError, GenieResult};

/// A heap pop finds the heap empty where the caller checked its top.
const EMPTY_HEAP: GenieError = GenieError::unreachable("a heap pop without a top");

/// The heap's slot 0 (`+0x119424`), below every cost: a sift up stops there.
pub(super) const SENTINEL: LongCost = LongCost(0x88ca_6c00_u32 as i32);

/// A cost of the search (a tile's `+0x38420`, a heap slot's): in quarter tiles squared, in the
/// game's wrapping `i32` arithmetic. The start costs its gap to the goal ([`LongCost::of_gap`]);
/// a step adds its length over 4 and moves the estimate to the goal along ([`step_cost`]), a
/// turn 1 more ([`LongCost::turned`]); the goal's tile costs 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct LongCost(i32);

impl LongCost {
    /// The goal's tile.
    pub(super) const ZERO: Self = Self(0);

    /// The gap `(dx, dy)` in quarter tiles, squared per axis and summed (y's first).
    pub(super) const fn of_gap(dx: i32, dy: i32) -> Self {
        Self(quad(dy).wrapping_add(quad(dx)))
    }

    /// A quarter (`>> 2`): a popped tile's distance to the goal against the start's.
    pub(super) const fn quartered(self) -> Self {
        Self(self.0 >> 2)
    }

    /// One more for a step that turns.
    pub(super) const fn turned(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// `a` tiles in quarter tiles, squared (wrapping).
const fn quad(a: i32) -> i32 {
    (a * 4).wrapping_mul(a * 4)
}

/// The open tiles: a binary heap from slot 1, slot 0 the sentinel (`+0x119420`, count
/// `+0x2db428`).
pub(super) struct Heap {
    pub(super) slots: Vec<(Tile, LongCost)>,
}

impl Heap {
    pub(super) const fn count(&self) -> i32 {
        self.slots.len() as i32 - 1
    }

    /// `FUN_140fa0bd0` on a slot just filled: it rises while its cost is at most its parent's.
    pub(super) fn push(&mut self, at: Tile, cost: LongCost) -> GenieResult<()> {
        self.slots.push((at, cost));
        let mut i = self.slots.len() - 1;
        let mut parent = i >> 1;
        if self.slots.checked(parent)?.1 < cost {
            return Ok(());
        }
        loop {
            *self.slots.checked_mut(i)? = *self.slots.checked_mut(parent)?;
            i /= 2;
            parent /= 2;
            if self.slots.checked(parent)?.1 < cost {
                break;
            }
        }
        *self.slots.checked_mut(i)? = (at, cost);
        Ok(())
    }

    /// The pop after the top's expansion check (`FUN_140f9eb70`): the last slot moves to the top
    /// and sinks, the game's way: only while the count's half is above 1, only to a child below
    /// that half, past a child whose cost is not above it.
    pub(super) fn pop(&mut self) -> GenieResult<()> {
        let last = self.slots.pop().ok_or(EMPTY_HEAP)?;
        let count = self.count();
        if count < 1 {
            return Ok(());
        }
        *self.slots.checked_mut(1)? = last;
        let half = count / 2;
        if half <= 1 {
            return Ok(());
        }
        let mut i = 1usize;
        loop {
            let mut child = 2 * i;
            if (child as i32) < count
                && self.slots.checked(child + 1)?.1 < self.slots.checked(child)?.1
            {
                child += 1;
            }
            if last.1 < self.slots.checked(child)?.1 {
                break;
            }
            *self.slots.checked_mut(i)? = *self.slots.checked_mut(child)?;
            i = child;
            if half <= child as i32 {
                break;
            }
        }
        if i != 1 {
            *self.slots.checked_mut(i)? = last;
        }
        Ok(())
    }
}

/// The search's cost of the step from `from` (its cost `here`) to `to`: the step's length in
/// quarter tiles squared over 4, and the estimate to the goal (`goal`, quarter tiles squared
/// per axis) moved from `from`'s to `to`'s, in the game's wrapping arithmetic.
pub(super) const fn step_cost(here: LongCost, from: Tile, to: Tile, goal: Tile) -> LongCost {
    let walked = quad(to.y() - from.y()).wrapping_add(quad(to.x() - from.x())) >> 2;
    LongCost(
        walked
            .wrapping_add(here.0)
            .wrapping_sub(quad(goal.y() - from.y()))
            .wrapping_sub(quad(goal.x() - from.x()))
            .wrapping_add(quad(goal.y() - to.y()))
            .wrapping_add(quad(goal.x() - to.x())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_heap_sinks_the_games_way() {
        let mut h = Heap {
            slots: vec![(Tile::new(0, 0), SENTINEL)],
        };
        for (i, c) in [5, 3, 8, 1, 9, 2].into_iter().enumerate() {
            h.push(Tile::new(i as i32, 0), LongCost(c)).unwrap();
        }
        assert_eq!(h.slots[1].1, LongCost(1));
        h.pop().unwrap();
        // count 5, half 2: no sinking past the top's children below 2
        let costs: Vec<i32> = h.slots[1..].iter().map(|s| s.1.0).collect();
        assert_eq!(costs, [2, 3, 8, 5, 9]);
    }

    /// The start costs its gap in quarter tiles squared; a step adds its length over 4 and
    /// moves the estimate to the goal along; a turn costs 1 more, wrapping at the top; a popped
    /// tile's distance is a quarter.
    #[test]
    fn step_costs_its_length_and_moves_the_estimate() {
        assert_eq!(LongCost::of_gap(3, 4), LongCost(16 * 9 + 16 * 16));
        assert_eq!(LongCost(401).quartered(), LongCost(100));
        assert_eq!(LongCost(i32::MAX).turned(), LongCost(i32::MIN));
        let (from, to, goal) = (Tile::new(0, 0), Tile::new(1, 1), Tile::new(4, 0));
        // 32 / 4 walked; the estimate from 16 * 16 to 16 * 9 + 16
        assert_eq!(
            step_cost(LongCost(100), from, to, goal),
            LongCost(100 + 8 - 256 + 144 + 16)
        );
        assert!(LongCost::ZERO < LongCost(1) && SENTINEL < LongCost::ZERO);
    }
}
