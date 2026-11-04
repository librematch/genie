//! A group's statistics (FD-036): `FUN_14100bcb0`, the centroid, radii, ranges, spacing and
//! speed of the members, the objects their searches list, and the formation's type after a
//! change of members (`FUN_14100b940`).

use crate::combat::attack::ATTACK;
use crate::groups::FormationKind;
use crate::groups::GroupCommandKind;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::LeavingWalk;
use crate::groups::MotionFlags;
use crate::groups::attack::moved_place;
use crate::groups::create::FormationUnit;
use crate::groups::formation::top_path;
use crate::groups::group_list::member_gone;
use crate::groups::{UnitGroup, WorldObject};
use crate::map::Distance;
use crate::map::spatial::SearchLists;
use crate::math::fmath::BVector;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered, sqrt};
use crate::movement::PathFlags;
use crate::movement::Speed;
use crate::objects::ObjectState;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::unit::UnitFlags;
use crate::objects::{ObjectId, ObjectKey};
use crate::players::UnitRelation;
use crate::unit_ai::AiAction;
use crate::unit_ai::AlertMode;
use crate::unit_ai::{Memory, Pick, Search, kind_attacks_neutral, search_area};
use crate::world::Seconds;
use crate::world::Unread;
use crate::world::World;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};
use crate::{GenieError, GenieResult};

/// The objects of the members' searches a group keeps at most (`FUN_140c65c60`'s `0x46`).
const MOST_OBJECTS: usize = 70;

/// The members' views in the members' order, each with the index the member has then.
type MemberViews = Vec<(usize, MemberView)>;

/// What the statistics read of one member.
struct MemberView {
    id: ObjectId,
    unit: FormationUnit,
    /// Slot 0x520's vector (x, height, y): where the unit last moved to.
    at: BVector,
    /// Slot 0x208: its top speed.
    speed: Speed,
    /// Slot 0x230: its weapon range.
    range: Distance,
    /// `+0x685`, `under_attack_value` (`FUN_1411a6f80`).
    under_attack: bool,
    /// Its unit AI's current action is the attack.
    attacking: bool,
}

/// What the group statistics (`FUN_14100bcb0`'s `param_2`) do with a member far from its slot
/// ([`World::group_statistics`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FarMembers {
    /// It leaves the group (1).
    Leave,
    /// It stays, untested (0).
    Stay,
}

impl World {
    /// `FUN_14100bcb0` with `far` (`param_2`): the statistics start over (the centroid, the largest
    /// radius and range, the spacing sums and the searches' objects at 0, the smallest radius,
    /// range and ranged range at `f32::MAX`); each member the world holds then, in the members'
    /// order, lists the objects its unit AI's search finds ([`World::member_objects`]); with
    /// [`FarMembers::Leave`], one far from its slot ([`World::far_from_slot`]) leaves the group
    /// (`removeUnit` with target -1, [`World::remove_unit`]) and, when it left, stops (slot 0x5d0)
    /// and the next member takes its index; either way it counts in no statistic. Any other adds
    /// its place (slot 0x520) to the centroid and its radius ([`FormationUnit`]) to the spacing,
    /// and takes part in the largest and smallest radius, the slowest speed (slot 0x208) and the
    /// ranges (slot 0x230; the ranged ones only above 0); its seconds since attacking are 0 in the
    /// attack action (600), else they grow by the update's time. With members, the centroid and the
    /// desired height are their means, the spacing twice the largest radius, the total spacing
    /// `(largest + sum) + (largest + sum)` in that order, the stored positions' desired length the
    /// members' count times the spacing; the motion's top speed is the slowest member's (divided by
    /// `GroupTuning::slow_group_factor` with [`GroupFlags::CATCHING_UP`]), the seconds not under
    /// attack 0 when a member is under attack, else they grow by the update's time; a member inside
    /// another object stops ([`World::stop_if_inside`]); each sub-formation's spacing is twice its
    /// widest unit's radius and its attack range its units' largest against the group's target
    /// ([`World::range_against`]), its units looked up in the world's table; with
    /// [`MotionFlags::NEW`] the formation type follows ([`World::calc_formation_kind`]). Without
    /// one the smallest radius, the spacing and the smallest ranges are 0.
    ///
    /// Not modelled: `+0x478` (a member's `+0x30c`; the group holds no such field), a charge's
    /// range against a target, and the attack plan's re-attack (`FUN_14100f7f0`) against a
    /// target the world holds.
    pub fn group_statistics(&mut self, group: GroupId, far: FarMembers) -> GenieResult<()> {
        if self.statistics_only(group, far)? {
            self.calc_formation_kind(group)?;
        }
        Ok(())
    }

    /// [`World::group_statistics`] up to the formation type: true when that follows
    /// ([`MotionFlags::NEW`]).
    ///
    /// # Errors
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn statistics_only(&mut self, group: GroupId, far: FarMembers) -> GenieResult<bool> {
        let dt = self.dt_seconds;
        let (views, objects) = self.member_views(group, far)?;
        let unit_group = self.groups.held_mut(group)?;
        let (held, mut slowest, under_attack) = member_totals(unit_group, &views, objects, dt)?;
        if held == 0 {
            unit_group.smallest_unit_radius = Distance::ZERO;
            unit_group.desired_spacing = Distance::ZERO;
            unit_group.smallest_unit_range = Distance::ZERO;
            unit_group.smallest_ranged_unit_range = Distance::ZERO;
            return Ok(false);
        }
        // a member inside another object (`+0x2f0`) stops (slot 0x5d0), in the members' order
        for (_, member_view) in &views {
            self.stop_if_inside(member_view.id)?;
        }
        let unit_group = self.groups.held_mut(group)?;
        let held_count = held as f32;
        unit_group.centroid = BVector::new(
            unit_group.centroid.x() / held_count,
            unit_group.centroid.height() / held_count,
            unit_group.centroid.y() / held_count,
        );
        unit_group.desired_height /= held_count;
        let largest = unit_group.largest_unit_radius;
        unit_group.desired_spacing = largest + largest;
        unit_group.total_spacing =
            largest + unit_group.total_spacing + largest + unit_group.total_spacing;
        unit_group.stored_positions.desired_length =
            (largest + largest) * unit_group.members.len() as f32;
        if unit_group.flags.contains(GroupFlags::CATCHING_UP) {
            slowest /= self.model.tuning.group.slow_group_factor;
        }
        unit_group.motion.maximum_velocity_forward = Speed::new(slowest);
        if under_attack {
            unit_group.seconds_not_under_attack = Seconds::ZERO;
        } else {
            unit_group.seconds_not_under_attack += dt;
        }
        let old_plan = unit_group.attack_plan_kind;
        unit_group.attack_plan_kind = 0;
        // the readers look the target's integer up as an object, whatever its kind
        let target = unit_group.current_target.target.key();
        // each sub-formation with units: its attack range the largest of its units' slot 0x238
        // against the group's target, its spacing twice its widest unit (the first unit's
        // place counts even when the world does not hold it); its units are looked up in the
        // world's table, members or not
        self.sub_formation_stats(group, target)?;
        let group = self.groups.held(group)?;
        // the re-attack: an attack plan of 1 before, command 9 (attack) and a target the
        // world's table holds; a target it does not hold does nothing
        if old_plan == 1
            && group.current_command == GroupCommandKind::ATTACK
            && self.world_object(target).is_some()
        {
            return Err(GenieError::unsupported(
                "a group's attack plan that attacks again (FUN_14100f7f0)",
            ));
        }
        // the motion flags' (`+0xb8`, 0x14100c657), not the group flags'
        Ok(group.motion.flags.contains(MotionFlags::NEW))
    }

    /// [`World::statistics_only`]'s member loop: each member's view in the members' order with the
    /// index it has then, and the objects of their searches.
    fn member_views(
        &mut self,
        group: GroupId,
        far: FarMembers,
    ) -> GenieResult<(MemberViews, Vec<ObjectId>)> {
        // each member's view in the members' order, its search first (FUN_140c65c60 before
        // anything else of it), then the far test; the index the member has then, which a
        // later member's leaving does not move
        let mut views: MemberViews = Vec::new();
        let mut objects = Vec::new();
        // `+0x154` as the loop goes, which the far test reads
        let mut largest = 0.0f32;
        let mut index = 0;
        loop {
            let held = self.groups.held(group)?;
            let Some(member) = held.members.get(index) else {
                break;
            };
            let id = member.id;
            let Some((player_id, object)) = self.object(id) else {
                index += 1;
                continue;
            };
            let master = object.master(self.master_tables(), player_id)?;
            let unit = FormationUnit::of(master, &self.model.tuning.group);
            let ai = self
                .parts
                .unit_ai
                .get(object.id)
                .ok_or(GenieError::unsupported("a group member without a unit AI"))?;
            let view = MemberView {
                id,
                unit,
                // the game's vector (x, height, y) of where it last moved to (x, y, height)
                at: moved_place(object, self.parts.mover_of(object.id)),
                speed: self
                    .parts
                    .mover_of(object.id)
                    .map_or(Speed::ZERO, |mover| mover.stats.speed),
                range: weapon_range(master),
                under_attack: object.flags.contains(UnitFlags::UNDER_ATTACK),
                attacking: ai.current_action == Some(AiAction::Attack),
            };
            self.member_objects(id, &mut objects)?;
            if far == FarMembers::Leave && self.far_from_slot(group, &view, largest)? {
                if self.remove_unit(group, id, None, LeavingWalk::Stands)? {
                    if self.object(id).is_some()
                        && let Some(mover) = self.parts.mover_mut(id)
                    {
                        mover.stop()?;
                    }
                    // the index steps back: the next member takes this one's place
                    continue;
                }
                index += 1;
                continue;
            }
            if largest < view.unit.radius.get() {
                largest = view.unit.radius.get();
            }
            views.push((index, view));
            index += 1;
        }
        Ok((views, objects))
    }

    /// [`World::statistics_only`]'s sub-formations: each one with units gets its attack range
    /// against `target` and its spacing.
    fn sub_formation_stats(&mut self, group: GroupId, target: Option<ObjectId>) -> GenieResult<()> {
        let held = self.groups.held(group)?;
        let stats: Vec<Vec<ObjectId>> = held
            .unit_kinds
            .iter()
            .map(|unit_kind_statistic| unit_kind_statistic.units.clone())
            .collect();
        for (category, units) in stats.iter().enumerate() {
            if units.is_empty() {
                continue;
            }
            let (mut range, mut widest) = (Distance::ZERO, Distance::ZERO);
            for (index, &id) in units.iter().enumerate() {
                let Some(object) = self.world_object(id) else {
                    continue;
                };
                let distance = self.range_against(object, self.world_object(target))?;
                // COMISS/JBE at 0x14100c506 and 0x14100c51e
                if range < distance {
                    range = distance;
                }
                let radius = match object {
                    WorldObject::Unit(owner, unit, _) => {
                        FormationUnit::of(
                            unit.master(self.master_tables(), owner)?,
                            &self.model.tuning.group,
                        )
                        .radius
                    }
                    WorldObject::Ghost(player_id, _) => {
                        FormationUnit::of(
                            self.master(player_id, object.master_id(self.model.slots))?,
                            &self.model.tuning.group,
                        )
                        .radius
                    }
                };
                if index == 0 || widest < radius {
                    widest = radius;
                }
            }
            let unit_group = self.groups.held_mut(group)?;
            unit_group.unit_kinds.checked_mut(category)?.attack_range = range;
            unit_group.unit_kinds.checked_mut(category)?.desired_spacing = widest + widest;
        }
        Ok(())
    }

    /// `FUN_140c65c60` with -1 and [`MOST_OBJECTS`] for member `id` into the group's
    /// sighted objects (`+0x49c`, its count `+0x5b4`, [`crate::groups::UnitGroup::sighted`]):
    /// nothing once they are full. Else its unit AI's
    /// slot 0x50 with 1 from its place (x, y): the search (`FUN_140c63e20` mode 1, every type's,
    /// [`crate::unit_ai::Pick::Look`] over its type's area, [`crate::unit_ai::search_area`]),
    /// which, when it queried
    /// afresh, sets the alert mode (`+0x1b8`) to whether it found enemies and, with enemies,
    /// restarts the time since a sighting (`+0x1b4`; a computer player's notify after 10 s is
    /// not modelled). Then the objects of the search's lists 4, 3 and 2 in that order
    /// ([`crate::map::spatial::Spatial::searched`]) that the world's table holds in a state below 3
    /// (`+0x640`), of any owner, the third list's only of a class the unit AI attacks among
    /// neutrals (slot 0xa8, [`crate::unit_ai::kind_attacks_neutral`]), join the list: each
    /// once when it held any on entry (the duplicate test runs only then), and the list stops at
    /// [`MOST_OBJECTS`].
    ///
    /// # Errors
    ///
    /// A member without a unit AI (the game calls through its null pointer) or of a computer
    /// player (its notify), and the search's.
    fn member_objects(&mut self, id: ObjectId, objects: &mut Vec<ObjectId>) -> GenieResult<()> {
        if MOST_OBJECTS <= objects.len() {
            return Ok(());
        }
        let on_entry = objects.len();
        let (player_id, held) = self.object(id).ok_or(GenieError::unsupported(
            "a group member's search for a unit not held",
        ))?;
        let ai = self
            .parts
            .unit_ai
            .get(held.id)
            .ok_or(GenieError::unsupported("a group member without a unit AI"))?;
        let ai_kind = ai.ai_kind;
        let player_kind = self.players.player(player_id)?.kind;
        if player_kind.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's group member's search (its cared classes)",
            ));
        }
        let master = held.master(self.master_tables(), player_id)?;
        let attack_priority = master.creatable_part().attack_priority;
        let (classes, radius) = search_area(
            ai_kind,
            Some(master.class),
            master.line_of_sight,
            master.action_part().search_radius,
            player_kind,
            &self.model.roles,
        );
        let search = Search {
            at: held.position().ground(),
            radius,
            classes,
            size: master.radius,
            pick: Pick::Look,
            memory: Memory::default(),
        };
        let fresh = self.spatial.search_query(
            player_id,
            &search,
            &self.model.tuning.spatial,
            self.model.tuning.registry,
        )? == SearchLists::Fresh;
        let lists = self.spatial.searched().ok_or(GenieError::unsupported(
            "a group member's search that reads the registry's last answer (before any query)",
        ))?;
        if fresh {
            let enemies = !lists[UnitRelation::Enemy].is_empty();
            if let Some(ai) = self.parts.unit_ai.get_mut(id) {
                ai.alert_mode = AlertMode::of_search(enemies);
                if enemies {
                    ai.time_since_enemy = Seconds::ZERO;
                }
            }
        }
        for list in [
            UnitRelation::Enemy,
            UnitRelation::Neutral,
            UnitRelation::Ally,
        ] {
            for record in &lists[list] {
                let Some(object) = self.world_object(record.id) else {
                    continue;
                };
                let state = match object {
                    WorldObject::Unit(_, unit, _) => unit.life.state,
                    WorldObject::Ghost(_, ghost) => ghost.state,
                };
                if ObjectState::Dying <= state {
                    continue;
                }
                if list == UnitRelation::Neutral {
                    let class = self
                        .master(object.player(), object.master_id(self.model.slots))?
                        .class;
                    if !kind_attacks_neutral(ai_kind, attack_priority, class, &self.model.roles) {
                        continue;
                    }
                }
                if 0 < on_entry && objects.contains(&record.id) {
                    continue;
                }
                objects.push(record.id);
                if MOST_OBJECTS <= objects.len() {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Slot 0x238 of `object` against `target` (the group's, as the world's table gives it): a
    /// combat object's (types 50 to 80, `FUN_14108d9b0`) weapon range (slot 0x230,
    /// [`weapon_range`]) plus its master's `charge_event` (`+0x920`) when slot 0x2f8
    /// (`FUN_14120f510`, [`crate::combat::damage::range_charge_hit`]) holds against the target, or
    /// without one its top action's target: a charge of type 6 or 7 (slot 0x308) that works on
    /// it ([`crate::combat::damage::charge_works`]) and is ready (slot 0x300: at least 1); any other
    /// object's 0 (`FUN_14106ff60`).
    ///
    /// # Errors
    ///
    /// An object of a master the slice does not have.
    pub(super) fn range_against(
        &self,
        object: WorldObject<'_>,
        target: Option<WorldObject<'_>>,
    ) -> GenieResult<Distance> {
        let master = self.master(object.player(), object.master_id(self.model.slots))?;
        if !COMBAT_KINDS.contains(&master.kind) {
            return Ok(Distance::ZERO);
        }
        let range = weapon_range(master);
        let WorldObject::Unit(_, unit, _) = object else {
            return Ok(range);
        };
        let target = target.or_else(|| {
            self.parts
                .actions_of(unit.id)
                .top_target()
                .filter(|&object_id| !object_id.is_recyclable())
                .and_then(|object_id| self.world_object(object_id))
        });
        let Some(world_object) = target else {
            return Ok(range);
        };
        // slot 0x300 of a charge type other than 1 and 3: at least 1 (`COMISS`, a NaN is not)
        Ok(if self.charge_hits(unit, master, &world_object)? {
            range + master.creatable_part().charge_event.range()
        } else {
            range
        })
    }

    /// `FUN_14100bcb0`'s far test (`0x14100be04`..`0x14100c1c9`, `param_2` 1) of member `view` of
    /// group `group`, `largest` the largest radius of the members before it: the group walks no
    /// patrol (the top path's [`PathFlags::LOOPS`], `FUN_141046500`), its flags lack
    /// [`GroupFlags::PATROL`], the member's action list holds no Attack (slot 0x538 with 9) and it
    /// has a slot in the formation (the first by its id) farther on the ground from where it last
    /// moved to (slot 0x520) than `GroupTuning::far`, or `GroupTuning::far_wide` in a formation of type 7 or once a
    /// member of radius `GroupTuning::wide_radius` or more came before (`COMISS` with `JC`: a NaN radius is
    /// not wide; `COMISS` with `JBE`: a NaN distance is not far).
    ///
    /// # Errors
    ///
    /// An action the slice does not model, and a member of a master the slice does not have.
    fn far_from_slot(&self, group: GroupId, view: &MemberView, largest: f32) -> GenieResult<bool> {
        let held = self.groups.held(group)?;
        let patrol = top_path(&held.motion.paths)
            .is_some_and(|path_data| path_data.path_flags.contains(PathFlags::LOOPS));
        if patrol || held.flags.contains(GroupFlags::PATROL) {
            return Ok(false);
        }
        let (player_id, unit) = self.object(view.id).ok_or_else(|| member_gone(view.id))?;
        // The types of action objects (`RGE_Action_Object`), whose action list slot 0x538 reads.
        if unit.master(self.master_tables(), player_id)?.kind >= MasterKind::ACTION {
            if let Some(error) = self.parts.unread.get(unit.id).map(Unread::error) {
                return Err(error);
            }
            if self.parts.actions_of(unit.id).top_action_kind() == Some(ATTACK) {
                return Ok(false);
            }
        }
        let Some(formation) = held.current_formation.as_ref() else {
            return Ok(false);
        };
        let Some(slot) = formation
            .slots
            .iter()
            .find(|slot| slot.unit_id == Some(view.id))
        else {
            return Ok(false);
        };
        let mut offset = view.at - slot.transform_position;
        offset = offset.with_height(0.0);
        let limit = if formation.kind == FormationKind::Staggered
            || !below_or_unordered(largest, self.model.tuning.group.wide_radius)
        {
            self.model.tuning.group.far_wide
        } else {
            self.model.tuning.group.far
        };
        let gap = sqrt(offset.x() * offset.x() + 0.0 + offset.y() * offset.y());
        Ok(!at_most_or_unordered(gap, limit))
    }

    /// A member inside another object (`+0x2f0`: another object's garrison holds it) stops
    /// (slot 0x5d0, `FUN_14108c2f0`, [`crate::movement::Mover`]'s stop), as `FUN_14100bcb0` does
    /// for each such member it reads.
    ///
    /// # Errors
    ///
    /// What the stop does not model.
    fn stop_if_inside(&mut self, id: ObjectId) -> GenieResult<()> {
        let inside = self
            .players
            .iter()
            .flat_map(|player| player.objects.iter().chain(&player.sleeping))
            .any(|unit| unit.garrison.contains(&id));
        if !inside {
            return Ok(());
        }
        if self.object(id).is_some()
            && let Some(mover) = self.parts.mover_mut(id)
        {
            mover.stop()?;
        }
        Ok(())
    }
}

/// The master types of the combat objects (`RGE_Combat_Object` and its subclasses: combat,
/// missile, TRIBE combat and building), whose slot 0x230 is `FUN_14108d960`.
pub(in crate::groups) const COMBAT_KINDS: [MasterKind; 4] = [
    MasterKind::RGE_COMBAT,
    MasterKind::MISSILE,
    MasterKind::COMBAT,
    MasterKind::BUILDING,
];

/// [`World::statistics_only`]'s totals: the group's statistics reset, then the members' views
/// added up (centroid, radii, ranges, attack times), `objects` its search list; the members
/// counted, the slowest speed and whether any member is under attack.
fn member_totals(
    group: &mut UnitGroup,
    views: &[(usize, MemberView)],
    objects: Vec<ObjectId>,
    dt: Seconds,
) -> GenieResult<(i32, f32, bool)> {
    group.centroid = BVector::ZERO;
    group.desired_height = 0.0;
    group.largest_unit_radius = Distance::ZERO;
    group.smallest_unit_radius = Distance::MAX;
    group.desired_spacing = Distance::ZERO;
    group.smallest_unit_range = Distance::new(f32::MAX);
    group.smallest_ranged_unit_range = Distance::new(f32::MAX);
    group.largest_unit_range = Distance::ZERO;
    group.largest_ranged_unit_range = Distance::ZERO;
    group.total_spacing = Distance::ZERO;
    group.sighted = objects;
    let mut slowest = f32::MAX;
    let mut held = 0;
    let mut under_attack = false;
    for &(index, ref member_view) in views {
        held += 1;
        group.centroid = BVector::new(
            group.centroid.x() + member_view.at.x(),
            group.centroid.height() + member_view.at.height(),
            group.centroid.y() + member_view.at.y(),
        );
        let distance = member_view.unit.radius;
        group.total_spacing += distance;
        if group.largest_unit_radius < distance {
            group.largest_unit_radius = distance;
        }
        if distance < group.smallest_unit_radius {
            group.smallest_unit_radius = distance;
        }
        if member_view.speed < Speed::new(slowest) {
            slowest = member_view.speed.get();
        }
        if member_view.range < group.smallest_unit_range {
            group.smallest_unit_range = member_view.range;
        }
        if group.largest_unit_range < member_view.range {
            group.largest_unit_range = member_view.range;
        }
        if Distance::ZERO < member_view.range
            && member_view.range < group.smallest_ranged_unit_range
        {
            group.smallest_ranged_unit_range = member_view.range;
        }
        if Distance::ZERO < member_view.range && group.largest_ranged_unit_range < member_view.range
        {
            group.largest_ranged_unit_range = member_view.range;
        }
        under_attack |= member_view.under_attack;
        let member = group.members.checked_mut(index)?;
        if member_view.attacking {
            member.seconds_since_attacking = Seconds::ZERO;
        } else {
            member.seconds_since_attacking += dt;
        }
    }
    Ok((held, slowest, under_attack))
}

/// Slot 0x230 of an object of `master`: a combat object's weapon range (`+0x828`,
/// `FUN_14108d960`), any other's 0 (`FUN_14106ff50`).
pub(crate) fn weapon_range(master: &Master) -> Distance {
    if COMBAT_KINDS.contains(&master.kind) {
        master.combat_part().weapon_range
    } else {
        Distance::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Charge;
    use crate::groups::FormationCategory;
    use crate::groups::GroupTarget;
    use crate::groups::{Member, UnitGroup, UnitGroups, UnitKindStatistic};
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Point;
    use crate::map::Position;
    use crate::objects::ListIndex;
    use crate::objects::masters::ChargeEvent;
    use crate::objects::masters::ChargeKind;
    use crate::objects::masters::ChargeTarget;
    use crate::objects::masters::ClassId;
    use crate::objects::masters::CombatPart;
    use crate::objects::masters::CreatablePart;
    use crate::objects::masters::FormationClass;
    use crate::objects::masters::MasterId;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::unit::Unit;
    use crate::players::PlayerId;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::vision::VisionBits;
    use crate::world::Millis;
    use crate::world::WorldTime;
    use std::sync::Arc;

    #[cfg(test)]
    use crate::gathering::hunt::tests::walker;
    #[cfg(test)]
    use crate::groups::{Formation, Slot};
    #[cfg(test)]
    use crate::map::spatial::Spatial;
    #[cfg(test)]
    use crate::objects::layers::Static;
    #[cfg(test)]
    use crate::objects::masters::SpacingModifier;
    #[cfg(test)]
    use crate::players::ByPlayer;
    use crate::players::relation_row;
    use crate::players::relations;
    use crate::test_ids::group_id;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::test_ids::object_id_list;
    #[cfg(test)]
    use crate::unit_ai::UnitAi;
    use crate::unit_ai::UnitAiKind;
    #[cfg(test)]
    use crate::vision::registry::Registry;
    use crate::vision::registry::UnitUpdate;
    #[cfg(test)]
    use crate::vision::visibility::Visibility;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Player;

    /// Slot 0x238 (`FUN_14108d9b0`): the weapon range, plus `charge_event` for a ready charge of
    /// type 6 or 7 that works on the target (here every class, −1); without the charge ready, of
    /// another type, on no target (and no action's) or on a class outside its groups, the
    /// weapon range alone.
    #[test]
    fn ready_charge_of_kind_6_adds_its_charge_event_to_the_range() {
        let mut searched_world = searched_world();
        let charging = |world: &mut World, charge_kind: i16, charge_target: i16| {
            let master = Arc::make_mut(&mut world.model.masters[1])
                .get_mut(master_id(4))
                .unwrap();
            let creatable_part = master.creatable.get_or_insert_with(Default::default);
            (
                creatable_part.charge_kind,
                creatable_part.charge_target,
                creatable_part.charge_event,
            ) = (
                ChargeKind::of_data(charge_kind),
                ChargeTarget::of_data(charge_target),
                ChargeEvent::of_data(2),
            );
        };
        let range = |world: &World, target: Option<ObjectId>| {
            let me = world.world_object(object_id(10)).unwrap();
            world
                .range_against(me, target.and_then(|id: ObjectId| world.world_object(id)))
                .unwrap()
        };
        charging(&mut searched_world, 6, -1);
        assert_eq!(
            range(&searched_world, Some(object_id(20))),
            Distance::new(1.0)
        );
        searched_world
            .parts
            .combat_mut(searched_world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .charge = Charge::new(1.0);
        assert_eq!(
            range(&searched_world, Some(object_id(20))),
            Distance::new(3.0)
        );
        assert_eq!(range(&searched_world, None), Distance::new(1.0));
        charging(&mut searched_world, 7, 1 << 5);
        assert_eq!(
            (
                range(&searched_world, Some(object_id(20))),
                range(&searched_world, Some(object_id(31)))
            ),
            (Distance::new(1.0), Distance::new(3.0))
        );
        charging(&mut searched_world, 1, -1);
        assert_eq!(
            range(&searched_world, Some(object_id(20))),
            Distance::new(1.0)
        );
    }

    /// Player 1's soldier 10 (master 4, class 6) at (5, 5) in group 1; player 2 (its enemy)
    /// holds 20 at (6, 5) and 21 (state 3) at (5, 6); player 3 (neutral) holds 30 (class 6) at
    /// (4, 5) and 31 (class 4, a villager) at (5, 4); every one of them in player 1's sight.
    fn searched_world() -> World {
        let unit = |id: i32, master: MasterId, x: f32, y: f32| {
            let mut unit = Unit {
                id: object_id(id),
                place: Static::at(Position::new(x, y, 0.0)),
                ..Unit::default()
            };
            unit.life.master_id = master;
            unit.life.state = ObjectState::Alive;
            unit
        };
        let soldier = unit(10, master_id(4), 5.0, 5.0);
        let soldier_ai = UnitAi {
            ai_kind: UnitAiKind::Soldier,
            ..UnitAi::default()
        };
        let mut dying = unit(21, master_id(4), 5.0, 6.0);
        dying.life.state = ObjectState::Dying;
        let players = vec![
            Player::default(),
            Player {
                objects: vec![soldier].into(),
                ..Player::default()
            },
            Player {
                objects: vec![unit(20, master_id(4), 6.0, 5.0), dying].into(),
                ..Player::default()
            },
            Player {
                objects: vec![
                    unit(30, master_id(4), 4.0, 5.0),
                    unit(31, ids::ID_6, 5.0, 4.0),
                ]
                .into(),
                ..Player::default()
            },
        ];
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(0),
            PlayerId::new(0),
            vec![Millis::ZERO; 4],
            players,
        )
        .with_ais([(object_id(10), soldier_ai)]);
        let master = |id: MasterId, class: ClassId| Master {
            id,
            kind: MasterKind::COMBAT,
            class,
            line_of_sight: Distance::new(4.0),
            radius: Footprint::new(0.2, 0.2),
            creatable: Some(CreatablePart {
                formation_category: FormationClass::of_data(1),
                ..CreatablePart::default()
            }),
            combat: Some(CombatPart {
                weapon_range: Distance::new(1.0),
                ..CombatPart::default()
            }),
            ..Master::default()
        };
        world.model.masters = vec![
            Masters::new([
                master(master_id(4), class::CLASS_6),
                master(ids::ID_6, class::VILLAGER)
            ]);
            4
        ]
        .into_iter()
        .map(Arc::new)
        .collect();
        let mut spatial = Spatial::new(GridSize::new(20, 20));
        spatial.relations = ByPlayer::new(vec![
            relation_row([0, 3, 3, 3]),
            relation_row([0, 0, 3, 1]),
            relation_row([0, 3, 0, 3]),
            relation_row([0, 1, 3, 0]),
        ]);
        spatial.unit_diplomacy = ByPlayer::new(vec![
            relations([1, 4, 4, 4]),
            relations([0, 1, 4, 3]),
            relations([0, 4, 1, 4]),
            relations([0, 3, 4, 1]),
        ]);
        let mut registry = Registry::new(4, 65);
        for (id, owner, at, class) in [
            (20, 2, Point::new(6.0, 5.0), class::CLASS_6),
            (21, 2, Point::new(5.0, 6.0), class::CLASS_6),
            (30, 3, Point::new(4.0, 5.0), class::CLASS_6),
            (31, 3, Point::new(5.0, 4.0), class::VILLAGER),
        ] {
            registry
                .update_unit(UnitUpdate {
                    id: object_id(id),
                    owner: PlayerId::new(owner),
                    position: at,
                    radii: Footprint::new(0.2, 0.2),
                    class,
                    old: VisionBits::NONE,
                    new: VisionBits::from_bits_retain(1 << 1),
                })
                .unwrap();
        }
        let mut visibility = Visibility::new(
            GridSize::new(20, 20),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
            Vec::new(),
        );
        visibility.registry = Some(registry);
        spatial.sight = Some(visibility);
        world.spatial = spatial;
        let mut group = UnitGroup::new(group_id(1));
        group.owner = Some(PlayerId::new(1));
        group.members = vec![Member {
            id: object_id(10),
            ..Member::default()
        }];
        group.unit_kinds = vec![UnitKindStatistic {
            formation_category: FormationCategory::named(1),
            units: vec![object_id(10)],
            ..UnitKindStatistic::default()
        }];
        world.groups = UnitGroups {
            groups: vec![group],
            ..UnitGroups::default()
        };
        world
    }

    /// `FUN_140c65c60`: the member's search lists its enemies (list 4: 20, not 21 in state 3),
    /// then the neutrals it attacks (list 3: 30, not the villager 31, slot 0xa8); the fresh
    /// query sets the alert mode for the enemies and restarts the time since a sighting. Again
    /// from the same place (the cache's answer, not fresh) the list held objects on entry, so
    /// nothing joins twice; a list one short of 70 takes one more.
    #[test]
    fn member_search_lists_its_enemies_then_the_neutrals_it_attacks() {
        let mut world = searched_world();
        world
            .parts
            .unit_ai
            .get_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .time_since_enemy = Seconds::new(7.0);
        let mut objects = Vec::new();
        world.member_objects(object_id(10), &mut objects).unwrap();
        assert_eq!(objects, [object_id(20), object_id(30)]);
        let ai = world
            .parts
            .unit_ai
            .get(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap();
        assert_eq!(
            (ai.alert_mode, ai.time_since_enemy),
            (AlertMode::Alert, Seconds::ZERO)
        );
        world.member_objects(object_id(10), &mut objects).unwrap();
        assert_eq!(objects, [object_id(20), object_id(30)]);
        let mut full: Vec<ObjectId> = object_id_list(1000..1069);
        world.member_objects(object_id(10), &mut full).unwrap();
        assert_eq!((full.len(), full[69]), (70, object_id(20)));
        world.member_objects(object_id(10), &mut full).unwrap();
        assert_eq!(full.len(), 70);
    }

    /// Slot 0x50's search and slot 0xa8 by the member's type: a transport (type 11) cares about
    /// neither class around (6, 4), so it lists nothing and finds no enemy; a civilian (8) lists
    /// its enemies within its grown radius (22, 8 tiles off: its line of sight 4 plus 1 plus 7)
    /// but no neutral (its slot 0xa8 never attacks one); a soldier the near ones only.
    #[test]
    fn member_search_takes_its_kind_classes_radius_and_neutral_test() {
        let world = |ai_kind: UnitAiKind| {
            let mut searched_world = searched_world();
            let mut far =
                searched_world.object_of(&searched_world.players[2].objects[ListIndex::new(0)]);
            far.unit.id = object_id(22);
            far.unit.place_for_test(Position::new(13.0, 5.0, 0.0));
            let _ = searched_world.push_live(PlayerId::new(2), far).unwrap();
            let sight = searched_world.spatial.sight.as_mut().unwrap();
            sight
                .registry
                .as_mut()
                .unwrap()
                .update_unit(UnitUpdate {
                    id: object_id(22),
                    owner: PlayerId::new(2),
                    position: Point::new(13.0, 5.0),
                    radii: Footprint::new(0.2, 0.2),
                    class: class::CLASS_6,
                    old: VisionBits::NONE,
                    new: VisionBits::from_bits_retain(1 << 1),
                })
                .unwrap();
            searched_world
                .parts
                .unit_ai
                .get_mut(searched_world.players[1].objects[ListIndex::new(0)].id)
                .unwrap()
                .ai_kind = ai_kind;
            searched_world
        };
        let found = |ai_kind: UnitAiKind| {
            let mut ai_kind_world = world(ai_kind);
            let mut objects = Vec::new();
            ai_kind_world
                .member_objects(object_id(10), &mut objects)
                .unwrap();
            (
                objects,
                ai_kind_world
                    .parts
                    .unit_ai
                    .get(ai_kind_world.players[1].objects[ListIndex::new(0)].id)
                    .unwrap()
                    .alert_mode,
            )
        };
        assert_eq!(found(UnitAiKind::Transport), (vec![], AlertMode::Calm));
        assert_eq!(
            found(UnitAiKind::Civilian),
            (vec![object_id(20), object_id(22)], AlertMode::Alert)
        );
        assert_eq!(
            found(UnitAiKind::Soldier),
            (vec![object_id(20), object_id(30)], AlertMode::Alert)
        );
    }

    /// `FUN_14100bcb0`'s sub-formations: a statistic's unit the world holds that is no member
    /// (50, weapon range 6) counts for its attack range; a group target the world does not hold
    /// is none (slot 0x238 is the weapon range), and with it the re-attack does nothing. The
    /// searches fill the sighted objects and leave the units kept apart.
    #[test]
    fn sub_formation_range_takes_its_held_units_against_the_group_target() {
        let mut world = searched_world();
        let mut far = Unit {
            id: object_id(50),
            ..Unit::default()
        };
        far.life.master_id = ids::ID_7;
        let _ = world.push_live(PlayerId::new(1), far).unwrap();
        for masters in &mut world.model.masters {
            let archer = Master {
                id: ids::ID_7,
                kind: MasterKind::COMBAT,
                class: class::CLASS_0,
                radius: Footprint::new(0.3, 0.3),
                combat: Some(CombatPart {
                    weapon_range: Distance::new(6.0),
                    ..CombatPart::default()
                }),
                ..Master::default()
            };
            *masters = Arc::new(Masters::new(masters.iter().cloned().chain([archer])));
        }
        let group = world.groups.get_mut(group_id(1)).unwrap();
        group.unit_kinds[0].units.push(object_id(50));
        group.current_target.target = GroupTarget::of_saved(99, -1);
        group.attack_plan_kind = 1;
        group.current_command = GroupCommandKind::ATTACK;
        group.objects = vec![object_id(77)];
        let _ = world
            .statistics_only(group_id(1), FarMembers::Stay)
            .unwrap();
        let group = world.groups.get(group_id(1)).unwrap();
        assert_eq!(group.unit_kinds[0].attack_range, Distance::new(6.0));
        // the searches fill their own list (+0x49c), not the units kept apart (+0x458)
        assert_eq!(
            (&group.sighted[..], &group.objects[..]),
            (&[object_id(20), object_id(30)][..], &[object_id(77)][..])
        );
        assert_eq!(group.attack_plan_kind, 0);
        // a target the world holds: the re-attack is not modelled
        let group = world.groups.get_mut(group_id(1)).unwrap();
        group.current_target.target = GroupTarget::of_saved(20, -1);
        group.attack_plan_kind = 1;
        assert!(
            world
                .statistics_only(group_id(1), FarMembers::Stay)
                .is_err()
        );
    }

    /// `FUN_14100bcb0` with `far`: member 11, 24 from its slot on the ground and first, leaves
    /// the group (its link cleared) and stops, the next member taking its index, and counts in
    /// no statistic; in a formation of type 7, or after a member of radius 0.5 or more (10's,
    /// before it), the distance is 40 and it stays; none leaves without `far`, on a patrol or
    /// with [`GroupFlags::PATROL`].
    #[test]
    fn member_far_from_its_slot_leaves_the_group() {
        let world = |spacing: f32, order: [i32; 2]| {
            let mut searched_world = searched_world();
            for masters in &mut searched_world.model.masters {
                *masters = Arc::new(Masters::new(masters.iter().cloned().map(|mut master| {
                    master.height = Distance::new(1.0);
                    if let Some(creatable_part) = master.creatable.as_mut() {
                        creatable_part.spacing_modifier = SpacingModifier::new(spacing);
                    }
                    master
                })));
            }
            let mut stray =
                searched_world.object_of(&searched_world.players[1].objects[ListIndex::new(0)]);
            stray.unit.id = object_id(11);
            stray.unit.place_for_test(Position::new(6.0, 6.0, 0.0));
            searched_world
                .parts
                .put_mover(stray.unit.id, Some(walker()));
            searched_world
                .parts
                .mover_mut(stray.unit.id)
                .unwrap()
                .moved_at = Position::new(6.0, 6.0, 0.0);
            stray.unit.group.id = Some(group_id(1));
            searched_world.players[1].objects[ListIndex::new(0)]
                .group
                .id = Some(group_id(1));
            let _ = searched_world.push_live(PlayerId::new(1), stray).unwrap();
            let group = searched_world.groups.get_mut(group_id(1)).unwrap();
            group.flags = GroupFlags::NONE;
            group.members = order
                .map(|id| Member {
                    id: object_id(id),
                    ..Member::default()
                })
                .to_vec();
            group.unit_kinds[0].units = object_id_list(order);
            group.current_formation = Some(Formation {
                slots: vec![
                    Slot {
                        unit_id: Some(object_id(10)),
                        transform_position: BVector::new(5.0, 0.0, 5.0),
                        ..Slot::default()
                    },
                    Slot {
                        unit_id: Some(object_id(11)),
                        transform_position: BVector::new(30.0, 0.0, 6.0),
                        ..Slot::default()
                    },
                ],
                ..Formation::default()
            });
            searched_world
        };
        let ids = |world: &World| -> Vec<ObjectId> {
            world
                .groups
                .get(group_id(1))
                .unwrap()
                .members
                .iter()
                .map(|member| member.id)
                .collect()
        };
        let mut game = world(1.0, [11, 10]);
        let _ = game
            .statistics_only(group_id(1), FarMembers::Leave)
            .unwrap();
        assert_eq!(ids(&game), [object_id(10)]);
        let unit = game.object(object_id(11)).unwrap().1;
        assert_eq!(unit.group.id, None);
        assert_eq!(
            game.parts.mover_of(object_id(11)).unwrap().animated.speed,
            Speed::new(0.0)
        );
        let group = game.groups.get(group_id(1)).unwrap();
        assert_eq!(group.centroid, BVector::new(5.0, 0.0, 5.0));
        assert_eq!(group.largest_unit_radius, Distance::new(0.2));
        // a formation of type 7, a wide member before it: 40
        let mut game = world(1.0, [11, 10]);
        game.groups
            .get_mut(group_id(1))
            .unwrap()
            .current_formation
            .as_mut()
            .unwrap()
            .kind = FormationKind::Staggered;
        let _ = game
            .statistics_only(group_id(1), FarMembers::Leave)
            .unwrap();
        assert_eq!(ids(&game), [object_id(11), object_id(10)]);
        let mut game = world(2.5, [10, 11]);
        let _ = game
            .statistics_only(group_id(1), FarMembers::Leave)
            .unwrap();
        assert_eq!(ids(&game), [object_id(10), object_id(11)]);
        let mut game = world(2.5, [11, 10]);
        let _ = game
            .statistics_only(group_id(1), FarMembers::Leave)
            .unwrap();
        assert_eq!(ids(&game), [object_id(10)]);
        // no far test without far, on a patrol, with [`GroupFlags::PATROL`]
        let mut game = world(1.0, [11, 10]);
        let _ = game.statistics_only(group_id(1), FarMembers::Stay).unwrap();
        assert_eq!(ids(&game), [object_id(11), object_id(10)]);
        let mut game = world(1.0, [11, 10]);
        game.groups.get_mut(group_id(1)).unwrap().flags = GroupFlags::PATROL;
        let _ = game
            .statistics_only(group_id(1), FarMembers::Leave)
            .unwrap();
        assert_eq!(ids(&game), [object_id(11), object_id(10)]);
        let mut world = world(1.0, [11, 10]);
        let paths = &mut world.groups.get_mut(group_id(1)).unwrap().motion.paths;
        paths[0].has_path = true;
        paths[0].path_flags = PathFlags::LOOPS;
        let _ = world
            .statistics_only(group_id(1), FarMembers::Leave)
            .unwrap();
        assert_eq!(ids(&world), [object_id(11), object_id(10)]);
    }
}
