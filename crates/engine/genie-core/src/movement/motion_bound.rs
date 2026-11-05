//! genie's bound on how far a unit may move and act (no routine of the game): what a stopped
//! player's unit may have done since it froze ([`World::grow_unknown`],
//! [`Spatial::check_known`]). The game goes on updating a frozen unit, so the bound reads only
//! what its master, the model and the objects allow: the master's speed (`+0x740`, slot 0xe8)
//! times the largest factor of its sprites and the speed-up, the speed powerups, the charge's run, and the
//! farthest work or weapon range with the range powerups.
//!
//! [`World::grow_unknown`]: crate::world::World::grow_unknown
//! [`Spatial::check_known`]: crate::map::spatial::Spatial::check_known

use crate::GenieResult;
use crate::combat::Charge;
use crate::combat::ClassOrMaster;
use crate::combat::aura::share_of;
use crate::combat::powerup::sped_by_infantry;
use crate::combat::powerup::{Powerup, PowerupAmount, PowerupAttribute, powerup_amount};
use crate::combat::stingers::STINGER;
use crate::map::Distance;
use crate::movement::Speed;
use crate::movement::run_factor;
use crate::objects::Graphics;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::masters::SpecialAttack;
use crate::players::PlayerId;
use crate::world::Model;
use crate::world::layer::Roles;
use crate::world::layer::generated::MovementTuning;
use crate::world::parts::Parts;
use crate::world::player::Player;
use std::collections::BTreeMap;
use std::ptr;

/// An aura task (type 155) of a master that raises the speed or the weapon range: what an
/// object it names may take.
#[derive(Clone, Copy, Debug, PartialEq)]
struct AuraTerm {
    /// Its powerup: an object holds one of a source and attribute at a time (the map's key,
    /// [`World::apply_powerup`]).
    ///
    /// [`World::apply_powerup`]: crate::world::World::apply_powerup
    powerup: Powerup,
    /// The objects it powers.
    target: ClassOrMaster,
}

/// What may raise a unit's top speed (slot 0x208, [`TopSpeed::of`]) above its master's speed
/// and its weapon range above its master's, over the model and the objects as they stand
/// ([`MotionBound::of`]).
///
/// [`TopSpeed::of`]: crate::movement::TopSpeed::of
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MotionBound<'world> {
    /// The graphics, for the largest factor (`speed_multiplier`) of the sprites a master shows.
    graphics: &'world Graphics,
    /// The largest share of its master's speed a speed-up adds (`+0x80c`), none below 0: only a
    /// loaded one, as no routine of build 158041 sets one.
    speedup: f32,
    /// The speed and range auras of the sources' masters, by the objects they name.
    auras: BTreeMap<ClassOrMaster, Vec<AuraTerm>>,
    /// What the stinger tasks (type 157) that raise the speed sting: each hit adds its value
    /// again, so a unit they name may move without bound.
    raising_stingers: Vec<ClassOrMaster>,
    /// The speed each unit of infantry inside a siege unit adds (`powerup::SPEED_PER_INFANTRY`).
    per_infantry: f32,
    /// The roles, for the siege units the infantry speeds up.
    roles: &'world Roles,
    /// The charge's run without a run task.
    movement: &'world MovementTuning,
}

impl<'world> MotionBound<'world> {
    /// The bound of `model`, `parts` and `players` as they stand: the graphics, the movers'
    /// speed-ups and the aura and stinger tasks of the masters of the objects the players hold
    /// (live or asleep; each master once): a powerup or a stinger needs its source near. A
    /// source a stopped player's building made in the game since its freeze is not held.
    ///
    /// # Errors
    ///
    /// An object whose master the world lacks.
    pub(crate) fn of(model: &'world Model, parts: &Parts, players: &[Player]) -> GenieResult<Self> {
        let speedup = parts
            .motion
            .iter()
            .filter_map(|(_, motion)| motion.mover())
            .filter(|mover| mover.speedup.building.is_some())
            .map(|mover| mover.speedup.value.get())
            .fold(0.0, larger_bound);
        let tables = MasterTables::new(&model.masters, &parts.own_master);
        let mut sources: Vec<&Master> = Vec::new();
        for (player_id, player) in PlayerId::each(players.len()).zip(players) {
            for unit in player.objects.iter().chain(player.sleeping.iter()) {
                let master = unit.master(tables, player_id)?;
                if !sources.iter().any(|source| ptr::eq(*source, master)) {
                    sources.push(master);
                }
            }
        }
        let tasks = || {
            sources.iter().flat_map(|master| {
                master
                    .action_part()
                    .tasks
                    .iter()
                    .map(move |task| (*master, task))
            })
        };
        let mut auras: BTreeMap<ClassOrMaster, Vec<AuraTerm>> = BTreeMap::new();
        for (master, task) in tasks().filter(|(_, task)| task.action_kind == ActionKind::AURA) {
            let aura = AuraTerm {
                powerup: Powerup {
                    attribute: PowerupAttribute::of_task(task.wait_time),
                    source: master.id,
                    amount: PowerupAmount::new(task.work_val1.get()),
                    share: share_of(task),
                },
                target: ClassOrMaster::of_task(task),
            };
            if matches!(
                aura.powerup.attribute,
                PowerupAttribute::MOVEMENT_SPEED | PowerupAttribute::MAXIMUM_RANGE
            ) {
                auras.entry(aura.target).or_default().push(aura);
            }
        }
        let raising_stingers = tasks()
            .filter(|(_, task)| {
                task.action_kind == STINGER
                    && PowerupAttribute::of_task(task.wait_time) == PowerupAttribute::MOVEMENT_SPEED
                    && !at_most_zero(task.work_val1.get())
            })
            .map(|(_, task)| ClassOrMaster::of_task(task))
            .collect();
        Ok(Self {
            graphics: &model.graphics,
            speedup,
            auras,
            raising_stingers,
            per_infantry: model.tuning.powerup.speed_per_infantry,
            roles: &model.roles,
            movement: &model.tuning.movement,
        })
    }

    /// The most a unit of `master` may move in a second: its speed times the largest factor of
    /// the sprites it shows ([`Master::shown_sprites`], at least 1: a factor not above 0 applies
    /// none) plus the largest speed-up, plus its speed powerups (each source's largest, and
    /// a siege unit's infantry inside to its garrison capacity), times its charge's run factor
    /// (at least 1) where its master has a charge of special attack 3; without bound where a stinger may raise it
    /// or the sum is not a number.
    #[must_use]
    pub(crate) fn speed(&self, master: &Master) -> Speed {
        if self
            .raising_stingers
            .iter()
            .any(|target| target.names(master.class, master.id))
        {
            return Speed::new(f32::INFINITY);
        }
        let base = master.animated_part().max_speed.get();
        let sprite = master
            .shown_sprites()
            .into_iter()
            .filter_map(|sprite_id| self.graphics.get(sprite_id))
            .map(|graphic| graphic.speed_multiplier.get())
            .fold(1.0, larger_bound);
        let own = base * (sprite + self.speedup);
        let infantry = if sped_by_infantry(master, self.roles) {
            master.garrison.units() as f32 * self.per_infantry
        } else {
            0.0
        };
        let powerups = self.aura_sum(master, PowerupAttribute::MOVEMENT_SPEED) + infantry.max(0.0);
        // the run needs a ready charge of special attack 3 (`FUN_1412161b0`)
        let creatable = master.creatable_part();
        let run = if master.kind == MasterKind::COMBAT
            && Charge::ZERO < creatable.max_charge
            && creatable.special_attack == SpecialAttack::CHARGE
        {
            larger_bound(1.0, run_factor(master, self.movement).get())
        } else {
            1.0
        };
        Speed::new(bounded((own + powerups) * run))
    }

    /// The farthest a unit of `master` may act beyond its footprint: its largest work range, or
    /// its weapon range plus its range powerups (each source's largest).
    #[must_use]
    pub(crate) fn reach(&self, master: &Master) -> Distance {
        let weapon = master.combat_part().weapon_range.get()
            + self.aura_sum(master, PowerupAttribute::MAXIMUM_RANGE);
        Distance::new(bounded(
            master
                .action_part()
                .tasks
                .iter()
                .map(|task| task.work_range.get())
                .fold(weapon, larger_bound),
        ))
    }

    /// What the auras of `attribute` that name `master` may add to it: per source master its
    /// largest amount ([`powerup_amount`], none below 0), summed.
    fn aura_sum(&self, master: &Master, attribute: PowerupAttribute) -> f32 {
        // the targets that can name it: any (below 0), its class's and its master's; a term
        // found twice counts once, as each source counts its largest
        let any = self.auras.range(..ClassOrMaster::of_data(0));
        let own = [
            ClassOrMaster::of_class(master.class),
            ClassOrMaster::of_master(master.id),
        ]
        .into_iter()
        .filter_map(|target| self.auras.get_key_value(&target));
        let mut by_source: BTreeMap<MasterId, f32> = BTreeMap::new();
        for aura in any.chain(own).flat_map(|(_, terms)| terms).filter(|aura| {
            aura.powerup.attribute == attribute && aura.target.names(master.class, master.id)
        }) {
            let adds = powerup_amount(master, aura.powerup).map_or(0.0, PowerupAmount::get);
            let most = by_source.entry(aura.powerup.source).or_insert(0.0);
            *most = larger_bound(*most, adds);
        }
        by_source.values().sum()
    }
}

/// The larger of `bound` and `value`; a `value` that is not a number has no bound.
pub(crate) const fn larger_bound(bound: f32, value: f32) -> f32 {
    if value.is_nan() {
        f32::INFINITY
    } else {
        bound.max(value)
    }
}

/// `value` as a bound: none below 0, and none for a value that is not a number.
const fn bounded(value: f32) -> f32 {
    larger_bound(0.0, value)
}

/// Whether `value` is at most 0 (a NaN is not: a stinger of it may raise the speed).
const fn at_most_zero(value: f32) -> bool {
    value <= 0.0
}

#[cfg(test)]
mod tests;
