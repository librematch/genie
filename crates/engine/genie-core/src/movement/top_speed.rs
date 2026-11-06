//! A moving object's top speed (object slot 0x208, `maximumSpeed`): RGE's (`FUN_14114d1d0`, its
//! own speed with a speed-up building's share) and a TRIBE combat object's (`FUN_141202c70`):
//! none while a lock holds it, else its own speed plus its speed powerup and stinger, and the run
//! of a ready charge of special attack 3 (`FUN_1412161b0`) near its target.

use crate::GenieResult;
use crate::combat::Charge;
use crate::combat::LockdownCount;
use crate::combat::damage::SpecialCharge;
use crate::combat::damage::charge_works;
use crate::groups::GroupCommandKind;
use crate::groups::UnitGroups;
use crate::groups::WorldObject;
use crate::groups::world_object_in;
use crate::map::Distance;
use crate::map::Point;
use crate::math::fmath::sqrt;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::Task;
use crate::objects::masters::player_master;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::UnitAi;
use crate::world::Model;
use crate::world::OBJECT_MASTER;
use crate::world::World;
use crate::world::index::ObjectIndex;
use crate::world::layer::generated::MovementTuning;
use crate::world::parts::Parts;
use crate::world::player::Player;

/// The task whose work values the run of a ready charge reads: type 0x85, any `work_flag`,
/// `work_flag2` 0x7d1.
const RUN_TASK: (ActionKind, Option<i16>, Option<i16>) =
    (ActionKind::ACTION_133, None, Some(0x7d1));

impl Mover {
    /// RGE's slot 0x208 (`FUN_14114d1d0`): the top speed with its sprite ([`MoveStats::speed`],
    /// the master's speed times the sprite's factor above 0), plus the master's speed times its
    /// speed-up's value while it holds a building (`+0x808` not −1).
    ///
    /// [`MoveStats::speed`]: crate::movement::MoveStats::speed
    #[must_use]
    pub fn own_speed(&self) -> Speed {
        if self.speedup.building.is_some() {
            Speed::new(self.stats.speed.get() + self.stats.base.get() * self.speedup.value.get())
        } else {
            self.stats.speed
        }
    }

    /// Slot 0x208 of the object at `at` that moves with this layer and reads `top`: its top
    /// speed there ([`TopSpeed::of`] over [`Mover::own_speed`]).
    #[must_use]
    pub fn top_speed(&self, top: TopSpeed, at: Point) -> Speed {
        top.of(self.own_speed(), at)
    }
}

/// What slot 0x208 reads besides the object's own speed and its place: nothing for RGE's
/// (`FUN_14114d1d0`), a TRIBE combat object's terms for its (`FUN_141202c70`, types 70 and 80).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TopSpeed {
    /// RGE's slot: the object's own speed.
    #[default]
    Own,
    /// A TRIBE combat object's.
    Combat(CombatSpeed),
}

/// A TRIBE combat object's terms of its top speed (`FUN_141202c70`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CombatSpeed {
    /// `locked_down_count` (`+0xa08`): while a lock holds it, the top speed is 0.
    pub locked_down: LockdownCount,
    /// `mSpeedPowerup` (`+0xa2c`).
    pub powerup: Speed,
    /// `mSpeedStinger` (`+0xb38`).
    pub stinger: Speed,
    /// The run of its ready charge ([`ChargeRun`]): none for an object of another type than 70,
    /// a master without a charge (`max_charge`, `+0x918`, not above 0: `COMISS`, `JBE`, a NaN
    /// has none), a charge not ready ([`SpeedReads::special_charge_ready`]) or no target.
    pub run: Option<ChargeRun>,
}

/// The run of a ready charge: near its target the object's top speed grows by a factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChargeRun {
    /// The target's place (`+0x3c8`, `+0x4c8`).
    pub target: Point,
    /// The radius on x (`+0x5d8`) of the object's master and of the target's.
    pub radii: (Distance, Distance),
    /// The gap below which it runs: its master's task (0x85, any, 0x7d1)'s `work_val2`
    /// (`+0x2c`), else `MovementTuning::charge_run_reach`.
    pub reach: Distance,
    /// The factor it runs by: that task's `work_range` (`+0x30`) when not 0 (`UCOMISS`, `JP`
    /// then `JZ`: a NaN is not 0), else `MovementTuning::charge_run_factor`.
    pub factor: RunFactor,
}

/// The factor a ready charge's run multiplies the top speed by ([`ChargeRun::factor`]).
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct RunFactor(f32);

impl RunFactor {
    /// The factor of the float `value` (a task's `work_range`, a tuning value).
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    /// The float.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl TopSpeed {
    /// Slot 0x208 of the object at `at` that reads these terms and moves with the layer `mover`:
    /// 0 without one (a static object's `0x141071810`), else [`Mover::top_speed`].
    #[must_use]
    pub fn of_object(self, mover: Option<&Mover>, at: Point) -> Speed {
        mover.map_or(Speed::ZERO, |mover| mover.top_speed(self, at))
    }

    /// The top speed of an object whose own speed is `own` (RGE's slot, [`Mover::own_speed`])
    /// at `at`: for a combat object, 0 while a lock holds it (`CMP` with 0, `JLE`); else `own`
    /// plus its powerup plus its stinger (in that order); a sum of 0 (`UCOMISS`, `JP` then
    /// `JZ`) stays, any other runs on a ready charge within its reach ([`ChargeRun::reaches`]).
    #[must_use]
    pub fn of(self, own: Speed, at: Point) -> Speed {
        let Self::Combat(terms) = self else {
            return own;
        };
        if terms.locked_down.holds() {
            return Speed::ZERO;
        }
        let speed = own.get() + terms.powerup.get() + terms.stinger.get();
        if speed == 0.0 {
            return Speed::new(speed);
        }
        match terms.run {
            Some(run) if run.reaches(at) => Speed::new(speed * run.factor.get()),
            _ => Speed::new(speed),
        }
    }
}

impl ChargeRun {
    /// An object at `at` runs: the gap between the places (`dy² + dx²`, its root), less its
    /// master's radius on x, less the target's, is below the reach (`COMISS`, `JBE`: a NaN is
    /// not).
    #[must_use]
    pub fn reaches(self, at: Point) -> bool {
        let dx = self.target.x() - at.x();
        let dy = self.target.y() - at.y();
        let gap = sqrt(dy * dy + dx * dx);
        gap - self.radii.0.get() - self.radii.1.get() < self.reach.get()
    }
}

/// The factor a ready charge of `master` runs by (slot 0x208's tail): its task (0x85, any,
/// 0x7d1, `FUN_14109e1a0`)'s `work_range` when not 0 (`UCOMISS`, `JP` then `JZ`: a NaN is not
/// 0), else the tuning's `charge_run_factor`; without the task, the tuning's.
#[must_use]
pub(crate) fn run_factor(master: &Master, tuning: &MovementTuning) -> RunFactor {
    let range = task_by_flags(&master.action_part().tasks, RUN_TASK)
        .map_or(0.0, |task| task.work_range.get());
    RunFactor::new(if range == 0.0 {
        tuning.charge_run_factor
    } else {
        range
    })
}

/// `FUN_14109e1a0`: the first task of `tasks` (a master's list, `+0x788`) of action `kind`, of
/// `work_flag` `flag` and of `work_flag2` `flag2`, each None for any (the routine's −1).
#[must_use]
pub fn task_by_flags(
    tasks: &[Task],
    (kind, flag, flag2): (ActionKind, Option<i16>, Option<i16>),
) -> Option<&Task> {
    tasks.iter().find(|task| {
        task.action_kind == kind
            && flag.is_none_or(|work_flag| task.work_flag == work_flag)
            && flag2.is_none_or(|work_flag2| task.work_flag2 == work_flag2)
    })
}

/// What slot 0x208 reads of the world (`SpeedReads::of`): the players' lists and the table by
/// id (the charge's target, `FUN_1415417a0`), the objects' parts (the combat layer, the mover,
/// the unit AI), the unit groups (`FUN_14152ff40`) and the model (the masters, the tuning, the
/// roles). It borrows no table a walk changes, so a caller holding the planners reads it too.
#[derive(Clone, Copy)]
pub(crate) struct SpeedReads<'world> {
    /// The players, whose lists hold the objects.
    players: &'world [Player],
    /// The world's table by id.
    index: &'world ObjectIndex,
    /// The objects' parts.
    parts: &'world Parts,
    /// The world's unit groups.
    groups: &'world UnitGroups,
    /// The model: the masters, the tuning, the roles and the slots.
    model: &'world Model,
}

impl<'world> SpeedReads<'world> {
    /// The reads of the world's `players`, its table by id `index`, its `parts`, its `groups` and
    /// its `model`.
    #[must_use]
    pub(crate) const fn of(
        players: &'world [Player],
        index: &'world ObjectIndex,
        parts: &'world Parts,
        groups: &'world UnitGroups,
        model: &'world Model,
    ) -> Self {
        Self {
            players,
            index,
            parts,
            groups,
            model,
        }
    }

    /// No objects, parts or groups, the test build's model: a test's reads of an empty world.
    #[cfg(test)]
    pub(crate) fn empty() -> SpeedReads<'static> {
        use crate::testing::model;
        use crate::world::parts::no_parts;
        // A constant that holds a `Vec` is not promoted: the inline `const` lends it for
        // `'static`.
        let groups = const { &UnitGroups::NONE };
        SpeedReads::of(&[], ObjectIndex::scanning(), no_parts(), groups, model())
    }

    /// Slot 0x208 of `unit` of player `player` at its place, its unit AI's target `ai_target`
    /// (`+0x48`; none without a unit AI): 0 without movement (a static object's
    /// `0x141071810`), else [`Mover::top_speed`] with [`SpeedReads::terms`].
    ///
    /// # Errors
    ///
    /// A unit whose master the world lacks, or a charge's target whose master it lacks.
    pub(crate) fn top_speed_with(
        self,
        player: PlayerId,
        unit: &Unit,
        ai_target: Option<ObjectId>,
    ) -> GenieResult<Speed> {
        let Some(mover) = self.parts.mover_of(unit.id) else {
            return Ok(Speed::ZERO);
        };
        let terms = self.terms(player, unit, ai_target)?;
        Ok(mover.top_speed(terms, unit.position().ground()))
    }

    /// [`SpeedReads::top_speed_with`] with the target of the unit AI the world holds for `unit`
    /// (none while its unit AI is lent out: its caller passes the target).
    ///
    /// # Errors
    ///
    /// As [`SpeedReads::top_speed_with`].
    pub(crate) fn top_speed(self, player: PlayerId, unit: &Unit) -> GenieResult<Speed> {
        self.top_speed_with(player, unit, self.ai_target(unit.id))
    }

    /// The target (`+0x48`) of the unit AI the world holds for object `id`; none without one.
    pub(crate) fn ai_target(self, id: ObjectId) -> Option<ObjectId> {
        self.parts
            .unit_ai
            .get(id)
            .and_then(|unit_ai: &UnitAi| unit_ai.current_target)
    }

    /// What slot 0x208 of `unit` of player `player` (its unit AI's target `ai_target`) reads
    /// besides its own speed and its place ([`TopSpeed`]): RGE's for a master below type 70, a
    /// combat object's terms for types 70 and 80, the run of its ready charge for a master of
    /// type 70 with a charge.
    ///
    /// # Errors
    ///
    /// A unit whose master the world lacks, or a charge's target whose master it lacks.
    pub(crate) fn terms(
        self,
        player: PlayerId,
        unit: &Unit,
        ai_target: Option<ObjectId>,
    ) -> GenieResult<TopSpeed> {
        let master = unit.master(self.master_tables(), player)?;
        if master.kind != MasterKind::COMBAT && master.kind != MasterKind::BUILDING {
            return Ok(TopSpeed::Own);
        }
        let combat = self.parts.combat_of(unit.id);
        let run = if master.kind == MasterKind::COMBAT
            && Charge::ZERO < master.creatable_part().max_charge
        {
            self.charge_run(unit, ai_target, master)?
        } else {
            None
        };
        Ok(TopSpeed::Combat(CombatSpeed {
            locked_down: combat.locked_down,
            powerup: combat.speed_powerup,
            stinger: combat.speed_stinger,
            run,
        }))
    }

    /// The masters an object's master pointer reads.
    fn master_tables(self) -> MasterTables<'world> {
        MasterTables::new(&self.model.masters, &self.parts.own_master)
    }

    /// The run of `unit`'s ready charge (slot 0x208's tail, its unit AI's target `ai_target`):
    /// its own target lookup ([`SpeedReads::run_target`]), then the charge's test
    /// ([`SpeedReads::special_charge_ready`]); with both, the reach and factor of its master's
    /// task (0x85, any, 0x7d1, `FUN_14109e1a0`).
    fn charge_run(
        self,
        unit: &Unit,
        ai_target: Option<ObjectId>,
        master: &Master,
    ) -> GenieResult<Option<ChargeRun>> {
        let target = self.run_target(unit, ai_target);
        if !self.special_charge_ready(unit, ai_target, master)? {
            return Ok(None);
        }
        let Some(target) = target else {
            return Ok(None);
        };
        let tuning = &self.model.tuning.movement;
        let reach = task_by_flags(&master.action_part().tasks, RUN_TASK)
            .map_or(tuning.charge_run_reach, |task| task.work_val2.get());
        let factor = run_factor(master, tuning);
        Ok(Some(ChargeRun {
            target: target.position().ground(),
            radii: (
                Distance::new(master.radius.x()),
                Distance::new(self.object_master(target)?.radius.x()),
            ),
            reach: Distance::new(reach),
            factor,
        }))
    }

    /// Object `id` of the world's table (`FUN_1415417a0`, [`world_object_in`]).
    fn world_object(self, id: Option<ObjectId>) -> Option<WorldObject<'world>> {
        world_object_in(self.players, self.index, self.parts, id)
    }

    /// Slot 0x208's target of `unit`: its unit AI's target `ai_target` (`+0x48`,
    /// `FUN_140c4cc40`) when the world holds it (`FUN_1415417a0`), else its group's target
    /// (`+0x3c0` of the group `+0x6d0`, `FUN_14152ff40`) whatever the group's command.
    fn run_target(self, unit: &Unit, ai_target: Option<ObjectId>) -> Option<WorldObject<'world>> {
        if let Some(target) = self.world_object(ai_target) {
            return Some(target);
        }
        let group = unit.group.id.and_then(|id| self.groups.get(id))?;
        self.world_object(ObjectId::of_field(group.current_target.target.saved().0))
    }

    /// `FUN_1412161b0`: `unit` (its unit AI's target `ai_target`, none without a unit AI; its
    /// master `master`) has a ready charge of special attack 3 on a target. Its master's
    /// special attack is 3, it is not inside another (`+0x2f0`, `FUN_141188da0`), alive (state
    /// 2), of type 70, and its charge (`+0x9e0`) is at least its master's most less
    /// `MovementTuning::charge_ready_slack` (both widened: `COMISD` then `JC`, a NaN is not).
    /// Its target is its unit AI's (`+0x48`) when the world holds it, else its group's
    /// (`+0x3c0`) while the group attacks (command 9). The charge works on the target's class
    /// and type ([`charge_works`], `FUN_1412623d0`), and the target is not inside another and
    /// alive.
    ///
    /// # Errors
    ///
    /// A target whose master the world lacks.
    pub(crate) fn special_charge_ready(
        self,
        unit: &Unit,
        ai_target: Option<ObjectId>,
        master: &Master,
    ) -> GenieResult<bool> {
        let creatable = master.creatable_part();
        if creatable.special_attack != SpecialAttack::CHARGE
            || unit.life.inside.is_some()
            || unit.life.state != ObjectState::Alive
            || master.kind != MasterKind::COMBAT
        {
            return Ok(false);
        }
        let charge = f64::from(self.parts.combat_of(unit.id).charge.get());
        let most = f64::from(creatable.max_charge.get());
        let ready = most - self.model.tuning.movement.charge_ready_slack <= charge;
        if !ready {
            return Ok(false);
        }
        let Some(target) = self.charge_target(unit, ai_target) else {
            return Ok(false);
        };
        let target_master = self.object_master(target)?;
        Ok(charge_works(
            creatable.charge_target,
            creatable.charge_kind,
            target_master.class,
            target_master.kind,
            &self.model.roles,
        ) && !target.inside()
            && target.state() == ObjectState::Alive)
    }

    /// [`SpeedReads::special_charge_ready`]'s answer, which a hit's read of its attacker holds
    /// ([`crate::combat::damage::Attacker`]).
    ///
    /// # Errors
    ///
    /// As [`SpeedReads::special_charge_ready`].
    pub(crate) fn special_charge(
        self,
        unit: &Unit,
        ai_target: Option<ObjectId>,
        master: &Master,
    ) -> GenieResult<SpecialCharge> {
        self.special_charge_ready(unit, ai_target, master)
            .map(SpecialCharge::of)
    }

    /// `FUN_1412161b0`'s target of `unit`: its unit AI's target `ai_target` when the world
    /// holds it, else its group's target while the group's command is an attack (9).
    fn charge_target(
        self,
        unit: &Unit,
        ai_target: Option<ObjectId>,
    ) -> Option<WorldObject<'world>> {
        if let Some(target) = self.world_object(ai_target) {
            return Some(target);
        }
        let group = unit.group.id.and_then(|id| self.groups.get(id))?;
        if group.current_command != GroupCommandKind::ATTACK {
            return None;
        }
        self.world_object(ObjectId::of_field(group.current_target.target.saved().0))
    }

    /// The master an object of the world's table reads through its master pointer: a unit's
    /// ([`Unit::master`]), a ghost's the build's `doppelganger` of its player.
    ///
    /// # Errors
    ///
    /// A master the world lacks.
    pub(crate) fn object_master(self, object: WorldObject<'world>) -> GenieResult<&'world Master> {
        match object {
            WorldObject::Unit(owner, unit, _) => unit.master(self.master_tables(), owner),
            WorldObject::Ghost(owner, _) => {
                let master_id = object.master_id(self.model.slots);
                player_master(&self.model.masters, owner, master_id).ok_or_else(|| {
                    OBJECT_MASTER
                        .with("player", owner)
                        .with("master", master_id)
                })
            }
        }
    }
}

impl World {
    /// What slot 0x208 reads of the world ([`SpeedReads`]).
    pub(crate) fn speed_reads(&self) -> SpeedReads<'_> {
        SpeedReads::of(
            &self.players,
            &self.index,
            &self.parts,
            &self.groups,
            &self.model,
        )
    }

    /// [`SpeedReads::top_speed_with`] over the world.
    ///
    /// # Errors
    ///
    /// As [`SpeedReads::top_speed_with`].
    pub(crate) fn top_speed_with(
        &self,
        player: PlayerId,
        unit: &Unit,
        ai_target: Option<ObjectId>,
    ) -> GenieResult<Speed> {
        self.speed_reads().top_speed_with(player, unit, ai_target)
    }

    /// [`SpeedReads::top_speed`] over the world.
    ///
    /// # Errors
    ///
    /// As [`SpeedReads::top_speed_with`].
    pub(crate) fn top_speed(&self, player: PlayerId, unit: &Unit) -> GenieResult<Speed> {
        self.speed_reads().top_speed(player, unit)
    }

    /// [`SpeedReads::ai_target`] over the world.
    pub(crate) fn ai_target(&self, id: ObjectId) -> Option<ObjectId> {
        self.speed_reads().ai_target(id)
    }

    /// [`SpeedReads::terms`] over the world, the unit AI's target `ai_target` given (its unit
    /// AI lent out).
    ///
    /// # Errors
    ///
    /// As [`SpeedReads::terms`].
    pub(crate) fn top_speed_terms_with(
        &self,
        player: PlayerId,
        unit: &Unit,
        ai_target: Option<ObjectId>,
    ) -> GenieResult<TopSpeed> {
        self.speed_reads().terms(player, unit, ai_target)
    }

    /// [`SpeedReads::special_charge`] over the world, the unit AI's target `ai_target` given (its
    /// unit AI lent out).
    ///
    /// # Errors
    ///
    /// As [`SpeedReads::special_charge_ready`].
    pub(crate) fn special_charge_with(
        &self,
        unit: &Unit,
        ai_target: Option<ObjectId>,
        master: &Master,
    ) -> GenieResult<SpecialCharge> {
        self.speed_reads().special_charge(unit, ai_target, master)
    }

    /// [`SpeedReads::special_charge`] over the world with the target of the unit AI it holds for
    /// `unit`.
    ///
    /// # Errors
    ///
    /// As [`SpeedReads::special_charge_ready`].
    pub(crate) fn special_charge(
        &self,
        unit: &Unit,
        master: &Master,
    ) -> GenieResult<SpecialCharge> {
        let reads = self.speed_reads();
        reads.special_charge(unit, reads.ai_target(unit.id), master)
    }

    /// [`SpeedReads::terms`] over the world with the target of the unit AI it holds for `unit`.
    ///
    /// # Errors
    ///
    /// As [`SpeedReads::terms`].
    pub(crate) fn top_speed_terms(&self, player: PlayerId, unit: &Unit) -> GenieResult<TopSpeed> {
        let reads = self.speed_reads();
        reads.terms(player, unit, reads.ai_target(unit.id))
    }
}

#[cfg(test)]
mod tests;
