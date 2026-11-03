//! createColumnLine (`FUN_141012280`): the sizes and the slots of its sub-formations.

use crate::GenieError;
use crate::groups::FormationCategory;
use crate::groups::FormationKind;
use crate::groups::GroupId;
use crate::groups::Layout;
use crate::groups::SlotState;
use crate::groups::TransformKind;
use crate::groups::UnitKind;
use crate::groups::formation::SlotOrder;
use crate::groups::formation::{CATEGORIES, EPSILON};
use crate::groups::layout::{clear_line, reset_line};
use crate::groups::{Formation, Slot, UnitGroup};
use crate::map::Distance;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::BVector;
use crate::math::fmath::sqrt;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::layer::generated::GroupTuning;
use crate::world::slots::CheckedIndex;

/// The formation categories' count, as an array's length.
const CATEGORY_COUNT: usize = CATEGORIES as usize;

/// The formation createColumnLine (`FUN_141012280`) lays out: a column (type 1, the column
/// ratio 2 and the line ratio -1), else a line (ratios -1 and 2): plain (type 2), staggered
/// (7) or flanked (8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LineKind {
    /// A column.
    Column,
    /// A plain line.
    Line,
    /// A staggered line: its spacing doubled, every other line a quarter of it ahead.
    Staggered,
    /// A flanked line: its slots apart from the middle.
    Flanked,
}

impl LineKind {
    /// The kind of formation type `kind`: a column, a staggered or a flanked line, any other a
    /// plain line (the layout's callers pass these four).
    pub(crate) const fn of_formation(kind: FormationKind) -> Self {
        match kind {
            FormationKind::Column => Self::Column,
            FormationKind::Staggered => Self::Staggered,
            FormationKind::Flanked => Self::Flanked,
            _ => Self::Line,
        }
    }

    /// The formation's type.
    const fn formation_kind(self) -> FormationKind {
        match self {
            Self::Column => FormationKind::Column,
            Self::Line => FormationKind::Line,
            Self::Staggered => FormationKind::Staggered,
            Self::Flanked => FormationKind::Flanked,
        }
    }
}

/// What createColumnLine reads of one sub-formation's statistic.
#[derive(Clone, Debug)]
pub(super) struct Category {
    pub(super) count: i32,
    pub(super) spacing: Distance,
    pub(super) kinds: i32,
    pub(super) unit_kinds: Vec<UnitKind>,
    pub(super) unit_kind_counts: Vec<u32>,
}

/// The line and column counts of a sub-formation of `count` units for a line (the column ratio -1)
/// or a column (`kind` [`LineKind::Column`]): the root of the count, rounded up, as the lines (a
/// line's) or the columns (a column's), then fewer lines until a line is at least twice as wide as
/// the formation is deep (a column the reverse). Returns (lines, columns).
pub(super) fn sizes(count: i32, kind: LineKind) -> (i32, i32) {
    let root = cvttss2si(sqrt(count as f32).ceil());
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
    if kind == LineKind::Column {
        (second, first)
    } else {
        (first, second)
    }
}

/// The slots of createColumnLine (`FUN_141012280`, [`World::create_column_line`]) up to its
/// transform: the group's sub-formations (its unit types, by category 0 to 5) laid out in lines on
/// `formation`, `world_time` the slots' time, as a column or a line of `kind`. Not laid for a
/// group without members (nothing laid out); its callers always pass a ratio.
///
/// Each sub-formation with units takes the lines and columns of [`sizes`]; the widest line (columns
/// times the sub-formation's spacing) sets the width of the others when two or more have units: one
/// that fits takes one line, else as many lines as its width needs, and a line less while its
/// columns are fewer than its empty places. The slots' array grows to the widest line's columns
/// times the lines (`FUN_14102d580`), the lines' to the lines (`FUN_14102d490`); every slot turns
/// inactive. Then, sub-formation by sub-formation (its unit types' used counts start over,
/// `DAT_143f20640`), line by line (the last of a sub-formation takes the rest of its units): the
/// line's slots start at half its columns of the single offset (the spacing straight back, `(0, 0,
/// -spacing)`, doubled when staggered) and step by it; a staggered line moves by a quarter of it
/// (back on even lines, forward on odd ones), an even line of a plain one by half of it back; a
/// flanked line's slots move apart by `GroupTuning::flank_spacing` less the line's share of the
/// depth. A slot takes no unit, the offset, the time, the line as its depth, the sub-formation and,
/// with several unit types, the next one with units left (round robin; with two types the last slot
/// of a line keeps the type for the next line). The line takes its slots' indices and its distance
/// back, which grows by each line's spacing (doubled when staggered) and, when a wider
/// sub-formation follows, by the difference.
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
    kind: LineKind,
    world_time: WorldTime,
    group_tuning: &GroupTuning,
) -> Result<Layout, GenieError> {
    if group.members.is_empty() {
        return Ok(Layout::NotLaid);
    }
    let (lines_of, columns_of) = category_sizes(group, kind)?;
    let max_width = columns_of.iter().copied().fold(0, i32::max);
    let depth: i32 = lines_of.iter().sum();
    formation.resize_slots((depth * max_width).max(0) as usize);
    formation.resize_lines(depth.max(0) as usize);
    for s in formation.every_slot_mut() {
        s.active = 0;
    }
    formation.kind = kind.formation_kind();
    formation.transform_kind = TransformKind::ByLines;
    formation.maximum_width = max_width;
    formation.maximum_depth = depth;
    formation.number_lines = depth;
    let style = Style {
        kind,
        depth,
        world_time,
        group_tuning,
    };
    let mut at = Cursor::default();
    for (k, c) in FormationCategory::EACH.into_iter().enumerate() {
        let Some(cat) = category(group, c).filter(|cat| 0 < cat.kinds) else {
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
    Ok(Layout::Laid)
}

/// The statistic of the group's sub-formation of formation category `c`, none without one.
fn category(group: &UnitGroup, c: FormationCategory) -> Option<Category> {
    let stat = group
        .unit_kinds
        .iter()
        .find(|s| s.formation_category == c)?;
    Some(Category {
        count: stat.units.len() as i32,
        spacing: stat.desired_spacing,
        kinds: stat.number_unit_kinds,
        unit_kinds: stat.unit_kinds.clone(),
        unit_kind_counts: stat.unit_kind_counts.clone(),
    })
}

/// The lines and the columns of each category with units: its [`sizes`]; with two or more
/// such, each but the widest (its columns times its spacing, the first of equals) fits the
/// widest's width ([`fit_to_widest`]).
fn category_sizes(
    group: &UnitGroup,
    kind: LineKind,
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
        let (lines, columns) = sizes(cat.count, kind);
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

/// How the lines are laid out: the kind of line, the formation's depth, the slots' time and the
/// groups' tuning values.
#[derive(Clone, Copy)]
struct Style<'a> {
    kind: LineKind,
    depth: i32,
    world_time: WorldTime,
    group_tuning: &'a GroupTuning,
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
struct SubKinds {
    next: i32,
    used: [u32; 50],
}

impl SubKinds {
    /// The type after a slot of type `next` at column `col` of a line of `actual`: the next type
    /// round robin (with two types the last slot of a line keeps it for the next line); one whose
    /// units the slots used up passes to the next with units left, else to none.
    fn advance(&mut self, cat: &Category, col: i32, actual: i32) -> Result<(), GenieError> {
        *self.used.checked_mut(self.next as usize)? += 1;
        let mut sub_kind = self.next;
        if cat.kinds != 2 || col < actual - 1 {
            sub_kind += 1;
        }
        let next = if sub_kind < cat.kinds { sub_kind } else { 0 };
        self.next = next;
        if *cat.unit_kind_counts.checked(next as usize)? <= *self.used.checked(next as usize)? {
            // out of units for that sub type: the next with units left, else none
            let mut j = if next + 1 < cat.kinds { next + 1 } else { 0 };
            while j != next
                && *cat.unit_kind_counts.checked(j as usize)? <= *self.used.checked(j as usize)?
            {
                j = if j + 1 < cat.kinds { j + 1 } else { 0 };
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
    style: Style<'_>,
    at: &mut Cursor,
) -> Result<(), GenieError> {
    let group_tuning = style.group_tuning;
    let mut subs = SubKinds {
        next: if cat.kinds == 1 { -1 } else { 0 },
        used: [0u32; 50],
    };
    let mut single: BVector = cat.spacing.get() * BVector::new(0.0, 0.0, -1.0);
    if style.kind == LineKind::Staggered {
        single = BVector::new(
            single.x() * group_tuning.spacing_factor,
            single.height() * group_tuning.spacing_factor,
            single.y() * group_tuning.spacing_factor,
        );
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
        let total = line_start(single, actual, at.line_index, style.kind, group_tuning);
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
        at.last_spacing = if style.kind == LineKind::Staggered {
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
fn line_start(
    single: BVector,
    actual: i32,
    line_index: i32,
    kind: LineKind,
    group_tuning: &GroupTuning,
) -> BVector {
    let half = (actual / 2) as f32 * single;
    let total = half + BVector::ZERO;
    if kind == LineKind::Staggered {
        let quarter = BVector::new(
            single.x() / group_tuning.stagger_share,
            single.height() / group_tuning.stagger_share,
            single.y() / group_tuning.stagger_share,
        );
        if line_index & 1 == 0 {
            total - quarter
        } else {
            BVector::new(
                total.x() + quarter.x(),
                total.height() + quarter.height(),
                total.y() + quarter.y(),
            )
        }
    } else if actual & 1 == 0 {
        let half_step = BVector::new(
            single.x() / group_tuning.spacing_factor,
            single.height() / group_tuning.spacing_factor,
            single.y() / group_tuning.spacing_factor,
        );
        total - half_step
    } else {
        total
    }
}

/// The slots of one line from `total` on, a `single` step each: a flanked line's apart
/// ([`flank_offset`]); each takes no unit, its offset, the time, the line as its depth, the
/// category, its unit type ([`SubKinds`]), active, and keeps its transformed mark; the line
/// takes its index, counted on over every line.
fn lay_line(
    formation: &mut Formation,
    cat: &Category,
    c: FormationCategory,
    (actual, mut total, single): (i32, BVector, BVector),
    style: Style<'_>,
    at: &mut Cursor,
    subs: &mut SubKinds,
) -> Result<(), GenieError> {
    for col in 0..actual.max(0) {
        let i = at.used_slots + col as usize;
        let desired_sub_category = if subs.next < 0 {
            UnitKind::NONE
        } else {
            *cat.unit_kinds.checked(subs.next as usize)?
        };
        *formation.slot_mut(i)? = Slot {
            unit_id: None,
            offset: flank_offset(total, at.line_index, style),
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
        total = total - single;
        at.slot_index += 1;
        if 0 <= subs.next {
            subs.advance(cat, col, actual)?;
        }
    }
    Ok(())
}

/// A flanked line's slot moves apart (away from the middle on y) by `GroupTuning::flank_spacing`
/// less the line's share of the depth; any other keeps `total`.
fn flank_offset(total: BVector, line_index: i32, style: Style<'_>) -> BVector {
    let mut offset = total;
    if style.kind == LineKind::Flanked {
        let flank = style.group_tuning.flank_spacing - line_index as f32 / style.depth as f32;
        if 0.0 <= offset.y() {
            offset = offset.with_y(offset.y() + flank);
        } else {
            offset = offset.with_y(offset.y() - flank);
        }
    }
    offset
}

impl World {
    /// createColumnLine (`FUN_141012280`) of `formation` for group `g`: a column or a line of
    /// `kind`; not laid without members. Its slots ([`column_line_slots`]), then the transform and
    /// the units ([`World::lay_units`]) with the column-to-line flag (0x141013d20..0x141013d3a): a
    /// line (2, 7 or 8) whose group's previous formation is a column (1). The game reads that type
    /// through the previous formation's pointer, which `UpdateDesiredPositions` sets whenever it
    /// lays a formation out anew; a group without one is not a column here.
    ///
    /// # Errors
    ///
    /// As the transform and the units' assignment.
    pub(crate) fn create_column_line(
        &mut self,
        formation: &mut Formation,
        group: GroupId,
        kind: LineKind,
    ) -> Result<Layout, GenieError> {
        let g = self.groups.held(group)?;
        if column_line_slots(
            formation,
            g,
            kind,
            self.world_time,
            &self.model.tuning.group,
        )? == Layout::NotLaid
        {
            return Ok(Layout::NotLaid);
        }
        let after_column = matches!(
            formation.kind,
            FormationKind::Line | FormationKind::Staggered | FormationKind::Flanked
        ) && g
            .previous_formation
            .as_ref()
            .is_some_and(|p| p.kind == FormationKind::Column);
        let order = if after_column {
            SlotOrder::PreviousColumn
        } else {
            SlotOrder::Closest
        };
        self.lay_units(formation, group, order)?;
        Ok(Layout::Laid)
    }
}
