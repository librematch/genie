//! An object's powerups (`+0xa24`..`+0xa54`, `+0xa98`, `+0xad4`): an aura's powerup
//! ([`World::apply_powerup`], `FUN_141200dd0`) and their reset ([`World::reset_powerups`],
//! `FUN_1412066b0`).

use crate::combat::AttackBonus;
use crate::combat::attack::ATTACK;
use crate::combat::damage::ArmorBonuses;
use crate::combat::damage::Shown;
use crate::combat::damage::shown_attack;
use crate::combat::victims::victim_of;
use crate::map::Distance;
use crate::math::conversions::cvttss2si;
use crate::math::image::powerup::SPEED_SHARE_BASE;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::Regeneration;
use crate::objects::WorkRate;
use crate::objects::layers::Combat;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::DamageClass;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::player_master;
use crate::vision::SightLayer;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::World;
use crate::world::WorldTime;
use crate::world::layer::generated::PowerupTuning;
use crate::world::layer::{ClassRoles, MasterRoles, Roles};
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// The attribute a powerup raises: `FUN_141200dd0`'s second argument, an aura task's
/// `search_wait_time` (`+0x1c`) as a whole number (`CVTTSS2SI`, [`PowerupAttribute::of_task`]).
/// The routine's switch tells the named ones apart; any other raises nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PowerupAttribute(i32);

impl PowerupAttribute {
    /// 0, the most hit points (`+0xa3c`).
    pub const HIT_POINTS: Self = Self(0);
    /// 1, the line of sight (`+0xad4`).
    pub const LINE_OF_SIGHT: Self = Self(1);
    /// 5, the speed (`+0xa2c`).
    pub const MOVEMENT_SPEED: Self = Self(5);
    /// 9, the attack (`+0xa24`).
    pub const ATTACK: Self = Self(9);
    /// 10, the reload time (`+0xa28`).
    pub const ATTACK_RELOAD_TIME: Self = Self(10);
    /// 12, the most range (`+0xa40`).
    pub const MAXIMUM_RANGE: Self = Self(12);
    /// 13, the work rate (`+0xa30`).
    pub const WORK_RATE: Self = Self(13);
    /// 109 (`0x6d`), the regeneration rate (`+0xa34`).
    pub const REGENERATION_RATE: Self = Self(0x6d);
    /// 113 (`0x71`), the conversion chance (`+0xa50`).
    pub const CONVERSION_CHANCE: Self = Self(0x71);
    /// 116 (`0x74`), the melee armor (`+0xa48`).
    pub const MELEE_ARMOR: Self = Self(0x74);
    /// 117 (`0x75`), the pierce armor (`+0xa4c`).
    pub const PIERCE_ARMOR: Self = Self(0x75);
    /// 120 (`0x78`), the regeneration rate as a share of the most hit points (`+0xa34`).
    pub const REGENERATION_HP_PERCENT: Self = Self(0x78);

    /// The save's integer `value` (an active stinger's kind, `+0x8`).
    #[must_use]
    pub const fn of_data(value: i32) -> Self {
        Self(value)
    }

    /// The attribute of an aura or stinger task's `search_wait_time` (`CVTTSS2SI`: a NaN or a
    /// value past 32 bits is `0x80000000`).
    #[must_use]
    pub fn of_task(search_wait_time: Seconds) -> Self {
        Self(cvttss2si(search_wait_time.get()))
    }

    /// The whole number.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for PowerupAttribute {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

/// A powerup an object holds: a key of its map `+0xa98` (a `std::map` ordered by the object's
/// master, the source's master, then the attribute; unsaved, so empty after a load). While it
/// holds one, the same powerup again waits for `+0xa54` ([`World::apply_powerup`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, genie_diff::Diff)]
pub struct AppliedPowerup {
    /// The object's own master (`+0x58`, `+0x24` of the key).
    pub(crate) own: MasterId,
    /// The master of the aura's source (`+0x20`).
    pub(crate) source: MasterId,
    /// The attribute it raises (`+0x1c`).
    pub(crate) attribute: PowerupAttribute,
}

/// What an aura's powerup amount is (`FUN_141200dd0`'s fifth argument, the aura task's flag 1):
/// itself, or a share of the object's master's value of the attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerupShare {
    /// The amount as the aura gives it.
    Plain,
    /// The amount times the master's value (the attack's: of its shown attack less its powerup).
    MasterShare,
}

/// An aura's powerup amount (`FUN_141200dd0`'s fourth argument, a float): what the aura gives
/// ([`Powerup::amount`]), and what that adds to its attribute's powerup ([`powerup_amount`]:
/// itself, or with [`PowerupShare::MasterShare`] its share of the master's value).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct PowerupAmount(f32);

impl PowerupAmount {
    /// The float `value`.
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

/// An aura's powerup for one object (`FUN_141200dd0`'s arguments but the object and the
/// aura's sprite, `+0x9f8`, which only the renderer reads).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Powerup {
    /// The attribute it raises.
    pub attribute: PowerupAttribute,
    /// The master of the aura's source.
    pub source: MasterId,
    /// The amount.
    pub amount: PowerupAmount,
    /// What the amount is.
    pub share: PowerupShare,
}

/// An object's conversion chance powerup (`+0xa50`, unsaved): what slot 0x730 adds to its
/// master's conversion chance modifier (`FUN_141202600`, the conversion's, which genie does not
/// port yet).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct ConversionPowerup(f32);

impl ConversionPowerup {
    /// None.
    pub const ZERO: Self = Self(0.0);

    /// The float `value`.
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

impl genie_diff::Diff for ConversionPowerup {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

/// The infantry inside a siege unit (`+0xad8`, a 32-bit integer, unsaved): slot 0x1b0
/// (`FUN_141209d60`, [`count_infantry`]) adds the enter's 1 and the leave's −1, slot 0x1b8
/// (`FUN_141202270`, [`InfantryCount::get`]) reads it ([`speed_of_infantry`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InfantryCount(i32);

impl InfantryCount {
    /// None.
    pub const ZERO: Self = Self(0);

    /// The count after `change` (slot 0x1b0, a wrapping 32-bit `ADD`).
    #[must_use]
    pub const fn changed(self, change: InfantryChange) -> Self {
        Self(self.0.wrapping_add(change.get()))
    }

    /// The integer.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl genie_diff::Diff for InfantryCount {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        self.0.diff(&other.0, differ);
    }
}

/// A unit of a class that speeds up a siege unit going into or out of it (slot 0x1b0's
/// argument).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfantryChange {
    /// 1: it entered (slot 0x1a0).
    Entered,
    /// −1: it left (slot 0x1a8).
    Left,
}

impl InfantryChange {
    /// The game's argument.
    #[must_use]
    pub const fn get(self) -> i32 {
        match self {
            Self::Entered => 1,
            Self::Left => -1,
        }
    }
}

/// Slot 0x1b0 (`FUN_141209d60`) of a siege unit's `combat`: its count of infantry inside takes
/// `change` (`ADD`), and its speed powerup the change times the tuning's speed per unit
/// (`CVTDQ2PS`, `MULSS` with `powerup::SPEED_PER_INFANTRY`, then `ADDSS` with the powerup).
pub(crate) fn count_infantry(combat: &mut Combat, change: InfantryChange, tuning: PowerupTuning) {
    combat.infantry_inside = combat.infantry_inside.changed(change);
    let speed = change.get() as f32 * tuning.speed_per_infantry;
    combat.speed_powerup = Speed::new(speed + combat.speed_powerup.get());
}

/// Whether a unit of master `unit` counts among the infantry inside a container of master
/// `container` (slots 52 and 53, `FUN_141212e10`, `FUN_141213120`): the container is a siege unit
/// (class 13) other than masters 1704 and 1706, and the unit's class speeds it up (6, 45, 46, 50
/// or 56).
#[must_use]
pub(crate) fn speeds_up_siege(unit: &Master, container: &Master, roles: &Roles) -> bool {
    roles.of_class(container.class).contains(ClassRoles::SIEGE)
        && !roles
            .of_master(container.id)
            .contains(MasterRoles::UNSPED_SIEGE)
        && roles
            .of_class(unit.class)
            .contains(ClassRoles::SPEEDS_SIEGE)
}

/// The milliseconds an aura's powerup holds (`FUN_141200dd0`'s immediate `0x3e8` at
/// `0x14120101f`), besides its draw.
const POWERUP_HOLD: Millis = Millis::new(1000);

/// The draws an aura's powerup takes mod (`FUN_141200dd0`, `0x141201019`: `IMUL ECX,EDX,0x64`).
const POWERUP_DRAW_SPAN: u32 = 100;

/// When an aura's powerup taken at `now` with the draw `draw` ends (`+0xa54`): the draw mod 100
/// plus 1000 plus the world time (the draw is 15 bits, so its signed mod is the unsigned one).
#[must_use]
pub(crate) const fn powerup_until(now: WorldTime, draw: u32) -> WorldTime {
    now.after(Millis::new(draw % POWERUP_DRAW_SPAN))
        .after(POWERUP_HOLD)
}

/// The speed powerup of `count` infantry inside a siege unit (`FUN_1412066b0`): slot 0x1b8
/// sign-extends the count to 64 bits, `CVTSI2SS` takes it as unsigned (the halving path for the
/// top bit, which rounds as a direct conversion), and `MULSS` scales it by the tuning's speed
/// per unit (`powerup::SPEED_PER_INFANTRY`).
#[must_use]
pub(crate) fn speed_of_infantry(count: InfantryCount, tuning: PowerupTuning) -> Speed {
    let wide = i64::from(count.get()).cast_unsigned();
    Speed::new(wide as f32 * tuning.speed_per_infantry)
}

/// `FUN_1412066b0`'s values of `combat`: the attack, reload, work rate, regeneration, most hit
/// points, range, armor and conversion powerups go (the hit points powerup `+0xa38` stays), the
/// map of the powerups it holds empties, and the speed powerup is its infantry's
/// ([`speed_of_infantry`]). The aura's sprite `+0x9f8` is the renderer's. The sight powerup
/// goes in [`World::reset_powerups`].
pub(crate) fn reset_powerup_values(combat: &mut Combat, tuning: PowerupTuning) {
    combat.attack_powerup = AttackBonus::ZERO;
    combat.reload_powerup = Seconds::ZERO;
    combat.work_rate_powerup = WorkRate::ZERO;
    combat.regen_powerup = Regeneration::ZERO;
    combat.hp_powerup.max = HitPoints::ZERO;
    combat.range_powerup = Distance::ZERO;
    combat.armor_powerups = ArmorBonuses::ZERO;
    combat.conversion_powerup = ConversionPowerup::ZERO;
    combat.applied_powerups.clear();
    combat.speed_powerup = speed_of_infantry(combat.infantry_inside, tuning);
}

/// `FUN_141202660`: a master's armor of `class`, its first armor entry of the class (`+0x808`,
/// count `+0x802`), else its base armor (`+0x800`).
#[must_use]
pub(crate) fn armor_of_class(master: &Master, class: DamageClass) -> DamageAmount {
    let combat = master.combat_part();
    combat
        .armors
        .iter()
        .find(|entry| entry.class == class)
        .map_or(combat.base_armor, |entry| entry.amount)
}

/// The amount a powerup of `share` adds of `attribute` to an object of `master` (`FUN_141200dd0`'s
/// cases but the line of sight's and the attack's): `amount`, or with
/// [`PowerupShare::MasterShare`] its share of the master's value (the speed's share is the
/// amount less 1, `powerup::SPEED_SHARE_BASE`, unless the amount is below 0: `COMISS`/`JBE`, a
/// NaN amount takes the less-1 path). None for an attribute it does not raise, and
/// for the regeneration's share of the hit points when a share.
#[must_use]
pub(crate) fn powerup_amount(master: &Master, powerup: Powerup) -> Option<PowerupAmount> {
    let Powerup {
        attribute,
        amount,
        share,
        ..
    } = powerup;
    let shared = share == PowerupShare::MasterShare;
    let amount = amount.get();
    let scaled = |value: f32| if shared { amount * value } else { amount };
    let hit_points = f32::from(master.hit_points.get());
    Some(PowerupAmount::new(match attribute {
        PowerupAttribute::HIT_POINTS => {
            if shared {
                hit_points * amount
            } else {
                amount
            }
        }
        PowerupAttribute::MOVEMENT_SPEED => {
            let speed = master.animated_part().max_speed.get();
            if !shared {
                amount
            } else if 0.0 > amount {
                speed * amount
            } else {
                (amount - SPEED_SHARE_BASE) * speed
            }
        }
        PowerupAttribute::ATTACK_RELOAD_TIME => scaled(master.combat_part().reload.get()),
        PowerupAttribute::MAXIMUM_RANGE => scaled(master.combat_part().weapon_range.get()),
        PowerupAttribute::WORK_RATE => scaled(master.action_part().work_rate.get()),
        PowerupAttribute::REGENERATION_RATE => scaled(master.creatable_part().regeneration.get()),
        PowerupAttribute::CONVERSION_CHANCE => scaled(master.creatable_part().conversion.chance),
        PowerupAttribute::MELEE_ARMOR | PowerupAttribute::PIERCE_ARMOR => {
            let class = if attribute == PowerupAttribute::MELEE_ARMOR {
                DamageClass::MELEE
            } else {
                DamageClass::PIERCE
            };
            let armor = f32::from(armor_of_class(master, class).get());
            if shared { armor * amount } else { amount }
        }
        PowerupAttribute::REGENERATION_HP_PERCENT if !shared => hit_points * amount,
        _ => return None,
    }))
}

/// An object a powerup reaches that the world does not hold: the aura found it, or it updates.
const UNHELD: GenieError = GenieError::unreachable("a powered-up object the world does not hold");

/// A powerup of an attribute [`powerup_amount`] gives no amount for, which it returns before.
const UNNAMED_ATTRIBUTE: GenieError =
    GenieError::unreachable("a powerup's amount of an attribute it does not raise");

impl World {
    /// `FUN_1412066b0`, the powerup reset of the combat object `id`: its values
    /// ([`reset_powerup_values`]), then a sight powerup above `powerup::EPSILON` comes off
    /// (slots 0x1d8, 0x1d0 around it). The combat setup, a master change and the powerup update
    /// run it.
    ///
    /// # Errors
    ///
    /// An object the world does not hold.
    pub(crate) fn reset_powerups(&mut self, id: ObjectId) -> GenieResult<()> {
        let (_, unit) = self.object(id).ok_or(UNHELD)?;
        let inside = unit.life.inside.is_some();
        reset_powerup_values(self.parts.combat_mut(id)?, self.model.tuning.powerup);
        self.spatial.take_sight_powerup(
            id,
            inside,
            SightLayer {
                roles: &self.model.roles,
                visibility_tuning: self.model.tuning.visibility,
            },
        )
    }

    /// `FUN_141200dd0`, an aura's powerup `powerup` on the combat object `id`: a powerup its map
    /// holds (its own master's, the source's, the attribute's) waits while the world time is
    /// below `+0xa54`, else every powerup goes first ([`World::reset_powerups`]). The map takes
    /// it, the Object stream's draw `ApplyPowerupToUnit` (line 0x1e0e) after the world time's
    /// read sets `+0xa54` ([`powerup_until`]), and the attribute's powerup adds its amount
    /// ([`powerup_amount`]; the attack's share sets it: the shown attack less the powerup as a
    /// whole number, times the amount; the line of sight's goes through its sight, slots 0x1d8
    /// and 0x1d0). The last attribute (`+0xac0`) is the spectator's getter's (slot 228) and the
    /// sprite (`+0x9f8`) the renderer's.
    ///
    /// # Errors
    ///
    /// An object the world does not hold, or a master its player has not got.
    pub(crate) fn apply_powerup(&mut self, id: ObjectId, powerup: Powerup) -> GenieResult<()> {
        let (player, unit) = self.object(id).ok_or(UNHELD)?;
        let own = unit.master(self.master_tables(), player)?.id;
        let key = AppliedPowerup {
            own,
            source: powerup.source,
            attribute: powerup.attribute,
        };
        let combat = self.parts.combat_of(id);
        if combat.applied_powerups.contains(&key) {
            if self.world_time < combat.powerup_update {
                return Ok(());
            }
            self.reset_powerups(id)?;
        }
        let draw = self.streams.get_mut(Stream::Object).rand15();
        let until = powerup_until(self.world_time, draw);
        let combat = self.parts.combat_mut(id)?;
        if !combat.applied_powerups.contains(&key) {
            combat.applied_powerups.push(key);
        }
        combat.powerup_update = until;
        match powerup.attribute {
            PowerupAttribute::LINE_OF_SIGHT => self.raise_sight(id, powerup),
            PowerupAttribute::ATTACK => self.raise_attack(id, powerup),
            _ => self.raise_powerup(id, powerup),
        }
    }

    /// [`World::apply_powerup`]'s attributes but the line of sight and the attack: the amount
    /// ([`powerup_amount`]) plus the powerup (`ADDSS`, the amount first; the armors add whole
    /// numbers, `CVTTSS2SI`).
    fn raise_powerup(&mut self, id: ObjectId, powerup: Powerup) -> GenieResult<()> {
        let (player, unit) = self.object(id).ok_or(UNHELD)?;
        let master = unit.master(self.master_tables(), player)?;
        let Some(amount) = powerup_amount(master, powerup) else {
            return Ok(());
        };
        let amount = amount.get();
        let combat = self.parts.combat_mut(id)?;
        match powerup.attribute {
            PowerupAttribute::HIT_POINTS => {
                combat.hp_powerup.max = HitPoints::new(amount + combat.hp_powerup.max.get());
            }
            PowerupAttribute::MOVEMENT_SPEED => {
                combat.speed_powerup = Speed::new(amount + combat.speed_powerup.get());
            }
            PowerupAttribute::ATTACK_RELOAD_TIME => {
                combat.reload_powerup = Seconds::new(amount + combat.reload_powerup.get());
            }
            PowerupAttribute::MAXIMUM_RANGE => {
                combat.range_powerup = Distance::new(amount + combat.range_powerup.get());
            }
            PowerupAttribute::WORK_RATE => {
                combat.work_rate_powerup = WorkRate::new(amount + combat.work_rate_powerup.get());
            }
            PowerupAttribute::REGENERATION_RATE | PowerupAttribute::REGENERATION_HP_PERCENT => {
                combat.regen_powerup = Regeneration::new(amount + combat.regen_powerup.get());
            }
            PowerupAttribute::CONVERSION_CHANCE => {
                combat.conversion_powerup =
                    ConversionPowerup::new(amount + combat.conversion_powerup.get());
            }
            PowerupAttribute::MELEE_ARMOR => {
                combat.armor_powerups.melee =
                    combat.armor_powerups.melee.wrapping_add(cvttss2si(amount));
            }
            PowerupAttribute::PIERCE_ARMOR => {
                combat.armor_powerups.pierce =
                    combat.armor_powerups.pierce.wrapping_add(cvttss2si(amount));
            }
            // `powerup_amount` names every other attribute none
            _ => return Err(UNNAMED_ATTRIBUTE.with("attribute", powerup.attribute.get())),
        }
        Ok(())
    }

    /// [`World::apply_powerup`]'s line of sight: the sight goes (slot 0x1d8), the powerup takes
    /// the amount (with a share, times the master's line of sight `+0x5d0`; `ADDSS` with the
    /// powerup), and the sight comes again (slot 0x1d0).
    fn raise_sight(&mut self, id: ObjectId, powerup: Powerup) -> GenieResult<()> {
        let (player, unit) = self.object(id).ok_or(UNHELD)?;
        let inside = unit.life.inside.is_some();
        let master = unit.master(self.master_tables(), player)?;
        let amount = Distance::new(if powerup.share == PowerupShare::MasterShare {
            powerup.amount.get() * master.line_of_sight.get()
        } else {
            powerup.amount.get()
        });
        let layer = SightLayer {
            roles: &self.model.roles,
            visibility_tuning: self.model.tuning.visibility,
        };
        self.spatial.raise_sight_powerup(id, inside, amount, layer)
    }

    /// [`World::apply_powerup`]'s attack: the amount plus the powerup after the floor (`floorf`),
    /// or with a share the shown attack (slot 0x878, [`World::shown_attack_of`]) less the
    /// powerup as a whole number (slot 0x6d8, `CVTTSS2SI` to 16 bits), times the amount: that
    /// share replaces the powerup.
    fn raise_attack(&mut self, id: ObjectId, powerup: Powerup) -> GenieResult<()> {
        let attack = if powerup.share == PowerupShare::MasterShare {
            let shown = self.shown_attack_of(id)?;
            let powerup_whole = cvttss2si(self.parts.combat_of(id).attack_powerup.get()) as i16;
            let base = i32::from(shown.get()) - i32::from(powerup_whole);
            (base as f32) * powerup.amount.get()
        } else {
            libm::floorf(powerup.amount.get()) + self.parts.combat_of(id).attack_powerup.get()
        };
        self.parts.combat_mut(id)?.attack_powerup = AttackBonus::new(attack);
        Ok(())
    }

    /// Slot 0x878 of the combat object `id` (`FUN_141213990`, [`shown_attack`]): the attack it
    /// shows, with its top action's target when that attacks.
    ///
    /// # Errors
    ///
    /// An object the world does not hold, or a master its player has not got.
    pub(crate) fn shown_attack_of(&self, id: ObjectId) -> GenieResult<DamageAmount> {
        let (player, unit) = self.object(id).ok_or(UNHELD)?;
        let master = unit.master(self.master_tables(), player)?;
        let roles = &self.model.roles;
        let terrain = self.spatial.terrain.as_ref();
        let owner = self.players.player(player)?;
        let victim = victim_of(
            (
                unit,
                self.parts.unit_ai.contains(unit.id),
                self.parts.building_of(unit.id),
                self.parts.combat_of(unit.id),
                self.parts.actions_of(unit.id),
                self.parts.unread.get(unit.id),
            ),
            master,
            &owner.attributes,
            player,
            terrain,
            self.mode_680,
            roles,
        );
        let actions = self.parts.actions_of(unit.id);
        let attacked = (actions.top_action_kind() == Some(ATTACK))
            .then(|| actions.top_target())
            .flatten();
        let target = match attacked.and_then(|target_id| self.object(target_id)) {
            Some((target_player, target)) => {
                let target_master = target.master(self.master_tables(), target_player)?;
                Some(victim_of(
                    (
                        target,
                        self.parts.unit_ai.contains(target.id),
                        self.parts.building_of(target.id),
                        self.parts.combat_of(target.id),
                        self.parts.actions_of(target.id),
                        self.parts.unread.get(target.id),
                    ),
                    target_master,
                    &self.players.player(target_player)?.attributes,
                    target_player,
                    terrain,
                    self.mode_680,
                    roles,
                ))
            }
            None => None,
        };
        let shown = Shown {
            target: target.as_ref(),
            projectile: master
                .creatable_part()
                .charge_projectile
                .and_then(|projectile| player_master(&self.model.masters, player, projectile))
                .map(|projectile| &projectile.combat_part().weapons[..]),
            charge_event: master.creatable_part().charge_event,
            range_powerup: self.parts.combat_of(unit.id).range_powerup,
        };
        Ok(shown_attack(&victim, &shown, roles))
    }
}

#[cfg(test)]
mod tests;
