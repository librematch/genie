//! An object's change by an attribute (its slots 0x78, 0x80 and 0x88): the static object's,
//! the combat unit's and the building's.

use crate::GenieError;
use crate::combat::Charge;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::{Master, Masters, master_type, player_master};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::players::attribute::AttributeId;
use crate::players::tracker::ListKey;
use crate::technology::effect::{Change, OBJECT_GONE};
use crate::world::World;
use crate::world::slots::PlayerSlots;

pub(crate) const POPULATION: AttributeId = attribute::POPULATION;
pub(super) const RESOURCE_19: AttributeId = attribute::UNITS_OWNED;
/// A storage flag of `'@'` (64): the object's storage stays with it (`FUN_141195a50`).
pub(super) const KEPT_STORAGE: u8 = 64;
/// A hit point total a capped multiply holds an object's below (`FUN_1411962e0`: `0x46fffe00`).
pub(super) const OBJECT_HP_CAP: f32 = 32767.0;
/// A multiply's rounding (`DAT_142e7f384`, 0.5).
pub(super) const ROUND: f32 = crate::math::image::effect::ROUND;

impl World {
    /// What the masters' setters read of the world for player `p` ([`AttributeWorld`]): the
    /// sprite table (the `.dat`'s graphics), no handicap (`player+0x668`, not held: none in the
    /// games probed) and the player's `+0x648` unset (not held: 0 for every player probed).
    ///
    /// [`AttributeWorld`]: crate::objects::masters::AttributeWorld
    pub(super) fn attribute_world(
        graphics: &std::collections::BTreeMap<i16, crate::objects::sprite::Graphic>,
    ) -> impl Fn(i32) -> bool + '_ {
        move |id| i16::try_from(id).is_ok_and(|id| graphics.contains_key(&id))
    }

    /// The ids player `p`'s tracker files under class `class` (`FUN_14104f8f0`, key class + 900)
    /// and those pending under it (`FUN_14104f9e0`, key class + 90900), each in the tracker's
    /// order. A queued unit's place holds −1, which no object has: every reader's lookup passes
    /// it over, so it is left out ([`tracked_ids`]).
    pub(super) fn tracked_class(
        &self,
        p: PlayerId,
        class: ClassId,
    ) -> Result<(Vec<ObjectId>, Vec<ObjectId>), GenieError> {
        let t = &self.players.player(p)?.tracker;
        let key = ListKey::class(class);
        Ok((tracked_ids(t.list(key)), tracked_ids(t.list(key.pending()))))
    }

    /// The ids player `p`'s tracker files as done under key master `key` (`FUN_14104f560`'s list
    /// of one key, whatever the objects' masters), in the tracker's order.
    pub(super) fn tracked_key(
        &self,
        p: PlayerId,
        key: MasterId,
    ) -> Result<Vec<ObjectId>, GenieError> {
        Ok(tracked_ids(
            self.players.player(p)?.tracker.list(ListKey::master(key)),
        ))
    }

    /// The ids player `p`'s tracker files under master `master` (`FUN_14104f960`), of which
    /// `FUN_14165a860` keeps those of that very master; and for a building master those pending
    /// under it, or under the stack unit it heads (`FUN_14104fa50`, key + 90000), whatever their
    /// masters. Each in the tracker's order.
    pub(super) fn tracked_master(
        &self,
        p: PlayerId,
        master: &Master,
    ) -> Result<(Vec<ObjectId>, Vec<ObjectId>), GenieError> {
        let player = self.players.player(p)?;
        let of_master = |id: &ObjectId| {
            player
                .objects
                .iter()
                .chain(&player.sleeping)
                .any(|u| u.id == *id && u.life.master_id == master.id)
        };
        let done: Vec<ObjectId> = tracked_ids(player.tracker.list(ListKey::master(master.id)))
            .into_iter()
            .filter(of_master)
            .collect();
        let mut pending = Vec::new();
        if master_type::BUILDING <= master.kind {
            let stack = master.building.as_ref().and_then(|b| b.stack_unit);
            let key = stack
                .and_then(|stack| player_master(&self.masters, p, stack))
                .filter(|s| {
                    s.kind == master_type::BUILDING
                        && s.building
                            .as_ref()
                            .is_some_and(|b| b.head_unit == Some(master.id))
                })
                .map_or(master.id, |s| s.id);
            pending = tracked_ids(player.tracker.list(ListKey::master(key).pending()));
        }
        Ok((done, pending))
    }
}

/// The objects of a tracker's list, its queued units' places (−1) left out.
fn tracked_ids(list: &[Option<ObjectId>]) -> Vec<ObjectId> {
    list.iter().copied().flatten().collect()
}

impl World {
    /// `FUN_14165bae0` and the objects' slots 0x78, 0x80, 0x88 (by `change`) on object `id` of
    /// player `p`: nothing for an object the world lacks (or one with a master of its own,
    /// `FUN_141188bc0`, which the slice does not hold: none taken to have one); else its type's
    /// slot, which reads its master's values before the master changes. Returns whether it ran.
    pub(crate) fn change_object(
        &mut self,
        id: ObjectId,
        change: Change,
        v: f32,
        attribute: MasterAttribute,
    ) -> Result<bool, GenieError> {
        let Some((owner, u)) = self.object(id) else {
            return Ok(false);
        };
        let master = u.master(&self.masters, owner)?.clone();
        match master.kind {
            master_type::BUILDING => {
                self.change_building(owner, id, &master, change, v, attribute)?;
            }
            master_type::COMBAT => self.change_combat(owner, id, &master, change, v, attribute)?,
            _ => self.change_static(owner, id, &master, change, v, attribute)?,
        }
        Ok(true)
    }

    /// `FUN_141195a50`, `FUN_141196060`, `FUN_1411962e0`: the static object's slots. The hit
    /// points take the master's share (a set: `v × hp / master's`, an add: `(master's + v) × hp /
    /// master's`, truncated to a short; a multiply: `v × hp + 0.5`, truncated, held at 32767 when
    /// that is not below it under the player's `+0x648` ([`CAPPED`])); the line of sight lets the old circle go and takes
    /// the new one (an object on the map in state 2: slot 0x1d8, then 0x1d0 with the new value,
    /// truncated); a set of a resource storage (21, 26, 27) moves its player's attribute from the
    /// old amount to the new one (slot 0xf8 twice) unless the master keeps it (flag `'@'`).
    pub(super) fn change_static(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: &Master,
        change: Change,
        v: f32,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::HIT_POINTS => {
                let (_, u) = self.object_mut(id).ok_or(OBJECT_GONE)?;
                let full = f32::from(master.hit_points);
                let hp = u.life.hp.get();
                let short = |x: f32| f32::from(crate::math::conversions::cvttss2si(x) as i16);
                u.life.hp = HitPoints::new(match change {
                    Change::Set => short(v * hp / full),
                    Change::Add => short((full + v) * hp / full),
                    // COMISS 32767, x; JBE: a NaN holds the cap too
                    Change::Multiply
                        if CAPPED
                            && crate::math::fmath::at_most_or_unordered(
                                OBJECT_HP_CAP,
                                v * hp + ROUND,
                            ) =>
                    {
                        OBJECT_HP_CAP
                    }
                    Change::Multiply => short(v * hp + ROUND),
                });
            }
            a::LINE_OF_SIGHT => {
                let los = match change {
                    Change::Set => v,
                    Change::Add => v + master.line_of_sight.get(),
                    Change::Multiply => (v * master.line_of_sight).get(),
                };
                self.sight_again(id, crate::math::conversions::cvttss2si(los))?;
            }
            a::STORAGE_1 | a::STORAGE_2 | a::STORAGE_3 if change == Change::Set => {
                let j = match attribute {
                    a::STORAGE_1 => 0,
                    a::STORAGE_2 => 1,
                    _ => 2,
                };
                if let Some(&(Some(kind), amount, flag)) = master.storage.get(j)
                    && flag != KEPT_STORAGE
                {
                    let owner = self.players.player_mut(p)?;
                    owner.add_attribute(kind, -amount)?;
                    owner.add_attribute(kind, Amount::new(v))?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The line of sight's change on object `id` (`FUN_141195a50`'s attribute 1): an object on
    /// the map (its tile `+0x2e8`, which one inside another has not) in state 2 lets its circle go
    /// (slot 0x1d8, its master's line of sight) and takes one of `los` (slot 0x1d0).
    pub(super) fn sight_again(&mut self, id: ObjectId, los: i32) -> Result<(), GenieError> {
        let Some((_, u)) = self.object(id) else {
            return Ok(());
        };
        if u.life.state != ObjectState::Alive || u.life.inside.is_some() {
            return Ok(());
        }
        let Some(placed) = self.spatial.get(id).copied() else {
            return Ok(());
        };
        if let Some(sight) = self.spatial.sight.as_mut() {
            sight.sight_off(id, placed.position, placed.radius, false, false)?;
            let _ = sight.sight_on_los(id, placed.position, placed.radius, false, los, false)?;
        }
        Ok(())
    }

    /// `FUN_1412163c0`, `FUN_1412167d0`, `FUN_141216970`: a combat unit's slots. Its charge
    /// (`+0x9e0`) keeps its share of the maximum (none: a set takes the value, an add adds it);
    /// its volleys (`+0xa00`) take the change; a set of the population (110) moves its player's
    /// attributes by its master's storage (headroom: plus the old amount's magnitude, then the
    /// new; population and resource 19: less the old, then plus the new's magnitude). The hero
    /// glow's particles (40) are not simulation; the rest is the static object's.
    pub(super) fn change_combat(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: &Master,
        change: Change,
        v: f32,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        use crate::objects::masters::attribute as a;
        match (attribute, change) {
            (a::HERO_STATUS, Change::Set | Change::Add) => Ok(()),
            (a::MAXIMUM_CHARGE, _) => {
                let (_, u) = self.object_mut(id).ok_or(OBJECT_GONE)?;
                u.charge = charge_after(u.charge, master.max_charge, change, v);
                Ok(())
            }
            (a::TOTAL_MISSILES, _) => {
                let (_, u) = self.object_mut(id).ok_or(OBJECT_GONE)?;
                u.firepower = match change {
                    Change::Set => v,
                    Change::Add => v + u.firepower,
                    Change::Multiply => v * u.firepower,
                };
                Ok(())
            }
            (a::POPULATION, Change::Set) => self.population_storage(p, master, v),
            _ => self.change_static(p, id, master, change, v, attribute),
        }
    }

    /// `FUN_1412163c0`'s population (110): for each of the master's three storage entries (`+0x60a`
    /// to `+0x60e`, their amounts `+0x610` on), one of the headroom (4) adds the old amount's
    /// magnitude then `v`; one of the population (11) or resource 19 its negation then `v`'s
    /// magnitude (slot 0xf8 each).
    pub(super) fn population_storage(
        &mut self,
        p: PlayerId,
        master: &Master,
        v: f32,
    ) -> Result<(), GenieError> {
        let owner = self.players.player_mut(p)?;
        for &(kind, amount, _) in master.storage.iter().take(3) {
            match kind {
                // The resources the population attribute (110) moves an object's storage of (`FUN_1412163c0`):
                // headroom (4), and current population (11) and 19, which hold its magnitude.
                Some(attribute::HEADROOM) => {
                    owner.add_attribute(attribute::HEADROOM, amount.abs())?;
                    owner.add_attribute(attribute::HEADROOM, Amount::new(v))?;
                }
                Some(kind @ (POPULATION | RESOURCE_19)) => {
                    owner.add_attribute(kind, -amount)?;
                    owner.add_attribute(kind, Amount::new(v.abs()))?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// `FUN_1411d4e70`, `FUN_1411d4f30`, `FUN_1411d4fe0`: a building's slots. Its build points
    /// (`+0xc50`) keep their share of its master's build time (its first train location's,
    /// `t`): a set `v / t × points`, an add `(t + v) / t × points`, a multiply
    /// `points / t × (t × v + 0.5, truncated to a short)`, or `1 × that` for a time of at most 0;
    /// its icon override (17, `+0xe`) is the interface's; the rest is the combat unit's.
    pub(super) fn change_building(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        master: &Master,
        change: Change,
        v: f32,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        use crate::objects::masters::attribute as a;
        match (attribute, change) {
            (a::BUILDING_ICON_OVERRIDE, Change::Set) => Ok(()),
            (a::TRAIN_TIME, _) => {
                let (_, u) = self.object_mut(id).ok_or(OBJECT_GONE)?;
                if let Some(c) = u.construction.as_mut() {
                    c.progress = build_points_after(c.progress, master.build_time, change, v);
                }
                Ok(())
            }
            _ => self.change_combat(p, id, master, change, v, attribute),
        }
    }
}

/// 1.0 (`DAT_14330eb8c`): a building whose master has no build time takes the new one as its
/// points.
pub(super) const ONE: f32 = crate::math::image::effect::ONE;
/// The player's `+0x648`, which caps an add or multiply of hit points (not held: 0 for every
/// player probed).
pub(super) const CAPPED: bool = false;

/// A combat unit's charge after its master's maximum charge (`max`, the old) changes
/// (`FUN_1412163c0`'s `;`): a set takes `v / max × charge` (`v` without a maximum), an add
/// `(v + max) / max × charge` (`v + charge` without one), a multiply `v × charge`.
pub(super) fn charge_after(charge: Charge, max: Charge, change: Change, v: f32) -> Charge {
    let (charge, max) = (charge.get(), max.get());
    Charge::new(match change {
        Change::Set if max != 0.0 => v / max * charge,
        Change::Set => v,
        Change::Add if max != 0.0 => (v + max) / max * charge,
        Change::Add => v + charge,
        Change::Multiply => v * charge,
    })
}

/// A building's build points after its master's build time (`time`, the old) changes
/// (`FUN_1411d4e70`'s `e`).
pub(super) fn build_points_after(points: f32, time: i16, change: Change, v: f32) -> f32 {
    let t = f32::from(time);
    match change {
        Change::Set => v / t * points,
        Change::Add => (t + v) / t * points,
        Change::Multiply => {
            let new = f32::from(crate::math::conversions::cvttss2si(t * v + ROUND) as i16);
            if 0.0 < t { points / t * new } else { ONE * new }
        }
    }
}

/// `unit` lies within the player's master table (`0 <= unit < player+0x20`): the slice's
/// [`Masters`] hold no count past their last master, which stands for the table's end.
pub(super) fn within_table(masters: &Masters, unit: MasterId) -> bool {
    masters.iter().any(|m| unit <= m.id)
}

/// The object has an action in its list (`+0x880`'s first node; a building producing has its
/// Make): the slice's top actions.
pub(super) fn has_actions(u: &Unit) -> bool {
    u.actions.artifact().is_some()
        || u.actions.build().is_some()
        || u.actions.move_to().is_some()
        || u.actions.fly().is_some()
        || u.actions.hunt().is_some()
        || u.actions.gather().is_some()
        || u.actions.attack().is_some()
        || u.actions.explore().is_some()
        || u.actions.enter().is_some()
        || u.actions.transport().is_some()
        || u.missile.is_some()
        || u.actions.make().is_some()
        || u.production.as_ref().is_some_and(|q| q.action.is_some())
}
