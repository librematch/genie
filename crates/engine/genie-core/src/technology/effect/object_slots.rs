//! An object's change by an attribute (its slots 0x78, 0x80 and 0x88): the static object's,
//! the combat unit's and the building's.

use crate::GenieResult;
use crate::combat::Charge;
use crate::combat::Firepower;
use crate::combat::missile::Missile;
use crate::map::Distance;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::at_most_or_unordered;
use crate::math::image::effect::{ONE as EFFECT_ONE, ROUND as EFFECT_ROUND};
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::tracker::ListKey;
use crate::production::Production;
use crate::technology::effect::{Change, OBJECT_GONE};
use crate::vision::SightCentre;
use crate::vision::SightLayer;
use crate::vision::SightShape;
use crate::vision::SightTiles;
use crate::vision::visibility::LosSight;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::slots::PlayerSlotsExt;

/// A storage flag of `'@'` (64): the object's storage stays with it (`FUN_141195a50`).
pub(super) const KEPT_STORAGE: u8 = 64;
/// A hit point total a capped multiply holds an object's below (`FUN_1411962e0`: `0x46fffe00`).
pub(super) const OBJECT_HP_CAP: f32 = 32767.0;
/// A multiply's rounding (`DAT_142e7f384`, 0.5).
pub(super) const ROUND: f32 = EFFECT_ROUND;

impl World {
    /// The ids player `player_id`'s tracker files under class `class` (`FUN_14104f8f0`, key
    /// class + 900) and those pending under it (`FUN_14104f9e0`, key class + 90900), each in the
    /// tracker's order. A queued unit's place holds −1, which no object has: every reader's lookup
    /// passes it over, so it is left out ([`tracked_ids`]).
    pub(super) fn tracked_class(
        &self,
        player_id: PlayerId,
        class: ClassId,
    ) -> GenieResult<(Vec<ObjectId>, Vec<ObjectId>)> {
        let tracker = &self.players.player(player_id)?.tracker;
        let key = ListKey::class(class);
        Ok((
            tracked_ids(tracker.list(key)),
            tracked_ids(tracker.list(key.pending())),
        ))
    }

    /// The ids player `player_id`'s tracker files as done under key master `key` (`FUN_14104f560`'s
    /// list of one key, whatever the objects' masters), in the tracker's order.
    pub(super) fn tracked_key(
        &self,
        player_id: PlayerId,
        key: MasterId,
    ) -> GenieResult<Vec<ObjectId>> {
        Ok(tracked_ids(
            self.players
                .player(player_id)?
                .tracker
                .list(ListKey::master(key)),
        ))
    }

    /// The ids player `player_id`'s tracker files under master `master` (`FUN_14104f960`), of which
    /// `FUN_14165a860` keeps those of that very master; and for a building master those pending
    /// under it, or under the stack unit it heads (`FUN_14104fa50`, key + 90000), whatever their
    /// masters. Each in the tracker's order.
    pub(super) fn tracked_master(
        &self,
        player_id: PlayerId,
        master: &Master,
    ) -> GenieResult<(Vec<ObjectId>, Vec<ObjectId>)> {
        let player = self.players.player(player_id)?;
        let of_master = |id: &ObjectId| {
            player
                .objects
                .iter()
                .chain(&player.sleeping)
                .any(|unit| unit.id == *id && unit.life.master_id == master.id)
        };
        let done: Vec<ObjectId> = tracked_ids(player.tracker.list(ListKey::master(master.id)))
            .into_iter()
            .filter(of_master)
            .collect();
        let mut pending = Vec::new();
        if MasterKind::BUILDING <= master.kind {
            let stack = master
                .building
                .as_ref()
                .and_then(|building_master| building_master.stack_unit);
            let key = stack
                .and_then(|stack| player_master(&self.model.masters, player_id, stack))
                .filter(|candidate| {
                    candidate.kind == MasterKind::BUILDING
                        && candidate.building.as_ref().is_some_and(|building_master| {
                            building_master.head_unit == Some(master.id)
                        })
                })
                .map_or(master.id, |master| master.id);
            pending = tracked_ids(player.tracker.list(ListKey::master(key).pending()));
        }
        Ok((done, pending))
    }
}

/// Whether a technology's change of an object ran ([`World::change_object`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub(crate) enum ObjectChange {
    /// Its type's slot ran.
    Ran,
    /// The world lacks the object.
    NotHeld,
}

/// The objects of a tracker's list, its queued units' places (−1) left out.
fn tracked_ids(list: &[Option<ObjectId>]) -> Vec<ObjectId> {
    list.iter().copied().flatten().collect()
}

impl World {
    /// `FUN_14165bae0` and the objects' slots 0x78, 0x80, 0x88 (by `change`) on object `id`:
    /// nothing for an object the world lacks (or one with a master of its own,
    /// `FUN_141188bc0`, which the slice does not hold: none taken to have one); else its type's
    /// slot, which reads its master's values before the master changes ([`ObjectChange::Ran`]).
    pub(crate) fn change_object(
        &mut self,
        id: ObjectId,
        change: Change,
        value: f32,
        attribute: MasterAttribute,
    ) -> GenieResult<ObjectChange> {
        let Some((owner, unit)) = self.object(id) else {
            return Ok(ObjectChange::NotHeld);
        };
        let master = unit.master(self.master_tables(), owner)?.clone();
        match master.kind {
            MasterKind::BUILDING => {
                self.change_building(owner, id, &master, change, value, attribute)?;
            }
            MasterKind::COMBAT => {
                self.change_combat(owner, id, &master, change, value, attribute)?;
            }
            _ => self.change_static(owner, id, &master, change, value, attribute)?,
        }
        Ok(ObjectChange::Ran)
    }

    /// `FUN_141195a50`, `FUN_141196060`, `FUN_1411962e0`: the static object's slots. The hit points
    /// take the master's share (a set: `value × hp / master's`, an add: `(master's + v) × hp /
    /// master's`, truncated to a short; a multiply: `value × hp + 0.5`, truncated, held at 32767
    /// when that is not below it under the player's `+0x648` ([`CAPPED`])); the line of sight lets
    /// the old circle go and takes the new one (an object on the map in state 2: slot 0x1d8, then
    /// 0x1d0 with the new value, truncated); a set of a resource storage (21, 26, 27) moves its
    /// player's attribute from the old amount to the new one (slot 0xf8 with 0 and 1 twice,
    /// [`World::add_player_attribute`]) unless the master keeps it (flag `'@'`).
    pub(super) fn change_static(
        &mut self,
        player_id: PlayerId,
        id: ObjectId,
        master: &Master,
        change: Change,
        value: f32,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::HIT_POINTS => {
                let (_, unit) = self.object_mut(id).ok_or(OBJECT_GONE)?;
                let full = HitPoints::of_master(master.hit_points).get();
                let hp = unit.life.hp.get();
                let short = |x: f32| f32::from(cvttss2si(x) as i16);
                unit.life.hp = HitPoints::new(match change {
                    Change::Set => short(value * hp / full),
                    Change::Add => short((full + value) * hp / full),
                    // COMISS 32767, x; JBE: a NaN holds the cap too
                    Change::Multiply
                        if CAPPED && at_most_or_unordered(OBJECT_HP_CAP, value * hp + ROUND) =>
                    {
                        OBJECT_HP_CAP
                    }
                    Change::Multiply => short(value * hp + ROUND),
                });
            }
            MasterAttribute::LINE_OF_SIGHT => {
                let los = match change {
                    Change::Set => value,
                    Change::Add => value + master.line_of_sight.get(),
                    Change::Multiply => (value * master.line_of_sight).get(),
                };
                self.sight_again(id, SightTiles::of(Distance::new(los)))?;
            }
            MasterAttribute::STORAGE_1
            | MasterAttribute::STORAGE_2
            | MasterAttribute::STORAGE_3
                if change == Change::Set =>
            {
                let slot = match attribute {
                    MasterAttribute::STORAGE_1 => 0,
                    MasterAttribute::STORAGE_2 => 1,
                    _ => 2,
                };
                if let Some(&(Some(kind), amount, flag)) = master.storage.get(slot)
                    && flag != KEPT_STORAGE
                {
                    // slot 0xf8 with 0 and 1, the old amount's sign flipped, then the new one
                    self.add_player_attribute(player_id, kind, -amount)?;
                    self.add_player_attribute(player_id, kind, Amount::new(value))?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The line of sight's change on object `id` (`FUN_141195a50`'s attribute 1): an object on
    /// the map (its tile `+0x2e8`, which one inside another has not) in state 2 lets its circle go
    /// (slot 0x1d8, its master's line of sight) and takes one of `los` (slot 0x1d0).
    pub(super) fn sight_again(&mut self, id: ObjectId, los: SightTiles) -> GenieResult<()> {
        let Some((_, unit)) = self.object(id) else {
            return Ok(());
        };
        if unit.life.state != ObjectState::Alive || unit.life.inside.is_some() {
            return Ok(());
        }
        let Some(placed) = self.spatial.get(id).copied() else {
            return Ok(());
        };
        if let Some(sight) = self.spatial.sight.as_mut() {
            sight.sight_off(
                id,
                placed.position,
                placed.radius,
                SightShape::Circle,
                SightCentre::Tile,
                SightLayer {
                    roles: &self.model.roles,
                    visibility_tuning: self.model.tuning.visibility,
                },
            )?;
            let _ = sight.sight_on_los(
                id,
                LosSight::tile_circle(placed.position, placed.radius, los),
                &self.model.roles,
                self.model.tuning.visibility,
            )?;
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
        player_id: PlayerId,
        id: ObjectId,
        master: &Master,
        change: Change,
        value: f32,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        use crate::objects::masters::MasterAttribute;
        match (attribute, change) {
            (MasterAttribute::HERO_STATUS, Change::Set | Change::Add) => Ok(()),
            (MasterAttribute::MAXIMUM_CHARGE, _) => {
                let _ = self.object(id).ok_or(OBJECT_GONE)?;
                let combat = self.parts.combat_mut(id)?;
                combat.charge = charge_after(
                    combat.charge,
                    master.creatable_part().max_charge,
                    change,
                    value,
                );
                Ok(())
            }
            (MasterAttribute::TOTAL_MISSILES, _) => self.change_firepower(id, change, value),
            (MasterAttribute::POPULATION, Change::Set) => {
                self.population_storage(player_id, master, value)
            }
            _ => self.change_static(player_id, id, master, change, value, attribute),
        }
    }

    /// [`World::change_combat`]'s volleys (`+0xa00`) of object `id`: a set takes `value`, an add
    /// adds it, a multiply scales them.
    fn change_firepower(&mut self, id: ObjectId, change: Change, value: f32) -> GenieResult<()> {
        let _ = self.object(id).ok_or(OBJECT_GONE)?;
        let combat = self.parts.combat_mut(id)?;
        combat.firepower = match change {
            Change::Set => Firepower::new(value),
            Change::Add => Firepower::new(value) + combat.firepower,
            Change::Multiply => Firepower::new(value * combat.firepower.get()),
        };
        Ok(())
    }

    /// `FUN_1412163c0`'s population (110): for each of the master's three storage entries (`+0x60a`
    /// to `+0x60e`, their amounts `+0x610` on), one of the headroom (4) adds the old amount's
    /// magnitude then `value`; one of the population (11) or resource 19 its negation then
    /// `value`'s magnitude (slot 0xf8 each).
    pub(super) fn population_storage(
        &mut self,
        player_id: PlayerId,
        master: &Master,
        value: f32,
    ) -> GenieResult<()> {
        let owner = self.players.player_mut(player_id)?;
        for &(kind, amount, _) in master.storage.iter().take(3) {
            // The resources the population attribute (110) moves an object's storage of
            // (`FUN_1412163c0`): headroom (4), and current population (11) and 19, which hold
            // its magnitude.
            let held = self.model.roles.of_some_attribute(kind);
            if held.contains(AttributeRoles::HEADROOM) {
                owner.add_attribute(
                    self.model.slots.attribute.headroom,
                    amount.abs(),
                    &self.model.roles,
                );
                owner.add_attribute(
                    self.model.slots.attribute.headroom,
                    Amount::new(value),
                    &self.model.roles,
                );
            } else if held.contains(AttributeRoles::MAGNITUDE_STORAGE)
                && let Some(kind) = kind
            {
                owner.add_attribute(kind, -amount, &self.model.roles);
                owner.add_attribute(kind, Amount::new(value.abs()), &self.model.roles);
            }
        }
        Ok(())
    }

    /// `FUN_1411d4e70`, `FUN_1411d4f30`, `FUN_1411d4fe0`: a building's slots. Its build points
    /// (`+0xc50`) keep their share of its master's build time (its first train location's, `time`):
    /// a set `value / time × points`, an add `(time + value) / time × points`, a multiply `points /
    /// time × (time × value + 0.5, truncated to a short)`, or `1 × that` for a time of at most 0;
    /// its icon override (17, `+0xe`) is the interface's; the rest is the combat unit's.
    pub(super) fn change_building(
        &mut self,
        player_id: PlayerId,
        id: ObjectId,
        master: &Master,
        change: Change,
        value: f32,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        use crate::objects::masters::MasterAttribute;
        match (attribute, change) {
            (MasterAttribute::BUILDING_ICON_OVERRIDE, Change::Set) => Ok(()),
            (MasterAttribute::TRAIN_TIME, _) => {
                let (_, layer) = self.object_and_layer_mut(id)?.ok_or(OBJECT_GONE)?;
                if let Some(construction) = layer.construction.as_mut() {
                    construction.progress = build_points_after(
                        construction.progress,
                        master.creatable_part().build_time,
                        change,
                        value,
                    );
                }
                Ok(())
            }
            _ => self.change_combat(player_id, id, master, change, value, attribute),
        }
    }
}

/// 1.0 (`DAT_14330eb8c`): a building whose master has no build time takes the new one as its
/// points.
pub(super) const ONE: f32 = EFFECT_ONE;
/// The player's `+0x648`, which caps an add or multiply of hit points (not held: 0 for every
/// player probed).
pub(super) const CAPPED: bool = false;

/// A combat unit's charge after its master's maximum charge (`max`, the old) changes
/// (`FUN_1412163c0`'s `;`): a set takes `value / max × charge` (`value` without a maximum), an add
/// `(value + max) / max × charge` (`value + charge` without one), a multiply `value × charge`.
pub(super) fn charge_after(charge: Charge, max: Charge, change: Change, value: f32) -> Charge {
    let (charge, max) = (charge.get(), max.get());
    Charge::new(match change {
        Change::Set if max != 0.0 => value / max * charge,
        Change::Set => value,
        Change::Add if max != 0.0 => (value + max) / max * charge,
        Change::Add => value + charge,
        Change::Multiply => value * charge,
    })
}

/// A building's build points after its master's build time (`time`, the old) changes
/// (in `FUN_1411d4e70`).
pub(super) fn build_points_after(
    points: Seconds,
    time: WholeSeconds,
    change: Change,
    value: f32,
) -> Seconds {
    let (points, seconds) = (points.get(), Seconds::of_master(time).get());
    Seconds::new(match change {
        Change::Set => value / seconds * points,
        Change::Add => (seconds + value) / seconds * points,
        Change::Multiply => {
            let new = f32::from(cvttss2si(seconds * value + ROUND) as i16);
            if 0.0 < seconds {
                points / seconds * new
            } else {
                ONE * new
            }
        }
    })
}

/// `unit` lies within the player's master table (`0 <= unit < player+0x20`): the slice's
/// [`Masters`] hold no count past their last master, which stands for the table's end.
pub(super) fn within_table(masters: &Masters, unit: MasterId) -> bool {
    masters.iter().any(|master| unit <= master.id)
}

/// The object has an action in its list (`+0x880`'s first node; a building producing has its
/// Make in its `production`, a missile its own part, `missile`): the slice's top actions.
pub(super) fn has_actions(
    actions: &Action,
    production: Option<&Production>,
    missile: Option<&Missile>,
) -> bool {
    actions.list.artifact().is_some()
        || actions.list.build().is_some()
        || actions.list.move_to().is_some()
        || actions.list.fly().is_some()
        || actions.list.hunt().is_some()
        || actions.list.gather().is_some()
        || actions.list.attack().is_some()
        || actions.list.explore().is_some()
        || actions.list.enter().is_some()
        || actions.list.transport().is_some()
        || missile.is_some()
        || actions.list.make().is_some()
        || production.is_some_and(|production| production.action.is_some())
}
