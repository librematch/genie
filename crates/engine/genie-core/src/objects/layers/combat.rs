//! `RGE_Combat_Object` and `TRIBE_Combat_Object` (types 50 and up, FD-045-D06): the combat
//! layer, its fields in the save's order (`RgeCombatObject`, then `TribeCombatObject`). Every
//! object holds it for now, with the game's defaults below type 50 (T4 step 1).

use crate::combat::Charge;
use crate::combat::KillState;
use crate::combat::LockdownCount;
use crate::combat::abilities::Abilities;
use crate::combat::damage::{ArmorBonuses, ArmorPair};
use crate::combat::powerup::{AppliedPowerup, ConversionPowerup, InfantryCount};
use crate::combat::stingers::ActiveStinger;
use crate::combat::{
    AbilityTimer, AttackBonus, AttackCount, Firepower, MissileCount, SpecialSprite,
};
use crate::construction::BuilderCount;
use crate::garrison::RaiderCountdown;
use crate::gathering::DecayTimer;
use crate::gathering::SalvageAmounts;
use crate::map::Distance;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::Regeneration;
use crate::objects::WorkRate;
use crate::orders::OrderSource;
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::world::Seconds;
use crate::world::WorldTime;
use genie_diff::Diff;

/// The combat layer of an object: its attack's timer and count, its capture flag, its timed
/// ability, its volleys, its special sprite, a salvage pile's amounts and decay timer, its unit
/// AI, its town bell, its builders, its charge, its powerups and stingers, its store.
#[derive(Clone, Debug, Default, PartialEq, Diff)]
pub struct Combat {
    /// `+0x91c`, `attack_timer`: the time until it may attack again; `mReloadStinger` and
    /// `mReloadPowerup` (`+0xb40`, `+0xa28`), which change its reload time.
    pub attack_timer: Seconds,
    /// `mReloadWorldtime` (`+0x920`, slots 0x248 and 0x250): the world time its attack timer
    /// runs out at (`FUN_141089820`), 0 for none; once past, the volley under way is over.
    pub reload_worldtime: WorldTime,
    /// `+0x900`, `attack_count`: its swings at a target, melee or ranged (`FUN_141081020`); a
    /// missile's is its firing object's when fired.
    #[diff(
        gap = "differs on 533 of MarvinBlue/426048731's samples (every 10th of updates 1..=3000; 9181 at update 1171: genie 2, game 0): genie resets it where the game does not, which resets it only on a new target (FUN_140a7c210, FUN_140a82a10)"
    )]
    pub attack_count: AttackCount,
    /// `mHitSomethingValidForChargeAttack` (`+0x8a8`): its last hit was its charge's.
    pub charge_hit: bool,
    /// `m_specialAbilityDurationMs` and `m_abilityActivatedTimeMs` (`+0x914`, `+0x918`): its
    /// timed ability ([`AbilityTimer`]).
    pub ability: AbilityTimer,
    /// `mProcessedKill` (`+0x954`): whether its kill is counted ([`KillState`]).
    pub kill_state: KillState,
    /// `next_volley` (`+0xa04`): the missiles of the volley under way already fired.
    pub next_volley: MissileCount,
    /// `using_special_attack_animation` (`+0xa05`): whether it shows its master's special sprite
    /// ([`SpecialSprite`]).
    pub special_sprite: SpecialSprite,
    /// `attr_amt` (`+0x990`..`+0x99a`): what a salvage pile holds of six attributes
    /// ([`SalvageAmounts`]).
    pub salvage: SalvageAmounts,
    /// `decay_timer` (`+0x99c`): the milliseconds before a salvage pile loses one of what it
    /// holds ([`DecayTimer`]).
    pub decay_timer: DecayTimer,
    /// A combat object's `raider_build_countdown` (`+0x9a0`): the seconds it may stay inside a
    /// building.
    pub raider_countdown: RaiderCountdown,
    /// `locked_down_count` (`+0xa08`): the locks that hold it in place, its top speed 0 while
    /// any holds ([`LockdownCount`]).
    pub locked_down: LockdownCount,
    /// `berserker_timer` (`+0xa10`): the seconds the regeneration holds back toward its next
    /// whole hit point ([`crate::objects::unit::Unit::regenerate`]).
    pub regen_timer: Seconds,
    /// `+0x9e8` (`num_builders`) of a combat object that is no building: the Repair actions
    /// counted among its repairers. A building's is its construction's
    /// ([`crate::construction::build::Construction::builders`]).
    pub builders: BuilderCount,
    /// Its charge (`+0x9e0`).
    pub charge: Charge,
    /// The melee and pierce armor taken off it by charge hits (`+0x9e4`, `+0x9e6`).
    pub armor_removed: ArmorPair,
    /// What its master's passive abilities keep on it ([`crate::combat::abilities`]): the movement damage
    /// timer, the attack timer's rate and its terrain look.
    pub abilities: Abilities,
    /// What a hit reads of its unit and its target besides the masters: `+0xa24`
    /// (`mAttackPowerup`, the attacker's slot 0x6d8), the armor powerups (`+0xa48` melee,
    /// `+0xa4c` pierce, slots 0x708 and 0x710; whole numbers), the armor attacks removed
    /// (`+0x9e4`, `+0x9e6`), the charge (`+0x9e0`) and `mHitSomethingValidForChargeAttack`
    /// (`+0x8a8`: its last hit was its charge's).
    pub attack_powerup: AttackBonus,
    /// `mReloadPowerup` (`+0xa28`), taken off its master's reload time.
    pub reload_powerup: Seconds,
    /// `+0xa30`, `mWorkRatePowerup`: what the unit adds to its master's work rate.
    pub work_rate_powerup: WorkRate,
    /// `mRegenerationRatePowerup` (`+0xa34`), which the regeneration rate adds to its master's
    /// ([`regeneration_rate`]).
    ///
    /// [`regeneration_rate`]: crate::objects::hit_points::regeneration_rate
    pub regen_powerup: Regeneration,
    /// `mRegenerationStinger` (`+0xb3c`), which the regeneration rate adds last
    /// ([`regeneration_rate`]).
    ///
    /// [`regeneration_rate`]: crate::objects::hit_points::regeneration_rate
    pub regen_stinger: Regeneration,
    /// `mReloadStinger` (`+0xb40`), added to its master's reload time.
    pub reload_stinger: Seconds,
    /// `mSpeedStinger` (`+0xb38`), which its top speed adds (slot 0x208, `FUN_141202c70`).
    pub speed_stinger: Speed,
    /// Its hit point stinger (`+0xb44`, unsaved, slot 0x760), which its most hit points add
    /// (slot 0xc8, `FUN_141202bb0`).
    #[diff(unsaved)]
    pub hp_stinger: HitPoints,
    /// Its attack stinger (`+0xb48`, unsaved, slot 0x768), which its attack adds with its
    /// attack powerup.
    #[diff(unsaved)]
    pub attack_stinger: AttackBonus,
    /// Its melee and pierce armor stingers (`+0xb4c`, `+0xb50`, unsaved, slots 0x770 and 0x778).
    #[diff(unsaved)]
    pub armor_stingers: ArmorBonuses,
    /// `mActiveStingers` (`+0xb58`, saved): the stingers its attackers' hits handed it, from
    /// which the stinger update rebuilds the stinger sums above in each update
    /// ([`crate::world::World::object_stingers`]).
    pub active_stingers: Vec<ActiveStinger>,
    /// `mPowerupUpdateOffset` (`+0xa54`): the world time after which the powerup update
    /// (`FUN_141204a10`) resets the powerups (`FUN_1412066b0`) in each update. The combat
    /// setup (`FUN_14121df60`) sets it to the world time plus its "setup" draw mod 100, an
    /// aura's powerup (`FUN_141200dd0`) to that plus 1000.
    pub powerup_update: WorldTime,
    /// `mHitpointsPowerup` and `mHitpointsMaxPowerup` (`+0xa38`, `+0xa3c`), which the task finder
    /// adds to the hit points.
    pub hp_powerup: HpPowerup,
    /// `mMaxRangePowerup` (`+0xa40`, slot 0x700): what its shown range adds
    /// ([`crate::combat::damage::shown_range`]).
    pub range_powerup: Distance,
    /// Its melee and pierce armor powerups (`+0xa48`, `+0xa4c`, 32 bits read as 16).
    pub armor_powerups: ArmorBonuses,
    /// `mSpeedPowerup` (`+0xa2c`): what an aura's speed powerup adds, and a siege unit's share
    /// of the infantry inside it (slot 0x1b0, [`crate::combat::powerup::count_infantry`]; after a
    /// powerup reset [`crate::combat::powerup::speed_of_infantry`]).
    pub speed_powerup: Speed,
    /// Its conversion chance powerup (`+0xa50`, unsaved; [`ConversionPowerup`]).
    #[diff(unsaved)]
    pub conversion_powerup: ConversionPowerup,
    /// The powerups it holds (its map `+0xa98`, unsaved; [`AppliedPowerup`]).
    #[diff(unsaved)]
    pub applied_powerups: Vec<AppliedPowerup>,
    /// A siege unit's infantry inside it (`+0xad8`, unsaved; [`InfantryCount`]).
    #[diff(unsaved)]
    pub infantry_inside: InfantryCount,
    /// A building's own store (`+0xae0`, `FUN_1411abad0`'s map by attribute): what its storage
    /// entries of mode 16 put in at its completion and take out at its death
    /// (`FUN_1411e6580`).
    pub store: Vec<(AttributeId, Amount)>,
    /// `+0xa00` (not saved): a combat object's missiles per volley, its master's volley fire plus
    /// the shares of the units inside it ([`crate::garrison::share`]).
    #[diff(unsaved)]
    pub firepower: Firepower,
    /// `+0xa14` (not saved; 1 from the constructor, `FUN_1411ea960`, and the load): where the
    /// order its work runs on comes from ([`OrderSource`]).
    #[diff(unsaved)]
    pub order_source: OrderSource,
}

impl Combat {
    /// The layer of an object the world's table does not hold, a new object's: every timer,
    /// count, powerup and stinger 0, no timed ability, its kill open, the master's sprites, the
    /// constructor's abilities ([`Abilities::NEW`]), an empty store and a direct order.
    pub const NONE: Self = Self {
        attack_timer: Seconds::ZERO,
        reload_worldtime: WorldTime::ZERO,
        attack_count: AttackCount::ZERO,
        charge_hit: false,
        ability: AbilityTimer::NONE,
        kill_state: KillState::Open,
        next_volley: MissileCount::ZERO,
        special_sprite: SpecialSprite::Hidden,
        salvage: SalvageAmounts::NONE,
        decay_timer: DecayTimer::ZERO,
        raider_countdown: RaiderCountdown::NONE,
        locked_down: LockdownCount::ZERO,
        regen_timer: Seconds::ZERO,
        builders: BuilderCount::NONE,
        charge: Charge::ZERO,
        armor_removed: ArmorPair::ZERO,
        abilities: Abilities::NEW,
        attack_powerup: AttackBonus::ZERO,
        reload_powerup: Seconds::ZERO,
        work_rate_powerup: WorkRate::ZERO,
        regen_powerup: Regeneration::ZERO,
        regen_stinger: Regeneration::ZERO,
        reload_stinger: Seconds::ZERO,
        speed_stinger: Speed::ZERO,
        hp_stinger: HitPoints::ZERO,
        attack_stinger: AttackBonus::ZERO,
        armor_stingers: ArmorBonuses::ZERO,
        active_stingers: Vec::new(),
        powerup_update: WorldTime::ZERO,
        hp_powerup: HpPowerup::NONE,
        range_powerup: Distance::ZERO,
        armor_powerups: ArmorBonuses::ZERO,
        speed_powerup: Speed::ZERO,
        conversion_powerup: ConversionPowerup::ZERO,
        applied_powerups: Vec::new(),
        infantry_inside: InfantryCount::ZERO,
        store: Vec::new(),
        firepower: Firepower::ZERO,
        order_source: OrderSource::Direct,
    };
}
