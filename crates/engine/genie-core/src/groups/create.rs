//! A group's members (FD-036): `AddUnit` (`FUN_1410083c0`) with its sub-formation statistics
//! (`FUN_141008d90`, `FUN_1410116c0`) and a unit's radius in a formation (`FUN_141025920`).

use crate::GenieError;
use crate::groups::FormationCategory;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::MotionFlags;
use crate::groups::MovePath;
use crate::groups::MoveReason;
use crate::groups::UnitType;
use crate::groups::WorldObject;
use crate::groups::group_list::member_gone;
use crate::groups::{Member, SpecificMove, UnitGroup, UnitTypeStatistic};
use crate::map::Distance;
use crate::math::image::group::{
    DEFAULT_RADIUS as GROUP_DEFAULT_RADIUS, SPACING_FACTOR as GROUP_SPACING_FACTOR,
};
use crate::math::rng::Stream;
use crate::objects::ObjectId;
use crate::objects::masters::FormationClass;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::world::Seconds;
use crate::world::World;
use crate::world::index::Place;
use crate::world::slots::{CheckedIndex, PlayerSlots};
use std::mem::take;

/// A unit's radius in a formation where its master has no full footprint (`FUN_141025920`,
/// `DAT_142e7f384`).
const DEFAULT_RADIUS: f32 = GROUP_DEFAULT_RADIUS;
/// A radius's share of the spacing (`DAT_142e8c150`): two radii side by side.
const SPACING_FACTOR: f32 = GROUP_SPACING_FACTOR;
/// A group holds fewer than this many members (`FUN_1410083c0`'s `0x46`: "No room in this
/// group").
const MAX_MEMBERS: usize = 70;
/// The members' arrays of a group without any first grow to this many (`FUN_1410083c0`'s
/// `FUN_14102d710` with 10), then, as every full array, to twice their capacity.
const FIRST_CAPACITY: i32 = 10;
/// The unit AI's actions and orders `AddUnit` clears (`FUN_1410083c0`): actions 0x259, 0x2bf and
/// 0x2c0, the defend order (and orders 0x323 and 0x324, which no order holds, [`OrderKind`]).
const CLEARED_ACTIONS: [AiAction; 3] =
    [AiAction::Defend, AiAction::MoveAside, AiAction::GroupAside];

/// What a group reads of a unit's master for its formation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FormationUnit {
    pub(crate) master: MasterId,
    pub(crate) kind: MasterKind,
    pub(crate) hero_flag: HeroFlags,
    pub(crate) formation_category: FormationClass,
    /// `FUN_141025920`: the unit's radius in a formation.
    pub(crate) radius: Distance,
}

impl FormationUnit {
    /// The unit's radius in a formation (`FUN_141025920`): a master above type 69 with a footprint
    /// and a height (`FUN_141194060` with 1) has its spacing modifier (`+0x8b8`) times its radius
    /// on x (`+0x5d8`), any other [`DEFAULT_RADIUS`].
    pub(crate) fn of(master: &Master) -> Self {
        let full =
            Distance::ZERO < master.height && 0.0 < master.radius.x() && 0.0 < master.radius.y();
        let radius = if MasterKind::COMBAT <= master.kind && full {
            master.spacing_modifier.get() * master.radius.x()
        } else {
            DEFAULT_RADIUS
        };
        Self {
            master: master.id,
            kind: master.kind,
            hero_flag: master.hero_flag,
            formation_category: master.formation_category,
            radius: Distance::new(radius),
        }
    }
}

impl UnitGroup {
    /// The members' arrays' capacity (`+0xe8`): the members and the spare places past them.
    #[must_use]
    pub const fn member_capacity(&self) -> usize {
        self.members.len() + self.spare_seconds.len()
    }

    /// `FUN_14102d710`: the members' arrays (ids `+0xf0`, own moves `+0xf8`,
    /// `mSecondsSinceAttacking` `+0x480`) grow to `n`. False, with nothing changed, for `n` of 0
    /// or less or not above the capacity (`+0xe8`). Else the members keep their ids, moves and
    /// seconds, and every place past them is fresh: no unit (-1), no move (active 0, path and
    /// reason -1, the rest 0) and its seconds at `f32::MAX` (`0x7f7fffff`), an earlier spare
    /// value gone; the capacity is `n`. Allocation failure is Rust's abort.
    pub fn resize_members(&mut self, n: i32) -> bool {
        if n <= 0 || n as usize <= self.member_capacity() {
            return false;
        }
        self.spare_seconds = vec![Seconds::MAX; n as usize - self.members.len()];
        true
    }

    /// The members' arrays' room for one more (`FUN_1410083c0`): a full array (the members at
    /// the capacity) grows ([`UnitGroup::resize_members`]): one of none to [`FIRST_CAPACITY`]
    /// and then, as any full one, to twice its capacity (20 the first time). The next member
    /// takes the first spare place's seconds, which a removed member may have left
    /// (`FUN_14102c160` shifts the ids and moves, not this array).
    fn next_seconds(&mut self) -> Seconds {
        if self.spare_seconds.is_empty() {
            if self.member_capacity() == 0 {
                let _ = self.resize_members(FIRST_CAPACITY);
            }
            let _ = self.resize_members(self.member_capacity() as i32 * 2);
        }
        self.spare_seconds.remove(0)
    }

    /// `FUN_141008d90` on the statistic of sub-formation `sub` (a new one at the list's end when
    /// none has it, `FUN_1410083c0`): the unit joins its units, the statistic's spacing is the
    /// widest member's two radii, and its unit types are counted again ([`count_unit_types`]).
    fn join_statistic(
        &mut self,
        sub: FormationCategory,
        id: ObjectId,
        unit: FormationUnit,
        masters_of: &dyn Fn(ObjectId) -> Option<MasterId>,
        scratch: &mut Vec<UnitType>,
    ) -> Result<(), GenieError> {
        let at = self
            .unit_types
            .iter()
            .position(|s| s.formation_category == sub)
            .unwrap_or_else(|| {
                self.unit_types.push(UnitTypeStatistic {
                    formation_category: sub,
                    ..UnitTypeStatistic::default()
                });
                self.unit_types.len() - 1
            });
        let stat = self.unit_types.checked_mut(at)?;
        let spacing = unit.radius * SPACING_FACTOR;
        if stat.desired_spacing < spacing {
            stat.desired_spacing = spacing;
        }
        stat.units.push(id);
        count_unit_types(stat, masters_of, scratch);
        Ok(())
    }
}

impl UnitGroup {
    /// [`World::leave_statistic`]'s part on the group, for unit `id` of sub-formation `category`
    /// (None for a unit the world does not hold): it leaves the first statistic of that category
    /// (none for -1), or, for a unit the world does not hold, the first statistic that holds it
    /// (`FUN_14102c320`: its units close up and its unit types are counted again,
    /// [`count_unit_types`]). False when none held it.
    fn drop_from_statistic(
        &mut self,
        id: ObjectId,
        category: Option<FormationCategory>,
        masters_of: &dyn Fn(ObjectId) -> Option<MasterId>,
        scratch: &mut Vec<UnitType>,
    ) -> bool {
        let leave = |stat: &mut UnitTypeStatistic, scratch: &mut Vec<UnitType>| {
            let Some(i) = stat.units.iter().position(|&u| u == id) else {
                return false;
            };
            let _ = stat.units.remove(i);
            count_unit_types(stat, masters_of, scratch);
            true
        };
        match category {
            Some(FormationCategory::ANY) => false,
            Some(c) => self
                .unit_types
                .iter_mut()
                .find(|s| s.formation_category == c)
                .is_some_and(|stat| leave(stat, scratch)),
            None => self.unit_types.iter_mut().any(|stat| leave(stat, scratch)),
        }
    }
}

impl World {
    /// `removeUnit` (`FUN_14102c160`) of unit `id` from group `group`, `target` (`param_3`) the
    /// object its sub-formation is reckoned against (`FUN_141011300`'s rams rule, which no code
    /// enables) and `keep_moving` (`param_4`): false for a unit not a member. A member the world
    /// holds whose group link names the group stands (slot 0x560, [`crate::movement::Mover::
    /// stand`], unless `keep_moving`) and its link clears (`+0x6d0` -1). The members' ids and
    /// own moves close up; `mSecondsSinceAttacking` keeps its places (the last member's value
    /// goes to the spare ones). It leaves its sub-formation's statistic (`FUN_14102c480`); a
    /// lead that leaves gives way to the member nearest the target (`FUN_141016870`,
    /// [`World::nearest_to_target`]); the flags take `0x10002`.
    ///
    /// # Errors
    ///
    /// A member of a master the slice does not have, or a stand the slice does not model.
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn remove_unit(
        &mut self,
        group: GroupId,
        id: ObjectId,
        target: Option<ObjectId>,
        keep_moving: bool,
    ) -> Result<bool, GenieError> {
        let _ = target;
        let Some(g) = self.groups.get(group) else {
            return Ok(false);
        };
        let Some(index) = g.members.iter().position(|m| m.id == id) else {
            return Ok(false);
        };
        let group_id = g.id;
        if let Some(place) = self.place(id) {
            let u = match place {
                Place::Live(p, i) => self.players.live_mut(p, i)?,
                Place::Sleeping(p, i) => self.players.asleep_mut(p, i)?,
            };
            if u.group.id == Some(group_id) {
                if !keep_moving && let Some(m) = self.parts.mover_mut(u.id) {
                    m.stand()?;
                }
                u.group.id = None;
            }
        }
        let g = self.groups.held_mut(group)?;
        let seconds: Vec<Seconds> = g
            .members
            .iter()
            .map(|m| m.seconds_since_attacking)
            .collect();
        let _ = g.members.remove(index);
        for (m, &s) in g.members.iter_mut().zip(&seconds) {
            m.seconds_since_attacking = s;
        }
        g.spare_seconds
            .insert(0, *seconds.checked(seconds.len() - 1)?);
        let _ = self.leave_statistic(group, id)?;
        let g = self.groups.held(group)?;
        if g.motion.lead == Some(id) {
            let lead = self.nearest_to_target(group);
            self.groups.held_mut(group)?.motion.lead = lead;
        }
        self.groups.held_mut(group)?.flags |= GroupFlags::STATISTICS | GroupFlags::RELAYOUT;
        Ok(true)
    }
}

impl World {
    /// `FUN_14102c480` of unit `id` and group `group`'s statistics: for an object the world's
    /// table holds ([`World::world_object`]) its sub-formation ([`World::unit_category`]; -1:
    /// false) and the first statistic of it (none: false), which the unit leaves; for one it
    /// does not hold, the first statistic that held it ([`UnitGroup`]'s
    /// `drop_from_statistic`, `FUN_14102c320`). The routine's target (`param_4`) enters only the
    /// rams' rule of `FUN_141011300`, which never holds: the port takes none.
    ///
    /// # Errors
    ///
    /// A held object of a master the slice does not have.
    ///
    /// When the world does not hold `group`: its caller (`removeUnit`) holds it.
    pub(crate) fn leave_statistic(
        &mut self,
        group: GroupId,
        id: ObjectId,
    ) -> Result<bool, GenieError> {
        let category = match self.world_object(id) {
            Some(object) => Some(self.unit_category(Some(object))?),
            None => None,
        };
        let masters: Vec<(ObjectId, MasterId)> = self
            .players
            .iter()
            .flat_map(|q| q.objects.iter().chain(&q.sleeping))
            .map(|u| (u.id, u.life.master_id))
            .collect();
        let masters_of = |id: ObjectId| masters.iter().find(|(u, _)| *u == id).map(|&(_, m)| m);
        let mut scratch = take(&mut self.groups.unit_types_scratch);
        let left = self.groups.held_mut(group)?.drop_from_statistic(
            id,
            category,
            &masters_of,
            &mut scratch,
        );
        self.groups.unit_types_scratch = scratch;
        Ok(left)
    }
}

/// `FUN_1410116c0`: a sub-formation's unit types in the order its units first have them, each with
/// its count, counted in the process's scratch arrays (`DAT_143f23b70`, `DAT_143f23c40`: the counts
/// are cleared first, the types not), whose 50 words each the statistic takes whole. A unit the
/// world does not hold is passed over; past 51 types the count stops.
pub(crate) fn count_unit_types(
    stat: &mut UnitTypeStatistic,
    masters_of: &dyn Fn(ObjectId) -> Option<MasterId>,
    scratch: &mut Vec<UnitType>,
) {
    scratch.resize(50, UnitType::CLEARED);
    let mut counts = vec![0u32; 50];
    let mut n = 0usize;
    for &id in &stat.units {
        let Some(master) = masters_of(id) else {
            continue;
        };
        let master = UnitType::of(master);
        if let Some(k) = scratch.iter().take(n).position(|&t| t == master) {
            // k < n <= 50, the counts' length
            if let Some(count) = counts.get_mut(k) {
                *count += 1;
            }
            continue;
        }
        if n < 50
            && let (Some(unit_type), Some(count)) = (scratch.get_mut(n), counts.get_mut(n))
        {
            *unit_type = master;
            *count = 1;
        }
        n += 1;
        if 50 < n {
            break;
        }
    }
    stat.unit_types.clone_from(scratch);
    stat.unit_type_counts = counts;
    stat.number_unit_types = n as i32;
}

impl World {
    /// `AddUnit` (`FUN_1410083c0` with `param_4` 0) of unit `id` to group `group`: false for an
    /// object the world's table does not hold ([`World::world_object`]), of no sub-formation
    /// ("Invalid formation category", [`World::unit_category`]) or when the group holds
    /// [`MAX_MEMBERS`] already; true when it is a member already. Else the members' arrays make
    /// room ([`UnitGroup::resize_members`]); a unit in another group the world holds leaves it
    /// ([`World::remove_unit`]); the unit's group link names the group (`+0x6d0`), its unit AI
    /// drops the actions and orders of [`CLEARED_ACTIONS`] and [`CLEARED_ORDERS`] (`FUN_140c64b50`
    /// and `FUN_140c64b60` with -1) and empties its order queue (`FUN_140c5e010`); it joins the
    /// members with no move of its own (active 0, path and reason -1), its sub-formation's
    /// statistic and the group's spacing (its two radii, the widest: `COMISS`/`JBE` at
    /// 0x141008ced); the group takes [`GroupFlags::RELAYOUT`], and [`GroupFlags::STATISTICS`] too
    /// when its motion lacks [`MotionFlags::NEW`]. A sleeping unit joins as a live one.
    ///
    /// The routine's target (`param_3`), which it hands to `removeUnit` and to its second
    /// `FUN_141011300`, enters only the rams' rule, which never holds
    /// ([`World::unit_category`]): the port takes none.
    ///
    /// # Errors
    ///
    /// When the world does not hold `group`: the game's routine gets the group itself, from a
    /// caller that holds it.
    pub fn add_to_group(&mut self, group: GroupId, id: ObjectId) -> Result<bool, GenieError> {
        let Some(object) = self.world_object(id) else {
            return Ok(false);
        };
        let sub = self.unit_category(Some(object))?;
        // a ghost (type 25) has no sub-formation: only a unit gets past this
        let WorldObject::Unit(p, u, _) = object else {
            return Ok(false);
        };
        if sub == FormationCategory::ANY {
            return Ok(false);
        }
        let unit = FormationUnit::of(u.master(self.master_tables(), p)?);
        let old = u.group.id;
        let g = self.groups.held(group)?;
        if g.has(id) {
            return Ok(true);
        }
        if MAX_MEMBERS <= g.members.len() {
            return Ok(false);
        }
        if let Some(old) = old
            && self.groups.get(old).is_some()
        {
            // the unit leaves its old group first (removeUnit with -1, moving stopped)
            let _ = self.remove_unit(old, id, None, false)?;
        }
        let place = self.place(id).ok_or_else(|| member_gone(id))?;
        let ai_rng = self.streams.get(Stream::Ai);
        // a sleeping unit joins as a live one
        let u = match place {
            Place::Live(q, i) => self.players.live_mut(q, i)?,
            Place::Sleeping(q, i) => self.players.asleep_mut(q, i)?,
        };
        u.group.id = Some(group);
        if let Some(ai) = self.parts.unit_ai.get_mut(u.id) {
            if ai
                .current_action
                .is_some_and(|a| CLEARED_ACTIONS.contains(&a))
                || ai.current_order == Some(OrderKind::Defend)
            {
                ai.set_action(None);
                ai.set_order(None, ai_rng);
            }
            ai.orders.clear();
        }
        let masters: Vec<(ObjectId, MasterId)> = self
            .players
            .iter()
            .flat_map(|q| q.objects.iter().chain(&q.sleeping))
            .map(|u| (u.id, u.life.master_id))
            .collect();
        let masters_of = |id: ObjectId| masters.iter().find(|(u, _)| *u == id).map(|&(_, m)| m);
        let mut scratch = take(&mut self.groups.unit_types_scratch);
        let g = self.groups.held_mut(group)?;
        let seconds = g.next_seconds();
        g.members.push(Member {
            id,
            specific_move: SpecificMove {
                path_id: MovePath::NONE,
                reason: MoveReason::NONE,
                ..SpecificMove::default()
            },
            seconds_since_attacking: seconds,
        });
        g.join_statistic(sub, id, unit, &masters_of, &mut scratch)?;
        let spacing = unit.radius * SPACING_FACTOR;
        if g.desired_spacing < spacing {
            g.desired_spacing = spacing;
        }
        g.flags |= GroupFlags::RELAYOUT;
        if !g.motion.flags.contains(MotionFlags::NEW) {
            g.flags |= GroupFlags::STATISTICS | GroupFlags::RELAYOUT;
        }
        self.groups.unit_types_scratch = scratch;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use crate::groups::FormationCategory;
    use crate::groups::FormationKind;
    use crate::groups::GroupFlags;
    use crate::groups::MotionFlags;
    use crate::groups::{Member, UnitGroup, UnitGroups, UnitTypeStatistic};
    use crate::map::Distance;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Position;
    use crate::movement::PathState;
    use crate::objects::ListIndex;
    use crate::objects::ObjectId;
    use crate::objects::masters::FormationClass;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::masters::{MasterId, ids};
    use crate::objects::unit::Unit;
    use crate::players::PlayerId;
    use crate::world::Millis;
    use crate::world::Seconds;
    use crate::world::WorldTime;
    use std::sync::Arc;

    #[cfg(test)]
    use crate::gathering::hunt::tests::walker;
    #[cfg(test)]
    use crate::math::image::group::SPACING_FACTOR as GROUP_SPACING_FACTOR;
    #[cfg(test)]
    use crate::movement::PathData;
    #[cfg(test)]
    use crate::objects::layers::{Motion, Static};
    #[cfg(test)]
    use crate::objects::masters::SpacingModifier;
    #[cfg(test)]
    use crate::players::ByPlayer;
    use crate::players::relation_row;
    use crate::players::relations;
    use crate::test_ids::group_id;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    #[cfg(test)]
    use crate::unit_ai::{SOLDIER_AI, UnitAi};
    #[cfg(test)]
    use crate::vision::registry::Registry;
    #[cfg(test)]
    use crate::vision::visibility::Visibility;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::{Player, World};

    /// Group 1 of player 1's units 10, 11 (master 4, category 1) and 12 (master 6, category 3),
    /// their seconds since attacking 1, 2 and 3, 11 the lead and walking.
    fn three() -> World {
        let unit = |id: i32, master: MasterId, x: f32| {
            let mut u = Unit {
                id: object_id(id),
                place: Static::at(Position::new(x, 0.0, 0.0)),
                ..Unit::default()
            };
            u.life.master_id = master;
            u.group.id = Some(group_id(1));
            u
        };
        let walking = unit(11, master_id(4), 2.0);
        let mut mover = walker();
        mover.paths = vec![
            mover
                .paths
                .first()
                .cloned()
                .unwrap_or(PathData::without_path()),
        ];
        mover.paths[0].state = PathState::Walking;
        let player = Player {
            objects: vec![
                unit(10, master_id(4), 1.0),
                walking,
                unit(12, ids::ID_6, 3.0),
            ]
            .into(),
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(0),
            PlayerId::new(0),
            vec![Millis::ZERO, Millis::ZERO],
            vec![Player::default(), player],
        )
        .with_motions([(object_id(11), Motion::Moving(mover))]);
        let master = |id: MasterId, category: u8| Master {
            id,
            kind: MasterKind::COMBAT,
            formation_category: FormationClass::of_data(category),
            ..Master::default()
        };
        let masters = Masters::new([master(master_id(4), 1), master(ids::ID_6, 3)]);
        t.model.masters = vec![Arc::new(masters.clone()), Arc::new(masters)];
        let mut g = UnitGroup::new(group_id(1));
        g.members = [(10, 1.0), (11, 2.0), (12, 3.0)]
            .iter()
            .map(|&(id, s)| Member {
                id: object_id(id),
                seconds_since_attacking: Seconds::new(s),
                ..Member::default()
            })
            .collect();
        g.spare_seconds = vec![Seconds::MAX];
        g.unit_types = vec![
            UnitTypeStatistic {
                formation_category: FormationCategory::named(1),
                units: vec![object_id(10), object_id(11)],
                ..UnitTypeStatistic::default()
            },
            UnitTypeStatistic {
                formation_category: FormationCategory::named(3),
                units: vec![object_id(12)],
                ..UnitTypeStatistic::default()
            },
        ];
        g.motion.lead = Some(object_id(11));
        t.groups = UnitGroups {
            groups: vec![g],
            ..UnitGroups::default()
        };
        t
    }

    /// `FUN_14102d710`: nothing for 0, a negative size or one not above the capacity; a larger
    /// one keeps the members and makes every place past them fresh, an old spare value gone.
    /// `AddUnit` grows a group without places to 10, then 20.
    #[test]
    fn the_members_arrays_grow_with_fresh_places_past_the_members() {
        let mut g = UnitGroup::new(group_id(1));
        g.members = vec![Member {
            id: object_id(5),
            seconds_since_attacking: Seconds::new(2.0),
            ..Member::default()
        }];
        g.spare_seconds = vec![Seconds::new(7.0), Seconds::new(8.0)];
        assert!(!g.resize_members(0));
        assert!(!g.resize_members(-4));
        assert!(!g.resize_members(3));
        assert_eq!(g.spare_seconds, [Seconds::new(7.0), Seconds::new(8.0)]);
        assert!(g.resize_members(4));
        assert_eq!(
            (g.members[0].id, g.members[0].seconds_since_attacking),
            (object_id(5), Seconds::new(2.0))
        );
        assert_eq!(g.spare_seconds, [Seconds::MAX; 3]);
        assert_eq!(g.member_capacity(), 4);
        // the place taken for the next member leaves 19 spare of 20
        let mut empty = UnitGroup::new(group_id(2));
        assert_eq!(empty.next_seconds(), Seconds::MAX);
        assert_eq!(empty.spare_seconds.len(), 19);
    }

    /// `FUN_1410083c0` adds a sleeping unit as a live one: its link names the group, it joins
    /// the members and its sub-formation's statistic.
    #[test]
    fn sleeping_unit_joins_group_as_live_one() {
        let mut t = three();
        let mut asleep = Unit {
            id: object_id(13),
            ..Unit::default()
        };
        asleep.life.master_id = master_id(4);
        let _ = t.push_asleep(PlayerId::new(1), asleep).unwrap();
        assert!(t.add_to_group(group_id(1), object_id(13)).unwrap());
        assert_eq!(
            t.players[1].sleeping[ListIndex::new(0)].group.id,
            Some(group_id(1))
        );
        let g = t.groups.get(group_id(1)).unwrap();
        assert_eq!(g.members.last().map(|m| m.id), Some(object_id(13)));
        assert_eq!(
            g.unit_types[0].units,
            [object_id(10), object_id(11), object_id(13)]
        );
        // an object the world's table does not hold, or of no sub-formation, joins nothing
        assert!(!t.add_to_group(group_id(1), object_id(99)).unwrap());
    }

    /// `FUN_14102c480`: a held unit of no sub-formation (type below 70) or of one no statistic
    /// has leaves nothing (false); a held unit leaves its sub-formation's first statistic.
    #[test]
    fn unit_leaves_only_its_own_sub_formation_statistic() {
        let mut t = three();
        let unit = |id: i32, master: MasterId| {
            let mut u = Unit {
                id: object_id(id),
                ..Unit::default()
            };
            u.life.master_id = master;
            u
        };
        let _ = t.push_live(PlayerId::new(1), unit(14, ids::ID_7)).unwrap();
        let _ = t
            .push_live(PlayerId::new(1), unit(15, master_id(8)))
            .unwrap();
        for masters in &mut t.model.masters {
            let extra = [
                Master {
                    id: ids::ID_7,
                    kind: MasterKind::MISSILE,
                    formation_category: FormationClass::of_data(1),
                    ..Master::default()
                },
                Master {
                    id: master_id(8),
                    kind: MasterKind::COMBAT,
                    formation_category: FormationClass::of_data(2),
                    ..Master::default()
                },
            ];
            *masters = Arc::new(Masters::new(masters.iter().cloned().chain(extra)));
        }
        t.groups.groups[0].unit_types[0].units.push(object_id(14));
        assert!(!t.leave_statistic(group_id(1), object_id(14)).unwrap());
        assert!(!t.leave_statistic(group_id(1), object_id(15)).unwrap());
        assert_eq!(
            t.groups.get(group_id(1)).unwrap().unit_types[0].units,
            [object_id(10), object_id(11), object_id(14)]
        );
        assert!(t.leave_statistic(group_id(1), object_id(12)).unwrap());
        assert_eq!(
            t.groups.get(group_id(1)).unwrap().unit_types[1].units,
            Vec::new()
        );
    }

    /// `FUN_14102c160`: 11 leaves: the ids close up, the seconds keep their places (11's 2 now
    /// 12's, 12's 3 the first spare); its statistic counts 10 alone; it stands and its link
    /// clears; the lead becomes the member nearest the target (the origin: 10); flags `0x10002`.
    #[test]
    fn removed_member_leaves_its_slot_statistic_and_lead() {
        let mut t = three();
        assert!(
            t.remove_unit(group_id(1), object_id(11), None, false)
                .unwrap()
        );
        let g = t.groups.get(group_id(1)).unwrap();
        let members: Vec<(ObjectId, Seconds)> = g
            .members
            .iter()
            .map(|m| (m.id, m.seconds_since_attacking))
            .collect();
        assert_eq!(
            members,
            [
                (object_id(10), Seconds::new(1.0)),
                (object_id(12), Seconds::new(2.0))
            ]
        );
        assert_eq!(g.spare_seconds, [Seconds::new(3.0), Seconds::MAX]);
        assert_eq!(g.unit_types[0].units, [object_id(10)]);
        assert_eq!(
            (
                g.unit_types[0].number_unit_types,
                g.unit_types[0].unit_type_counts[0]
            ),
            (1, 1)
        );
        assert_eq!(g.unit_types[1].units, [object_id(12)]);
        assert_eq!(
            (
                g.motion.lead,
                g.flags
                    .contains(GroupFlags::RELAYOUT | GroupFlags::STATISTICS)
            ),
            (Some(object_id(10)), true)
        );
        let u = &t.players[1].objects[ListIndex::new(1)];
        assert_eq!(u.group.id, None);
        let m = t.parts.mover_of(u.id).unwrap();
        assert_eq!(m.paths[0].state, PathState::Idle);
        // not a member, or a group the world does not hold
        assert!(
            !t.remove_unit(group_id(1), object_id(11), None, false)
                .unwrap()
        );
        assert!(
            !t.remove_unit(group_id(2), object_id(10), None, false)
                .unwrap()
        );
    }

    /// `FUN_14100bcb0`'s closing `CalcFormationType` is gated on [`MotionFlags::NEW`]
    /// (`+0xb8`, `SHR EAX, 1; TEST AL, 1` at 0x14100c657), not the group flags'.
    #[test]
    fn the_statistics_call_for_the_formation_type_by_the_motion_flags() {
        let mut t = three();
        for u in &mut t.players[1].objects {
            t.parts.set_unit_ai(
                u.id,
                Some(UnitAi {
                    ai_type: SOLDIER_AI,
                    ..UnitAi::default()
                }),
            );
        }
        // the members' searches find nothing: an empty registry, no relations
        let mut v = Visibility::new(GridSize::new(20, 20), vec![], vec![], vec![], 0, vec![]);
        v.registry = Some(Registry::new(2, 65));
        t.spatial.sight = Some(v);
        t.spatial.relations = ByPlayer::new(vec![relation_row([0, 0]), relation_row([0, 0])]);
        t.spatial.unit_diplomacy = ByPlayer::new(vec![relations([1, 2]), relations([0, 1])]);
        let g = t.groups.get_mut(group_id(1)).unwrap();
        g.flags |= GroupFlags::RELAYOUT;
        g.motion.flags &= !MotionFlags::NEW;
        assert!(!t.statistics_only(group_id(1), false).unwrap());
        let g = t.groups.get_mut(group_id(1)).unwrap();
        g.flags &= !GroupFlags::RELAYOUT;
        g.motion.flags |= MotionFlags::NEW;
        assert!(t.statistics_only(group_id(1), false).unwrap());
    }

    /// `FUN_14100d140` writes the formation type of every member `FUN_1415417a0` finds with a
    /// unit AI, a sleeping one too.
    #[test]
    fn the_stances_reach_sleeping_member() {
        let mut t = three();
        for u in &mut t.players[1].objects {
            t.parts.set_unit_ai(u.id, Some(UnitAi::default()));
        }
        let asleep = t.remove_live(PlayerId::new(1), ListIndex::new(2)).unwrap();
        let _ = t.push_asleep(PlayerId::new(1), asleep).unwrap();
        t.stances_state(group_id(1), FormationKind::USER_DEFINED)
            .unwrap();
        assert_eq!(
            t.parts
                .unit_ai
                .get(t.players[1].sleeping[ListIndex::new(0)].id)
                .unwrap()
                .formation_type,
            FormationKind::USER_DEFINED
        );
        assert_eq!(
            t.parts
                .unit_ai
                .get(t.players[1].objects[ListIndex::new(0)].id)
                .unwrap()
                .formation_type,
            FormationKind::USER_DEFINED
        );
    }

    /// `FUN_14100d140`'s stance: the members' most common, the lower on equal counts; a group
    /// none of whose members holds a unit AI stays aggressive.
    #[test]
    fn the_group_takes_its_members_most_common_stance() {
        use crate::unit_ai::UnitStance;
        let with = |stances: [UnitStance; 3]| {
            let mut t = three();
            for (u, stance) in t.players[1].objects.iter_mut().zip(stances) {
                t.parts.set_unit_ai(
                    u.id,
                    Some(UnitAi {
                        stance,
                        ..UnitAi::default()
                    }),
                );
            }
            t.stances_state(group_id(1), FormationKind::USER_DEFINED)
                .unwrap();
            t.groups.get(group_id(1)).unwrap().stance
        };
        let (a, d, s) = (
            UnitStance::AGGRESSIVE,
            UnitStance::DEFENSIVE,
            UnitStance::STAND_GROUND,
        );
        assert_eq!(with([d, s, d]), Some(d));
        assert_eq!(with([s, s, a]), Some(s));
        assert_eq!(with([a, d, s]), Some(a));
        assert_eq!(
            with([UnitStance::NO_ATTACK; 3]),
            Some(UnitStance::NO_ATTACK)
        );
        let mut t = three();
        t.stances_state(group_id(1), FormationKind::USER_DEFINED)
            .unwrap();
        assert_eq!(t.groups.get(group_id(1)).unwrap().stance, Some(a));
    }

    /// `LockFormation` (`FUN_141024e70`) re-reads the spacing (`+0x15c`) for every member
    /// (`MOVSS`/`UCOMISS` at 0x14102500a): the first held member with a radius sets it, the later
    /// ones are skipped.
    #[test]
    fn lock_formation_takes_the_first_member_spacing() {
        let mut t = three();
        for masters in &mut t.model.masters {
            for (id, r) in [(4, 0.2f32), (6, 0.5)] {
                let m = Arc::make_mut(masters).get_mut(master_id(id)).unwrap();
                m.radius = Footprint::new(r, r);
                m.height = Distance::new(1.0);
                m.spacing_modifier = SpacingModifier::new(1.0);
            }
        }
        t.groups.get_mut(group_id(1)).unwrap().desired_spacing = Distance::ZERO;
        let _ = t.lock_formation(group_id(1), FormationKind::LINE);
        let spacing = t.groups.get(group_id(1)).unwrap().desired_spacing;
        assert_eq!(spacing, Distance::new(0.2 * GROUP_SPACING_FACTOR));
    }

    /// Kept moving, the unit's walk stays; a unit the world does not hold leaves the first
    /// statistic that holds it.
    #[test]
    fn unit_kept_moving_walks_on_and_gone_one_leaves_its_statistic() {
        let mut t = three();
        assert!(
            t.remove_unit(group_id(1), object_id(11), None, true)
                .unwrap()
        );
        assert_eq!(
            t.parts
                .mover_of(t.players[1].objects[ListIndex::new(1)].id)
                .unwrap()
                .paths[0]
                .state,
            PathState::Walking
        );
        let mut t = three();
        let _ = t.remove_live(PlayerId::new(1), ListIndex::new(2)).unwrap();
        assert!(
            t.remove_unit(group_id(1), object_id(12), None, false)
                .unwrap()
        );
        let g = t.groups.get(group_id(1)).unwrap();
        assert_eq!(
            (g.unit_types[0].units.len(), g.unit_types[1].units.len()),
            (2, 0)
        );
    }
}
