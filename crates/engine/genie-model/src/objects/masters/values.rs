//! What the attribute setters share: the caps, the truncations, the sprite and class amounts,
//! the damage sprites' and train locations' entries.

use crate::math::conversions::{cvttss2si, cvttss2si_64};
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, maxss};
use crate::math::image::masters::{
    HANDICAP_FLOOR as MASTERS_HANDICAP_FLOOR, MISSILE_SPEED_CAP as MASTERS_MISSILE_SPEED_CAP,
    PERCENT as MASTERS_PERCENT, ROUND as MASTERS_ROUND, SHORT_CAP as MASTERS_SHORT_CAP,
    SPEED_CAP as MASTERS_SPEED_CAP,
};
use crate::objects::Graphics;
use crate::objects::SpriteId;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::{ClassAmount, DamageClass, DamageGraphicEntry, TrainLocationEntry};
use crate::objects::masters::{DamageGraphic, TrainLocation, attribute};
use crate::players::attribute::{self as players_attribute, AttributeId};
use std::fmt::{Debug, Error, Formatter};

/// 32767.0 (`DAT_142eb69cc`): a capped add or multiply holds hit points and carry capacity
/// below it.
pub(super) const SHORT_CAP: f32 = MASTERS_SHORT_CAP;
/// 0.5 (`DAT_142e7f384`): a multiply's rounding.
pub(super) const ROUND: f32 = MASTERS_ROUND;
/// 4.5 (`DAT_142f52490`): the most speed a capped add or multiply leaves .
pub(super) const SPEED_CAP: f32 = MASTERS_SPEED_CAP;
/// 20.0 (`DAT_142e7f38c`): a missile master's .
pub(super) const MISSILE_SPEED_CAP: f32 = MASTERS_MISSILE_SPEED_CAP;
/// 1.0 (`DAT_14330eb8c`): a handicap above it raises an attack class at 0 .
pub(super) const HANDICAP_FLOOR: f32 = MASTERS_HANDICAP_FLOOR;
/// 100.0 (`DAT_14330ebcc`): a multiply's armor or attack amount is a percentage (an image role to
/// add: `masters::PERCENT`).
pub(super) const PERCENT: f32 = MASTERS_PERCENT;
/// The attack classes a handicap leaves alone (`0x280000018`'s bits up to class 0x21): 3, 4, 31
/// and 33.
pub(super) const UNHANDICAPPED: u64 = 0x2_8000_0018;
/// The most damage sprites (`FUN_1410e87c0`, 0xff) and train locations (`FUN_141249220`,
/// 0x7fff) a resize leaves; an insert past one less is refused (`FUN_1410e8680`,
/// `FUN_141249040`).
pub(super) const MOST_DAMAGE_GRAPHICS: u64 = 0xff;
pub(super) const MOST_TRAIN_LOCATIONS: u64 = 0x7fff;

/// What the masters' attribute setters read beyond the master.
#[derive(Clone, Copy)]
pub struct AttributeWorld<'a> {
    /// The world's sprite table (`+0xc0`, its count `+0xd0`): a sprite attribute takes an id only
    /// where the table holds a sprite (`FUN_14152e360`, `FUN_141533c70`).
    pub sprites: &'a Graphics,
    /// The player's handicap's attack factor (`player+0x668`, its `+0x14`), None without one.
    pub handicap: Option<f32>,
    /// The player's `+0x648`, which an add or a multiply passes: hit points and carry capacity
    /// stop at 32767, the speed at its cap.
    pub capped: bool,
}

/// The sprite table shows as its absence of a name.
impl Debug for AttributeWorld<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        f.debug_struct("AttributeWorld")
            .field("handicap", &self.handicap)
            .field("capped", &self.capped)
            .finish_non_exhaustive()
    }
}

/// `CVTTSS2SI` into a 32-bit register, then its low 16 bits.
pub(super) fn low16(v: f32) -> i16 {
    cvttss2si(v) as i16
}

/// `CVTTSS2SI` into a 32-bit register, then its low byte.
pub(super) fn low8(v: f32) -> u8 {
    cvttss2si(v) as u8
}

/// A saved id (an `int` field) as the slice holds it: None for one below 0 or beyond a short,
/// as the save's (`volley_missile_id`, `mChargeProjectileUnit`).
pub(super) fn id_of(id: i32) -> Option<MasterId> {
    i16::try_from(id).ok().and_then(MasterId::of_data)
}

/// `FUN_14152e360`: the sprite `id`, when the world's table holds one there; else the field keeps
/// its sprite (the game releases the old one first, which is not simulation).
pub(super) fn set_sprite(field: &mut Option<SpriteId>, id: i32, world: &AttributeWorld<'_>) {
    if let Some(sprite) = i16::try_from(id).ok().and_then(SpriteId::of_data)
        && world.sprites.contains_key(sprite)
    {
        *field = Some(sprite);
    }
}

/// An add to a sprite attribute: its id (−1 for none) plus the amount (`CVTTSS2SI`, in 32
/// bits), then [`set_sprite`].
pub(super) fn add_sprite(field: &mut Option<SpriteId>, v: f32, world: &AttributeWorld<'_>) {
    let id = i32::from(field.map_or(-1, SpriteId::get)) + cvttss2si(v);
    set_sprite(field, id, world);
}

/// An add to the hit points or the carry capacity (`FUN_141126af0`): capped, a sum of at least
/// 32767 (or NaN, `COMISS` then `JBE`) holds 0x7fff; else the amount's low 16 bits add.
pub(super) fn add_short(value: i16, v: f32, capped: bool) -> i16 {
    if capped && at_most_or_unordered(SHORT_CAP, f32::from(value) + v) {
        0x7fff
    } else {
        low16(v).wrapping_add(value)
    }
}

/// A multiply of the hit points or the carry capacity (`FUN_1411271e0`): `value × v + 0.5`,
/// truncated; capped, a product of at least 32767 (or NaN) holds 0x7fff.
pub(super) fn multiply_short(value: i16, v: f32, capped: bool) -> i16 {
    let x = f32::from(value) * v + ROUND;
    if capped && at_most_or_unordered(SHORT_CAP, x) {
        0x7fff
    } else {
        low16(x)
    }
}

/// An armor or attack amount's class (its high byte) and amount (its low byte, negated for a
/// negative amount; `COMISS` with 0 then `JC`, so a NaN takes the negative branch).
pub(super) fn class_amount(v: f32) -> ClassAmount {
    if below_or_unordered(v, 0.0) {
        let u = cvttss2si(-v) as u16;
        ClassAmount::new((u as i16) >> 8, ((u & 0xff) as i16).wrapping_neg())
    } else {
        let u = cvttss2si(v) as u16;
        ClassAmount::new((u as i16) >> 8, (u & 0xff) as i16)
    }
}

/// The handicap raises an attack class (`FUN_1410c12b0`): a factor above 1 (`COMISS`, `JBE`: a
/// NaN does not) for a class above 0x21 (an unsigned compare) or not one of [`UNHANDICAPPED`].
pub(super) fn handicapped(handicap: Option<f32>, class: DamageClass) -> Option<f32> {
    let f = handicap?;
    let raised = !at_most_or_unordered(f, HANDICAP_FLOOR)
        && (0x21 < class.get() as u16 || UNHANDICAPPED >> (class.get() as u16 & 0x3f) & 1 == 0);
    raised.then_some(f)
}

/// The resource a cost attribute names (`FUN_1412661d0`'s 100, 103 to 106): none (−1) every
/// paid one, then food, wood, gold (3) and stone (2).
#[must_use]
pub const fn cost_resource(attribute: MasterAttribute) -> Option<AttributeId> {
    match attribute {
        attribute::FOOD_COSTS => Some(players_attribute::FOOD),
        attribute::WOOD_COSTS => Some(players_attribute::WOOD),
        attribute::GOLD_COSTS => Some(players_attribute::GOLD),
        attribute::STONE_COSTS => Some(players_attribute::STONE),
        _ => None,
    }
}

/// `FUN_1410e87c0`: the list takes `count` entries (`MAXSS` with 0, so a NaN stays and becomes
/// the most; to an unsigned 64-bit count, at most `most`), new ones `fill`; the selected entry
/// (`entry`) stays below the count (an unsigned compare: with none left it stays).
pub(super) fn resize_count(v: f32) -> u64 {
    let f = maxss(0.0, v);
    // `CVTTSS2SI RBX` plus 2^63 past 2^63: any value from there on is past the most
    cvttss2si_64(f) as u64
}

/// The damage sprites resized to `v` (`FUN_1410e87c0`): new entries are zero, the selected entry
/// (an unsigned byte compare with the count less one) at most the last.
pub(super) fn resize_damage_graphics(
    list: &mut Vec<DamageGraphic>,
    entry: &mut DamageGraphicEntry,
    v: f32,
) {
    let n = resize_count(v).min(MOST_DAMAGE_GRAPHICS) as usize;
    list.resize(n, DamageGraphic::default());
    let last = DamageGraphicEntry::of_data((n as u8).wrapping_sub(1));
    if last < *entry {
        *entry = last;
    }
}

/// The train locations resized to `v` (`FUN_141249220`): new entries copy the last one (zero
/// for an empty list), the selected entry (a signed short compare with the count less one) at
/// most the last, −1 for none.
pub(super) fn resize_train_locations(
    list: &mut Vec<TrainLocation>,
    entry: &mut TrainLocationEntry,
    v: f32,
) {
    let n = resize_count(v).min(MOST_TRAIN_LOCATIONS) as usize;
    let fill = list.last().copied().unwrap_or_default();
    list.resize(n, fill);
    let last = TrainLocationEntry::of_data((n as i16).wrapping_sub(1));
    if last < *entry {
        *entry = last;
    }
}

/// A multiply of the damage sprites' selected entry (`FUN_1410e8680`): `v` truncated (32 bits);
/// below 0 the entry at its complement (at most the last) goes, else a zero entry comes at it
/// (at most the count; none past 0xfe entries). The selected entry is that place, at most the
/// last (−1, so 0xff, with none left); an empty list or a full one changes nothing.
pub(super) fn insert_damage_graphic(
    list: &mut Vec<DamageGraphic>,
    entry: &mut DamageGraphicEntry,
    v: f32,
) {
    let mut at = cvttss2si(v);
    if at < 0 {
        if list.is_empty() {
            return;
        }
        at = (!at).min(list.len() as i32 - 1);
        let _ = list.remove(at as usize);
    } else {
        if MOST_DAMAGE_GRAPHICS as usize <= list.len() {
            return;
        }
        at = at.min(list.len() as i32);
        list.insert(at as usize, DamageGraphic::default());
    }
    let last = list.len() as i32 - 1;
    *entry = DamageGraphicEntry::of_selected(if last < at { last } else { at });
}

/// A multiply of the train locations' selected entry (`FUN_141249040`): as
/// [`insert_damage_graphic`], up to 0x7ffe entries, the new one a copy of the last (zero for an
/// empty list).
pub(super) fn insert_train_location(
    list: &mut Vec<TrainLocation>,
    entry: &mut TrainLocationEntry,
    v: f32,
) {
    let mut at = cvttss2si(v);
    if at < 0 {
        if list.is_empty() {
            return;
        }
        at = (!at).min(list.len() as i32 - 1);
        let _ = list.remove(at as usize);
    } else {
        if MOST_TRAIN_LOCATIONS as usize <= list.len() {
            return;
        }
        at = at.min(list.len() as i32);
        let fill = list.last().copied().unwrap_or_default();
        list.insert(at as usize, fill);
    }
    let last = list.len() as i32 - 1;
    *entry = TrainLocationEntry::of_selected(if last < at { last } else { at });
}

/// The selected entry moved to `n` (the attributes 154 and 158): at most the last of `count`
/// (a signed compare), nothing for an empty list.
pub(super) fn select_entry(count: usize, n: i32) -> Option<i32> {
    (count != 0).then(|| (count as i32 - 1).min(n))
}
