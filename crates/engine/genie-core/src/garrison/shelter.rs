//! The garrison order and `SEEK_SHELTER`: each unit to a building that takes it in, nearest
//! first ([`Shelters`]).

use crate::GenieError;
use crate::garrison::ClassFlags;
use crate::garrison::ContainerId;
use crate::garrison::GarrisonScope;
use crate::garrison::InsideCount;
use crate::garrison::ShelterReach;
use crate::garrison::{
    COMPUTER_IDS, FAR, HUMAN_IDS, NEARBY, TOWN_BELL_REACH, class_has_flags, has_garrison_task,
};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::math::fmath::below_or_unordered;
use crate::math::image::garrison::LEAST_HP_SHARE as GARRISON_LEAST_HP_SHARE;
use crate::math::msvc::{sort, unordered_map_order};
use crate::movement::footprint_gap_squared;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::GarrisonCapacity;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::MasterId;
use crate::objects::masters::class;
use crate::objects::masters::ids::MASTER_1808;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::unit::Unit;
use crate::orders::Dispatch;
use crate::orders::object_gone;
use crate::orders::{GARRISON, Work};
use crate::players::PlayerId;
use crate::players::tracker::ListKey;
use crate::unit_ai::OrderKind;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::{Issuer, OrderPriority, OrderTargetType, QueuedOrder};
use crate::world::World;
use crate::world::index::Place;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;

/// A shelter index that the count list of the callers does not cover.
const NO_COUNT: GenieError = GenieError::unreachable("a shelter without a count");

/// A building the first take names that the shelter list lacks.
const NO_LISTED_BUILDING: GenieError =
    GenieError::unreachable("a taken building outside the shelter list");

/// A unit the shelter queues an order for, whose AI was tested above.
const NO_UNIT_AI: GenieError = GenieError::unreachable("a sheltered unit without a unit AI");

/// The shelters `FUN_141621220` sends units to: the buildings (its `param_3`), the transports
/// (`param_4`), and the count list (`param_5`) its pairs index by their shelter's index, a
/// transport's after the buildings. Its callers push the counts in their own order (the
/// buildings' scan near a target pushes one for each object in reach, gates and annexes too),
/// so a pair's count need not be its shelter's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Shelters {
    /// The buildings.
    pub buildings: Vec<ContainerId>,
    /// The transports.
    pub transports: Vec<ContainerId>,
    /// The counts ([`World::garrisoned`], slot 0x498).
    pub counts: Vec<InsideCount>,
}
/// A building takes units in only above this share of its master's hit points (`FUN_14120eb10`,
/// in doubles).
pub(super) const LEAST_HP_SHARE: f64 = GARRISON_LEAST_HP_SHARE;

impl World {
    /// `UNIT_ORDER`'s garrison (action 5) on `target`, which the world holds (`FUN_140e7f200`; the
    /// UI's sound for it, `FUN_1409648c0`, is not held): `FUN_14161e370` lists the order's units
    /// (the ids the world holds of a type 70 or more), then the shelters ([`Shelters`]). With
    /// [`GarrisonScope::Target`] (the packet's `+0x1e`) the target alone: one of a type below 80 a transport,
    /// else a building (an annex its main building, but a gate's), with its count
    /// ([`World::garrisoned`]). With [`GarrisonScope::Nearby`], each object of the target's owner's tracker under the
    /// master the target's line stands for (`FUN_140fdd6c0`, `FUN_140fdd6f0`, [`UnitLines::base`];
    /// `FUN_14104f960`, [`crate::players::tracker::Tracker::list`]), in the tracker's order, within
    /// [`NEARBY`] of the target (`FUN_14118f8c0`, the squared footprint gap; `COMISS`/`JC`: a NaN
    /// is out): one of a type below 80 is a transport, one not a gate (class 39) and no annex
    /// (`+0xbb8`) a building, and each one in reach pushes its count, the gates and annexes
    /// left out too. Then [`World::shelter`] (`param_6` 0, [`ShelterReach::Far`]) by `dispatch`
    /// (the packet's `+0x1d`: through the unit AIs' queues with [`Dispatch::Queued`]).
    ///
    /// [`UnitLines::base`]: crate::objects::lines::UnitLines::base
    pub(crate) fn garrison_order(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
        target: Option<ObjectId>,
        dispatch: Dispatch,
        scope: GarrisonScope,
    ) -> Result<(), GenieError> {
        let Some((tp, t)) = self.object(target) else {
            return Ok(());
        };
        let target = t.id;
        let (target_master, target_annex) =
            (t.life.master_id, self.parts.building_of(t.id).annex_of);
        let mut units = Vec::with_capacity(ids.len());
        for &id in ids.iter().flatten() {
            let Some((p, u)) = self.object(id) else {
                continue;
            };
            if COMBAT <= u.master(self.master_tables(), p)?.kind {
                units.push(id);
            }
        }
        let shelters = if scope == GarrisonScope::Nearby {
            self.shelters_near(tp, target, target_master)?
        } else {
            let tm = self.master(tp, target_master)?;
            if tm.kind < BUILDING {
                Shelters {
                    buildings: Vec::new(),
                    transports: vec![ContainerId::new(target)],
                    counts: vec![self.garrisoned(target)?],
                }
            } else {
                let building = match target_annex {
                    // The class of buildings, which a unit's garrison task names (`FUN_1415c9240` with 3, 3).
                    // A gate's class: its annex is not its main building for a garrison order.
                    Some(main) if tm.class != class::GATE => main.id(),
                    _ => target,
                };
                Shelters {
                    buildings: vec![ContainerId::new(building)],
                    transports: Vec::new(),
                    counts: vec![self.garrisoned(building)?],
                }
            }
        };
        self.shelter(player, &units, &shelters, ShelterReach::Far, dispatch)
    }

    /// `SEEK_SHELTER` (0x2c, `FUN_140e6a3a0`), after the command's check (`FUN_140e558b0`): of
    /// the first 60 `ids` (500 for a computer player), an id the world does not hold, and another
    /// player's unit unless players may order each other's ([`crate::world::settings::Settings::cheats_enabled`],
    /// `+0x1074`), are dropped ([`World::checked_units`]); with every id dropped the command does
    /// nothing. Of the others, the villagers (class 4 of type 70) and the units of master 1808
    /// seek shelter (a villager of master 1808 is listed twice) in the player's buildings that
    /// take villagers: in the player's object order, those of type 80 in state 2, not inside
    /// another (`+0x2f0`, `FUN_141188da0`), whose master's `garrison_type` has
    /// [`crate::objects::masters::GarrisonKinds::VILLAGERS`]
    /// (`FUN_1411ccb60` with 1), each with its count ([`World::garrisoned`]); then
    /// [`World::shelter`] (`FUN_141622450`).
    pub(crate) fn seek_shelter(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
    ) -> Result<(), GenieError> {
        let cap = if self.players.player(player)?.kind.is_computer() {
            COMPUTER_IDS
        } else {
            HUMAN_IDS
        };
        let checked = self.checked_units(player, ids.get(..cap).unwrap_or(ids));
        // a human's command whose ids all dropped is refused; run (the cheat +0xb12) on no
        // units it would only read the buildings' counts
        if checked.iter().all(Option::is_none) {
            return Ok(());
        }
        let mut units = Vec::with_capacity(checked.len());
        for id in checked.into_iter().flatten() {
            let Some((p, u)) = self.object(id) else {
                continue;
            };
            let m = u.master(self.master_tables(), p)?;
            if m.class == class::VILLAGER && m.kind == COMBAT {
                units.push(id);
            }
            if m.id == MASTER_1808 {
                units.push(id);
            }
        }
        let mut buildings = Vec::new();
        for b in &self.players.player(player)?.objects {
            let m = b.master(self.master_tables(), player)?;
            let takes_villagers = m
                .building
                .is_some_and(|x| x.garrison_type.contains(GarrisonKinds::VILLAGERS));
            if b.life.state == ObjectState::Alive
                && b.life.inside.is_none()
                && m.kind == BUILDING
                && takes_villagers
            {
                buildings.push(ContainerId::new(b.id));
            }
        }
        let counts = buildings
            .iter()
            .map(|&b| self.garrisoned(b.id()))
            .collect::<Result<Vec<_>, GenieError>>()?;
        let shelters = Shelters {
            buildings,
            transports: Vec::new(),
            counts,
        };
        // FUN_141622450: no transports, no gap limit, not through the unit AIs
        self.shelter(player, &units, &shelters, ShelterReach::Far, Dispatch::Now)
    }

    /// `FUN_14161e370`'s shelters near `target` (of player `tp` and master `master`, its
    /// `param_9`): the objects of `tp`'s tracker under the master `master`'s line stands for, in
    /// the tracker's order ([`World::garrison_order`]).
    pub(crate) fn shelters_near(
        &self,
        tp: PlayerId,
        target: ObjectId,
        master: MasterId,
    ) -> Result<Shelters, GenieError> {
        let mut shelters = Shelters::default();
        let key = ListKey::of_master(self.lines.base(master));
        let (at, radius) = self.ground(target)?;
        for &id in self.players.player(tp)?.tracker.list(key) {
            // the tracker's keepers follow every object its player gains and loses, so the
            // world holds each (the game reads a null one's place)
            let Some((op, o)) = self.object(id) else {
                continue;
            };
            let id = o.id;
            let (other_at, other_radius) = self.ground(id)?;
            // COMISS NEARBY, gap; JC: a gap above it, or a NaN, is out
            let gap = footprint_gap_squared(other_at, other_radius, at, radius);
            if below_or_unordered(NEARBY, gap.get()) {
                continue;
            }
            let om = o.master(self.master_tables(), op)?;
            if om.kind < BUILDING {
                shelters.transports.push(ContainerId::new(id));
            } else if om.class != class::GATE && self.parts.building_of(o.id).annex_of.is_none() {
                shelters.buildings.push(ContainerId::new(id));
            }
            shelters.counts.push(self.garrisoned(id)?);
        }
        Ok(shelters)
    }

    /// The place and master radii of the object `id` (what `FUN_14118f8c0` reads of it).
    pub(crate) fn ground(&self, id: ObjectId) -> Result<(Position, Footprint), GenieError> {
        self.spatial
            .get_known(id)?
            .map(|p| (p.position, p.radius))
            .ok_or(GenieError::unsupported(
                "a garrison's object without a place",
            ))
    }

    /// `FUN_141621220`: each of `units` pairs with each building of `shelters` (type 80) whose
    /// master has a capacity (`obj_max`) and takes the unit's master's copy in
    /// ([`World::takes_in`]), and with each transport of a unit whose master's class has a flag
    /// of 0x10002 ([`class_has_flags`]) when the commanding player's master of the unit's master
    /// id has a task of action 3 on the transport's class ([`has_garrison_task`]), within the reach
    /// (`reach`, `param_6`: [`TOWN_BELL_REACH`] or [`FAR`]; `COMISS`/`JBE`: a NaN is out); a
    /// pair names its shelter by its index, a transport's after the buildings. The pairs sorted
    /// by their squared footprint gap (`FUN_1415f0830`, MSVC's `std::sort`) give each unit, in
    /// that order, the first shelter whose count is below its master's capacity, then any unit
    /// left its nearest shelter. The count is the one of the count list at the pair's index
    /// (the list holds what its caller pushed, in its own order: an index into it need not be
    /// its shelter's), which each taker raises. The shelters go in a hash map by id
    /// (`FUN_1415f0c40`), whose order ([`crate::math::msvc::unordered_map_order`]) the next part
    /// follows ([`World::shelter_in`] by `dispatch`). Not modelled: the groups the splitter forms.
    pub(crate) fn shelter(
        &mut self,
        player: PlayerId,
        units: &[ObjectId],
        shelters: &Shelters,
        reach: ShelterReach,
        dispatch: Dispatch,
    ) -> Result<(), GenieError> {
        let reach = match reach {
            ShelterReach::TownBell => TOWN_BELL_REACH,
            ShelterReach::Far => FAR,
        };
        let all: Vec<ContainerId> = shelters
            .buildings
            .iter()
            .chain(&shelters.transports)
            .copied()
            .collect();
        // the pairs of a unit and a shelter: (unit index, shelter index, squared gap)
        let mut pairs = Vec::new();
        let mut capacities = Vec::with_capacity(all.len());
        for &building in &all {
            let (bp, b) = self
                .object(building)
                .ok_or(GenieError::unsupported("a garrison's building is gone"))?;
            let bm = b.master(self.master_tables(), bp)?;
            capacities.push((bm.kind == BUILDING, bm.garrison, bm.class));
        }
        let first_transport = shelters.buildings.len();
        for (i, &id) in units.iter().enumerate() {
            let (p, u) = self
                .object(id)
                .ok_or(GenieError::unsupported("a garrison's unit is gone"))?;
            let own = u.master(self.master_tables(), p)?;
            let (copy, master, class) = (own.copy_id, own.id, own.class);
            // COMISS reach, gap; JBE: a gap at the reach or past it, or a NaN, pairs nothing
            let gap_to = |shelter: ObjectId| -> Result<f32, GenieError> {
                let (at, r) = self.ground(id)?;
                let (sat, sr) = self.ground(shelter)?;
                Ok(footprint_gap_squared(at, r, sat, sr).get())
            };
            for (j, (&building, &(is_building, capacity, _))) in
                shelters.buildings.iter().zip(&capacities).enumerate()
            {
                if is_building && capacity.holds() && self.takes_in(building, player, copy)? {
                    let gap = gap_to(building.id())?;
                    if gap < reach {
                        pairs.push((i, j, gap));
                    }
                }
            }
            if !class_has_flags(class, ClassFlags::TRANSPORTED) {
                continue;
            }
            for (k, &transport) in shelters.transports.iter().enumerate() {
                // FUN_1415c9240 reads the commanding player's master of the unit's master id
                // (its slot unchecked: one the player lacks would crash the game)
                let (_, _, transport_class) = *capacities.checked(first_transport + k)?;
                let list = self.master(player, master)?;
                if has_garrison_task(&list.tasks, GARRISON, transport_class) {
                    let gap = gap_to(transport.id())?;
                    if gap < reach {
                        pairs.push((i, first_transport + k, gap));
                    }
                }
            }
        }
        sort(
            &mut pairs,
            &|a: (usize, usize, f32), b: (usize, usize, f32)| a.2 < b.2,
        )?;
        let (going, first_taken) =
            assign_shelters(&pairs, units, (&all, &capacities), shelters.counts.clone())?;
        for building in unordered_map_order(first_taken) {
            let j = all
                .iter()
                .position(|&b| b == building)
                .ok_or(NO_LISTED_BUILDING)?;
            self.shelter_in(building, going.checked(j)?, dispatch)?;
        }
        Ok(())
    }

    /// `FUN_140c40e00`: a unit AI's test of the building `b` of player `bp` for its unit of
    /// player `player` and master `master`: none for an object below type 70, else whether it
    /// takes the unit in ([`World::takes_in`], with 1 and 1).
    pub(super) fn ai_takes(
        &self,
        bp: PlayerId,
        b: &Unit,
        player: PlayerId,
        master: MasterId,
    ) -> Result<bool, GenieError> {
        if b.master(self.master_tables(), bp)?.kind < COMBAT {
            return Ok(false);
        }
        self.takes_in(ContainerId::new(b.id), player, Some(master))
    }

    /// One shelter's part of [`World::shelter`]: the order splitter (`FUN_140fd9dc0` with
    /// command `0x2713`) on its units, which keeps those whose unit AI's test passes
    /// ([`World::ai_takes`]); then each loose unit (not in a group, `+0x6d0`; in state 2 or 6 or
    /// less, of a type 70 or more) takes slot 0x7e8 on the shelter at its place with task
    /// filter 3, or, with [`Dispatch::Queued`] (`param_7`) and a unit AI, the unit AI's slot 0x30 (into
    /// slot 0x28, `FUN_140c55c90`: not immediate, behind) queues the enter order (`0x2cd`) on
    /// the shelter at its place: issued by the unit's own id, of the shelter's owner and master,
    /// range 0, priority 100, the point shown, ready.
    pub(super) fn shelter_in(
        &mut self,
        building: ContainerId,
        going: &[ObjectId],
        dispatch: Dispatch,
    ) -> Result<(), GenieError> {
        let (bp, b) = self
            .object(building)
            .ok_or(GenieError::unsupported("a garrison's building is gone"))?;
        // the splitter tests a unit with a unit AI only
        let keeps = |id: ObjectId| -> Result<bool, GenieError> {
            let (p, u) = self.object(id).ok_or_else(|| object_gone(id))?;
            if !self.parts.unit_ai.contains(u.id) {
                return Ok(true);
            }
            self.ai_takes(bp, b, p, u.life.master_id)
        };
        let (point, owner, master) = (b.position(), bp, b.life.master_id);
        let mut kept = Vec::with_capacity(going.len());
        for &id in going {
            if self.object(id).is_some() && keeps(id)? {
                kept.push(id);
            }
        }
        let going: Vec<Option<ObjectId>> = going.iter().copied().map(Some).collect();
        let loose = self.loose_units_kept(&going, &|id| kept.contains(&id))?;
        for id in loose {
            let Some(Place::Live(p, idx)) = self.place(id) else {
                return Err(GenieError::unsupported(
                    "a garrison of a unit the slice does not hold live",
                ));
            };
            let unit = self.players.live(p, idx)?;
            let kind = unit.master(self.master_tables(), p)?.kind;
            if (ObjectState::Alive < unit.life.state && unit.life.state != ObjectState::Undead)
                || kind < COMBAT
            {
                continue;
            }
            if dispatch == Dispatch::Queued && self.parts.unit_ai.contains(unit.id) {
                let state = unit.life.state;
                let order = QueuedOrder {
                    issuer: Some(Issuer::Object(id)),
                    order: OrderKind::Enter,
                    priority: OrderPriority::COMMAND,
                    target: Some(building.id()),
                    target_owner: Some(owner),
                    point,
                    range: Distance::ZERO,
                    shown: point.ground(),
                    ready: true,
                    target_type: Some(OrderTargetType::of_master(master)),
                };
                let ai = self
                    .parts
                    .unit_ai
                    .get_mut(self.players.live(p, idx)?.id)
                    .ok_or(NO_UNIT_AI)?;
                let _ = ai.queue_order(state, order, Dispatch::Queued, QueueEnd::Back)?;
                continue;
            }
            let work = Work {
                target: Some(building.id()),
                point,
                carry_test: false,
                filter: Some(GARRISON),
                notified: true,
            };
            self.order_work(p, idx, &work)?;
        }
        Ok(())
    }
}

/// [`World::shelter`]'s assignment: each unit, in the order of the sorted `pairs`, takes the
/// first shelter whose count is below its capacity, then any unit left its nearest shelter.
/// Each shelter's units by shelter index, and the order of the shelters' first take.
fn assign_shelters(
    pairs: &[(usize, usize, f32)],
    units: &[ObjectId],
    (all, capacities): (&[ContainerId], &[(bool, GarrisonCapacity, ClassId)]),
    mut counts: Vec<InsideCount>,
) -> Result<(Vec<Vec<ObjectId>>, Vec<ContainerId>), GenieError> {
    let mut taken = vec![false; units.len()];
    // each shelter's units, by shelter index, and the order of the shelters' first take
    let mut going: Vec<Vec<ObjectId>> = vec![Vec::new(); all.len()];
    let mut first_taken = Vec::new();
    for &(i, j, _) in pairs {
        // the count list is at least as long as the shelters: its callers push a count for
        // each shelter, and FUN_14161e370 one for each gate and annex beside
        let count = counts.get_mut(j).ok_or(NO_COUNT)?;
        if !*taken.checked(i)? && count.below(capacities.checked(j)?.1) {
            going.checked_mut(j)?.push(*units.checked(i)?);
            first_taken.push(*all.checked(j)?);
            *count = count.plus(InsideCount::ONE);
            *taken.checked_mut(i)? = true;
        }
    }
    for &(i, j, _) in pairs {
        if !*taken.checked(i)? {
            going.checked_mut(j)?.push(*units.checked(i)?);
            first_taken.push(*all.checked(j)?);
            *taken.checked_mut(i)? = true;
        }
    }
    Ok((going, first_taken))
}
