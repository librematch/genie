//! createColumnLine (`FUN_141012280`): the sizes and the slots of its sub-formations.

use crate::GenieError;
use crate::groups::FormationCategory;
use crate::groups::FormationKind;
use crate::groups::GroupId;
use crate::groups::SlotState;
use crate::groups::formation::{CATEGORIES, EPSILON, FLANK_SPACING, STAGGER_SHARE, TWO};
use crate::groups::layout::{clear_line, reset_line};
use crate::groups::{Formation, Slot, UnitGroup};
use crate::map::Distance;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{BVector, add, scale, sub};
use crate::world::World;
use crate::world::WorldTime;
use crate::world::slots::CheckedIndex;

/// The formation categories' count, as an array's length.
const CATEGORY_COUNT: usize = CATEGORIES as usize;

/// What createColumnLine reads of one sub-formation's statistic.
#[derive(Clone, Debug)]
pub(super) struct Category {
    pub(super) count: i32,
    pub(super) spacing: Distance,
    pub(super) types: i32,
    pub(super) unit_types: Vec<u32>,
    pub(super) unit_type_counts: Vec<u32>,
}

/// The line and column counts of a sub-formation of `count` units for a line (`by_line`, the
/// column ratio -1) or a column: the root of the count, rounded up, as the lines (a line's) or
/// the columns (a column's), then fewer lines until a line is at least twice as wide as the
/// formation is deep (a column the reverse). Returns (lines, columns).
pub(super) fn sizes(count: i32, by_line: bool) -> (i32, i32) {
    let root = cvttss2si(crate::math::fmath::sqrt(count as f32).ceil());
    let root = root.max(1);
    let per = |a: i32| {
        let b = count / a;
        if 0 < count % a { b + 1 } else { b }
    };
    // `first` is the side the root sizes, `second` the other; `first` shrinks until it is at
    // most half of `second`
    let mut first = (count / root).max(1);
    let mut second = per(first);
    while second / 2 < first {
        first -= 1;
        if first < 1 {
            first = 1;
            second = count;
            break;
        }
        second = per(first);
    }
    if by_line {
        (first, second)
    } else {
        (second, first)
    }
}

/// The slots of createColumnLine (`FUN_141012280`, [`World::create_column_line`])
/// up to its transform: the group's sub-formations (its unit types, by category 0 to 5) laid
/// out in lines on `formation`, `world_time` the slots' time. `column` lays a column (the column
/// ratio set, type 1), else a line (type 2, staggered 7, flanked 8). Returns false for a group
/// without members (nothing laid out); its callers always pass a ratio.
///
/// Each sub-formation with units takes the lines and columns of [`sizes`]; the widest line
/// (columns times the sub-formation's spacing) sets the width of the others when two or more
/// have units: one that fits takes one line, else as many lines as its width needs, and a line
/// less while its columns are fewer than its empty places. The slots' array grows to the
/// widest line's columns times the lines (`FUN_14102d580`), the lines' to the lines
/// (`FUN_14102d490`); every slot turns inactive. Then, sub-formation by sub-formation (its unit
/// types' used counts start over, `DAT_143f20640`), line by line (the last of a sub-formation
/// takes the rest of its units): the line's slots start at half its columns of the single
/// offset (the spacing straight back, `(0, 0, -spacing)`, doubled when staggered) and step by
/// it; a staggered line moves by a quarter of it (back on even lines, forward on odd ones), an
/// even line of a plain one by half of it back; a flanked line's slots move apart by
/// [`FLANK_SPACING`] less the line's share of the depth. A slot takes no unit, the offset, the
/// time, the line as its depth, the sub-formation and, with several unit types, the next one
/// with units left (round robin; with two types the last slot of a line keeps the type for the
/// next line). The line takes its slots' indices and its distance back, which grows by each
/// line's spacing (doubled when staggered) and, when a wider sub-formation follows, by the
/// difference.
///
/// The roots and the line counts are the CRT's `ceilf` (the ucrtbase import at 0x142a8e728),
/// for which `f32::ceil` stands: both are exact (the result is the integer above, a NaN stays
/// NaN); `CVTTSS2SI` then makes a NaN or an infinity `i32::MIN`
/// ([`crate::math::conversions::cvttss2si`]), which the clamps to 1 take in.
///
/// # Panics
///
/// Never for a line or slot past its array: the lines' array holds the sum of the
/// sub-formations' lines (`FUN_14102d490`), and the slots' array their lines times the widest
/// line's columns (`FUN_14102d580`), while no line holds more than its sub-formation's
/// columns.
pub(crate) fn column_line_slots(
    formation: &mut Formation,
    group: &UnitGroup,
    column: bool,
    (staggered, flanked): (bool, bool),
    world_time: WorldTime,
) -> Result<bool, GenieError> {
    if group.members.is_empty() {
        return Ok(false);
    }
    let (lines_of, columns_of) = category_sizes(group, column)?;
    let max_width = columns_of.iter().copied().fold(0, i32::max);
    let depth: i32 = lines_of.iter().sum();
    formation.resize_slots((depth * max_width).max(0) as usize);
    formation.resize_lines(depth.max(0) as usize);
    for s in formation.every_slot_mut() {
        s.active = 0;
    }
    formation.kind = if column {
        FormationKind::COLUMN
    } else if flanked {
        FormationKind::FLANKED
    } else if staggered {
        FormationKind::STAGGERED
    } else {
        FormationKind::LINE
    };
    formation.transform_type = 0;
    formation.maximum_width = max_width;
    formation.maximum_depth = depth;
    formation.number_lines = depth;
    let style = Style {
        staggered,
        flanked,
        depth,
        world_time,
    };
    let mut at = Cursor::default();
    for (k, c) in FormationCategory::EACH.into_iter().enumerate() {
        let Some(cat) = category(group, c).filter(|cat| 0 < cat.types) else {
            continue;
        };
        lay_category(
            formation,
            &cat,
            c,
            (*lines_of.checked(k)?, *columns_of.checked(k)?),
            style,
            &mut at,
        )?;
    }
    formation.use_slots(at.used_slots);
    Ok(true)
}

/// The statistic of the group's sub-formation of formation category `c`, none without one.
fn category(group: &UnitGroup, c: FormationCategory) -> Option<Category> {
    let stat = group
        .unit_types
        .iter()
        .find(|s| s.formation_category == c)?;
    Some(Category {
        count: stat.units.len() as i32,
        spacing: stat.desired_spacing,
        types: stat.number_unit_types,
        unit_types: stat.unit_types.clone(),
        unit_type_counts: stat.unit_type_counts.clone(),
    })
}

/// The lines and the columns of each category with units: its [`sizes`]; with two or more
/// such, each but the widest (its columns times its spacing, the first of equals) fits the
/// widest's width ([`fit_to_widest`]).
fn category_sizes(
    group: &UnitGroup,
    column: bool,
) -> Result<([i32; CATEGORY_COUNT], [i32; CATEGORY_COUNT]), GenieError> {
    let mut lines_of = [0i32; CATEGORY_COUNT];
    let mut columns_of = [0i32; CATEGORY_COUNT];
    let mut widest = 0.0f32;
    let mut widest_index = FormationCategory::ANY;
    let mut valid = 0;
    for (k, c) in FormationCategory::EACH.into_iter().enumerate() {
        let Some(cat) = category(group, c).filter(|cat| 0 < cat.count) else {
            continue;
        };
        valid += 1;
        let (lines, columns) = sizes(cat.count, !column);
        *lines_of.checked_mut(k)? = lines;
        *columns_of.checked_mut(k)? = columns;
        let width = columns as f32 * cat.spacing.get();
        if widest_index == FormationCategory::ANY || widest < width {
            widest = width;
            widest_index = c;
        }
    }
    if widest_index != FormationCategory::ANY && 2 <= valid {
        for (k, c) in FormationCategory::EACH.into_iter().enumerate() {
            if c == widest_index {
                continue;
            }
            let Some(cat) = category(group, c).filter(|cat| 0 < cat.count) else {
                continue;
            };
            (*lines_of.checked_mut(k)?, *columns_of.checked_mut(k)?) = fit_to_widest(&cat, widest);
        }
    }
    Ok((lines_of, columns_of))
}

/// A category beside the widest, as (lines, columns): one that fits its width (its count times
/// its spacing at most `widest`) takes one line of all its units; else as many lines as its width
/// needs (the ratio rounded up, `CVTTSS2SI`, at least 1) and the columns its count needs over
/// them, then a line less while its columns are fewer than its empty places.
pub(super) fn fit_to_widest(cat: &Category, widest: f32) -> (i32, i32) {
    if cat.count as f32 * cat.spacing.get() <= widest {
        return (1, cat.count);
    }
    let mut lines = cvttss2si(((cat.count as f32 * cat.spacing.get()) / widest).ceil());
    if lines < 1 {
        lines = 1;
    }
    let mut columns = cat.count / lines;
    if 0 < cat.count % lines {
        columns += 1;
    }
    let mut empty = columns * lines - cat.count;
    while columns < empty && 1 < lines {
        lines -= 1;
        empty = columns * lines - cat.count;
    }
    (lines, columns)
}

/// How the lines are laid out: staggered or flanked, the formation's depth and the slots' time.
#[derive(Clone, Copy)]
struct Style {
    staggered: bool,
    flanked: bool,
    depth: i32,
    world_time: WorldTime,
}

/// What the layout carries from line to line: the next line's and slot's indices, the distance
/// back, the last line's spacing and the slots used.
#[derive(Default)]
struct Cursor {
    line_index: i32,
    slot_index: i32,
    distance_back: Distance,
    last_spacing: Distance,
    used_slots: usize,
}

/// The unit type the next slot asks for (-1 for any: one unit type, or none left) and the slots
/// each type has had in this category (`DAT_143f20640`, starting over per category).
struct SubTypes {
    next: i32,
    used: [u32; 50],
}

impl SubTypes {
    /// The type after a slot of type `next` at column `col` of a line of `actual`: the next type
    /// round robin (with two types the last slot of a line keeps it for the next line); one whose
    /// units the slots used up passes to the next with units left, else to none.
    fn advance(&mut self, cat: &Category, col: i32, actual: i32) -> Result<(), GenieError> {
        *self.used.checked_mut(self.next as usize)? += 1;
        let mut sub_type = self.next;
        if cat.types != 2 || col < actual - 1 {
            sub_type += 1;
        }
        let next = if sub_type < cat.types { sub_type } else { 0 };
        self.next = next;
        if *cat.unit_type_counts.checked(next as usize)? <= *self.used.checked(next as usize)? {
            // out of units for that sub type: the next with units left, else none
            let mut j = if next + 1 < cat.types { next + 1 } else { 0 };
            while j != next
                && *cat.unit_type_counts.checked(j as usize)? <= *self.used.checked(j as usize)?
            {
                j = if j + 1 < cat.types { j + 1 } else { 0 };
            }
            self.next = if j == next { -1 } else { j };
        }
        Ok(())
    }
}

/// The lines of category `c` (`lines` of `columns`, the last one the rest of its units): its
/// single offset is its spacing straight back, doubled when staggered; a wider category after
/// another grows the distance back by the difference. Each line starts over ([`clear_line`],
/// [`reset_line`]) at the distance back, takes its slots ([`lay_line`]) and moves the distance
/// back by its spacing (doubled when staggered).
fn lay_category(
    formation: &mut Formation,
    cat: &Category,
    c: FormationCategory,
    (lines, columns): (i32, i32),
    style: Style,
    at: &mut Cursor,
) -> Result<(), GenieError> {
    let mut subs = SubTypes {
        next: if cat.types == 1 { -1 } else { 0 },
        used: [0u32; 50],
    };
    let mut single: BVector = scale(cat.spacing.get(), BVector::new(0.0, 0.0, -1.0));
    if style.staggered {
        single = BVector::new(single.x() * TWO, single.height() * TWO, single.y() * TWO);
    }
    if EPSILON < at.distance_back.get()
        && EPSILON < at.last_spacing.get()
        && at.last_spacing < cat.spacing
    {
        at.distance_back += cat.spacing - at.last_spacing;
    }
    for in_category in 0..lines {
        let mut actual = columns;
        if 1 < lines && in_category == lines - 1 {
            actual = cat.count - (lines - 1) * columns;
        }
        let total = line_start(single, actual, at.line_index, style.staggered);
        let line = formation.lines.checked_mut(at.line_index as usize)?;
        clear_line(line);
        let _ = reset_line(line, actual, at.line_index);
        line.distance_back = at.distance_back;
        lay_line(
            formation,
            cat,
            c,
            (actual, total, single),
            style,
            at,
            &mut subs,
        )?;
        at.used_slots += actual.max(0) as usize;
        at.line_index += 1;
        at.last_spacing = if style.staggered {
            cat.spacing + cat.spacing
        } else {
            cat.spacing
        };
        at.distance_back += at.last_spacing;
    }
    Ok(())
}

/// Where a line of `actual` slots starts: half its slots (truncated) of `single` ahead; a
/// staggered line moves by a quarter of `single` (back on even lines, forward on odd ones), an
/// even line of a plain one by half of it back.
fn line_start(single: BVector, actual: i32, line_index: i32, staggered: bool) -> BVector {
    let half = scale((actual / 2) as f32, single);
    let total = add(half, BVector::ZERO);
    if staggered {
        let quarter = BVector::new(
            single.x() / STAGGER_SHARE,
            single.height() / STAGGER_SHARE,
            single.y() / STAGGER_SHARE,
        );
        if line_index & 1 == 0 {
            sub(total, quarter)
        } else {
            BVector::new(
                total.x() + quarter.x(),
                total.height() + quarter.height(),
                total.y() + quarter.y(),
            )
        }
    } else if actual & 1 == 0 {
        let half_step = BVector::new(single.x() / TWO, single.height() / TWO, single.y() / TWO);
        sub(total, half_step)
    } else {
        total
    }
}

/// The slots of one line from `total` on, a `single` step each: a flanked line's apart
/// ([`flank_offset`]); each takes no unit, its offset, the time, the line as its depth, the
/// category, its unit type ([`SubTypes`]), active, and keeps its transformed mark; the line
/// takes its index, counted on over every line.
fn lay_line(
    formation: &mut Formation,
    cat: &Category,
    c: FormationCategory,
    (actual, mut total, single): (i32, BVector, BVector),
    style: Style,
    at: &mut Cursor,
    subs: &mut SubTypes,
) -> Result<(), GenieError> {
    for col in 0..actual.max(0) {
        let i = at.used_slots + col as usize;
        let desired_sub_category = if subs.next < 0 {
            -1
        } else {
            *cat.unit_types.checked(subs.next as usize)? as i32
        };
        *formation.slot_mut(i)? = Slot {
            unit_id: None,
            offset: (flank_offset(total, at.line_index, style)),
            desired_transform_position: BVector::ZERO,
            time: style.world_time,
            state: SlotState::Steering,
            depth: at.line_index,
            transform_position: BVector::ZERO,
            position_filled: 0,
            desired_category: c,
            active: 1,
            desired_sub_category,
            transformed: formation.slot_mut(i)?.transformed,
        };
        let line = formation.lines.checked_mut(at.line_index as usize)?;
        if (col as usize) < line.indices.len() {
            *line.indices.checked_mut(col as usize)? = at.slot_index as u32;
        }
        total = sub(total, single);
        at.slot_index += 1;
        if 0 <= subs.next {
            subs.advance(cat, col, actual)?;
        }
    }
    Ok(())
}

/// A flanked line's slot moves apart (away from the middle on y) by [`FLANK_SPACING`] less the
/// line's share of the depth; any other keeps `total`.
fn flank_offset(total: BVector, line_index: i32, style: Style) -> BVector {
    let mut offset = total;
    if style.flanked {
        let flank = FLANK_SPACING - line_index as f32 / style.depth as f32;
        if 0.0 <= offset.y() {
            offset = offset.with_y(offset.y() + flank);
        } else {
            offset = offset.with_y(offset.y() - flank);
        }
    }
    offset
}

impl World {
    /// createColumnLine (`FUN_141012280`) of `formation` for group `g`: a column (`column`, the
    /// column ratio 2 and line ratio -1) or a line (ratios -1 and 2), staggered or flanked;
    /// false without members. Its slots ([`column_line_slots`]), then the transform and the
    /// units ([`World::lay_units`]) with the column-to-line flag (0x141013d20..0x141013d3a): a
    /// line (2, 7 or 8) whose group's previous formation is a column (1). The game reads that
    /// type through the
    /// previous formation's pointer, which `UpdateDesiredPositions` sets whenever it lays a
    /// formation out anew; a group without one is not a column here.
    ///
    /// # Errors
    ///
    /// As the transform and the units' assignment.
    pub(crate) fn create_column_line(
        &mut self,
        formation: &mut Formation,
        group: GroupId,
        column: bool,
        staggered_flanked: (bool, bool),
    ) -> Result<bool, GenieError> {
        let g = self.groups.held(group)?;
        if !column_line_slots(formation, g, column, staggered_flanked, self.world_time)? {
            return Ok(false);
        }
        let after_column = matches!(
            formation.kind,
            FormationKind::LINE | FormationKind::STAGGERED | FormationKind::FLANKED
        ) && g
            .previous_formation
            .as_ref()
            .is_some_and(|p| p.kind == FormationKind::COLUMN);
        self.lay_units(formation, group, after_column)?;
        Ok(true)
    }
}
