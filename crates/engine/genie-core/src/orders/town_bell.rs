//! A villager's town-bell record (`+0x9ac`..`+0x9dc`): what it worked on when an order to a
//! target took it off its work (`FUN_141206a20`), which a later back-to-work order sends it back
//! to, and the flag a garrison order sets (`FUN_1412067f0`). Saved as `mTownBellFlag`,
//! `mBackToWork1` and `mBackToWork2`.

use crate::GenieError;
use crate::commands::Bell;
use crate::garrison::ContainerId;
use crate::garrison::ShelterReach;
use crate::garrison::Shelters;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::HUNT;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::below_or_unordered;
use crate::math::image::garrison::NO_POINT as GARRISON_NO_POINT;
use crate::movement::footprint_gap_squared;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::MasterKind;
use crate::orders::Dispatch;
use crate::orders::Work;
use crate::orders::{GARRISON, own_order_target};
use crate::players::AllyTest;
use crate::players::PlayerId;
use crate::players::tracker::ListKey;
use crate::unit_ai::AiAction;
use crate::unit_ai::AiMove;
use crate::unit_ai::OrderForce;
use crate::unit_ai::task_finder::Diplomacy;
use crate::unit_ai::task_finder::world::TaskWorld;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlots;
use genie_diff::Diff;

/// One back-to-work record.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct BackToWork {
    /// `townBellTargetId` (`+0x9ac`): the top action's target (the pasture of a target in one),
    /// else the target of the unit AI's first own queued order.
    pub target: Option<ObjectId>,
    /// `town_bell_target_type` (`+0x9b0`): a Gather's (5) or Hunt's (`0x6e`) target class
    /// (`+0x68`), else −1.
    pub target_class: Option<ClassId>,
    /// `town_bell_action` (`+0x9b4`): the unit AI's current action (`+0x44`).
    pub action: Option<AiAction>,
    /// `townBellTargetId2` (`+0x9b8`): the top action's second target (`+0x34`).
    pub target2: Option<ObjectId>,
    /// `town_bell_target_x`, `_y` (`+0x9bc`, `+0x9c0`): the target's place, else the unit's own.
    pub point: Point,
}

impl BackToWork {
    /// A record of nothing, as a new unit's (`FUN_14121df60`) and the second record a work other
    /// than building drops: no ids, the point (−1, −1).
    pub const NONE: Self = Self {
        target: None,
        target_class: None,
        action: None,
        target2: None,
        point: Point::new(-1.0, -1.0),
    };
}

/// The town-bell flag and the two records of a combat object (type 70 and up).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct TownBell {
    /// `mTownBellFlag` (`+0x9dc`): a garrison order sent the villager into a building that takes
    /// villagers; no record is made while it holds.
    pub flag: bool,
    /// `mBackToWork1` (`+0x9ac`..`+0x9c0`): the work it was taken off last.
    pub work: BackToWork,
    /// `mBackToWork2` (`+0x9c4`..`+0x9d8`): the work before a building it was taken off.
    pub before_building: BackToWork,
}

impl Default for TownBell {
    fn default() -> Self {
        Self {
            flag: false,
            work: BackToWork::NONE,
            before_building: BackToWork::NONE,
        }
    }
}

impl TownBell {
    /// `FUN_141206a20`: with the flag clear, the unit's present work replaces the first record
    /// when any of its fields differs. The first record moves to the second when the new work is
    /// building (`0x25a`) and the old was not; any other new work clears the second (its ids −1,
    /// its point (−1, −1)). The new record ([`BackToWork`]) is the caller's: its target, target
    /// class, the unit AI's action, the second target and the point.
    pub fn record(&mut self, now: BackToWork) {
        if self.flag || now == self.work {
            return;
        }
        if now.action == Some(AiAction::Build) {
            if self.work.action != Some(AiAction::Build) {
                self.before_building = self.work;
            }
        } else {
            self.before_building = BackToWork::NONE;
        }
        self.work = now;
    }
}

/// The unit AI's actions whose record a finished building ends: building (`0x25a`), gathering
/// (`0x261`) and hunting (`0x265`) (`FUN_1411f8fa0`'s mask `0x881`).
const ENDED_ON_BUILDING: [AiAction; 3] = [AiAction::Build, AiAction::Gather, AiAction::Hunt];

/// What a back-to-work record's check (`FUN_1411f8fa0`) reads of an object the world holds:
/// whether it is a building (type 80), its master's class, and its build points (`+0xc50`)
/// against its master's build time (the first train location's train time).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordTarget {
    /// The object.
    pub id: ObjectId,
    /// Its master is of type 80.
    pub building: bool,
    /// Its master's class.
    pub class: ClassId,
    /// It has had its master's build time of work (`build_time <= +0xc50`).
    pub built: bool,
}

/// `FUN_1411f8fa0`: the object a record names, its target, else its second target, the world
/// holds (`lookup`); for the actions of [`ENDED_ON_BUILDING`] a building other than a farm
/// that is built ends the record (no object, no targets in the copy returned). A negative id
/// names none (`JL`), as −1 does.
fn record_object(
    record: BackToWork,
    lookup: &dyn Fn(ObjectId) -> Result<Option<RecordTarget>, GenieError>,
    roles: &Roles,
) -> Result<(Option<RecordTarget>, BackToWork), GenieError> {
    let mut out = record;
    let named = |id: Option<ObjectId>| id.filter(|id| !id.is_recyclable());
    let mut found = match named(record.target) {
        Some(t) => lookup(t)?,
        None => None,
    };
    if found.is_none() {
        let Some(t2) = named(record.target2) else {
            return Ok((None, out));
        };
        found = lookup(t2)?;
    }
    if let Some(o) = found
        && record
            .action
            .is_some_and(|a| ENDED_ON_BUILDING.contains(&a))
        && o.building
        // The farms' class, whose records a built farm does not end.
        && !roles.of_class(o.class).contains(ClassRoles::FARM)
        && o.built
    {
        out.target = None;
        out.target2 = None;
        return Ok((None, out));
    }
    Ok((found, out))
}

impl TownBell {
    /// `FUN_1412035a0`: the work to go back to and the object it is on. The first record's
    /// object ([`record_object`]) wins; else the second's, which then becomes the first (the
    /// second cleared); else no object and the first record as it is, or the second when the
    /// first has no point. The record returned keeps the point of the one whose object was
    /// found.
    ///
    /// # Errors
    ///
    /// The lookup's: an object whose master the world lacks.
    pub fn back_to_work(
        &mut self,
        lookup: &dyn Fn(ObjectId) -> Result<Option<RecordTarget>, GenieError>,
        roles: &Roles,
    ) -> Result<(Option<RecordTarget>, BackToWork), GenieError> {
        let (found, out) = record_object(self.work, lookup, roles)?;
        if found.is_some() {
            return Ok((found, out));
        }
        let before = self.before_building;
        let (found, out) = record_object(before, lookup, roles)?;
        if found.is_some() {
            self.work = before;
            self.before_building = BackToWork::NONE;
            return Ok((found, out));
        }
        let none = Point::new(-1.0, -1.0);
        if self.work.point == none {
            Ok((None, before))
        } else {
            Ok((None, self.work))
        }
    }
}

/// A record's point on neither axis (`DAT_14330ec20`).
const NO_POINT: f32 = GARRISON_NO_POINT;

impl World {
    /// `TOWN_BELL` (0x7f, `do_command`): the object `id` (none for −1, `FUN_140e6bad0`) of the
    /// commanding `player`, or of any player while players may order each other's
    /// ([`crate::world::settings::Settings::cheats_enabled`], `FUN_1412ffe00`; the cheat `+0xb12` is not held),
    /// rings its owner's bell ([`Bell::Ring`], [`crate::world::World::ring_town_bell`]) or ends it
    /// ([`crate::world::World::end_town_bell`]). A special object's (`+0x30c` bit 0) UI flag is
    /// not simulation.
    pub(crate) fn town_bell(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        bell: Bell,
    ) -> Result<(), GenieError> {
        let Some((owner, _)) = self.object(id) else {
            return Ok(());
        };
        if owner != player && !self.settings.cheats_enabled {
            return Ok(());
        }
        match bell {
            Bell::Ring => self.ring_town_bell(owner, id),
            Bell::End => self.end_town_bell(owner, id),
        }
    }

    /// `FUN_14163a650`: player `owner`'s town centre `tc` (master 109; any other object does
    /// nothing) rings: of its owner's live objects, in the list's order, those in state 2 not
    /// inside another (`FUN_141188da0`) within `GarrisonTuning::town_bell_reach` of it (`FUN_14118f8c0`; `COMISS`/`JC`: a
    /// NaN gap is out) are the units, a villager (class 4 of type 70) or master 1808, else the
    /// buildings (type 80) that take villagers
    /// ([`crate::objects::masters::GarrisonKinds::VILLAGERS`], `FUN_1411ccb60`), each with its
    /// count ([`crate::world::World::garrisoned`]) and its bell flag (`+0x9dc`) set (its sound,
    /// slot 0x4c0, is not simulation); then [`crate::world::World::shelter`] with the reach
    /// (`param_6` 1), no transports, not through the unit AIs.
    pub(crate) fn ring_town_bell(
        &mut self,
        owner: PlayerId,
        tc: ObjectId,
    ) -> Result<(), GenieError> {
        let Some((_, t)) = self.object(tc) else {
            return Ok(());
        };
        // The town centre, whose bell the command rings (`0x6109` raw).
        // The master that answers the bell besides the villagers (`0x710`).
        // The villagers' class.
        // The units' least type (`0x46`) and the buildings' (`'P'`).
        if !self
            .model
            .roles
            .of_master(t.life.master_id)
            .contains(MasterRoles::TOWN_CENTRE)
        {
            return Ok(());
        }
        let (at, radius) = self.ground(tc)?;
        let mut units = Vec::new();
        let mut shelters = Shelters::default();
        let mut k = ListIndex::FIRST;
        while self.players.player(owner)?.objects.holds(k) {
            let here = k;
            let o = &self.players.live(owner, here)?;
            k = k.next();
            if o.life.state != ObjectState::Alive || o.life.inside.is_some() {
                continue;
            }
            let id = o.id;
            let m = o.master(self.master_tables(), owner)?;
            let (kind, class, master) = (m.kind, m.class, m.id);
            let takes_villagers = m
                .building
                .is_some_and(|b| b.garrison_kind.contains(GarrisonKinds::VILLAGERS));
            let near = |t: &Self| -> Result<bool, GenieError> {
                let (other_at, other_radius) = t.ground(id)?;
                let gap = footprint_gap_squared(other_at, other_radius, at, radius);
                // COMISS reach, gap; JC: a gap above it, or a NaN, is out
                Ok(!below_or_unordered(
                    t.model.tuning.garrison.town_bell_reach,
                    gap.get(),
                ))
            };
            if (self
                .model
                .roles
                .of_class(class)
                .contains(ClassRoles::VILLAGER)
                && kind == MasterKind::COMBAT
                && near(self)?)
                || (self
                    .model
                    .roles
                    .of_master(master)
                    .contains(MasterRoles::MULE_CART)
                    && near(self)?)
            {
                units.push(id);
            } else if kind == MasterKind::BUILDING && takes_villagers && near(self)? {
                shelters.buildings.push(ContainerId::new(id));
                shelters.counts.push(self.garrisoned(id)?);
                self.parts.set_town_bell_flag(id, true);
            }
        }
        self.shelter(
            owner,
            &units,
            &shelters,
            ShelterReach::TownBell,
            Dispatch::Now,
        )
    }

    /// `FUN_14163ab80`: player `owner`'s town centre `tc` (master 109; any other object does
    /// nothing) ends its bell. Each of its owner's live objects, in the list's order, of type 80
    /// in a state below 3 that takes villagers (`FUN_1411ccb60` with 1) and holds its bell flag
    /// (`+0x9dc`) drops the flag; within `GarrisonTuning::town_bell_reach` of the centre (`COMISS`/`JC`: a NaN gap is
    /// out) it lets each unit inside of type 70 or more that is a villager (class 4) or of master
    /// 1808 back to work, from the last in ([`crate::world::World::back_to_work`]; its sound, slot
    /// 0x4c0, is not simulation). Then every unit its owner's tracker files under class 4
    /// (`FUN_14104f8f0`) and master 1808 (`FUN_14104f960`), in that order, not inside another
    /// (`+0x2f0`) goes back to work from where it is.
    pub(crate) fn end_town_bell(
        &mut self,
        owner: PlayerId,
        tc: ObjectId,
    ) -> Result<(), GenieError> {
        let Some((_, t)) = self.object(tc) else {
            return Ok(());
        };
        if !self
            .model
            .roles
            .of_master(t.life.master_id)
            .contains(MasterRoles::TOWN_CENTRE)
        {
            return Ok(());
        }
        let (at, radius) = self.ground(tc)?;
        let mut k = ListIndex::FIRST;
        while self.players.player(owner)?.objects.holds(k) {
            let idx = k;
            k = k.next();
            let b = &self.players.live(owner, idx)?;
            let m = b.master(self.master_tables(), owner)?;
            let takes_villagers = m
                .building
                .is_some_and(|x| x.garrison_kind.contains(GarrisonKinds::VILLAGERS));
            if m.kind != MasterKind::BUILDING
                || ObjectState::Dying <= b.life.state
                || !takes_villagers
                || !self.parts.town_bell_of(b.id).flag
            {
                continue;
            }
            let id = b.id;
            self.parts.set_town_bell_flag(id, false);
            let (building_at, building_radius) = self.ground(id)?;
            // COMISS reach, gap; JC: a gap above it, or a NaN, lets none out
            let gap = footprint_gap_squared(building_at, building_radius, at, radius);
            if below_or_unordered(self.model.tuning.garrison.town_bell_reach, gap.get()) {
                continue;
            }
            self.workers_out(owner, idx)?;
        }
        self.workers_back(owner)
    }

    /// `ALL_BACK_TO_WORK` (`do_command` 0x8c; another player's object only while players may
    /// order each other's, [`crate::world::settings::Settings::cheats_enabled`]; the cheat `+0xb12` is
    /// not held): `FUN_141617420` for the owner of object `id`, as the bell's end
    /// ([`crate::world::World::end_town_bell`]) without its flags: each of the owner's live
    /// objects, in the list's order, in state 2, not inside another (`+0x2f0`), of type 80, that
    /// takes villagers (`FUN_1411ccb60` with 1) and is within `GarrisonTuning::town_bell_reach` of the object
    /// (`COMISS`/`JC`: a NaN gap is out) lets its villagers and units of master 1808 out back to
    /// work, from the last in; then the units its owner's tracker files under class 4 and master
    /// 1808 not inside another go back from where they are. The game screen's flag (`+0x1874`) is
    /// not simulation.
    pub(crate) fn all_back_to_work(
        &mut self,
        player: PlayerId,
        id: ObjectId,
    ) -> Result<(), GenieError> {
        let Some((owner, _)) = self.object(id) else {
            return Ok(());
        };
        if owner != player && !self.settings.cheats_enabled {
            return Ok(());
        }
        let (at, radius) = self.ground(id)?;
        let mut k = ListIndex::FIRST;
        while self.players.player(owner)?.objects.holds(k) {
            let idx = k;
            k = k.next();
            let b = &self.players.live(owner, idx)?;
            let m = b.master(self.master_tables(), owner)?;
            let takes_villagers = m
                .building
                .is_some_and(|x| x.garrison_kind.contains(GarrisonKinds::VILLAGERS));
            if b.life.state != ObjectState::Alive
                || b.life.inside.is_some()
                || m.kind != MasterKind::BUILDING
                || !takes_villagers
            {
                continue;
            }
            let (building_at, building_radius) = self.ground(b.id)?;
            // COMISS reach, gap; JC: a gap above it, or a NaN, lets none out
            let gap = footprint_gap_squared(building_at, building_radius, at, radius);
            if below_or_unordered(self.model.tuning.garrison.town_bell_reach, gap.get()) {
                continue;
            }
            self.workers_out(owner, idx)?;
        }
        self.workers_back(owner)
    }

    /// The building at `idx` of player `owner` lets each unit inside of type 70 or more that is a
    /// villager (class 4) or of master 1808 back to work, from the last in
    /// ([`crate::world::World::back_to_work`]; its sound, slot 0x4c0, is not simulation).
    fn workers_out(&mut self, owner: PlayerId, idx: ListIndex) -> Result<(), GenieError> {
        let mut i = self.players.live(owner, idx)?.garrison.len();
        while 0 < i {
            i -= 1;
            let Some(&inside) = self.players.live(owner, idx)?.garrison.get(i) else {
                continue;
            };
            // any player's unit; an id the world no longer holds is a null entry
            let Some((up, u)) = self.object(inside) else {
                continue;
            };
            let um = u.master(self.master_tables(), up)?;
            if MasterKind::COMBAT <= um.kind
                && (self
                    .model
                    .roles
                    .of_class(um.class)
                    .contains(ClassRoles::VILLAGER)
                    || self
                        .model
                        .roles
                        .of_master(um.id)
                        .contains(MasterRoles::MULE_CART))
            {
                let _ = self.back_to_work(Some((owner, idx)), inside)?;
            }
        }
        Ok(())
    }

    /// Every unit player `owner`'s tracker files under class 4 (`FUN_14104f8f0`) and master
    /// 1808 (`FUN_14104f960`), in that order, not inside another (`+0x2f0`) goes back to work
    /// from where it is.
    fn workers_back(&mut self, owner: PlayerId) -> Result<(), GenieError> {
        let tracker = &self.players.player(owner)?.tracker;
        let mut ids = tracker.class_list(self.model.slots.class.villager).to_vec();
        ids.extend_from_slice(tracker.list(ListKey::master(self.model.slots.master.mule_cart)));
        // a queued unit's place (−1) names no object
        for id in ids.into_iter().flatten() {
            // the tracker's keepers follow every object its player gains and loses: the world
            // holds each (the game reads it without a test)
            let Some((_, u)) = self.object(id) else {
                continue;
            };
            if u.life.inside.is_none() {
                let _ = self.back_to_work(None, id)?;
            }
        }
        Ok(())
    }

    /// `FUN_1412081e0`'s part for a unit outside a building (its `param_2` null), after the
    /// records' lookup: with its bell flag (`+0x9dc`) and the record's point not −1 on either
    /// axis (`UCOMISS`/`JP`/`JZ`: only an exact −1 stops it, a NaN goes on), it goes there: with
    /// no object to work on, slot 0x340 (`FUN_141216c50`, [`crate::world::World::move_aside`]) to
    /// the point on the ground, range 0, forced, not walking; else slot 0x7e8 on the object at
    /// the point with the carried-resource test, any task, `+0xa14` 0. Then it is no longer under
    /// the town bell (`+0xa14` takes 1, not held). True when it went.
    pub(crate) fn back_outside(
        &mut self,
        id: ObjectId,
        work_on: Option<ObjectId>,
        point: Point,
    ) -> Result<bool, GenieError> {
        let Some(Place::Live(up, ui)) = self.place(id) else {
            return Ok(false);
        };
        if !self.parts.town_bell_of(id).flag {
            return Ok(false);
        }
        let (x, y) = (point.x(), point.y());
        if x == NO_POINT || y == NO_POINT {
            return Ok(false);
        }
        match work_on {
            None => self.move_aside(
                up,
                ui,
                AiMove {
                    point: Position::new(x, y, 0.0),
                    range: Distance::ZERO,
                    force: OrderForce::Forced,
                    walk: false,
                },
            )?,
            Some(target) => self.order_work(
                up,
                ui,
                &Work {
                    target: Some(target),
                    point: Position::new(x, y, 0.0),
                    carry_test: true,
                    filter: None,
                    notified: false,
                },
            )?,
        }
        if self.object(id).is_some() {
            self.parts.set_town_bell_flag(id, false);
        }
        Ok(true)
    }

    /// `FUN_1412067f0` before slot 0x7e8's work command, for a villager or master 1808 and a
    /// target the world holds: the town-bell record ([`World::record_town_bell`]); then, with the
    /// garrison filter (3), the flag goes on for a building in state 2 of the player or an ally
    /// whose master takes villagers
    /// ([`crate::objects::masters::GarrisonKinds::VILLAGERS`]), with any other filter it goes off.
    /// The UI's flag for the viewing player's building (`+0x1874`) is not held.
    pub(crate) fn town_bell_order(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
        filter: Option<ActionKind>,
    ) -> Result<(), GenieError> {
        let unit = &self.players.live(player, idx)?;
        let master = unit.master(self.master_tables(), player)?;
        if !(self
            .model
            .roles
            .of_class(master.class)
            .contains(ClassRoles::VILLAGER)
            || self
                .model
                .roles
                .of_master(master.id)
                .contains(MasterRoles::MULE_CART))
        {
            return Ok(());
        }
        let Some((tp, t)) = self.object(target) else {
            return Ok(());
        };
        let takes = if filter == Some(GARRISON) {
            let tm = t.master(self.master_tables(), tp)?;
            let world = TaskWorld {
                players: &self.players,
                drop_sites: &self.model.drop_sites,
            };
            (tm.kind == MasterKind::BUILDING
                && t.life.state == ObjectState::Alive
                && world.allied(player, tp, AllyTest::WithRelation4)
                && tm
                    .building
                    .is_some_and(|b| b.garrison_kind.contains(GarrisonKinds::VILLAGERS)))
            .then_some(true)
        } else {
            Some(false)
        };
        self.record_town_bell(player, idx)?;
        if let Some(flag) = takes {
            let id = self.players.live(player, idx)?.id;
            self.parts.set_town_bell_flag(id, flag);
        }
        Ok(())
    }

    /// `FUN_141206a20`: the unit's present work as its town-bell record
    /// ([`crate::orders::town_bell::TownBell::record`]): its top action's target (slot 400), or, for one
    /// in a pasture the world holds, the pasture; without one the target of its unit AI's first
    /// own queued order (`FUN_140c4da50`: issued by the unit, with a target), else none at its own
    /// place. Then a Gather's or Hunt's target class (`+0x68`), its unit AI's current action
    /// (`+0x44`) and the top action's second target (`FUN_140a78430`). The targets count only
    /// where the world holds them.
    pub(crate) fn record_town_bell(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
    ) -> Result<(), GenieError> {
        use crate::orders::town_bell::BackToWork;
        let unit = &self.players.live(player, idx)?;
        if self.parts.town_bell_of(unit.id).flag {
            return Ok(());
        }
        if self.parts.unread.contains(unit.id) {
            return Err(GenieError::unsupported(
                "a town-bell record of an action not held",
            ));
        }
        // an id below 0 names none (`JL`)
        let held = |id: ObjectId| (!id.is_recyclable()).then(|| self.object(id)).flatten();
        let mut found = self
            .parts
            .actions_of(unit.id)
            .top_target()
            .and_then(held)
            .map(|(_, t)| t);
        if let Some(t) = found
            && let Some(pasture) = t.pasture
        {
            found = held(pasture.id()).map(|(_, p)| p);
        }
        if found.is_none() {
            let own = self
                .parts
                .unit_ai
                .get(unit.id)
                .and_then(|ai| own_order_target(ai, unit.id));
            found = own.and_then(held).map(|(_, t)| t);
        }
        let (target, point) = found.map_or_else(
            || (None, unit.position().ground()),
            |t| (Some(t.id), t.position().ground()),
        );
        let actions = &self.parts.actions_of(unit.id);
        let target_class = match actions.top_action_kind() {
            Some(HUNT) => actions.list.hunt().and_then(|h| h.target_class),
            Some(GATHER) => actions.list.gather().and_then(|g| g.target_class),
            _ => None,
        };
        let ai = self
            .parts
            .unit_ai
            .get(unit.id)
            .ok_or(GenieError::unsupported(
                "a town-bell record of a unit without a unit AI",
            ))?;
        let target2 = actions
            .top_second_target()
            .and_then(held)
            .map(|(_, t)| t.id);
        let now = BackToWork {
            target,
            target_class,
            action: ai.current_action,
            target2,
            point,
        };
        let id = self.players.live(player, idx)?.id;
        let mut bell = self.parts.town_bell_of(id);
        bell.record(now);
        self.parts.set_town_bell(id, bell);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::map::Point;
    use crate::objects::ObjectId;
    use crate::test_ids::{class_of, object_id};
    use crate::testing::class;
    use crate::testing::roles;

    use super::*;

    /// Object `id`'s town bell in `t`.
    fn bell(t: &World, id: i32) -> TownBell {
        t.parts.town_bell_of(object_id(id))
    }

    /// Object `id`'s town bell in `t` changed by `change`.
    fn set_bell(t: &mut World, id: i32, change: impl FnOnce(&mut TownBell)) {
        let mut b = bell(t, id);
        change(&mut b);
        t.parts.set_town_bell(object_id(id), b);
    }

    fn work(target: Option<ObjectId>, action: Option<AiAction>, point: Point) -> BackToWork {
        BackToWork {
            target,
            action,
            point,
            ..BackToWork::NONE
        }
    }

    #[test]
    fn new_work_replaces_the_record_and_building_keeps_the_one_before() {
        // MarvinBlue/426048731's 9181: its first record (no target, its own place), then the
        // INTERACT of update 748 while it builds 10028, then the one of 797 while it cuts a tree
        let mut bell = TownBell {
            work: work(None, None, Point::new(29.0, 99.0)),
            ..TownBell::default()
        };
        let building = work(
            Some(object_id(10028)),
            Some(AiAction::Build),
            Point::new(30.0, 88.0),
        );
        bell.record(building);
        assert_eq!(bell.work, building);
        assert_eq!(
            bell.before_building,
            work(None, None, Point::new(29.0, 99.0))
        );
        let cutting = BackToWork {
            target_class: class_of(15),
            ..work(
                Some(object_id(9365)),
                Some(AiAction::Hunt),
                Point::new(24.5, 97.5),
            )
        };
        bell.record(cutting);
        assert_eq!(bell.work, cutting);
        assert_eq!(bell.before_building, BackToWork::NONE);
        // the same work again changes nothing, nor does any work while the flag holds
        bell.before_building = building;
        bell.record(cutting);
        assert_eq!(bell.before_building, building);
        bell.flag = true;
        bell.record(work(
            Some(object_id(1)),
            Some(AiAction::Wait),
            Point::new(3.0, 4.0),
        ));
        assert_eq!(bell.work, cutting);
    }

    /// The objects of a test world: a house 7 that is built, a house 8 that is not, a farm 9
    /// that is built, a sheep 10.
    fn object(id: ObjectId) -> Option<RecordTarget> {
        let (building, class, built) = match id.get() {
            7 => (true, class::BUILDING, true),
            8 => (true, class::BUILDING, false),
            9 => (true, class::FARM, true),
            10 => (false, class::LIVESTOCK, false),
            _ => return None,
        };
        Some(RecordTarget {
            id,
            building,
            class,
            built,
        })
    }

    #[test]
    fn record_names_its_target_else_its_second_target_unless_built_building_ended_it() {
        // FUN_1411f8fa0: the target the world holds
        let shepherd = BackToWork {
            target2: Some(object_id(7)),
            ..work(
                Some(object_id(10)),
                Some(AiAction::Gather),
                Point::new(5.0, 6.0),
            )
        };
        assert_eq!(
            record_object(shepherd, &|id| Ok(object(id)), &roles()).unwrap(),
            (object(object_id(10)), shepherd)
        );
        // a target the world no longer holds: the second target, unless it is below 0
        let gone = BackToWork {
            target2: Some(object_id(10)),
            ..work(
                Some(object_id(11)),
                Some(AiAction::Hunt),
                Point::new(5.0, 6.0),
            )
        };
        assert_eq!(
            record_object(gone, &|id| Ok(object(id)), &roles()).unwrap(),
            (object(object_id(10)), gone)
        );
        let none = BackToWork {
            target2: None,
            ..gone
        };
        assert_eq!(
            record_object(none, &|id| Ok(object(id)), &roles()).unwrap(),
            (None, none)
        );
        let lost = BackToWork {
            target2: Some(object_id(12)),
            ..gone
        };
        assert_eq!(
            record_object(lost, &|id| Ok(object(id)), &roles()).unwrap(),
            (None, lost)
        );
        // a target below 0 is not looked up: the second target is
        let second = BackToWork {
            target2: Some(object_id(10)),
            ..work(None, Some(AiAction::Build), Point::new(5.0, 6.0))
        };
        assert_eq!(
            record_object(second, &|id| Ok(object(id)), &roles()).unwrap(),
            (object(object_id(10)), second)
        );
        // building, gathering and hunting (mask 0x881 over 0x25a..0x265) on a built building
        // other than a farm end the record: no object, both targets −1 in the copy
        for action in [AiAction::Build, AiAction::Gather, AiAction::Hunt].map(Some) {
            let on_house = BackToWork {
                target2: Some(object_id(10)),
                ..work(Some(object_id(7)), action, Point::new(5.0, 6.0))
            };
            let ended = BackToWork {
                target: None,
                target2: None,
                ..on_house
            };
            assert_eq!(
                record_object(on_house, &|id| Ok(object(id)), &roles()).unwrap(),
                (None, ended)
            );
            // reached through the second target, too
            let through_second = BackToWork {
                target2: Some(object_id(7)),
                ..work(Some(object_id(11)), action, Point::new(5.0, 6.0))
            };
            let ended = BackToWork {
                target: None,
                target2: None,
                ..through_second
            };
            assert_eq!(
                record_object(through_second, &|id| Ok(object(id)), &roles()).unwrap(),
                (None, ended)
            );
            // a house not built yet and a built farm stay
            for kept in [8, 9] {
                let r = work(Some(object_id(kept)), action, Point::new(5.0, 6.0));
                assert_eq!(
                    record_object(r, &|id| Ok(object(id)), &roles()).unwrap(),
                    (object(object_id(kept)), r)
                );
            }
        }
        // the actions between them, and any other, keep a built building
        for action in [
            Some(AiAction::Heal),
            Some(AiAction::Move),
            Some(AiAction::Follow),
            Some(AiAction::Transport),
            Some(AiAction::Defend),
            None,
        ] {
            let r = work(Some(object_id(7)), action, Point::new(5.0, 6.0));
            assert_eq!(
                record_object(r, &|id| Ok(object(id)), &roles()).unwrap(),
                (object(object_id(7)), r)
            );
        }
    }

    mod rings {
        use super::{bell, set_bell};
        use crate::commands::Bell;
        #[cfg(test)]
        use crate::garrison::ContainerId;
        use crate::garrison::tests::{
            ARCHER, VILLAGER, inside_tc, let_out_world, sized_world, tracked, walker,
        };
        use crate::map::{Point, Position};
        use crate::objects::ObjectId;
        #[cfg(test)]
        use crate::orders::town_bell::BackToWork as TownBellBackToWork;
        use crate::players::PlayerId;
        use crate::test_ids::object_id;
        use crate::test_ids::player_id;
        use crate::world::World;

        fn enter(t: &World, id: i32) -> Option<ObjectId> {
            t.parts
                .actions_of(object_id(id))
                .list
                .enter()
                .and_then(|e| e.target)
                .map(ContainerId::id)
        }

        #[test]
        fn the_bell_sends_the_villagers_in_reach_to_the_buildings_in_reach_and_flags_them() {
            // FUN_14163a650: villager 1000 is within 25 tiles of town centre 100, 1001 is not,
            // archer 1002 is no villager; the centre takes villagers and is in reach of itself
            let mut t = sized_world(60, 60);
            walker(
                &mut t,
                PlayerId::new(1),
                1000,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            walker(
                &mut t,
                PlayerId::new(1),
                1001,
                VILLAGER,
                Point::new(45.5, 45.5),
            );
            walker(&mut t, PlayerId::new(1), 1002, ARCHER, Point::new(6.5, 5.5));
            t.town_bell(player_id(1), object_id(100), Bell::Ring)
                .unwrap();
            assert_eq!(
                [1000, 1001, 1002].map(|id| enter(&t, id)),
                [Some(object_id(100)), None, None]
            );
            assert!(bell(&t, 100).flag);
        }

        #[test]
        fn the_bell_is_its_owners_or_any_players_with_allied_control() {
            let mut t = let_out_world();
            walker(
                &mut t,
                PlayerId::new(1),
                1000,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            // player 2 rings player 1's centre: refused
            t.town_bell(player_id(2), object_id(100), Bell::Ring)
                .unwrap();
            assert_eq!(enter(&t, 1000), None);
            t.settings.cheats_enabled = true;
            t.town_bell(player_id(2), object_id(100), Bell::Ring)
                .unwrap();
            assert_eq!(enter(&t, 1000), Some(object_id(100)));
            // an object that is no town centre rings nothing; nor does an id the world lacks
            let mut t = let_out_world();
            walker(
                &mut t,
                PlayerId::new(1),
                1000,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            t.town_bell(player_id(1), object_id(1000), Bell::Ring)
                .unwrap();
            t.town_bell(player_id(1), object_id(4242), Bell::Ring)
                .unwrap();
            assert_eq!(enter(&t, 1000), None);
        }

        #[test]
        fn the_bells_end_lets_the_villagers_out_and_sends_the_others_back_by_the_tracker() {
            // FUN_14163ab80: the centre drops its flag and lets villager 1000 out toward its
            // record; villager 1001 outside, filed under class 4, walks back to its record's
            // point (slot 0x340); 1002 has no flag and stays; 1003 is not filed and stays
            let mut t = let_out_world();
            set_bell(&mut t, 100, |b| b.flag = true);
            inside_tc(&mut t, PlayerId::new(1), 1000, VILLAGER);
            walker(
                &mut t,
                PlayerId::new(1),
                1001,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            walker(
                &mut t,
                PlayerId::new(1),
                1002,
                VILLAGER,
                Point::new(6.5, 5.5),
            );
            walker(
                &mut t,
                PlayerId::new(1),
                1003,
                VILLAGER,
                Point::new(7.5, 5.5),
            );
            for (id, point) in [
                (1000, Point::new(4.5, 4.5)),
                (1001, Point::new(3.5, 3.5)),
                (1002, Point::new(2.5, 2.5)),
            ] {
                set_bell(&mut t, id, |v| {
                    v.flag = id != 1002;
                    v.work = TownBellBackToWork {
                        point,
                        ..TownBellBackToWork::NONE
                    };
                });
            }
            set_bell(&mut t, 1003, |b| b.flag = true);
            set_bell(&mut t, 1003, |b| b.work.point = Point::new(1.5, 1.5));
            tracked(&mut t, VILLAGER, &[1000, 1001, 1002]);
            t.town_bell(player_id(1), object_id(100), Bell::End)
                .unwrap();
            assert!(!bell(&t, 100).flag);
            let v = t.object(object_id(1000)).unwrap().1;
            assert_eq!(
                (v.life.inside, t.parts.town_bell_of(v.id).flag),
                (None, false)
            );
            let goal = |t: &World, id: i32| {
                let u = t.object(object_id(id)).unwrap().1;
                (
                    t.parts
                        .actions_of(object_id(id))
                        .list
                        .move_to()
                        .copied()
                        .and_then(|m| m.goal()),
                    t.parts.town_bell_of(u.id).flag,
                )
            };
            assert_eq!(goal(&t, 1001), (Some(Position::new(3.5, 3.5, 0.0)), false));
            assert_eq!(goal(&t, 1002), (None, false));
            assert_eq!(goal(&t, 1003), (None, true));
            // a record without a point on an axis (an exact -1) keeps the unit and its flag
            let mut t = let_out_world();
            walker(
                &mut t,
                PlayerId::new(1),
                1001,
                VILLAGER,
                Point::new(5.5, 5.5),
            );
            set_bell(&mut t, 1001, |v| {
                v.flag = true;
                v.work.point = Point::new(3.5, -1.0);
            });
            tracked(&mut t, VILLAGER, &[1001]);
            t.town_bell(player_id(1), object_id(100), Bell::End)
                .unwrap();
            assert_eq!(goal(&t, 1001), (None, true));
        }
        #[test]
        fn all_back_to_work_lets_the_villagers_out_and_sends_the_others_back_leaving_the_flag() {
            // ALL_BACK_TO_WORK (do_command 0x8c, FUN_141617420): as the bell's end, but with no
            // bell flag to test or drop: villager 1000 comes out of the centre toward its
            // record, 1001 outside, filed under class 4, walks back to its record's point
            let world = || {
                let mut t = let_out_world();
                set_bell(&mut t, 100, |b| b.flag = true);
                inside_tc(&mut t, PlayerId::new(1), 1000, VILLAGER);
                walker(
                    &mut t,
                    PlayerId::new(1),
                    1001,
                    VILLAGER,
                    Point::new(5.5, 5.5),
                );
                for (id, point) in [(1000, Point::new(4.5, 4.5)), (1001, Point::new(3.5, 3.5))] {
                    set_bell(&mut t, id, |v| {
                        v.flag = true;
                        v.work = TownBellBackToWork {
                            point,
                            ..TownBellBackToWork::NONE
                        };
                    });
                }
                tracked(&mut t, VILLAGER, &[1000, 1001]);
                t
            };
            let mut t = world();
            t.all_back_to_work(player_id(1), object_id(100)).unwrap();
            assert!(bell(&t, 100).flag);
            let v = t.object(object_id(1000)).unwrap().1;
            assert_eq!(
                (v.life.inside, t.parts.town_bell_of(v.id).flag),
                (None, false)
            );
            let u = t.object(object_id(1001)).unwrap().1;
            assert_eq!(
                (
                    t.parts
                        .actions_of(object_id(1001))
                        .list
                        .move_to()
                        .copied()
                        .and_then(|m| m.goal()),
                    t.parts.town_bell_of(u.id).flag
                ),
                (Some(Position::new(3.5, 3.5, 0.0)), false)
            );
            // another player's command does nothing without the game's cheats
            let mut t = world();
            t.all_back_to_work(player_id(2), object_id(100)).unwrap();
            assert_eq!(
                t.object(object_id(1000)).unwrap().1.life.inside,
                Some(ContainerId::new(object_id(100)))
            );
        }
    }

    #[test]
    fn back_to_work_takes_the_first_record_else_the_second_which_moves_up() {
        // FUN_1412035a0: the first record's object wins; the records stay
        let first = work(
            Some(object_id(10)),
            Some(AiAction::Gather),
            Point::new(5.0, 6.0),
        );
        let second = work(
            Some(object_id(8)),
            Some(AiAction::Build),
            Point::new(1.0, 2.0),
        );
        let mut bell = TownBell {
            flag: false,
            work: first,
            before_building: second,
        };
        assert_eq!(
            bell.back_to_work(&|id| Ok(object(id)), &roles()).unwrap(),
            (object(object_id(10)), first)
        );
        assert_eq!((bell.work, bell.before_building), (first, second));
        // the first's object gone: the second's object, and the second becomes the first
        let gone = work(
            Some(object_id(11)),
            Some(AiAction::Gather),
            Point::new(5.0, 6.0),
        );
        bell.work = gone;
        assert_eq!(
            bell.back_to_work(&|id| Ok(object(id)), &roles()).unwrap(),
            (object(object_id(8)), second)
        );
        assert_eq!(
            (bell.work, bell.before_building),
            (second, BackToWork::NONE)
        );
        // neither has an object: the first record as it is, its targets kept even when a built
        // building ended it
        let ended = work(
            Some(object_id(7)),
            Some(AiAction::Build),
            Point::new(5.0, 6.0),
        );
        let mut bell = TownBell {
            flag: false,
            work: ended,
            before_building: work(
                Some(object_id(12)),
                Some(AiAction::Gather),
                Point::new(1.0, 2.0),
            ),
        };
        assert_eq!(
            bell.back_to_work(&|id| Ok(object(id)), &roles()).unwrap(),
            (None, ended)
        );
        assert_eq!(bell.work, ended);
        // ... or the second when the first has no point (−1, −1 on both axes)
        let pointless = work(
            Some(object_id(11)),
            Some(AiAction::Gather),
            Point::new(-1.0, -1.0),
        );
        let before = work(
            Some(object_id(12)),
            Some(AiAction::Gather),
            Point::new(1.0, 2.0),
        );
        let mut bell = TownBell {
            flag: false,
            work: pointless,
            before_building: before,
        };
        assert_eq!(
            bell.back_to_work(&|id| Ok(object(id)), &roles()).unwrap(),
            (None, before)
        );
        // one axis at −1 is a point
        let half = work(
            Some(object_id(11)),
            Some(AiAction::Gather),
            Point::new(-1.0, 3.0),
        );
        bell.work = half;
        assert_eq!(
            bell.back_to_work(&|id| Ok(object(id)), &roles()).unwrap(),
            (None, half)
        );
        assert_eq!((bell.work, bell.before_building), (half, before));
    }
}
