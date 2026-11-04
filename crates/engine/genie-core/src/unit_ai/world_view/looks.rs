//! What the unit AI reads through its view of its unit and of the objects it names: the unit's
//! action list as it stands now, its patrol path, the stand and reach tests, the build answer,
//! an object's look and its target flags, a master's class, and what a damage notify weighs
//! (the duel, a harmless attacker, a defended unit's nearest attacker).

use crate::GenieError;
use crate::construction::FoundationId;
use crate::map::Distance;
use crate::map::Point;
use crate::math::rng::Stream;
use crate::movement::Planning;
use crate::movement::WaypointIndex;
use crate::movement::footprint_gap_squared;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::unit::UnitFlags;
use crate::orders::object_gone;
use crate::players::AllyTest;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::allied;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{UnitView, patrol_information};
use crate::unit_ai::{Duel, NearestAttacker, PatrolPath, TargetFlags, UnitSeen};
use crate::world::index::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;

impl UnitView<'_, '_> {
    /// The unit's slot 0x430 as it stands now (`FUN_14106d0e0`; its slot 0x818, `FUN_14106d100`,
    /// is 0): its action list holds an action, the stops and commands its AI made in the update
    /// included ([`SurroundingFlags::BUSY`] at the update's start).
    ///
    /// [`SurroundingFlags::BUSY`]: crate::unit_ai::SurroundingFlags::BUSY
    pub(crate) fn busy_now(&self) -> bool {
        self.actions.top_action_kind().is_some() || self.c.w.parts.unread.contains(self.unit.id)
    }

    /// The unit's slot 0x2d0 as it stands now: its top action may be interrupted
    /// ([`SurroundingFlags::INTERRUPTIBLE`] at the update's start).
    ///
    /// # Errors
    ///
    /// An action whose slot 0x2d0 the slice does not model.
    ///
    /// [`SurroundingFlags::INTERRUPTIBLE`]: crate::unit_ai::SurroundingFlags::INTERRUPTIBLE
    pub(crate) fn interruptible_now(&self) -> Result<bool, GenieError> {
        self.actions
            .interruptible(self.c.w.parts.unread.get(self.unit.id))
    }

    /// The unit's slot 0x468 (`FUN_141157c50`, `getPatrolInformation`): a copy of its top path
    /// (`FUN_140f960f0`; `FUN_140f95c00` into a new path: its points, its length, its flags) and
    /// the waypoint of its top level (`+0x24`); None without one.
    pub(crate) fn patrol_information(&self) -> Option<(PatrolPath, WaypointIndex)> {
        patrol_information(self.c.w.parts.mover_of(self.unit.id))
    }

    /// Slot 400 (`FUN_14106d090`) as the full update's tail reads it: the target of the unit's
    /// top action as it stands now (`FUN_140a78450`, [`Action::top_target`]: an object the world
    /// holds), the commands the unit took inside the update included (its attack command,
    /// [`UnitView::attack`]; a wait for a foundation and a Transport hold none); None without
    /// one.
    ///
    /// [`Action::top_target`]: crate::objects::layers::Action::top_target
    pub(crate) fn top_target_now(&self) -> Option<ObjectId> {
        let target = self.actions.top_target();
        target.filter(|&t| self.c.w.spatial.get(t).is_some())
    }

    /// What the full update's tail tests of object `id`, the target of its unit's top action
    /// ([`TargetFlags`]), found as [`UnitView::seen_unit`] finds it; None when the world does not
    /// hold it.
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    pub(crate) fn target_flags(&self, id: ObjectId) -> Result<Option<TargetFlags>, GenieError> {
        let Some(seen) = self.seen_unit(id)? else {
            return Ok(None);
        };
        let u = owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?
        .get(id)
        .ok_or_else(|| object_gone(id))?;
        let kind = u.master(self.c.w.master_tables(), seen.owner)?.kind;
        Ok(Some(TargetFlags {
            owner: seen.owner,
            kind,
            capture: self.c.w.parts.combat_of(u.id).capture_flag,
            pasture: u.pasture,
            untargetable: u.flags.contains(UnitFlags::UNTARGETABLE),
            attackable: u.flags.contains(UnitFlags::ATTACKABLE),
        }))
    }

    /// The object `id` as the unit AI reads it ([`UnitSeen`]): None when the world does not
    /// hold it (`FUN_1415417a0`).
    ///
    /// # Errors
    ///
    /// An object the slice does not hold.
    pub(crate) fn seen_unit(&self, id: ObjectId) -> Result<Option<UnitSeen>, GenieError> {
        let Some(p) = self.c.w.spatial.get(id) else {
            return Ok(None);
        };
        let (owner, u) = OthersRef::of(&self.c.w.players, &self.c.w.index, self.c.player)?
            .find(id)
            .ok_or(GenieError::unsupported("an object the slice does not hold"))?;
        let m = u.master(self.c.w.master_tables(), owner)?;
        let me = self.unit.master(self.c.w.master_tables(), self.player)?;
        Ok(Some(UnitSeen {
            id,
            class: m.class,
            owner,
            combat_level: m.combat_level,
            state: u.life.state,
            target: self
                .c
                .w
                .parts
                .unit_ai
                .get(u.id)
                .and_then(|a| a.current_target),
            gap_squared: footprint_gap_squared(
                self.unit.position(),
                me.radius,
                p.position,
                p.radius,
            ),
            position: p.position,
            hp: u.life.hp,
            master: u.life.master_id,
            kind: m.kind,
            main: self.c.w.parts.building_of(u.id).annex_of,
            allied: allied(
                &self.c.w.players.player(self.player)?.relations,
                self.player,
                owner,
                AllyTest::WithRelation4,
            ),
        }))
    }

    /// The class of master `master` of player `player` (`FUN_14152fe40`, the player's master
    /// table `+0x28`: the masters the slice holds of each player): None for a player or master
    /// the game does not hold.
    pub(crate) fn master_class(&self, player: PlayerId, master: MasterId) -> Option<ClassId> {
        self.c
            .w
            .model
            .masters
            .get(player.index())
            .and_then(|ms| ms.get(master))
            .map(|m| m.class)
    }

    /// What a computer player's unit weighs of object `other` (its current target or its
    /// attacker) on a damage notify ([`Duel`], [`UnitAi::damaged`]).
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    ///
    /// [`UnitAi::damaged`]: crate::unit_ai::UnitAi::damaged
    pub(crate) fn duel(&self, other: ObjectId) -> Result<Duel, GenieError> {
        let roles = &self.c.w.model.roles.clone();
        self.picker(None)?.duel(
            &self.c.w.spatial,
            other,
            roles,
            &self.c.w.model.tuning.damage,
            &self.c.w.model.tuning.unit_ai,
        )
    }

    /// The villager's test of its attacker on a damage notify: the attacker shows too little
    /// attack, armor and range to run from, and is fought back as Gaia's are
    /// ([`UnitAi::villager_damaged`]).
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    ///
    /// [`UnitAi::villager_damaged`]: crate::unit_ai::UnitAi::villager_damaged
    pub(crate) fn harmless(&self, attacker: ObjectId) -> Result<bool, GenieError> {
        let roles = &self.c.w.model.roles.clone();
        self.picker(None)?.harmless(
            &self.c.w.spatial,
            attacker,
            roles,
            &self.c.w.model.tuning.unit_ai,
        )
    }

    /// The attacker of object `target` nearest to it (its unit AI's slot 0x78, `FUN_140c48610`:
    /// of its attackers in their order, those the world holds, the least footprint gap,
    /// `FUN_14118f680`, the first of a tie; [`NearestAttacker`], [`UnitAi::defend_misc`]).
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    ///
    /// [`UnitAi::defend_misc`]: crate::unit_ai::UnitAi::defend_misc
    pub(crate) fn defended_attacker(
        &self,
        target: ObjectId,
    ) -> Result<NearestAttacker, GenieError> {
        self.picker(None)?.nearest_attacker_of(target)
    }

    /// The unit's slot 0x278 (`FUN_141158720`, [`Spatial::point_clear`]): the unit, of its radius
    /// and its restriction's terrain classes, could stand at `point`.
    ///
    /// # Errors
    ///
    /// A unit without movement state, and what the test does not model.
    ///
    /// [`Spatial::point_clear`]: crate::map::spatial::Spatial::point_clear
    pub(crate) fn can_stand(&mut self, point: Point) -> Result<bool, GenieError> {
        let unit = &*self.unit;
        let mover = self
            .c
            .w
            .parts
            .mover_of(unit.id)
            .ok_or(GenieError::unsupported(
                "a stand test of a unit without movement state",
            ))?;
        let by_id = ObjectsRef::new(
            &self.c.w.players,
            &self.c.w.index,
            &self.c.w.model.masters,
            &self.c.w.parts,
            &self.c.w.model.roles,
            &self.c.w.model.tuning,
        );
        self.c.w.spatial.point_clear(
            unit.id,
            point,
            Distance::new(mover.stats.radius.x()),
            mover.stats.restriction_mask,
            by_id,
        )
    }

    /// The unit's slot 0x3d0: whether it reaches object `target` within `range`.
    ///
    /// # Errors
    ///
    /// A unit that does not move, and what the reach test does not model.
    pub(crate) fn reaches(
        &mut self,
        target: ObjectId,
        range: Distance,
    ) -> Result<bool, GenieError> {
        let mut plan = Planning {
            pathing: &mut self.c.w.pathing,
            now: self.c.w.world_time,
            player_kind: self.player_kind,
            object: self.c.w.streams.get(Stream::Object),
            groups: &self.c.w.groups,
        };
        let unit = &*self.unit;
        let walker = unit.me(self.actions);
        let mover = self
            .c
            .w
            .parts
            .mover_of(unit.id)
            .ok_or(GenieError::unsupported(
                "a reach test of a unit that does not move",
            ))?;
        let by_id = ObjectsRef::new(
            &self.c.w.players,
            &self.c.w.index,
            &self.c.w.model.masters,
            &self.c.w.parts,
            &self.c.w.model.roles,
            &self.c.w.model.tuning,
        );
        mover.reaches_object(
            (unit.position(), &walker),
            (&mut self.c.w.spatial, by_id),
            &mut plan,
            target,
            range,
            unit.life.master_id,
        )
    }

    /// The unit's build command's answer (slot 0x380, forced) on `foundation` before it runs:
    /// false for a foundation the world does not hold or no task for it (its 0).
    ///
    /// # Errors
    ///
    /// What the task search does not model.
    pub(crate) fn builds(&self, foundation: FoundationId) -> Result<bool, GenieError> {
        let roles = &self.c.w.model.roles.clone();
        let slots = self.c.w.model.slots;
        self.builds_on(foundation, slots, roles)
    }
}
