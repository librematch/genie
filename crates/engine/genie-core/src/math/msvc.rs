//! The MSVC standard library's algorithms as build 158041 compiled them, where their order shows
//! in the simulation: `std::sort` is an introsort whose order of equal elements depends on the
//! algorithm, so a port must run the same steps.
//!
//! `pred(a, b)` is the sort's comparator: true when `a` goes before `b`.

use crate::GenieResult;
use crate::world::slots::CheckedIndex;
use std::mem::take;

/// `std::sort` (`_Sort_unchecked`, the tile pathfinder's at `0x140f9c510`): quicksort by the
/// median guess while the range holds more than 32 elements and divisions are left, a heap sort
/// when they run out, an insertion sort for the rest.
///
/// # Errors
///
/// A position outside `values`, which the algorithm's bounds never reach (an unreachable error
/// that names it), whatever `pred` answers.
pub fn sort<T: Copy>(values: &mut [T], pred: &impl Fn(T, T) -> bool) -> GenieResult<()> {
    let ideal = values.len() as isize;
    sort_unchecked(values, 0, values.len(), ideal, pred)
}

/// `_ISORT_MAX`.
const INSERTION_MAX: usize = 32;

/// The element at `i` of `values`.
fn at<T: Copy>(values: &[T], i: usize) -> GenieResult<T> {
    values.checked(i).copied()
}

/// `std::iter_swap` of the elements at `a` and `b` of `values`.
fn swap<T>(values: &mut [T], a: usize, b: usize) -> GenieResult<()> {
    let _ = (values.checked(a)?, values.checked(b)?);
    values.swap(a, b);
    Ok(())
}

fn sort_unchecked<T: Copy>(
    values: &mut [T],
    mut first: usize,
    mut last: usize,
    mut ideal: isize,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<()> {
    loop {
        if last - first <= INSERTION_MAX {
            return insertion_sort(values, first, last, pred);
        }
        if ideal <= 0 {
            let range = values.head_mut(last)?.tail_mut(first)?;
            make_heap(range, pred)?;
            return sort_heap(range, pred);
        }
        let (mid_first, mid_last) = partition_by_median_guess(values, first, last, pred)?;
        ideal = (ideal >> 1) + (ideal >> 2);
        if mid_first - first < last - mid_last {
            sort_unchecked(values, first, mid_first, ideal, pred)?;
            first = mid_last;
        } else {
            sort_unchecked(values, mid_last, last, ideal, pred)?;
            last = mid_first;
        }
    }
}

/// `_Insertion_sort_unchecked`: equal elements keep their order.
fn insertion_sort<T: Copy>(
    values: &mut [T],
    first: usize,
    last: usize,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<()> {
    if first == last {
        return Ok(());
    }
    for mid in first + 1..last {
        let val = at(values, mid)?;
        if pred(val, at(values, first)?) {
            values.head_mut(mid + 1)?.copy_within(first..mid, first + 1);
            *values.checked_mut(first)? = val;
        } else {
            let mut hole = mid;
            while pred(val, at(values, hole - 1)?) {
                *values.checked_mut(hole)? = at(values, hole - 1)?;
                hole -= 1;
            }
            *values.checked_mut(hole)? = val;
        }
    }
    Ok(())
}

/// `_Med3_unchecked`: the median of three to the middle.
fn med3<T: Copy>(
    values: &mut [T],
    first: usize,
    mid: usize,
    last: usize,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<()> {
    if pred(at(values, mid)?, at(values, first)?) {
        swap(values, mid, first)?;
    }
    if pred(at(values, last)?, at(values, mid)?) {
        swap(values, last, mid)?;
        if pred(at(values, mid)?, at(values, first)?) {
            swap(values, mid, first)?;
        }
    }
    Ok(())
}

/// `_Guess_median_unchecked` (`0x140f9c040`): Tukey's ninther above 40 elements; `last` is the
/// range's last element.
fn guess_median<T: Copy>(
    values: &mut [T],
    first: usize,
    mid: usize,
    last: usize,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<()> {
    let count = last - first;
    if 40 < count {
        let step = (count + 1) >> 3;
        let two_step = step << 1;
        med3(values, first, first + step, first + two_step, pred)?;
        med3(values, mid - step, mid, mid + step, pred)?;
        med3(values, last - two_step, last - step, last, pred)?;
        med3(values, first + step, mid, last - step, pred)
    } else {
        med3(values, first, mid, last, pred)
    }
}

/// Whether neither of `a` and `b` goes before the other.
fn equal<T: Copy>(a: T, b: T, pred: &impl Fn(T, T) -> bool) -> bool {
    !pred(a, b) && !pred(b, a)
}

/// `_Partition_by_median_guess_unchecked` (`0x140f9c210`): the range of elements equal to the
/// pivot, those before it in front, those after it behind.
fn partition_by_median_guess<T: Copy>(
    values: &mut [T],
    first: usize,
    last: usize,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<(usize, usize)> {
    let mid = first + ((last - first) >> 1);
    guess_median(values, first, mid, last - 1, pred)?;
    let (mut pfirst, mut plast) = pivot_run(values, first, last, mid, pred)?;
    let mut gfirst = plast;
    let mut glast = pfirst;
    loop {
        (plast, gfirst) = scan_up(values, (pfirst, plast, gfirst), last, pred)?;
        (pfirst, glast) = scan_down(values, (pfirst, glast), first, pred)?;
        if glast == first && gfirst == last {
            return Ok((pfirst, plast));
        }
        if glast == first {
            if plast != gfirst {
                swap(values, pfirst, plast)?;
            }
            plast += 1;
            swap(values, pfirst, gfirst)?;
            pfirst += 1;
            gfirst += 1;
        } else if gfirst == last {
            glast -= 1;
            pfirst -= 1;
            if glast != pfirst {
                swap(values, glast, pfirst)?;
            }
            plast -= 1;
            swap(values, pfirst, plast)?;
        } else {
            glast -= 1;
            swap(values, gfirst, glast)?;
            gfirst += 1;
        }
    }
}

/// [`partition_by_median_guess`]'s pivot run: from `mid` the elements equal to it before and
/// after it, as a range.
fn pivot_run<T: Copy>(
    values: &[T],
    first: usize,
    last: usize,
    mid: usize,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<(usize, usize)> {
    let mut pfirst = mid;
    let mut plast = pfirst + 1;
    while first < pfirst && equal(at(values, pfirst - 1)?, at(values, pfirst)?, pred) {
        pfirst -= 1;
    }
    while plast < last && equal(at(values, plast)?, at(values, pfirst)?, pred) {
        plast += 1;
    }
    Ok((pfirst, plast))
}

/// [`partition_by_median_guess`]'s scan up from `gfirst`: past the elements after the pivot
/// (at `pfirst`), an equal one joining the run's end `plast`; it stops at one before the pivot
/// or at `last`. The run's end and where the scan stopped.
fn scan_up<T: Copy>(
    values: &mut [T],
    (pfirst, mut plast, mut gfirst): (usize, usize, usize),
    last: usize,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<(usize, usize)> {
    while gfirst < last {
        let (pivot, g) = (at(values, pfirst)?, at(values, gfirst)?);
        if pred(pivot, g) {
        } else if pred(g, pivot) {
            break;
        } else {
            if plast != gfirst {
                swap(values, plast, gfirst)?;
            }
            plast += 1;
        }
        gfirst += 1;
    }
    Ok((plast, gfirst))
}

/// [`partition_by_median_guess`]'s scan down from `glast`: past the elements before the pivot
/// (at `pfirst`), an equal one joining the run's start; it stops at one after the pivot or at
/// `first`. The run's start and where the scan stopped.
fn scan_down<T: Copy>(
    values: &mut [T],
    (mut pfirst, mut glast): (usize, usize),
    first: usize,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<(usize, usize)> {
    while first < glast {
        let (g, pivot) = (at(values, glast - 1)?, at(values, pfirst)?);
        if pred(g, pivot) {
        } else if pred(pivot, g) {
            break;
        } else {
            pfirst -= 1;
            if pfirst != glast - 1 {
                swap(values, pfirst, glast - 1)?;
            }
        }
        glast -= 1;
    }
    Ok((pfirst, glast))
}

/// `_Push_heap_by_index`.
fn push_heap_by_index<T: Copy>(
    values: &mut [T],
    mut hole: usize,
    top: usize,
    val: T,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<()> {
    while top < hole {
        let idx = (hole - 1) >> 1;
        let parent = at(values, idx)?;
        if !pred(parent, val) {
            break;
        }
        *values.checked_mut(hole)? = parent;
        hole = idx;
    }
    *values.checked_mut(hole)? = val;
    Ok(())
}

/// `_Pop_heap_hole_by_index`: the hole moves down to the larger child, then `val` goes up.
fn pop_heap_hole_by_index<T: Copy>(
    values: &mut [T],
    mut hole: usize,
    bottom: usize,
    val: T,
    pred: &impl Fn(T, T) -> bool,
) -> GenieResult<()> {
    let top = hole;
    let mut idx = hole;
    let max_non_leaf = (bottom.wrapping_sub(1)) >> 1;
    while idx < max_non_leaf {
        idx = 2 * idx + 2;
        if pred(at(values, idx)?, at(values, idx - 1)?) {
            idx -= 1;
        }
        *values.checked_mut(hole)? = at(values, idx)?;
        hole = idx;
    }
    if idx == max_non_leaf && bottom.is_multiple_of(2) {
        *values.checked_mut(hole)? = at(values, bottom - 1)?;
        hole = bottom - 1;
    }
    push_heap_by_index(values, hole, top, val, pred)
}

/// `_Make_heap_unchecked`.
fn make_heap<T: Copy>(values: &mut [T], pred: &impl Fn(T, T) -> bool) -> GenieResult<()> {
    let bottom = values.len();
    let mut hole = bottom >> 1;
    while 0 < hole {
        hole -= 1;
        let val = at(values, hole)?;
        pop_heap_hole_by_index(values, hole, bottom, val, pred)?;
    }
    Ok(())
}

/// `_Sort_heap_unchecked`.
fn sort_heap<T: Copy>(values: &mut [T], pred: &impl Fn(T, T) -> bool) -> GenieResult<()> {
    let mut last = values.len();
    while 2 <= last {
        last -= 1;
        let val = at(values, last)?;
        *values.checked_mut(last)? = at(values, 0)?;
        pop_heap_hole_by_index(values, 0, last, val, pred)?;
    }
    Ok(())
}

/// The buckets of a new `std::unordered_map` (`_Min_buckets`).
const MIN_BUCKETS: usize = 8;
/// Below this many buckets a rehash grows them eightfold (`0x1ff < buckets` grows to the need).
const GROW_EIGHTFOLD_BELOW: usize = 512;

/// A key of a `std::unordered_map<int, …>`: a value the game keeps as a 32-bit integer.
pub trait IntKey: Copy + PartialEq {
    /// The integer `std::hash<int>` hashes.
    fn int(self) -> i32;
}

impl IntKey for i32 {
    fn int(self) -> i32 {
        self
    }
}

/// The bucket of a 32-bit key: `std::hash<int>`, FNV-1a over its four bytes (little-endian), and
/// the bucket mask.
fn bucket(key: impl IntKey, mask: u64) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let hash = key
        .int()
        .to_le_bytes()
        .iter()
        .fold(OFFSET, |h, &b| (h ^ u64::from(b)).wrapping_mul(PRIME));
    hash & mask
}

/// A new key goes in front of its bucket's elements, or at the list's end when its bucket is
/// empty (`_Find_last`'s insertion point, then `_Insert_new_node_before`).
fn insert_in_bucket<K: IntKey>(list: &mut Vec<K>, key: K, mask: u64) {
    let b = bucket(key, mask);
    match list.iter().position(|&k| bucket(k, mask) == b) {
        Some(lo) => list.insert(lo, key),
        None => list.push(key),
    }
}

/// The order in which a `std::unordered_map<int, …>` with the maximum load factor 1.0 lists its
/// keys (its element list, which iteration follows) after `try_emplace` of `keys` in turn
/// (`FUN_1415f0c40`; a key already in it is found, not inserted). An insertion that would pass
/// the load factor first rehashes (`FUN_141615a30`): to eight times the buckets below 512, else
/// to the need, then each element in list order goes in front of its new bucket's (where it
/// already leads its bucket, it stays).
#[must_use]
pub fn unordered_map_order<K: IntKey>(keys: impl IntoIterator<Item = K>) -> Vec<K> {
    let mut buckets = MIN_BUCKETS;
    let mut list: Vec<K> = Vec::new();
    for key in keys {
        if list.contains(&key) {
            continue;
        }
        let size = list.len() + 1;
        if 1.0 < size as f32 / buckets as f32 {
            let need = MIN_BUCKETS.max(size);
            if buckets < need {
                buckets = if GROW_EIGHTFOLD_BELOW <= buckets || buckets * 8 < need {
                    need
                } else {
                    buckets * 8
                };
            }
            buckets = buckets.next_power_of_two();
            let mask = buckets as u64 - 1;
            let old = take(&mut list);
            for k in old {
                insert_in_bucket(&mut list, k, mask);
            }
        }
        insert_in_bucket(&mut list, key, buckets as u64 - 1);
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pairs sorted by their key, descending, as the pathfinder sorts its open nodes.
    fn by_key(v: &mut [(u32, usize)]) {
        sort(v, &|a: (u32, usize), b: (u32, usize)| a.0 > b.0).unwrap();
    }

    #[test]
    fn map_lists_key_of_taken_bucket_in_front_of_it() {
        // FNV-1a & 7: 1 -> bucket 4, 9 -> bucket 4 too, 2 -> bucket 7 (as the game's hash)
        assert_eq!(bucket(1, 7), bucket(9, 7));
        assert_ne!(bucket(1, 7), bucket(2, 7));
        assert_eq!(unordered_map_order([1, 2, 9, 2]), [9, 1, 2]);
    }

    #[test]
    fn rehash_keeps_every_key_once() {
        let order = unordered_map_order((0..100).chain(0..10));
        assert_eq!(order.len(), 100);
        let mut sorted = order;
        sorted.sort_unstable();
        assert_eq!(sorted, (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn short_range_sorts_by_insertion_and_keeps_equal_elements_in_order() {
        let mut v: Vec<(u32, usize)> = [3, 1, 3, 2, 1, 3].iter().copied().zip(0..).collect();
        by_key(&mut v);
        assert_eq!(v, [(3, 0), (3, 2), (3, 5), (2, 3), (1, 1), (1, 4)]);
    }

    #[test]
    fn long_range_is_sorted_and_keeps_every_element() {
        // a fixed pseudo-random sequence with many equal keys (an LCG)
        let mut x = 12345u32;
        let mut v: Vec<(u32, usize)> = (0..500)
            .map(|i| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
                ((x >> 16) % 37, i)
            })
            .collect();
        let mut expected = v.clone();
        by_key(&mut v);
        assert!(v.windows(2).all(|w| w[0].0 >= w[1].0));
        let mut got = v.clone();
        got.sort_unstable();
        expected.sort_unstable();
        assert_eq!(got, expected);
    }

    #[test]
    fn the_heap_sort_sorts_when_no_divisions_are_left() {
        let mut v: Vec<(u32, usize)> = (0..100).map(|i| ((i * 7919 % 100) as u32, i)).collect();
        let count = v.len();
        sort_unchecked(&mut v, 0, count, 0, &|a: (u32, usize), b: (u32, usize)| {
            a.0 > b.0
        })
        .unwrap();
        assert!(v.windows(2).all(|w| w[0].0 >= w[1].0));
    }

    /// The keys of [`the_game_sort_orders_equal_keys_as_its_own`]: an LCG's `(x >> 16) % m`.
    fn lcg_keys(n: usize, m: u32, seed: u32) -> Vec<(f32, usize)> {
        let mut x = seed;
        (0..n)
            .map(|i| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
                (((x >> 16) % m) as f32, i)
            })
            .collect()
    }

    /// The tile pathfinder's `std::sort` instance (`0x140f9c510`: pointers to nodes by their f32 at
    /// `+0x10`, descending) run in unicorn on these keys gives these orders, equal keys included:
    /// the partition (100 elements), the heap sort (no divisions left), Tukey's ninther (above
    /// 40) and a partition that runs out of divisions (3 for 120).
    #[test]
    fn the_game_sort_orders_equal_keys_as_its_own() {
        #[rustfmt::skip]
        let cases: [(usize, u32, u32, isize, &[usize]); 5] = [
            (100, 37, 12345, 100, &[
                41, 73, 87, 26, 65, 78, 98, 70, 53, 13, 60, 35, 29, 44, 46, 0, 86,
                34, 15, 66, 92, 45, 17, 89, 12, 93, 79, 54, 58, 32, 52, 91, 42, 97,
                76, 5, 55, 80, 88, 84, 28, 16, 71, 63, 33, 36, 19, 11, 1, 39, 49,
                61, 68, 3, 82, 67, 31, 57, 23, 94, 48, 77, 62, 2, 81, 59, 21, 22,
                75, 64, 37, 18, 14, 99, 8, 47, 96, 7, 72, 27, 85, 90, 74, 69, 6, 56,
                10, 4, 25, 83, 51, 95, 43, 50, 40, 30, 38, 24, 20, 9,
            ]),
            (100, 100, 7919, 0, &[
                69, 3, 74, 60, 97, 50, 52, 37, 32, 81, 67, 8, 10, 92, 33, 86, 15,
                46, 58, 68, 7, 90, 13, 17, 83, 38, 94, 16, 55, 39, 34, 31, 65, 11,
                25, 4, 56, 20, 98, 44, 2, 45, 29, 42, 93, 73, 24, 5, 21, 63, 91, 79,
                1, 87, 40, 99, 70, 76, 48, 80, 88, 49, 59, 61, 89, 82, 72, 36, 30,
                54, 71, 23, 18, 22, 35, 0, 96, 26, 43, 19, 84, 66, 53, 28, 95, 75,
                64, 9, 27, 14, 62, 77, 47, 6, 57, 78, 51, 85, 41, 12,
            ]),
            (64, 5, 42, 64, &[
                0, 3, 4, 57, 54, 11, 52, 49, 29, 30, 31, 39, 61, 7, 55, 56, 37, 14,
                36, 48, 47, 28, 20, 46, 63, 2, 12, 15, 19, 22, 23, 24, 32, 45, 50,
                51, 59, 1, 25, 40, 42, 43, 21, 18, 17, 13, 53, 16, 60, 5, 26, 38,
                41, 44, 33, 8, 34, 35, 10, 9, 6, 58, 27, 62,
            ]),
            (41, 3, 1, 41, &[
                0, 13, 14, 19, 16, 17, 6, 7, 8, 38, 18, 11, 22, 34, 35, 36, 40, 5,
                10, 15, 20, 27, 28, 29, 30, 31, 32, 33, 1, 2, 3, 4, 26, 21, 12, 23,
                24, 37, 9, 39, 25,
            ]),
            (120, 7, 5, 3, &[
                105, 44, 33, 3, 111, 56, 64, 103, 102, 53, 27, 54, 104, 37, 14, 24,
                69, 17, 98, 22, 45, 25, 57, 95, 97, 87, 61, 50, 63, 90, 118, 51, 77,
                60, 48, 66, 78, 116, 10, 71, 36, 74, 2, 52, 40, 23, 84, 13, 43, 101,
                114, 107, 70, 32, 11, 12, 20, 31, 39, 42, 30, 65, 79, 83, 86, 91,
                94, 99, 89, 109, 15, 9, 28, 26, 38, 4, 92, 93, 96, 62, 1, 115, 119,
                19, 8, 68, 6, 5, 88, 72, 35, 80, 82, 55, 29, 59, 49, 21, 100, 108,
                47, 18, 34, 7, 67, 46, 41, 73, 75, 76, 81, 58, 0, 85, 106, 16, 110,
                112, 117, 113,
            ]),
        ];
        for (n, m, seed, ideal, game) in cases {
            let mut lcg_keys = lcg_keys(n, m, seed);
            sort_unchecked(
                &mut lcg_keys,
                0,
                n,
                ideal,
                &|a: (f32, usize), b: (f32, usize)| a.0 > b.0,
            )
            .unwrap();
            let ours: Vec<usize> = lcg_keys.iter().map(|e| e.1).collect();
            assert_eq!(
                ours, game,
                "{n} keys mod {m}, seed {seed}, divisions {ideal}"
            );
        }
    }

    /// The garrison's `std::sort` instance (`0x1415f0830`: 0x28-byte pairs by their f32 gap at
    /// `+0x20`, ascending) run in unicorn on the keys of [`lcg_keys`]: genie's orders.
    #[test]
    fn the_garrison_sort_orders_equal_gaps_as_its_own() {
        #[rustfmt::skip]
        let cases: [(usize, u32, u32, isize, &[usize]); 5] = [
            (100, 37, 12345, 100, &[
                24, 9, 20, 30, 38, 40, 50, 43, 4, 51, 95, 83, 25, 10, 56, 6, 69, 74,
                7, 90, 85, 27, 72, 47, 96, 8, 14, 37, 18, 99, 75, 64, 22, 21, 2, 59,
                81, 62, 77, 48, 57, 94, 23, 67, 31, 82, 3, 68, 61, 11, 1, 19, 49,
                39, 33, 63, 36, 16, 28, 71, 84, 5, 55, 80, 88, 76, 42, 97, 58, 54,
                79, 52, 32, 91, 12, 93, 45, 89, 17, 66, 15, 34, 92, 86, 0, 44, 46,
                29, 60, 35, 53, 13, 78, 70, 98, 73, 26, 41, 65, 87,
            ]),
            (100, 100, 7919, 0, &[
                41, 12, 85, 51, 78, 57, 6, 77, 47, 62, 9, 64, 27, 14, 75, 95, 28,
                53, 66, 84, 19, 43, 96, 26, 0, 35, 22, 18, 23, 71, 54, 30, 72, 36,
                82, 89, 49, 88, 59, 61, 80, 48, 76, 70, 40, 87, 99, 1, 79, 91, 63,
                21, 73, 5, 24, 42, 93, 45, 2, 29, 44, 98, 20, 56, 4, 11, 25, 65, 31,
                34, 39, 55, 16, 94, 38, 17, 83, 90, 13, 7, 68, 58, 46, 15, 33, 86,
                10, 92, 32, 67, 8, 81, 37, 52, 50, 97, 60, 74, 3, 69,
            ]),
            (64, 5, 42, 64, &[
                8, 58, 6, 9, 10, 44, 27, 38, 35, 41, 34, 33, 62, 1, 26, 60, 5, 53,
                16, 13, 43, 42, 17, 18, 21, 40, 25, 12, 15, 19, 22, 23, 24, 32, 45,
                63, 50, 51, 59, 2, 31, 20, 14, 46, 48, 36, 37, 7, 47, 56, 28, 61,
                39, 55, 0, 11, 30, 29, 49, 52, 54, 57, 4, 3,
            ]),
            (41, 3, 1, 41, &[
                37, 1, 2, 3, 4, 26, 21, 25, 24, 9, 23, 39, 12, 20, 5, 34, 35, 36,
                10, 15, 40, 27, 28, 29, 30, 31, 32, 33, 6, 7, 8, 16, 14, 19, 22, 18,
                17, 13, 38, 11, 0,
            ]),
            (120, 7, 5, 3, &[
                0, 117, 81, 85, 58, 16, 113, 110, 106, 112, 76, 73, 75, 59, 49, 82,
                46, 72, 7, 47, 80, 18, 55, 21, 108, 100, 29, 67, 34, 35, 41, 1, 4,
                5, 8, 26, 92, 119, 38, 68, 88, 19, 62, 115, 96, 6, 93, 28, 94, 99,
                9, 109, 11, 12, 15, 20, 30, 31, 39, 42, 89, 65, 79, 83, 86, 91, 84,
                70, 40, 13, 101, 114, 2, 52, 116, 71, 32, 10, 36, 74, 66, 43, 107,
                78, 23, 22, 90, 63, 118, 60, 61, 87, 51, 95, 48, 77, 97, 98, 45, 57,
                25, 50, 102, 103, 33, 105, 3, 69, 24, 54, 53, 111, 44, 17, 64, 14,
                56, 27, 37, 104,
            ]),
        ];
        for (n, m, seed, ideal, game) in cases {
            let mut lcg_keys = lcg_keys(n, m, seed);
            sort_unchecked(
                &mut lcg_keys,
                0,
                n,
                ideal,
                &|a: (f32, usize), b: (f32, usize)| a.0 < b.0,
            )
            .unwrap();
            let ours: Vec<usize> = lcg_keys.iter().map(|e| e.1).collect();
            assert_eq!(
                ours, game,
                "{n} keys mod {m}, seed {seed}, divisions {ideal}"
            );
        }
    }
}
